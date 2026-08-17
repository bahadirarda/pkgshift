use std::collections::BTreeSet;
use std::str::FromStr;

use serde_json::json;

use super::model::{DenoPermission, RuntimePlanArtifact, RuntimeRunArtifact};
use super::plan::create_runtime_plan;
use super::transaction::{apply_plan, rollback_run};
use crate::command::{CommandOptions, artifact, resolve_state_directory, result, summary};
use crate::model::{CommandExecution, CommandStatus, Diagnostic, NextAction, SideEffect};
use crate::util::{Result, resolve_root};

fn unsupported_target(target: &str) -> CommandExecution {
    CommandExecution {
        exit_code: 3,
        result: result(
            format!("runtime to {target}"),
            CommandStatus::Unsupported,
            summary([("target", json!(target))]),
            vec![Diagnostic::blocking(
                "RUNTIME_TARGET_UNSUPPORTED",
                format!("Unsupported runtime target: {target}"),
                vec!["Use the currently supported Bun-to-Deno runtime target.".to_owned()],
            )],
            Vec::new(),
            None,
            None,
            Vec::new(),
        ),
    }
}

fn normalized_permissions(
    values: &[String],
) -> std::result::Result<BTreeSet<DenoPermission>, CommandExecution> {
    let mut permissions = BTreeSet::new();
    for value in values {
        let Ok(permission) = DenoPermission::from_str(value) else {
            return Err(CommandExecution {
                exit_code: 2,
                result: result(
                    "runtime to deno",
                    CommandStatus::Unsupported,
                    summary([("permission", json!(value))]),
                    vec![Diagnostic::blocking(
                        "DENO_PERMISSION_UNSUPPORTED",
                        format!("Unsupported Deno permission: {value}"),
                        vec!["Use read, write, net, env, run, sys, ffi, or hrtime.".to_owned()],
                    )],
                    Vec::new(),
                    None,
                    None,
                    Vec::new(),
                ),
            });
        };
        permissions.insert(permission);
    }
    Ok(permissions)
}

pub(crate) fn to_command(
    options: &CommandOptions,
    target: &str,
    permission_values: &[String],
) -> Result<CommandExecution> {
    if !target.trim().eq_ignore_ascii_case("deno") {
        return Ok(unsupported_target(target));
    }
    if options.trial {
        return Ok(CommandExecution {
            exit_code: 2,
            result: result(
                "runtime to deno",
                CommandStatus::Unsupported,
                summary([("trial", json!(true))]),
                vec![Diagnostic::blocking(
                    "RUNTIME_TRIAL_UNSUPPORTED",
                    "Runtime recipes do not yet support the isolated --trial surface.",
                    vec![
                        "Use --dry-run to review the read-only plan before exact approval."
                            .to_owned(),
                    ],
                )],
                Vec::new(),
                None,
                None,
                Vec::new(),
            ),
        });
    }
    let permissions = match normalized_permissions(permission_values) {
        Ok(permissions) => permissions,
        Err(execution) => return Ok(execution),
    };
    let root = resolve_root(&options.cwd)?;
    let state_directory = resolve_state_directory(&root, options.state_directory.as_deref());
    let plan = create_runtime_plan(&root, &permissions)?;
    let plan_artifact = RuntimePlanArtifact::from(&plan);
    let artifacts = vec![artifact(
        plan.plan_id.clone(),
        "runtime-migration-plan",
        "application/vnd.pkgshift.runtime-plan+json",
        &plan_artifact,
    )?];
    let file_count = plan
        .operations
        .iter()
        .flat_map(|operation| &operation.mutations)
        .count();

    if !plan.executable {
        return Ok(CommandExecution {
            exit_code: 3,
            result: result(
                "runtime to deno",
                CommandStatus::Blocked,
                summary([
                    ("source", json!(plan.source)),
                    ("target", json!(plan.target)),
                    ("files", json!(file_count)),
                    ("recipes", json!(plan.recipes.len())),
                    ("permissions", json!(plan.permissions)),
                    ("repositoryChanged", json!(false)),
                ]),
                plan.diagnostics.clone(),
                artifacts,
                Some(plan.plan_id.clone()),
                None,
                Vec::new(),
            ),
        });
    }

    let mut approved_argv = vec![
        "pkgshift".to_owned(),
        "runtime".to_owned(),
        "to".to_owned(),
        "deno".to_owned(),
    ];
    for permission in &plan.permissions {
        approved_argv.push("--deno-permission".to_owned());
        approved_argv.push(permission.to_string());
    }
    if let Some(value) = options.state_directory.as_deref() {
        approved_argv.push("--state-dir".to_owned());
        approved_argv.push(value.to_string_lossy().into_owned());
    }
    approved_argv.extend([
        "--approve".to_owned(),
        plan.plan_id.clone(),
        "--json".to_owned(),
        "--no-color".to_owned(),
        "--non-interactive".to_owned(),
    ]);
    let next_action = NextAction {
        argv: approved_argv,
        requires_approval: true,
        side_effect: SideEffect::RepositoryWrite,
    };
    if options.dry_run || options.approval.as_deref() != Some(plan.plan_id.as_str()) {
        let approval_missing = !options.dry_run;
        let mut diagnostics = plan.diagnostics.clone();
        if approval_missing {
            diagnostics.push(Diagnostic::blocking(
                "APPROVAL_REQUIRED",
                format!(
                    "Runtime migration requires exact approval for {}.",
                    plan.plan_id
                ),
                vec!["Execute the returned nextActions[0].argv unchanged after review.".to_owned()],
            ));
        }
        return Ok(CommandExecution {
            exit_code: if approval_missing { 7 } else { 0 },
            result: result(
                "runtime to deno",
                CommandStatus::Planned,
                summary([
                    ("source", json!(plan.source)),
                    ("target", json!(plan.target)),
                    ("files", json!(file_count)),
                    ("recipes", json!(plan.recipes.len())),
                    ("permissions", json!(plan.permissions)),
                    ("dryRun", json!(options.dry_run)),
                    ("repositoryChanged", json!(false)),
                ]),
                diagnostics,
                artifacts,
                Some(plan.plan_id.clone()),
                None,
                if options.dry_run {
                    Vec::new()
                } else {
                    vec![next_action]
                },
            ),
        });
    }

    let outcome = apply_plan(&root, &state_directory, &plan, options.approval.as_deref())?;
    let failed = outcome.run.state != "succeeded";
    let mut result_artifacts = artifacts;
    result_artifacts.push(artifact(
        outcome.run.run_id.clone(),
        "runtime-run-journal",
        "application/vnd.pkgshift.runtime-run+json",
        &RuntimeRunArtifact::from(&outcome.run),
    )?);
    if let Some(verification) = &outcome.verification {
        result_artifacts.push(artifact(
            verification.report_id.clone(),
            "runtime-verification-report",
            "application/vnd.pkgshift.runtime-verification+json",
            verification,
        )?);
    }
    let rollback = NextAction {
        argv: vec![
            "pkgshift".to_owned(),
            "runtime".to_owned(),
            "rollback".to_owned(),
            outcome.run.run_id.clone(),
            "--state-dir".to_owned(),
            state_directory.to_string_lossy().into_owned(),
            "--approve".to_owned(),
            outcome.run.run_id.clone(),
            "--json".to_owned(),
            "--non-interactive".to_owned(),
        ],
        requires_approval: true,
        side_effect: SideEffect::RepositoryWrite,
    };
    Ok(CommandExecution {
        exit_code: if failed { 5 } else { 0 },
        result: result(
            "runtime to deno",
            if failed {
                CommandStatus::Failed
            } else {
                CommandStatus::Completed
            },
            summary([
                ("source", json!(plan.source)),
                ("target", json!(plan.target)),
                ("files", json!(file_count)),
                ("recipes", json!(plan.recipes.len())),
                ("permissions", json!(plan.permissions)),
                ("runStatus", json!(outcome.run.state)),
                ("rollbackAvailable", json!(true)),
            ]),
            outcome.run.diagnostics.clone(),
            result_artifacts,
            Some(plan.plan_id),
            Some(outcome.run.run_id),
            vec![rollback],
        ),
    })
}

pub(crate) fn rollback_command(options: &CommandOptions, run_id: &str) -> Result<CommandExecution> {
    let root = resolve_root(&options.cwd)?;
    let state_directory = resolve_state_directory(&root, options.state_directory.as_deref());
    let run = rollback_run(&root, &state_directory, run_id, options.approval.as_deref())?;
    let failed = run.state == "rollback-failed";
    Ok(CommandExecution {
        exit_code: if failed { 5 } else { 0 },
        result: result(
            "runtime rollback",
            if failed {
                CommandStatus::Failed
            } else {
                CommandStatus::RolledBack
            },
            summary([
                ("runStatus", json!(run.state)),
                ("repositoryFilesRestored", json!(!failed)),
            ]),
            run.diagnostics.clone(),
            vec![artifact(
                run.run_id.clone(),
                "runtime-run-journal",
                "application/vnd.pkgshift.runtime-run+json",
                &RuntimeRunArtifact::from(&run),
            )?],
            Some(run.plan.plan_id.clone()),
            Some(run.run_id.clone()),
            Vec::new(),
        ),
    })
}
