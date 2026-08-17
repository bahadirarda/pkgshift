use std::collections::{BTreeMap, BTreeSet};

use super::named_import::{imported_name, local_name, parse};
use super::{TransformResult, apply_replacements, blocking};
use crate::runtime::lex::{code_mask, contains_code};

const INCOMPATIBLE_DATABASE_MEMBERS: &[&str] = &["query", "run", "serialize", "transaction"];

fn transform_import(
    path: &str,
    line: &str,
    content: &str,
) -> Result<String, crate::model::Diagnostic> {
    let Some(import) = parse(line, "bun:sqlite") else {
        return Err(blocking(
            "RUNTIME_BUN_SQLITE_IMPORT_UNSUPPORTED",
            path,
            "The bun:sqlite import shape is outside the deterministic Database recipe.",
            "Use a named Database import and the shared prepare, exec, and close API subset.",
        ));
    };
    let mut mapped = Vec::new();
    for name in &import.names {
        if imported_name(name) != Some("Database") {
            return Err(blocking(
                "RUNTIME_BUN_SQLITE_API_UNSUPPORTED",
                path,
                "A bun:sqlite export has no enabled deterministic node:sqlite mapping.",
                "Limit the import to Database or migrate the additional SQLite API explicitly.",
            ));
        }
        let Some(local) = local_name(name) else {
            return Err(blocking(
                "RUNTIME_BUN_SQLITE_IMPORT_UNSUPPORTED",
                path,
                "The bun:sqlite Database alias could not be parsed safely.",
                "Use a direct named import or a single 'as' alias.",
            ));
        };
        // Bun and node:sqlite return differently shaped prepared statements.
        // Fail closed when any Bun-only Database member is present rather than
        // guessing which local variable received the imported constructor.
        if INCOMPATIBLE_DATABASE_MEMBERS
            .iter()
            .any(|member| contains_code(content, &format!(".{member}(")))
        {
            return Err(blocking(
                "RUNTIME_BUN_SQLITE_API_UNSUPPORTED",
                path,
                "The Bun Database value uses a member outside the verified node:sqlite subset.",
                "Rewrite the value to use prepare, exec, and close before retrying.",
            ));
        }
        mapped.push(format!("DatabaseSync as {local}"));
    }
    Ok(format!(
        "{}import {{ {} }} from \"node:sqlite\"{}",
        import.indentation,
        mapped.join(", "),
        import.semicolon
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
            && trimmed.contains("bun:sqlite")
        {
            match transform_import(path, line, content) {
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
            BTreeMap::from([("bun.sqlite-to-node.sqlite", count)])
        },
        diagnostics,
        required_permissions: BTreeSet::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::transform;

    #[test]
    fn maps_database_to_database_sync_without_renaming_usage() {
        let result = transform(
            "database.ts",
            "import { Database as DB } from \"bun:sqlite\";\nconst db = new DB(\":memory:\");\ndb.exec(\"select 1\");\n",
        );
        assert_eq!(
            result.content,
            "import { DatabaseSync as DB } from \"node:sqlite\";\nconst db = new DB(\":memory:\");\ndb.exec(\"select 1\");\n"
        );
        assert!(result.diagnostics.is_empty());
    }

    #[test]
    fn blocks_bun_only_query_members() {
        let result = transform(
            "database.ts",
            "import { Database } from \"bun:sqlite\";\nconst db = new Database();\ndb.query(\"select 1\");\n",
        );
        assert!(
            result
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "RUNTIME_BUN_SQLITE_API_UNSUPPORTED")
        );
    }
}
