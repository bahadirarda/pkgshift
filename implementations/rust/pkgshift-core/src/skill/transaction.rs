use std::fs;
use std::path::Path;

use crate::model::Diagnostic;
use crate::util::unix_timestamp_millis;

use super::inspect::{destination, inspect_skill, validate_parent_path};
use super::model::{SkillClient, SkillInstallMode, SkillScope, SkillStatus};
use super::source::{SkillBundle, load_bundle};

#[derive(Debug)]
pub(super) struct SkillMutationOutcome {
    pub status: SkillStatus,
    pub mutation_performed: bool,
}

fn diagnostic(code: &str, summary: impl Into<String>, remediation: &str) -> Diagnostic {
    Diagnostic::blocking(code, summary, vec![remediation.to_owned()])
}

fn operation_error(error: impl std::fmt::Display) -> Diagnostic {
    diagnostic(
        "SKILL_OPERATION_FAILED",
        format!("The Agent Skill operation failed: {error}"),
        "Run pkgshift skill doctor and resolve the reported installation state.",
    )
}

fn create_safe_parents(root: &Path, target: &Path) -> Result<(), Diagnostic> {
    let parent = target.parent().ok_or_else(|| {
        diagnostic(
            "SKILL_TARGET_PATH_UNSAFE",
            "The Agent Skill destination has no parent directory.",
            "Use a registered project or user skill destination.",
        )
    })?;
    let relative = parent.strip_prefix(root).map_err(|_| {
        diagnostic(
            "SKILL_TARGET_PATH_UNSAFE",
            "The Agent Skill destination escapes its declared scope root.",
            "Use a registered project or user skill destination.",
        )
    })?;
    let mut current = root.to_path_buf();
    for component in relative.components() {
        current.push(component.as_os_str());
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
                return Err(diagnostic(
                    "SKILL_TARGET_PATH_UNSAFE",
                    format!(
                        "The Agent Skill destination traverses an unsafe parent: {}",
                        current.display()
                    ),
                    "Replace symbolic-link or non-directory parents with confined directories.",
                ));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                fs::create_dir(&current).map_err(operation_error)?;
            }
            Err(error) => return Err(operation_error(error)),
        }
    }
    validate_parent_path(root, target)
}

fn write_bundle(bundle: &SkillBundle, target: &Path) -> Result<(), Diagnostic> {
    let parent = target.parent().ok_or_else(|| {
        diagnostic(
            "SKILL_TARGET_PATH_UNSAFE",
            "The Agent Skill destination has no parent directory.",
            "Use a registered project or user skill destination.",
        )
    })?;
    let name = target
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("pkgshift");
    let temporary = parent.join(format!(
        ".{name}.{}.{}.tmp",
        std::process::id(),
        unix_timestamp_millis()
    ));
    fs::create_dir(&temporary).map_err(operation_error)?;
    let result = (|| -> Result<(), Diagnostic> {
        for file in &bundle.files {
            let output = temporary.join(Path::new(&file.path));
            let output_parent = output.parent().ok_or_else(|| {
                diagnostic(
                    "SKILL_PATH_TYPE_UNSAFE",
                    "A portable skill file has no confined parent.",
                    "Repair the portable skill source before installation.",
                )
            })?;
            fs::create_dir_all(output_parent).map_err(operation_error)?;
            fs::write(&output, &file.content).map_err(operation_error)?;
        }
        if fs::symlink_metadata(target).is_ok() {
            return Err(diagnostic(
                "SKILL_INSTALL_CONFLICT",
                "The Agent Skill destination appeared during installation.",
                "Inspect the destination and retry without concurrent modification.",
            ));
        }
        fs::rename(&temporary, target).map_err(operation_error)
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&temporary);
    }
    result
}

#[cfg(unix)]
fn create_directory_link(source: &Path, target: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(source, target)
}

#[cfg(windows)]
fn create_directory_link(source: &Path, target: &Path) -> std::io::Result<()> {
    std::os::windows::fs::symlink_dir(source, target)
}

fn remove_exact_installation(target: &Path, mode: SkillInstallMode) -> Result<(), Diagnostic> {
    match mode {
        SkillInstallMode::Copy => fs::remove_dir_all(target).map_err(operation_error),
        SkillInstallMode::Link => fs::remove_file(target).map_err(operation_error),
    }
}

fn blocking(status: &SkillStatus) -> Option<Diagnostic> {
    status
        .diagnostics
        .iter()
        .find(|entry| entry.blocking)
        .cloned()
}

pub(super) fn install_skill(
    project_root: &Path,
    source_path: &Path,
    scope: SkillScope,
    client: SkillClient,
    mode: SkillInstallMode,
    user_root: Option<&Path>,
) -> Result<SkillMutationOutcome, Diagnostic> {
    let before = inspect_skill(project_root, Some(source_path), scope, client, user_root);
    if let Some(diagnostic) = blocking(&before) {
        return Err(diagnostic);
    }
    if before.installed {
        if before.healthy && before.mode == Some(mode) {
            return Ok(SkillMutationOutcome {
                status: before,
                mutation_performed: false,
            });
        }
        return Err(diagnostic(
            "SKILL_INSTALL_CONFLICT",
            "An existing Agent Skill installation cannot be replaced safely.",
            "Review or uninstall the existing installation first.",
        ));
    }
    let bundle = load_bundle(source_path)?;
    if before.source_digest.as_deref() != Some(bundle.digest.as_str()) {
        return Err(diagnostic(
            "SKILL_SOURCE_CHANGED",
            "The portable Agent Skill source changed during installation.",
            "Inspect the current skill source and retry the operation.",
        ));
    }
    let (root, target) = destination(project_root, scope, client, user_root)?;
    create_safe_parents(&root, &target)?;
    match mode {
        SkillInstallMode::Copy => write_bundle(&bundle, &target)?,
        SkillInstallMode::Link => {
            create_directory_link(&bundle.source_path, &target).map_err(operation_error)?;
        }
    }
    let status = inspect_skill(
        project_root,
        Some(&bundle.source_path),
        scope,
        client,
        user_root,
    );
    if !status.healthy || status.mode != Some(mode) {
        let _ = remove_exact_installation(&target, mode);
        return Err(diagnostic(
            "SKILL_OPERATION_FAILED",
            "The installed Agent Skill did not match its reviewed source.",
            "Inspect the destination before retrying the installation.",
        ));
    }
    Ok(SkillMutationOutcome {
        status,
        mutation_performed: true,
    })
}

pub(super) fn uninstall_skill(
    project_root: &Path,
    source_path: &Path,
    scope: SkillScope,
    client: SkillClient,
    user_root: Option<&Path>,
) -> Result<SkillMutationOutcome, Diagnostic> {
    let before = inspect_skill(project_root, Some(source_path), scope, client, user_root);
    if let Some(diagnostic) = blocking(&before) {
        return Err(diagnostic);
    }
    if !before.installed {
        return Ok(SkillMutationOutcome {
            status: before,
            mutation_performed: false,
        });
    }
    if before.mode == Some(SkillInstallMode::Copy) && before.source_digest.is_none() {
        return Err(diagnostic(
            "SKILL_UNINSTALL_SOURCE_UNVERIFIED",
            "The managed copy cannot be compared with its portable source.",
            "Restore the pkgshift skill source before uninstalling the managed copy.",
        ));
    }
    if before.modified {
        return Err(diagnostic(
            "SKILL_UNINSTALL_MODIFIED",
            "The installed Agent Skill contains local modifications.",
            "Preserve or remove the local changes manually before uninstalling.",
        ));
    }
    let (root, target) = destination(project_root, scope, client, user_root)?;
    validate_parent_path(&root, &target)?;
    let current = inspect_skill(project_root, Some(source_path), scope, client, user_root);
    if current.mode != before.mode
        || current.installed_digest != before.installed_digest
        || current.modified != before.modified
    {
        return Err(diagnostic(
            "SKILL_INSTALL_CONFLICT",
            "The Agent Skill destination changed during uninstall.",
            "Inspect the current destination and retry without concurrent modification.",
        ));
    }
    let mode = before.mode.ok_or_else(|| {
        diagnostic(
            "SKILL_INSTALL_CONFLICT",
            "The installed Agent Skill has no recognized ownership mode.",
            "Inspect the destination before uninstalling it manually.",
        )
    })?;
    remove_exact_installation(&target, mode)?;
    Ok(SkillMutationOutcome {
        status: inspect_skill(project_root, Some(source_path), scope, client, user_root),
        mutation_performed: true,
    })
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};

    use super::{install_skill, uninstall_skill};
    use crate::skill::model::{SkillClient, SkillInstallMode, SkillScope};

    fn source(root: &Path) -> PathBuf {
        let source = root.join("source");
        fs::create_dir(&source).expect("source directory");
        fs::write(
            source.join("SKILL.md"),
            "---\nname: pkgshift\ndescription: Safe migrations.\n---\n",
        )
        .expect("skill source");
        source
    }

    #[test]
    fn installs_and_removes_a_managed_copy() {
        let project = tempfile::tempdir().expect("project directory");
        let source = source(project.path());
        let installed = install_skill(
            project.path(),
            &source,
            SkillScope::Project,
            SkillClient::Codex,
            SkillInstallMode::Copy,
            None,
        )
        .expect("installed skill");
        assert!(installed.mutation_performed);
        assert!(installed.status.healthy);
        let removed = uninstall_skill(
            project.path(),
            &source,
            SkillScope::Project,
            SkillClient::Codex,
            None,
        )
        .expect("removed skill");
        assert!(removed.mutation_performed);
        assert!(!removed.status.installed);
    }

    #[test]
    fn protects_a_modified_managed_copy() {
        let project = tempfile::tempdir().expect("project directory");
        let source = source(project.path());
        install_skill(
            project.path(),
            &source,
            SkillScope::Project,
            SkillClient::Claude,
            SkillInstallMode::Copy,
            None,
        )
        .expect("installed skill");
        fs::write(
            project.path().join(".claude/skills/pkgshift/SKILL.md"),
            "local edits",
        )
        .expect("modified copy");
        let error = uninstall_skill(
            project.path(),
            &source,
            SkillScope::Project,
            SkillClient::Claude,
            None,
        )
        .expect_err("modified copy must be protected");
        assert_eq!(error.code, "SKILL_UNINSTALL_MODIFIED");
        assert!(project.path().join(".claude/skills/pkgshift").exists());
    }

    #[test]
    fn confines_a_user_installation_to_the_selected_user_root() {
        let project = tempfile::tempdir().expect("project directory");
        let user = tempfile::tempdir().expect("user directory");
        let source = source(project.path());
        let installed = install_skill(
            project.path(),
            &source,
            SkillScope::User,
            SkillClient::Codex,
            SkillInstallMode::Copy,
            Some(user.path()),
        )
        .expect("user skill");
        assert!(installed.status.healthy);
        assert!(
            user.path()
                .join(".agents/skills/pkgshift/SKILL.md")
                .is_file()
        );
        assert!(!project.path().join(".agents").exists());
    }
}
