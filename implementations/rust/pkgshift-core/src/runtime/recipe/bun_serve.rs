use std::collections::{BTreeMap, BTreeSet};

use super::{TransformResult, apply_replacements, blocking};
use crate::runtime::lex::{code_mask, code_occurrences, without_comments};
use crate::runtime::model::DenoPermission;

fn matching_delimiter(content: &str, open: usize, left: u8, right: u8) -> Option<usize> {
    let bytes = content.as_bytes();
    let mask = code_mask(content);
    if bytes.get(open) != Some(&left) || !mask.get(open).copied().unwrap_or(false) {
        return None;
    }
    let mut depth = 0usize;
    for index in open..bytes.len() {
        if !mask[index] {
            continue;
        }
        if bytes[index] == left {
            depth += 1;
        } else if bytes[index] == right {
            depth -= 1;
            if depth == 0 {
                return Some(index);
            }
        }
    }
    None
}

fn split_properties(content: &str) -> Option<Vec<&str>> {
    let bytes = content.as_bytes();
    let mask = code_mask(content);
    let (mut braces, mut brackets, mut parentheses) = (0usize, 0usize, 0usize);
    let mut start = 0usize;
    let mut properties = Vec::new();
    for index in 0..bytes.len() {
        if !mask[index] {
            continue;
        }
        match bytes[index] {
            b'{' => braces += 1,
            b'}' => braces = braces.checked_sub(1)?,
            b'[' => brackets += 1,
            b']' => brackets = brackets.checked_sub(1)?,
            b'(' => parentheses += 1,
            b')' => parentheses = parentheses.checked_sub(1)?,
            b',' if braces == 0 && brackets == 0 && parentheses == 0 => {
                properties.push(content[start..index].trim());
                start = index + 1;
            }
            _ => {}
        }
    }
    if braces != 0 || brackets != 0 || parentheses != 0 {
        return None;
    }
    let final_property = content[start..].trim();
    if !final_property.is_empty() {
        properties.push(final_property);
    }
    Some(properties)
}

fn method_handler(property: &str) -> Option<String> {
    let (asynchronous, rest) = property
        .strip_prefix("async ")
        .map_or((false, property), |rest| (true, rest));
    let rest = rest.strip_prefix("fetch")?;
    let leading = rest.len() - rest.trim_start().len();
    let open = if asynchronous { "async ".len() } else { 0 } + "fetch".len() + leading;
    let close = matching_delimiter(property, open, b'(', b')')?;
    if split_properties(&property[open + 1..close])?.len() != 1 {
        return None;
    }
    let after = property[close + 1..].trim_start();
    if !after.starts_with('{') {
        return None;
    }
    let block_open = property.len() - after.len();
    let block_close = matching_delimiter(property, block_open, b'{', b'}')?;
    if !property[block_close + 1..].trim().is_empty() {
        return None;
    }
    let prefix = if asynchronous { "async " } else { "" };
    Some(format!(
        "{prefix}({}) => {}",
        &property[open + 1..close],
        &property[block_open..=block_close]
    ))
}

fn imports_hono(content: &str) -> bool {
    content.lines().any(|line| {
        let trimmed = line.trim();
        let Some(body) = trimmed.strip_prefix("import ") else {
            return false;
        };
        let (Some(open), Some(close)) = (body.find('{'), body.find('}')) else {
            return false;
        };
        let imports_hono = body[open + 1..close]
            .split(',')
            .map(str::trim)
            .any(|name| name == "Hono");
        let tail = body[close + 1..].trim().trim_end_matches(';');
        imports_hono
            && matches!(
                tail,
                "from \"hono\"" | "from 'hono'" | "from \"npm:hono\"" | "from 'npm:hono'"
            )
    })
}

fn assigned_hono_instance(content: &str, identifier: &str) -> bool {
    content.lines().any(|line| {
        let trimmed = line.trim();
        ["const ", "let ", "var "].iter().any(|prefix| {
            let Some(declaration) = trimmed.strip_prefix(prefix) else {
                return false;
            };
            let Some((name, value)) = declaration.split_once('=') else {
                return false;
            };
            name.trim() == identifier && value.trim_start().starts_with("new Hono(")
        })
    })
}

fn hono_fetch_handler(content: &str, value: &str) -> Option<String> {
    let identifier = value.trim().strip_suffix(".fetch")?;
    let content = without_comments(content);
    if identifier.is_empty()
        || !identifier
            .chars()
            .all(|character| character == '_' || character == '$' || character.is_alphanumeric())
        || !imports_hono(&content)
        || !assigned_hono_instance(&content, identifier)
    {
        return None;
    }
    Some(value.trim().to_owned())
}

fn serve_replacement(content: &str, start: usize) -> Option<(usize, String)> {
    let mut cursor = start + "Bun.serve".len();
    while content
        .as_bytes()
        .get(cursor)
        .is_some_and(u8::is_ascii_whitespace)
    {
        cursor += 1;
    }
    if content.as_bytes().get(cursor) != Some(&b'(') {
        return None;
    }
    let call_close = matching_delimiter(content, cursor, b'(', b')')?;
    let arguments = content[cursor + 1..call_close].trim();
    if !arguments.starts_with('{') || !arguments.ends_with('}') {
        return None;
    }
    let object_open = cursor + 1 + content[cursor + 1..call_close].find('{')?;
    let object_close = matching_delimiter(content, object_open, b'{', b'}')?;
    if object_close + 1 != call_close && !content[object_close + 1..call_close].trim().is_empty() {
        return None;
    }
    let properties = split_properties(&content[object_open + 1..object_close])?;
    let mut port = None;
    let mut handler = None;
    for property in properties {
        if property == "port" {
            port = Some("port".to_owned());
        } else if let Some(value) = property.strip_prefix("port:") {
            port = Some(value.trim().to_owned());
        } else if let Some(value) = property.strip_prefix("fetch:") {
            handler = hono_fetch_handler(content, value);
        } else if property.starts_with("fetch(") || property.starts_with("async fetch(") {
            handler = method_handler(property);
        } else {
            return None;
        }
    }
    let handler = handler?;
    let replacement = port.map_or_else(
        || format!("Deno.serve({handler})"),
        |port| format!("Deno.serve({{ port: {port} }}, {handler})"),
    );
    Some((call_close + 1, replacement))
}

pub(super) fn transform(path: &str, content: &str) -> TransformResult {
    let mut replacements = Vec::new();
    let mut diagnostics = Vec::new();
    for start in code_occurrences(content, "Bun.serve") {
        if let Some((end, replacement)) = serve_replacement(content, start) {
            replacements.push((start, end, replacement));
        } else {
            diagnostics.push(blocking(
                "RUNTIME_BUN_SERVE_UNSUPPORTED",
                path,
                "Bun.serve uses options outside the deterministic fetch-handler recipe.",
                "Use only fetch and an optional port, or migrate routes, WebSockets, and lifecycle hooks manually.",
            ));
        }
    }
    let mut counts = BTreeMap::new();
    if !replacements.is_empty() {
        counts.insert("bun.serve-to-deno.serve", replacements.len());
    }
    TransformResult {
        content: apply_replacements(content, &replacements),
        counts,
        diagnostics,
        required_permissions: if replacements.is_empty() {
            BTreeSet::new()
        } else {
            BTreeSet::from([DenoPermission::Net])
        },
    }
}

#[cfg(test)]
mod tests {
    use super::transform;

    #[test]
    fn transforms_method_shorthand() {
        let result = transform(
            "server.ts",
            "Bun.serve({ port, fetch(request) { return new Response(request.url); } });",
        );
        assert_eq!(
            result.content,
            "Deno.serve({ port: port }, (request) => { return new Response(request.url); });"
        );
        assert!(result.diagnostics.is_empty());
    }

    #[test]
    fn transforms_an_official_hono_fetch_handler_shape() {
        let result = transform(
            "server.ts",
            "import { Hono } from \"hono\";\nconst app = new Hono();\nBun.serve({ port: 3000, fetch: app.fetch });",
        );
        assert_eq!(
            result.content,
            "import { Hono } from \"hono\";\nconst app = new Hono();\nDeno.serve({ port: 3000 }, app.fetch);"
        );
        assert!(result.diagnostics.is_empty());
    }

    #[test]
    fn blocks_unverified_handler_references_and_two_parameter_methods() {
        let reference = transform("server.ts", "Bun.serve({ fetch: handler });");
        assert!(!reference.diagnostics.is_empty());

        let method = transform(
            "server.ts",
            "Bun.serve({ fetch(request, server) { return handler(request, server); } });",
        );
        assert!(!method.diagnostics.is_empty());
    }

    #[test]
    fn blocks_routes() {
        let result = transform("server.ts", "Bun.serve({ routes: {}, fetch: app.fetch });");
        assert!(!result.diagnostics.is_empty());
    }

    #[test]
    fn transforms_async_method_shorthand() {
        let result = transform(
            "server.ts",
            "Bun.serve({ async fetch(request) { return await handler(request); } });",
        );
        assert_eq!(
            result.content,
            "Deno.serve(async (request) => { return await handler(request); });"
        );
        assert!(result.diagnostics.is_empty());
    }
}
