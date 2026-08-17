use std::fs;
use std::path::Path;

use crate::util::{PkgshiftError, Result};

const IGNORED_DIRECTORIES: &[&str] = &[".git", ".pkgshift", "node_modules", "target"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RuntimeEntryKind {
    File,
    Symlink { target_is_directory: bool },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RuntimeEntry {
    pub path: String,
    pub kind: RuntimeEntryKind,
}

fn relative_path(root: &Path, path: &Path) -> Result<String> {
    Ok(path
        .strip_prefix(root)
        .map_err(|_| PkgshiftError::UnsafePath(path.display().to_string()))?
        .to_string_lossy()
        .replace('\\', "/"))
}

fn visit(root: &Path, directory: &Path, output: &mut Vec<RuntimeEntry>) -> Result<()> {
    let mut entries = fs::read_dir(directory)
        .map_err(|source| PkgshiftError::Io {
            path: directory.to_path_buf(),
            source,
        })?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|source| PkgshiftError::Io {
            path: directory.to_path_buf(),
            source,
        })?;
    entries.sort_by_key(fs::DirEntry::file_name);

    for entry in entries {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let file_type = entry.file_type().map_err(|source| PkgshiftError::Io {
            path: entry.path(),
            source,
        })?;
        if file_type.is_dir() {
            if !IGNORED_DIRECTORIES.contains(&name.as_ref()) {
                visit(root, &entry.path(), output)?;
            }
        } else if file_type.is_file() {
            output.push(RuntimeEntry {
                path: relative_path(root, &entry.path())?,
                kind: RuntimeEntryKind::File,
            });
        } else if file_type.is_symlink() && !IGNORED_DIRECTORIES.contains(&name.as_ref()) {
            let target_is_directory = fs::metadata(entry.path()).is_ok_and(|value| value.is_dir());
            output.push(RuntimeEntry {
                path: relative_path(root, &entry.path())?,
                kind: RuntimeEntryKind::Symlink {
                    target_is_directory,
                },
            });
        }
    }
    Ok(())
}

pub(super) fn runtime_entries(root: &Path) -> Result<Vec<RuntimeEntry>> {
    let mut entries = Vec::new();
    visit(root, root, &mut entries)?;
    entries.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{RuntimeEntryKind, runtime_entries};

    #[cfg(unix)]
    #[test]
    fn reports_symlinks_without_following_them() {
        use std::os::unix::fs::symlink;

        let root = tempfile::tempdir().expect("temporary directory");
        fs::create_dir(root.path().join("external")).expect("external directory");
        fs::write(root.path().join("external/server.ts"), "Bun.serve({});")
            .expect("external source");
        symlink("external", root.path().join("linked-source")).expect("directory symlink");

        let entries = runtime_entries(root.path()).expect("entries");
        assert!(entries.iter().any(|entry| {
            entry.path == "linked-source"
                && entry.kind
                    == RuntimeEntryKind::Symlink {
                        target_is_directory: true,
                    }
        }));
        assert!(
            !entries
                .iter()
                .any(|entry| entry.path == "linked-source/server.ts")
        );
    }
}
