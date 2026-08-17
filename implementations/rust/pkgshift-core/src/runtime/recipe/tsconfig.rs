use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

use super::{TransformResult, blocking};

pub(super) fn transform(path: &str, content: &str) -> TransformResult {
    let Ok(mut value) = json5::from_str::<Value>(content) else {
        return TransformResult {
            content: content.to_owned(),
            counts: BTreeMap::new(),
            diagnostics: vec![blocking(
                "RUNTIME_TSCONFIG_INVALID",
                path,
                "TypeScript configuration could not be parsed for Bun type cleanup.",
                "Repair the configuration before retrying the runtime migration.",
            )],
            required_permissions: BTreeSet::new(),
        };
    };
    let Some(compiler_options) = value
        .get_mut("compilerOptions")
        .and_then(Value::as_object_mut)
    else {
        return TransformResult::unchanged(content);
    };
    let Some(types) = compiler_options
        .get_mut("types")
        .and_then(Value::as_array_mut)
    else {
        return TransformResult::unchanged(content);
    };
    let before = types.len();
    types.retain(|value| !matches!(value.as_str(), Some("bun-types" | "@types/bun")));
    let removed = before - types.len();
    if removed == 0 {
        return TransformResult::unchanged(content);
    }
    if types.is_empty() {
        compiler_options.remove("types");
    }
    let mut transformed =
        serde_json::to_string_pretty(&value).expect("serializing a parsed JSON value cannot fail");
    transformed.push('\n');
    TransformResult {
        content: transformed,
        counts: BTreeMap::from([("bun.types-to-deno", removed)]),
        diagnostics: Vec::new(),
        required_permissions: BTreeSet::new(),
    }
}
