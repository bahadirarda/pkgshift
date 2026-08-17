use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::model::Diagnostic;
use crate::util::hex_lower;

const SKILL_NAME: &str = "pkgshift";

#[derive(Debug, Clone)]
pub(super) struct SkillFile {
    pub path: String,
    pub content: Vec<u8>,
}

#[derive(Debug, Clone)]
pub(super) struct SkillBundle {
    pub source_path: PathBuf,
    pub digest: String,
    pub files: Vec<SkillFile>,
}

fn digest_files(files: &[SkillFile]) -> String {
    let mut hash = Sha256::new();
    for file in files {
        hash.update(file.path.as_bytes());
        hash.update([0]);
        hash.update(&file.content);
        hash.update([0]);
    }
    format!("sha256:{}", hex_lower(&hash.finalize()))
}

fn read_directory(root: &Path) -> Result<(PathBuf, Vec<SkillFile>), Diagnostic> {
    let root = fs::canonicalize(root).map_err(|_| {
        diagnostic(
            "SKILL_SOURCE_NOT_FOUND",
            format!("Agent Skill directory was not found: {}", root.display()),
            "Restore the expected Agent Skill directory before retrying.",
        )
    })?;
    if !root.is_dir() {
        return Err(diagnostic(
            "SKILL_SOURCE_NOT_FOUND",
            format!("Agent Skill path is not a directory: {}", root.display()),
            "Restore the expected Agent Skill directory before retrying.",
        ));
    }
    let mut files = Vec::new();
    visit(&root, &root, &mut files)?;
    files.sort_by(|left, right| left.path.cmp(&right.path));
    Ok((root, files))
}

fn diagnostic(code: &str, summary: impl Into<String>, remediation: &str) -> Diagnostic {
    Diagnostic::blocking(code, summary, vec![remediation.to_owned()])
}

fn visit(root: &Path, directory: &Path, files: &mut Vec<SkillFile>) -> Result<(), Diagnostic> {
    let mut entries = fs::read_dir(directory)
        .map_err(|error| {
            diagnostic(
                "SKILL_SOURCE_INVALID",
                format!("Portable skill source could not be read: {error}"),
                "Repair the portable skill source before installation.",
            )
        })?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|error| {
            diagnostic(
                "SKILL_SOURCE_INVALID",
                format!("Portable skill source could not be enumerated: {error}"),
                "Repair the portable skill source before installation.",
            )
        })?;
    entries.sort_by_key(fs::DirEntry::file_name);
    for entry in entries {
        let file_type = entry.file_type().map_err(|error| {
            diagnostic(
                "SKILL_SOURCE_INVALID",
                format!("Portable skill entry could not be inspected: {error}"),
                "Repair the portable skill source before installation.",
            )
        })?;
        if file_type.is_dir() {
            visit(root, &entry.path(), files)?;
        } else if file_type.is_file() {
            let path = entry
                .path()
                .strip_prefix(root)
                .map_err(|_| {
                    diagnostic(
                        "SKILL_PATH_TYPE_UNSAFE",
                        "Portable skill content escaped its source root.",
                        "Use only regular files and directories inside the portable skill source.",
                    )
                })?
                .to_string_lossy()
                .replace('\\', "/");
            let content = fs::read(entry.path()).map_err(|error| {
                diagnostic(
                    "SKILL_SOURCE_INVALID",
                    format!("Portable skill file could not be read: {error}"),
                    "Repair the portable skill source before installation.",
                )
            })?;
            files.push(SkillFile { path, content });
        } else {
            return Err(diagnostic(
                "SKILL_PATH_TYPE_UNSAFE",
                format!(
                    "Portable skill content contains an unsupported path type: {}",
                    entry.path().display()
                ),
                "Use only regular files and directories inside the portable skill source.",
            ));
        }
    }
    Ok(())
}

fn validate_frontmatter(files: &[SkillFile]) -> Result<(), Diagnostic> {
    let content = files
        .iter()
        .find(|file| file.path == "SKILL.md")
        .ok_or_else(|| {
            diagnostic(
                "SKILL_SOURCE_INVALID",
                "Portable skill source does not contain SKILL.md.",
                "Restore the portable pkgshift skill source before installation.",
            )
        })?;
    let content = std::str::from_utf8(&content.content).map_err(|_| {
        diagnostic(
            "SKILL_SOURCE_INVALID",
            "Portable SKILL.md is not valid UTF-8.",
            "Repair the portable skill source before installation.",
        )
    })?;
    let body = content
        .strip_prefix("---\n")
        .and_then(|value| {
            value
                .split_once("\n---\n")
                .map(|(frontmatter, _)| frontmatter)
        })
        .ok_or_else(|| {
            diagnostic(
                "SKILL_SOURCE_INVALID",
                "SKILL.md must begin with YAML frontmatter.",
                "Repair the portable skill source before installation.",
            )
        })?;
    let mut keys = BTreeSet::new();
    let mut name = None;
    let mut description = None;
    for line in body.lines() {
        let Some((key, value)) = line.split_once(':') else {
            return Err(diagnostic(
                "SKILL_SOURCE_INVALID",
                "SKILL.md frontmatter must contain simple name and description fields.",
                "Repair the portable skill source before installation.",
            ));
        };
        let key = key.trim();
        let value = value.trim();
        if !keys.insert(key) || !matches!(key, "name" | "description") || value.is_empty() {
            return Err(diagnostic(
                "SKILL_SOURCE_INVALID",
                "SKILL.md frontmatter must contain only one name and one description.",
                "Repair the portable skill source before installation.",
            ));
        }
        match key {
            "name" => name = Some(value),
            "description" => description = Some(value),
            _ => unreachable!(),
        }
    }
    if name != Some(SKILL_NAME) || description.is_none() || keys.len() != 2 {
        return Err(diagnostic(
            "SKILL_SOURCE_INVALID",
            "SKILL.md frontmatter does not identify the portable pkgshift skill.",
            "Repair the portable skill source before installation.",
        ));
    }
    Ok(())
}

pub(super) fn load_bundle(source_path: &Path) -> Result<SkillBundle, Diagnostic> {
    let (source_path, files) = read_directory(source_path).map_err(|mut entry| {
        if entry.code == "SKILL_SOURCE_NOT_FOUND" {
            entry.summary = format!(
                "Portable skill source was not found: {}",
                source_path.display()
            );
            entry.remediation =
                vec!["Run the command from a complete pkgshift distribution.".to_owned()];
        }
        entry
    })?;
    validate_frontmatter(&files)?;
    Ok(SkillBundle {
        source_path,
        digest: digest_files(&files),
        files,
    })
}

pub(super) fn directory_digest(path: &Path) -> Result<String, Diagnostic> {
    let (_, files) = read_directory(path)?;
    Ok(digest_files(&files))
}

fn candidate_paths(project_root: &Path) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(executable) = std::env::current_exe().and_then(fs::canonicalize)
        && let Some(parent) = executable.parent()
    {
        candidates.push(parent.join("skills/pkgshift"));
        candidates.push(parent.join("../share/pkgshift/skills/pkgshift"));
        candidates.push(parent.join("../lib/pkgshift/skills/pkgshift"));
        for ancestor in parent.ancestors().take(5) {
            candidates.push(ancestor.join("skills/pkgshift"));
        }
        if let Some(data_root) = std::env::var_os("PKGSHIFT_DATA_DIR") {
            candidates.push(PathBuf::from(data_root).join("skills/pkgshift"));
        }
        if let Some(data_root) = std::env::var_os("XDG_DATA_HOME") {
            candidates.push(PathBuf::from(data_root).join("pkgshift/skills/pkgshift"));
        }
        if let Some(home) = std::env::var_os("HOME") {
            candidates.push(PathBuf::from(home).join(".local/share/pkgshift/skills/pkgshift"));
        }
        if let Some(local_data) = std::env::var_os("LOCALAPPDATA") {
            candidates.push(PathBuf::from(local_data).join("pkgshift/skills/pkgshift"));
        }
    }
    candidates.push(project_root.join("skills/pkgshift"));
    candidates
}

pub(super) fn resolve_source(project_root: &Path) -> PathBuf {
    let candidates = candidate_paths(project_root);
    for candidate in &candidates {
        if candidate.join("SKILL.md").is_file() {
            return candidate.clone();
        }
    }
    candidates
        .into_iter()
        .next()
        .unwrap_or_else(|| project_root.join("skills/pkgshift"))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::load_bundle;

    #[test]
    fn validates_and_hashes_a_portable_skill() {
        let root = tempfile::tempdir().expect("temporary directory");
        fs::write(
            root.path().join("SKILL.md"),
            "---\nname: pkgshift\ndescription: Safe migrations.\n---\n\n# pkgshift\n",
        )
        .expect("skill source");
        let bundle = load_bundle(root.path()).expect("valid bundle");
        assert!(bundle.digest.starts_with("sha256:"));
        assert_eq!(bundle.files.len(), 1);
    }
}
