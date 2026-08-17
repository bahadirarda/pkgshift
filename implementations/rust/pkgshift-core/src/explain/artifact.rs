use std::fs;
use std::path::Path;

use serde::Serialize;

use crate::command::artifact;
use crate::model::{
    Diagnostic, ResultArtifact, SCHEMA_VERSION, StoredRun, VerificationReport, VerificationStatus,
};
use crate::runtime::{RuntimeRunArtifact, load_run_artifact};
use crate::transaction::{load_plan, load_run};
use crate::util::{PkgshiftError, Result, is_short_digest_id, read_json, short_digest};

const MAX_STORED_RUNS: usize = 4096;

pub(super) struct LoadedArtifact {
    pub kind: &'static str,
    pub plan_id: Option<String>,
    pub run_id: Option<String>,
    pub status: Option<String>,
    pub diagnostics: Vec<Diagnostic>,
    pub artifacts: Vec<ResultArtifact>,
}

fn artifact_value<T: Serialize>(
    id: &str,
    artifact_type: &str,
    media_type: &str,
    value: &T,
) -> Result<ResultArtifact> {
    artifact(id.to_owned(), artifact_type, media_type, value)
}

fn verification_status(status: VerificationStatus) -> String {
    match status {
        VerificationStatus::Passed => "passed",
        VerificationStatus::Failed => "failed",
        VerificationStatus::Skipped => "skipped",
    }
    .to_owned()
}

fn candidate_run_ids(directory: &Path, prefix: &str) -> Result<Vec<String>> {
    let metadata = match fs::symlink_metadata(directory) {
        Ok(metadata) => metadata,
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(source) => {
            return Err(PkgshiftError::Io {
                path: directory.to_path_buf(),
                source,
            });
        }
    };
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(PkgshiftError::InvalidState(format!(
            "stored artifact directory is unsafe: {}",
            directory.display()
        )));
    }
    let mut identifiers = Vec::new();
    let mut entries_seen = 0_usize;
    for entry in fs::read_dir(directory).map_err(|source| PkgshiftError::Io {
        path: directory.to_path_buf(),
        source,
    })? {
        entries_seen += 1;
        if entries_seen > MAX_STORED_RUNS {
            return Err(PkgshiftError::InvalidState(format!(
                "stored artifact scan exceeds {MAX_STORED_RUNS} run directories"
            )));
        }
        let entry = entry.map_err(|source| PkgshiftError::Io {
            path: directory.to_path_buf(),
            source,
        })?;
        let file_type = entry.file_type().map_err(|source| PkgshiftError::Io {
            path: entry.path(),
            source,
        })?;
        let Some(identifier) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if file_type.is_dir() && is_short_digest_id(&identifier, prefix) {
            identifiers.push(identifier);
        }
    }
    identifiers.sort();
    Ok(identifiers)
}

fn verification_path(state_directory: &Path, run_id: &str) -> std::path::PathBuf {
    state_directory
        .join("runs")
        .join(run_id)
        .join("verification.json")
}

fn load_verification(
    state_directory: &Path,
    run: &StoredRun,
) -> Result<Option<VerificationReport>> {
    let path = verification_path(state_directory, &run.run_id);
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(source) => return Err(PkgshiftError::Io { path, source }),
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(PkgshiftError::InvalidState(format!(
            "verification artifact is not a regular file: {}",
            path.display()
        )));
    }
    let report: VerificationReport = read_json(&path)?;
    let expected = short_digest(
        "verification_",
        &(
            &report.run_id,
            &report.plan_id,
            report.status,
            &report.checks,
            &report.diagnostics,
            &report.lock_graph_comparison,
        ),
    )?;
    if report.schema_version != SCHEMA_VERSION
        || report.run_id != run.run_id
        || report.plan_id != run.plan.plan_id
        || report.report_id != expected
    {
        return Err(PkgshiftError::InvalidState(format!(
            "verification artifact failed identity validation: {}",
            path.display()
        )));
    }
    Ok(Some(report))
}

fn package_plan(state_directory: &Path, identifier: &str) -> Result<LoadedArtifact> {
    let stored = load_plan(state_directory, identifier)?;
    Ok(LoadedArtifact {
        kind: "package-manager-plan-bundle",
        plan_id: Some(identifier.to_owned()),
        run_id: None,
        status: Some(if stored.plan.executable {
            "executable".to_owned()
        } else {
            "blocked".to_owned()
        }),
        diagnostics: stored.plan.diagnostics.clone(),
        artifacts: vec![artifact_value(
            identifier,
            "package-manager-plan-bundle",
            "application/vnd.pkgshift.plan-bundle+json",
            &stored,
        )?],
    })
}

fn package_run(state_directory: &Path, identifier: &str) -> Result<LoadedArtifact> {
    let run = load_run(state_directory, identifier)?;
    let verification = load_verification(state_directory, &run)?;
    let mut artifacts = vec![artifact_value(
        identifier,
        "run-journal",
        "application/vnd.pkgshift.run+json",
        &run,
    )?];
    if let Some(report) = &verification {
        artifacts.push(artifact_value(
            &report.report_id,
            "verification-report",
            "application/vnd.pkgshift.verification+json",
            report,
        )?);
    }
    Ok(LoadedArtifact {
        kind: "run-journal",
        plan_id: Some(run.plan.plan_id.clone()),
        run_id: Some(run.run_id.clone()),
        status: Some(run.state.clone()),
        diagnostics: run.diagnostics.clone(),
        artifacts,
    })
}

fn package_verification(
    state_directory: &Path,
    identifier: &str,
) -> Result<Option<LoadedArtifact>> {
    for run_id in candidate_run_ids(&state_directory.join("runs"), "run_")? {
        let Ok(run) = load_run(state_directory, &run_id) else {
            continue;
        };
        let Some(report) = load_verification(state_directory, &run)? else {
            continue;
        };
        if report.report_id == identifier {
            return Ok(Some(LoadedArtifact {
                kind: "verification-report",
                plan_id: Some(report.plan_id.clone()),
                run_id: Some(report.run_id.clone()),
                status: Some(verification_status(report.status)),
                diagnostics: report.diagnostics.clone(),
                artifacts: vec![artifact_value(
                    identifier,
                    "verification-report",
                    "application/vnd.pkgshift.verification+json",
                    &report,
                )?],
            }));
        }
    }
    Ok(None)
}

fn runtime_match(run: &RuntimeRunArtifact, identifier: &str) -> Result<Option<LoadedArtifact>> {
    let (kind, content, diagnostics, status) = if run.run_id == identifier {
        (
            "runtime-run",
            artifact_value(
                identifier,
                "runtime-run",
                "application/vnd.pkgshift.runtime-run+json",
                &run,
            )?,
            run.diagnostics.clone(),
            run.state.clone(),
        )
    } else if run.plan.plan_id == identifier {
        (
            "runtime-migration-plan",
            artifact_value(
                identifier,
                "runtime-migration-plan",
                "application/vnd.pkgshift.runtime-plan+json",
                &run.plan,
            )?,
            run.plan.diagnostics.clone(),
            if run.plan.executable {
                "executable".to_owned()
            } else {
                "blocked".to_owned()
            },
        )
    } else if run
        .verification
        .as_ref()
        .is_some_and(|report| report.report_id == identifier)
    {
        let report = run.verification.as_ref().expect("matched verification");
        (
            "runtime-verification-report",
            artifact_value(
                identifier,
                "runtime-verification-report",
                "application/vnd.pkgshift.runtime-verification+json",
                report,
            )?,
            report.diagnostics.clone(),
            verification_status(report.status),
        )
    } else {
        return Ok(None);
    };
    Ok(Some(LoadedArtifact {
        kind,
        plan_id: Some(run.plan.plan_id.clone()),
        run_id: Some(run.run_id.clone()),
        status: Some(status),
        diagnostics,
        artifacts: vec![content],
    }))
}

fn runtime_artifact(state_directory: &Path, identifier: &str) -> Result<Option<LoadedArtifact>> {
    if is_short_digest_id(identifier, "runtime_run_") {
        return runtime_match(&load_run_artifact(state_directory, identifier)?, identifier);
    }
    for run_id in candidate_run_ids(&state_directory.join("runtime/runs"), "runtime_run_")? {
        let Ok(run) = load_run_artifact(state_directory, &run_id) else {
            continue;
        };
        if let Some(found) = runtime_match(&run, identifier)? {
            return Ok(Some(found));
        }
    }
    Ok(None)
}

pub(super) fn load(state_directory: &Path, identifier: &str) -> Result<Option<LoadedArtifact>> {
    if is_short_digest_id(identifier, "plan_") {
        return package_plan(state_directory, identifier).map(Some);
    }
    if is_short_digest_id(identifier, "run_") {
        return package_run(state_directory, identifier).map(Some);
    }
    if is_short_digest_id(identifier, "verification_") {
        return package_verification(state_directory, identifier);
    }
    if is_short_digest_id(identifier, "runtime_run_")
        || is_short_digest_id(identifier, "runtime_plan_")
        || is_short_digest_id(identifier, "runtime_verification_")
    {
        return runtime_artifact(state_directory, identifier);
    }
    Ok(None)
}
