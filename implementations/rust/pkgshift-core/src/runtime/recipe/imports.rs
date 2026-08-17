use std::collections::{BTreeMap, BTreeSet};

use super::named_import::{imported_name, parse};
use super::{TransformResult, apply_replacements, blocking};
use crate::runtime::lex::code_mask;

const NODE_TEST_NAMES: &[&str] = &[
    "test",
    "describe",
    "it",
    "suite",
    "before",
    "after",
    "beforeEach",
    "afterEach",
];

fn transform_test_import(path: &str, line: &str) -> Result<String, crate::model::Diagnostic> {
    let Some(import) = parse(line, "bun:test") else {
        return Err(blocking(
            "RUNTIME_BUN_TEST_IMPORT_UNSUPPORTED",
            path,
            "The bun:test import shape is outside the deterministic named-import recipe.",
            "Use named imports, then retry the runtime migration.",
        ));
    };
    let mut node_names = Vec::new();
    let mut expect_names = Vec::new();
    for name in import.names {
        match imported_name(&name) {
            Some("expect") => expect_names.push(name),
            Some(base) if NODE_TEST_NAMES.contains(&base) => node_names.push(name),
            _ => {
                return Err(blocking(
                    "RUNTIME_BUN_TEST_API_UNSUPPORTED",
                    path,
                    "A bun:test API has no verified node:test or @std/expect mapping.",
                    "Replace unsupported mocks, spies, or matchers explicitly before retrying.",
                ));
            }
        }
    }
    let mut imports = Vec::new();
    if !node_names.is_empty() {
        imports.push(format!(
            "{}import {{ {} }} from \"node:test\"{}",
            import.indentation,
            node_names.join(", "),
            import.semicolon
        ));
    }
    if !expect_names.is_empty() {
        imports.push(format!(
            "{}import {{ {} }} from \"jsr:@std/expect\"{}",
            import.indentation,
            expect_names.join(", "),
            import.semicolon
        ));
    }
    Ok(imports.join("\n"))
}

pub(super) fn transform(path: &str, content: &str) -> TransformResult {
    let mask = code_mask(content);
    let mut replacements = Vec::new();
    let mut diagnostics = Vec::new();
    let mut test_count = 0;
    let mut offset = 0usize;
    for segment in content.split_inclusive('\n') {
        let line = segment.strip_suffix('\n').unwrap_or(segment);
        let trimmed = line.trim_start();
        let leading = line.len() - trimmed.len();
        if !mask.get(offset + leading).copied().unwrap_or(false) || !trimmed.starts_with("import ")
        {
            offset += segment.len();
            continue;
        }
        let transformed = if trimmed.contains("bun:test") {
            let transformed = transform_test_import(path, line);
            if transformed.is_ok() {
                test_count += 1;
            }
            transformed
        } else {
            offset += segment.len();
            continue;
        };
        match transformed {
            Ok(value) => replacements.push((offset, offset + line.len(), value)),
            Err(diagnostic) => diagnostics.push(diagnostic),
        }
        offset += segment.len();
    }
    let mut counts = BTreeMap::new();
    if test_count > 0 {
        counts.insert("bun.test-to-node.test", test_count);
    }
    TransformResult {
        content: apply_replacements(content, &replacements),
        counts,
        diagnostics,
        required_permissions: BTreeSet::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::transform;

    #[test]
    fn splits_expect_from_node_test_imports() {
        let result = transform(
            "math.test.ts",
            "import { describe, it, expect } from \"bun:test\";\n",
        );
        assert_eq!(
            result.content,
            "import { describe, it } from \"node:test\";\nimport { expect } from \"jsr:@std/expect\";\n"
        );
        assert!(result.diagnostics.is_empty());
    }
}
