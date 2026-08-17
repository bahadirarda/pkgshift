use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

use super::{TransformResult, blocking};
use crate::runtime::model::DenoPermission;

fn source_entry(value: &str) -> bool {
    [".js", ".jsx", ".mjs", ".cjs", ".ts", ".tsx", ".mts", ".cts"]
        .iter()
        .any(|extension| value.ends_with(extension))
}

fn permission_flags(permissions: &BTreeSet<DenoPermission>) -> String {
    permissions
        .iter()
        .map(|permission| permission.flag())
        .collect::<Vec<_>>()
        .join(" ")
}

fn deno_run(entry: &str, watch: bool, permissions: &BTreeSet<DenoPermission>) -> String {
    let mut parts = vec!["deno", "run"];
    if watch {
        parts.push("--watch");
    }
    let flags = permission_flags(permissions);
    let mut command = parts.join(" ");
    if !flags.is_empty() {
        command.push(' ');
        command.push_str(&flags);
    }
    command.push(' ');
    command.push_str(entry);
    command
}

fn transform_script(
    command: &str,
    permissions: &BTreeSet<DenoPermission>,
) -> Option<Result<String, ()>> {
    let tokens = command.split_whitespace().collect::<Vec<_>>();
    let mentions_bun = tokens
        .iter()
        .any(|token| *token == "bun" || *token == "bunx");
    if !mentions_bun {
        return None;
    }
    if command.chars().any(|value| "&|;><`$".contains(value)) {
        return Some(Err(()));
    }
    let transformed = match tokens.as_slice() {
        ["bun", "test"] => {
            let flags = permission_flags(permissions);
            if flags.is_empty() {
                "deno test".to_owned()
            } else {
                format!("deno test {flags}")
            }
        }
        ["bun", "run", "--hot", entry] | ["bun", "--hot", entry] if source_entry(entry) => {
            deno_run(entry, true, permissions)
        }
        ["bun", "run", entry] | ["bun", entry] if source_entry(entry) => {
            deno_run(entry, false, permissions)
        }
        ["bun", "run", task]
            if task
                .chars()
                .all(|value| value.is_ascii_alphanumeric() || "_-:.".contains(value)) =>
        {
            format!("deno task {task}")
        }
        _ => return Some(Err(())),
    };
    Some(Ok(transformed))
}

pub(super) fn transform(
    path: &str,
    content: &str,
    permissions: &BTreeSet<DenoPermission>,
) -> TransformResult {
    let normalized = crate::util::strip_json_comments_and_trailing_commas(content);
    let Ok(mut value) = serde_json::from_str::<Value>(&normalized) else {
        return TransformResult {
            content: content.to_owned(),
            counts: BTreeMap::new(),
            diagnostics: vec![blocking(
                "RUNTIME_MANIFEST_INVALID",
                path,
                "package.json could not be parsed for runtime recipes.",
                "Repair the manifest before retrying the runtime migration.",
            )],
            required_permissions: BTreeSet::new(),
        };
    };
    let Some(object) = value.as_object_mut() else {
        return TransformResult {
            content: content.to_owned(),
            counts: BTreeMap::new(),
            diagnostics: vec![blocking(
                "RUNTIME_MANIFEST_INVALID",
                path,
                "package.json must contain a JSON object.",
                "Repair the manifest before retrying the runtime migration.",
            )],
            required_permissions: BTreeSet::new(),
        };
    };
    let mut dependency_count = 0;
    for section in [
        "dependencies",
        "devDependencies",
        "optionalDependencies",
        "peerDependencies",
    ] {
        if let Some(entries) = object.get_mut(section).and_then(Value::as_object_mut) {
            for dependency in ["bun-types", "@types/bun"] {
                if entries.remove(dependency).is_some() {
                    dependency_count += 1;
                }
            }
        }
    }
    let mut script_count = 0;
    let mut diagnostics = Vec::new();
    if let Some(scripts) = object.get_mut("scripts").and_then(Value::as_object_mut) {
        for (name, value) in scripts {
            let Some(command) = value.as_str() else {
                continue;
            };
            match transform_script(command, permissions) {
                Some(Ok(transformed)) => {
                    *value = Value::String(transformed);
                    script_count += 1;
                }
                Some(Err(())) => diagnostics.push(blocking(
                    "RUNTIME_BUN_SCRIPT_UNSUPPORTED",
                    &format!("{path}#/scripts/{name}"),
                    "A package script mixes Bun with unsupported flags or shell semantics.",
                    "Rewrite the script as a direct deno run, deno task, or deno test command.",
                )),
                None => {}
            }
        }
    }
    let changed = dependency_count + script_count;
    let transformed = if changed == 0 {
        content.to_owned()
    } else {
        let mut serialized = serde_json::to_string_pretty(&value)
            .expect("serializing a parsed JSON value cannot fail");
        serialized.push('\n');
        serialized
    };
    let mut counts = BTreeMap::new();
    if dependency_count > 0 {
        counts.insert("bun.runtime-types-remove", dependency_count);
    }
    if script_count > 0 {
        counts.insert("bun.scripts-to-deno", script_count);
    }
    TransformResult {
        content: transformed,
        counts,
        diagnostics,
        required_permissions: BTreeSet::new(),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::transform;
    use crate::runtime::model::DenoPermission;

    #[test]
    fn rewrites_hono_script_with_explicit_permissions() {
        let result = transform(
            "package.json",
            r#"{"scripts":{"dev":"bun run --hot src/index.ts"},"devDependencies":{"@types/bun":"latest"}}"#,
            &BTreeSet::from([DenoPermission::Net]),
        );
        assert!(
            result
                .content
                .contains("deno run --watch --allow-net src/index.ts")
        );
        assert!(!result.content.contains("@types/bun"));
        assert!(result.diagnostics.is_empty());
    }
}
