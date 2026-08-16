use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde_json::{Map, Value};

use crate::catalog::{get_package_manager, native_import_strategy};
use crate::model::{
    CapabilityAnalysis, CapabilityClassification, CapabilityDecision, CapabilitySummary,
    Diagnostic, DiagnosticSeverity, EvidenceDetail, LockGraph, MigrationPlan, MutationAction,
    NativeImportMode, PackageManagerId, PlannedFileMutation, PlannedOperation, ProjectInspection,
    ProjectIr, SCHEMA_VERSION, SideEffect, SupportTier,
};
use crate::util::{PkgshiftError, Result, digest_text, read_json_object, read_text, short_digest};

#[derive(Debug, Clone)]
struct Outcome {
    classification: CapabilityClassification,
    risk: &'static str,
    transformation: Option<&'static str>,
    summary: String,
}

fn outcome(
    classification: CapabilityClassification,
    risk: &'static str,
    transformation: Option<&'static str>,
    summary: impl Into<String>,
) -> Outcome {
    Outcome {
        classification,
        risk,
        transformation,
        summary: summary.into(),
    }
}

fn native(summary: impl Into<String>) -> Outcome {
    outcome(CapabilityClassification::Native, "none", None, summary)
}

fn transform(id: &'static str, summary: impl Into<String>) -> Outcome {
    outcome(
        CapabilityClassification::Transform,
        "medium",
        Some(id),
        summary,
    )
}

fn lossy(id: &'static str, summary: impl Into<String>) -> Outcome {
    outcome(CapabilityClassification::Lossy, "high", Some(id), summary)
}

fn unsupported(summary: impl Into<String>) -> Outcome {
    outcome(CapabilityClassification::Unsupported, "high", None, summary)
}

fn unknown(summary: impl Into<String>) -> Outcome {
    outcome(CapabilityClassification::Unknown, "high", None, summary)
}

fn not_applicable(summary: impl Into<String>) -> Outcome {
    outcome(
        CapabilityClassification::NotApplicable,
        "none",
        None,
        summary,
    )
}

#[allow(clippy::too_many_lines)]
fn rule(feature: &str, target: PackageManagerId) -> Outcome {
    use PackageManagerId::{Bun, Deno, Npm, Pnpm, Vlt, YarnClassic, YarnModern};
    match feature {
        "workspace.manifest" => match target {
            Npm | Pnpm | YarnClassic | YarnModern | Bun => {
                native("The target natively represents workspace membership.")
            }
            Deno => transform(
                "workspace.to-deno-workspace",
                "Deno requires workspace membership in Deno configuration.",
            ),
            Vlt => unknown("The preview adapter has not verified workspace semantics."),
        },
        "workspace.negative-patterns" => match target {
            Pnpm | Bun => native("The target supports workspace exclusion patterns."),
            _ => unknown("Equivalent workspace exclusion behavior is not verified."),
        },
        "dependency.workspace-protocol" => match target {
            Pnpm | YarnModern | Bun => native("The target supports workspace specifiers."),
            Npm | YarnClassic => transform(
                "workspace.expand-to-semver",
                "Workspace specifiers must be expanded to semver ranges.",
            ),
            Vlt | Deno => unknown("Workspace protocol parity is not verified."),
        },
        "dependency.catalog-protocol" | "policy.catalogs" => match target {
            Pnpm | Bun => native("The target natively represents dependency catalogs."),
            Npm | YarnClassic | YarnModern => lossy(
                "catalog.expand-to-range",
                "Catalog references must be expanded and centralized policy is lost.",
            ),
            Deno => lossy(
                "catalog.expand-to-range",
                "Catalog references must be expanded for Deno dependency declarations.",
            ),
            Vlt => unknown("Catalog behavior is not verified for the preview adapter."),
        },
        "dependency.patch-protocol" => match target {
            YarnModern => native("The target supports patch protocol dependencies."),
            Pnpm => transform(
                "patch.yarn-to-pnpm",
                "Yarn patch entries require deterministic pnpm policy rendering.",
            ),
            Bun => transform(
                "patch.yarn-to-bun",
                "Yarn patch entries require deterministic Bun policy rendering.",
            ),
            Vlt => unknown("Patch protocol behavior is not verified."),
            _ => unsupported("The target has no supported patch protocol equivalent."),
        },
        "dependency.portal-protocol" => match target {
            YarnModern => native("The target supports portal dependencies."),
            Npm => lossy(
                "portal.to-file",
                "Portal dependencies become file references.",
            ),
            Pnpm | YarnClassic => lossy(
                "portal.to-link",
                "Portal dependencies become link references.",
            ),
            Bun | Vlt => unknown("Portal semantics are not verified for the target."),
            Deno => unsupported("Deno dependency mode has no portal equivalent."),
        },
        "dependency.link-protocol" => match target {
            Pnpm | YarnClassic | YarnModern => native("The target supports link references."),
            Npm => lossy("link.to-file", "Link references become file references."),
            Bun | Vlt => unknown("Link protocol parity is not verified for the target."),
            Deno => unsupported("Deno dependency mode has no link protocol equivalent."),
        },
        "resolution.overrides" => match target {
            Npm | Bun => native("The target natively supports dependency overrides."),
            Pnpm => transform(
                "overrides.to-pnpm",
                "Overrides must move into pnpm workspace settings.",
            ),
            YarnClassic | YarnModern => lossy(
                "overrides.to-resolutions",
                "Overrides become Yarn resolutions with selector review.",
            ),
            Vlt => unknown("Override selector parity is not verified."),
            Deno => unsupported("Deno dependency mode has no override mapping."),
        },
        "resolution.nested-overrides" => match target {
            Npm => native("npm supports nested override objects."),
            Pnpm => transform(
                "overrides.nested-to-selector",
                "Nested overrides require pnpm selectors.",
            ),
            YarnClassic | YarnModern => lossy(
                "overrides.nested-to-resolutions",
                "Nested overrides lose selector fidelity as Yarn resolutions.",
            ),
            Bun | Deno => unsupported("The target has no safe nested override mapping."),
            Vlt => unknown("Nested override parity is not verified."),
        },
        "resolution.resolutions" => match target {
            YarnClassic | YarnModern | Bun => {
                native("The target natively supports resolution policy.")
            }
            Npm => transform(
                "resolutions.to-overrides",
                "Resolutions require npm override rendering.",
            ),
            Pnpm => transform(
                "resolutions.to-pnpm-overrides",
                "Resolutions require pnpm override rendering.",
            ),
            Vlt => unknown("Resolution selector parity is not verified."),
            Deno => unsupported("Deno dependency mode has no resolution policy mapping."),
        },
        "resolution.package-extensions" => match target {
            Npm | Pnpm | YarnModern => native("The target supports package extensions."),
            Bun | Vlt => unknown("Package extension parity is not verified."),
            YarnClassic | Deno => unsupported("The target has no package extensions mechanism."),
        },
        "patch.patched-dependencies" => match target {
            Pnpm | Bun => native("The target supports patched dependency policy."),
            YarnModern => transform(
                "patch.patched-to-yarn",
                "Patched dependencies require Yarn patch protocol rendering.",
            ),
            Vlt => unknown("Patched dependency behavior is not verified."),
            _ => unsupported("The target has no supported patched dependency mechanism."),
        },
        "install.pnp-linker" => match target {
            Pnpm | YarnModern => native("The target supports Plug and Play linking."),
            Npm | YarnClassic => lossy(
                "linker.pnp-to-node-modules",
                "The migration switches from Plug and Play to node_modules.",
            ),
            Bun => lossy(
                "linker.pnp-to-isolated",
                "The migration switches from Plug and Play to isolated linking.",
            ),
            Vlt => unknown("Plug and Play behavior is not verified."),
            Deno => not_applicable("Deno dependency mode has no Node linker."),
        },
        "install.isolated-linker" => match target {
            Pnpm | Bun => native("The target supports isolated linking."),
            YarnModern => transform(
                "linker.isolated-to-yarn-pnpm",
                "The migration selects Yarn's pnpm linker.",
            ),
            Npm | YarnClassic => lossy(
                "linker.isolated-to-hoisted",
                "The migration switches to a hoisted node_modules layout.",
            ),
            Vlt => unknown("Isolated linker behavior is not verified."),
            Deno => not_applicable("Deno dependency mode has no Node linker."),
        },
        "policy.yarn-constraints" => match target {
            YarnModern => native("The target executes Yarn constraints."),
            Vlt => unknown("Constraint policy behavior is not verified."),
            _ => unsupported("Arbitrary Yarn constraint logic cannot be translated safely."),
        },
        "hook.pnpmfile" => match target {
            Pnpm => native("The target executes pnpm hook modules."),
            Vlt => unknown("Hook extensibility is not verified."),
            _ => unsupported("Arbitrary pnpm hook code cannot be translated safely."),
        },
        "registry.npmrc" => match target {
            Npm | Pnpm | YarnClassic | Bun => {
                native("The target consumes npm-compatible registry configuration.")
            }
            YarnModern => transform(
                "registry.npmrc-to-yarnrc",
                "Registry scopes require Yarn Modern configuration rendering.",
            ),
            Vlt | Deno => unknown("Registry credential mapping is not verified."),
        },
        "lifecycle.trusted-dependencies" => match target {
            Bun => native("Bun natively represents trusted dependencies."),
            Pnpm => transform(
                "lifecycle.to-pnpm-build-policy",
                "Trusted dependencies require pnpm build policy rendering.",
            ),
            YarnModern => transform(
                "lifecycle.to-yarn-build-policy",
                "Trusted dependencies require Yarn build policy rendering.",
            ),
            Npm | YarnClassic => lossy(
                "lifecycle.to-global-script-policy",
                "Per-dependency lifecycle policy becomes a global script policy.",
            ),
            Vlt => unknown("Lifecycle allow-list behavior is not verified."),
            Deno => unsupported("Deno dependency mode has no lifecycle allow-list."),
        },
        _ => unknown(format!(
            "No capability rule is registered for {feature} on {target}."
        )),
    }
}

pub fn analyze_capabilities(
    project_ir: &ProjectIr,
    target: PackageManagerId,
) -> Result<Option<CapabilityAnalysis>> {
    let Some(source) = project_ir.source else {
        return Ok(None);
    };
    let mut decisions = project_ir
        .features
        .iter()
        .map(|feature| {
            let outcome = if source == target {
                native(format!(
                    "{} remains on its source package manager.",
                    feature.id
                ))
            } else {
                rule(&feature.id, target)
            };
            CapabilityDecision {
                feature_id: feature.id.clone(),
                target,
                classification: outcome.classification,
                risk: outcome.risk.to_owned(),
                transformation_id: outcome.transformation.map(str::to_owned),
                summary: outcome.summary,
                locations: feature.locations.clone(),
            }
        })
        .collect::<Vec<_>>();
    decisions.sort_by(|left, right| left.feature_id.cmp(&right.feature_id));
    let mut summary = CapabilitySummary::default();
    for decision in &decisions {
        match decision.classification {
            CapabilityClassification::Native => summary.native += 1,
            CapabilityClassification::Transform => summary.transform += 1,
            CapabilityClassification::Lossy => summary.lossy += 1,
            CapabilityClassification::Unsupported => summary.unsupported += 1,
            CapabilityClassification::Unknown => summary.unknown += 1,
            CapabilityClassification::NotApplicable => summary.not_applicable += 1,
        }
    }
    let diagnostics = decisions
        .iter()
        .filter_map(|decision| match decision.classification {
            CapabilityClassification::Lossy => Some(Diagnostic {
                code: "CAPABILITY_LOSSY".to_owned(),
                severity: DiagnosticSeverity::Warning,
                summary: decision.summary.clone(),
                blocking: false,
                evidence: decision
                    .locations
                    .iter()
                    .map(|location| EvidenceDetail {
                        location: location.clone(),
                        detail: decision.feature_id.clone(),
                    })
                    .collect(),
                remediation: vec![
                    "Review and explicitly accept the semantic compromise before apply."
                        .to_owned(),
                ],
            }),
            CapabilityClassification::Unsupported => Some(Diagnostic {
                code: "CAPABILITY_UNSUPPORTED".to_owned(),
                severity: DiagnosticSeverity::Error,
                summary: decision.summary.clone(),
                blocking: true,
                evidence: decision
                    .locations
                    .iter()
                    .map(|location| EvidenceDetail {
                        location: location.clone(),
                        detail: decision.feature_id.clone(),
                    })
                    .collect(),
                remediation: vec![
                    "Remove the source capability, choose another target, or add a verified adapter rule."
                        .to_owned(),
                ],
            }),
            CapabilityClassification::Unknown => Some(Diagnostic {
                code: "CAPABILITY_UNKNOWN".to_owned(),
                severity: DiagnosticSeverity::Error,
                summary: decision.summary.clone(),
                blocking: true,
                evidence: decision
                    .locations
                    .iter()
                    .map(|location| EvidenceDetail {
                        location: location.clone(),
                        detail: decision.feature_id.clone(),
                    })
                    .collect(),
                remediation: vec![
                    "Gather authoritative target evidence or choose a target with known support."
                        .to_owned(),
                ],
            }),
            _ => None,
        })
        .collect::<Vec<_>>();
    let analysis_id = short_digest(
        "cap_",
        &(
            SCHEMA_VERSION,
            &project_ir.project_ir_id,
            source,
            target,
            &decisions,
            &summary,
        ),
    )?;
    Ok(Some(CapabilityAnalysis {
        schema_version: SCHEMA_VERSION.to_owned(),
        analysis_id,
        project_ir_id: project_ir.project_ir_id.clone(),
        source,
        target,
        decisions,
        summary,
        diagnostics,
    }))
}

fn json_content(value: &Map<String, Value>) -> Result<String> {
    let mut content =
        serde_json::to_string_pretty(value).map_err(|source| PkgshiftError::Json {
            path: "<manifest>".into(),
            source,
        })?;
    content.push('\n');
    Ok(content)
}

fn mutation(
    root: &Path,
    path: &str,
    action: MutationAction,
    content: Option<String>,
    reason: impl Into<String>,
    capabilities: Vec<String>,
) -> Result<Option<PlannedFileMutation>> {
    let before = read_text(&root.join(path))?;
    if action == MutationAction::Write && before == content {
        return Ok(None);
    }
    if action == MutationAction::Delete && before.is_none() {
        return Ok(None);
    }
    Ok(Some(PlannedFileMutation {
        path: path.to_owned(),
        action,
        before_digest: before.as_deref().map(digest_text),
        after_digest: content.as_deref().map(digest_text),
        content,
        reason: reason.into(),
        capabilities,
    }))
}

fn package_version_by_name(project_ir: &ProjectIr) -> BTreeMap<String, String> {
    project_ir
        .packages
        .iter()
        .filter_map(|package| Some((package.name.clone()?, package.version.clone()?)))
        .collect()
}

fn transform_specifier(
    name: &str,
    specifier: &str,
    decision: &CapabilityDecision,
    versions: &BTreeMap<String, String>,
) -> Option<String> {
    match decision.transformation_id.as_deref()? {
        "workspace.expand-to-semver" => {
            let suffix = specifier.strip_prefix("workspace:")?;
            let version = versions.get(name);
            match suffix {
                "*" => Some(version.cloned().unwrap_or_else(|| "*".to_owned())),
                "^" => Some(version.map_or_else(|| "*".to_owned(), |value| format!("^{value}"))),
                "~" => Some(version.map_or_else(|| "*".to_owned(), |value| format!("~{value}"))),
                other => Some(other.to_owned()),
            }
        }
        "portal.to-file" | "link.to-file" => specifier
            .split_once(':')
            .map(|(_, value)| format!("file:{value}")),
        "portal.to-link" => specifier
            .strip_prefix("portal:")
            .map(|value| format!("link:{value}")),
        _ => None,
    }
}

fn parse_pnpm_catalogs(content: &str) -> (Map<String, Value>, Map<String, Value>) {
    let mut catalog = Map::new();
    let mut catalogs = Map::new();
    let mut section = "";
    let mut named_catalog = "";
    for line in content.lines() {
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            continue;
        }
        let indent = line.len() - line.trim_start().len();
        let trimmed = line.trim();
        if indent == 0 && trimmed.ends_with(':') {
            section = trimmed.trim_end_matches(':');
            named_catalog = "";
            continue;
        }
        if section == "catalog" && indent >= 2 {
            if let Some((key, value)) = trimmed.split_once(':') {
                catalog.insert(
                    key.trim().to_owned(),
                    Value::String(value.trim().trim_matches(['\'', '"']).to_owned()),
                );
            }
        } else if section == "catalogs" {
            if indent == 2 && trimmed.ends_with(':') {
                named_catalog = trimmed.trim_end_matches(':');
                catalogs.insert(named_catalog.to_owned(), Value::Object(Map::new()));
            } else if indent >= 4
                && let Some((key, value)) = trimmed.split_once(':')
                && let Some(entries) = catalogs
                    .get_mut(named_catalog)
                    .and_then(Value::as_object_mut)
            {
                entries.insert(
                    key.trim().to_owned(),
                    Value::String(value.trim().trim_matches(['\'', '"']).to_owned()),
                );
            }
        }
    }
    (catalog, catalogs)
}

fn render_pnpm_workspace(patterns: &[String], root_manifest: &Map<String, Value>) -> String {
    let mut lines = vec!["packages:".to_owned()];
    for pattern in patterns {
        lines.push(format!("  - '{pattern}'"));
    }
    if let Some(catalog) = root_manifest.get("catalog").and_then(Value::as_object) {
        lines.push("catalog:".to_owned());
        for (name, value) in catalog {
            if let Some(value) = value.as_str() {
                lines.push(format!("  {name}: '{value}'"));
            }
        }
    }
    if let Some(catalogs) = root_manifest.get("catalogs").and_then(Value::as_object) {
        lines.push("catalogs:".to_owned());
        for (catalog_name, value) in catalogs {
            lines.push(format!("  {catalog_name}:"));
            if let Some(entries) = value.as_object() {
                for (name, value) in entries {
                    if let Some(value) = value.as_str() {
                        lines.push(format!("    {name}: '{value}'"));
                    }
                }
            }
        }
    }
    lines.push(String::new());
    lines.join("\n")
}

struct Transformation {
    manifest_mutations: Vec<PlannedFileMutation>,
    configuration_mutations: Vec<PlannedFileMutation>,
    integration_mutations: Vec<PlannedFileMutation>,
    cleanup_mutations: Vec<PlannedFileMutation>,
    diagnostics: Vec<Diagnostic>,
}

#[allow(clippy::too_many_lines)]
fn transform_project(
    inspection: &ProjectInspection,
    project_ir: &ProjectIr,
    analysis: &CapabilityAnalysis,
    target: PackageManagerId,
) -> Result<Transformation> {
    let root = Path::new(&inspection.root);
    let mut diagnostics = Vec::new();
    let supported_transformations = [
        "workspace.expand-to-semver",
        "portal.to-file",
        "portal.to-link",
        "link.to-file",
        "catalog.expand-to-range",
    ];
    for decision in &analysis.decisions {
        if matches!(
            decision.classification,
            CapabilityClassification::Transform | CapabilityClassification::Lossy
        ) && decision
            .transformation_id
            .as_deref()
            .is_some_and(|id| !supported_transformations.contains(&id))
        {
            diagnostics.push(Diagnostic::blocking(
                "TRANSFORMATION_UNIMPLEMENTED",
                format!(
                    "The Rust renderer does not yet implement {}.",
                    decision.transformation_id.as_deref().unwrap_or("unknown")
                ),
                vec![
                    "Keep the TypeScript implementation as the execution boundary for this capability."
                        .to_owned(),
                ],
            ));
        }
    }

    let decisions = analysis
        .decisions
        .iter()
        .map(|decision| (decision.feature_id.as_str(), decision))
        .collect::<BTreeMap<_, _>>();
    let versions = package_version_by_name(project_ir);
    let mut manifest_mutations = Vec::new();
    let mut root_manifest_after = None;
    let pnpm_workspace = read_text(&root.join("pnpm-workspace.yaml"))?;
    let (pnpm_catalog, pnpm_catalogs) = pnpm_workspace
        .as_deref()
        .map(parse_pnpm_catalogs)
        .unwrap_or_default();
    for package in &project_ir.packages {
        let Some(mut manifest) = read_json_object(&root.join(&package.manifest_path))? else {
            continue;
        };
        if package.path == "." {
            manifest.insert(
                "packageManager".to_owned(),
                Value::String(get_package_manager(target).package_manager_pin.to_owned()),
            );
            if !project_ir.workspace_patterns.is_empty() && target != PackageManagerId::Pnpm {
                manifest.insert(
                    "workspaces".to_owned(),
                    Value::Array(
                        project_ir
                            .workspace_patterns
                            .iter()
                            .cloned()
                            .map(Value::String)
                            .collect(),
                    ),
                );
            }
            if target == PackageManagerId::Bun {
                if !pnpm_catalog.is_empty() {
                    manifest.insert("catalog".to_owned(), Value::Object(pnpm_catalog.clone()));
                }
                if !pnpm_catalogs.is_empty() {
                    manifest.insert("catalogs".to_owned(), Value::Object(pnpm_catalogs.clone()));
                }
            }
        }
        for section in [
            "dependencies",
            "devDependencies",
            "optionalDependencies",
            "peerDependencies",
        ] {
            let Some(entries) = manifest.get_mut(section).and_then(Value::as_object_mut) else {
                continue;
            };
            for (name, value) in entries {
                let Some(specifier) = value.as_str() else {
                    continue;
                };
                let feature = if specifier.starts_with("workspace:") {
                    "dependency.workspace-protocol"
                } else if specifier.starts_with("catalog:") {
                    "dependency.catalog-protocol"
                } else if specifier.starts_with("portal:") {
                    "dependency.portal-protocol"
                } else if specifier.starts_with("link:") {
                    "dependency.link-protocol"
                } else {
                    continue;
                };
                let Some(decision) = decisions.get(feature) else {
                    continue;
                };
                if decision.transformation_id.as_deref() == Some("catalog.expand-to-range") {
                    let key = specifier.strip_prefix("catalog:").unwrap_or_default();
                    let catalog_key = if key.is_empty() { name.as_str() } else { key };
                    if let Some(range) = pnpm_catalog.get(catalog_key).and_then(Value::as_str) {
                        *value = Value::String(range.to_owned());
                    } else {
                        diagnostics.push(Diagnostic::blocking(
                            "CATALOG_ENTRY_NOT_FOUND",
                            format!("No catalog entry was found for {name}."),
                            vec![
                                "Define the catalog entry before retrying the migration."
                                    .to_owned(),
                            ],
                        ));
                    }
                } else if let Some(transformed) =
                    transform_specifier(name, specifier, decision, &versions)
                {
                    *value = Value::String(transformed);
                }
            }
        }
        let content = json_content(&manifest)?;
        if package.path == "." {
            root_manifest_after = Some(manifest);
        }
        if let Some(change) = mutation(
            root,
            &package.manifest_path,
            MutationAction::Write,
            Some(content),
            format!("Render {target} manifest semantics."),
            analysis
                .decisions
                .iter()
                .map(|decision| decision.feature_id.clone())
                .collect(),
        )? {
            manifest_mutations.push(change);
        }
    }

    let mut configuration_mutations = Vec::new();
    if target == PackageManagerId::Pnpm
        && !project_ir.workspace_patterns.is_empty()
        && let Some(root_manifest) = root_manifest_after.as_ref()
        && let Some(change) = mutation(
            root,
            "pnpm-workspace.yaml",
            MutationAction::Write,
            Some(render_pnpm_workspace(
                &project_ir.workspace_patterns,
                root_manifest,
            )),
            "Render pnpm workspace and catalog configuration.",
            vec!["workspace.manifest".to_owned()],
        )?
    {
        configuration_mutations.push(change);
    }
    if target == PackageManagerId::Bun
        && project_ir
            .features
            .iter()
            .any(|feature| feature.id == "install.pnp-linker")
        && let Some(change) = mutation(
            root,
            "bunfig.toml",
            MutationAction::Write,
            Some("[install]\nlinker = \"isolated\"\n".to_owned()),
            "Select Bun isolated linking for a reviewed Plug and Play migration.",
            vec!["install.pnp-linker".to_owned()],
        )?
    {
        configuration_mutations.push(change);
    }

    let source = inspection
        .package_manager
        .selected
        .expect("planning requires a selected source");
    let source_command = match source {
        PackageManagerId::YarnClassic | PackageManagerId::YarnModern => "yarn".to_owned(),
        _ => source.to_string(),
    };
    let target_command = match target {
        PackageManagerId::YarnClassic | PackageManagerId::YarnModern => "yarn".to_owned(),
        _ => target.to_string(),
    };
    let mut integration_mutations = Vec::new();
    if source_command != target_command {
        for integration in &inspection.integrations {
            let Some(content) = read_text(&root.join(&integration.path))? else {
                continue;
            };
            let replaced = replace_command_token(&content, &source_command, &target_command);
            if let Some(change) = mutation(
                root,
                &integration.path,
                MutationAction::Write,
                Some(replaced),
                "Translate recognized package manager commands.",
                Vec::new(),
            )? {
                integration_mutations.push(change);
            }
        }
    }

    let target_definition = get_package_manager(target);
    let source_definition = get_package_manager(source);
    let target_artifacts = target_definition
        .lockfiles
        .iter()
        .chain(target_definition.configuration_files.iter())
        .copied()
        .collect::<BTreeSet<_>>();
    let mut cleanup_mutations = Vec::new();
    for path in source_definition
        .lockfiles
        .iter()
        .chain(source_definition.configuration_files.iter())
        .copied()
    {
        if target_artifacts.contains(path) || path == ".npmrc" {
            continue;
        }
        if let Some(change) = mutation(
            root,
            path,
            MutationAction::Delete,
            None,
            format!("Retire source-only {source} artifact."),
            Vec::new(),
        )? {
            cleanup_mutations.push(change);
        }
    }
    Ok(Transformation {
        manifest_mutations,
        configuration_mutations,
        integration_mutations,
        cleanup_mutations,
        diagnostics,
    })
}

fn replace_command_token(content: &str, source: &str, target: &str) -> String {
    let mut output = String::with_capacity(content.len());
    let mut remainder = content;
    while let Some(index) = remainder.find(source) {
        output.push_str(&remainder[..index]);
        let before = output.chars().next_back();
        let after = remainder[index + source.len()..].chars().next();
        let boundary = |character: Option<char>| {
            character.is_none_or(|value| !value.is_ascii_alphanumeric() && value != '_')
        };
        if boundary(before) && boundary(after) {
            output.push_str(target);
        } else {
            output.push_str(source);
        }
        remainder = &remainder[index + source.len()..];
    }
    output.push_str(remainder);
    output
}

fn operation(
    index: usize,
    phase: &str,
    kind: &str,
    description: String,
    mutations: Vec<PlannedFileMutation>,
) -> Option<PlannedOperation> {
    if mutations.is_empty() {
        return None;
    }
    let capabilities = mutations
        .iter()
        .flat_map(|entry| entry.capabilities.iter().cloned())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    Some(PlannedOperation {
        id: format!("op_{index:03}"),
        phase: phase.to_owned(),
        kind: kind.to_owned(),
        description,
        paths: mutations.iter().map(|entry| entry.path.clone()).collect(),
        command: Vec::new(),
        capabilities,
        side_effect: SideEffect::RepositoryWrite,
        reversible: true,
        preconditions: vec!["Current file digests match the accepted plan.".to_owned()],
        postconditions: vec!["Written file digests match the accepted plan.".to_owned()],
        mutations,
    })
}

pub fn plan_package_manager_migration(
    inspection: &ProjectInspection,
    project_ir: &ProjectIr,
    analysis: &CapabilityAnalysis,
    source_lock_graph: Option<&LockGraph>,
    target: PackageManagerId,
    accepted_lossy: bool,
) -> Result<Option<MigrationPlan>> {
    let Some(source) = inspection.package_manager.selected else {
        return Ok(None);
    };
    let target_definition = get_package_manager(target);
    let source_definition = get_package_manager(source);
    let transformation = transform_project(inspection, project_ir, analysis, target)?;
    let mut diagnostics = project_ir.diagnostics.clone();
    diagnostics.extend(analysis.diagnostics.clone());
    diagnostics.extend(transformation.diagnostics);
    if let Some(graph) = source_lock_graph {
        diagnostics.extend(graph.diagnostics.clone());
    }
    let native_import = native_import_strategy(source, target, source_lock_graph.is_some());
    if source != target && source_lock_graph.is_some() && native_import.is_none() {
        diagnostics.push(Diagnostic {
            code: "NATIVE_IMPORT_UNAVAILABLE".to_owned(),
            severity: DiagnosticSeverity::Warning,
            summary: format!(
                "No verified target-native lockfile importer is registered for {source} to {target}."
            ),
            blocking: false,
            evidence: Vec::new(),
            remediation: vec![
                "pkgshift will generate target dependency state and require lock graph verification."
                    .to_owned(),
            ],
        });
    }
    if target_definition.tier == SupportTier::PreviewTarget {
        diagnostics.push(Diagnostic {
            code: "PM_TARGET_PREVIEW".to_owned(),
            severity: DiagnosticSeverity::Warning,
            summary: format!(
                "{} is a preview migration target.",
                target_definition.display_name
            ),
            blocking: false,
            evidence: Vec::new(),
            remediation: vec![
                "Use the plan for assessment; preview targets cannot be applied.".to_owned(),
            ],
        });
    }
    if source == target {
        diagnostics.push(Diagnostic {
            code: "PM_TARGET_ALREADY_SELECTED".to_owned(),
            severity: DiagnosticSeverity::Warning,
            summary: format!("{} is already selected.", target_definition.display_name),
            blocking: false,
            evidence: Vec::new(),
            remediation: vec!["Select another target or verify the current state.".to_owned()],
        });
    }
    if analysis.summary.lossy > 0 && !accepted_lossy {
        diagnostics.push(Diagnostic::blocking(
            "LOSSY_ACCEPTANCE_REQUIRED",
            "Lossy capability decisions require explicit acceptance in the plan.",
            vec!["Review the diagnostics and re-plan with --accept-lossy.".to_owned()],
        ));
    }

    let mut operations = Vec::new();
    if source != target {
        if let Some(value) = operation(
            operations.len() + 1,
            "configure",
            "manifest.render-target",
            format!(
                "Render {}-compatible package manifests.",
                target_definition.display_name
            ),
            transformation.manifest_mutations,
        ) {
            operations.push(value);
        }
        if let Some(value) = operation(
            operations.len() + 1,
            "configure",
            "configuration.render-target",
            format!(
                "Render deterministic {} configuration.",
                target_definition.display_name
            ),
            transformation.configuration_mutations,
        ) {
            operations.push(value);
        }
        if let Some(value) = operation(
            operations.len() + 1,
            "integrate",
            "integration.translate-commands",
            format!(
                "Translate recognized {} commands in repository integrations.",
                source_definition.display_name
            ),
            transformation.integration_mutations,
        ) {
            operations.push(value);
        }
        if let Some(strategy) = native_import
            .as_ref()
            .filter(|strategy| strategy.mode == NativeImportMode::DedicatedCommand)
        {
            operations.push(PlannedOperation {
                id: format!("op_{:03}", operations.len() + 1),
                phase: "install".to_owned(),
                kind: "dependency.import-target".to_owned(),
                description: strategy.summary.clone(),
                paths: target_definition
                    .lockfiles
                    .iter()
                    .map(ToString::to_string)
                    .collect(),
                command: strategy.command.clone(),
                capabilities: analysis
                    .decisions
                    .iter()
                    .map(|decision| decision.feature_id.clone())
                    .collect(),
                side_effect: SideEffect::DependencyState,
                reversible: true,
                preconditions: vec![
                    "Source dependency state and target configuration match the accepted plan."
                        .to_owned(),
                ],
                postconditions: vec!["The target-native importer exits successfully.".to_owned()],
                mutations: Vec::new(),
            });
        }
        let install_integrates_import = native_import
            .as_ref()
            .is_some_and(|strategy| strategy.mode == NativeImportMode::InstallIntegrated);
        operations.push(PlannedOperation {
            id: format!("op_{:03}", operations.len() + 1),
            phase: "install".to_owned(),
            kind: if install_integrates_import {
                "dependency.import-and-install-target"
            } else {
                "dependency.install-target"
            }
            .to_owned(),
            description: native_import
                .as_ref()
                .filter(|strategy| strategy.mode == NativeImportMode::InstallIntegrated)
                .map_or_else(
                    || {
                        format!(
                            "Generate {} dependency state without lifecycle scripts.",
                            target_definition.display_name
                        )
                    },
                    |strategy| strategy.summary.clone(),
                ),
            paths: target_definition
                .lockfiles
                .iter()
                .map(ToString::to_string)
                .collect(),
            command: target_definition
                .install_command
                .iter()
                .map(ToString::to_string)
                .collect(),
            capabilities: analysis
                .decisions
                .iter()
                .map(|decision| decision.feature_id.clone())
                .collect(),
            side_effect: SideEffect::DependencyState,
            reversible: true,
            preconditions: vec!["Target configuration matches the plan.".to_owned()],
            postconditions: vec!["The target installer exits successfully.".to_owned()],
            mutations: Vec::new(),
        });
        if let Some(value) = operation(
            operations.len() + 1,
            "cleanup",
            "source.retire",
            format!(
                "Retire source-only {} artifacts.",
                source_definition.display_name
            ),
            transformation.cleanup_mutations,
        ) {
            operations.push(value);
        }
        operations.push(PlannedOperation {
            id: format!("op_{:03}", operations.len() + 1),
            phase: "verify".to_owned(),
            kind: "migration.verify".to_owned(),
            description: "Verify planned digests and target dependency state.".to_owned(),
            paths: Vec::new(),
            command: Vec::new(),
            capabilities: analysis
                .decisions
                .iter()
                .map(|decision| decision.feature_id.clone())
                .collect(),
            side_effect: SideEffect::None,
            reversible: false,
            preconditions: vec!["All apply operations completed.".to_owned()],
            postconditions: vec!["No blocking verification check remains.".to_owned()],
            mutations: Vec::new(),
        });
    }

    let executable = source != target
        && target_definition.tier == SupportTier::ProductionTarget
        && !diagnostics.iter().any(|entry| entry.blocking);
    let plan_id = short_digest(
        "plan_",
        &(
            SCHEMA_VERSION,
            source,
            target,
            target_definition.tier,
            &inspection.fingerprint,
            &project_ir.project_ir_id,
            &analysis.analysis_id,
            &analysis.summary,
            source_lock_graph.map(|graph| &graph.graph_id),
            &native_import,
            accepted_lossy,
            executable,
            &operations,
            &diagnostics,
        ),
    )?;
    Ok(Some(MigrationPlan {
        schema_version: SCHEMA_VERSION.to_owned(),
        plan_id,
        executable,
        accepted_lossy,
        source,
        target,
        target_tier: target_definition.tier,
        repository_fingerprint: inspection.fingerprint.clone(),
        project_ir_id: project_ir.project_ir_id.clone(),
        capability_analysis_id: analysis.analysis_id.clone(),
        capability_summary: analysis.summary.clone(),
        source_lock_graph_id: source_lock_graph.map(|graph| graph.graph_id.clone()),
        native_import,
        operations,
        diagnostics,
        verification: vec![
            "planned file digests match".to_owned(),
            "target package manager is selected".to_owned(),
            "target lockfile exists".to_owned(),
            "source-only artifacts are retired".to_owned(),
            "workspace membership is preserved".to_owned(),
            "target installation operation succeeded".to_owned(),
            if source_lock_graph.is_some() {
                "source and target resolution sets match".to_owned()
            } else {
                "resolved graph comparison is skipped when no source lockfile exists".to_owned()
            },
        ],
    }))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use crate::inspect::{build_project_ir, inspect_project};

    use super::*;

    #[test]
    fn plans_a_pnpm_to_bun_workspace() {
        let directory = tempdir().expect("temporary directory");
        fs::create_dir_all(directory.path().join("packages/app")).expect("workspace directory");
        fs::write(
            directory.path().join("package.json"),
            r#"{"name":"fixture","private":true,"packageManager":"pnpm@11.21.0","workspaces":["packages/*"]}"#,
        )
        .expect("root manifest");
        fs::write(
            directory.path().join("packages/app/package.json"),
            r#"{"name":"@fixture/app","version":"1.0.0"}"#,
        )
        .expect("package manifest");
        fs::write(
            directory.path().join("pnpm-lock.yaml"),
            "lockfileVersion: '9.0'\n",
        )
        .expect("lockfile");
        fs::write(
            directory.path().join("pnpm-workspace.yaml"),
            "packages:\n  - 'packages/*'\n",
        )
        .expect("workspace configuration");
        let inspection = inspect_project(directory.path()).expect("inspection");
        let ir = build_project_ir(&inspection)
            .expect("IR build")
            .expect("project IR");
        let analysis = analyze_capabilities(&ir, PackageManagerId::Bun)
            .expect("analysis")
            .expect("capability analysis");
        let plan = plan_package_manager_migration(
            &inspection,
            &ir,
            &analysis,
            None,
            PackageManagerId::Bun,
            false,
        )
        .expect("planning")
        .expect("migration plan");
        assert!(plan.executable);
        assert!(plan.operations.iter().any(|operation| {
            operation.kind == "dependency.install-target"
                && operation
                    .command
                    .first()
                    .is_some_and(|value| value == "bun")
        }));
        assert!(plan.operations.iter().any(|operation| {
            operation
                .mutations
                .iter()
                .any(|mutation| mutation.path == "pnpm-lock.yaml")
        }));
    }

    #[test]
    fn plans_all_basic_production_directions() {
        let production = [
            PackageManagerId::Npm,
            PackageManagerId::Pnpm,
            PackageManagerId::YarnClassic,
            PackageManagerId::YarnModern,
            PackageManagerId::Bun,
        ];
        for source in production {
            for target in production {
                if source == target {
                    continue;
                }
                let directory = tempdir().expect("temporary directory");
                let definition = get_package_manager(source);
                fs::write(
                    directory.path().join("package.json"),
                    format!(
                        "{{\"name\":\"fixture\",\"private\":true,\"packageManager\":\"{}\"}}",
                        definition.package_manager_pin
                    ),
                )
                .expect("manifest");
                fs::write(directory.path().join(definition.lockfiles[0]), "fixture\n")
                    .expect("source lockfile");
                let inspection = inspect_project(directory.path()).expect("inspection");
                let ir = build_project_ir(&inspection)
                    .expect("IR build")
                    .expect("project IR");
                let analysis = analyze_capabilities(&ir, target)
                    .expect("analysis")
                    .expect("capability analysis");
                let plan = plan_package_manager_migration(
                    &inspection,
                    &ir,
                    &analysis,
                    None,
                    target,
                    false,
                )
                .expect("planning")
                .expect("migration plan");
                assert!(plan.executable, "{source} to {target} should be executable");
            }
        }
    }
}
