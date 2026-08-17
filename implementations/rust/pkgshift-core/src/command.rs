use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::{Value, json};

use crate::catalog::{PACKAGE_MANAGERS, normalize_package_manager_id};
use crate::inspect::{build_project_ir, inspect_project};
use crate::lock_graph::extract_lock_graph;
use crate::model::{
    CapabilityAnalysis, CommandExecution, CommandResult, CommandStatus, Diagnostic,
    DiagnosticSeverity, LockGraph, MigrationPlan, NextAction, PackageManagerId, ProjectInspection,
    ProjectIr, ResultArtifact, SCHEMA_VERSION, SideEffect, StoredPlan, VerificationStatus,
};
use crate::plan::{analyze_capabilities, plan_package_manager_migration};
use crate::transaction::{apply_stored_plan, save_plan, trial_stored_plan};
use crate::util::{PkgshiftError, Result, resolve_root};

mod comparison;
mod lifecycle;

use lifecycle::{apply_command, rollback_command, verify_command};

#[derive(Debug, Clone)]
pub enum CommandKind {
    Inspect,
    Compare {
        targets: Vec<String>,
    },
    Plan {
        target: String,
    },
    To {
        target: String,
    },
    Apply {
        plan_id: String,
    },
    Verify {
        run_id: String,
    },
    Rollback {
        run_id: String,
    },
    RuntimeTo {
        target: String,
        permissions: Vec<String>,
    },
    RuntimeRollback {
        run_id: String,
    },
    Support,
}

#[derive(Debug, Clone)]
pub struct CommandOptions {
    pub command: CommandKind,
    pub cwd: PathBuf,
    pub state_directory: Option<PathBuf>,
    pub accept_lossy: bool,
    pub approval: Option<String>,
    pub dry_run: bool,
    pub trial: bool,
    pub verification_scripts: Vec<String>,
}

impl CommandOptions {
    pub fn new(command: CommandKind, cwd: PathBuf) -> Self {
        Self {
            command,
            cwd,
            state_directory: None,
            accept_lossy: false,
            approval: None,
            dry_run: false,
            trial: false,
            verification_scripts: Vec::new(),
        }
    }
}

pub(crate) fn artifact<T: Serialize>(
    id: String,
    artifact_type: &str,
    media_type: &str,
    value: &T,
) -> Result<ResultArtifact> {
    let content = serde_json::to_value(value).map_err(|source| PkgshiftError::Json {
        path: PathBuf::from("<memory>"),
        source,
    })?;
    Ok(ResultArtifact {
        id,
        r#type: artifact_type.to_owned(),
        media_type: media_type.to_owned(),
        content,
    })
}

pub(crate) fn result(
    command: impl Into<String>,
    status: CommandStatus,
    summary: BTreeMap<String, Value>,
    diagnostics: Vec<Diagnostic>,
    artifacts: Vec<ResultArtifact>,
    plan_id: Option<String>,
    run_id: Option<String>,
    next_actions: Vec<NextAction>,
) -> CommandResult {
    CommandResult {
        schema_version: SCHEMA_VERSION.to_owned(),
        command: command.into(),
        status,
        plan_id,
        run_id,
        summary,
        artifacts,
        diagnostics,
        next_actions,
    }
}

pub(crate) fn summary(
    entries: impl IntoIterator<Item = (&'static str, Value)>,
) -> BTreeMap<String, Value> {
    entries
        .into_iter()
        .map(|(key, value)| (key.to_owned(), value))
        .collect()
}

fn failure(command: &str, error: &PkgshiftError) -> CommandExecution {
    let message = error.to_string();
    let approval_required = message.contains("requires exact approval");
    let precondition_failed = message.contains("changed after planning");
    let diagnostic = Diagnostic {
        code: if approval_required {
            "APPROVAL_REQUIRED"
        } else if precondition_failed {
            "PLAN_PRECONDITION_FAILED"
        } else {
            "PKGSHIFT_INTERNAL_ERROR"
        }
        .to_owned(),
        severity: DiagnosticSeverity::Error,
        summary: message,
        blocking: true,
        evidence: Vec::new(),
        remediation: vec![if approval_required {
            "Retry with --approve followed by the exact plan or run identifier.".to_owned()
        } else {
            "Preserve the state directory and inspect the reported diagnostic.".to_owned()
        }],
    };
    CommandExecution {
        exit_code: if approval_required {
            7
        } else if precondition_failed {
            4
        } else {
            8
        },
        result: result(
            command,
            CommandStatus::Failed,
            summary([(
                "trustworthyResult",
                json!(!matches!(error, PkgshiftError::Json { .. })),
            )]),
            vec![diagnostic],
            Vec::new(),
            None,
            None,
            Vec::new(),
        ),
    }
}

fn normalized_target(
    command: &str,
    target: &str,
) -> std::result::Result<PackageManagerId, CommandExecution> {
    normalize_package_manager_id(target).ok_or_else(|| {
        let diagnostic = Diagnostic::blocking(
            "PM_TARGET_UNSUPPORTED",
            format!("Unsupported package manager target: {target}"),
            vec!["Run pkgshift support and use a listed adapter identifier.".to_owned()],
        );
        CommandExecution {
            exit_code: 3,
            result: result(
                command,
                CommandStatus::Unsupported,
                summary([("target", json!(target))]),
                vec![diagnostic],
                Vec::new(),
                None,
                None,
                Vec::new(),
            ),
        }
    })
}

fn inspection_artifacts(
    inspection: &ProjectInspection,
    project_ir: Option<&ProjectIr>,
) -> Result<Vec<ResultArtifact>> {
    let fingerprint_suffix = inspection
        .fingerprint
        .strip_prefix("sha256:")
        .unwrap_or(&inspection.fingerprint)
        .chars()
        .take(24)
        .collect::<String>();
    let mut artifacts = vec![artifact(
        format!("inspection_{fingerprint_suffix}"),
        "project-inspection",
        "application/vnd.pkgshift.inspection+json",
        inspection,
    )?];
    if let Some(project_ir) = project_ir {
        artifacts.push(artifact(
            project_ir.project_ir_id.clone(),
            "project-ir",
            "application/vnd.pkgshift.project-ir+json",
            project_ir,
        )?);
    }
    Ok(artifacts)
}

fn inspect_command(cwd: &Path) -> Result<CommandExecution> {
    let inspection = inspect_project(cwd)?;
    let project_ir = build_project_ir(&inspection)?;
    let diagnostics = project_ir.as_ref().map_or_else(
        || inspection.diagnostics.clone(),
        |ir| ir.diagnostics.clone(),
    );
    let blocked = diagnostics.iter().any(|entry| entry.blocking);
    let artifacts = inspection_artifacts(&inspection, project_ir.as_ref())?;
    Ok(CommandExecution {
        exit_code: if blocked { 3 } else { 0 },
        result: result(
            "inspect package-manager",
            if blocked {
                CommandStatus::Blocked
            } else {
                CommandStatus::Completed
            },
            summary([
                ("root", json!(inspection.root)),
                ("fingerprint", json!(inspection.fingerprint)),
                ("selected", json!(inspection.package_manager.selected)),
                (
                    "candidates",
                    json!(inspection.package_manager.candidates.len()),
                ),
                ("workspace", json!(inspection.workspace.configured)),
                ("integrations", json!(inspection.integrations.len())),
                (
                    "packages",
                    json!(project_ir.as_ref().map_or(0, |ir| ir.packages.len())),
                ),
                (
                    "features",
                    json!(project_ir.as_ref().map_or(0, |ir| ir.features.len())),
                ),
            ]),
            diagnostics,
            artifacts,
            None,
            None,
            Vec::new(),
        ),
    })
}

struct PlannedMigration {
    project_ir: ProjectIr,
    analysis: CapabilityAnalysis,
    source_lock_graph: Option<LockGraph>,
    plan: MigrationPlan,
}

fn create_plan(
    cwd: &Path,
    target: PackageManagerId,
    accept_lossy: bool,
    verification_scripts: &[String],
) -> Result<Option<PlannedMigration>> {
    let inspection = inspect_project(cwd)?;
    let Some(project_ir) = build_project_ir(&inspection)? else {
        return Ok(None);
    };
    let Some(analysis) = analyze_capabilities(&project_ir, target)? else {
        return Ok(None);
    };
    let source_lock_graph = project_ir
        .source
        .map(|source| extract_lock_graph(Path::new(&inspection.root), source))
        .transpose()?
        .flatten();
    let Some(plan) = plan_package_manager_migration(
        &inspection,
        &project_ir,
        &analysis,
        source_lock_graph.as_ref(),
        target,
        accept_lossy,
        verification_scripts,
    )?
    else {
        return Ok(None);
    };
    Ok(Some(PlannedMigration {
        project_ir,
        analysis,
        source_lock_graph,
        plan,
    }))
}

fn planned_artifacts(planned: &PlannedMigration) -> Result<Vec<ResultArtifact>> {
    let mut artifacts = vec![
        artifact(
            planned.project_ir.project_ir_id.clone(),
            "project-ir",
            "application/vnd.pkgshift.project-ir+json",
            &planned.project_ir,
        )?,
        artifact(
            planned.analysis.analysis_id.clone(),
            "capability-analysis",
            "application/vnd.pkgshift.capability-analysis+json",
            &planned.analysis,
        )?,
        artifact(
            planned.plan.plan_id.clone(),
            "package-manager-plan",
            "application/vnd.pkgshift.plan+json",
            &planned.plan,
        )?,
    ];
    if let Some(graph) = &planned.source_lock_graph {
        artifacts.push(artifact(
            graph.graph_id.clone(),
            "source-lock-graph",
            "application/vnd.pkgshift.lock-graph+json",
            graph,
        )?);
    }
    Ok(artifacts)
}

fn blocked_plan(cwd: &Path, command: &str, target: PackageManagerId) -> Result<CommandExecution> {
    let inspection = inspect_project(cwd)?;
    Ok(CommandExecution {
        exit_code: 3,
        result: result(
            command,
            CommandStatus::Blocked,
            summary([
                ("target", json!(target)),
                ("fingerprint", json!(inspection.fingerprint)),
            ]),
            inspection.diagnostics.clone(),
            inspection_artifacts(&inspection, None)?,
            None,
            None,
            Vec::new(),
        ),
    })
}

pub(crate) fn resolve_state_directory(root: &Path, value: Option<&Path>) -> PathBuf {
    value.map_or_else(
        || root.join(".pkgshift/state"),
        |path| {
            if path.is_absolute() {
                path.to_path_buf()
            } else {
                root.join(path)
            }
        },
    )
}

fn persist_planned(state_directory: &Path, planned: &PlannedMigration) -> Result<()> {
    save_plan(
        state_directory,
        &StoredPlan {
            schema_version: SCHEMA_VERSION.to_owned(),
            plan: planned.plan.clone(),
            project_ir: planned.project_ir.clone(),
            capability_analysis: planned.analysis.clone(),
            source_lock_graph: planned.source_lock_graph.clone(),
        },
    )?;
    Ok(())
}

fn plan_command(options: &CommandOptions, target_value: &str) -> Result<CommandExecution> {
    let target = match normalized_target("plan package-manager", target_value) {
        Ok(target) => target,
        Err(execution) => return Ok(execution),
    };
    let Some(planned) = create_plan(
        &options.cwd,
        target,
        options.accept_lossy,
        &options.verification_scripts,
    )?
    else {
        return blocked_plan(&options.cwd, "plan package-manager", target);
    };
    let blocked = planned.plan.diagnostics.iter().any(|entry| entry.blocking);
    let mut artifacts = planned_artifacts(&planned)?;
    let mut artifact_stored = false;
    if let Some(state_directory) = options.state_directory.as_deref() {
        let root = resolve_root(&options.cwd)?;
        let state_directory = resolve_state_directory(&root, Some(state_directory));
        persist_planned(&state_directory, &planned)?;
        artifacts.push(artifact(
            format!(
                "stored_{}",
                planned.plan.plan_id.trim_start_matches("plan_")
            ),
            "stored-artifact-reference",
            "application/vnd.pkgshift.artifact-reference+json",
            &json!({
                "planId": planned.plan.plan_id,
                "stateDirectory": state_directory,
            }),
        )?);
        artifact_stored = true;
    }
    let next_actions = if artifact_stored && planned.plan.executable {
        let state_directory = options
            .state_directory
            .as_deref()
            .expect("stored plans have a state directory");
        vec![NextAction {
            argv: vec![
                "pkgshift".to_owned(),
                "apply".to_owned(),
                planned.plan.plan_id.clone(),
                "--state-dir".to_owned(),
                state_directory.to_string_lossy().into_owned(),
                "--approve".to_owned(),
                planned.plan.plan_id.clone(),
                "--json".to_owned(),
                "--non-interactive".to_owned(),
            ],
            requires_approval: true,
            side_effect: SideEffect::RepositoryWrite,
        }]
    } else {
        Vec::new()
    };
    Ok(CommandExecution {
        exit_code: if blocked { 3 } else { 0 },
        result: result(
            "plan package-manager",
            if blocked {
                CommandStatus::Blocked
            } else {
                CommandStatus::Planned
            },
            summary([
                ("source", json!(planned.plan.source)),
                ("target", json!(planned.plan.target)),
                ("targetTier", json!(planned.plan.target_tier)),
                ("operations", json!(planned.plan.operations.len())),
                (
                    "warnings",
                    json!(
                        planned
                            .plan
                            .diagnostics
                            .iter()
                            .filter(|entry| entry.severity == DiagnosticSeverity::Warning)
                            .count()
                    ),
                ),
                ("capabilities", json!(planned.plan.capability_summary)),
                (
                    "sourceLockGraph",
                    json!(planned.plan.source_lock_graph_id.is_some()),
                ),
                (
                    "nativeImport",
                    json!(planned.plan.native_import.as_ref().map(|entry| &entry.id)),
                ),
                ("artifactStored", json!(artifact_stored)),
                ("executionAvailable", json!(planned.plan.executable)),
                ("verificationScripts", json!(options.verification_scripts)),
            ]),
            planned.plan.diagnostics.clone(),
            artifacts,
            Some(planned.plan.plan_id.clone()),
            None,
            next_actions,
        ),
    })
}

fn guided_command(options: &CommandOptions, target_value: &str) -> Result<CommandExecution> {
    let command_name = format!("to {target_value}");
    let target = match normalized_target(&command_name, target_value) {
        Ok(target) => target,
        Err(execution) => return Ok(execution),
    };
    let Some(planned) = create_plan(
        &options.cwd,
        target,
        options.accept_lossy,
        &options.verification_scripts,
    )?
    else {
        return blocked_plan(&options.cwd, &command_name, target);
    };
    if !planned.plan.executable {
        let mut execution = plan_command(options, target_value)?;
        execution.result.command = command_name;
        return Ok(execution);
    }
    let files = planned
        .plan
        .operations
        .iter()
        .flat_map(|operation| operation.mutations.iter().map(|mutation| &mutation.path))
        .collect::<BTreeSet<_>>()
        .len();
    let root = resolve_root(&options.cwd)?;
    let state_directory = resolve_state_directory(&root, options.state_directory.as_deref());
    let mut approved_argv = vec![
        "pkgshift".to_owned(),
        "to".to_owned(),
        planned.plan.target.to_string(),
    ];
    if options.accept_lossy {
        approved_argv.push("--accept-lossy".to_owned());
    }
    if options.trial {
        approved_argv.push("--trial".to_owned());
    }
    for script in &options.verification_scripts {
        approved_argv.push("--verify-script".to_owned());
        approved_argv.push(script.clone());
    }
    if !options.trial
        && let Some(value) = options.state_directory.as_deref()
    {
        approved_argv.push("--state-dir".to_owned());
        approved_argv.push(value.to_string_lossy().into_owned());
    }
    approved_argv.extend([
        "--approve".to_owned(),
        planned.plan.plan_id.clone(),
        "--json".to_owned(),
        "--no-color".to_owned(),
        "--non-interactive".to_owned(),
    ]);
    let next_action = NextAction {
        argv: approved_argv,
        requires_approval: true,
        side_effect: if options.trial {
            SideEffect::ProcessExecution
        } else {
            SideEffect::RepositoryWrite
        },
    };
    if options.dry_run || options.approval.as_deref() != Some(planned.plan.plan_id.as_str()) {
        let approval_missing = !options.dry_run;
        let mut diagnostics = planned.plan.diagnostics.clone();
        if approval_missing {
            let trial_flag = if options.trial { " --trial" } else { "" };
            diagnostics.push(Diagnostic::blocking(
                "APPROVAL_REQUIRED",
                format!(
                    "Guided migration requires exact approval for {}.",
                    planned.plan.plan_id
                ),
                vec![format!(
                    "Retry with pkgshift to {}{} --approve {}.",
                    planned.plan.target, trial_flag, planned.plan.plan_id
                )],
            ));
        }
        return Ok(CommandExecution {
            exit_code: if approval_missing { 7 } else { 0 },
            result: result(
                command_name,
                CommandStatus::Planned,
                summary([
                    ("source", json!(planned.plan.source)),
                    ("target", json!(planned.plan.target)),
                    ("guided", json!(true)),
                    ("trial", json!(options.trial)),
                    ("dryRun", json!(options.dry_run)),
                    ("files", json!(files)),
                    (
                        "sourceLockGraph",
                        json!(planned.plan.source_lock_graph_id.is_some()),
                    ),
                    (
                        "nativeImport",
                        json!(planned.plan.native_import.as_ref().map(|entry| &entry.id)),
                    ),
                    ("repositoryChanged", json!(false)),
                    ("verificationScripts", json!(options.verification_scripts)),
                ]),
                diagnostics,
                planned_artifacts(&planned)?,
                Some(planned.plan.plan_id.clone()),
                None,
                vec![next_action],
            ),
        });
    }

    if options.trial {
        let stored_plan = StoredPlan {
            schema_version: SCHEMA_VERSION.to_owned(),
            plan: planned.plan.clone(),
            project_ir: planned.project_ir.clone(),
            capability_analysis: planned.analysis.clone(),
            source_lock_graph: planned.source_lock_graph.clone(),
        };
        let report = trial_stored_plan(&root, &stored_plan, options.approval.as_deref())?;
        let failed = report.status == VerificationStatus::Failed;
        let exit_code = if !failed {
            0
        } else if report.verification.is_some() {
            6
        } else {
            5
        };
        let mut artifacts = planned_artifacts(&planned)?;
        artifacts.push(artifact(
            report.report_id.clone(),
            "trial-report",
            "application/vnd.pkgshift.trial+json",
            &report,
        )?);
        return Ok(CommandExecution {
            exit_code,
            result: result(
                command_name,
                if failed {
                    CommandStatus::Failed
                } else {
                    CommandStatus::Completed
                },
                summary([
                    ("source", json!(planned.plan.source)),
                    ("target", json!(planned.plan.target)),
                    ("guided", json!(true)),
                    ("trial", json!(true)),
                    ("files", json!(files)),
                    ("repositoryChanged", json!(!report.repository_unchanged)),
                    ("repositoryUnchanged", json!(report.repository_unchanged)),
                    (
                        "dependencyStateCleanups",
                        json!(report.dependency_state_cleanups.len()),
                    ),
                    ("processes", json!(report.processes.len())),
                ]),
                report.diagnostics.clone(),
                artifacts,
                Some(planned.plan.plan_id.clone()),
                None,
                Vec::new(),
            ),
        });
    }

    persist_planned(&state_directory, &planned)?;
    let outcome = apply_stored_plan(
        &root,
        &state_directory,
        &planned.plan.plan_id,
        options.approval.as_deref(),
    )?;
    let failed = outcome.run.state != "succeeded";
    let mut artifacts = planned_artifacts(&planned)?;
    artifacts.push(artifact(
        outcome.run.run_id.clone(),
        "run-journal",
        "application/vnd.pkgshift.run+json",
        &outcome.run,
    )?);
    if let Some(verification) = &outcome.verification {
        artifacts.push(artifact(
            verification.report_id.clone(),
            "verification-report",
            "application/vnd.pkgshift.verification+json",
            verification,
        )?);
    }
    let rollback = NextAction {
        argv: vec![
            "pkgshift".to_owned(),
            "rollback".to_owned(),
            outcome.run.run_id.clone(),
            "--state-dir".to_owned(),
            state_directory.to_string_lossy().into_owned(),
            "--approve".to_owned(),
            outcome.run.run_id.clone(),
        ],
        requires_approval: true,
        side_effect: SideEffect::RepositoryWrite,
    };
    Ok(CommandExecution {
        exit_code: if failed { 5 } else { 0 },
        result: result(
            command_name,
            if failed {
                CommandStatus::Failed
            } else {
                CommandStatus::Completed
            },
            summary([
                ("source", json!(planned.plan.source)),
                ("target", json!(planned.plan.target)),
                ("guided", json!(true)),
                ("files", json!(files)),
                ("operations", json!(planned.plan.operations.len())),
                (
                    "dependencyStateCleanups",
                    json!(outcome.run.dependency_state_cleanups.len()),
                ),
                ("runStatus", json!(outcome.run.state)),
            ]),
            outcome.run.diagnostics.clone(),
            artifacts,
            Some(planned.plan.plan_id.clone()),
            Some(outcome.run.run_id.clone()),
            if failed { vec![rollback] } else { Vec::new() },
        ),
    })
}

fn support_command() -> Result<CommandExecution> {
    let adapters = PACKAGE_MANAGERS
        .iter()
        .map(|definition| {
            json!({
                "id": definition.id,
                "displayName": definition.display_name,
                "tier": definition.tier,
                "aliases": definition.aliases,
                "lockfiles": definition.lockfiles,
                "configurationFiles": definition.configuration_files,
                "installCommand": definition.install_command,
                "packageManagerPin": definition.package_manager_pin,
            })
        })
        .collect::<Vec<_>>();
    Ok(CommandExecution {
        exit_code: 0,
        result: result(
            "support",
            CommandStatus::Completed,
            summary([
                ("adapters", json!(adapters.len())),
                (
                    "productionTargets",
                    json!(
                        PACKAGE_MANAGERS
                            .iter()
                            .filter(|entry| matches!(
                                entry.tier,
                                crate::model::SupportTier::ProductionTarget
                            ))
                            .count()
                    ),
                ),
                (
                    "previewTargets",
                    json!(
                        PACKAGE_MANAGERS
                            .iter()
                            .filter(|entry| matches!(
                                entry.tier,
                                crate::model::SupportTier::PreviewTarget
                            ))
                            .count()
                    ),
                ),
            ]),
            Vec::new(),
            vec![artifact(
                "package-manager-support".to_owned(),
                "package-manager-support",
                "application/vnd.pkgshift.support+json",
                &adapters,
            )?],
            None,
            None,
            Vec::new(),
        ),
    })
}

pub fn execute(options: &CommandOptions) -> CommandExecution {
    let command_name = match &options.command {
        CommandKind::Inspect => "inspect package-manager".to_owned(),
        CommandKind::Compare { .. } => "compare".to_owned(),
        CommandKind::Plan { .. } => "plan package-manager".to_owned(),
        CommandKind::To { target } => format!("to {target}"),
        CommandKind::Apply { .. } => "apply".to_owned(),
        CommandKind::Verify { .. } => "verify".to_owned(),
        CommandKind::Rollback { .. } => "rollback".to_owned(),
        CommandKind::RuntimeTo { target, .. } => format!("runtime to {target}"),
        CommandKind::RuntimeRollback { .. } => "runtime rollback".to_owned(),
        CommandKind::Support => "support".to_owned(),
    };
    let execution = match &options.command {
        CommandKind::Inspect => inspect_command(&options.cwd),
        CommandKind::Compare { targets } => comparison::comparison_command(options, targets),
        CommandKind::Plan { target } => plan_command(options, target),
        CommandKind::To { target } => guided_command(options, target),
        CommandKind::Apply { plan_id } => apply_command(options, plan_id),
        CommandKind::Verify { run_id } => verify_command(options, run_id),
        CommandKind::Rollback { run_id } => rollback_command(options, run_id),
        CommandKind::RuntimeTo {
            target,
            permissions,
        } => crate::runtime::to_command(options, target, permissions),
        CommandKind::RuntimeRollback { run_id } => {
            crate::runtime::rollback_command(options, run_id)
        }
        CommandKind::Support => support_command(),
    };
    execution.unwrap_or_else(|error| failure(&command_name, &error))
}
