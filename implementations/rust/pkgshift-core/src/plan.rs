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

fn yaml_single_quoted(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn render_pnpm_workspace(
    patterns: &[String],
    root_manifest: &Map<String, Value>,
    overrides: &Map<String, Value>,
    node_linker: Option<&str>,
    trusted_dependencies: &[String],
    lifecycle_policy_present: bool,
) -> String {
    let mut lines = Vec::new();
    if !patterns.is_empty() {
        lines.push("packages:".to_owned());
        for pattern in patterns {
            lines.push(format!("  - {}", yaml_single_quoted(pattern)));
        }
    }
    if let Some(catalog) = root_manifest.get("catalog").and_then(Value::as_object) {
        lines.push("catalog:".to_owned());
        for (name, value) in catalog {
            if let Some(value) = value.as_str() {
                lines.push(format!("  {name}: {}", yaml_single_quoted(value)));
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
                        lines.push(format!("    {name}: {}", yaml_single_quoted(value)));
                    }
                }
            }
        }
    }
    if !overrides.is_empty() {
        lines.push("overrides:".to_owned());
        for (selector, value) in overrides {
            if let Some(value) = value.as_str() {
                lines.push(format!(
                    "  {}: {}",
                    yaml_single_quoted(selector),
                    yaml_single_quoted(value)
                ));
            }
        }
    }
    if let Some(node_linker) = node_linker {
        lines.push(format!("nodeLinker: {node_linker}"));
    }
    if lifecycle_policy_present {
        if trusted_dependencies.is_empty() {
            lines.push("allowBuilds: {}".to_owned());
        } else {
            lines.push("allowBuilds:".to_owned());
            for dependency in trusted_dependencies {
                lines.push(format!("  {}: true", yaml_single_quoted(dependency)));
            }
        }
    }
    lines.push(String::new());
    lines.join("\n")
}

fn string_array(value: Option<&Value>) -> impl Iterator<Item = &str> {
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
}

fn source_trusted_dependencies(
    root_manifest: &Map<String, Value>,
    pnpm_manifest: &Map<String, Value>,
    pnpm_configuration: &Map<String, Value>,
    yarn_configuration: &Map<String, Value>,
) -> Vec<String> {
    let mut trusted = BTreeSet::new();
    for value in [
        root_manifest.get("trustedDependencies"),
        pnpm_manifest.get("onlyBuiltDependencies"),
        pnpm_configuration.get("onlyBuiltDependencies"),
    ] {
        trusted.extend(string_array(value).map(str::to_owned));
    }
    if let Some(allow_builds) = pnpm_configuration
        .get("allowBuilds")
        .and_then(Value::as_object)
    {
        trusted.extend(
            allow_builds
                .iter()
                .filter(|(_, allowed)| allowed.as_bool() == Some(true))
                .map(|(name, _)| name.clone()),
        );
    }
    if yarn_configuration
        .get("enableScripts")
        .and_then(Value::as_bool)
        == Some(false)
        && let Some(dependencies_meta) = root_manifest
            .get("dependenciesMeta")
            .and_then(Value::as_object)
    {
        trusted.extend(
            dependencies_meta
                .iter()
                .filter(|(_, metadata)| {
                    metadata
                        .as_object()
                        .and_then(|entry| entry.get("built"))
                        .and_then(Value::as_bool)
                        == Some(true)
                })
                .map(|(name, _)| name.clone()),
        );
    }
    trusted.into_iter().collect()
}

fn remove_source_lifecycle_policy(
    manifest: &mut Map<String, Value>,
    remove_yarn_build_policy: bool,
) {
    manifest.remove("trustedDependencies");
    if let Some(pnpm) = manifest.get_mut("pnpm").and_then(Value::as_object_mut) {
        pnpm.remove("onlyBuiltDependencies");
        pnpm.remove("allowBuilds");
        if pnpm.is_empty() {
            manifest.remove("pnpm");
        }
    }
    if !remove_yarn_build_policy {
        return;
    }
    let Some(dependencies_meta) = manifest
        .get_mut("dependenciesMeta")
        .and_then(Value::as_object_mut)
    else {
        return;
    };
    dependencies_meta.retain(|_, value| {
        let Some(metadata) = value.as_object_mut() else {
            return true;
        };
        metadata.remove("built");
        !metadata.is_empty()
    });
    if dependencies_meta.is_empty() {
        manifest.remove("dependenciesMeta");
    }
}

fn configure_yarn_lifecycle_policy(
    manifest: &mut Map<String, Value>,
    trusted_dependencies: &[String],
) {
    if trusted_dependencies.is_empty() {
        return;
    }
    let dependencies_meta = manifest
        .entry("dependenciesMeta")
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .expect("dependenciesMeta is initialized as an object");
    for dependency in trusted_dependencies {
        let metadata = dependencies_meta
            .entry(dependency.clone())
            .or_insert_with(|| Value::Object(Map::new()))
            .as_object_mut()
            .expect("dependency metadata is initialized as an object");
        metadata.insert("built".to_owned(), Value::Bool(true));
    }
}

#[derive(Default)]
struct YarnRegistryConfiguration {
    always_auth: Option<bool>,
    registry_server: Option<String>,
    registries: BTreeMap<String, String>,
    scopes: BTreeMap<String, String>,
}

fn environment_reference(value: &str) -> bool {
    let Some(name) = value
        .strip_prefix("${")
        .and_then(|value| value.strip_suffix('}'))
    else {
        return false;
    };
    let mut characters = name.chars();
    characters
        .next()
        .is_some_and(|character| character == '_' || character.is_ascii_alphabetic())
        && characters.all(|character| character == '_' || character.is_ascii_alphanumeric())
}

fn npmrc_for_yarn(content: &str, diagnostics: &mut Vec<Diagnostic>) -> YarnRegistryConfiguration {
    let mut output = YarnRegistryConfiguration::default();
    for raw_line in content.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        let Some((setting, value)) = line.split_once('=') else {
            diagnostics.push(Diagnostic::blocking(
                "NPMRC_SETTING_UNSUPPORTED",
                "Yarn Modern translation found an unsupported .npmrc setting.",
                vec!["Reduce .npmrc to supported registry and authentication settings.".to_owned()],
            ));
            continue;
        };
        let setting = setting.trim();
        let value = value.trim();
        if setting == "node-linker" {
            if matches!(value, "pnp" | "isolated" | "hoisted" | "node-modules") {
                continue;
            }
            diagnostics.push(Diagnostic::blocking(
                "NPMRC_SETTING_UNSUPPORTED",
                "Yarn Modern translation found an unsupported legacy node-linker value.",
                vec!["Use pnp, isolated, hoisted, or node-modules before retrying.".to_owned()],
            ));
            continue;
        }
        if setting == "registry" {
            output.registry_server = Some(value.to_owned());
            continue;
        }
        if let Some(scope) = setting
            .strip_prefix('@')
            .and_then(|value| value.strip_suffix(":registry"))
            .filter(|scope| !scope.is_empty() && !scope.chars().any(char::is_whitespace))
        {
            output.scopes.insert(scope.to_owned(), value.to_owned());
            continue;
        }
        if let Some(registry) = setting
            .strip_suffix(":_authToken")
            .filter(|registry| registry.starts_with("//") && registry.len() > 2)
        {
            if environment_reference(value) {
                output
                    .registries
                    .insert(registry.to_owned(), value.to_owned());
            } else {
                diagnostics.push(Diagnostic::blocking(
                    "REGISTRY_SECRET_REQUIRES_ENVIRONMENT_REFERENCE",
                    "Yarn Modern registry migration requires authentication tokens to use an environment reference.",
                    vec!["Replace the literal token in .npmrc with a ${NAME} reference.".to_owned()],
                ));
            }
            continue;
        }
        if setting == "always-auth" && matches!(value, "true" | "false") {
            output.always_auth = Some(value == "true");
            continue;
        }
        diagnostics.push(Diagnostic::blocking(
            "NPMRC_SETTING_UNSUPPORTED",
            "Yarn Modern translation found an unsupported .npmrc setting.",
            vec!["Reduce .npmrc to supported registry and authentication settings.".to_owned()],
        ));
    }
    output
}

fn render_yarn_configuration(
    node_linker: &str,
    lifecycle_policy_present: bool,
    registry: &YarnRegistryConfiguration,
) -> String {
    let mut lines = vec![format!("nodeLinker: {node_linker}")];
    if lifecycle_policy_present {
        lines.push("enableScripts: false".to_owned());
    }
    if let Some(server) = &registry.registry_server {
        lines.push(format!("npmRegistryServer: {}", yaml_single_quoted(server)));
    }
    if let Some(always_auth) = registry.always_auth {
        lines.push(format!("npmAlwaysAuth: {always_auth}"));
    }
    if !registry.scopes.is_empty() {
        lines.push("npmScopes:".to_owned());
        for (scope, server) in &registry.scopes {
            lines.push(format!("  {}:", yaml_single_quoted(scope)));
            lines.push(format!(
                "    npmRegistryServer: {}",
                yaml_single_quoted(server)
            ));
        }
    }
    if !registry.registries.is_empty() {
        lines.push("npmRegistries:".to_owned());
        for (registry, token) in &registry.registries {
            lines.push(format!("  {}:", yaml_single_quoted(registry)));
            lines.push("    npmAlwaysAuth: true".to_owned());
            lines.push(format!("    npmAuthToken: {}", yaml_single_quoted(token)));
        }
    }
    lines.push(String::new());
    lines.join("\n")
}

fn render_bun_configuration(before: Option<&str>, isolated: bool) -> Option<String> {
    if !isolated {
        return before.map(str::to_owned);
    }
    let before = before.unwrap_or_default();
    let mut lines = before.lines().map(str::to_owned).collect::<Vec<_>>();
    let install_sections = lines
        .iter()
        .enumerate()
        .filter(|(_, line)| line.trim() == "[install]")
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    if install_sections.len() > 1 {
        return None;
    }
    if let Some(start) = install_sections.first().copied() {
        let end = lines
            .iter()
            .enumerate()
            .skip(start + 1)
            .find(|(_, line)| {
                let line = line.trim();
                line.starts_with('[') && line.ends_with(']')
            })
            .map_or(lines.len(), |(index, _)| index);
        let linkers = lines[start + 1..end]
            .iter()
            .enumerate()
            .filter(|(_, line)| {
                line.split_once('=')
                    .is_some_and(|(key, _)| key.trim() == "linker")
            })
            .map(|(index, _)| start + 1 + index)
            .collect::<Vec<_>>();
        if linkers.len() > 1 {
            return None;
        }
        if let Some(index) = linkers.first().copied() {
            lines[index] = "linker = \"isolated\"".to_owned();
        } else {
            lines.insert(start + 1, "linker = \"isolated\"".to_owned());
        }
    } else {
        if lines.last().is_some_and(|line| !line.trim().is_empty()) {
            lines.push(String::new());
        }
        lines.push("[install]".to_owned());
        lines.push("linker = \"isolated\"".to_owned());
    }
    lines.push(String::new());
    Some(lines.join("\n"))
}

fn flatten_nested_overrides(
    overrides: &Map<String, Value>,
    separator: &str,
) -> Option<Map<String, Value>> {
    let mut flattened = Map::new();
    for (parent, value) in overrides {
        if let Some(value) = value.as_str() {
            flattened.insert(parent.clone(), Value::String(value.to_owned()));
            continue;
        }
        let children = value.as_object()?;
        for (child, value) in children {
            let value = value.as_str()?;
            let selector = if child == "." {
                parent.clone()
            } else {
                format!("{parent}{separator}{child}")
            };
            flattened.insert(selector, Value::String(value.to_owned()));
        }
    }
    Some(flattened)
}

fn compatible_resolutions(resolutions: &Map<String, Value>) -> Option<Map<String, Value>> {
    let mut overrides = Map::new();
    for (selector, value) in resolutions {
        let has_selector_syntax = |part: &str| {
            part.chars()
                .any(|character| matches!(character, '/' | '@' | '*'))
        };
        let bare_package = selector.strip_prefix('@').map_or_else(
            || !selector.is_empty() && !has_selector_syntax(selector),
            |scoped| {
                scoped.split_once('/').is_some_and(|(scope, name)| {
                    !scope.is_empty()
                        && !name.is_empty()
                        && !has_selector_syntax(scope)
                        && !has_selector_syntax(name)
                })
            },
        );
        if !bare_package || !value.is_string() {
            return None;
        }
        overrides.insert(selector.clone(), value.clone());
    }
    Some(overrides)
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
        "overrides.to-pnpm",
        "overrides.to-resolutions",
        "overrides.nested-to-selector",
        "overrides.nested-to-resolutions",
        "resolutions.to-overrides",
        "resolutions.to-pnpm-overrides",
        "linker.pnp-to-node-modules",
        "linker.pnp-to-isolated",
        "linker.isolated-to-yarn-pnpm",
        "linker.isolated-to-hoisted",
        "registry.npmrc-to-yarnrc",
        "lifecycle.to-pnpm-build-policy",
        "lifecycle.to-yarn-build-policy",
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
    let root_manifest_before = read_json_object(&root.join("package.json"))?.unwrap_or_default();
    let manifest_overrides = root_manifest_before
        .get("overrides")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let pnpm_manifest = root_manifest_before
        .get("pnpm")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let pnpm_configuration = pnpm_workspace
        .as_deref()
        .and_then(|content| noyalib::from_str::<Value>(content).ok())
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default();
    let yarn_configuration = read_text(&root.join(".yarnrc.yml"))?
        .as_deref()
        .and_then(|content| noyalib::from_str::<Value>(content).ok())
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default();
    let trusted_dependencies = source_trusted_dependencies(
        &root_manifest_before,
        &pnpm_manifest,
        &pnpm_configuration,
        &yarn_configuration,
    );
    let lifecycle_policy_present = project_ir
        .features
        .iter()
        .any(|feature| feature.id == "lifecycle.trusted-dependencies");
    let remove_yarn_build_policy = inspection.package_manager.selected
        == Some(PackageManagerId::YarnModern)
        && yarn_configuration
            .get("enableScripts")
            .and_then(Value::as_bool)
            == Some(false);
    if inspection.package_manager.selected == Some(PackageManagerId::YarnModern)
        && !remove_yarn_build_policy
        && root_manifest_before
            .get("dependenciesMeta")
            .and_then(Value::as_object)
            .is_some_and(|entries| {
                entries.values().any(|metadata| {
                    metadata
                        .as_object()
                        .and_then(|entry| entry.get("built"))
                        .and_then(Value::as_bool)
                        == Some(false)
                })
            })
    {
        diagnostics.push(Diagnostic::blocking(
            "YARN_BUILD_POLICY_UNSUPPORTED",
            "The target cannot preserve Yarn per-dependency build denials safely.",
            vec![
                "Remove the denial or convert it to a reviewed lifecycle allow-list before retrying."
                    .to_owned(),
            ],
        ));
    }
    let source_overrides = pnpm_configuration
        .get("overrides")
        .and_then(Value::as_object)
        .or_else(|| pnpm_manifest.get("overrides").and_then(Value::as_object))
        .cloned()
        .filter(|overrides| !overrides.is_empty())
        .unwrap_or(manifest_overrides);
    let source_resolutions = root_manifest_before
        .get("resolutions")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let selected_policy = if source_overrides.is_empty() {
        &source_resolutions
    } else {
        &source_overrides
    };
    let mut pnpm_overrides = Map::new();
    let rendered_policy = match target {
        PackageManagerId::Pnpm if !selected_policy.is_empty() => {
            if source_overrides.is_empty() {
                compatible_resolutions(selected_policy)
            } else {
                flatten_nested_overrides(selected_policy, ">")
            }
        }
        PackageManagerId::YarnClassic | PackageManagerId::YarnModern
            if !source_overrides.is_empty() =>
        {
            flatten_nested_overrides(selected_policy, "/")
        }
        PackageManagerId::Npm if source_overrides.is_empty() && !source_resolutions.is_empty() => {
            compatible_resolutions(selected_policy)
        }
        PackageManagerId::Npm | PackageManagerId::Bun if !source_overrides.is_empty() => {
            Some(source_overrides.clone())
        }
        _ => Some(Map::new()),
    };
    if let Some(policy) = rendered_policy.as_ref() {
        if target == PackageManagerId::Pnpm {
            pnpm_overrides = policy.clone();
        }
    } else if source_overrides.is_empty() {
        diagnostics.push(Diagnostic::blocking(
            "RESOLUTION_SELECTOR_UNSUPPORTED",
            "Yarn resolution selectors cannot be translated without reducing selector fidelity.",
            vec!["Review the root resolutions policy before retrying the migration.".to_owned()],
        ));
    } else {
        diagnostics.push(Diagnostic::blocking(
            "NESTED_OVERRIDE_UNSUPPORTED",
            "Nested overrides exceed the deterministic target selector subset.",
            vec!["Reduce nested overrides to one parent-child level before retrying.".to_owned()],
        ));
    }
    for package in &project_ir.packages {
        let Some(mut manifest) = read_json_object(&root.join(&package.manifest_path))? else {
            continue;
        };
        if package.path == "." {
            remove_source_lifecycle_policy(&mut manifest, remove_yarn_build_policy);
            manifest.insert(
                "packageManager".to_owned(),
                Value::String(get_package_manager(target).package_manager_pin.to_owned()),
            );
            if target != PackageManagerId::Pnpm
                && let Some(pnpm) = manifest.get_mut("pnpm").and_then(Value::as_object_mut)
            {
                pnpm.remove("overrides");
                if pnpm.is_empty() {
                    manifest.remove("pnpm");
                }
            }
            match target {
                PackageManagerId::Pnpm => {
                    manifest.remove("overrides");
                    manifest.remove("resolutions");
                }
                PackageManagerId::YarnClassic | PackageManagerId::YarnModern
                    if !source_overrides.is_empty() =>
                {
                    manifest.remove("overrides");
                    manifest.remove("resolutions");
                    if let Some(policy) = rendered_policy.as_ref() {
                        manifest.insert("resolutions".to_owned(), Value::Object(policy.clone()));
                    }
                }
                PackageManagerId::Npm
                    if source_overrides.is_empty() && !source_resolutions.is_empty() =>
                {
                    manifest.remove("resolutions");
                    if let Some(policy) = rendered_policy.as_ref() {
                        manifest.insert("overrides".to_owned(), Value::Object(policy.clone()));
                    }
                }
                PackageManagerId::Npm | PackageManagerId::Bun if !source_overrides.is_empty() => {
                    manifest.remove("resolutions");
                    if let Some(policy) = rendered_policy.as_ref() {
                        manifest.insert("overrides".to_owned(), Value::Object(policy.clone()));
                    }
                }
                _ => {}
            }
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
                if !trusted_dependencies.is_empty() {
                    manifest.insert(
                        "trustedDependencies".to_owned(),
                        Value::Array(
                            trusted_dependencies
                                .iter()
                                .cloned()
                                .map(Value::String)
                                .collect(),
                        ),
                    );
                }
            }
            if target == PackageManagerId::YarnModern {
                configure_yarn_lifecycle_policy(&mut manifest, &trusted_dependencies);
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

    let pnp = project_ir
        .features
        .iter()
        .any(|feature| feature.id == "install.pnp-linker");
    let isolated = project_ir
        .features
        .iter()
        .any(|feature| feature.id == "install.isolated-linker");
    let pnpm_node_linker = pnp
        .then_some("pnp")
        .or_else(|| isolated.then_some("isolated"));
    let mut configuration_mutations = Vec::new();
    if target == PackageManagerId::Pnpm
        && (!project_ir.workspace_patterns.is_empty()
            || !pnpm_overrides.is_empty()
            || pnpm_node_linker.is_some()
            || lifecycle_policy_present)
        && let Some(root_manifest) = root_manifest_after.as_ref()
        && let Some(change) = mutation(
            root,
            "pnpm-workspace.yaml",
            MutationAction::Write,
            Some(render_pnpm_workspace(
                &project_ir.workspace_patterns,
                root_manifest,
                &pnpm_overrides,
                pnpm_node_linker,
                &trusted_dependencies,
                lifecycle_policy_present,
            )),
            "Render pnpm workspace and policy configuration.",
            vec![
                "workspace.manifest".to_owned(),
                "resolution.overrides".to_owned(),
                "install.pnp-linker".to_owned(),
                "install.isolated-linker".to_owned(),
                "lifecycle.trusted-dependencies".to_owned(),
            ],
        )?
    {
        configuration_mutations.push(change);
    }
    if target == PackageManagerId::YarnModern {
        let npmrc = read_text(&root.join(".npmrc"))?;
        let registry = npmrc
            .as_deref()
            .map(|content| npmrc_for_yarn(content, &mut diagnostics))
            .unwrap_or_default();
        let yarn_node_linker = if pnp {
            "pnp"
        } else if isolated {
            "pnpm"
        } else {
            "node-modules"
        };
        if let Some(change) = mutation(
            root,
            ".yarnrc.yml",
            MutationAction::Write,
            Some(render_yarn_configuration(
                yarn_node_linker,
                lifecycle_policy_present,
                &registry,
            )),
            "Render Yarn Modern linker, lifecycle, and registry configuration.",
            vec![
                "install.pnp-linker".to_owned(),
                "install.isolated-linker".to_owned(),
                "registry.npmrc".to_owned(),
                "lifecycle.trusted-dependencies".to_owned(),
            ],
        )? {
            configuration_mutations.push(change);
        }
    }
    if target == PackageManagerId::Bun && (pnp || isolated) {
        let before = read_text(&root.join("bunfig.toml"))?;
        if let Some(after) = render_bun_configuration(before.as_deref(), true) {
            if let Some(change) = mutation(
                root,
                "bunfig.toml",
                MutationAction::Write,
                Some(after),
                "Select Bun isolated linking for a reviewed linker migration.",
                vec![
                    "install.pnp-linker".to_owned(),
                    "install.isolated-linker".to_owned(),
                ],
            )? {
                configuration_mutations.push(change);
            }
        } else {
            diagnostics.push(Diagnostic::blocking(
                "CONFIGURATION_PARSE_FAILED",
                "bunfig.toml contains ambiguous install linker configuration.",
                vec![
                    "Keep one [install] section and one linker setting before retrying.".to_owned(),
                ],
            ));
        }
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
        if target_artifacts.contains(path)
            || (path == ".npmrc" && target != PackageManagerId::YarnModern)
        {
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

    use serde_json::json;
    use tempfile::tempdir;

    use crate::inspect::{build_project_ir, inspect_project};

    use super::*;

    fn plan_manifest_policy(
        source: PackageManagerId,
        target: PackageManagerId,
        policy: &Value,
        accepted_lossy: bool,
    ) -> MigrationPlan {
        let directory = tempdir().expect("temporary directory");
        let source_definition = get_package_manager(source);
        let mut manifest = Map::from_iter([
            ("name".to_owned(), Value::String("fixture".to_owned())),
            ("private".to_owned(), Value::Bool(true)),
            (
                "packageManager".to_owned(),
                Value::String(source_definition.package_manager_pin.to_owned()),
            ),
        ]);
        let policy = policy.as_object().expect("policy object");
        manifest.extend(policy.clone());
        fs::write(
            directory.path().join("package.json"),
            json_content(&manifest).expect("manifest serialization"),
        )
        .expect("manifest");
        fs::write(
            directory.path().join(source_definition.lockfiles[0]),
            "fixture\n",
        )
        .expect("source lockfile");
        let inspection = inspect_project(directory.path()).expect("inspection");
        let ir = build_project_ir(&inspection)
            .expect("IR build")
            .expect("project IR");
        let analysis = analyze_capabilities(&ir, target)
            .expect("analysis")
            .expect("capability analysis");
        plan_package_manager_migration(&inspection, &ir, &analysis, None, target, accepted_lossy)
            .expect("planning")
            .expect("migration plan")
    }

    fn mutation_content<'a>(plan: &'a MigrationPlan, path: &str) -> &'a str {
        plan.operations
            .iter()
            .flat_map(|operation| &operation.mutations)
            .find(|mutation| mutation.path == path && mutation.action == MutationAction::Write)
            .and_then(|mutation| mutation.content.as_deref())
            .expect("planned mutation content")
    }

    fn plan_fixture(
        files: &[(&str, &str)],
        target: PackageManagerId,
        accepted_lossy: bool,
    ) -> MigrationPlan {
        let directory = tempdir().expect("temporary directory");
        for (path, content) in files {
            let path = directory.path().join(path);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).expect("fixture parent");
            }
            fs::write(path, content).expect("fixture file");
        }
        let inspection = inspect_project(directory.path()).expect("inspection");
        let ir = build_project_ir(&inspection)
            .expect("IR build")
            .expect("project IR");
        let analysis = analyze_capabilities(&ir, target)
            .expect("analysis")
            .expect("capability analysis");
        plan_package_manager_migration(&inspection, &ir, &analysis, None, target, accepted_lossy)
            .expect("planning")
            .expect("migration plan")
    }

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

    #[test]
    fn renders_nested_npm_overrides_as_pnpm_selectors() {
        let plan = plan_manifest_policy(
            PackageManagerId::Npm,
            PackageManagerId::Pnpm,
            &json!({
                "overrides": {
                    "parent": {
                        ".": "2.0.0",
                        "child": "1.2.3"
                    }
                }
            }),
            false,
        );

        assert!(plan.executable);
        assert!(
            !plan
                .diagnostics
                .iter()
                .any(|entry| entry.code == "TRANSFORMATION_UNIMPLEMENTED")
        );
        let manifest: Value =
            serde_json::from_str(mutation_content(&plan, "package.json")).expect("manifest JSON");
        assert!(manifest.get("overrides").is_none());
        let configuration = mutation_content(&plan, "pnpm-workspace.yaml");
        assert!(configuration.contains("'parent': '2.0.0'"));
        assert!(configuration.contains("'parent>child': '1.2.3'"));
    }

    #[test]
    fn renders_nested_npm_overrides_as_yarn_resolutions() {
        let plan = plan_manifest_policy(
            PackageManagerId::Npm,
            PackageManagerId::YarnModern,
            &json!({
                "overrides": {
                    "parent": {
                        "child": "1.2.3"
                    }
                }
            }),
            true,
        );

        assert!(plan.executable);
        assert!(plan.accepted_lossy);
        let manifest: Value =
            serde_json::from_str(mutation_content(&plan, "package.json")).expect("manifest JSON");
        assert_eq!(manifest["resolutions"]["parent/child"], "1.2.3");
        assert!(manifest.get("overrides").is_none());
    }

    #[test]
    fn renders_compatible_yarn_resolutions_as_npm_overrides() {
        let plan = plan_manifest_policy(
            PackageManagerId::YarnClassic,
            PackageManagerId::Npm,
            &json!({
                "resolutions": {
                    "@scope/package": "1.2.3",
                    "lodash": "4.17.21"
                }
            }),
            false,
        );

        assert!(plan.executable);
        let manifest: Value =
            serde_json::from_str(mutation_content(&plan, "package.json")).expect("manifest JSON");
        assert_eq!(manifest["overrides"]["@scope/package"], "1.2.3");
        assert_eq!(manifest["overrides"]["lodash"], "4.17.21");
        assert!(manifest.get("resolutions").is_none());
    }

    #[test]
    fn blocks_yarn_resolution_selectors_that_cannot_preserve_fidelity() {
        let plan = plan_manifest_policy(
            PackageManagerId::YarnClassic,
            PackageManagerId::Npm,
            &json!({ "resolutions": { "parent/child": "1.2.3" } }),
            false,
        );

        assert!(!plan.executable);
        assert!(
            plan.diagnostics
                .iter()
                .any(|entry| entry.code == "RESOLUTION_SELECTOR_UNSUPPORTED" && entry.blocking)
        );
    }

    #[test]
    fn carries_pnpm_workspace_overrides_into_npm() {
        let directory = tempdir().expect("temporary directory");
        fs::write(
            directory.path().join("package.json"),
            r#"{"name":"fixture","private":true,"packageManager":"pnpm@11.21.0"}"#,
        )
        .expect("manifest");
        fs::write(
            directory.path().join("pnpm-workspace.yaml"),
            "overrides:\n  parent:\n    child: 1.2.3\n",
        )
        .expect("pnpm configuration");
        fs::write(
            directory.path().join("pnpm-lock.yaml"),
            "lockfileVersion: '9.0'\n",
        )
        .expect("source lockfile");

        let inspection = inspect_project(directory.path()).expect("inspection");
        let ir = build_project_ir(&inspection)
            .expect("IR build")
            .expect("project IR");
        assert!(
            ir.features
                .iter()
                .any(|feature| feature.id == "resolution.overrides")
        );
        assert!(
            ir.features
                .iter()
                .any(|feature| feature.id == "resolution.nested-overrides")
        );
        let analysis = analyze_capabilities(&ir, PackageManagerId::Npm)
            .expect("analysis")
            .expect("capability analysis");
        let plan = plan_package_manager_migration(
            &inspection,
            &ir,
            &analysis,
            None,
            PackageManagerId::Npm,
            false,
        )
        .expect("planning")
        .expect("migration plan");

        assert!(plan.executable);
        let manifest: Value =
            serde_json::from_str(mutation_content(&plan, "package.json")).expect("manifest JSON");
        assert_eq!(manifest["overrides"]["parent"]["child"], "1.2.3");
        assert!(
            plan.operations
                .iter()
                .flat_map(|entry| &entry.mutations)
                .any(|mutation| mutation.path == "pnpm-workspace.yaml"
                    && mutation.action == MutationAction::Delete)
        );
    }

    #[test]
    fn blocks_override_nesting_beyond_the_deterministic_subset() {
        let plan = plan_manifest_policy(
            PackageManagerId::Npm,
            PackageManagerId::Pnpm,
            &json!({
                "overrides": {
                    "grandparent": {
                        "parent": {
                            "child": "1.2.3"
                        }
                    }
                }
            }),
            false,
        );

        assert!(!plan.executable);
        assert!(
            plan.diagnostics
                .iter()
                .any(|entry| entry.code == "NESTED_OVERRIDE_UNSUPPORTED" && entry.blocking)
        );
    }

    #[test]
    fn renders_environment_backed_registry_configuration_for_yarn_modern() {
        let plan = plan_fixture(
            &[
                (
                    "package.json",
                    r#"{"name":"fixture","private":true,"packageManager":"npm@12.0.2"}"#,
                ),
                ("package-lock.json", "{}\n"),
                (
                    ".npmrc",
                    "registry=https://registry.npmjs.org\n@company:registry=https://npm.company.test\n//npm.company.test/:_authToken=${COMPANY_NPM_TOKEN}\nalways-auth=true\n",
                ),
            ],
            PackageManagerId::YarnModern,
            false,
        );

        assert!(plan.executable);
        let configuration = mutation_content(&plan, ".yarnrc.yml");
        assert!(configuration.contains("nodeLinker: node-modules"));
        assert!(configuration.contains("npmRegistryServer: 'https://registry.npmjs.org'"));
        assert!(configuration.contains("'company':"));
        assert!(configuration.contains("'//npm.company.test/':"));
        assert!(configuration.contains("'${COMPANY_NPM_TOKEN}'"));
        assert!(
            plan.operations
                .iter()
                .flat_map(|entry| &entry.mutations)
                .any(|mutation| mutation.path == ".npmrc"
                    && mutation.action == MutationAction::Delete)
        );
    }

    #[test]
    fn keeps_literal_registry_tokens_out_of_persisted_plans() {
        let secret = "literal-token-must-not-persist";
        let npmrc = format!("//registry.npmjs.org/:_authToken={secret}\n");
        let plan = plan_fixture(
            &[
                (
                    "package.json",
                    r#"{"name":"fixture","private":true,"packageManager":"npm@12.0.2"}"#,
                ),
                ("package-lock.json", "{}\n"),
                (".npmrc", &npmrc),
            ],
            PackageManagerId::YarnModern,
            false,
        );

        assert!(!plan.executable);
        assert!(plan.diagnostics.iter().any(|entry| {
            entry.code == "REGISTRY_SECRET_REQUIRES_ENVIRONMENT_REFERENCE" && entry.blocking
        }));
        assert!(
            !serde_json::to_string(&plan)
                .expect("serialized plan")
                .contains(secret)
        );
    }

    #[test]
    fn renders_isolated_linking_and_current_pnpm_build_policy() {
        let plan = plan_fixture(
            &[
                (
                    "package.json",
                    r#"{"name":"fixture","private":true,"packageManager":"bun@1.3.14","trustedDependencies":["esbuild","sharp"]}"#,
                ),
                ("bun.lock", "{\"lockfileVersion\":1,\"packages\":{}}\n"),
                ("bunfig.toml", "[install]\nlinker = \"isolated\"\n"),
            ],
            PackageManagerId::Pnpm,
            false,
        );

        assert!(plan.executable);
        let configuration = mutation_content(&plan, "pnpm-workspace.yaml");
        assert!(configuration.contains("nodeLinker: isolated"));
        assert!(configuration.contains("allowBuilds:"));
        assert!(configuration.contains("'esbuild': true"));
        assert!(configuration.contains("'sharp': true"));
        assert!(!configuration.contains("onlyBuiltDependencies"));
        let manifest: Value =
            serde_json::from_str(mutation_content(&plan, "package.json")).expect("manifest JSON");
        assert!(manifest.get("trustedDependencies").is_none());
    }

    #[test]
    fn renders_a_yarn_lifecycle_allow_list_with_scripts_disabled() {
        let plan = plan_fixture(
            &[
                (
                    "package.json",
                    r#"{"name":"fixture","private":true,"packageManager":"pnpm@11.21.0"}"#,
                ),
                ("pnpm-lock.yaml", "lockfileVersion: '9.0'\n"),
                (
                    "pnpm-workspace.yaml",
                    "nodeLinker: isolated\nallowBuilds:\n  esbuild: true\n  blocked-package: false\n",
                ),
            ],
            PackageManagerId::YarnModern,
            false,
        );

        assert!(plan.executable);
        let configuration = mutation_content(&plan, ".yarnrc.yml");
        assert!(configuration.contains("nodeLinker: pnpm"));
        assert!(configuration.contains("enableScripts: false"));
        let manifest: Value =
            serde_json::from_str(mutation_content(&plan, "package.json")).expect("manifest JSON");
        assert_eq!(manifest["dependenciesMeta"]["esbuild"]["built"], true);
        assert!(
            manifest["dependenciesMeta"]
                .get("blocked-package")
                .is_none()
        );
    }

    #[test]
    fn reads_a_yarn_lifecycle_allow_list_when_migrating_to_bun() {
        let plan = plan_fixture(
            &[
                (
                    "package.json",
                    r#"{"name":"fixture","private":true,"packageManager":"yarn@4.18.0","dependenciesMeta":{"esbuild":{"built":true},"sharp":{"built":false}}}"#,
                ),
                ("yarn.lock", "# fixture\n"),
                (
                    ".yarnrc.yml",
                    "nodeLinker: node-modules\nenableScripts: false\n",
                ),
            ],
            PackageManagerId::Bun,
            false,
        );

        assert!(plan.executable);
        let manifest: Value =
            serde_json::from_str(mutation_content(&plan, "package.json")).expect("manifest JSON");
        assert_eq!(manifest["trustedDependencies"], json!(["esbuild"]));
        assert!(manifest.get("dependenciesMeta").is_none());
    }

    #[test]
    fn blocks_yarn_build_denials_outside_allow_list_mode() {
        let plan = plan_fixture(
            &[
                (
                    "package.json",
                    r#"{"name":"fixture","private":true,"packageManager":"yarn@4.18.0","dependenciesMeta":{"native-addon":{"built":false}}}"#,
                ),
                ("yarn.lock", "# fixture\n"),
                (".yarnrc.yml", "nodeLinker: node-modules\n"),
            ],
            PackageManagerId::Bun,
            false,
        );

        assert!(!plan.executable);
        assert!(
            plan.diagnostics
                .iter()
                .any(|entry| { entry.code == "YARN_BUILD_POLICY_UNSUPPORTED" && entry.blocking })
        );
    }

    #[test]
    fn blocks_unknown_legacy_node_linker_values() {
        let plan = plan_fixture(
            &[
                (
                    "package.json",
                    r#"{"name":"fixture","private":true,"packageManager":"pnpm@11.21.0"}"#,
                ),
                ("pnpm-lock.yaml", "lockfileVersion: '9.0'\n"),
                (".npmrc", "node-linker=mystery\n"),
            ],
            PackageManagerId::YarnModern,
            false,
        );

        assert!(!plan.executable);
        assert!(
            plan.diagnostics
                .iter()
                .any(|entry| entry.code == "NPMRC_SETTING_UNSUPPORTED" && entry.blocking)
        );
    }

    #[test]
    fn preserves_an_empty_yarn_lifecycle_allow_list_in_pnpm() {
        let plan = plan_fixture(
            &[
                (
                    "package.json",
                    r#"{"name":"fixture","private":true,"packageManager":"yarn@4.18.0"}"#,
                ),
                ("yarn.lock", "# fixture\n"),
                (
                    ".yarnrc.yml",
                    "nodeLinker: node-modules\nenableScripts: false\n",
                ),
            ],
            PackageManagerId::Pnpm,
            false,
        );

        assert!(plan.executable);
        assert!(mutation_content(&plan, "pnpm-workspace.yaml").contains("allowBuilds: {}"));
    }
}
