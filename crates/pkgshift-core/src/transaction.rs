use std::collections::BTreeSet;
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};

use crate::catalog::get_package_manager;
use crate::inspect::{build_project_ir, inspect_project};
use crate::model::{
    ApplyOutcome, Diagnostic, DiagnosticSeverity, MigrationPlan, MutationAction,
    ProcessExecutionRecord, SCHEMA_VERSION, SnapshotEntry, StoredPlan, StoredRun,
    VerificationCheck, VerificationReport, VerificationStatus,
};
use crate::util::{
    PkgshiftError, Result, atomic_write, create_new_lock, digest_json, file_digest, read_json,
    resolve_root, safe_join, short_digest, unix_timestamp_millis, write_private_json,
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

impl Drop for RepositoryLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn diagnostic(code: &str, summary: impl Into<String>, remediation: Vec<String>) -> Diagnostic {
    Diagnostic {
        code: code.to_owned(),
        severity: DiagnosticSeverity::Error,
        summary: summary.into(),
        blocking: true,
        evidence: Vec::new(),
        remediation,
    }
}

fn plan_path(state_directory: &Path, plan_id: &str) -> PathBuf {
    state_directory
        .join("plans")
        .join(format!("{plan_id}.json"))
}

fn run_path(state_directory: &Path, run_id: &str) -> PathBuf {
    state_directory.join("runs").join(run_id).join("run.json")
}

pub fn save_plan(state_directory: &Path, stored: &StoredPlan) -> Result<PathBuf> {
    let path = plan_path(state_directory, &stored.plan.plan_id);
    write_envelope(&path, stored)?;
    Ok(path)
}

pub fn load_plan(state_directory: &Path, plan_id: &str) -> Result<StoredPlan> {
    if !plan_id.starts_with("plan_") {
        return Err(PkgshiftError::InvalidState(
            "plan identifiers must start with plan_".to_owned(),
        ));
    }
    let stored: StoredPlan = read_envelope(&plan_path(state_directory, plan_id))?;
    if stored.plan.plan_id != plan_id {
        return Err(PkgshiftError::InvalidState(
            "stored plan identity does not match its path".to_owned(),
        ));
    }
    Ok(stored)
}

pub fn load_run(state_directory: &Path, run_id: &str) -> Result<StoredRun> {
    if !run_id.starts_with("run_") {
        return Err(PkgshiftError::InvalidState(
            "run identifiers must start with run_".to_owned(),
        ));
    }
    let run: StoredRun = read_envelope(&run_path(state_directory, run_id))?;
    if run.run_id != run_id {
        return Err(PkgshiftError::InvalidState(
            "stored run identity does not match its path".to_owned(),
        ));
    }
    Ok(run)
}

fn save_run(state_directory: &Path, run: &StoredRun) -> Result<()> {
    write_envelope(&run_path(state_directory, &run.run_id), run)
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
    let envelope: StoreEnvelope<T> = read_json(path)?;
    if envelope.store_schema_version != SCHEMA_VERSION
        || digest_json(&envelope.content)? != envelope.digest
    {
        return Err(PkgshiftError::InvalidState(format!(
            "stored artifact failed integrity verification: {}",
            path.display()
        )));
    }
    Ok(envelope.content)
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
    let directory = state_directory.join("runs").join(run_id).join("snapshots");
    fs::create_dir_all(&directory).map_err(|source| PkgshiftError::Io {
        path: directory.clone(),
        source,
    })?;
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
                "snapshot target is not a regular file: {relative}"
            )));
        }
        let content = fs::read(&absolute).map_err(|source| PkgshiftError::Io {
            path: absolute.clone(),
            source,
        })?;
        let backup_path = format!("snapshots/{:04}.bin", index + 1);
        atomic_write(
            &state_directory.join("runs").join(run_id).join(&backup_path),
            &content,
        )?;
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

fn restore_snapshot(state_directory: &Path, root: &Path, run: &StoredRun) -> Result<()> {
    for entry in &run.snapshot_entries {
        let absolute = safe_join(root, &entry.path)?;
        if let Some(metadata) = path_state(&absolute)?
            && (metadata.file_type().is_symlink() || !metadata.is_file())
        {
            return Err(PkgshiftError::InvalidState(format!(
                "restore target is not a regular file: {}",
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
                "snapshot backup metadata is missing for {}",
                entry.path
            ))
        })?;
        let backup = state_directory
            .join("runs")
            .join(&run.run_id)
            .join(backup_path);
        let content = fs::read(&backup).map_err(|source| PkgshiftError::Io {
            path: backup.clone(),
            source,
        })?;
        if file_digest(&backup)? != entry.digest {
            return Err(PkgshiftError::InvalidState(format!(
                "snapshot digest verification failed for {}",
                entry.path
            )));
        }
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
            "mutation target is not a regular file: {}",
            mutation.path
        )));
    }
    if file_digest(&path)? != mutation.before_digest {
        return Err(PkgshiftError::InvalidState(format!(
            "mutation precondition changed after planning: {}",
            mutation.path
        )));
    }
    match mutation.action {
        MutationAction::Write => {
            let content = mutation.content.as_deref().ok_or_else(|| {
                PkgshiftError::InvalidState(format!(
                    "write mutation has no content: {}",
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
            "mutation postcondition failed: {}",
            mutation.path
        )));
    }
    Ok(())
}

fn withheld_output(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        String::new()
    } else {
        format!("<{} bytes withheld by pkgshift>", bytes.len())
    }
}

fn run_process(root: &Path, argv: &[String]) -> Result<ProcessExecutionRecord> {
    let (program, arguments) = argv.split_first().ok_or_else(|| {
        PkgshiftError::InvalidState("process operation has an empty command".to_owned())
    })?;
    let output = Command::new(program)
        .args(arguments)
        .current_dir(root)
        .env("npm_config_ignore_scripts", "true")
        .env("YARN_ENABLE_SCRIPTS", "false")
        .env("BUN_INSTALL_IGNORE_SCRIPTS", "1")
        .output()
        .map_err(|source| PkgshiftError::Process(format!("could not start {program}: {source}")))?;
    Ok(ProcessExecutionRecord {
        argv: argv.to_vec(),
        exit_code: output.status.code(),
        stdout: withheld_output(&output.stdout),
        stderr: withheld_output(&output.stderr),
        success: output.status.success(),
    })
}

fn verify_plan(
    root: &Path,
    plan: &MigrationPlan,
    expected_packages: &[String],
    run_id: &str,
    install_succeeded: bool,
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
    checks.push(VerificationCheck {
        id: "target-lockfile".to_owned(),
        status: if existing_locks.is_empty() {
            VerificationStatus::Failed
        } else {
            VerificationStatus::Passed
        },
        summary: if existing_locks.is_empty() {
            format!("No {} lockfile was generated.", plan.target)
        } else {
            format!(
                "Target dependency state exists: {}.",
                existing_locks.join(", ")
            )
        },
        evidence: existing_locks,
    });

    let current_packages: Vec<String> = build_project_ir(&inspection)?
        .map(|ir| {
            ir.packages
                .into_iter()
                .map(|package| package.path)
                .collect()
        })
        .unwrap_or_default();
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

    checks.push(VerificationCheck {
        id: "dependency-graph-drift".to_owned(),
        status: VerificationStatus::Skipped,
        summary: "Resolved graph drift is not compared in the Rust MVP.".to_owned(),
        evidence: vec!["This limitation is reported explicitly.".to_owned()],
    });
    let failed = checks
        .iter()
        .filter(|check| check.status == VerificationStatus::Failed)
        .count();
    let diagnostics = if failed == 0 {
        Vec::new()
    } else {
        vec![diagnostic(
            "VERIFICATION_FAILED",
            format!("{failed} blocking verification checks failed."),
            vec!["Repair the repository or roll back the run.".to_owned()],
        )]
    };
    let status = if failed == 0 {
        VerificationStatus::Passed
    } else {
        VerificationStatus::Failed
    };
    let report_id = short_digest(
        "verification_",
        &(run_id, &plan.plan_id, status, &checks, &diagnostics),
    )?;
    Ok(VerificationReport {
        schema_version: SCHEMA_VERSION.to_owned(),
        report_id,
        run_id: run_id.to_owned(),
        plan_id: plan.plan_id.clone(),
        status,
        checks,
        diagnostics,
    })
}

pub fn apply_stored_plan(
    root: &Path,
    state_directory: &Path,
    plan_id: &str,
    approval: Option<&str>,
) -> Result<ApplyOutcome> {
    let root = resolve_root(root)?;
    let state_directory = if state_directory.is_absolute() {
        state_directory.to_path_buf()
    } else {
        root.join(state_directory)
    };
    let _lock = RepositoryLock::acquire(&state_directory, &root, "apply")?;
    let stored_plan = load_plan(&state_directory, plan_id)?;
    let plan = &stored_plan.plan;
    if approval != Some(plan.plan_id.as_str()) {
        return Err(PkgshiftError::InvalidState(format!(
            "apply requires exact approval for {}",
            plan.plan_id
        )));
    }
    if !plan.executable {
        return Err(PkgshiftError::InvalidState(format!(
            "plan {} is not executable",
            plan.plan_id
        )));
    }
    let current = inspect_project(&root)?;
    if current.fingerprint != plan.repository_fingerprint {
        return Err(PkgshiftError::InvalidState(
            "migration-relevant repository evidence changed after planning".to_owned(),
        ));
    }

    let run_id = short_digest(
        "run_",
        &(
            &plan.plan_id,
            root.to_string_lossy(),
            unix_timestamp_millis(),
            std::process::id(),
        ),
    )?;
    let mut snapshot_paths = plan
        .operations
        .iter()
        .flat_map(|operation| operation.mutations.iter().map(|entry| entry.path.clone()))
        .collect::<BTreeSet<_>>();
    snapshot_paths.extend(
        get_package_manager(plan.target)
            .lockfiles
            .iter()
            .map(ToString::to_string),
    );
    let snapshot_entries = snapshot(&state_directory, &root, &run_id, &snapshot_paths)?;
    let mut run = StoredRun {
        schema_version: SCHEMA_VERSION.to_owned(),
        run_id: run_id.clone(),
        plan: plan.clone(),
        state: "applying".to_owned(),
        snapshot_directory: format!("runs/{run_id}/snapshots"),
        snapshot_entries,
        processes: Vec::new(),
        diagnostics: Vec::new(),
    };
    save_run(&state_directory, &run)?;

    for operation in plan
        .operations
        .iter()
        .filter(|operation| operation.phase != "verify")
    {
        let execution = (|| -> Result<()> {
            for mutation in &operation.mutations {
                execute_mutation(&root, mutation)?;
            }
            if !operation.command.is_empty() {
                let process = run_process(&root, &operation.command)?;
                let success = process.success;
                run.processes.push(process);
                save_run(&state_directory, &run)?;
                if !success {
                    return Err(PkgshiftError::Process(format!(
                        "{} did not complete successfully",
                        operation.command.join(" ")
                    )));
                }
            }
            Ok(())
        })();
        if let Err(error) = execution {
            run.state = "failed".to_owned();
            run.diagnostics.push(diagnostic(
                "EXECUTION_FAILED",
                error.to_string(),
                vec![format!(
                    "Run pkgshift rollback {} --approve {}.",
                    run.run_id, run.run_id
                )],
            ));
            save_run(&state_directory, &run)?;
            return Ok(ApplyOutcome {
                run,
                verification: None,
            });
        }
    }

    run.state = "verifying".to_owned();
    save_run(&state_directory, &run)?;
    let expected_packages = stored_plan
        .project_ir
        .packages
        .iter()
        .map(|package| package.path.clone())
        .collect::<Vec<_>>();
    let verification = verify_plan(&root, plan, &expected_packages, &run_id, true)?;
    run.state = if verification.status == VerificationStatus::Passed {
        "succeeded"
    } else {
        "failed"
    }
    .to_owned();
    run.diagnostics.extend(verification.diagnostics.clone());
    save_run(&state_directory, &run)?;
    write_private_json(
        &state_directory
            .join("runs")
            .join(&run_id)
            .join("verification.json"),
        &verification,
    )?;
    Ok(ApplyOutcome {
        run,
        verification: Some(verification),
    })
}

pub fn verify_stored_run(
    root: &Path,
    state_directory: &Path,
    run_id: &str,
) -> Result<VerificationReport> {
    let root = resolve_root(root)?;
    let state_directory = if state_directory.is_absolute() {
        state_directory.to_path_buf()
    } else {
        root.join(state_directory)
    };
    let _lock = RepositoryLock::acquire(&state_directory, &root, "verify")?;
    let mut run = load_run(&state_directory, run_id)?;
    let stored_plan = load_plan(&state_directory, &run.plan.plan_id)?;
    let expected_packages = stored_plan
        .project_ir
        .packages
        .iter()
        .map(|package| package.path.clone())
        .collect::<Vec<_>>();
    let install_succeeded = run.processes.iter().any(|process| process.success);
    let report = verify_plan(
        &root,
        &run.plan,
        &expected_packages,
        run_id,
        install_succeeded,
    )?;
    run.state = if report.status == VerificationStatus::Passed {
        "succeeded"
    } else {
        "failed"
    }
    .to_owned();
    run.diagnostics = report.diagnostics.clone();
    save_run(&state_directory, &run)?;
    write_private_json(
        &state_directory
            .join("runs")
            .join(run_id)
            .join("verification.json"),
        &report,
    )?;
    Ok(report)
}

pub fn rollback_stored_run(
    root: &Path,
    state_directory: &Path,
    run_id: &str,
    approval: Option<&str>,
) -> Result<StoredRun> {
    let root = resolve_root(root)?;
    let state_directory = if state_directory.is_absolute() {
        state_directory.to_path_buf()
    } else {
        root.join(state_directory)
    };
    let _lock = RepositoryLock::acquire(&state_directory, &root, "rollback")?;
    if approval != Some(run_id) {
        return Err(PkgshiftError::InvalidState(format!(
            "rollback requires exact approval for {run_id}"
        )));
    }
    let mut run = load_run(&state_directory, run_id)?;
    if run.state == "rolled-back" {
        return Err(PkgshiftError::InvalidState(format!(
            "run {run_id} is already rolled back"
        )));
    }
    restore_snapshot(&state_directory, &root, &run)?;
    let inspection = inspect_project(&root)?;
    if inspection.fingerprint != run.plan.repository_fingerprint {
        run.state = "rollback-failed".to_owned();
        run.diagnostics.push(diagnostic(
            "ROLLBACK_FAILED",
            "restored repository fingerprint does not match the plan baseline",
            vec!["Preserve the state directory and inspect snapshot integrity.".to_owned()],
        ));
    } else {
        run.state = "rolled-back".to_owned();
        run.diagnostics = vec![Diagnostic {
            code: "ROLLBACK_EXTERNAL_EFFECTS_REMAIN".to_owned(),
            severity: DiagnosticSeverity::Warning,
            summary: "Repository files were restored; dependency cache and node_modules effects are not reverted."
                .to_owned(),
            blocking: false,
            evidence: Vec::new(),
            remediation: vec![
                "Reinstall the source dependency state when node_modules parity is required."
                    .to_owned(),
            ],
        }];
    }
    save_run(&state_directory, &run)?;
    Ok(run)
}
