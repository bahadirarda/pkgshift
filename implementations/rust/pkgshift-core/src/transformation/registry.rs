use super::{BTreeMap, Diagnostic, Map, Value, append_yaml_mapping, yaml_single_quoted};

#[derive(Default)]
pub(super) struct YarnRegistryConfiguration {
    always_auth: Option<bool>,
    registry_server: Option<String>,
    registries: BTreeMap<String, String>,
    scopes: BTreeMap<String, String>,
}

fn environment_reference(value: &str) -> bool {
    let Some(name) = value
        .strip_prefix("${")
        .and_then(|value| value.strip_suffix('}'))
    else {
        return false;
    };
    let mut characters = name.chars();
    characters
        .next()
        .is_some_and(|character| character == '_' || character.is_ascii_alphabetic())
        && characters.all(|character| character == '_' || character.is_ascii_alphanumeric())
}

pub(super) fn npmrc_for_yarn(
    content: &str,
    diagnostics: &mut Vec<Diagnostic>,
) -> YarnRegistryConfiguration {
    let mut output = YarnRegistryConfiguration::default();
    for raw_line in content.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        let Some((setting, value)) = line.split_once('=') else {
            diagnostics.push(Diagnostic::blocking(
                "NPMRC_SETTING_UNSUPPORTED",
                "Yarn Modern translation found an unsupported .npmrc setting.",
                vec!["Reduce .npmrc to supported registry and authentication settings.".to_owned()],
            ));
            continue;
        };
        let setting = setting.trim();
        let value = value.trim();
        if setting == "node-linker" {
            if matches!(value, "pnp" | "isolated" | "hoisted" | "node-modules") {
                continue;
            }
            diagnostics.push(Diagnostic::blocking(
                "NPMRC_SETTING_UNSUPPORTED",
                "Yarn Modern translation found an unsupported legacy node-linker value.",
                vec!["Use pnp, isolated, hoisted, or node-modules before retrying.".to_owned()],
            ));
            continue;
        }
        if setting == "registry" {
            output.registry_server = Some(value.to_owned());
            continue;
        }
        if let Some(scope) = setting
            .strip_prefix('@')
            .and_then(|value| value.strip_suffix(":registry"))
            .filter(|scope| !scope.is_empty() && !scope.chars().any(char::is_whitespace))
        {
            output.scopes.insert(scope.to_owned(), value.to_owned());
            continue;
        }
        if let Some(registry) = setting
            .strip_suffix(":_authToken")
            .filter(|registry| registry.starts_with("//") && registry.len() > 2)
        {
            if environment_reference(value) {
                output
                    .registries
                    .insert(registry.to_owned(), value.to_owned());
            } else {
                diagnostics.push(Diagnostic::blocking(
                    "REGISTRY_SECRET_REQUIRES_ENVIRONMENT_REFERENCE",
                    "Yarn Modern registry migration requires authentication tokens to use an environment reference.",
                    vec!["Replace the literal token in .npmrc with a ${NAME} reference.".to_owned()],
                ));
            }
            continue;
        }
        if setting == "always-auth" && matches!(value, "true" | "false") {
            output.always_auth = Some(value == "true");
            continue;
        }
        diagnostics.push(Diagnostic::blocking(
            "NPMRC_SETTING_UNSUPPORTED",
            "Yarn Modern translation found an unsupported .npmrc setting.",
            vec!["Reduce .npmrc to supported registry and authentication settings.".to_owned()],
        ));
    }
    output
}

pub(super) fn apply_vlt_registry_to_yarn(
    configuration: &Map<String, Value>,
    output: &mut YarnRegistryConfiguration,
) {
    let configuration = configuration
        .get("config")
        .and_then(Value::as_object)
        .unwrap_or(configuration);
    if let Some(registry) = configuration.get("registry").and_then(Value::as_str) {
        output.registry_server = Some(registry.to_owned());
    }
    if let Some(scopes) = configuration
        .get("scoped-registries")
        .and_then(Value::as_object)
    {
        output
            .scopes
            .extend(scopes.iter().filter_map(|(scope, value)| {
                Some((
                    scope.strip_prefix('@')?.to_owned(),
                    value.as_str()?.to_owned(),
                ))
            }));
    }
}

pub(super) fn npmrc_for_vlt(
    content: Option<&str>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Map<String, Value> {
    let mut configuration = Map::from_iter([(
        "registry".to_owned(),
        Value::String("https://registry.npmjs.org/".to_owned()),
    )]);
    let mut scopes = Map::new();
    let mut reported_authentication = false;
    for raw_line in content.into_iter().flat_map(str::lines) {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        let Some((setting, value)) = line.split_once('=') else {
            diagnostics.push(Diagnostic::blocking(
                "NPMRC_SETTING_UNSUPPORTED",
                "vlt translation found an unsupported .npmrc setting.",
                vec!["Reduce .npmrc to registry and scope mappings before retrying.".to_owned()],
            ));
            continue;
        };
        let setting = setting.trim();
        let value = value.trim();
        if setting == "registry" {
            configuration.insert("registry".to_owned(), Value::String(value.to_owned()));
        } else if setting.starts_with('@') && setting.ends_with(":registry") {
            scopes.insert(
                setting.trim_end_matches(":registry").to_owned(),
                Value::String(value.to_owned()),
            );
        } else if setting.ends_with("_authToken")
            || setting.ends_with("_auth")
            || setting.ends_with("_password")
            || setting.ends_with("username")
            || setting == "always-auth"
        {
            if !reported_authentication {
                diagnostics.push(Diagnostic::blocking(
                    "VLT_REGISTRY_AUTH_MANUAL_REQUIRED",
                    "vlt keeps registry credentials outside vlt.json.",
                    vec!["Authenticate with vlt login after the migration.".to_owned()],
                ));
                reported_authentication = true;
            }
        } else if setting != "node-linker" {
            diagnostics.push(Diagnostic::blocking(
                "NPMRC_SETTING_UNSUPPORTED",
                "vlt translation found an unsupported .npmrc setting.",
                vec!["Reduce .npmrc to registry and scope mappings before retrying.".to_owned()],
            ));
        }
    }
    if !scopes.is_empty() {
        configuration.insert("scoped-registries".to_owned(), Value::Object(scopes));
    }
    Map::from_iter([("config".to_owned(), Value::Object(configuration))])
}

pub(super) fn npmrc_from_vlt(configuration: &Map<String, Value>) -> Option<String> {
    let configuration = configuration
        .get("config")
        .and_then(Value::as_object)
        .unwrap_or(configuration);
    let mut lines = Vec::new();
    if let Some(registry) = configuration.get("registry").and_then(Value::as_str) {
        lines.push(format!("registry={registry}"));
    }
    if let Some(scopes) = configuration
        .get("scoped-registries")
        .and_then(Value::as_object)
    {
        lines.extend(
            scopes
                .iter()
                .filter_map(|(scope, value)| Some(format!("{scope}:registry={}", value.as_str()?))),
        );
    }
    if lines.is_empty() {
        None
    } else {
        lines.push(String::new());
        Some(lines.join("\n"))
    }
}

pub(super) fn render_yarn_configuration(
    node_linker: &str,
    lifecycle_policy_present: bool,
    registry: &YarnRegistryConfiguration,
    package_extensions: &Map<String, Value>,
) -> String {
    let mut lines = vec![format!("nodeLinker: {node_linker}")];
    if lifecycle_policy_present {
        lines.push("enableScripts: false".to_owned());
    }
    if !package_extensions.is_empty() {
        append_yaml_mapping(&mut lines, "packageExtensions", package_extensions);
    }
    if let Some(server) = &registry.registry_server {
        lines.push(format!("npmRegistryServer: {}", yaml_single_quoted(server)));
    }
    if let Some(always_auth) = registry.always_auth {
        lines.push(format!("npmAlwaysAuth: {always_auth}"));
    }
    if !registry.scopes.is_empty() {
        lines.push("npmScopes:".to_owned());
        for (scope, server) in &registry.scopes {
            lines.push(format!("  {}:", yaml_single_quoted(scope)));
            lines.push(format!(
                "    npmRegistryServer: {}",
                yaml_single_quoted(server)
            ));
        }
    }
    if !registry.registries.is_empty() {
        lines.push("npmRegistries:".to_owned());
        for (registry, token) in &registry.registries {
            lines.push(format!("  {}:", yaml_single_quoted(registry)));
            lines.push("    npmAlwaysAuth: true".to_owned());
            lines.push(format!("    npmAuthToken: {}", yaml_single_quoted(token)));
        }
    }
    lines.push(String::new());
    lines.join("\n")
}
