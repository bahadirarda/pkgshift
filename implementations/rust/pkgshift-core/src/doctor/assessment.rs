use std::collections::BTreeSet;

use serde::Serialize;

use crate::VerificationPolicy;
use crate::catalog::get_package_manager;
use crate::doctor::context::ReadinessContext;
use crate::doctor::model::{
    IntegrationImpact, MigrationEffects, MigrationReadiness, ReadinessVerdict,
};
use crate::model::{
    CapabilityAnalysis, CapabilitySummary, Diagnostic, DiagnosticSeverity, IntegrationKind,
    MigrationPlan, MutationAction, PackageManagerId, ProjectIr, SupportTier,
};
use crate::plan::{analyze_capabilities, plan_package_manager_migration};
use crate::util::{Result, short_digest};

pub(crate) struct ReadinessAssessment {
    pub capability_analysis: Option<CapabilityAnalysis>,
    pub report: MigrationReadiness,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ReportIdentity<'a> {
    repository_fingerprint: &'a str,
    project_ir_id: Option<&'a str>,
    capability_analysis_id: Option<&'a str>,
    source: Option<PackageManagerId>,
    target: PackageManagerId,
    target_tier: SupportTier,
    verdict: ReadinessVerdict,
    executable: bool,
    available_after_review: bool,
    accepted_lossy: bool,
    verification_policy: &'a VerificationPolicy,
    package_count: usize,
    workspace_patterns: &'a [String],
    available_root_scripts: &'a [String],
    integrations: &'a IntegrationImpact,
    capabilities: &'a CapabilitySummary,
    effects: &'a MigrationEffects,
    diagnostics: &'a [Diagnostic],
}

fn integration_impact(project: Option<&ProjectIr>) -> IntegrationImpact {
    let mut ci = BTreeSet::new();
    let mut containers = BTreeSet::new();
    let mut documentation = BTreeSet::new();
    let mut automation = BTreeSet::new();
    for integration in project.into_iter().flat_map(|value| &value.integrations) {
        match integration.kind {
            IntegrationKind::Ci => &mut ci,
            IntegrationKind::Container => &mut containers,
            IntegrationKind::Documentation => &mut documentation,
            IntegrationKind::Automation => &mut automation,
        }
        .insert(integration.path.clone());
    }
    IntegrationImpact {
        ci: ci.into_iter().collect(),
        containers: containers.into_iter().collect(),
        documentation: documentation.into_iter().collect(),
        automation: automation.into_iter().collect(),
    }
}

fn available_root_scripts(project: Option<&ProjectIr>) -> Vec<String> {
    project
        .and_then(|value| {
            value
                .packages
                .iter()
                .find(|package| package.path == value.root_package_path)
        })
        .map_or_else(Vec::new, |package| package.script_names.clone())
}

fn migration_effects(
    plan: Option<&MigrationPlan>,
    verification_scripts: &[String],
) -> MigrationEffects {
    let mut writes = BTreeSet::new();
    let mut deletions = BTreeSet::new();
    let mut dependency_state = BTreeSet::new();
    let mut source_artifacts = BTreeSet::new();
    let mut commands = BTreeSet::new();
    for operation in plan.into_iter().flat_map(|value| &value.operations) {
        if operation.kind == "dependency.clean-source-state" {
            dependency_state.extend(operation.paths.iter().cloned());
        }
        if !operation.command.is_empty() {
            commands.insert(operation.command.clone());
        }
        for mutation in &operation.mutations {
            match mutation.action {
                MutationAction::Write => {
                    writes.insert(mutation.path.clone());
                }
                MutationAction::Delete => {
                    deletions.insert(mutation.path.clone());
                    if operation.kind == "source.retire" {
                        source_artifacts.insert(mutation.path.clone());
                    }
                }
            }
        }
    }
    MigrationEffects {
        file_writes: writes.into_iter().collect(),
        file_deletions: deletions.into_iter().collect(),
        dependency_state_cleanups: dependency_state.into_iter().collect(),
        source_artifact_retirements: source_artifacts.into_iter().collect(),
        process_commands: commands.into_iter().collect(),
        verification_scripts: verification_scripts.to_vec(),
    }
}

fn verdict(
    source: Option<PackageManagerId>,
    target: PackageManagerId,
    diagnostics: &[Diagnostic],
    executable: bool,
) -> (ReadinessVerdict, bool) {
    if source == Some(target) {
        return (ReadinessVerdict::AlreadySelected, false);
    }
    let blockers = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.blocking)
        .collect::<Vec<_>>();
    let available_after_review = !blockers.is_empty()
        && blockers
            .iter()
            .all(|diagnostic| diagnostic.code == "LOSSY_ACCEPTANCE_REQUIRED");
    if !blockers.is_empty() {
        return (
            if available_after_review {
                ReadinessVerdict::ReviewRequired
            } else {
                ReadinessVerdict::Blocked
            },
            available_after_review,
        );
    }
    if executable
        && diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == DiagnosticSeverity::Warning)
    {
        return (ReadinessVerdict::ReviewRequired, false);
    }
    if executable {
        (ReadinessVerdict::Ready, false)
    } else {
        (ReadinessVerdict::Blocked, false)
    }
}

pub(crate) fn assess(
    context: &ReadinessContext,
    target: PackageManagerId,
    accepted_lossy: bool,
    verification_scripts: &[String],
    verification_policy: &VerificationPolicy,
) -> Result<ReadinessAssessment> {
    let capability_analysis = match context.project_ir.as_ref() {
        Some(project) => analyze_capabilities(project, target)?,
        None => None,
    };
    let plan = match (context.project_ir.as_ref(), capability_analysis.as_ref()) {
        (Some(project), Some(analysis)) => plan_package_manager_migration(
            &context.inspection,
            project,
            analysis,
            context.source_lock_graph.as_ref(),
            target,
            accepted_lossy,
            verification_scripts,
            verification_policy,
        )?,
        _ => None,
    };
    let diagnostics = plan.as_ref().map_or_else(
        || {
            context.project_ir.as_ref().map_or_else(
                || context.inspection.diagnostics.clone(),
                |project| project.diagnostics.clone(),
            )
        },
        |value| value.diagnostics.clone(),
    );
    let source = context
        .project_ir
        .as_ref()
        .and_then(|project| project.source);
    let executable = plan.as_ref().is_some_and(|value| value.executable);
    let (verdict, available_after_review) = verdict(source, target, &diagnostics, executable);
    let integrations = integration_impact(context.project_ir.as_ref());
    let effects = migration_effects(plan.as_ref(), verification_scripts);
    let capabilities = capability_analysis
        .as_ref()
        .map_or_else(CapabilitySummary::default, |value| value.summary.clone());
    let package_count = context
        .project_ir
        .as_ref()
        .map_or(0, |value| value.packages.len());
    let workspace_patterns = context
        .project_ir
        .as_ref()
        .map_or_else(Vec::new, |value| value.workspace_patterns.clone());
    let scripts = available_root_scripts(context.project_ir.as_ref());
    let project_ir_id = context
        .project_ir
        .as_ref()
        .map(|value| value.project_ir_id.clone());
    let capability_analysis_id = capability_analysis
        .as_ref()
        .map(|value| value.analysis_id.clone());
    let target_tier = get_package_manager(target).tier;
    let report_id = short_digest(
        "doctor_",
        &ReportIdentity {
            repository_fingerprint: &context.inspection.fingerprint,
            project_ir_id: project_ir_id.as_deref(),
            capability_analysis_id: capability_analysis_id.as_deref(),
            source,
            target,
            target_tier,
            verdict,
            executable,
            available_after_review,
            accepted_lossy,
            verification_policy,
            package_count,
            workspace_patterns: &workspace_patterns,
            available_root_scripts: &scripts,
            integrations: &integrations,
            capabilities: &capabilities,
            effects: &effects,
            diagnostics: &diagnostics,
        },
    )?;
    let report = MigrationReadiness {
        schema_version: MigrationReadiness::schema_version(),
        report_id,
        verdict,
        read_only: true,
        migration_available: executable,
        available_after_review,
        accepted_lossy,
        verification_policy: verification_policy.clone(),
        source,
        target,
        target_tier,
        repository_fingerprint: context.inspection.fingerprint.clone(),
        project_ir_id,
        capability_analysis_id,
        package_count,
        workspace_configured: context.inspection.workspace.configured,
        workspace_patterns,
        available_root_scripts: scripts,
        integrations,
        capabilities,
        effects,
        diagnostics,
    };
    Ok(ReadinessAssessment {
        capability_analysis,
        report,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distinguishes_review_only_blockers_from_hard_blockers() {
        let lossy =
            Diagnostic::blocking("LOSSY_ACCEPTANCE_REQUIRED", "review required", Vec::new());
        assert_eq!(
            verdict(
                Some(PackageManagerId::Pnpm),
                PackageManagerId::Bun,
                &[lossy],
                false
            ),
            (ReadinessVerdict::ReviewRequired, true)
        );
        let unsupported = Diagnostic::blocking("CAPABILITY_UNSUPPORTED", "blocked", Vec::new());
        assert_eq!(
            verdict(
                Some(PackageManagerId::Pnpm),
                PackageManagerId::Bun,
                &[unsupported],
                false
            ),
            (ReadinessVerdict::Blocked, false)
        );
    }
}
