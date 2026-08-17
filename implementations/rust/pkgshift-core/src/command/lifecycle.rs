use serde_json::json;

use super::{CommandOptions, artifact, resolve_state_directory, result, summary};
use crate::model::{CommandExecution, CommandStatus, NextAction, SideEffect, VerificationStatus};
use crate::transaction::{rollback_stored_run, verify_stored_run};
use crate::util::{Result, resolve_root};

pub(super) fn apply_command(options: &CommandOptions, plan_id: &str) -> Result<CommandExecution> {
    let root = resolve_root(&options.cwd)?;
    let state_directory = resolve_state_directory(&root, options.state_directory.as_deref());
    let outcome = crate::transaction::apply_stored_plan(
        &root,
        &state_directory,
        plan_id,
        options.approval.as_deref(),
    )?;
    let failed = outcome.run.state != "succeeded";
    let next_actions = if failed {
        let mut argv = vec![
            "pkgshift".to_owned(),
            "rollback".to_owned(),
            outcome.run.run_id.clone(),
        ];
        argv.extend([
            "--state-dir".to_owned(),
            state_directory.to_string_lossy().into_owned(),
            "--approve".to_owned(),
            outcome.run.run_id.clone(),
        ]);
        vec![NextAction {
            argv,
            requires_approval: true,
            side_effect: SideEffect::RepositoryWrite,
        }]
    } else {
        Vec::new()
    };
    let mut artifacts = vec![artifact(
        outcome.run.run_id.clone(),
        "run-journal",
        "application/vnd.pkgshift.run+json",
        &outcome.run,
    )?];
    if let Some(report) = &outcome.verification {
        artifacts.push(artifact(
            report.report_id.clone(),
            "verification-report",
            "application/vnd.pkgshift.verification+json",
            report,
        )?);
    }
    Ok(CommandExecution {
        exit_code: if failed { 5 } else { 0 },
        result: result(
            "apply",
            if failed {
                CommandStatus::Failed
            } else {
                CommandStatus::Completed
            },
            summary([
                ("runStatus", json!(outcome.run.state)),
                (
                    "dependencyStateCleanups",
                    json!(outcome.run.dependency_state_cleanups.len()),
                ),
                ("processes", json!(outcome.run.processes.len())),
                ("rollbackAvailable", json!(true)),
            ]),
            outcome.run.diagnostics.clone(),
            artifacts,
            Some(plan_id.to_owned()),
            Some(outcome.run.run_id),
            next_actions,
        ),
    })
}

pub(super) fn verify_command(options: &CommandOptions, run_id: &str) -> Result<CommandExecution> {
    let root = resolve_root(&options.cwd)?;
    let state_directory = resolve_state_directory(&root, options.state_directory.as_deref());
    let report = verify_stored_run(&root, &state_directory, run_id)?;
    let failed = report.status == VerificationStatus::Failed;
    let next_actions = if failed {
        let mut argv = vec![
            "pkgshift".to_owned(),
            "rollback".to_owned(),
            run_id.to_owned(),
        ];
        argv.extend([
            "--state-dir".to_owned(),
            state_directory.to_string_lossy().into_owned(),
            "--approve".to_owned(),
            run_id.to_owned(),
        ]);
        vec![NextAction {
            argv,
            requires_approval: true,
            side_effect: SideEffect::RepositoryWrite,
        }]
    } else {
        Vec::new()
    };
    Ok(CommandExecution {
        exit_code: if failed { 6 } else { 0 },
        result: result(
            "verify",
            if failed {
                CommandStatus::Failed
            } else {
                CommandStatus::Completed
            },
            summary([
                ("checks", json!(report.checks.len())),
                (
                    "passed",
                    json!(
                        report
                            .checks
                            .iter()
                            .filter(|check| check.status == VerificationStatus::Passed)
                            .count()
                    ),
                ),
                (
                    "failed",
                    json!(
                        report
                            .checks
                            .iter()
                            .filter(|check| check.status == VerificationStatus::Failed)
                            .count()
                    ),
                ),
                (
                    "skipped",
                    json!(
                        report
                            .checks
                            .iter()
                            .filter(|check| check.status == VerificationStatus::Skipped)
                            .count()
                    ),
                ),
            ]),
            report.diagnostics.clone(),
            vec![artifact(
                report.report_id.clone(),
                "verification-report",
                "application/vnd.pkgshift.verification+json",
                &report,
            )?],
            Some(report.plan_id.clone()),
            Some(run_id.to_owned()),
            next_actions,
        ),
    })
}

pub(super) fn rollback_command(options: &CommandOptions, run_id: &str) -> Result<CommandExecution> {
    let root = resolve_root(&options.cwd)?;
    let state_directory = resolve_state_directory(&root, options.state_directory.as_deref());
    let run = rollback_stored_run(&root, &state_directory, run_id, options.approval.as_deref())?;
    let failed = run.state == "rollback-failed";
    Ok(CommandExecution {
        exit_code: if failed { 5 } else { 0 },
        result: result(
            "rollback",
            if failed {
                CommandStatus::Failed
            } else {
                CommandStatus::RolledBack
            },
            summary([
                ("runStatus", json!(run.state)),
                ("repositoryFilesRestored", json!(!failed)),
                ("externalDependencyStateRestored", json!(false)),
            ]),
            run.diagnostics.clone(),
            vec![artifact(
                run.run_id.clone(),
                "run-journal",
                "application/vnd.pkgshift.run+json",
                &run,
            )?],
            Some(run.plan.plan_id.clone()),
            Some(run.run_id.clone()),
            Vec::new(),
        ),
    })
}
