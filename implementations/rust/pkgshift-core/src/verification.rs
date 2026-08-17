use std::path::Path;

use crate::catalog::get_package_manager;
use crate::cleanup;
use crate::inspect::{build_project_ir, inspect_project};
use crate::lock_graph::{compare_lock_graphs_for_project, extract_lock_graph};
use crate::model::{
    DependencyStateCleanupRecord, Diagnostic, DiagnosticSeverity, LockGraph, MigrationPlan,
    ProjectIr, SCHEMA_VERSION, VerificationCheck, VerificationReport, VerificationStatus,
};
use crate::util::{Result, file_digest, safe_join, short_digest};

fn failure_diagnostic(failed: usize) -> Diagnostic {
    Diagnostic {
        code: "VERIFICATION_FAILED".to_owned(),
        severity: DiagnosticSeverity::Error,
        summary: format!("{failed} blocking verification checks failed."),
        blocking: true,
        evidence: Vec::new(),
        remediation: vec!["Repair the repository or roll back the run.".to_owned()],
    }
}

#[allow(clippy::too_many_lines)]
pub(crate) fn verify(
    root: &Path,
    plan: &MigrationPlan,
    source_lock_graph: Option<&LockGraph>,
    project_ir: &ProjectIr,
    run_id: &str,
    install_succeeded: bool,
    dependency_state_cleanups: &[DependencyStateCleanupRecord],
) -> Result<VerificationReport> {
    let mut checks = Vec::new();
    let mut mismatches = Vec::new();
    for mutation in plan
        .operations
        .iter()
        .flat_map(|operation| operation.mutations.iter())
    {
        if file_digest(&safe_join(root, &mutation.path)?)? != mutation.after_digest {
            mismatches.push(mutation.path.clone());
        }
    }
    checks.push(VerificationCheck {
        id: "planned-file-digests".to_owned(),
        status: if mismatches.is_empty() {
            VerificationStatus::Passed
        } else {
            VerificationStatus::Failed
        },
        summary: if mismatches.is_empty() {
            "Every planned file mutation matches its post-apply digest.".to_owned()
        } else {
            format!("{} planned file mutations do not match.", mismatches.len())
        },
        evidence: mismatches,
    });

    checks.push(cleanup::clean_install_check(
        plan,
        dependency_state_cleanups,
    ));

    let inspection = inspect_project(root)?;
    let target_selected = inspection.package_manager.selected == Some(plan.target);
    checks.push(VerificationCheck {
        id: "target-selection".to_owned(),
        status: if target_selected {
            VerificationStatus::Passed
        } else {
            VerificationStatus::Failed
        },
        summary: if target_selected {
            format!("{} is the selected package manager.", plan.target)
        } else {
            format!(
                "Expected {}, detected {}.",
                plan.target,
                inspection
                    .package_manager
                    .selected
                    .map_or_else(|| "none".to_owned(), |value| value.to_string())
            )
        },
        evidence: inspection
            .package_manager
            .candidates
            .iter()
            .map(|candidate| format!("{}:{}", candidate.manager, candidate.score))
            .collect(),
    });

    let existing_locks = get_package_manager(plan.target)
        .lockfiles
        .iter()
        .filter(|path| root.join(path).is_file())
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    let empty_target_lock_omitted = existing_locks.is_empty()
        && source_lock_graph.is_some_and(|graph| graph.complete && graph.nodes.is_empty());
    checks.push(VerificationCheck {
        id: "target-lockfile".to_owned(),
        status: if !existing_locks.is_empty() || empty_target_lock_omitted {
            VerificationStatus::Passed
        } else {
            VerificationStatus::Failed
        },
        summary: if empty_target_lock_omitted {
            format!(
                "{} omitted an empty lockfile for an empty resolved package set.",
                plan.target
            )
        } else if existing_locks.is_empty() {
            format!("No {} lockfile was generated.", plan.target)
        } else {
            format!(
                "Target dependency state exists: {}.",
                existing_locks.join(", ")
            )
        },
        evidence: if empty_target_lock_omitted {
            vec![
                "sourceResolutions:0".to_owned(),
                "targetLockfile:absent".to_owned(),
            ]
        } else {
            existing_locks
        },
    });

    checks.push(cleanup::source_artifact_check(root, plan));

    let current_packages: Vec<String> = build_project_ir(&inspection)?
        .map(|ir| {
            ir.packages
                .into_iter()
                .map(|package| package.path)
                .collect()
        })
        .unwrap_or_default();
    let expected_packages = project_ir
        .packages
        .iter()
        .map(|package| package.path.clone())
        .collect::<Vec<_>>();
    let workspace_matches = current_packages == expected_packages;
    checks.push(VerificationCheck {
        id: "workspace-membership".to_owned(),
        status: if workspace_matches {
            VerificationStatus::Passed
        } else {
            VerificationStatus::Failed
        },
        summary: if workspace_matches {
            "Workspace package membership is preserved.".to_owned()
        } else {
            "Workspace package membership changed.".to_owned()
        },
        evidence: vec![
            format!("expected:{}", expected_packages.join(",")),
            format!("actual:{}", current_packages.join(",")),
        ],
    });

    checks.push(VerificationCheck {
        id: "target-install".to_owned(),
        status: if install_succeeded {
            VerificationStatus::Passed
        } else {
            VerificationStatus::Failed
        },
        summary: if install_succeeded {
            "The target installation operation completed successfully.".to_owned()
        } else {
            "The target installation operation is not successful.".to_owned()
        },
        evidence: vec![format!("success:{install_succeeded}")],
    });

    let lock_graph_comparison = if let Some(source_graph) = source_lock_graph {
        match extract_lock_graph(root, plan.target)? {
            Some(target_graph) => {
                let comparison =
                    compare_lock_graphs_for_project(source_graph, &target_graph, project_ir)?;
                let passed = comparison.status == VerificationStatus::Passed;
                let mut evidence = vec![
                    format!("policy:{}", comparison.policy),
                    format!("sourceResolutions:{}", comparison.source_resolutions),
                    format!("targetResolutions:{}", comparison.target_resolutions),
                    format!("added:{}", comparison.added_resolutions.len()),
                    format!("removed:{}", comparison.removed_resolutions.len()),
                    format!(
                        "integrityMismatches:{}",
                        comparison.integrity_mismatches.len()
                    ),
                    format!("edgeChanges:{}", comparison.edge_changes.len()),
                    format!(
                        "prunedSource:{}",
                        comparison.pruned_source_resolutions.len()
                    ),
                    format!(
                        "prunedTarget:{}",
                        comparison.pruned_target_resolutions.len()
                    ),
                    format!(
                        "optionalPlatformDifferences:{}",
                        comparison.optional_platform_differences.len()
                    ),
                    format!(
                        "reachabilityIssues:{}",
                        comparison.reachability_issues.len()
                    ),
                ];
                evidence.extend(
                    target_graph
                        .diagnostics
                        .iter()
                        .map(|entry| format!("{}:{}", entry.code, entry.summary)),
                );
                checks.push(VerificationCheck {
                    id: "dependency-graph-drift".to_owned(),
                    status: comparison.status,
                    summary: if passed {
                        format!(
                            "Source and target resolved package sets match under {}.",
                            comparison.policy
                        )
                    } else {
                        "Target dependency state drifted from the accepted source lock graph."
                            .to_owned()
                    },
                    evidence,
                });
                Some(comparison)
            }
            None => {
                if source_graph.complete && source_graph.nodes.is_empty() {
                    let absent_target = LockGraph {
                        schema_version: SCHEMA_VERSION.to_owned(),
                        graph_id: format!("lockgraph_absent_{}", plan.target),
                        manager: plan.target,
                        lockfile_path: String::new(),
                        lockfile_digest: "absent".to_owned(),
                        format: "absent-empty".to_owned(),
                        complete: true,
                        nodes: Vec::new(),
                        edges: Vec::new(),
                        diagnostics: Vec::new(),
                    };
                    let mut comparison =
                        compare_lock_graphs_for_project(source_graph, &absent_target, project_ir)?;
                    comparison.target_graph_id = None;
                    checks.push(VerificationCheck {
                        id: "dependency-graph-drift".to_owned(),
                        status: comparison.status,
                        summary:
                            "The target omitted an empty lockfile and preserved the empty resolution set."
                                .to_owned(),
                        evidence: vec![
                            format!("policy:{}", comparison.policy),
                            "sourceResolutions:0".to_owned(),
                            "targetResolutions:0".to_owned(),
                            "targetLockfile:absent".to_owned(),
                        ],
                    });
                    Some(comparison)
                } else {
                    checks.push(VerificationCheck {
                        id: "dependency-graph-drift".to_owned(),
                        status: VerificationStatus::Failed,
                        summary: "No target lock graph was available for comparison.".to_owned(),
                        evidence: vec![format!("target:{}", plan.target)],
                    });
                    None
                }
            }
        }
    } else {
        checks.push(VerificationCheck {
            id: "dependency-graph-drift".to_owned(),
            status: VerificationStatus::Skipped,
            summary: "No source lockfile existed, so resolved graph comparison is not applicable."
                .to_owned(),
            evidence: vec!["sourceLockGraph:none".to_owned()],
        });
        None
    };
    let failed = checks
        .iter()
        .filter(|check| check.status == VerificationStatus::Failed)
        .count();
    let diagnostics = if failed == 0 {
        Vec::new()
    } else {
        vec![failure_diagnostic(failed)]
    };
    let status = if failed == 0 {
        VerificationStatus::Passed
    } else {
        VerificationStatus::Failed
    };
    let report_id = short_digest(
        "verification_",
        &(
            run_id,
            &plan.plan_id,
            status,
            &checks,
            &diagnostics,
            &lock_graph_comparison,
        ),
    )?;
    Ok(VerificationReport {
        schema_version: SCHEMA_VERSION.to_owned(),
        report_id,
        run_id: run_id.to_owned(),
        plan_id: plan.plan_id.clone(),
        status,
        checks,
        diagnostics,
        lock_graph_comparison,
    })
}
