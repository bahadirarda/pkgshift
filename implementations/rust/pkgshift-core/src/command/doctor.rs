use serde_json::json;

use crate::doctor::model::ReadinessVerdict;
use crate::model::{CommandExecution, CommandStatus, NextAction, SideEffect};
use crate::util::Result;

use super::{CommandOptions, artifact, inspection_artifacts, normalized_target, result, summary};

fn next_action(
    options: &CommandOptions,
    target: &str,
    available: bool,
    available_after_review: bool,
) -> Vec<NextAction> {
    if !available && !available_after_review {
        return Vec::new();
    }
    let mut argv = vec![
        "pkgshift".to_owned(),
        "plan".to_owned(),
        "package-manager".to_owned(),
        "--to".to_owned(),
        target.to_owned(),
    ];
    if options.accept_lossy || available_after_review {
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

pub(crate) fn doctor_command(
    options: &CommandOptions,
    target_value: &str,
) -> Result<CommandExecution> {
    let target = match normalized_target("doctor", target_value) {
        Ok(target) => target,
        Err(execution) => return Ok(execution),
    };
    let assessment = crate::doctor::assess(
        &options.cwd,
        target,
        options.accept_lossy,
        &options.verification_scripts,
    )?;
    let mut artifacts =
        inspection_artifacts(&assessment.inspection, assessment.project_ir.as_ref())?;
    if let Some(analysis) = &assessment.capability_analysis {
        artifacts.push(artifact(
            analysis.analysis_id.clone(),
            "capability-analysis",
            "application/vnd.pkgshift.capability-analysis+json",
            analysis,
        )?);
    }
    if let Some(graph) = &assessment.source_lock_graph {
        artifacts.push(artifact(
            graph.graph_id.clone(),
            "source-lock-graph",
            "application/vnd.pkgshift.lock-graph+json",
            graph,
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
            next_action(
                options,
                &target.to_string(),
                assessment.report.migration_available,
                assessment.report.available_after_review,
            ),
        ),
    })
}
