use std::fs;
use std::path::{Path, PathBuf};

use crate::model::{Diagnostic, SCHEMA_VERSION};

use super::model::{SkillClient, SkillInstallMode, SkillScope, SkillStatus};
use super::source::{SkillBundle, directory_digest, load_bundle, resolve_source};

const SKILL_NAME: &str = "pkgshift";

fn diagnostic(code: &str, summary: impl Into<String>, remediation: &str) -> Diagnostic {
    Diagnostic::blocking(code, summary, vec![remediation.to_owned()])
}

pub(super) fn default_user_root() -> Result<PathBuf, Diagnostic> {
    let value = if cfg!(windows) {
        std::env::var_os("USERPROFILE")
    } else {
        std::env::var_os("HOME")
    };
    let root = value.map(PathBuf::from).ok_or_else(|| {
        diagnostic(
            "SKILL_USER_ROOT_NOT_FOUND",
            "The current user home directory could not be resolved.",
            "Set the platform user-home environment before using user-scoped skill commands.",
        )
    })?;
    fs::canonicalize(&root).map_err(|error| {
        diagnostic(
            "SKILL_USER_ROOT_NOT_FOUND",
            format!("The current user home directory could not be resolved: {error}"),
            "Use an existing user home directory for user-scoped skill commands.",
        )
    })
}

pub(super) fn destination(
    project_root: &Path,
    scope: SkillScope,
    client: SkillClient,
    user_root: Option<&Path>,
) -> Result<(PathBuf, PathBuf), Diagnostic> {
    let root = match scope {
        SkillScope::Project => project_root.to_path_buf(),
        SkillScope::User => match user_root {
            Some(root) => fs::canonicalize(root).map_err(|error| {
                diagnostic(
                    "SKILL_USER_ROOT_NOT_FOUND",
                    format!("The selected user root could not be resolved: {error}"),
                    "Use an existing user root for user-scoped skill commands.",
                )
            })?,
            None => default_user_root()?,
        },
    };
    Ok((
        root.clone(),
        root.join(client.directory())
            .join("skills")
            .join(SKILL_NAME),
    ))
}

pub(super) fn validate_parent_path(root: &Path, target: &Path) -> Result<(), Diagnostic> {
    let relative = target.strip_prefix(root).map_err(|_| {
        diagnostic(
            "SKILL_TARGET_PATH_UNSAFE",
            "The Agent Skill destination escapes its declared scope root.",
            "Use the registered project or user destination for this client.",
        )
    })?;
    let Some(parent) = relative.parent() else {
        return Err(diagnostic(
            "SKILL_TARGET_PATH_UNSAFE",
            "The Agent Skill destination has no confined parent directory.",
            "Use the registered project or user destination for this client.",
        ));
    };
    let mut current = root.to_path_buf();
    for component in parent.components() {
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
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
            Err(error) => {
                return Err(diagnostic(
                    "SKILL_TARGET_PATH_UNSAFE",
                    format!("The Agent Skill destination could not be inspected: {error}"),
                    "Repair the destination parents before installing the skill.",
                ));
            }
        }
    }
    Ok(())
}

fn inspect_destination(
    source: Option<&SkillBundle>,
    target: &Path,
    diagnostics: &mut Vec<Diagnostic>,
) -> (bool, Option<SkillInstallMode>, Option<String>) {
    let metadata = match fs::symlink_metadata(target) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return (false, None, None),
        Err(error) => {
            diagnostics.push(diagnostic(
                "SKILL_INSTALL_CONFLICT",
                format!("The Agent Skill destination could not be inspected: {error}"),
                "Repair or move the destination before installing this skill.",
            ));
            return (false, None, None);
        }
    };
    if metadata.file_type().is_symlink() {
        let linked = fs::read_link(target)
            .ok()
            .map(|value| target.parent().unwrap_or(Path::new(".")).join(value))
            .and_then(|value| fs::canonicalize(value).ok());
        let installed = source.is_some_and(|bundle| linked.as_ref() == Some(&bundle.source_path));
        if !installed {
            diagnostics.push(diagnostic(
                "SKILL_INSTALL_CONFLICT",
                format!(
                    "{} links to a different or unavailable skill source.",
                    target.display()
                ),
                "Move the conflicting link before installing this skill.",
            ));
        }
        return (
            installed,
            Some(SkillInstallMode::Link),
            installed.then(|| source.expect("installed link has source").digest.clone()),
        );
    }
    if metadata.is_dir() {
        match directory_digest(target) {
            Ok(digest) => (true, Some(SkillInstallMode::Copy), Some(digest)),
            Err(entry) => {
                diagnostics.push(Diagnostic {
                    code: "SKILL_INSTALL_CONFLICT".to_owned(),
                    summary: format!(
                        "The installed Agent Skill cannot be verified: {}",
                        entry.summary
                    ),
                    ..entry
                });
                (true, Some(SkillInstallMode::Copy), None)
            }
        }
    } else {
        diagnostics.push(diagnostic(
            "SKILL_INSTALL_CONFLICT",
            format!(
                "{} is not an Agent Skill directory or managed link.",
                target.display()
            ),
            "Move the conflicting path before installing this skill.",
        ));
        (false, None, None)
    }
}

pub(super) fn inspect_skill(
    project_root: &Path,
    source_path: Option<&Path>,
    scope: SkillScope,
    client: SkillClient,
    user_root: Option<&Path>,
) -> SkillStatus {
    let source_path = source_path.map_or_else(|| resolve_source(project_root), Path::to_path_buf);
    let mut diagnostics = Vec::new();
    let source = match load_bundle(&source_path) {
        Ok(bundle) => Some(bundle),
        Err(diagnostic) => {
            diagnostics.push(diagnostic);
            None
        }
    };
    let destination = destination(project_root, scope, client, user_root);
    let (target_root, target_path) = match destination {
        Ok(value) => value,
        Err(diagnostic) => {
            diagnostics.push(diagnostic);
            let root = user_root.unwrap_or(project_root);
            (
                root.to_path_buf(),
                root.join(client.directory())
                    .join("skills")
                    .join(SKILL_NAME),
            )
        }
    };
    if let Err(diagnostic) = validate_parent_path(&target_root, &target_path) {
        diagnostics.push(diagnostic);
    }
    let path_is_unsafe = diagnostics
        .iter()
        .any(|entry| entry.code == "SKILL_TARGET_PATH_UNSAFE");
    let (installed, mode, installed_digest) = if path_is_unsafe {
        (false, None, None)
    } else {
        inspect_destination(source.as_ref(), &target_path, &mut diagnostics)
    };
    let source_digest = source.as_ref().map(|bundle| bundle.digest.clone());
    let modified = installed
        && mode == Some(SkillInstallMode::Copy)
        && source_digest.is_some()
        && installed_digest.is_some()
        && source_digest != installed_digest;
    if modified {
        diagnostics.push(Diagnostic {
            code: "SKILL_INSTALL_MODIFIED".to_owned(),
            severity: crate::model::DiagnosticSeverity::Warning,
            summary: "The installed managed copy differs from the portable pkgshift skill."
                .to_owned(),
            blocking: false,
            evidence: Vec::new(),
            remediation: vec![
                "Review local edits before updating or uninstalling the skill.".to_owned(),
            ],
        });
    }
    let healthy = installed
        && !modified
        && !diagnostics.iter().any(|entry| entry.blocking)
        && source_digest.is_some();
    SkillStatus {
        schema_version: SCHEMA_VERSION.to_owned(),
        name: SKILL_NAME.to_owned(),
        client,
        scope,
        source_path: source
            .as_ref()
            .map_or(source_path, |bundle| bundle.source_path.clone())
            .to_string_lossy()
            .into_owned(),
        target_path: target_path.to_string_lossy().into_owned(),
        source_digest,
        installed_digest,
        installed,
        mode,
        healthy,
        modified,
        diagnostics,
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::inspect_skill;
    use crate::skill::model::{SkillClient, SkillScope};

    #[cfg(unix)]
    #[test]
    fn rejects_a_symbolic_link_destination_parent() {
        use std::os::unix::fs::symlink;

        let project = tempfile::tempdir().expect("project directory");
        let external = tempfile::tempdir().expect("external directory");
        let source = project.path().join("source");
        fs::create_dir(&source).expect("source directory");
        fs::write(
            source.join("SKILL.md"),
            "---\nname: pkgshift\ndescription: Safe migrations.\n---\n",
        )
        .expect("skill source");
        symlink(external.path(), project.path().join(".agents")).expect("parent link");
        let status = inspect_skill(
            project.path(),
            Some(&source),
            SkillScope::Project,
            SkillClient::Codex,
            None,
        );
        assert!(
            status
                .diagnostics
                .iter()
                .any(|entry| entry.code == "SKILL_TARGET_PATH_UNSAFE")
        );
    }
}
