use std::path::Path;

use serde_json::json;

use super::artifact::load;
use super::catalog::find;
use crate::command::{CommandOptions, artifact, resolve_state_directory, result, summary};
use crate::model::{CommandExecution, CommandStatus, Diagnostic, DiagnosticSeverity};
use crate::util::{PkgshiftError, Result, resolve_root};

const ARTIFACT_PREFIXES: &[&str] = &[
    "plan_",
    "run_",
    "verification_",
    "runtime_plan_",
    "runtime_run_",
    "runtime_verification_",
];

fn looks_like_artifact(identifier: &str) -> bool {
    ARTIFACT_PREFIXES
        .iter()
        .any(|prefix| identifier.starts_with(prefix))
}

fn blocked(identifier: &str, code: &str, message: String, remediation: &str) -> CommandExecution {
    CommandExecution {
        exit_code: if code == "DIAGNOSTIC_CODE_UNKNOWN" {
            2
        } else {
            4
        },
        result: result(
            "explain",
            CommandStatus::Blocked,
            summary([("identifier", json!(identifier))]),
            vec![Diagnostic {
                code: code.to_owned(),
                severity: DiagnosticSeverity::Error,
                summary: message,
                blocking: true,
                evidence: Vec::new(),
                remediation: vec![remediation.to_owned()],
            }],
            Vec::new(),
            None,
            None,
            Vec::new(),
        ),
    }
}

fn artifact_failure(identifier: &str, error: &PkgshiftError) -> CommandExecution {
    let not_found = matches!(
        error,
        PkgshiftError::Io { source, .. } if source.kind() == std::io::ErrorKind::NotFound
    );
    if not_found {
        blocked(
            identifier,
            "ARTIFACT_NOT_FOUND",
            format!("Stored artifact not found: {identifier}"),
            "Confirm the identifier and select the state directory that created it.",
        )
    } else {
        blocked(
            identifier,
            "ARTIFACT_INVALID",
            format!("Stored artifact failed integrity validation: {identifier}"),
            "Do not trust the artifact; preserve state and create a fresh one.",
        )
    }
}

fn state_directory(cwd: &Path, configured: Option<&Path>) -> Result<std::path::PathBuf> {
    let root = resolve_root(cwd)?;
    Ok(resolve_state_directory(&root, configured))
}

pub(crate) fn explain_command(
    options: &CommandOptions,
    identifier: &str,
) -> Result<CommandExecution> {
    if let Some(explanation) = find(identifier) {
        return Ok(CommandExecution {
            exit_code: 0,
            result: result(
                "explain",
                CommandStatus::Completed,
                summary([
                    ("code", json!(explanation.code)),
                    ("title", json!(explanation.title)),
                    ("readOnly", json!(true)),
                ]),
                Vec::new(),
                vec![artifact(
                    format!("explanation_{}", explanation.code),
                    "diagnostic-explanation",
                    "application/vnd.pkgshift.diagnostic+json",
                    explanation,
                )?],
                None,
                None,
                Vec::new(),
            ),
        });
    }

    if !looks_like_artifact(identifier) {
        return Ok(blocked(
            identifier,
            "DIAGNOSTIC_CODE_UNKNOWN",
            format!("Unknown diagnostic code: {identifier}"),
            "Use a diagnostic code or stored artifact identifier returned by this CLI schema.",
        ));
    }

    let state_directory = state_directory(&options.cwd, options.state_directory.as_deref())?;
    let loaded = match load(&state_directory, identifier) {
        Ok(Some(loaded)) => loaded,
        Ok(None) => {
            return Ok(blocked(
                identifier,
                "ARTIFACT_NOT_FOUND",
                format!("Stored artifact not found: {identifier}"),
                "Confirm the identifier and select the state directory that created it.",
            ));
        }
        Err(error) => return Ok(artifact_failure(identifier, &error)),
    };
    let mut entries = vec![
        ("artifact", json!(identifier)),
        ("type", json!(loaded.kind)),
        ("readOnly", json!(true)),
    ];
    if let Some(status) = &loaded.status {
        entries.push(("artifactStatus", json!(status)));
    }
    Ok(CommandExecution {
        exit_code: 0,
        result: result(
            "explain",
            CommandStatus::Completed,
            summary(entries),
            loaded.diagnostics,
            loaded.artifacts,
            loaded.plan_id,
            loaded.run_id,
            Vec::new(),
        ),
    })
}
