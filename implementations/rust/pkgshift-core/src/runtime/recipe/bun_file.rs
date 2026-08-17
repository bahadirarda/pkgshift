use std::collections::{BTreeMap, BTreeSet};

use super::{TransformResult, apply_replacements, blocking};
use crate::runtime::lex::{code_mask, code_occurrences};
use crate::runtime::model::DenoPermission;

fn matching_parenthesis(content: &str, open: usize) -> Option<usize> {
    let bytes = content.as_bytes();
    let mask = code_mask(content);
    let mut depth = 0usize;
    for index in open..bytes.len() {
        if !mask[index] {
            continue;
        }
        if bytes[index] == b'(' {
            depth += 1;
        } else if bytes[index] == b')' {
            depth -= 1;
            if depth == 0 {
                return Some(index);
            }
        }
    }
    None
}

fn preceding_await(content: &str, start: usize) -> Option<usize> {
    let prefix = &content[..start];
    let trimmed = prefix.trim_end_matches(char::is_whitespace);
    let await_start = trimmed.len().checked_sub("await".len())?;
    if &trimmed[await_start..] != "await"
        || trimmed[..await_start]
            .chars()
            .next_back()
            .is_some_and(|value| value.is_ascii_alphanumeric() || value == '_')
    {
        return None;
    }
    Some(await_start)
}

pub(super) fn transform(path: &str, content: &str) -> TransformResult {
    let mut replacements = Vec::new();
    let mut diagnostics = Vec::new();
    for start in code_occurrences(content, "Bun.file") {
        let mut open = start + "Bun.file".len();
        while content
            .as_bytes()
            .get(open)
            .is_some_and(u8::is_ascii_whitespace)
        {
            open += 1;
        }
        if content.as_bytes().get(open) != Some(&b'(') {
            continue;
        }
        let Some(close) = matching_parenthesis(content, open) else {
            continue;
        };
        let argument = &content[open + 1..close];
        if content[close + 1..].starts_with(".text()") {
            replacements.push((
                start,
                close + 1 + ".text()".len(),
                format!("Deno.readTextFile({argument})"),
            ));
        } else if content[close + 1..].starts_with(".json()") {
            if let Some(await_start) = preceding_await(content, start) {
                replacements.push((
                    await_start,
                    close + 1 + ".json()".len(),
                    format!("JSON.parse(await Deno.readTextFile({argument}))"),
                ));
            } else {
                diagnostics.push(blocking(
                    "RUNTIME_BUN_FILE_JSON_UNSUPPORTED",
                    path,
                    "Bun.file(...).json() is only rewritten when directly awaited.",
                    "Await the JSON read explicitly or migrate its promise flow manually.",
                ));
            }
        }
    }
    let mut counts = BTreeMap::new();
    if !replacements.is_empty() {
        counts.insert("bun.file-to-deno-file-api", replacements.len());
    }
    TransformResult {
        content: apply_replacements(content, &replacements),
        counts,
        diagnostics,
        required_permissions: if replacements.is_empty() {
            BTreeSet::new()
        } else {
            BTreeSet::from([DenoPermission::Read])
        },
    }
}

#[cfg(test)]
mod tests {
    use super::transform;

    #[test]
    fn transforms_text_and_directly_awaited_json_reads() {
        let result = transform(
            "config.ts",
            "const text = await Bun.file(path).text();\nconst data = await Bun.file(\"config.json\").json();\n",
        );
        assert_eq!(
            result.content,
            "const text = await Deno.readTextFile(path);\nconst data = JSON.parse(await Deno.readTextFile(\"config.json\"));\n"
        );
        assert!(result.diagnostics.is_empty());
    }

    #[test]
    fn blocks_unawaited_json_reads() {
        let result = transform("config.ts", "const data = Bun.file(path).json();\n");
        assert_eq!(result.content, "const data = Bun.file(path).json();\n");
        assert!(
            result
                .diagnostics
                .iter()
                .any(|diagnostic| { diagnostic.code == "RUNTIME_BUN_FILE_JSON_UNSUPPORTED" })
        );
    }
}
