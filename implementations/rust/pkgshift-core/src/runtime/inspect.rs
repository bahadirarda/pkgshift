mod walk;

use std::path::Path;

use serde_json::Value;

use super::lex::{contains_code, without_comments};
use super::model::{RuntimeFile, RuntimeInspection};
use crate::model::{Diagnostic, DiagnosticSeverity, EvidenceDetail};
use crate::util::{Result, digest_text, read_text, short_digest};
use walk::{RuntimeEntryKind, runtime_entries};

const SOURCE_EXTENSIONS: &[&str] = &[".js", ".jsx", ".mjs", ".cjs", ".ts", ".tsx", ".mts", ".cts"];
const MAX_RUNTIME_FILE_BYTES: u64 = 512_000;

fn source_path(path: &str) -> bool {
    SOURCE_EXTENSIONS
        .iter()
        .any(|extension| path.ends_with(extension))
}

fn runtime_input_path(path: &str) -> bool {
    source_path(path)
        || path == "package.json"
        || path.ends_with("/package.json")
        || path == "bunfig.toml"
        || path.ends_with("/bunfig.toml")
        || path
            .rsplit('/')
            .next()
            .is_some_and(|name| name.starts_with("tsconfig") && name.ends_with(".json"))
}

fn unsafe_input(code: &str, path: &str, summary: &str, remediation: &str) -> Diagnostic {
    Diagnostic {
        code: code.to_owned(),
        severity: DiagnosticSeverity::Error,
        summary: summary.to_owned(),
        blocking: true,
        evidence: vec![EvidenceDetail {
            location: path.to_owned(),
            detail: "The runtime inspection boundary cannot safely read this entry.".to_owned(),
        }],
        remediation: vec![remediation.to_owned()],
    }
}

fn import_specifier_evidence(content: &str) -> bool {
    let content = without_comments(content);
    content.contains("\"bun:")
        || content.contains("'bun:")
        || content.contains(" from \"bun\"")
        || content.contains(" from 'bun'")
}

fn command_token(command: &str, token: &str) -> bool {
    command.match_indices(token).any(|(index, _)| {
        let before = command[..index].chars().next_back();
        let after = command[index + token.len()..].chars().next();
        let boundary = |character: Option<char>| {
            character.is_none_or(|value| !value.is_ascii_alphanumeric() && value != '_')
        };
        boundary(before) && boundary(after)
    })
}

fn manifest_has_bun_runtime(content: &str) -> bool {
    let Ok(value) = serde_json::from_str::<Value>(
        &crate::util::strip_json_comments_and_trailing_commas(content),
    ) else {
        return false;
    };
    let Some(object) = value.as_object() else {
        return false;
    };
    let dependency = [
        "dependencies",
        "devDependencies",
        "optionalDependencies",
        "peerDependencies",
    ]
    .iter()
    .any(|section| {
        object
            .get(*section)
            .and_then(Value::as_object)
            .is_some_and(|entries| {
                entries.contains_key("bun-types") || entries.contains_key("@types/bun")
            })
    });
    let script = object
        .get("scripts")
        .and_then(Value::as_object)
        .is_some_and(|scripts| {
            scripts
                .values()
                .filter_map(Value::as_str)
                .any(|command| command_token(command, "bun") || command_token(command, "bunx"))
        });
    dependency || script
}

fn tsconfig_has_bun_types(content: &str) -> bool {
    let Ok(value) = json5::from_str::<Value>(content) else {
        return false;
    };
    value
        .get("compilerOptions")
        .and_then(|value| value.get("types"))
        .and_then(Value::as_array)
        .is_some_and(|types| {
            types
                .iter()
                .any(|value| matches!(value.as_str(), Some("bun-types" | "@types/bun")))
        })
}

fn bun_evidence(path: &str, content: &str) -> Vec<String> {
    let mut evidence = Vec::new();
    if source_path(path) {
        if contains_code(content, "Bun.") {
            evidence.push(format!("{path}:Bun global API"));
        }
        if import_specifier_evidence(content) {
            evidence.push(format!("{path}:Bun module import"));
        }
    } else if path.ends_with("package.json") && manifest_has_bun_runtime(content) {
        evidence.push(format!("{path}:Bun runtime manifest reference"));
    } else if path.ends_with(".json") && tsconfig_has_bun_types(content) {
        evidence.push(format!("{path}:Bun TypeScript types"));
    } else if path.ends_with("bunfig.toml") {
        evidence.push(format!("{path}:Bun runtime configuration"));
    }
    evidence
}

pub(crate) fn inspect_runtime(root: &Path) -> Result<RuntimeInspection> {
    let mut files = Vec::new();
    let mut evidence = Vec::new();
    let mut input_diagnostics = Vec::new();
    let mut fingerprint_entries = Vec::new();
    for entry in runtime_entries(root)? {
        let path = entry.path;
        if let RuntimeEntryKind::Symlink {
            target_is_directory,
        } = entry.kind
        {
            if target_is_directory || runtime_input_path(&path) {
                fingerprint_entries.push((path.clone(), "unsafe:symlink".to_owned()));
                input_diagnostics.push(unsafe_input(
                    "RUNTIME_SOURCE_SYMLINK_UNSUPPORTED",
                    &path,
                    "A runtime source or source directory is represented by a symbolic link.",
                    "Replace the symlink with repository-owned files before planning the migration.",
                ));
            }
            continue;
        }
        if !runtime_input_path(&path) {
            continue;
        }
        let absolute = root.join(&path);
        let Some(metadata) = std::fs::metadata(&absolute).ok() else {
            continue;
        };
        if metadata.len() > MAX_RUNTIME_FILE_BYTES {
            fingerprint_entries.push((path.clone(), format!("unsafe:size:{}", metadata.len())));
            input_diagnostics.push(unsafe_input(
                "RUNTIME_SOURCE_FILE_TOO_LARGE",
                &path,
                "A runtime input exceeds the deterministic inspection size limit.",
                "Split the file below 512,000 bytes or migrate it explicitly before retrying.",
            ));
            continue;
        }
        let Some(content) = read_text(&absolute)? else {
            continue;
        };
        fingerprint_entries.push((path.clone(), digest_text(&content)));
        evidence.extend(bun_evidence(&path, &content));
        files.push(RuntimeFile { path, content });
    }
    let fingerprint = short_digest("runtime_repo_", &fingerprint_entries)?;
    Ok(RuntimeInspection {
        fingerprint,
        files,
        bun_evidence: evidence,
        input_diagnostics,
    })
}

pub(crate) fn residual_bun_references(root: &Path) -> Result<Vec<String>> {
    let inspection = inspect_runtime(root)?;
    let mut residues = inspection.bun_evidence;
    residues.extend(inspection.input_diagnostics.into_iter().map(|diagnostic| {
        let code = diagnostic.code;
        match diagnostic.evidence.first() {
            Some(evidence) => format!("{}:{code}", evidence.location),
            None => code,
        }
    }));
    Ok(residues)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::inspect_runtime;

    #[test]
    fn ignores_comments_and_package_manager_pin() {
        let root = tempfile::tempdir().expect("temporary directory");
        fs::write(
            root.path().join("package.json"),
            r#"{"packageManager":"bun@1.3.0","scripts":{"test":"deno test"}}"#,
        )
        .expect("manifest");
        fs::write(
            root.path().join("main.ts"),
            "// Bun.serve is documented here\n",
        )
        .expect("source");
        assert!(
            inspect_runtime(root.path())
                .expect("inspection")
                .bun_evidence
                .is_empty()
        );
    }

    #[test]
    fn blocks_oversized_runtime_inputs() {
        let root = tempfile::tempdir().expect("temporary directory");
        fs::write(root.path().join("main.ts"), vec![b'x'; 512_001]).expect("large source");
        let inspection = inspect_runtime(root.path()).expect("inspection");
        assert_eq!(
            inspection.input_diagnostics[0].code,
            "RUNTIME_SOURCE_FILE_TOO_LARGE"
        );
    }

    #[cfg(unix)]
    #[test]
    fn blocks_runtime_source_symlinks() {
        use std::os::unix::fs::symlink;

        let root = tempfile::tempdir().expect("temporary directory");
        fs::write(root.path().join("real.ts"), "Bun.file('value').text();").expect("source");
        symlink("real.ts", root.path().join("linked.ts")).expect("source symlink");
        let inspection = inspect_runtime(root.path()).expect("inspection");
        assert!(
            inspection
                .input_diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "RUNTIME_SOURCE_SYMLINK_UNSUPPORTED")
        );
    }
}
