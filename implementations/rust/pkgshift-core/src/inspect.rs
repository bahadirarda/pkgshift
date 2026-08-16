use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

use crate::detect::detect_package_manager;
use crate::model::{
    DependencyIr, DependencyProtocol, Diagnostic, IntegrationInspection, IntegrationKind,
    ManifestInspection, ObservedFeature, PackageIr, ProjectInspection, ProjectIr, SCHEMA_VERSION,
    WorkspaceInspection, WorkspaceSource,
};
use crate::util::{
    PkgshiftError, Result, digest_bytes, hex_lower, read_json_object, read_text,
    redact_sensitive_text, resolve_root, short_digest, walk_files,
};

const RELEVANT_BASENAMES: &[&str] = &[
    ".npmrc",
    ".pnpmfile.cjs",
    ".pnp.cjs",
    ".pnp.loader.mjs",
    ".yarnrc",
    ".yarnrc.yml",
    "bunfig.toml",
    "deno.json",
    "deno.jsonc",
    "deno.lock",
    "npm-shrinkwrap.json",
    "package-lock.json",
    "pnpm-lock.yaml",
    "pnpm-workspace.yaml",
    "vlt-lock.json",
    "vlt.json",
    "yarn.config.cjs",
    "yarn.lock",
    "bun.lock",
    "bun.lockb",
];

fn string_property(object: &Map<String, Value>, key: &str) -> Option<String> {
    object.get(key).and_then(Value::as_str).map(str::to_owned)
}

fn workspace_patterns(manifest: Option<&Map<String, Value>>) -> Vec<String> {
    let Some(workspaces) = manifest.and_then(|value| value.get("workspaces")) else {
        return Vec::new();
    };
    let list = workspaces
        .as_array()
        .or_else(|| workspaces.as_object()?.get("packages")?.as_array());
    list.into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect()
}

fn parse_yaml_list(content: &str, section: &str) -> Vec<String> {
    let mut values = Vec::new();
    let mut active = false;
    for line in content.lines() {
        let trimmed = line.trim();
        let indented = line.chars().next().is_some_and(char::is_whitespace);
        if !indented && trimmed.ends_with(':') {
            active = trimmed.trim_end_matches(':') == section;
            continue;
        }
        if active {
            if let Some(value) = trimmed.strip_prefix("- ") {
                values.push(
                    value
                        .trim()
                        .trim_matches(|character| character == '\'' || character == '"')
                        .to_owned(),
                );
            } else if !trimmed.is_empty() && !indented {
                active = false;
            }
        }
    }
    values
}

fn integration_kind(path: &str) -> IntegrationKind {
    let lowercase = path.to_ascii_lowercase();
    if path.starts_with(".github/workflows/")
        || path == ".gitlab-ci.yml"
        || path == "azure-pipelines.yml"
    {
        IntegrationKind::Ci
    } else if lowercase.contains("docker") || path == "Containerfile" {
        IntegrationKind::Container
    } else if lowercase.starts_with("readme") {
        IntegrationKind::Documentation
    } else {
        IntegrationKind::Automation
    }
}

fn is_integration_file(path: &str) -> bool {
    let basename = path.rsplit('/').next().unwrap_or(path);
    (path.starts_with(".github/workflows/") && (path.ends_with(".yml") || path.ends_with(".yaml")))
        || [
            ".gitlab-ci.yml",
            "azure-pipelines.yml",
            "Jenkinsfile",
            "Containerfile",
            "Dockerfile",
            "docker-compose.yml",
            "docker-compose.yaml",
            "README.md",
            "readme.md",
        ]
        .contains(&path)
        || basename.starts_with("Dockerfile.")
}

fn contains_token(content: &str, token: &str) -> bool {
    content.match_indices(token).any(|(index, _)| {
        let before = content[..index].chars().next_back();
        let after = content[index + token.len()..].chars().next();
        let boundary = |character: Option<char>| {
            character.is_none_or(|value| !value.is_ascii_alphanumeric() && value != '_')
        };
        boundary(before) && boundary(after)
    })
}

fn is_relevant(path: &str) -> bool {
    path == "package.json"
        || path.ends_with("/package.json")
        || (path.starts_with(".yarn/patches/") && path.ends_with(".patch"))
        || RELEVANT_BASENAMES.contains(&path)
}

fn repository_fingerprint(root: &Path, paths: &[String]) -> Result<String> {
    let mut hasher = Sha256::new();
    for relative in paths {
        let absolute = root.join(relative);
        let bytes = fs::read(&absolute).map_err(|source| PkgshiftError::Io {
            path: absolute,
            source,
        })?;
        let redacted = match std::str::from_utf8(&bytes) {
            Ok(content) => redact_sensitive_text(relative, content).into_bytes(),
            Err(_) => bytes,
        };
        hasher.update(relative.as_bytes());
        hasher.update([0]);
        hasher.update(digest_bytes(&redacted).as_bytes());
        hasher.update(*b"\n");
    }
    Ok(format!("sha256:{}", hex_lower(&hasher.finalize())))
}

pub fn inspect_project(path: &Path) -> Result<ProjectInspection> {
    let root = resolve_root(path)?;
    let manifest_path = root.join("package.json");
    let mut diagnostics = Vec::new();
    let manifest = match read_json_object(&manifest_path) {
        Ok(value) => value,
        Err(error) => {
            diagnostics.push(Diagnostic::blocking(
                "MANIFEST_INVALID",
                "The root package.json could not be parsed as a JSON object.",
                vec![format!(
                    "Fix package.json before retrying inspection: {error}"
                )],
            ));
            None
        }
    };
    if manifest.is_none() && diagnostics.is_empty() {
        diagnostics.push(Diagnostic::blocking(
            "MANIFEST_NOT_FOUND",
            "No root package.json was found.",
            vec!["Run pkgshift from a JavaScript project root.".to_owned()],
        ));
    }

    let package_manager = detect_package_manager(&root, manifest.as_ref())?;
    diagnostics.extend(package_manager.diagnostics.clone());

    let mut sources = Vec::new();
    let manifest_patterns = workspace_patterns(manifest.as_ref());
    if !manifest_patterns.is_empty() {
        sources.push(WorkspaceSource {
            location: "package.json".to_owned(),
            patterns: manifest_patterns,
        });
    }
    if let Some(content) = read_text(&root.join("pnpm-workspace.yaml"))? {
        let patterns = parse_yaml_list(&content, "packages");
        if !patterns.is_empty() {
            sources.push(WorkspaceSource {
                location: "pnpm-workspace.yaml".to_owned(),
                patterns,
            });
        }
    }
    let workspace = WorkspaceInspection {
        configured: !sources.is_empty(),
        sources,
    };

    let all_files = walk_files(&root)?;
    let relevant_files = all_files
        .iter()
        .filter(|path| is_relevant(path))
        .cloned()
        .collect::<Vec<_>>();
    let fingerprint = repository_fingerprint(&root, &relevant_files)?;

    let mut integrations = Vec::new();
    for relative in all_files.iter().filter(|path| is_integration_file(path)) {
        let Some(content) = read_text(&root.join(relative))? else {
            continue;
        };
        if content.len() > 512_000 {
            continue;
        }
        let lowercase = content.to_ascii_lowercase();
        let package_manager_tokens = ["npm", "pnpm", "yarn", "bun", "vlt", "deno"]
            .iter()
            .filter(|token| contains_token(&lowercase, token))
            .map(|token| (*token).to_owned())
            .collect::<Vec<_>>();
        if !package_manager_tokens.is_empty() {
            integrations.push(IntegrationInspection {
                kind: integration_kind(relative),
                path: relative.clone(),
                package_manager_tokens,
            });
        }
    }

    let manifest_inspection = manifest.as_ref().map(|value| ManifestInspection {
        path: "package.json".to_owned(),
        name: string_property(value, "name"),
        private: value.get("private").and_then(Value::as_bool),
        package_manager: string_property(value, "packageManager"),
    });
    Ok(ProjectInspection {
        root: root.to_string_lossy().into_owned(),
        fingerprint,
        relevant_files,
        manifest: manifest_inspection,
        package_manager,
        workspace,
        integrations,
        diagnostics,
    })
}

fn workspace_match(path: &str, pattern: &str) -> bool {
    let path = path.trim_start_matches("./").trim_matches('/');
    let pattern = pattern.trim_start_matches("./").trim_matches('/');
    if let Some(prefix) = pattern.strip_suffix("/**") {
        return path == prefix || path.starts_with(&format!("{prefix}/"));
    }
    if let Some(prefix) = pattern.strip_suffix("/*") {
        return path
            .strip_prefix(&format!("{prefix}/"))
            .is_some_and(|remainder| !remainder.is_empty() && !remainder.contains('/'));
    }
    if !pattern.contains('*') {
        return path == pattern;
    }
    false
}

fn discover_package_paths(inspection: &ProjectInspection) -> Result<Vec<String>> {
    let root = Path::new(&inspection.root);
    let all_files = walk_files(root)?;
    let positive = inspection
        .workspace
        .sources
        .iter()
        .flat_map(|source| source.patterns.iter())
        .filter(|pattern| !pattern.starts_with('!'))
        .collect::<Vec<_>>();
    let negative = inspection
        .workspace
        .sources
        .iter()
        .flat_map(|source| source.patterns.iter())
        .filter_map(|pattern| pattern.strip_prefix('!'))
        .collect::<Vec<_>>();
    let mut paths = BTreeSet::from([".".to_owned()]);
    for manifest in all_files
        .iter()
        .filter(|path| path.ends_with("/package.json"))
    {
        let directory = manifest.trim_end_matches("/package.json");
        if positive
            .iter()
            .any(|pattern| workspace_match(directory, pattern))
            && !negative
                .iter()
                .any(|pattern| workspace_match(directory, pattern))
        {
            paths.insert(directory.to_owned());
        }
    }
    Ok(paths.into_iter().collect())
}

fn dependency_protocol(specifier: &str) -> DependencyProtocol {
    let lowercase = specifier.to_ascii_lowercase();
    if lowercase.starts_with("workspace:") {
        DependencyProtocol::Workspace
    } else if lowercase.starts_with("catalog:") {
        DependencyProtocol::Catalog
    } else if lowercase.starts_with("npm:") {
        DependencyProtocol::NpmAlias
    } else if lowercase.starts_with("file:") {
        DependencyProtocol::File
    } else if lowercase.starts_with("link:") {
        DependencyProtocol::Link
    } else if lowercase.starts_with("portal:") {
        DependencyProtocol::Portal
    } else if lowercase.starts_with("patch:") {
        DependencyProtocol::Patch
    } else if lowercase.starts_with("git+") || lowercase.ends_with(".git") {
        DependencyProtocol::Git
    } else if lowercase.starts_with("http://") || lowercase.starts_with("https://") {
        DependencyProtocol::Url
    } else if lowercase.starts_with("jsr:") {
        DependencyProtocol::Jsr
    } else if lowercase == "latest"
        || lowercase
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '-')
    {
        DependencyProtocol::Tag
    } else if lowercase
        .starts_with(|character: char| character.is_ascii_digit() || "^~<>=*".contains(character))
    {
        DependencyProtocol::Semver
    } else {
        DependencyProtocol::Unknown
    }
}

fn feature_for_protocol(protocol: &DependencyProtocol) -> Option<&'static str> {
    match protocol {
        DependencyProtocol::Workspace => Some("dependency.workspace-protocol"),
        DependencyProtocol::Catalog => Some("dependency.catalog-protocol"),
        DependencyProtocol::Patch => Some("dependency.patch-protocol"),
        DependencyProtocol::Portal => Some("dependency.portal-protocol"),
        DependencyProtocol::Link => Some("dependency.link-protocol"),
        _ => None,
    }
}

fn add_feature(
    features: &mut BTreeMap<String, Vec<String>>,
    id: &str,
    location: impl Into<String>,
) {
    features
        .entry(id.to_owned())
        .or_default()
        .push(location.into());
}

fn collect_manifest_features(
    manifest: &Map<String, Value>,
    manifest_path: &str,
    features: &mut BTreeMap<String, Vec<String>>,
) {
    let policy_keys = [
        ("overrides", "resolution.overrides"),
        ("resolutions", "resolution.resolutions"),
        ("packageExtensions", "resolution.package-extensions"),
        ("patchedDependencies", "patch.patched-dependencies"),
        ("catalog", "policy.catalogs"),
        ("catalogs", "policy.catalogs"),
        ("trustedDependencies", "lifecycle.trusted-dependencies"),
    ];
    for (key, feature) in policy_keys {
        if manifest.get(key).is_some_and(|value| !value.is_null()) {
            add_feature(features, feature, format!("{manifest_path}#/{key}"));
        }
    }
    if manifest
        .get("overrides")
        .and_then(Value::as_object)
        .is_some_and(|overrides| overrides.values().any(Value::is_object))
    {
        add_feature(
            features,
            "resolution.nested-overrides",
            format!("{manifest_path}#/overrides"),
        );
    }
}

pub fn build_project_ir(inspection: &ProjectInspection) -> Result<Option<ProjectIr>> {
    let Some(source) = inspection.package_manager.selected else {
        return Ok(None);
    };
    if inspection.manifest.is_none() {
        return Ok(None);
    }
    let root = Path::new(&inspection.root);
    let package_paths = discover_package_paths(inspection)?;
    let mut packages = Vec::new();
    let mut features = BTreeMap::<String, Vec<String>>::new();
    if inspection.workspace.configured {
        add_feature(
            &mut features,
            "workspace.manifest",
            "package.json#/workspaces",
        );
    }
    if inspection
        .workspace
        .sources
        .iter()
        .flat_map(|source| source.patterns.iter())
        .any(|pattern| pattern.starts_with('!'))
    {
        add_feature(
            &mut features,
            "workspace.negative-patterns",
            "workspace configuration",
        );
    }

    for package_path in &package_paths {
        let manifest_path = if package_path == "." {
            "package.json".to_owned()
        } else {
            format!("{package_path}/package.json")
        };
        let Some(manifest) = read_json_object(&root.join(&manifest_path))? else {
            continue;
        };
        collect_manifest_features(&manifest, &manifest_path, &mut features);
        let mut dependencies = Vec::new();
        for section in [
            "dependencies",
            "devDependencies",
            "optionalDependencies",
            "peerDependencies",
        ] {
            let Some(entries) = manifest.get(section).and_then(Value::as_object) else {
                continue;
            };
            for (name, value) in entries {
                let Some(specifier) = value.as_str() else {
                    continue;
                };
                let protocol = dependency_protocol(specifier);
                if let Some(feature) = feature_for_protocol(&protocol) {
                    add_feature(
                        &mut features,
                        feature,
                        format!("{manifest_path}#/{section}/{name}"),
                    );
                }
                dependencies.push(DependencyIr {
                    package_path: package_path.clone(),
                    section: section.to_owned(),
                    name: name.clone(),
                    specifier: specifier.to_owned(),
                    protocol,
                    location: format!("{manifest_path}#/{section}/{name}"),
                });
            }
        }
        dependencies.sort_by(|left, right| {
            left.section
                .cmp(&right.section)
                .then_with(|| left.name.cmp(&right.name))
        });
        let mut script_names = manifest
            .get("scripts")
            .and_then(Value::as_object)
            .map(|scripts| scripts.keys().cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        script_names.sort();
        packages.push(PackageIr {
            path: package_path.clone(),
            manifest_path,
            name: string_property(&manifest, "name"),
            version: string_property(&manifest, "version"),
            private: manifest.get("private").and_then(Value::as_bool),
            dependencies,
            script_names,
        });
    }

    if root.join(".npmrc").exists() {
        add_feature(&mut features, "registry.npmrc", ".npmrc");
        if read_text(&root.join(".npmrc"))?
            .is_some_and(|content| content.contains("node-linker=isolated"))
        {
            add_feature(&mut features, "install.isolated-linker", ".npmrc");
        }
    }
    if read_text(&root.join(".yarnrc.yml"))?
        .is_some_and(|content| content.contains("nodeLinker: pnp"))
    {
        add_feature(&mut features, "install.pnp-linker", ".yarnrc.yml");
    }
    if root.join(".pnpmfile.cjs").exists() {
        add_feature(&mut features, "hook.pnpmfile", ".pnpmfile.cjs");
    }
    if root.join("yarn.config.cjs").exists() {
        add_feature(&mut features, "policy.yarn-constraints", "yarn.config.cjs");
    }

    let observed_features = features
        .into_iter()
        .map(|(id, mut locations)| {
            locations.sort();
            locations.dedup();
            ObservedFeature {
                id,
                count: locations.len(),
                locations,
            }
        })
        .collect::<Vec<_>>();
    let workspace_patterns = inspection
        .workspace
        .sources
        .iter()
        .flat_map(|source| source.patterns.iter().cloned())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let project_ir_id = short_digest(
        "ir_",
        &(
            &inspection.fingerprint,
            source,
            &packages,
            &workspace_patterns,
            &observed_features,
        ),
    )?;
    Ok(Some(ProjectIr {
        schema_version: SCHEMA_VERSION.to_owned(),
        project_ir_id,
        repository_fingerprint: inspection.fingerprint.clone(),
        source: Some(source),
        root_package_path: ".".to_owned(),
        packages,
        workspace_patterns,
        features: observed_features,
        integrations: inspection.integrations.clone(),
        diagnostics: inspection.diagnostics.clone(),
    }))
}

pub fn manifest_path(root: &Path, package_path: &str) -> PathBuf {
    if package_path == "." {
        root.join("package.json")
    } else {
        root.join(package_path).join("package.json")
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn builds_workspace_ir_and_redacts_registry_secrets() {
        let directory = tempdir().expect("temporary directory");
        fs::create_dir_all(directory.path().join("packages/app")).expect("workspace directory");
        fs::write(
            directory.path().join("package.json"),
            r#"{"name":"fixture","private":true,"packageManager":"pnpm@11.21.0","workspaces":["packages/*"]}"#,
        )
        .expect("root manifest");
        fs::write(
            directory.path().join("packages/app/package.json"),
            r#"{"name":"@fixture/app","dependencies":{"lib":"workspace:*"}}"#,
        )
        .expect("package manifest");
        fs::write(
            directory.path().join("pnpm-lock.yaml"),
            "lockfileVersion: '9.0'\n",
        )
        .expect("lockfile");
        fs::write(
            directory.path().join(".npmrc"),
            "//registry.npmjs.org/:_authToken=secret-value\n",
        )
        .expect("npm configuration");

        let inspection = inspect_project(directory.path()).expect("inspection");
        let ir = build_project_ir(&inspection)
            .expect("IR build")
            .expect("project IR");
        assert_eq!(ir.packages.len(), 2);
        assert!(
            ir.features
                .iter()
                .any(|feature| { feature.id == "dependency.workspace-protocol" })
        );
        assert!(
            !serde_json::to_string(&inspection)
                .expect("serialized inspection")
                .contains("secret-value")
        );
    }

    #[test]
    fn secret_rotation_does_not_change_fingerprint() {
        let directory = tempdir().expect("temporary directory");
        fs::write(
            directory.path().join("package.json"),
            r#"{"name":"fixture","packageManager":"npm@12.0.2"}"#,
        )
        .expect("manifest");
        fs::write(directory.path().join("package-lock.json"), "{}").expect("lockfile");
        fs::write(directory.path().join(".npmrc"), "token=first\n").expect("npm configuration");
        let first = inspect_project(directory.path()).expect("first inspection");
        fs::write(directory.path().join(".npmrc"), "token=second\n").expect("npm configuration");
        let second = inspect_project(directory.path()).expect("second inspection");
        assert_eq!(first.fingerprint, second.fingerprint);
    }
}
