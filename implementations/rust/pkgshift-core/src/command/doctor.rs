use serde_json::json;

use crate::doctor::model::{MigrationReadiness, ReadinessVerdict};
use crate::model::{CommandExecution, CommandStatus, NextAction, ResultArtifact, SideEffect};
use crate::util::Result;

use super::{CommandOptions, artifact, inspection_artifacts, normalized_target, result, summary};

fn next_action_for_report(
    options: &CommandOptions,
    report: &MigrationReadiness,
) -> Vec<NextAction> {
    if !report.migration_available && !report.available_after_review {
        return Vec::new();
    }
    let mut argv = vec![
        "pkgshift".to_owned(),
        "plan".to_owned(),
        "package-manager".to_owned(),
        "--to".to_owned(),
        report.target.to_string(),
    ];
    if options.accept_lossy || report.available_after_review {
        argv.push("--accept-lossy".to_owned());
    }
    for script in &options.verification_scripts {
        argv.push("--verify-script".to_owned());
        argv.push(script.clone());
    }
    argv.extend([
        "--json".to_owned(),
        "--no-color".to_owned(),
        "--non-interactive".to_owned(),
    ]);
    vec![NextAction {
        argv,
        requires_approval: false,
        side_effect: SideEffect::None,
    }]
}

fn context_artifacts(context: &crate::doctor::ReadinessContext) -> Result<Vec<ResultArtifact>> {
    let mut artifacts = inspection_artifacts(&context.inspection, context.project_ir.as_ref())?;
    if let Some(graph) = &context.source_lock_graph {
        artifacts.push(artifact(
            graph.graph_id.clone(),
            "source-lock-graph",
            "application/vnd.pkgshift.lock-graph+json",
            graph,
        )?);
    }
    Ok(artifacts)
}

fn target_doctor_command(options: &CommandOptions, target_value: &str) -> Result<CommandExecution> {
    let target = match normalized_target("doctor", target_value) {
        Ok(target) => target,
        Err(execution) => return Ok(execution),
    };
    let context = crate::doctor::load_context(&options.cwd)?;
    let assessment = crate::doctor::assess(
        &context,
        target,
        options.accept_lossy,
        &options.verification_scripts,
    )?;
    let mut artifacts = context_artifacts(&context)?;
    if let Some(analysis) = &assessment.capability_analysis {
        artifacts.push(artifact(
            analysis.analysis_id.clone(),
            "capability-analysis",
            "application/vnd.pkgshift.capability-analysis+json",
            analysis,
        )?);
    }
    artifacts.push(artifact(
        assessment.report.report_id.clone(),
        "migration-readiness",
        "application/vnd.pkgshift.readiness+json",
        &assessment.report,
    )?);
    let completed = assessment.report.migration_available
        || assessment.report.verdict == ReadinessVerdict::AlreadySelected;
    let blockers = assessment
        .report
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.blocking)
        .count();
    let warnings = assessment
        .report
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.severity == crate::model::DiagnosticSeverity::Warning)
        .count();
    Ok(CommandExecution {
        exit_code: if completed { 0 } else { 3 },
        result: result(
            "doctor",
            if completed {
                CommandStatus::Completed
            } else {
                CommandStatus::Blocked
            },
            summary([
                ("source", json!(assessment.report.source)),
                ("target", json!(assessment.report.target)),
                ("verdict", json!(assessment.report.verdict)),
                (
                    "migrationAvailable",
                    json!(assessment.report.migration_available),
                ),
                (
                    "availableAfterReview",
                    json!(assessment.report.available_after_review),
                ),
                ("packages", json!(assessment.report.package_count)),
                ("workspace", json!(assessment.report.workspace_configured)),
                (
                    "integrations",
                    json!(assessment.report.integrations.total()),
                ),
                ("blockers", json!(blockers)),
                ("warnings", json!(warnings)),
                ("repositoryChanged", json!(false)),
            ]),
            assessment.report.diagnostics.clone(),
            artifacts,
            None,
            None,
            next_action_for_report(options, &assessment.report),
        ),
    })
}

fn matrix_doctor_command(options: &CommandOptions) -> Result<CommandExecution> {
    let context = crate::doctor::load_context(&options.cwd)?;
    let matrix = crate::doctor::assess_all(
        &context,
        options.accept_lossy,
        &options.verification_scripts,
    )?;
    let mut artifacts = context_artifacts(&context)?;
    artifacts.push(artifact(
        matrix.matrix_id.clone(),
        "migration-readiness-matrix",
        "application/vnd.pkgshift.readiness-matrix+json",
        &matrix,
    )?);
    let completed = matrix.source.is_some();
    let next_actions = matrix
        .reports
        .iter()
        .flat_map(|report| next_action_for_report(options, report))
        .collect();
    Ok(CommandExecution {
        exit_code: if completed { 0 } else { 3 },
        result: result(
            "doctor",
            if completed {
                CommandStatus::Completed
            } else {
                CommandStatus::Blocked
            },
            summary([
                ("source", json!(matrix.source)),
                ("targets", json!(matrix.summary.targets)),
                (
                    "migrationAvailableTargets",
                    json!(matrix.summary.migration_available_targets),
                ),
                (
                    "availableAfterReviewTargets",
                    json!(matrix.summary.available_after_review_targets),
                ),
                ("readyTargets", json!(matrix.summary.ready_targets)),
                (
                    "reviewRequiredTargets",
                    json!(matrix.summary.review_required_targets),
                ),
                ("blockedTargets", json!(matrix.summary.blocked_targets)),
                (
                    "alreadySelectedTargets",
                    json!(matrix.summary.already_selected_targets),
                ),
                ("repositoryChanged", json!(false)),
            ]),
            context.inspection.diagnostics.clone(),
            artifacts,
            None,
            None,
            next_actions,
        ),
    })
}

pub(crate) fn doctor_command(
    options: &CommandOptions,
    target_value: Option<&str>,
) -> Result<CommandExecution> {
    match target_value {
        Some(target) => target_doctor_command(options, target),
        None => matrix_doctor_command(options),
    }
}
