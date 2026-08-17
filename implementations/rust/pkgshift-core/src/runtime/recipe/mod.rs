mod bun_file;
mod bun_serve;
mod bun_shell;
mod bun_sqlite;
mod imports;
mod manifest;
mod named_import;
mod tsconfig;

use std::collections::{BTreeMap, BTreeSet};

use crate::model::{Diagnostic, DiagnosticSeverity, EvidenceDetail};

use super::lex::{code_occurrences, contains_code, without_comments};
use super::model::{DenoPermission, RuntimeFile, RuntimeRecipeApplication};

#[derive(Debug)]
pub(crate) struct RecipeOutput {
    pub content: String,
    pub applications: Vec<RuntimeRecipeApplication>,
    pub diagnostics: Vec<Diagnostic>,
    pub required_permissions: BTreeSet<DenoPermission>,
}

#[derive(Debug)]
struct TransformResult {
    content: String,
    counts: BTreeMap<&'static str, usize>,
    diagnostics: Vec<Diagnostic>,
    required_permissions: BTreeSet<DenoPermission>,
}

impl TransformResult {
    fn unchanged(content: &str) -> Self {
        Self {
            content: content.to_owned(),
            counts: BTreeMap::new(),
            diagnostics: Vec::new(),
            required_permissions: BTreeSet::new(),
        }
    }

    fn merge(&mut self, next: TransformResult) {
        self.content = next.content;
        for (recipe, count) in next.counts {
            *self.counts.entry(recipe).or_default() += count;
        }
        self.diagnostics.extend(next.diagnostics);
        self.required_permissions.extend(next.required_permissions);
    }
}

fn blocking(code: &str, path: &str, summary: impl Into<String>, remediation: &str) -> Diagnostic {
    Diagnostic {
        code: code.to_owned(),
        severity: DiagnosticSeverity::Error,
        summary: summary.into(),
        blocking: true,
        evidence: vec![EvidenceDetail {
            location: path.to_owned(),
            detail: "Bun-specific runtime semantics require an explicit recipe.".to_owned(),
        }],
        remediation: vec![remediation.to_owned()],
    }
}

fn apply_replacements(content: &str, replacements: &[(usize, usize, String)]) -> String {
    let mut output = content.to_owned();
    let mut ordered = replacements.to_vec();
    ordered.sort_by_key(|(start, _, _)| *start);
    let mut non_overlapping = Vec::with_capacity(ordered.len());
    let mut previous_end = 0usize;
    for replacement in ordered {
        if replacement.0 >= previous_end {
            previous_end = replacement.1;
            non_overlapping.push(replacement);
        }
    }
    for (start, end, replacement) in non_overlapping.into_iter().rev() {
        output.replace_range(start..end, &replacement);
    }
    output
}

fn source_file(path: &str) -> bool {
    [".js", ".jsx", ".mjs", ".cjs", ".ts", ".tsx", ".mts", ".cts"]
        .iter()
        .any(|extension| path.ends_with(extension))
}

fn remaining_import(path: &str, content: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let content = without_comments(content);
    if content.contains("\"bun:") || content.contains("'bun:") {
        diagnostics.push(blocking(
            "RUNTIME_BUN_MODULE_UNSUPPORTED",
            path,
            "A Bun-specific module import has no enabled deterministic recipe.",
            "Replace the reported Bun module with a reviewed Deno, Node, npm, or JSR equivalent.",
        ));
    }
    if content.contains(" from \"bun\"") || content.contains(" from 'bun'") {
        diagnostics.push(blocking(
            "RUNTIME_BUN_SHELL_UNSUPPORTED",
            path,
            "The Bun runtime import remains outside the deterministic recipe set.",
            "Replace Bun shell APIs with Deno.Command or a reviewed dax integration.",
        ));
    }
    diagnostics
}

fn remaining_source_diagnostics(path: &str, content: &str) -> Vec<Diagnostic> {
    let mut diagnostics = remaining_import(path, content);
    if !code_occurrences(content, "Bun.serve").is_empty() {
        diagnostics.push(blocking(
            "RUNTIME_BUN_SERVE_UNSUPPORTED",
            path,
            "A Bun.serve call uses options outside the safe fetch-handler recipe.",
            "Reduce the call to fetch plus an optional port, or migrate routes and WebSocket behavior manually.",
        ));
    }
    if !code_occurrences(content, "Bun.file").is_empty() {
        diagnostics.push(blocking(
            "RUNTIME_BUN_FILE_UNSUPPORTED",
            path,
            "A Bun.file call uses behavior outside the safe text/json recipe.",
            "Replace the call with a reviewed Deno file API and declare its required permission.",
        ));
    }
    if contains_code(content, "Bun.") {
        diagnostics.push(blocking(
            "RUNTIME_BUN_GLOBAL_UNSUPPORTED",
            path,
            "A Bun global API remains after applying the available runtime recipes.",
            "Migrate the reported Bun API explicitly before applying this plan.",
        ));
    }
    if contains_code(content, "HTMLRewriter") {
        diagnostics.push(blocking(
            "RUNTIME_HTML_REWRITER_UNSUPPORTED",
            path,
            "HTMLRewriter has no direct Deno runtime equivalent.",
            "Choose and review an HTML parser or npm-compatible replacement.",
        ));
    }
    if content.contains("type: \"macro\"") || content.contains("type: 'macro'") {
        diagnostics.push(blocking(
            "RUNTIME_BUN_MACRO_UNSUPPORTED",
            path,
            "Bun build-time macros have no direct Deno equivalent.",
            "Move the macro work into an explicit build script before migration.",
        ));
    }
    diagnostics
}

pub(crate) fn transform_file(
    file: &RuntimeFile,
    permissions: &BTreeSet<DenoPermission>,
) -> RecipeOutput {
    let mut result = TransformResult::unchanged(&file.content);
    if source_file(&file.path) {
        result.merge(imports::transform(&file.path, &result.content));
        result.merge(bun_sqlite::transform(&file.path, &result.content));
        result.merge(bun_shell::transform(&file.path, &result.content));
        result.merge(bun_serve::transform(&file.path, &result.content));
        result.merge(bun_file::transform(&file.path, &result.content));
        result
            .diagnostics
            .extend(remaining_source_diagnostics(&file.path, &result.content));
    } else if file.path.ends_with("package.json") {
        result.merge(manifest::transform(
            &file.path,
            &result.content,
            permissions,
        ));
    } else if file
        .path
        .rsplit('/')
        .next()
        .is_some_and(|name| name.starts_with("tsconfig") && name.ends_with(".json"))
    {
        result.merge(tsconfig::transform(&file.path, &result.content));
    } else if file.path.ends_with("bunfig.toml") {
        result.diagnostics.push(blocking(
            "RUNTIME_BUNFIG_UNSUPPORTED",
            &file.path,
            "bunfig.toml requires a semantic configuration migration.",
            "Move reviewed tasks and tooling settings into deno.json before applying the runtime plan.",
        ));
    }
    RecipeOutput {
        content: result.content,
        applications: result
            .counts
            .into_iter()
            .map(|(recipe_id, replacements)| RuntimeRecipeApplication {
                recipe_id: recipe_id.to_owned(),
                path: file.path.clone(),
                replacements,
            })
            .collect(),
        diagnostics: result.diagnostics,
        required_permissions: result.required_permissions,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::{apply_replacements, transform_file};
    use crate::runtime::model::{DenoPermission, RuntimeFile};

    #[test]
    fn transforms_a_hono_style_entrypoint() {
        let output = transform_file(
            &RuntimeFile {
                path: "src/index.ts".to_owned(),
                content: "import { Hono } from \"hono\";\nconst app = new Hono();\nBun.serve({ port: 3000, fetch: app.fetch });\n".to_owned(),
            },
            &BTreeSet::from([DenoPermission::Net]),
        );
        assert_eq!(
            output.content,
            "import { Hono } from \"hono\";\nconst app = new Hono();\nDeno.serve({ port: 3000 }, app.fetch);\n"
        );
        assert!(output.diagnostics.is_empty());
        assert!(output.required_permissions.contains(&DenoPermission::Net));
    }

    #[test]
    fn ignores_overlapping_replacement_ranges() {
        let output = apply_replacements(
            "abcdef",
            &[(1, 4, "first".to_owned()), (2, 5, "overlap".to_owned())],
        );
        assert_eq!(output, "afirstef");
    }

    #[test]
    fn transforms_the_verified_sqlite_subset() {
        let output = transform_file(
            &RuntimeFile {
                path: "src/database.ts".to_owned(),
                content: "import { Database } from \"bun:sqlite\";\n".to_owned(),
            },
            &BTreeSet::new(),
        );
        assert_eq!(
            output.content,
            "import { DatabaseSync as Database } from \"node:sqlite\";\n"
        );
        assert!(output.diagnostics.is_empty());
    }
}
