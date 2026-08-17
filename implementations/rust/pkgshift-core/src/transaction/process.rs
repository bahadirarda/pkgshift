use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::model::ProcessExecutionRecord;
use crate::util::{PkgshiftError, Result};

fn withheld_output(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        String::new()
    } else {
        format!("<{} bytes withheld by pkgshift>", bytes.len())
    }
}

fn duration_millis(started: Instant) -> u64 {
    started.elapsed().as_millis().try_into().unwrap_or(u64::MAX)
}

pub(super) fn run_process(
    root: &Path,
    operation_id: &str,
    argv: &[String],
) -> Result<ProcessExecutionRecord> {
    let (program, arguments) = argv.split_first().ok_or_else(|| {
        PkgshiftError::InvalidState("process operation has an empty command".to_owned())
    })?;
    let started = Instant::now();
    let output = Command::new(program)
        .args(arguments)
        .current_dir(root)
        .env("npm_config_ignore_scripts", "true")
        .env("YARN_ENABLE_SCRIPTS", "false")
        .env("BUN_INSTALL_IGNORE_SCRIPTS", "1")
        .output()
        .map_err(|source| PkgshiftError::Process(format!("could not start {program}: {source}")))?;
    Ok(ProcessExecutionRecord {
        operation_id: operation_id.to_owned(),
        argv: argv.to_vec(),
        exit_code: output.status.code(),
        stdout: withheld_output(&output.stdout),
        stderr: withheld_output(&output.stderr),
        success: output.status.success(),
        timed_out: false,
        duration_millis: Some(duration_millis(started)),
    })
}

pub(super) fn run_bounded_process(
    root: &Path,
    operation_id: &str,
    argv: &[String],
    timeout_seconds: u64,
) -> Result<ProcessExecutionRecord> {
    let (program, arguments) = argv.split_first().ok_or_else(|| {
        PkgshiftError::InvalidState("process operation has an empty command".to_owned())
    })?;
    let mut stdout = tempfile::tempfile().map_err(temporary_file_error)?;
    let mut stderr = tempfile::tempfile().map_err(temporary_file_error)?;
    let stdout_writer = stdout.try_clone().map_err(temporary_file_error)?;
    let stderr_writer = stderr.try_clone().map_err(temporary_file_error)?;
    let started = Instant::now();
    let mut child = Command::new(program)
        .args(arguments)
        .current_dir(root)
        .stdout(Stdio::from(stdout_writer))
        .stderr(Stdio::from(stderr_writer))
        .spawn()
        .map_err(|source| PkgshiftError::Process(format!("could not start {program}: {source}")))?;
    let (status, timed_out) = wait_for_process(&mut child, program, timeout_seconds, started)?;
    let stdout_bytes = read_output(&mut stdout)?;
    let stderr_bytes = read_output(&mut stderr)?;
    Ok(ProcessExecutionRecord {
        operation_id: operation_id.to_owned(),
        argv: argv.to_vec(),
        exit_code: status.code(),
        stdout: withheld_output(&stdout_bytes),
        stderr: withheld_output(&stderr_bytes),
        success: status.success() && !timed_out,
        timed_out,
        duration_millis: Some(duration_millis(started)),
    })
}

fn wait_for_process(
    child: &mut std::process::Child,
    program: &str,
    timeout_seconds: u64,
    started: Instant,
) -> Result<(std::process::ExitStatus, bool)> {
    let timeout = Duration::from_secs(timeout_seconds);
    loop {
        if let Some(status) = child.try_wait().map_err(|source| {
            PkgshiftError::Process(format!("could not observe {program}: {source}"))
        })? {
            return Ok((status, false));
        }
        if started.elapsed() >= timeout {
            if let Some(status) = child.try_wait().map_err(|source| {
                PkgshiftError::Process(format!("could not observe {program}: {source}"))
            })? {
                return Ok((status, false));
            }
            child.kill().map_err(|source| {
                PkgshiftError::Process(format!("could not stop timed-out {program}: {source}"))
            })?;
            let status = child.wait().map_err(|source| {
                PkgshiftError::Process(format!("could not reap timed-out {program}: {source}"))
            })?;
            return Ok((status, true));
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}

fn read_output(file: &mut std::fs::File) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    file.seek(SeekFrom::Start(0))
        .and_then(|_| file.read_to_end(&mut bytes))
        .map_err(temporary_file_error)?;
    Ok(bytes)
}

fn temporary_file_error(source: std::io::Error) -> PkgshiftError {
    PkgshiftError::Io {
        path: std::env::temp_dir(),
        source,
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn withholds_bounded_process_output() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let record = run_bounded_process(
            directory.path(),
            "op_001",
            &["printf".to_owned(), "secret".to_owned()],
            1,
        )
        .expect("bounded process");

        assert!(record.success);
        assert_eq!(record.stdout, "<6 bytes withheld by pkgshift>");
        assert!(!record.stdout.contains("secret"));
        assert!(!record.timed_out);
    }

    #[test]
    fn stops_a_bounded_process_at_its_deadline() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let record = run_bounded_process(
            directory.path(),
            "op_001",
            &["sleep".to_owned(), "5".to_owned()],
            0,
        )
        .expect("bounded process");

        assert!(!record.success);
        assert!(record.timed_out);
        assert!(
            record
                .duration_millis
                .is_some_and(|duration| duration < 1_000)
        );
    }
}
