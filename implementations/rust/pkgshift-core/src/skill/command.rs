use std::collections::BTreeMap;
use std::path::Path;
use std::str::FromStr;

use serde_json::{Value, json};

use super::inspect::inspect_skill;
use super::model::{SkillClient, SkillInstallMode, SkillOperation, SkillScope, SkillStatus};
use super::source::resolve_source;
use super::transaction::{SkillMutationOutcome, install_skill, uninstall_skill};
use crate::command::{CommandOptions, artifact, result, summary};
use crate::model::{CommandExecution, CommandStatus, Diagnostic, NextAction, SideEffect};
use crate::util::{Result, resolve_root, short_digest};

fn invalid_input(command: &str, summary_text: &str) -> CommandExecution {
    CommandExecution {
        exit_code: 2,
        result: result(
            command,
            CommandStatus::Blocked,
            summary([("validInput", json!(false))]),
            vec![Diagnostic::blocking(
                "CLI_INVALID_INPUT",
                summary_text,
                vec!["Retry with a documented skill option value.".to_owned()],
            )],
            Vec::new(),
            None,
            None,
            Vec::new(),
        ),
    }
}

fn status_artifact(status: &SkillStatus) -> Result<crate::model::ResultArtifact> {
    artifact(
        format!("skill_{}_{}_{}", status.name, status.scope, status.client),
        "skill-status",
        "application/vnd.pkgshift.skill-status+json",
        status,
    )
}

fn status_summary(status: &SkillStatus, mutation_performed: bool) -> BTreeMap<String, Value> {
    let value = json!({
        "scope": status.scope,
        "client": status.client,
        "installed": status.installed,
        "healthy": status.healthy,
        "modified": status.modified,
        "mode": status.mode,
        "sourcePath": status.source_path,
        "targetPath": status.target_path,
        "mutationPerformed": mutation_performed,
    });
    value
        .as_object()
        .expect("skill summary is an object")
        .iter()
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect()
}

fn approval_action(
    operation: SkillOperation,
    scope: SkillScope,
    client: SkillClient,
    mode: SkillInstallMode,
    token: &str,
) -> NextAction {
    let mut argv = vec![
        "pkgshift".to_owned(),
        "skill".to_owned(),
        operation.to_string(),
        "--scope".to_owned(),
        scope.to_string(),
        "--client".to_owned(),
        client.to_string(),
    ];
    if operation == SkillOperation::Install {
        argv.extend(["--mode".to_owned(), mode.to_string()]);
    }
    argv.extend([
        "--approve".to_owned(),
        token.to_owned(),
        "--json".to_owned(),
        "--no-color".to_owned(),
        "--non-interactive".to_owned(),
    ]);
    NextAction {
        argv,
        requires_approval: true,
        side_effect: SideEffect::FilesystemWrite,
    }
}

fn skill_plan_id(
    operation: SkillOperation,
    scope: SkillScope,
    client: SkillClient,
    mode: SkillInstallMode,
    status: &SkillStatus,
) -> Result<String> {
    short_digest(
        "skill_plan_",
        &(
            operation.to_string(),
            scope,
            client,
            mode,
            &status.source_path,
            &status.target_path,
            &status.source_digest,
            status.installed,
            status.mode,
            &status.installed_digest,
            status.modified,
        ),
    )
}

fn read_only_result(command: &str, status: &SkillStatus) -> Result<CommandExecution> {
    let blocked = status.diagnostics.iter().any(|entry| entry.blocking);
    Ok(CommandExecution {
        exit_code: if blocked { 3 } else { 0 },
        result: result(
            command,
            if blocked {
                CommandStatus::Blocked
            } else {
                CommandStatus::Completed
            },
            status_summary(status, false),
            status.diagnostics.clone(),
            vec![status_artifact(status)?],
            None,
            None,
            Vec::new(),
        ),
    })
}

fn approved_result(
    command: &str,
    outcome: &SkillMutationOutcome,
    plan_id: &str,
) -> Result<CommandExecution> {
    Ok(CommandExecution {
        exit_code: 0,
        result: result(
            command,
            CommandStatus::Completed,
            status_summary(&outcome.status, outcome.mutation_performed),
            outcome.status.diagnostics.clone(),
            vec![status_artifact(&outcome.status)?],
            Some(plan_id.to_owned()),
            None,
            Vec::new(),
        ),
    })
}

fn mutation_failure(
    command: &str,
    project_root: &Path,
    source_path: &Path,
    scope: SkillScope,
    client: SkillClient,
    diagnostic: &Diagnostic,
    plan_id: &str,
) -> Result<CommandExecution> {
    let mut status = inspect_skill(project_root, Some(source_path), scope, client, None);
    if !status
        .diagnostics
        .iter()
        .any(|entry| entry.code == diagnostic.code)
    {
        status.diagnostics.push(diagnostic.clone());
    }
    let internal = diagnostic.code == "SKILL_OPERATION_FAILED";
    Ok(CommandExecution {
        exit_code: if internal { 8 } else { 3 },
        result: result(
            command,
            if internal {
                CommandStatus::Failed
            } else {
                CommandStatus::Blocked
            },
            status_summary(&status, false),
            status.diagnostics.clone(),
            vec![status_artifact(&status)?],
            Some(plan_id.to_owned()),
            None,
            Vec::new(),
        ),
    })
}

pub(crate) fn skill_command(
    options: &CommandOptions,
    operation: SkillOperation,
    scope_value: &str,
    client_value: &str,
    mode_value: &str,
) -> Result<CommandExecution> {
    let command = format!("skill {operation}");
    let Ok(scope) = SkillScope::from_str(scope_value) else {
        return Ok(invalid_input(&command, "--scope must be project or user."));
    };
    let Ok(client) = SkillClient::from_str(client_value) else {
        return Ok(invalid_input(&command, "--client must be codex or claude."));
    };
    let Ok(mode) = SkillInstallMode::from_str(mode_value) else {
        return Ok(invalid_input(&command, "--mode must be copy or link."));
    };
    let root = resolve_root(&options.cwd)?;
    let source_path = resolve_source(&root);
    let status = inspect_skill(&root, Some(&source_path), scope, client, None);
    if matches!(operation, SkillOperation::Status | SkillOperation::Doctor) {
        return read_only_result(&command, &status);
    }
    if options.trial {
        return Ok(invalid_input(
            &command,
            "--trial is not supported for Agent Skill lifecycle operations.",
        ));
    }
    if status.diagnostics.iter().any(|entry| entry.blocking) {
        return read_only_result(&command, &status);
    }
    let plan_id = skill_plan_id(operation, scope, client, mode, &status)?;
    if options.dry_run {
        return Ok(CommandExecution {
            exit_code: 0,
            result: result(
                command,
                CommandStatus::Planned,
                status_summary(&status, false),
                status.diagnostics.clone(),
                vec![status_artifact(&status)?],
                Some(plan_id),
                None,
                Vec::new(),
            ),
        });
    }
    if options.approval.as_deref() != Some(plan_id.as_str()) {
        let mut diagnostics = status.diagnostics.clone();
        diagnostics.push(Diagnostic::blocking(
            "APPROVAL_REQUIRED",
            format!("Skill {operation} requires exact approval for {plan_id}."),
            vec![format!("Retry with --approve {plan_id}.")],
        ));
        return Ok(CommandExecution {
            exit_code: 7,
            result: result(
                command,
                CommandStatus::Planned,
                status_summary(&status, false),
                diagnostics,
                vec![status_artifact(&status)?],
                Some(plan_id.clone()),
                None,
                vec![approval_action(operation, scope, client, mode, &plan_id)],
            ),
        });
    }
    let outcome = match operation {
        SkillOperation::Install => install_skill(&root, &source_path, scope, client, mode, None),
        SkillOperation::Uninstall => uninstall_skill(&root, &source_path, scope, client, None),
        SkillOperation::Status | SkillOperation::Doctor => unreachable!(),
    };
    match outcome {
        Ok(outcome) => approved_result(&command, &outcome, &plan_id),
        Err(diagnostic) => mutation_failure(
            &command,
            &root,
            &source_path,
            scope,
            client,
            &diagnostic,
            &plan_id,
        ),
    }
}
