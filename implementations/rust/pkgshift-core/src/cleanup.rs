use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use crate::catalog::get_package_manager;
use crate::model::{
    DependencyStateCleanupRecord, Diagnostic, DiagnosticSeverity, EvidenceDetail, MigrationPlan,
    PackageManagerId, PlannedOperation, ProjectIr, SideEffect, VerificationCheck,
    VerificationStatus,
};
use crate::util::{PkgshiftError, Result, read_json_object, read_text, safe_join, walk_files};

pub(crate) const OPERATION_KIND: &str = "dependency.clean-source-state";

pub(crate) fn plan_operation(index: usize, project: &ProjectIr) -> PlannedOperation {
    let paths = project
        .packages
        .iter()
        .map(|package| {
            if package.path == "." {
                "node_modules".to_owned()
            } else {
                format!("{}/node_modules", package.path)
            }
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    PlannedOperation {
        id: format!("op_{index:03}"),
        phase: "install".to_owned(),
        kind: OPERATION_KIND.to_owned(),
        description: "Remove pre-migration local dependency state before the target install."
            .to_owned(),
        paths,
        command: Vec::new(),
        timeout_seconds: None,
        capabilities: Vec::new(),
        side_effect: SideEffect::DependencyState,
        reversible: false,
        preconditions: vec![
            "Cleanup paths are package-local node_modules directories from the accepted project IR."
                .to_owned(),
        ],
        postconditions: vec![
            "No pre-migration local dependency state remains before target installation."
                .to_owned(),
        ],
        mutations: Vec::new(),
    }
}

fn path_state(path: &Path) -> Result<Option<fs::Metadata>> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => Ok(Some(metadata)),
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(PkgshiftError::Io {
            path: path.to_path_buf(),
            source,
        }),
    }
}

pub(crate) fn execute(
    root: &Path,
    operation: &PlannedOperation,
) -> Result<DependencyStateCleanupRecord> {
    if operation.kind != OPERATION_KIND {
        return Err(PkgshiftError::InvalidState(format!(
            "unsupported dependency-state cleanup operation: {}",
            operation.kind
        )));
    }
    let mut removed_paths = Vec::new();
    let mut absent_paths = Vec::new();
    for relative in &operation.paths {
        let relative_path = Path::new(relative);
        if relative_path.file_name().and_then(|value| value.to_str()) != Some("node_modules") {
            return Err(PkgshiftError::InvalidState(format!(
                "dependency-state cleanup path is not a node_modules directory: {relative}"
            )));
        }
        let absolute = safe_join(root, relative)?;
        let mut current = root.to_path_buf();
        for component in relative_path.components() {
            current.push(component.as_os_str());
            if let Some(metadata) = path_state(&current)?
                && metadata.file_type().is_symlink()
            {
                return Err(PkgshiftError::InvalidState(format!(
                    "dependency-state cleanup refuses symbolic links: {relative}"
                )));
            }
        }
        let Some(metadata) = path_state(&absolute)? else {
            absent_paths.push(relative.clone());
            continue;
        };
        if !metadata.is_dir() {
            return Err(PkgshiftError::InvalidState(format!(
                "dependency-state cleanup target is not a directory: {relative}"
            )));
        }
        let canonical_root = fs::canonicalize(root).map_err(|source| PkgshiftError::Io {
            path: root.to_path_buf(),
            source,
        })?;
        let canonical_target = fs::canonicalize(&absolute).map_err(|source| PkgshiftError::Io {
            path: absolute.clone(),
            source,
        })?;
        if !canonical_target.starts_with(&canonical_root) {
            return Err(PkgshiftError::InvalidState(format!(
                "dependency-state cleanup path resolves outside the repository: {relative}"
            )));
        }
        fs::remove_dir_all(&absolute).map_err(|source| PkgshiftError::Io {
            path: absolute.clone(),
            source,
        })?;
        if path_state(&absolute)?.is_some() {
            return Err(PkgshiftError::InvalidState(format!(
                "dependency-state cleanup postcondition failed: {relative}"
            )));
        }
        removed_paths.push(relative.clone());
    }
    Ok(DependencyStateCleanupRecord {
        operation_id: operation.id.clone(),
        removed_paths,
        absent_paths,
    })
}

pub(crate) fn clean_install_check(
    plan: &MigrationPlan,
    records: &[DependencyStateCleanupRecord],
) -> VerificationCheck {
    let operations = plan
        .operations
        .iter()
        .filter(|operation| operation.kind == OPERATION_KIND)
        .collect::<Vec<_>>();
    let mut evidence = Vec::new();
    let complete = operations.iter().all(|operation| {
        let Some(record) = records
            .iter()
            .find(|record| record.operation_id == operation.id)
        else {
            evidence.push(format!("missingRecord:{}", operation.id));
            return false;
        };
        let recorded = record
            .removed_paths
            .iter()
            .chain(&record.absent_paths)
            .cloned()
            .collect::<BTreeSet<_>>();
        let expected = operation.paths.iter().cloned().collect::<BTreeSet<_>>();
        evidence.extend(
            record
                .removed_paths
                .iter()
                .map(|path| format!("removed:{path}")),
        );
        evidence.extend(
            record
                .absent_paths
                .iter()
                .map(|path| format!("alreadyAbsent:{path}")),
        );
        recorded == expected
    });
    VerificationCheck {
        id: "clean-target-install".to_owned(),
        status: if operations.is_empty() {
            VerificationStatus::Skipped
        } else if complete {
            VerificationStatus::Passed
        } else {
            VerificationStatus::Failed
        },
        summary: if operations.is_empty() {
            "The stored plan predates explicit local dependency-state cleanup.".to_owned()
        } else if complete {
            "Pre-migration local dependency state was removed before target installation."
                .to_owned()
        } else {
            "Pre-migration local dependency-state cleanup is incomplete.".to_owned()
        },
        evidence,
    }
}

fn source_only_artifacts(plan: &MigrationPlan) -> Vec<&'static str> {
    let target_artifacts = get_package_manager(plan.target)
        .lockfiles
        .iter()
        .chain(get_package_manager(plan.target).configuration_files.iter())
        .copied()
        .collect::<BTreeSet<_>>();
    get_package_manager(plan.source)
        .lockfiles
        .iter()
        .chain(get_package_manager(plan.source).configuration_files.iter())
        .copied()
        .filter(|path| !target_artifacts.contains(path))
        .filter(|path| {
            !(plan.source == PackageManagerId::Deno && matches!(*path, "deno.json" | "deno.jsonc"))
        })
        .collect()
}

pub(crate) fn source_artifact_check(root: &Path, plan: &MigrationPlan) -> VerificationCheck {
    let expected_retired = source_only_artifacts(plan);
    let residues = expected_retired
        .iter()
        .filter(|path| root.join(path).exists())
        .map(|path| (*path).to_owned())
        .collect::<Vec<_>>();
    VerificationCheck {
        id: "source-artifact-residue".to_owned(),
        status: if residues.is_empty() {
            VerificationStatus::Passed
        } else {
            VerificationStatus::Failed
        },
        summary: if residues.is_empty() {
            "No source-only package manager artifact remains in the repository.".to_owned()
        } else {
            format!(
                "{} source-only package manager artifacts remain.",
                residues.len()
            )
        },
        evidence: if residues.is_empty() {
            vec![format!("retired:{}", expected_retired.len())]
        } else {
            residues
        },
    }
}

fn source_code_path(path: &str) -> bool {
    [".js", ".jsx", ".mjs", ".cjs", ".ts", ".tsx", ".mts", ".cts"]
        .iter()
        .any(|extension| path.ends_with(extension))
}

fn contains_command_token(command: &str, token: &str) -> bool {
    command.match_indices(token).any(|(index, _)| {
        let before = command[..index].chars().next_back();
        let after = command[index + token.len()..].chars().next();
        let boundary = |character: Option<char>| {
            character.is_none_or(|value| !value.is_ascii_alphanumeric() && value != '_')
        };
        boundary(before) && boundary(after)
    })
}

pub(crate) fn runtime_reference_diagnostics(
    root: &Path,
    project: &ProjectIr,
    target: PackageManagerId,
) -> Result<Vec<Diagnostic>> {
    if project.source != Some(PackageManagerId::Bun) || target == PackageManagerId::Bun {
        return Ok(Vec::new());
    }
    let mut references = BTreeSet::<(String, String)>::new();
    for package in &project.packages {
        if let Some(manifest) = read_json_object(&root.join(&package.manifest_path))? {
            for section in [
                "dependencies",
                "devDependencies",
                "optionalDependencies",
                "peerDependencies",
            ] {
                for name in ["@types/bun", "bun-types"] {
                    if manifest
                        .get(section)
                        .and_then(serde_json::Value::as_object)
                        .is_some_and(|dependencies| dependencies.contains_key(name))
                    {
                        references.insert((
                            format!("{}#/{section}/{name}", package.manifest_path),
                            format!("runtime dependency:{name}"),
                        ));
                    }
                }
            }
            if let Some(scripts) = manifest
                .get("scripts")
                .and_then(serde_json::Value::as_object)
            {
                for (name, value) in scripts {
                    let Some(command) = value.as_str() else {
                        continue;
                    };
                    if contains_command_token(command, "bun")
                        || contains_command_token(command, "bunx")
                    {
                        references.insert((
                            format!("{}#/scripts/{name}", package.manifest_path),
                            "Bun runtime command".to_owned(),
                        ));
                    }
                }
            }
        }
    }
    for relative in walk_files(root)?
        .into_iter()
        .filter(|path| source_code_path(path))
    {
        let absolute = root.join(&relative);
        let Some(metadata) = fs::metadata(&absolute).ok() else {
            continue;
        };
        if metadata.len() > 512_000 {
            continue;
        }
        let Some(content) = read_text(&absolute)? else {
            continue;
        };
        for (token, detail) in [
            ("Bun.", "Bun global API"),
            ("\"bun:", "bun: module import"),
            ("'bun:", "bun: module import"),
            ("`bun:", "bun: module import"),
        ] {
            if content.contains(token) {
                references.insert((relative.clone(), detail.to_owned()));
            }
        }
    }
    if references.is_empty() {
        return Ok(Vec::new());
    }
    let count = references.len();
    let evidence = references
        .into_iter()
        .take(64)
        .map(|(location, detail)| EvidenceDetail { location, detail })
        .collect();
    Ok(vec![Diagnostic {
        code: "SOURCE_RUNTIME_REFERENCES_PRESERVED".to_owned(),
        severity: DiagnosticSeverity::Warning,
        summary: format!(
            "{count} Bun runtime reference(s) remain outside the package-manager migration boundary."
        ),
        blocking: false,
        evidence,
        remediation: vec![
            "Migrate or intentionally retain the reported Bun runtime semantics; pkgshift never deletes them automatically."
                .to_owned(),
        ],
    }])
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::*;
    use crate::model::{CapabilitySummary, PackageIr, SCHEMA_VERSION};

    fn project(packages: &[&str]) -> ProjectIr {
        ProjectIr {
            schema_version: SCHEMA_VERSION.to_owned(),
            project_ir_id: "ir_cleanup".to_owned(),
            repository_fingerprint: "sha256:fixture".to_owned(),
            source: Some(PackageManagerId::Bun),
            root_package_path: ".".to_owned(),
            packages: packages
                .iter()
                .map(|path| PackageIr {
                    path: (*path).to_owned(),
                    manifest_path: if *path == "." {
                        "package.json".to_owned()
                    } else {
                        format!("{path}/package.json")
                    },
                    name: None,
                    version: None,
                    private: Some(true),
                    dependencies: Vec::new(),
                    script_names: Vec::new(),
                })
                .collect(),
            workspace_patterns: Vec::new(),
            features: Vec::new(),
            integrations: Vec::new(),
            diagnostics: Vec::new(),
        }
    }

    #[test]
    fn plans_and_removes_package_local_dependency_state() {
        let operation = plan_operation(4, &project(&[".", "packages/app"]));
        assert_eq!(
            operation.paths,
            ["node_modules", "packages/app/node_modules"]
        );
        assert!(!operation.reversible);

        let directory = tempdir().expect("temporary directory");
        fs::create_dir_all(directory.path().join("node_modules/.bun"))
            .expect("root dependency state");
        fs::write(directory.path().join("node_modules/.bun/source"), "source")
            .expect("source marker");
        let record = execute(directory.path(), &operation).expect("cleanup");
        assert_eq!(record.removed_paths, ["node_modules"]);
        assert_eq!(record.absent_paths, ["packages/app/node_modules"]);
        assert!(!directory.path().join("node_modules").exists());
        assert_eq!(
            clean_install_check(
                &MigrationPlan {
                    schema_version: SCHEMA_VERSION.to_owned(),
                    plan_id: "plan_cleanup".to_owned(),
                    executable: true,
                    accepted_lossy: false,
                    source: PackageManagerId::Bun,
                    target: PackageManagerId::Deno,
                    target_tier: crate::model::SupportTier::ProductionTarget,
                    repository_fingerprint: "sha256:fixture".to_owned(),
                    project_ir_id: "ir_cleanup".to_owned(),
                    capability_analysis_id: "cap_cleanup".to_owned(),
                    capability_summary: CapabilitySummary::default(),
                    source_lock_graph_id: None,
                    native_import: None,
                    target_executable: None,
                    verification_policy: crate::VerificationPolicy::default(),
                    operations: vec![operation],
                    diagnostics: Vec::new(),
                    verification: Vec::new(),
                },
                &[record]
            )
            .status,
            VerificationStatus::Passed
        );
    }

    #[cfg(unix)]
    #[test]
    fn refuses_symlinked_dependency_state() {
        use std::os::unix::fs::symlink;

        let directory = tempdir().expect("temporary directory");
        let outside = tempdir().expect("outside directory");
        fs::write(outside.path().join("marker"), "preserve").expect("outside dependency marker");
        symlink(outside.path(), directory.path().join("node_modules"))
            .expect("dependency-state symlink");

        let operation = plan_operation(1, &project(&["."]));
        let error = execute(directory.path(), &operation).expect_err("symlink must fail closed");
        assert!(error.to_string().contains("refuses symbolic links"));
        assert_eq!(
            fs::read_to_string(outside.path().join("marker")).expect("outside marker"),
            "preserve"
        );
    }

    #[test]
    fn reports_bun_runtime_references_without_deleting_them() {
        let directory = tempdir().expect("temporary directory");
        fs::write(
            directory.path().join("package.json"),
            r#"{
  "devDependencies": { "@types/bun": "1.3.14" },
  "scripts": { "test": "bun test" }
}
"#,
        )
        .expect("manifest");
        fs::create_dir_all(directory.path().join("src")).expect("source directory");
        fs::write(
            directory.path().join("src/server.ts"),
            "import { test } from \"bun:test\";\nBun.serve({ fetch() {} });\n",
        )
        .expect("source file");

        let diagnostics = runtime_reference_diagnostics(
            directory.path(),
            &project(&["."]),
            PackageManagerId::Deno,
        )
        .expect("runtime reference scan");
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].code, "SOURCE_RUNTIME_REFERENCES_PRESERVED");
        assert!(!diagnostics[0].blocking);
        assert_eq!(diagnostics[0].evidence.len(), 4);
        assert!(directory.path().join("src/server.ts").is_file());
    }
}
