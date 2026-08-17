use crate::model::{
    CapabilityAnalysis, CapabilityClassification, CapabilityDecision, CapabilitySummary,
    Diagnostic, DiagnosticSeverity, EvidenceDetail, PackageManagerId, ProjectIr, SCHEMA_VERSION,
};
use crate::util::{Result, short_digest};

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
            Vlt => transform(
                "workspace.to-vlt-workspace",
                "vlt requires workspace membership in vlt.json.",
            ),
        },
        "workspace.negative-patterns" => match target {
            Pnpm | Bun => native("The target supports workspace exclusion patterns."),
            _ => unknown("Equivalent workspace exclusion behavior is not verified."),
        },
        "dependency.workspace-protocol" => match target {
            Pnpm | YarnModern | Bun | Vlt | Deno => {
                native("The target supports workspace specifiers.")
            }
            Npm | YarnClassic => transform(
                "workspace.expand-to-semver",
                "Workspace specifiers must be expanded to semver ranges.",
            ),
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
            Vlt => native("vlt natively represents dependency catalogs."),
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
            Vlt => unsupported("vlt has no supported patch protocol mapping in this adapter."),
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
            Bun => unknown("Portal semantics are not verified for the target."),
            Vlt => unsupported("vlt has no supported portal mapping in this adapter."),
            Deno => unsupported("Deno dependency mode has no portal equivalent."),
        },
        "dependency.link-protocol" => match target {
            Pnpm | YarnClassic | YarnModern => native("The target supports link references."),
            Npm => lossy("link.to-file", "Link references become file references."),
            Bun => unknown("Link protocol parity is not verified for the target."),
            Vlt => unsupported("vlt has no supported link protocol mapping in this adapter."),
            Deno => unsupported("Deno dependency mode has no link protocol equivalent."),
        },
        "dependency.deno-import-map" => match target {
            Deno => native("Deno preserves imports and scopes in its runtime configuration."),
            _ => {
                unsupported("Deno import maps are outside the package-manager migration boundary.")
            }
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
            Vlt => transform(
                "overrides.to-vlt-modifiers",
                "Overrides require vlt graph modifier rendering.",
            ),
            Deno => native("Deno honors npm-compatible overrides in package.json."),
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
            Bun => unsupported("The target has no safe nested override mapping."),
            Vlt => transform(
                "overrides.to-vlt-modifiers",
                "Nested overrides require contextual vlt dependency selectors.",
            ),
            Deno => native("Deno honors nested npm-compatible overrides in package.json."),
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
            Vlt => transform(
                "resolutions.to-vlt-modifiers",
                "Resolutions require vlt graph modifier rendering.",
            ),
            Deno => transform(
                "resolutions.to-overrides",
                "Resolutions require Deno-compatible npm override rendering.",
            ),
        },
        "resolution.package-extensions" => match target {
            Npm | Pnpm | YarnModern => native("The target supports package extensions."),
            Bun => unknown("Package extension parity is not verified."),
            Vlt => unsupported("vlt has no supported package extensions mapping."),
            YarnClassic | Deno => unsupported("The target has no package extensions mechanism."),
        },
        "patch.patched-dependencies" => match target {
            Pnpm | Bun => native("The target supports patched dependency policy."),
            YarnModern => transform(
                "patch.patched-to-yarn",
                "Patched dependencies require Yarn patch protocol rendering.",
            ),
            Vlt => unsupported("vlt has no supported patched dependency mapping."),
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
            Vlt => lossy(
                "linker.pnp-to-isolated",
                "The migration switches from Plug and Play to vlt isolation.",
            ),
            Deno => lossy(
                "linker.pnp-to-isolated",
                "The migration switches from Plug and Play to Deno isolation.",
            ),
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
            Vlt | Deno => native("The target supports isolated dependency linking."),
        },
        "policy.yarn-constraints" => match target {
            YarnModern => native("The target executes Yarn constraints."),
            _ => unsupported("Arbitrary Yarn constraint logic cannot be translated safely."),
        },
        "hook.pnpmfile" => match target {
            Pnpm => native("The target executes pnpm hook modules."),
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
            Vlt => transform(
                "registry.npmrc-to-vlt",
                "Public registry mappings require vlt.json rendering; credentials stay external.",
            ),
            Deno => native("Deno consumes npm-compatible registry configuration."),
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
            Vlt | Deno => unsupported(
                "The target lifecycle allow-list cannot be preserved with a script-free migration install.",
            ),
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
