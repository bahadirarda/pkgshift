use std::path::Path;

use serde_json::{Map, Value};

use crate::catalog::get_package_manager;
use crate::model::{
    Diagnostic, DiagnosticSeverity, EvidenceDetail, IntegrationInspection, MutationAction,
    PackageManagerId, PlannedFileMutation, ProjectInspection,
};
use crate::util::Result;

use super::commands::{contains_package_manager_command, rewrite_package_manager_commands};
use super::{mutation, read_text};

struct IntegrationRewrite {
    content: String,
    diagnostics: Vec<Diagnostic>,
}

fn command_name(manager: PackageManagerId) -> &'static str {
    match manager {
        PackageManagerId::YarnClassic | PackageManagerId::YarnModern => "yarn",
        PackageManagerId::Npm => "npm",
        PackageManagerId::Pnpm => "pnpm",
        PackageManagerId::Bun => "bun",
        PackageManagerId::Vlt => "vlt",
        PackageManagerId::Deno => "deno",
    }
}

fn primary_lockfile(manager: PackageManagerId) -> &'static str {
    match manager {
        PackageManagerId::Npm => "package-lock.json",
        PackageManagerId::Pnpm => "pnpm-lock.yaml",
        PackageManagerId::YarnClassic | PackageManagerId::YarnModern => "yarn.lock",
        PackageManagerId::Bun => "bun.lock",
        PackageManagerId::Vlt => "vlt-lock.json",
        PackageManagerId::Deno => "deno.lock",
    }
}

fn setup_action(manager: PackageManagerId) -> Option<&'static str> {
    match manager {
        PackageManagerId::Pnpm => Some("pnpm/action-setup@v6"),
        PackageManagerId::Bun => Some("oven-sh/setup-bun@v2"),
        PackageManagerId::Deno => Some("denoland/setup-deno@v2"),
        _ => None,
    }
}

fn setup_action_prefix(manager: PackageManagerId) -> Option<&'static str> {
    match manager {
        PackageManagerId::Pnpm => Some("pnpm/action-setup@"),
        PackageManagerId::Bun => Some("oven-sh/setup-bun@"),
        PackageManagerId::Deno => Some("denoland/setup-deno@"),
        _ => None,
    }
}

fn cache_name(manager: PackageManagerId) -> Option<&'static str> {
    match manager {
        PackageManagerId::Npm => Some("npm"),
        PackageManagerId::Pnpm => Some("pnpm"),
        PackageManagerId::YarnClassic | PackageManagerId::YarnModern => Some("yarn"),
        _ => None,
    }
}

fn package_manager_version(manager: PackageManagerId) -> &'static str {
    get_package_manager(manager)
        .package_manager_pin
        .rsplit_once('@')
        .map_or("latest", |(_, version)| version)
}

fn volta_name(manager: PackageManagerId) -> Option<&'static str> {
    match manager {
        PackageManagerId::Npm => Some("npm"),
        PackageManagerId::Pnpm => Some("pnpm"),
        PackageManagerId::YarnClassic | PackageManagerId::YarnModern => Some("yarn"),
        _ => None,
    }
}

fn toolchain_name(manager: PackageManagerId) -> Option<&'static str> {
    match manager {
        PackageManagerId::Pnpm => Some("pnpm"),
        PackageManagerId::YarnClassic | PackageManagerId::YarnModern => Some("yarn"),
        PackageManagerId::Bun => Some("bun"),
        PackageManagerId::Deno => Some("deno"),
        _ => None,
    }
}

fn diagnostic(
    code: &str,
    summary: impl Into<String>,
    path: &str,
    line: usize,
    blocking: bool,
    remediation: Vec<String>,
) -> Diagnostic {
    Diagnostic {
        code: code.to_owned(),
        severity: if blocking {
            DiagnosticSeverity::Error
        } else {
            DiagnosticSeverity::Warning
        },
        summary: summary.into(),
        blocking,
        evidence: vec![EvidenceDetail {
            location: format!("{path}:{line}"),
            detail: "repository integration".to_owned(),
        }],
        remediation,
    }
}

pub(super) fn rewrite_manifest_toolchain_pins(
    manifest: &mut Map<String, Value>,
    source: PackageManagerId,
    target: PackageManagerId,
    manifest_path: &str,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let source_name = command_name(source);
    let target_name = command_name(target);
    let target_version = package_manager_version(target);

    if let Some(volta) = manifest.get_mut("volta").and_then(Value::as_object_mut)
        && volta.remove(source_name).is_some()
    {
        if let Some(target_name) = volta_name(target) {
            volta.insert(
                target_name.to_owned(),
                Value::String(target_version.to_owned()),
            );
        } else {
            diagnostics.push(diagnostic(
                "INTEGRATION_VOLTA_TARGET_UNSUPPORTED",
                format!("Volta cannot represent the registered {target} toolchain pin."),
                manifest_path,
                1,
                false,
                vec![
                    "Use the packageManager field or a target-compatible toolchain manager for the package manager pin."
                        .to_owned(),
                ],
            ));
        }
    }

    if let Some(engines) = manifest.get_mut("engines").and_then(Value::as_object_mut)
        && engines.remove(source_name).is_some()
    {
        engines.insert(
            target_name.to_owned(),
            Value::String(format!(">={target_version}")),
        );
    }

    if let Some(package_manager) = manifest
        .get_mut("devEngines")
        .and_then(Value::as_object_mut)
        .and_then(|value| value.get_mut("packageManager"))
        .and_then(Value::as_object_mut)
        && package_manager.get("name").and_then(Value::as_str) == Some(source_name)
    {
        package_manager.insert("name".to_owned(), Value::String(target_name.to_owned()));
        package_manager.insert(
            "version".to_owned(),
            Value::String(target_version.to_owned()),
        );
    }
}

fn replace_source_lockfiles(
    content: &str,
    source: PackageManagerId,
    target: PackageManagerId,
) -> String {
    let mut output = content.to_owned();
    let target_lockfile = primary_lockfile(target);
    for source_lockfile in get_package_manager(source).lockfiles {
        output = output.replace(source_lockfile, target_lockfile);
    }
    output
}

fn rewrite_scalar(value: &str, source: PackageManagerId, target: PackageManagerId) -> String {
    let trimmed = value.trim();
    let quote = trimmed
        .chars()
        .next()
        .filter(|value| matches!(value, '\'' | '"'));
    if let Some(quote) = quote
        && trimmed.ends_with(quote)
        && trimmed.len() >= 2
    {
        let leading = value.find(quote).unwrap_or_default();
        let trailing = value.len() - value.rfind(quote).unwrap_or(value.len());
        let inner_start = leading + quote.len_utf8();
        let inner_end = value.len() - trailing;
        let mut output = String::with_capacity(value.len());
        output.push_str(&value[..inner_start]);
        output.push_str(&rewrite_package_manager_commands(
            &value[inner_start..inner_end],
            source,
            target,
        ));
        output.push_str(&value[inner_end..]);
        return output;
    }
    rewrite_package_manager_commands(value, source, target)
}

fn yaml_command_payload(line: &str) -> Option<(usize, &str)> {
    let trimmed = line.trim_start();
    let indentation = line.len() - trimmed.len();
    let candidate = trimmed.strip_prefix("- ").unwrap_or(trimmed);
    for key in ["run:", "script:", "command:", "entrypoint:"] {
        if let Some(value) = candidate.strip_prefix(key) {
            let offset = line.len() - value.len();
            return Some((indentation, &line[offset..]));
        }
    }
    None
}

fn rewrite_setup_action(
    line: &str,
    path: &str,
    line_number: usize,
    source: PackageManagerId,
    target: PackageManagerId,
    diagnostics: &mut Vec<Diagnostic>,
) -> String {
    let Some(source_prefix) = setup_action_prefix(source) else {
        return line.to_owned();
    };
    let Some(index) = line.find(source_prefix) else {
        return line.to_owned();
    };
    let end = line[index..]
        .find(char::is_whitespace)
        .map_or(line.len(), |offset| index + offset);
    let Some(target_action) = setup_action(target) else {
        diagnostics.push(diagnostic(
            "INTEGRATION_SETUP_ACTION_UNSUPPORTED",
            format!(
                "The registered {source} setup action has no deterministic {target} replacement."
            ),
            path,
            line_number,
            true,
            vec![
                "Replace the setup step with a reviewed target installation step before retrying."
                    .to_owned(),
            ],
        ));
        return line.to_owned();
    };
    format!("{}{}{}", &line[..index], target_action, &line[end..])
}

fn rewrite_cache_input(
    line: &str,
    path: &str,
    line_number: usize,
    source: PackageManagerId,
    target: PackageManagerId,
    diagnostics: &mut Vec<Diagnostic>,
) -> String {
    let trimmed = line.trim_start();
    let Some(value) = trimmed.strip_prefix("cache:") else {
        return line.to_owned();
    };
    let source_name = command_name(source);
    if value.trim().trim_matches(['\'', '"']) != source_name {
        return line.to_owned();
    }
    let Some(target_name) = cache_name(target) else {
        diagnostics.push(diagnostic(
            "INTEGRATION_CACHE_UNSUPPORTED",
            format!("setup-node dependency caching cannot represent {target}."),
            path,
            line_number,
            true,
            vec![
                "Remove the source cache input or replace it with a reviewed target cache step."
                    .to_owned(),
            ],
        ));
        return line.to_owned();
    };
    line.replacen(source_name, target_name, 1)
}

fn push_residue_diagnostic(
    payload: &str,
    path: &str,
    line_number: usize,
    source: PackageManagerId,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if contains_package_manager_command(payload, source) {
        diagnostics.push(diagnostic(
            "INTEGRATION_COMMAND_AMBIGUOUS",
            format!("A {source} command remains in an executable integration context."),
            path,
            line_number,
            true,
            vec![
                "Rewrite the command into the deterministic package-management subset before retrying."
                    .to_owned(),
            ],
        ));
    }
}

fn rewrite_yaml(
    path: &str,
    content: &str,
    source: PackageManagerId,
    target: PackageManagerId,
) -> IntegrationRewrite {
    let mut output = String::with_capacity(content.len());
    let mut diagnostics = Vec::new();
    let mut command_block_indent = None;
    for (index, line_with_ending) in content.split_inclusive('\n').enumerate() {
        let line_number = index + 1;
        let newline = line_with_ending.strip_suffix('\n').map_or("", |_| "\n");
        let line = line_with_ending
            .strip_suffix('\n')
            .unwrap_or(line_with_ending);
        let trimmed = line.trim_start();
        let indentation = line.len() - trimmed.len();
        let mut rewritten =
            rewrite_setup_action(line, path, line_number, source, target, &mut diagnostics);
        rewritten = rewrite_cache_input(
            &rewritten,
            path,
            line_number,
            source,
            target,
            &mut diagnostics,
        );

        if let Some(block_indent) = command_block_indent {
            if trimmed.is_empty() || indentation > block_indent {
                let payload = rewrite_package_manager_commands(trimmed, source, target);
                push_residue_diagnostic(&payload, path, line_number, source, &mut diagnostics);
                rewritten = format!("{}{}", &line[..indentation], payload);
            } else {
                command_block_indent = None;
            }
        }

        if command_block_indent.is_none()
            && let Some((base_indent, payload)) = yaml_command_payload(&rewritten)
        {
            if matches!(payload.trim(), "|" | ">" | "|-" | ">-") {
                command_block_indent = Some(base_indent);
            } else {
                let payload_offset = rewritten.len() - payload.len();
                let rewritten_payload = rewrite_scalar(payload, source, target);
                push_residue_diagnostic(
                    &rewritten_payload,
                    path,
                    line_number,
                    source,
                    &mut diagnostics,
                );
                rewritten = format!("{}{}", &rewritten[..payload_offset], rewritten_payload);
            }
        }
        rewritten = replace_source_lockfiles(&rewritten, source, target);
        output.push_str(&rewritten);
        output.push_str(newline);
    }
    IntegrationRewrite {
        content: output,
        diagnostics,
    }
}

fn rewrite_dockerfile(
    path: &str,
    content: &str,
    source: PackageManagerId,
    target: PackageManagerId,
) -> IntegrationRewrite {
    let mut output = String::with_capacity(content.len());
    let mut diagnostics = Vec::new();
    let mut continued = false;
    for (index, line_with_ending) in content.split_inclusive('\n').enumerate() {
        let line_number = index + 1;
        let newline = line_with_ending.strip_suffix('\n').map_or("", |_| "\n");
        let line = line_with_ending
            .strip_suffix('\n')
            .unwrap_or(line_with_ending);
        let trimmed = line.trim_start();
        let recognized = ["RUN ", "CMD ", "ENTRYPOINT "]
            .iter()
            .find_map(|prefix| trimmed.strip_prefix(prefix));
        let mut rewritten = line.to_owned();
        if let Some(payload) = recognized.or_else(|| continued.then_some(trimmed)) {
            let offset = line.len() - payload.len();
            let rewritten_payload = rewrite_package_manager_commands(payload, source, target);
            push_residue_diagnostic(
                &rewritten_payload,
                path,
                line_number,
                source,
                &mut diagnostics,
            );
            rewritten = format!("{}{}", &line[..offset], rewritten_payload);
        }
        rewritten = replace_source_lockfiles(&rewritten, source, target);
        continued = rewritten.trim_end().ends_with('\\');
        output.push_str(&rewritten);
        output.push_str(newline);
    }
    IntegrationRewrite {
        content: output,
        diagnostics,
    }
}

fn rewrite_markdown(
    path: &str,
    content: &str,
    source: PackageManagerId,
    target: PackageManagerId,
) -> IntegrationRewrite {
    let mut output = String::with_capacity(content.len());
    let mut diagnostics = Vec::new();
    let mut shell_fence = false;
    for (index, line_with_ending) in content.split_inclusive('\n').enumerate() {
        let line_number = index + 1;
        let newline = line_with_ending.strip_suffix('\n').map_or("", |_| "\n");
        let line = line_with_ending
            .strip_suffix('\n')
            .unwrap_or(line_with_ending);
        let trimmed = line.trim_start();
        if let Some(language) = trimmed.strip_prefix("```") {
            if shell_fence {
                shell_fence = false;
            } else {
                shell_fence = matches!(language.trim(), "sh" | "bash" | "shell" | "console");
            }
            output.push_str(line);
            output.push_str(newline);
            continue;
        }
        let mut rewritten = if shell_fence {
            rewrite_package_manager_commands(line, source, target)
        } else {
            rewrite_inline_code(line, source, target)
        };
        if shell_fence {
            push_residue_diagnostic(&rewritten, path, line_number, source, &mut diagnostics);
        }
        output.push_str(&rewritten);
        output.push_str(newline);
        rewritten.clear();
    }
    IntegrationRewrite {
        content: output,
        diagnostics,
    }
}

fn rewrite_inline_code(line: &str, source: PackageManagerId, target: PackageManagerId) -> String {
    let mut output = String::with_capacity(line.len());
    let mut remainder = line;
    while let Some(opening) = remainder.find('`') {
        output.push_str(&remainder[..=opening]);
        let after_opening = &remainder[opening + 1..];
        let Some(closing) = after_opening.find('`') else {
            output.push_str(after_opening);
            return output;
        };
        output.push_str(&rewrite_package_manager_commands(
            &after_opening[..closing],
            source,
            target,
        ));
        output.push('`');
        remainder = &after_opening[closing + 1..];
    }
    output.push_str(remainder);
    output
}

fn rewrite_automation(
    path: &str,
    content: &str,
    source: PackageManagerId,
    target: PackageManagerId,
) -> IntegrationRewrite {
    let mut output = String::with_capacity(content.len());
    let mut diagnostics = Vec::new();
    for (index, line_with_ending) in content.split_inclusive('\n').enumerate() {
        let line_number = index + 1;
        let newline = line_with_ending.strip_suffix('\n').map_or("", |_| "\n");
        let line = line_with_ending
            .strip_suffix('\n')
            .unwrap_or(line_with_ending);
        let executable = line.starts_with('\t')
            || line.trim_start().starts_with("sh ")
            || line.trim_start().starts_with("bat ")
            || line.trim_start().starts_with("powershell ");
        let mut rewritten = if executable {
            rewrite_package_manager_commands(line, source, target)
        } else {
            line.to_owned()
        };
        if executable {
            push_residue_diagnostic(&rewritten, path, line_number, source, &mut diagnostics);
        }
        rewritten = replace_source_lockfiles(&rewritten, source, target);
        output.push_str(&rewritten);
        output.push_str(newline);
    }
    IntegrationRewrite {
        content: output,
        diagnostics,
    }
}

fn rewrite_tool_versions(
    path: &str,
    content: &str,
    source: PackageManagerId,
    target: PackageManagerId,
) -> IntegrationRewrite {
    let source_name = command_name(source);
    let target_name = toolchain_name(target);
    let mut output = String::with_capacity(content.len());
    let mut diagnostics = Vec::new();
    for (index, line_with_ending) in content.split_inclusive('\n').enumerate() {
        let newline = line_with_ending.strip_suffix('\n').map_or("", |_| "\n");
        let line = line_with_ending
            .strip_suffix('\n')
            .unwrap_or(line_with_ending);
        let mut fields = line.split_whitespace();
        if fields.next() == Some(source_name) && fields.next().is_some() {
            if let Some(target_name) = target_name {
                output.push_str(target_name);
                output.push(' ');
                output.push_str(package_manager_version(target));
                output.push_str(newline);
            } else {
                diagnostics.push(diagnostic(
                    "INTEGRATION_TOOLCHAIN_TARGET_UNSUPPORTED",
                    format!("{path} cannot represent the registered {target} toolchain pin."),
                    path,
                    index + 1,
                    false,
                    vec![
                        "Use the packageManager field or a reviewed target-compatible tool plugin."
                            .to_owned(),
                    ],
                ));
            }
        } else {
            output.push_str(line);
            output.push_str(newline);
        }
    }
    IntegrationRewrite {
        content: output,
        diagnostics,
    }
}

fn rewrite_mise(
    path: &str,
    content: &str,
    source: PackageManagerId,
    target: PackageManagerId,
) -> IntegrationRewrite {
    let source_name = command_name(source);
    let target_name = toolchain_name(target);
    let mut output = String::with_capacity(content.len());
    let mut diagnostics = Vec::new();
    let mut in_tools = false;
    for (index, line_with_ending) in content.split_inclusive('\n').enumerate() {
        let newline = line_with_ending.strip_suffix('\n').map_or("", |_| "\n");
        let line = line_with_ending
            .strip_suffix('\n')
            .unwrap_or(line_with_ending);
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_tools = trimmed == "[tools]";
        }
        let source_assignment = in_tools
            && trimmed
                .strip_prefix(source_name)
                .is_some_and(|suffix| suffix.trim_start().starts_with('='));
        if source_assignment {
            if let Some(target_name) = target_name {
                let indentation = line.len() - line.trim_start().len();
                output.push_str(&line[..indentation]);
                output.push_str(target_name);
                output.push_str(" = \"");
                output.push_str(package_manager_version(target));
                output.push('"');
                output.push_str(newline);
            } else {
                diagnostics.push(diagnostic(
                    "INTEGRATION_TOOLCHAIN_TARGET_UNSUPPORTED",
                    format!("{path} cannot represent the registered {target} toolchain pin."),
                    path,
                    index + 1,
                    false,
                    vec![
                        "Use the packageManager field or a reviewed target-compatible tool backend."
                            .to_owned(),
                    ],
                ));
            }
        } else {
            output.push_str(line);
            output.push_str(newline);
        }
    }
    IntegrationRewrite {
        content: output,
        diagnostics,
    }
}

fn json_command_range(line: &str) -> Option<std::result::Result<(usize, usize), ()>> {
    let trimmed = line.trim_start();
    let keys = [
        "initializeCommand",
        "onCreateCommand",
        "updateContentCommand",
        "postCreateCommand",
        "postStartCommand",
        "postAttachCommand",
        "postStopCommand",
    ];
    let _key = keys
        .iter()
        .find(|key| trimmed.starts_with(&format!("\"{key}\"")))?;
    let colon = line.find(':')?;
    let value = &line[colon + 1..];
    let leading = value.find(|character: char| !character.is_whitespace())?;
    if value.as_bytes().get(leading) != Some(&b'"') {
        return Some(Err(()));
    }
    let start = colon + 1 + leading + 1;
    let bytes = line.as_bytes();
    let mut escaped = false;
    for (offset, byte) in bytes[start..].iter().enumerate() {
        if *byte == b'"' && !escaped {
            return Some(Ok((start, start + offset)));
        }
        escaped = *byte == b'\\' && !escaped;
        if *byte != b'\\' {
            escaped = false;
        }
    }
    Some(Err(()))
}

fn rewrite_devcontainer(
    path: &str,
    content: &str,
    source: PackageManagerId,
    target: PackageManagerId,
) -> IntegrationRewrite {
    let mut output = String::with_capacity(content.len());
    let mut diagnostics = Vec::new();
    for (index, line_with_ending) in content.split_inclusive('\n').enumerate() {
        let newline = line_with_ending.strip_suffix('\n').map_or("", |_| "\n");
        let line = line_with_ending
            .strip_suffix('\n')
            .unwrap_or(line_with_ending);
        let rewritten = match json_command_range(line) {
            Some(Ok((start, end))) => {
                let command = rewrite_package_manager_commands(&line[start..end], source, target);
                push_residue_diagnostic(&command, path, index + 1, source, &mut diagnostics);
                format!("{}{}{}", &line[..start], command, &line[end..])
            }
            Some(Err(())) => {
                diagnostics.push(diagnostic(
                    "INTEGRATION_DEVCONTAINER_COMMAND_UNSUPPORTED",
                    "A devcontainer lifecycle command uses an unsupported object or array shape.",
                    path,
                    index + 1,
                    true,
                    vec![
                        "Use a string lifecycle command or migrate the command map manually before retrying."
                            .to_owned(),
                    ],
                ));
                line.to_owned()
            }
            None => line.to_owned(),
        };
        output.push_str(&replace_source_lockfiles(&rewritten, source, target));
        output.push_str(newline);
    }
    IntegrationRewrite {
        content: output,
        diagnostics,
    }
}

fn rewrite_integration(
    integration: &IntegrationInspection,
    content: &str,
    source: PackageManagerId,
    target: PackageManagerId,
) -> IntegrationRewrite {
    let lowercase = integration.path.to_ascii_lowercase();
    if integration.path == ".tool-versions" {
        rewrite_tool_versions(&integration.path, content, source, target)
    } else if matches!(integration.path.as_str(), ".mise.toml" | "mise.toml") {
        rewrite_mise(&integration.path, content, source, target)
    } else if matches!(
        integration.path.as_str(),
        ".devcontainer.json" | "devcontainer.json" | ".devcontainer/devcontainer.json"
    ) {
        rewrite_devcontainer(&integration.path, content, source, target)
    } else if lowercase.ends_with(".md") {
        rewrite_markdown(&integration.path, content, source, target)
    } else if lowercase.contains("dockerfile") || integration.path == "Containerfile" {
        rewrite_dockerfile(&integration.path, content, source, target)
    } else if lowercase.ends_with(".yml") || lowercase.ends_with(".yaml") {
        rewrite_yaml(&integration.path, content, source, target)
    } else {
        rewrite_automation(&integration.path, content, source, target)
    }
}

pub(super) fn transform_integrations(
    root: &Path,
    inspection: &ProjectInspection,
    source: PackageManagerId,
    target: PackageManagerId,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Vec<PlannedFileMutation>> {
    let mut mutations = Vec::new();
    for integration in &inspection.integrations {
        let Some(content) = read_text(&root.join(&integration.path))? else {
            continue;
        };
        let rewritten = rewrite_integration(integration, &content, source, target);
        diagnostics.extend(rewritten.diagnostics);
        if let Some(change) = mutation(
            root,
            &integration.path,
            MutationAction::Write,
            Some(rewritten.content),
            "Translate registered repository integration commands and dependency state references.",
            vec!["integration.translate-commands".to_owned()],
        )? {
            mutations.push(change);
        }
    }
    Ok(mutations)
}

#[cfg(test)]
mod tests;
