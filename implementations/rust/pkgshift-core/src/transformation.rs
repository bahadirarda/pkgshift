use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde_json::{Map, Value};

use crate::catalog::get_package_manager;
use crate::model::{
    CapabilityAnalysis, CapabilityClassification, CapabilityDecision, Diagnostic, MutationAction,
    PackageManagerId, PlannedFileMutation, ProjectInspection, ProjectIr,
};
use crate::util::{PkgshiftError, Result, digest_text, read_json_object, read_text};

pub(crate) fn json_content(value: &Map<String, Value>) -> Result<String> {
    let mut content =
        serde_json::to_string_pretty(value).map_err(|source| PkgshiftError::Json {
            path: "<manifest>".into(),
            source,
        })?;
    content.push('\n');
    Ok(content)
}

fn mutation(
    root: &Path,
    path: &str,
    action: MutationAction,
    content: Option<String>,
    reason: impl Into<String>,
    capabilities: Vec<String>,
) -> Result<Option<PlannedFileMutation>> {
    let before = read_text(&root.join(path))?;
    if action == MutationAction::Write && before == content {
        return Ok(None);
    }
    if action == MutationAction::Delete && before.is_none() {
        return Ok(None);
    }
    Ok(Some(PlannedFileMutation {
        path: path.to_owned(),
        action,
        before_digest: before.as_deref().map(digest_text),
        after_digest: content.as_deref().map(digest_text),
        content,
        reason: reason.into(),
        capabilities,
    }))
}

fn package_version_by_name(project_ir: &ProjectIr) -> BTreeMap<String, String> {
    project_ir
        .packages
        .iter()
        .filter_map(|package| Some((package.name.clone()?, package.version.clone()?)))
        .collect()
}

fn transform_specifier(
    name: &str,
    specifier: &str,
    decision: &CapabilityDecision,
    versions: &BTreeMap<String, String>,
) -> Option<String> {
    match decision.transformation_id.as_deref()? {
        "workspace.expand-to-semver" => {
            let suffix = specifier.strip_prefix("workspace:")?;
            let version = versions.get(name);
            match suffix {
                "*" => Some(version.cloned().unwrap_or_else(|| "*".to_owned())),
                "^" => Some(version.map_or_else(|| "*".to_owned(), |value| format!("^{value}"))),
                "~" => Some(version.map_or_else(|| "*".to_owned(), |value| format!("~{value}"))),
                other => Some(other.to_owned()),
            }
        }
        "portal.to-file" | "link.to-file" => specifier
            .split_once(':')
            .map(|(_, value)| format!("file:{value}")),
        "portal.to-link" => specifier
            .strip_prefix("portal:")
            .map(|value| format!("link:{value}")),
        _ => None,
    }
}

fn parse_pnpm_catalogs(content: &str) -> (Map<String, Value>, Map<String, Value>) {
    let mut catalog = Map::new();
    let mut catalogs = Map::new();
    let mut section = "";
    let mut named_catalog = "";
    for line in content.lines() {
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            continue;
        }
        let indent = line.len() - line.trim_start().len();
        let trimmed = line.trim();
        if indent == 0 && trimmed.ends_with(':') {
            section = trimmed.trim_end_matches(':');
            named_catalog = "";
            continue;
        }
        if section == "catalog" && indent >= 2 {
            if let Some((key, value)) = trimmed.split_once(':') {
                catalog.insert(
                    key.trim().to_owned(),
                    Value::String(value.trim().trim_matches(['\'', '"']).to_owned()),
                );
            }
        } else if section == "catalogs" {
            if indent == 2 && trimmed.ends_with(':') {
                named_catalog = trimmed.trim_end_matches(':');
                catalogs.insert(named_catalog.to_owned(), Value::Object(Map::new()));
            } else if indent >= 4
                && let Some((key, value)) = trimmed.split_once(':')
                && let Some(entries) = catalogs
                    .get_mut(named_catalog)
                    .and_then(Value::as_object_mut)
            {
                entries.insert(
                    key.trim().to_owned(),
                    Value::String(value.trim().trim_matches(['\'', '"']).to_owned()),
                );
            }
        }
    }
    (catalog, catalogs)
}

fn yaml_single_quoted(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn yaml_key(value: &str) -> String {
    let mut characters = value.chars();
    if characters
        .next()
        .is_some_and(|character| character.is_ascii_alphabetic() || character == '_')
        && characters.all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '_' | '.' | '-')
        })
    {
        value.to_owned()
    } else {
        yaml_single_quoted(value)
    }
}

fn append_yaml_entry(lines: &mut Vec<String>, indent: usize, key: &str, value: &Value) {
    let prefix = " ".repeat(indent);
    let key = yaml_key(key);
    match value {
        Value::Object(entries) if entries.is_empty() => {
            lines.push(format!("{prefix}{key}: {{}}"));
        }
        Value::Object(entries) => {
            lines.push(format!("{prefix}{key}:"));
            for (entry_key, entry_value) in entries {
                append_yaml_entry(lines, indent + 2, entry_key, entry_value);
            }
        }
        Value::Array(entries) if entries.is_empty() => {
            lines.push(format!("{prefix}{key}: []"));
        }
        Value::Array(entries) => {
            lines.push(format!("{prefix}{key}:"));
            for entry in entries {
                match entry {
                    Value::String(value) => {
                        lines.push(format!("{prefix}  - {}", yaml_single_quoted(value)));
                    }
                    Value::Bool(value) => lines.push(format!("{prefix}  - {value}")),
                    Value::Number(value) => lines.push(format!("{prefix}  - {value}")),
                    Value::Null => lines.push(format!("{prefix}  - null")),
                    Value::Array(_) | Value::Object(_) => {
                        lines.push(format!(
                            "{prefix}  - {}",
                            yaml_single_quoted(&entry.to_string())
                        ));
                    }
                }
            }
        }
        Value::String(value) => {
            lines.push(format!("{prefix}{key}: {}", yaml_single_quoted(value)));
        }
        Value::Bool(value) => lines.push(format!("{prefix}{key}: {value}")),
        Value::Number(value) => lines.push(format!("{prefix}{key}: {value}")),
        Value::Null => lines.push(format!("{prefix}{key}: null")),
    }
}

fn append_yaml_mapping(lines: &mut Vec<String>, key: &str, entries: &Map<String, Value>) {
    append_yaml_entry(lines, 0, key, &Value::Object(entries.clone()));
}

pub(super) fn package_name_end(selector: &str) -> Option<usize> {
    if selector.starts_with('@') {
        let slash = selector.find('/')?;
        if slash == 1 || slash + 1 == selector.len() {
            return None;
        }
        selector[slash + 1..]
            .find('@')
            .map_or(Some(selector.len()), |offset| Some(slash + 1 + offset))
    } else {
        if selector.starts_with('@') || selector.starts_with('.') || selector.starts_with('-') {
            return None;
        }
        selector.find('@').or(Some(selector.len()))
    }
}

fn valid_package_extension_selector(selector: &str) -> bool {
    let Some(name_end) = package_name_end(selector) else {
        return false;
    };
    let name = &selector[..name_end];
    let range = if name_end < selector.len() {
        selector.get(name_end + 1..)
    } else {
        None
    };
    !name.is_empty()
        && !name.chars().any(char::is_whitespace)
        && !name.contains(['#', ':', '\\'])
        && range.is_none_or(|value| {
            !value.is_empty() && !value.contains(['\n', '\r']) && !value.starts_with([':', '#'])
        })
}

fn valid_string_map(value: Option<&Value>) -> bool {
    value.is_some_and(|value| {
        value.as_object().is_some_and(|entries| {
            entries
                .iter()
                .all(|(name, range)| !name.is_empty() && range.is_string())
        })
    })
}

fn valid_peer_dependencies_meta(value: Option<&Value>) -> bool {
    value.is_some_and(|value| {
        value.as_object().is_some_and(|entries| {
            entries.iter().all(|(name, metadata)| {
                !name.is_empty()
                    && metadata.as_object().is_some_and(|metadata| {
                        metadata.keys().all(|key| key == "optional")
                            && metadata.get("optional").and_then(Value::as_bool).is_some()
                    })
            })
        })
    })
}

fn valid_package_extensions(entries: &Map<String, Value>) -> bool {
    entries.iter().all(|(selector, extension)| {
        valid_package_extension_selector(selector)
            && extension.as_object().is_some_and(|extension| {
                extension.iter().all(|(field, value)| match field.as_str() {
                    "dependencies" | "optionalDependencies" | "peerDependencies" => {
                        valid_string_map(Some(value))
                    }
                    "peerDependenciesMeta" => valid_peer_dependencies_meta(Some(value)),
                    _ => false,
                })
            })
    })
}

fn source_package_extensions(
    source: PackageManagerId,
    root_manifest: &Map<String, Value>,
    pnpm_manifest: &Map<String, Value>,
    pnpm_configuration: &Map<String, Value>,
    yarn_configuration: &Map<String, Value>,
) -> Map<String, Value> {
    let root = root_manifest.get("packageExtensions");
    let pnpm_manifest = pnpm_manifest.get("packageExtensions");
    let pnpm_configuration = pnpm_configuration.get("packageExtensions");
    let yarn = yarn_configuration.get("packageExtensions");
    let candidates = match source {
        PackageManagerId::Pnpm => [pnpm_configuration, pnpm_manifest, root, yarn],
        PackageManagerId::YarnModern => [yarn, root, pnpm_configuration, pnpm_manifest],
        _ => [root, pnpm_configuration, pnpm_manifest, yarn],
    };
    candidates
        .into_iter()
        .filter_map(|candidate| candidate.and_then(Value::as_object))
        .find(|entries| !entries.is_empty())
        .cloned()
        .unwrap_or_default()
}

fn render_pnpm_workspace(
    patterns: &[String],
    catalog: &Map<String, Value>,
    catalogs: &Map<String, Value>,
    overrides: &Map<String, Value>,
    package_extensions: &Map<String, Value>,
    patched_dependencies: &Map<String, Value>,
    node_linker: Option<&str>,
    trusted_dependencies: &[String],
    lifecycle_policy_present: bool,
) -> String {
    let mut lines = Vec::new();
    if !patterns.is_empty() {
        lines.push("packages:".to_owned());
        for pattern in patterns {
            lines.push(format!("  - {}", yaml_single_quoted(pattern)));
        }
    }
    if !catalog.is_empty() {
        lines.push("catalog:".to_owned());
        for (name, value) in catalog {
            if let Some(value) = value.as_str() {
                lines.push(format!("  {name}: {}", yaml_single_quoted(value)));
            }
        }
    }
    if !catalogs.is_empty() {
        lines.push("catalogs:".to_owned());
        for (catalog_name, value) in catalogs {
            lines.push(format!("  {catalog_name}:"));
            if let Some(entries) = value.as_object() {
                for (name, value) in entries {
                    if let Some(value) = value.as_str() {
                        lines.push(format!("    {name}: {}", yaml_single_quoted(value)));
                    }
                }
            }
        }
    }
    if !package_extensions.is_empty() {
        append_yaml_mapping(&mut lines, "packageExtensions", package_extensions);
    }
    if !patched_dependencies.is_empty() {
        append_yaml_mapping(&mut lines, "patchedDependencies", patched_dependencies);
    }
    if !overrides.is_empty() {
        lines.push("overrides:".to_owned());
        for (selector, value) in overrides {
            if let Some(value) = value.as_str() {
                lines.push(format!(
                    "  {}: {}",
                    yaml_single_quoted(selector),
                    yaml_single_quoted(value)
                ));
            }
        }
    }
    if let Some(node_linker) = node_linker {
        lines.push(format!("nodeLinker: {node_linker}"));
    }
    if lifecycle_policy_present {
        if trusted_dependencies.is_empty() {
            lines.push("allowBuilds: {}".to_owned());
        } else {
            lines.push("allowBuilds:".to_owned());
            for dependency in trusted_dependencies {
                lines.push(format!("  {}: true", yaml_single_quoted(dependency)));
            }
        }
    }
    lines.push(String::new());
    lines.join("\n")
}

fn string_array(value: Option<&Value>) -> impl Iterator<Item = &str> {
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
}

fn source_trusted_dependencies(
    root_manifest: &Map<String, Value>,
    pnpm_manifest: &Map<String, Value>,
    pnpm_configuration: &Map<String, Value>,
    yarn_configuration: &Map<String, Value>,
) -> Vec<String> {
    let mut trusted = BTreeSet::new();
    for value in [
        root_manifest.get("trustedDependencies"),
        pnpm_manifest.get("onlyBuiltDependencies"),
        pnpm_configuration.get("onlyBuiltDependencies"),
    ] {
        trusted.extend(string_array(value).map(str::to_owned));
    }
    if let Some(allow_builds) = pnpm_configuration
        .get("allowBuilds")
        .and_then(Value::as_object)
    {
        trusted.extend(
            allow_builds
                .iter()
                .filter(|(_, allowed)| allowed.as_bool() == Some(true))
                .map(|(name, _)| name.clone()),
        );
    }
    if yarn_configuration
        .get("enableScripts")
        .and_then(Value::as_bool)
        == Some(false)
        && let Some(dependencies_meta) = root_manifest
            .get("dependenciesMeta")
            .and_then(Value::as_object)
    {
        trusted.extend(
            dependencies_meta
                .iter()
                .filter(|(_, metadata)| {
                    metadata
                        .as_object()
                        .and_then(|entry| entry.get("built"))
                        .and_then(Value::as_bool)
                        == Some(true)
                })
                .map(|(name, _)| name.clone()),
        );
    }
    trusted.into_iter().collect()
}

fn remove_source_lifecycle_policy(
    manifest: &mut Map<String, Value>,
    remove_yarn_build_policy: bool,
) {
    manifest.remove("trustedDependencies");
    if let Some(pnpm) = manifest.get_mut("pnpm").and_then(Value::as_object_mut) {
        pnpm.remove("onlyBuiltDependencies");
        pnpm.remove("allowBuilds");
        if pnpm.is_empty() {
            manifest.remove("pnpm");
        }
    }
    if !remove_yarn_build_policy {
        return;
    }
    let Some(dependencies_meta) = manifest
        .get_mut("dependenciesMeta")
        .and_then(Value::as_object_mut)
    else {
        return;
    };
    dependencies_meta.retain(|_, value| {
        let Some(metadata) = value.as_object_mut() else {
            return true;
        };
        metadata.remove("built");
        !metadata.is_empty()
    });
    if dependencies_meta.is_empty() {
        manifest.remove("dependenciesMeta");
    }
}

fn configure_yarn_lifecycle_policy(
    manifest: &mut Map<String, Value>,
    trusted_dependencies: &[String],
) {
    if trusted_dependencies.is_empty() {
        return;
    }
    let dependencies_meta = manifest
        .entry("dependenciesMeta")
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .expect("dependenciesMeta is initialized as an object");
    for dependency in trusted_dependencies {
        let metadata = dependencies_meta
            .entry(dependency.clone())
            .or_insert_with(|| Value::Object(Map::new()))
            .as_object_mut()
            .expect("dependency metadata is initialized as an object");
        metadata.insert("built".to_owned(), Value::Bool(true));
    }
}

mod registry;

use registry::{
    apply_vlt_registry_to_yarn, npmrc_for_vlt, npmrc_for_yarn, npmrc_from_vlt,
    render_yarn_configuration,
};
fn render_bun_configuration(before: Option<&str>, isolated: bool) -> Option<String> {
    if !isolated {
        return before.map(str::to_owned);
    }
    let before = before.unwrap_or_default();
    let mut lines = before.lines().map(str::to_owned).collect::<Vec<_>>();
    let install_sections = lines
        .iter()
        .enumerate()
        .filter(|(_, line)| line.trim() == "[install]")
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    if install_sections.len() > 1 {
        return None;
    }
    if let Some(start) = install_sections.first().copied() {
        let end = lines
            .iter()
            .enumerate()
            .skip(start + 1)
            .find(|(_, line)| {
                let line = line.trim();
                line.starts_with('[') && line.ends_with(']')
            })
            .map_or(lines.len(), |(index, _)| index);
        let linkers = lines[start + 1..end]
            .iter()
            .enumerate()
            .filter(|(_, line)| {
                line.split_once('=')
                    .is_some_and(|(key, _)| key.trim() == "linker")
            })
            .map(|(index, _)| start + 1 + index)
            .collect::<Vec<_>>();
        if linkers.len() > 1 {
            return None;
        }
        if let Some(index) = linkers.first().copied() {
            lines[index] = "linker = \"isolated\"".to_owned();
        } else {
            lines.insert(start + 1, "linker = \"isolated\"".to_owned());
        }
    } else {
        if lines.last().is_some_and(|line| !line.trim().is_empty()) {
            lines.push(String::new());
        }
        lines.push("[install]".to_owned());
        lines.push("linker = \"isolated\"".to_owned());
    }
    lines.push(String::new());
    Some(lines.join("\n"))
}

fn flatten_nested_overrides(
    overrides: &Map<String, Value>,
    separator: &str,
) -> Option<Map<String, Value>> {
    let mut flattened = Map::new();
    for (parent, value) in overrides {
        if let Some(value) = value.as_str() {
            flattened.insert(parent.clone(), Value::String(value.to_owned()));
            continue;
        }
        let children = value.as_object()?;
        for (child, value) in children {
            let value = value.as_str()?;
            let selector = if child == "." {
                parent.clone()
            } else {
                format!("{parent}{separator}{child}")
            };
            flattened.insert(selector, Value::String(value.to_owned()));
        }
    }
    Some(flattened)
}

fn compatible_resolutions(resolutions: &Map<String, Value>) -> Option<Map<String, Value>> {
    let mut overrides = Map::new();
    for (selector, value) in resolutions {
        let has_selector_syntax = |part: &str| {
            part.chars()
                .any(|character| matches!(character, '/' | '@' | '*'))
        };
        let bare_package = selector.strip_prefix('@').map_or_else(
            || !selector.is_empty() && !has_selector_syntax(selector),
            |scoped| {
                scoped.split_once('/').is_some_and(|(scope, name)| {
                    !scope.is_empty()
                        && !name.is_empty()
                        && !has_selector_syntax(scope)
                        && !has_selector_syntax(name)
                })
            },
        );
        if !bare_package || !value.is_string() {
            return None;
        }
        overrides.insert(selector.clone(), value.clone());
    }
    Some(overrides)
}

fn valid_bare_package_name(value: &str) -> bool {
    if let Some(scoped) = value.strip_prefix('@') {
        return scoped.split_once('/').is_some_and(|(scope, name)| {
            !scope.is_empty()
                && !name.is_empty()
                && !scope.chars().any(|value| matches!(value, '@' | '/' | ' '))
                && !name.chars().any(|value| matches!(value, '@' | '/' | ' '))
        });
    }
    !value.is_empty()
        && !value
            .chars()
            .any(|character| matches!(character, '@' | '/' | ' '))
}

fn vlt_modifiers_to_overrides(modifiers: &Map<String, Value>) -> Option<Map<String, Value>> {
    let mut output = Map::new();
    for (selector, resolution) in modifiers {
        let resolution = resolution.as_str()?.to_owned();
        if let Some(name) = selector.strip_prefix('#')
            && !name.contains(" > ")
            && valid_bare_package_name(name)
        {
            if let Some(existing) = output.get_mut(name).and_then(Value::as_object_mut) {
                existing.insert(".".to_owned(), Value::String(resolution));
            } else {
                output.insert(name.to_owned(), Value::String(resolution));
            }
            continue;
        }
        let context = selector.strip_prefix(":root > #")?;
        let (parent, child) = context.split_once(" > #")?;
        if !valid_bare_package_name(parent) || !valid_bare_package_name(child) {
            return None;
        }
        let existing = output.remove(parent);
        let mut nested = match existing {
            Some(Value::Object(value)) => value,
            Some(Value::String(value)) => Map::from_iter([(".".to_owned(), Value::String(value))]),
            Some(_) => return None,
            None => Map::new(),
        };
        nested.insert(child.to_owned(), Value::String(resolution));
        output.insert(parent.to_owned(), Value::Object(nested));
    }
    Some(output)
}

fn overrides_to_vlt_modifiers(overrides: &Map<String, Value>) -> Option<Map<String, Value>> {
    let mut output = Map::new();
    for (parent, value) in overrides {
        if !valid_bare_package_name(parent) {
            return None;
        }
        if let Some(resolution) = value.as_str() {
            output.insert(format!("#{parent}"), Value::String(resolution.to_owned()));
            continue;
        }
        for (child, resolution) in value.as_object()? {
            let resolution = resolution.as_str()?;
            let selector = if child == "." {
                format!("#{parent}")
            } else if valid_bare_package_name(child) {
                format!(":root > #{parent} > #{child}")
            } else {
                return None;
            };
            output.insert(selector, Value::String(resolution.to_owned()));
        }
    }
    Some(output)
}

fn resolutions_to_vlt_modifiers(resolutions: &Map<String, Value>) -> Option<Map<String, Value>> {
    let mut output = Map::new();
    for (selector, resolution) in resolutions {
        let resolution = resolution.as_str()?;
        if valid_bare_package_name(selector) {
            output.insert(format!("#{selector}"), Value::String(resolution.to_owned()));
            continue;
        }
        let (parent, child) = selector.split_once('/')?;
        if parent.starts_with('@')
            || !valid_bare_package_name(parent)
            || !valid_bare_package_name(child)
        {
            return None;
        }
        output.insert(
            format!(":root > #{parent} > #{child}"),
            Value::String(resolution.to_owned()),
        );
    }
    Some(output)
}

mod commands;
mod integration;
mod patch;
mod project;

pub(crate) use project::transform_project;
