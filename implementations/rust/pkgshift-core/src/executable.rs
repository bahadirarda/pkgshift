use std::env;
use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::model::{ExecutableRequirement, ResolvedExecutable};
use crate::util::{PkgshiftError, Result};

const VERSION_PROBE_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_VERSION_OUTPUT_BYTES: usize = 64 * 1024;

fn candidate_is_executable(path: &Path) -> bool {
    let Ok(metadata) = fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

#[cfg(windows)]
fn candidate_names(program: &str) -> Vec<String> {
    if Path::new(program).extension().is_some() {
        return vec![program.to_owned()];
    }
    let extensions = env::var_os("PATHEXT")
        .map(|value| {
            value
                .to_string_lossy()
                .split(';')
                .filter(|entry| !entry.is_empty())
                .map(str::to_ascii_lowercase)
                .collect::<Vec<_>>()
        })
        .unwrap_or_else(|| vec![".exe".to_owned(), ".cmd".to_owned(), ".bat".to_owned()]);
    std::iter::once(program.to_owned())
        .chain(
            extensions
                .into_iter()
                .map(|extension| format!("{program}{extension}")),
        )
        .collect()
}

#[cfg(not(windows))]
fn candidate_names(program: &str) -> Vec<String> {
    vec![program.to_owned()]
}

fn locate(program: &str) -> Result<PathBuf> {
    let direct = Path::new(program);
    if direct.components().count() > 1 {
        let path = fs::canonicalize(direct).map_err(|_| {
            PkgshiftError::ExecutableUnavailable(format!(
                "{program} does not resolve to an executable file"
            ))
        })?;
        if !candidate_is_executable(&path) {
            return Err(PkgshiftError::ExecutableUnavailable(format!(
                "{program} does not resolve to an executable file"
            )));
        }
        return Ok(path);
    }
    let path = env::var_os("PATH").ok_or_else(|| {
        PkgshiftError::ExecutableUnavailable(format!(
            "{program} cannot be resolved because PATH is not set"
        ))
    })?;
    for directory in env::split_paths(&path) {
        for name in candidate_names(program) {
            let candidate = directory.join(name);
            if candidate_is_executable(&candidate) {
                return fs::canonicalize(&candidate).map_err(|source| {
                    PkgshiftError::ExecutableUnavailable(format!(
                        "{} could not be canonicalized: {source}",
                        candidate.display()
                    ))
                });
            }
        }
    }
    Err(PkgshiftError::ExecutableUnavailable(format!(
        "{program} was not found on PATH"
    )))
}

fn temporary_file_error(source: std::io::Error) -> PkgshiftError {
    PkgshiftError::Io {
        path: env::temp_dir(),
        source,
    }
}

fn read_output(file: &mut fs::File) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    file.seek(SeekFrom::Start(0))
        .and_then(|_| file.read_to_end(&mut bytes))
        .map_err(temporary_file_error)?;
    if bytes.len() > MAX_VERSION_OUTPUT_BYTES {
        return Err(PkgshiftError::ExecutableVersion(
            "version probe output exceeded the safety limit".to_owned(),
        ));
    }
    Ok(bytes)
}

fn probe(path: &Path, root: &Path, arguments: &[String]) -> Result<Vec<u8>> {
    let mut stdout = tempfile::tempfile().map_err(temporary_file_error)?;
    let mut stderr = tempfile::tempfile().map_err(temporary_file_error)?;
    let stdout_writer = stdout.try_clone().map_err(temporary_file_error)?;
    let stderr_writer = stderr.try_clone().map_err(temporary_file_error)?;
    let mut child = Command::new(path)
        .args(arguments)
        .current_dir(root)
        .env("npm_config_ignore_scripts", "true")
        .env("YARN_ENABLE_SCRIPTS", "false")
        .env("BUN_INSTALL_IGNORE_SCRIPTS", "1")
        .stdout(Stdio::from(stdout_writer))
        .stderr(Stdio::from(stderr_writer))
        .spawn()
        .map_err(|source| {
            PkgshiftError::ExecutableVersion(format!(
                "{} could not start its version probe: {source}",
                path.display()
            ))
        })?;
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().map_err(|source| {
            PkgshiftError::ExecutableVersion(format!(
                "{} version probe could not be observed: {source}",
                path.display()
            ))
        })? {
            break status;
        }
        if started.elapsed() >= VERSION_PROBE_TIMEOUT {
            child.kill().map_err(|source| {
                PkgshiftError::ExecutableVersion(format!(
                    "{} version probe could not be stopped: {source}",
                    path.display()
                ))
            })?;
            child.wait().map_err(|source| {
                PkgshiftError::ExecutableVersion(format!(
                    "{} version probe could not be reaped: {source}",
                    path.display()
                ))
            })?;
            return Err(PkgshiftError::ExecutableVersion(format!(
                "{} version probe exceeded five seconds",
                path.display()
            )));
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let mut output = read_output(&mut stdout)?;
    output.extend(read_output(&mut stderr)?);
    if !status.success() {
        return Err(PkgshiftError::ExecutableVersion(format!(
            "{} exited unsuccessfully while reporting its version",
            path.display()
        )));
    }
    Ok(output)
}

fn version_tokens(output: &[u8]) -> Vec<String> {
    String::from_utf8_lossy(output)
        .split_whitespace()
        .map(|token| {
            token.trim_matches(|character: char| {
                !character.is_ascii_alphanumeric() && !matches!(character, '.' | '-' | '+' | '_')
            })
        })
        .map(|token| token.strip_prefix('v').unwrap_or(token))
        .filter(|token| token.starts_with(|character: char| character.is_ascii_digit()))
        .map(str::to_owned)
        .collect()
}

pub(crate) fn resolve(
    root: &Path,
    requirement: &ExecutableRequirement,
) -> Result<ResolvedExecutable> {
    let path = locate(&requirement.program)?;
    let arguments = requirement
        .version_command
        .get(1..)
        .ok_or_else(|| PkgshiftError::ExecutableVersion("version command is empty".to_owned()))?;
    let output = probe(&path, root, arguments)?;
    let tokens = version_tokens(&output);
    let version = tokens
        .iter()
        .find(|token| **token == requirement.required_version)
        .cloned()
        .ok_or_else(|| {
            let reported = tokens.first().map_or("unrecognized", String::as_str);
            PkgshiftError::ExecutableVersion(format!(
                "{} requires version {}, but the resolved executable reported {reported}",
                requirement.program, requirement.required_version
            ))
        })?;
    Ok(ResolvedExecutable {
        program: requirement.program.clone(),
        path: path.to_string_lossy().into_owned(),
        version,
        package_manager_pin: requirement.package_manager_pin.clone(),
    })
}

#[cfg(all(test, unix))]
mod tests {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    use super::*;

    fn requirement(program: &Path) -> ExecutableRequirement {
        ExecutableRequirement {
            program: program.to_string_lossy().into_owned(),
            required_version: "1.2.3".to_owned(),
            version_command: vec![
                program.to_string_lossy().into_owned(),
                "--version".to_owned(),
            ],
            package_manager_pin: "fixture-pm@1.2.3".to_owned(),
        }
    }

    #[test]
    fn resolves_and_validates_an_exact_version_without_exposing_output() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let binary = directory.path().join("fixture-pm");
        fs::write(
            &binary,
            "#!/bin/sh\nprintf 'fixture-pm 1.2.3 secret-data\\n'\n",
        )
        .expect("fixture executable");
        fs::set_permissions(&binary, fs::Permissions::from_mode(0o755))
            .expect("fixture permissions");
        let resolved =
            resolve(directory.path(), &requirement(&binary)).expect("resolved executable");
        assert_eq!(resolved.version, "1.2.3");
        assert!(!format!("{resolved:?}").contains("secret-data"));
    }

    #[test]
    fn rejects_a_different_resolved_version() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let binary = directory.path().join("fixture-pm");
        fs::write(&binary, "#!/bin/sh\nprintf '2.0.0\\n'\n").expect("fixture executable");
        fs::set_permissions(&binary, fs::Permissions::from_mode(0o755))
            .expect("fixture permissions");
        let error = resolve(directory.path(), &requirement(&binary)).expect_err("version mismatch");
        assert!(error.to_string().contains("requires version 1.2.3"));
        assert!(!error.to_string().contains("secret-data"));
    }
}
