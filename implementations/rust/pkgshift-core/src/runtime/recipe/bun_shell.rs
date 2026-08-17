use std::collections::{BTreeMap, BTreeSet};

use super::named_import::{imported_name, local_name, parse};
use super::{TransformResult, apply_replacements, blocking};
use crate::runtime::lex::code_mask;
use crate::runtime::model::DenoPermission;

const DAX_SPECIFIER: &str = "jsr:@david/dax@0.49.0";

fn transform_import(path: &str, line: &str) -> Result<String, crate::model::Diagnostic> {
    let Some(import) = parse(line, "bun") else {
        return Err(blocking(
            "RUNTIME_BUN_SHELL_IMPORT_UNSUPPORTED",
            path,
            "The Bun shell import shape is outside the deterministic dax recipe.",
            "Use one named $ import from bun before retrying.",
        ));
    };
    if import.names.len() != 1 || imported_name(&import.names[0]) != Some("$") {
        return Err(blocking(
            "RUNTIME_BUN_SHELL_API_UNSUPPORTED",
            path,
            "The bun import contains exports beyond the supported $ shell template.",
            "Split the $ import and migrate the remaining Bun exports explicitly.",
        ));
    }
    let Some(local) = local_name(&import.names[0]) else {
        return Err(blocking(
            "RUNTIME_BUN_SHELL_IMPORT_UNSUPPORTED",
            path,
            "The Bun shell alias could not be parsed safely.",
            "Use a direct named import or a single 'as' alias.",
        ));
    };
    Ok(format!(
        "{}import {local} from \"{DAX_SPECIFIER}\"{}",
        import.indentation, import.semicolon,
    ))
}

pub(super) fn transform(path: &str, content: &str) -> TransformResult {
    let mask = code_mask(content);
    let mut replacements = Vec::new();
    let mut diagnostics = Vec::new();
    let mut count = 0usize;
    let mut offset = 0usize;
    for segment in content.split_inclusive('\n') {
        let line = segment.strip_suffix('\n').unwrap_or(segment);
        let trimmed = line.trim_start();
        let leading = line.len() - trimmed.len();
        if mask.get(offset + leading).copied().unwrap_or(false)
            && trimmed.starts_with("import ")
            && (trimmed.contains("from \"bun\"") || trimmed.contains("from 'bun'"))
        {
            match transform_import(path, line) {
                Ok(value) => {
                    replacements.push((offset, offset + line.len(), value));
                    count += 1;
                }
                Err(diagnostic) => diagnostics.push(diagnostic),
            }
        }
        offset += segment.len();
    }
    TransformResult {
        content: apply_replacements(content, &replacements),
        counts: if count == 0 {
            BTreeMap::new()
        } else {
            BTreeMap::from([("bun.shell-to-dax", count)])
        },
        diagnostics,
        required_permissions: if count == 0 {
            BTreeSet::new()
        } else {
            BTreeSet::from([DenoPermission::Env, DenoPermission::Run])
        },
    }
}

#[cfg(test)]
mod tests {
    use super::transform;

    #[test]
    fn maps_the_bun_shell_template_to_dax() {
        let result = transform(
            "build.ts",
            "import { $ } from \"bun\";\nawait $`echo hello`;\n",
        );
        assert_eq!(
            result.content,
            "import $ from \"jsr:@david/dax@0.49.0\";\nawait $`echo hello`;\n"
        );
        assert!(result.diagnostics.is_empty());
    }
}
