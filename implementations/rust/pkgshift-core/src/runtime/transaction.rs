use std::collections::BTreeSet;
use std::fs::{self, File};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::inspect::{inspect_runtime, residual_bun_references};
use super::model::{
    RuntimeApplyOutcome, RuntimeMigrationPlan, RuntimePlanArtifact, RuntimeVerificationCheck,
    RuntimeVerificationReport, StoredRuntimeRun,
};
use crate::model::{
    Diagnostic, DiagnosticSeverity, MutationAction, SCHEMA_VERSION, SnapshotEntry,
    VerificationStatus,
};
use crate::util::{
    PkgshiftError, Result, atomic_write, create_new_lock, digest_json, file_digest,
    is_short_digest_id, read_json, resolve_root, safe_join, short_digest, unix_timestamp_millis,
    write_private_json,
};

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoreEnvelope<T> {
    store_schema_version: String,
    digest: String,
    content: T,
}

struct RepositoryLock {
    path: PathBuf,
    _file: File,
}

impl RepositoryLock {
    fn acquire(state_directory: &Path, root: &Path, operation: &str) -> Result<Self> {
        let repository_id = short_digest("repository_", &root.to_string_lossy())?;
        let path = state_directory
            .join("repositories")
            .join(format!("{repository_id}.lock"));
        let content = format!(
            "operation={operation}\npid={}\ncreatedAt={}\n",
            std::process::id(),
            unix_timestamp_millis()
        );
        for attempt in 0..2 {
            match create_new_lock(&path, &content) {
                Ok(file) => return Ok(Self { path, _file: file }),
                Err(PkgshiftError::Io { source, .. })
                    if attempt == 0
                        && source.kind() == std::io::ErrorKind::AlreadyExists
                        && stale_lock(&path) =>
                {
                    fs::remove_file(&path).map_err(|source| PkgshiftError::Io {
                        path: path.clone(),
                        source,
                    })?;
                }
                Err(error) => {
                    return Err(PkgshiftError::InvalidState(format!(
                        "another pkgshift transaction may be active for {}: {error}",
                        root.display()
                    )));
                }
            }
        }
        Err(PkgshiftError::InvalidState(format!(
            "another pkgshift transaction is active for {}",
            root.display()
        )))
    }
}

impl Drop for RepositoryLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn stale_lock(path: &Path) -> bool {
    let Ok(content) = fs::read_to_string(path) else {
        return false;
    };
    let Some(pid) = content
        .lines()
        .find_map(|line| line.strip_prefix("pid="))
        .and_then(|value| value.parse::<u32>().ok())
    else {
        return false;
    };
    !process_is_alive(pid)
}

#[cfg(target_os = "linux")]
fn process_is_alive(pid: u32) -> bool {
    Path::new("/proc").join(pid.to_string()).exists()
}

#[cfg(not(target_os = "linux"))]
fn process_is_alive(_pid: u32) -> bool {
    true
}

fn write_envelope<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    write_private_json(
        path,
        &StoreEnvelope {
            store_schema_version: SCHEMA_VERSION.to_owned(),
            digest: digest_json(value)?,
            content: value,
        },
    )
}

fn read_envelope<T>(path: &Path) -> Result<T>
where
    T: Serialize + serde::de::DeserializeOwned,
{
    let metadata = fs::symlink_metadata(path).map_err(|source| PkgshiftError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(PkgshiftError::InvalidState(format!(
            "stored runtime artifact is not a regular file: {}",
            path.display()
        )));
    }
    let envelope: StoreEnvelope<T> = read_json(path)?;
    if envelope.store_schema_version != SCHEMA_VERSION
        || digest_json(&envelope.content)? != envelope.digest
    {
        return Err(PkgshiftError::InvalidState(format!(
            "stored runtime artifact failed integrity verification: {}",
            path.display()
        )));
    }
    Ok(envelope.content)
}

fn run_path(state_directory: &Path, run_id: &str) -> PathBuf {
    state_directory
        .join("runtime/runs")
        .join(run_id)
        .join("run.json")
}

fn save_run(state_directory: &Path, run: &StoredRuntimeRun) -> Result<()> {
    write_envelope(&run_path(state_directory, &run.run_id), run)
}

pub(crate) fn load_run(state_directory: &Path, run_id: &str) -> Result<StoredRuntimeRun> {
    if !is_short_digest_id(run_id, "runtime_run_") {
        return Err(PkgshiftError::InvalidState(
            "runtime run identifier is not a canonical runtime_run_ digest".to_owned(),
        ));
    }
    let run: StoredRuntimeRun = read_envelope(&run_path(state_directory, run_id))?;
    if run.run_id != run_id {
        return Err(PkgshiftError::InvalidState(
            "stored runtime run identity does not match its path".to_owned(),
        ));
    }
    Ok(run)
}

fn path_state(path: &Path) -> Result<Option<fs::Metadata>> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => Ok(Some(metadata)),
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(PkgshiftError::Io {
            path: path.to_path_buf(),
            source,
        }),
    }
}

#[cfg(unix)]
fn file_mode(metadata: &fs::Metadata) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    metadata.permissions().mode() & 0o777
}

#[cfg(not(unix))]
fn file_mode(_metadata: &fs::Metadata) -> u32 {
    0o644
}

#[cfg(unix)]
fn set_file_mode(path: &Path, mode: u32) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).map_err(|source| {
        PkgshiftError::Io {
            path: path.to_path_buf(),
            source,
        }
    })
}

#[cfg(not(unix))]
fn set_file_mode(_path: &Path, _mode: u32) -> Result<()> {
    Ok(())
}

fn snapshot(
    state_directory: &Path,
    root: &Path,
    run_id: &str,
    paths: &BTreeSet<String>,
) -> Result<Vec<SnapshotEntry>> {
    let run_directory = state_directory.join("runtime/runs").join(run_id);
    let directory = run_directory.join("snapshots");
    fs::create_dir_all(&directory).map_err(|source| PkgshiftError::Io {
        path: directory.clone(),
        source,
    })?;
    #[cfg(unix)]
    set_file_mode(&directory, 0o700)?;
    let mut entries = Vec::with_capacity(paths.len());
    for (index, relative) in paths.iter().enumerate() {
        let absolute = safe_join(root, relative)?;
        let Some(metadata) = path_state(&absolute)? else {
            entries.push(SnapshotEntry {
                path: relative.clone(),
                existed: false,
                digest: None,
                mode: None,
                backup_path: None,
            });
            continue;
        };
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(PkgshiftError::InvalidState(format!(
                "runtime snapshot target is not a regular file: {relative}"
            )));
        }
        let content = fs::read(&absolute).map_err(|source| PkgshiftError::Io {
            path: absolute.clone(),
            source,
        })?;
        let backup_path = format!("snapshots/{:04}.bin", index + 1);
        let backup = run_directory.join(&backup_path);
        atomic_write(&backup, &content)?;
        set_file_mode(&backup, 0o600)?;
        entries.push(SnapshotEntry {
            path: relative.clone(),
            existed: true,
            digest: file_digest(&absolute)?,
            mode: Some(file_mode(&metadata)),
            backup_path: Some(backup_path),
        });
    }
    Ok(entries)
}

fn restore_snapshot(state_directory: &Path, root: &Path, run: &StoredRuntimeRun) -> Result<()> {
    for entry in &run.snapshot_entries {
        let absolute = safe_join(root, &entry.path)?;
        if let Some(metadata) = path_state(&absolute)?
            && (metadata.file_type().is_symlink() || !metadata.is_file())
        {
            return Err(PkgshiftError::InvalidState(format!(
                "runtime restore target is not a regular file: {}",
                entry.path
            )));
        }
        if !entry.existed {
            if absolute.exists() {
                fs::remove_file(&absolute).map_err(|source| PkgshiftError::Io {
                    path: absolute.clone(),
                    source,
                })?;
            }
            continue;
        }
        let backup_path = entry.backup_path.as_ref().ok_or_else(|| {
            PkgshiftError::InvalidState(format!(
                "runtime snapshot backup metadata is missing for {}",
                entry.path
            ))
        })?;
        let backup = state_directory
            .join("runtime/runs")
            .join(&run.run_id)
            .join(backup_path);
        if file_digest(&backup)? != entry.digest {
            return Err(PkgshiftError::InvalidState(format!(
                "runtime snapshot digest verification failed for {}",
                entry.path
            )));
        }
        let content = fs::read(&backup).map_err(|source| PkgshiftError::Io {
            path: backup.clone(),
            source,
        })?;
        atomic_write(&absolute, &content)?;
        if let Some(mode) = entry.mode {
            set_file_mode(&absolute, mode)?;
        }
    }
    Ok(())
}

fn execute_mutation(root: &Path, mutation: &crate::model::PlannedFileMutation) -> Result<()> {
    let path = safe_join(root, &mutation.path)?;
    if let Some(metadata) = path_state(&path)?
        && (metadata.file_type().is_symlink() || !metadata.is_file())
    {
        return Err(PkgshiftError::InvalidState(format!(
            "runtime mutation target is not a regular file: {}",
            mutation.path
        )));
    }
    if file_digest(&path)? != mutation.before_digest {
        return Err(PkgshiftError::InvalidState(format!(
            "runtime mutation precondition changed after planning: {}",
            mutation.path
        )));
    }
    match mutation.action {
        MutationAction::Write => {
            let content = mutation.content.as_deref().ok_or_else(|| {
                PkgshiftError::InvalidState(format!(
                    "runtime write mutation has no content: {}",
                    mutation.path
                ))
            })?;
            let mode = path_state(&path)?.as_ref().map_or(0o644, file_mode);
            atomic_write(&path, content.as_bytes())?;
            set_file_mode(&path, mode)?;
        }
        MutationAction::Delete => {
            if path.exists() {
                fs::remove_file(&path).map_err(|source| PkgshiftError::Io {
                    path: path.clone(),
                    source,
                })?;
            }
        }
    }
    if file_digest(&path)? != mutation.after_digest {
        return Err(PkgshiftError::InvalidState(format!(
            "runtime mutation postcondition failed: {}",
            mutation.path
        )));
    }
    Ok(())
}

fn failure_diagnostic(code: &str, summary: impl Into<String>, run_id: &str) -> Diagnostic {
    Diagnostic {
        code: code.to_owned(),
        severity: DiagnosticSeverity::Error,
        summary: summary.into(),
        blocking: true,
        evidence: Vec::new(),
        remediation: vec![format!(
            "Run pkgshift runtime rollback {run_id} --approve {run_id}."
        )],
    }
}

fn verify_runtime(root: &Path, run: &StoredRuntimeRun) -> Result<RuntimeVerificationReport> {
    let digest_passed = run
        .plan
        .operations
        .iter()
        .flat_map(|operation| &operation.mutations)
        .all(|mutation| {
            file_digest(&root.join(&mutation.path))
                .is_ok_and(|digest| digest == mutation.after_digest)
        });
    let residues = residual_bun_references(root)?;
    let residue_passed = residues.is_empty();
    let mut diagnostics = Vec::new();
    if !digest_passed {
        diagnostics.push(failure_diagnostic(
            "RUNTIME_AFTER_DIGEST_FAILED",
            "At least one runtime mutation no longer matches its planned afterDigest.",
            &run.run_id,
        ));
    }
    if !residue_passed {
        diagnostics.push(Diagnostic {
            code: "RUNTIME_BUN_RESIDUE_REMAINS".to_owned(),
            severity: DiagnosticSeverity::Error,
            summary: format!(
                "{} Bun runtime reference(s) remain after migration.",
                residues.len()
            ),
            blocking: true,
            evidence: residues
                .iter()
                .take(64)
                .map(|value| crate::model::EvidenceDetail {
                    location: value
                        .split_once(':')
                        .map_or(value.as_str(), |entry| entry.0)
                        .to_owned(),
                    detail: "Bun runtime residue".to_owned(),
                })
                .collect(),
            remediation: vec![format!(
                "Run pkgshift runtime rollback {} --approve {}.",
                run.run_id, run.run_id
            )],
        });
    }
    let status = if digest_passed && residue_passed {
        VerificationStatus::Passed
    } else {
        VerificationStatus::Failed
    };
    let checks = vec![
        RuntimeVerificationCheck {
            id: "planned-after-digests".to_owned(),
            status: if digest_passed {
                VerificationStatus::Passed
            } else {
                VerificationStatus::Failed
            },
            summary: "All runtime mutation targets match their reviewed afterDigest.".to_owned(),
        },
        RuntimeVerificationCheck {
            id: "bun-runtime-residue".to_owned(),
            status: if residue_passed {
                VerificationStatus::Passed
            } else {
                VerificationStatus::Failed
            },
            summary: "The supported inspection boundary contains no Bun runtime residue."
                .to_owned(),
        },
    ];
    let report_id = short_digest(
        "runtime_verification_",
        &(
            &run.plan.plan_id,
            &run.run_id,
            status,
            &checks,
            &diagnostics,
        ),
    )?;
    Ok(RuntimeVerificationReport {
        schema_version: SCHEMA_VERSION.to_owned(),
        report_id,
        plan_id: run.plan.plan_id.clone(),
        run_id: run.run_id.clone(),
        status,
        checks,
        diagnostics,
    })
}

pub(super) fn apply_plan(
    root: &Path,
    state_directory: &Path,
    plan: &RuntimeMigrationPlan,
    approval: Option<&str>,
) -> Result<RuntimeApplyOutcome> {
    let root = resolve_root(root)?;
    let _lock = RepositoryLock::acquire(state_directory, &root, "runtime-apply")?;
    if approval != Some(plan.plan_id.as_str()) {
        return Err(PkgshiftError::InvalidState(format!(
            "runtime apply requires exact approval for {}",
            plan.plan_id
        )));
    }
    if !plan.executable {
        return Err(PkgshiftError::InvalidState(format!(
            "runtime plan {} is not executable",
            plan.plan_id
        )));
    }
    if inspect_runtime(&root)?.fingerprint != plan.repository_fingerprint {
        return Err(PkgshiftError::InvalidState(
            "runtime migration-relevant repository evidence changed after planning".to_owned(),
        ));
    }
    let run_id = short_digest(
        "runtime_run_",
        &(
            &plan.plan_id,
            root.to_string_lossy(),
            unix_timestamp_millis(),
            std::process::id(),
        ),
    )?;
    let paths = plan
        .operations
        .iter()
        .flat_map(|operation| {
            operation
                .mutations
                .iter()
                .map(|mutation| mutation.path.clone())
        })
        .collect::<BTreeSet<_>>();
    let snapshot_entries = snapshot(state_directory, &root, &run_id, &paths)?;
    let mut run = StoredRuntimeRun {
        schema_version: SCHEMA_VERSION.to_owned(),
        run_id: run_id.clone(),
        plan: RuntimePlanArtifact::from(plan),
        state: "applying".to_owned(),
        snapshot_directory: format!("runtime/runs/{run_id}/snapshots"),
        snapshot_entries,
        diagnostics: Vec::new(),
        verification: None,
    };
    save_run(state_directory, &run)?;
    for mutation in plan
        .operations
        .iter()
        .flat_map(|operation| &operation.mutations)
    {
        if let Err(error) = execute_mutation(&root, mutation) {
            run.state = "failed".to_owned();
            run.diagnostics.push(failure_diagnostic(
                "RUNTIME_EXECUTION_FAILED",
                error.to_string(),
                &run.run_id,
            ));
            save_run(state_directory, &run)?;
            return Ok(RuntimeApplyOutcome {
                run,
                verification: None,
            });
        }
    }
    run.state = "verifying".to_owned();
    save_run(state_directory, &run)?;
    let verification = verify_runtime(&root, &run)?;
    run.state = if verification.status == VerificationStatus::Passed {
        "succeeded"
    } else {
        "failed"
    }
    .to_owned();
    run.diagnostics = verification.diagnostics.clone();
    run.verification = Some(verification.clone());
    save_run(state_directory, &run)?;
    write_private_json(
        &state_directory
            .join("runtime/runs")
            .join(&run.run_id)
            .join("verification.json"),
        &verification,
    )?;
    Ok(RuntimeApplyOutcome {
        run,
        verification: Some(verification),
    })
}

pub(super) fn rollback_run(
    root: &Path,
    state_directory: &Path,
    run_id: &str,
    approval: Option<&str>,
) -> Result<StoredRuntimeRun> {
    let root = resolve_root(root)?;
    let _lock = RepositoryLock::acquire(state_directory, &root, "runtime-rollback")?;
    if approval != Some(run_id) {
        return Err(PkgshiftError::InvalidState(format!(
            "runtime rollback requires exact approval for {run_id}"
        )));
    }
    let mut run = load_run(state_directory, run_id)?;
    if run.state == "rolled-back" {
        return Err(PkgshiftError::InvalidState(format!(
            "runtime run {run_id} is already rolled back"
        )));
    }
    restore_snapshot(state_directory, &root, &run)?;
    if inspect_runtime(&root)?.fingerprint == run.plan.repository_fingerprint {
        run.state = "rolled-back".to_owned();
        run.diagnostics.clear();
    } else {
        run.state = "rollback-failed".to_owned();
        run.diagnostics = vec![failure_diagnostic(
            "RUNTIME_ROLLBACK_FAILED",
            "Restored runtime repository fingerprint does not match the plan baseline.",
            run_id,
        )];
    }
    save_run(state_directory, &run)?;
    Ok(run)
}
