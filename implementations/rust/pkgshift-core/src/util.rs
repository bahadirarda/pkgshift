use std::collections::BTreeMap;
use std::fmt::Write as FmtWrite;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PkgshiftError {
    #[error("filesystem operation failed for {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("JSON parsing failed for {path}: {source}")]
    Json {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("unsafe repository path: {0}")]
    UnsafePath(String),
    #[error("invalid migration state: {0}")]
    InvalidState(String),
    #[error("process execution failed: {0}")]
    Process(String),
}

pub type Result<T> = std::result::Result<T, PkgshiftError>;

pub fn resolve_root(path: &Path) -> Result<PathBuf> {
    let root = fs::canonicalize(path).map_err(|source| PkgshiftError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    if !root.is_dir() {
        return Err(PkgshiftError::UnsafePath(format!(
            "{} is not a directory",
            root.display()
        )));
    }
    Ok(root)
}

pub fn safe_join(root: &Path, relative: &str) -> Result<PathBuf> {
    let path = Path::new(relative);
    if path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, Component::ParentDir | Component::Prefix(_)))
    {
        return Err(PkgshiftError::UnsafePath(relative.to_owned()));
    }
    Ok(root.join(path))
}

pub fn read_text(path: &Path) -> Result<Option<String>> {
    match fs::read_to_string(path) {
        Ok(content) => Ok(Some(content)),
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(PkgshiftError::Io {
            path: path.to_path_buf(),
            source,
        }),
    }
}

pub fn read_json_object(path: &Path) -> Result<Option<Map<String, Value>>> {
    let Some(content) = read_text(path)? else {
        return Ok(None);
    };
    let content = content.strip_prefix('\u{feff}').unwrap_or(&content);
    let normalized = strip_json_comments_and_trailing_commas(content);
    let value: Value = serde_json::from_str(&normalized).map_err(|source| PkgshiftError::Json {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(value.as_object().cloned())
}

pub fn strip_json_comments_and_trailing_commas(content: &str) -> String {
    let mut output = String::with_capacity(content.len());
    let chars: Vec<char> = content.chars().collect();
    let mut index = 0;
    let mut in_string = false;
    let mut escaped = false;
    while index < chars.len() {
        let current = chars[index];
        if in_string {
            output.push(current);
            if escaped {
                escaped = false;
            } else if current == '\\' {
                escaped = true;
            } else if current == '"' {
                in_string = false;
            }
            index += 1;
            continue;
        }
        if current == '"' {
            in_string = true;
            output.push(current);
            index += 1;
            continue;
        }
        if current == '/' && chars.get(index + 1) == Some(&'/') {
            index += 2;
            while index < chars.len() && chars[index] != '\n' {
                index += 1;
            }
            continue;
        }
        if current == '/' && chars.get(index + 1) == Some(&'*') {
            index += 2;
            while index + 1 < chars.len() && !(chars[index] == '*' && chars[index + 1] == '/') {
                index += 1;
            }
            index = (index + 2).min(chars.len());
            continue;
        }
        output.push(current);
        index += 1;
    }

    let chars: Vec<char> = output.chars().collect();
    let mut cleaned = String::with_capacity(output.len());
    let mut index = 0;
    let mut in_string = false;
    let mut escaped = false;
    while index < chars.len() {
        let current = chars[index];
        if in_string {
            cleaned.push(current);
            if escaped {
                escaped = false;
            } else if current == '\\' {
                escaped = true;
            } else if current == '"' {
                in_string = false;
            }
            index += 1;
            continue;
        }
        if current == '"' {
            in_string = true;
            cleaned.push(current);
            index += 1;
            continue;
        }
        if current == ',' {
            let mut next = index + 1;
            while next < chars.len() && chars[next].is_whitespace() {
                next += 1;
            }
            if matches!(chars.get(next), Some('}' | ']')) {
                index += 1;
                continue;
            }
        }
        cleaned.push(current);
        index += 1;
    }
    cleaned
}

pub fn digest_bytes(content: &[u8]) -> String {
    format!("sha256:{}", hex_lower(&Sha256::digest(content)))
}

pub fn hex_lower(content: &[u8]) -> String {
    let mut output = String::with_capacity(content.len() * 2);
    for byte in content {
        write!(&mut output, "{byte:02x}").expect("writing to a string cannot fail");
    }
    output
}

pub fn digest_text(content: &str) -> String {
    digest_bytes(content.as_bytes())
}

pub fn digest_json<T: Serialize>(value: &T) -> Result<String> {
    let value = serde_json::to_value(value).map_err(|source| PkgshiftError::Json {
        path: PathBuf::from("<memory>"),
        source,
    })?;
    let canonical = canonicalize_json(value);
    let content = serde_json::to_vec(&canonical).map_err(|source| PkgshiftError::Json {
        path: PathBuf::from("<memory>"),
        source,
    })?;
    Ok(digest_bytes(&content))
}

pub fn short_digest<T: Serialize>(prefix: &str, value: &T) -> Result<String> {
    let digest = digest_json(value)?;
    Ok(format!(
        "{prefix}{}",
        &digest["sha256:".len().."sha256:".len() + 24]
    ))
}

fn canonicalize_json(value: Value) -> Value {
    match value {
        Value::Array(values) => Value::Array(values.into_iter().map(canonicalize_json).collect()),
        Value::Object(values) => {
            let sorted = values
                .into_iter()
                .map(|(key, value)| (key, canonicalize_json(value)))
                .collect::<BTreeMap<_, _>>();
            Value::Object(sorted.into_iter().collect())
        }
        other => other,
    }
}

pub fn file_digest(path: &Path) -> Result<Option<String>> {
    match fs::read(path) {
        Ok(content) => Ok(Some(digest_bytes(&content))),
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(PkgshiftError::Io {
            path: path.to_path_buf(),
            source,
        }),
    }
}

pub fn atomic_write(path: &Path, content: &[u8]) -> Result<()> {
    let parent = path.parent().ok_or_else(|| {
        PkgshiftError::UnsafePath(format!("{} has no parent directory", path.display()))
    })?;
    fs::create_dir_all(parent).map_err(|source| PkgshiftError::Io {
        path: parent.to_path_buf(),
        source,
    })?;
    let suffix = format!("{}.{}", std::process::id(), unix_timestamp_millis());
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("pkgshift");
    let temporary = parent.join(format!(".{name}.{suffix}.tmp"));
    let write_result = (|| -> Result<()> {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)
            .map_err(|source| PkgshiftError::Io {
                path: temporary.clone(),
                source,
            })?;
        file.write_all(content)
            .and_then(|()| file.sync_all())
            .map_err(|source| PkgshiftError::Io {
                path: temporary.clone(),
                source,
            })?;
        fs::rename(&temporary, path).map_err(|source| PkgshiftError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        Ok(())
    })();
    if write_result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    write_result
}

pub fn write_private_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let content = serde_json::to_vec_pretty(value).map_err(|source| PkgshiftError::Json {
        path: path.to_path_buf(),
        source,
    })?;
    atomic_write(path, &content)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600)).map_err(|source| {
            PkgshiftError::Io {
                path: path.to_path_buf(),
                source,
            }
        })?;
    }
    Ok(())
}

pub fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    let content = fs::read(path).map_err(|source| PkgshiftError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    serde_json::from_slice(&content).map_err(|source| PkgshiftError::Json {
        path: path.to_path_buf(),
        source,
    })
}

pub fn walk_files(root: &Path) -> Result<Vec<String>> {
    fn visit(root: &Path, directory: &Path, output: &mut Vec<String>) -> Result<()> {
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
        entries.sort_by_key(std::fs::DirEntry::file_name);
        for entry in entries {
            let file_type = entry.file_type().map_err(|source| PkgshiftError::Io {
                path: entry.path(),
                source,
            })?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if file_type.is_dir() {
                if [".git", ".pkgshift", "node_modules", "target"].contains(&name.as_ref()) {
                    continue;
                }
                visit(root, &entry.path(), output)?;
            } else if file_type.is_file() {
                let relative = entry
                    .path()
                    .strip_prefix(root)
                    .map_err(|_| PkgshiftError::UnsafePath(entry.path().display().to_string()))?
                    .to_string_lossy()
                    .replace('\\', "/");
                output.push(relative);
            }
        }
        Ok(())
    }

    let mut files = Vec::new();
    visit(root, root, &mut files)?;
    files.sort();
    Ok(files)
}

pub fn redact_sensitive_text(path: &str, content: &str) -> String {
    if path.ends_with(".npmrc") {
        return content
            .lines()
            .map(|line| {
                let Some((key, _)) = line.split_once('=') else {
                    return line.to_owned();
                };
                let normalized = key.to_ascii_lowercase();
                if normalized.contains("token")
                    || normalized.contains("password")
                    || normalized.contains("_auth")
                {
                    format!("{key}=<redacted>")
                } else {
                    line.to_owned()
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
    }
    content.to_owned()
}

pub fn unix_timestamp_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

pub fn create_new_lock(path: &Path, content: &str) -> Result<File> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| PkgshiftError::Io {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .and_then(|mut file| {
            file.write_all(content.as_bytes())?;
            file.sync_all()?;
            Ok(file)
        })
        .map_err(|source| PkgshiftError::Io {
            path: path.to_path_buf(),
            source,
        })
}
