use std::collections::BTreeSet;

use serde::Serialize;
use serde_json::json;

use super::{CommandOptions, PlannedMigration, artifact, create_plan, result, summary};
use crate::inspect::inspect_project;
use crate::model::{
    CapabilityAnalysis, CommandExecution, CommandStatus, Diagnostic, DiagnosticSeverity,
    EvidenceDetail, MigrationPlan, NextAction, PackageManagerId, SCHEMA_VERSION, SideEffect,
    StoredPlan, TrialReport, VerificationStatus,
};
use crate::transaction::trial_stored_plan;
use crate::util::{Result, resolve_root, short_digest};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ComparisonPlanCandidate {
    target: PackageManagerId,
    executable: bool,
    capability_analysis: CapabilityAnalysis,
    plan: MigrationPlan,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ComparisonPlanArtifact {
    schema_version: String,
    comparison_id: String,
    source: PackageManagerId,
    repository_fingerprint: String,
    candidates: Vec<ComparisonPlanCandidate>,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "kebab-case")]
enum CandidateStatus {
    Passed,
    Failed,
    Blocked,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ComparisonOutcome {
    target: PackageManagerId,
    plan_id: String,
    status: CandidateStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    trial: Option<TrialReport>,
    diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ComparisonReport {
    schema_version: String,
    report_id: String,
    comparison_id: String,
    source: PackageManagerId,
    repository_unchanged: bool,
    candidates: Vec<ComparisonOutcome>,
}

fn target_count_failure(targets: &[String]) -> CommandExecution {
    let diagnostic = Diagnostic::blocking(
        "COMPARISON_TARGET_COUNT_INVALID",
        "Multi-target comparison requires at least two distinct package manager targets.",
        vec!["Provide two or more targets, for example: pkgshift compare bun deno.".to_owned()],
    );
    CommandExecution {
        exit_code: 2,
        result: result(
            "compare",
            CommandStatus::Blocked,
            summary([("targets", json!(targets))]),
            vec![diagnostic],
            Vec::new(),
            None,
            None,
            Vec::new(),
        ),
    }
}

fn normalize_targets(
    targets: &[String],
) -> std::result::Result<Vec<PackageManagerId>, CommandExecution> {
    let mut normalized = BTreeSet::new();
    for target in targets {
        normalized.insert(super::normalized_target("compare", target)?);
    }
    if normalized.len() < 2 {
        return Err(target_count_failure(targets));
    }
    Ok(normalized.into_iter().collect())
}

fn comparison_id(planned: &[PlannedMigration]) -> Result<String> {
    short_digest(
        "plan_compare_",
        &(
            SCHEMA_VERSION,
            planned
                .iter()
                .map(|candidate| (&candidate.plan.target, &candidate.plan.plan_id))
                .collect::<Vec<_>>(),
        ),
    )
}

fn plan_artifact(comparison_id: &str, planned: &[PlannedMigration]) -> ComparisonPlanArtifact {
    let first = planned
        .first()
        .expect("comparison requires at least two candidates");
    ComparisonPlanArtifact {
        schema_version: SCHEMA_VERSION.to_owned(),
        comparison_id: comparison_id.to_owned(),
        source: first.plan.source,
        repository_fingerprint: first.plan.repository_fingerprint.clone(),
        candidates: planned
            .iter()
            .map(|candidate| ComparisonPlanCandidate {
                target: candidate.plan.target,
                executable: candidate.plan.executable,
                capability_analysis: candidate.analysis.clone(),
                plan: candidate.plan.clone(),
            })
            .collect(),
    }
}

fn next_action(
    comparison_id: &str,
    planned: &[PlannedMigration],
    options: &CommandOptions,
) -> NextAction {
    let mut argv = vec!["pkgshift".to_owned(), "compare".to_owned()];
    argv.extend(
        planned
            .iter()
            .map(|candidate| candidate.plan.target.to_string()),
    );
    if options.accept_lossy {
        argv.push("--accept-lossy".to_owned());
    }
    for script in &options.verification_scripts {
        argv.push("--verify-script".to_owned());
        argv.push(script.clone());
    }
    for platform in &options.verification_policy.target_platforms {
        argv.push("--target-platform".to_owned());
        argv.push(platform.to_string());
    }
    if options.verification_policy.edge_equivalence != crate::EdgeEquivalencePolicy::Compatible {
        argv.push("--edge-equivalence".to_owned());
        argv.push(options.verification_policy.edge_equivalence.to_string());
    }
    argv.extend([
        "--approve".to_owned(),
        comparison_id.to_owned(),
        "--json".to_owned(),
        "--no-color".to_owned(),
        "--non-interactive".to_owned(),
    ]);
    NextAction {
        argv,
        requires_approval: true,
        side_effect: SideEffect::ProcessExecution,
    }
}

fn blocked_candidate_notices(planned: &[PlannedMigration]) -> Vec<Diagnostic> {
    planned
        .iter()
        .filter(|candidate| !candidate.plan.executable)
        .map(|candidate| Diagnostic {
            code: "COMPARISON_CANDIDATE_BLOCKED".to_owned(),
            severity: DiagnosticSeverity::Warning,
            summary: format!(
                "{} is retained for comparison but cannot execute its isolated trial.",
                candidate.plan.target
            ),
            blocking: false,
            evidence: candidate
                .plan
                .diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.blocking)
                .map(|diagnostic| EvidenceDetail {
                    location: candidate.plan.target.to_string(),
                    detail: format!("{}: {}", diagnostic.code, diagnostic.summary),
                })
                .collect(),
            remediation: vec![
                "Review the candidate plan diagnostics; other executable targets may still run."
                    .to_owned(),
            ],
        })
        .collect()
}

fn preview(
    comparison_id: &str,
    planned: &[PlannedMigration],
    options: &CommandOptions,
) -> Result<CommandExecution> {
    let executable = planned
        .iter()
        .filter(|candidate| candidate.plan.executable)
        .count();
    let blocked = planned.len() - executable;
    let plan_artifact = plan_artifact(comparison_id, planned);
    let artifacts = vec![artifact(
        comparison_id.to_owned(),
        "target-comparison-plan",
        "application/vnd.pkgshift.target-comparison-plan+json",
        &plan_artifact,
    )?];
    if executable == 0 {
        let mut diagnostics = blocked_candidate_notices(planned);
        diagnostics.insert(
            0,
            Diagnostic::blocking(
                "COMPARISON_NO_EXECUTABLE_TARGETS",
                "No comparison target produced an executable migration plan.",
                vec!["Review each candidate plan's blocking diagnostics.".to_owned()],
            ),
        );
        return Ok(CommandExecution {
            exit_code: 3,
            result: result(
                "compare",
                CommandStatus::Blocked,
                summary([
                    ("source", json!(plan_artifact.source)),
                    ("targets", json!(plan_artifact.candidates.len())),
                    ("executableTargets", json!(0)),
                    ("blockedTargets", json!(blocked)),
                    ("repositoryChanged", json!(false)),
                ]),
                diagnostics,
                artifacts,
                Some(comparison_id.to_owned()),
                None,
                Vec::new(),
            ),
        });
    }

    let approval_missing = !options.dry_run;
    let mut diagnostics = blocked_candidate_notices(planned);
    if approval_missing {
        diagnostics.insert(
            0,
            Diagnostic::blocking(
                "APPROVAL_REQUIRED",
                format!("Isolated target comparison requires exact approval for {comparison_id}."),
                vec![
                    "Execute the returned nextActions[0].argv unchanged after approval.".to_owned(),
                ],
            ),
        );
    }
    Ok(CommandExecution {
        exit_code: if approval_missing { 7 } else { 0 },
        result: result(
            "compare",
            CommandStatus::Planned,
            summary([
                ("source", json!(plan_artifact.source)),
                (
                    "targets",
                    json!(
                        planned
                            .iter()
                            .map(|candidate| candidate.plan.target)
                            .collect::<Vec<_>>()
                    ),
                ),
                ("executableTargets", json!(executable)),
                ("blockedTargets", json!(blocked)),
                ("trial", json!(true)),
                ("dryRun", json!(options.dry_run)),
                ("repositoryChanged", json!(false)),
            ]),
            diagnostics,
            artifacts,
            Some(comparison_id.to_owned()),
            None,
            if approval_missing {
                vec![next_action(comparison_id, planned, options)]
            } else {
                Vec::new()
            },
        ),
    })
}

fn execute_trials(
    root: &std::path::Path,
    comparison_id: &str,
    planned: Vec<PlannedMigration>,
) -> Result<CommandExecution> {
    let accepted_plan = plan_artifact(comparison_id, &planned);
    let source = planned
        .first()
        .expect("comparison requires at least two candidates")
        .plan
        .source;
    let baseline = planned[0].plan.repository_fingerprint.clone();
    let mut outcomes = Vec::with_capacity(planned.len());
    for candidate in planned {
        if !candidate.plan.executable {
            outcomes.push(ComparisonOutcome {
                target: candidate.plan.target,
                plan_id: candidate.plan.plan_id.clone(),
                status: CandidateStatus::Blocked,
                trial: None,
                diagnostics: candidate.plan.diagnostics.clone(),
            });
            continue;
        }
        let stored = StoredPlan {
            schema_version: SCHEMA_VERSION.to_owned(),
            plan: candidate.plan.clone(),
            project_ir: candidate.project_ir,
            capability_analysis: candidate.analysis,
            source_lock_graph: candidate.source_lock_graph,
        };
        let trial = trial_stored_plan(root, &stored, Some(stored.plan.plan_id.as_str()))?;
        let passed = trial.status == VerificationStatus::Passed;
        outcomes.push(ComparisonOutcome {
            target: stored.plan.target,
            plan_id: stored.plan.plan_id.clone(),
            status: if passed {
                CandidateStatus::Passed
            } else {
                CandidateStatus::Failed
            },
            diagnostics: trial.diagnostics.clone(),
            trial: Some(trial),
        });
    }
    let current = inspect_project(root)?;
    let repository_unchanged = current.fingerprint == baseline;
    let report_id = short_digest(
        "comparison_",
        &(
            SCHEMA_VERSION,
            comparison_id,
            source,
            repository_unchanged,
            &outcomes,
        ),
    )?;
    let report = ComparisonReport {
        schema_version: SCHEMA_VERSION.to_owned(),
        report_id: report_id.clone(),
        comparison_id: comparison_id.to_owned(),
        source,
        repository_unchanged,
        candidates: outcomes,
    };
    let passed = report
        .candidates
        .iter()
        .filter(|candidate| matches!(candidate.status, CandidateStatus::Passed))
        .count();
    let failed = report
        .candidates
        .iter()
        .filter(|candidate| matches!(candidate.status, CandidateStatus::Failed))
        .count();
    let blocked = report
        .candidates
        .iter()
        .filter(|candidate| matches!(candidate.status, CandidateStatus::Blocked))
        .count();
    let diagnostics = if repository_unchanged {
        Vec::new()
    } else {
        vec![Diagnostic::blocking(
            "COMPARISON_REPOSITORY_CHANGED",
            "The source repository changed while isolated target comparisons were running.",
            vec!["Inspect concurrent repository activity before comparing again.".to_owned()],
        )]
    };
    Ok(CommandExecution {
        exit_code: if repository_unchanged { 0 } else { 5 },
        result: result(
            "compare",
            if repository_unchanged {
                CommandStatus::Completed
            } else {
                CommandStatus::Failed
            },
            summary([
                ("source", json!(source)),
                ("targets", json!(report.candidates.len())),
                ("passedTargets", json!(passed)),
                ("failedTargets", json!(failed)),
                ("blockedTargets", json!(blocked)),
                ("repositoryChanged", json!(!repository_unchanged)),
                ("repositoryUnchanged", json!(repository_unchanged)),
            ]),
            diagnostics,
            vec![
                artifact(
                    comparison_id.to_owned(),
                    "target-comparison-plan",
                    "application/vnd.pkgshift.target-comparison-plan+json",
                    &accepted_plan,
                )?,
                artifact(
                    report_id,
                    "target-comparison-report",
                    "application/vnd.pkgshift.target-comparison+json",
                    &report,
                )?,
            ],
            Some(comparison_id.to_owned()),
            None,
            Vec::new(),
        ),
    })
}

pub(super) fn comparison_command(
    options: &CommandOptions,
    target_values: &[String],
) -> Result<CommandExecution> {
    let targets = match normalize_targets(target_values) {
        Ok(targets) => targets,
        Err(execution) => return Ok(execution),
    };
    let mut planned = Vec::with_capacity(targets.len());
    for target in targets {
        let Some(candidate) = create_plan(
            &options.cwd,
            target,
            options.accept_lossy,
            &options.verification_scripts,
            &options.verification_policy,
        )?
        else {
            return super::blocked_plan(&options.cwd, "compare", target);
        };
        planned.push(candidate);
    }
    let comparison_id = comparison_id(&planned)?;
    if options.dry_run || options.approval.as_deref() != Some(comparison_id.as_str()) {
        return preview(&comparison_id, &planned, options);
    }
    let root = resolve_root(&options.cwd)?;
    execute_trials(&root, &comparison_id, planned)
}
