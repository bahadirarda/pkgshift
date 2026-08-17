use super::commands::rewrite_package_manager_commands;
use super::integration::{rewrite_manifest_toolchain_pins, transform_integrations};
use super::{
    BTreeMap, BTreeSet, CapabilityAnalysis, CapabilityClassification, Diagnostic, Map,
    MutationAction, PackageManagerId, Path, PlannedFileMutation, ProjectInspection, ProjectIr,
    Result, Value, apply_vlt_registry_to_yarn, compatible_resolutions,
    configure_yarn_lifecycle_policy, flatten_nested_overrides, get_package_manager, json_content,
    mutation, npmrc_for_vlt, npmrc_for_yarn, npmrc_from_vlt, overrides_to_vlt_modifiers,
    package_version_by_name, parse_pnpm_catalogs, read_json_object, read_text,
    remove_source_lifecycle_policy, render_bun_configuration, render_pnpm_workspace,
    render_yarn_configuration, resolutions_to_vlt_modifiers, source_package_extensions,
    source_patched_dependencies, source_trusted_dependencies, transform_specifier,
    valid_package_extensions, validated_patched_dependencies, vlt_modifiers_to_overrides,
    yarn_patch_conversion, yarn_patch_name, yarn_patch_resolutions,
};

pub(crate) struct Transformation {
    pub(crate) manifest_mutations: Vec<PlannedFileMutation>,
    pub(crate) configuration_mutations: Vec<PlannedFileMutation>,
    pub(crate) integration_mutations: Vec<PlannedFileMutation>,
    pub(crate) cleanup_mutations: Vec<PlannedFileMutation>,
    pub(crate) diagnostics: Vec<Diagnostic>,
}

#[allow(clippy::too_many_lines)]
pub(crate) fn transform_project(
    inspection: &ProjectInspection,
    project_ir: &ProjectIr,
    analysis: &CapabilityAnalysis,
    target: PackageManagerId,
) -> Result<Transformation> {
    let root = Path::new(&inspection.root);
    let mut diagnostics = Vec::new();
    let supported_transformations = [
        "workspace.expand-to-semver",
        "portal.to-file",
        "portal.to-link",
        "link.to-file",
        "catalog.expand-to-range",
        "overrides.to-pnpm",
        "overrides.to-resolutions",
        "overrides.nested-to-selector",
        "overrides.nested-to-resolutions",
        "resolutions.to-overrides",
        "resolutions.to-pnpm-overrides",
        "patch.yarn-to-pnpm",
        "patch.yarn-to-bun",
        "patch.patched-to-yarn",
        "linker.pnp-to-node-modules",
        "linker.pnp-to-isolated",
        "linker.isolated-to-yarn-pnpm",
        "linker.isolated-to-hoisted",
        "registry.npmrc-to-yarnrc",
        "lifecycle.to-pnpm-build-policy",
        "lifecycle.to-yarn-build-policy",
        "workspace.to-vlt-workspace",
        "workspace.to-deno-workspace",
        "overrides.to-vlt-modifiers",
        "resolutions.to-vlt-modifiers",
        "registry.npmrc-to-vlt",
    ];
    for decision in &analysis.decisions {
        if matches!(
            decision.classification,
            CapabilityClassification::Transform | CapabilityClassification::Lossy
        ) && decision
            .transformation_id
            .as_deref()
            .is_some_and(|id| !supported_transformations.contains(&id))
        {
            diagnostics.push(Diagnostic::blocking(
                "TRANSFORMATION_UNIMPLEMENTED",
                format!(
                    "The Rust renderer does not yet implement {}.",
                    decision.transformation_id.as_deref().unwrap_or("unknown")
                ),
                vec![
                    "Keep the TypeScript implementation as the execution boundary for this capability."
                        .to_owned(),
                ],
            ));
        }
    }

    let decisions = analysis
        .decisions
        .iter()
        .map(|decision| (decision.feature_id.as_str(), decision))
        .collect::<BTreeMap<_, _>>();
    let versions = package_version_by_name(project_ir);
    let mut manifest_mutations = Vec::new();
    let pnpm_workspace = read_text(&root.join("pnpm-workspace.yaml"))?;
    let (mut pnpm_catalog, mut pnpm_catalogs) = pnpm_workspace
        .as_deref()
        .map(parse_pnpm_catalogs)
        .unwrap_or_default();
    let root_manifest_before = read_json_object(&root.join("package.json"))?.unwrap_or_default();
    if let Some(catalog) = root_manifest_before
        .get("catalog")
        .and_then(Value::as_object)
    {
        pnpm_catalog.extend(catalog.clone());
    }
    if let Some(catalogs) = root_manifest_before
        .get("catalogs")
        .and_then(Value::as_object)
    {
        pnpm_catalogs.extend(catalogs.clone());
    }
    let vlt_configuration = read_json_object(&root.join("vlt.json"))?.unwrap_or_default();
    if let Some(catalog) = vlt_configuration.get("catalog").and_then(Value::as_object) {
        pnpm_catalog.extend(catalog.clone());
    }
    if let Some(catalogs) = vlt_configuration.get("catalogs").and_then(Value::as_object) {
        pnpm_catalogs.extend(catalogs.clone());
    }
    let deno_location = if root.join("deno.json").is_file() {
        "deno.json"
    } else if root.join("deno.jsonc").is_file() {
        "deno.jsonc"
    } else {
        "deno.json"
    };
    let mut deno_configuration = read_text(&root.join(deno_location))?
        .as_deref()
        .and_then(|content| json5::from_str::<Value>(content).ok())
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default();
    let manifest_overrides = root_manifest_before
        .get("overrides")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let pnpm_manifest = root_manifest_before
        .get("pnpm")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let pnpm_configuration = pnpm_workspace
        .as_deref()
        .and_then(|content| noyalib::from_str::<Value>(content).ok())
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default();
    let yarn_configuration = read_text(&root.join(".yarnrc.yml"))?
        .as_deref()
        .and_then(|content| noyalib::from_str::<Value>(content).ok())
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default();
    let source = inspection
        .package_manager
        .selected
        .expect("planning requires a selected source");
    if target == PackageManagerId::Deno {
        for dependency in project_ir
            .packages
            .iter()
            .flat_map(|package| &package.dependencies)
        {
            if matches!(
                dependency.protocol,
                crate::model::DependencyProtocol::File
                    | crate::model::DependencyProtocol::Link
                    | crate::model::DependencyProtocol::Portal
                    | crate::model::DependencyProtocol::Patch
                    | crate::model::DependencyProtocol::Git
                    | crate::model::DependencyProtocol::Url
                    | crate::model::DependencyProtocol::Unknown
            ) {
                diagnostics.push(Diagnostic::blocking(
                    "DENO_DEPENDENCY_PROTOCOL_UNSUPPORTED",
                    format!(
                        "Deno package.json dependency mode cannot preserve the specifier for {}.",
                        dependency.name
                    ),
                    vec![
                        "Move the dependency to a supported npm, JSR, workspace, or semver declaration before retrying."
                            .to_owned(),
                    ],
                ));
            }
        }
    }
    let mut package_extensions = source_package_extensions(
        source,
        &root_manifest_before,
        &pnpm_manifest,
        &pnpm_configuration,
        &yarn_configuration,
    );
    if !valid_package_extensions(&package_extensions) {
        diagnostics.push(Diagnostic::blocking(
            "PACKAGE_EXTENSIONS_UNSUPPORTED",
            "Package extensions contain a selector or field outside the deterministic shared subset.",
            vec![
                "Use package selectors with dependency, optional dependency, peer dependency, or optional peer metadata entries."
                    .to_owned(),
            ],
        ));
        package_extensions.clear();
    }
    let configured_patches = source_patched_dependencies(
        source,
        &root_manifest_before,
        &pnpm_manifest,
        &pnpm_configuration,
    );
    let mut patched_dependencies =
        validated_patched_dependencies(root, &configured_patches, &mut diagnostics)?;
    let mut yarn_patch_conversions = BTreeMap::new();
    if project_ir
        .features
        .iter()
        .any(|feature| feature.id == "dependency.patch-protocol")
    {
        if source != PackageManagerId::YarnModern {
            diagnostics.push(Diagnostic::blocking(
                "PATCH_SOURCE_UNSUPPORTED",
                "A Yarn patch protocol dependency was found outside a Yarn Modern project.",
                vec![
                    "Normalize the project with Yarn Modern before migrating its patch protocol."
                        .to_owned(),
                ],
            ));
        } else {
            for dependency in project_ir
                .packages
                .iter()
                .flat_map(|package| &package.dependencies)
                .filter(|dependency| {
                    matches!(dependency.protocol, crate::model::DependencyProtocol::Patch)
                })
            {
                let Some(conversion) = yarn_patch_conversion(
                    root,
                    &dependency.name,
                    &dependency.specifier,
                    &mut diagnostics,
                )?
                else {
                    continue;
                };
                if let Some(existing) = patched_dependencies.get(&conversion.selector)
                    && existing.as_str() != Some(conversion.path.as_str())
                {
                    diagnostics.push(Diagnostic::blocking(
                        "PATCH_POLICY_CONFLICT",
                        "Multiple patch declarations target the same exact package version with different files.",
                        vec!["Keep one patch file for each exact package version.".to_owned()],
                    ));
                    continue;
                }
                patched_dependencies.insert(
                    conversion.selector.clone(),
                    Value::String(conversion.path.clone()),
                );
                yarn_patch_conversions.insert(dependency.location.clone(), conversion);
            }
            if let Some(resolutions) = root_manifest_before
                .get("resolutions")
                .and_then(Value::as_object)
            {
                for (selector, value) in resolutions {
                    let Some(specifier) = value
                        .as_str()
                        .filter(|specifier| specifier.starts_with("patch:"))
                    else {
                        continue;
                    };
                    let Some(name) = yarn_patch_name(specifier) else {
                        diagnostics.push(Diagnostic::blocking(
                            "PATCH_LOCATOR_UNSUPPORTED",
                            "A Yarn patch resolution does not identify one registry package.",
                            vec![
                                "Regenerate the patch resolution with Yarn patch-commit --save."
                                    .to_owned(),
                            ],
                        ));
                        continue;
                    };
                    let Some(conversion) =
                        yarn_patch_conversion(root, name, specifier, &mut diagnostics)?
                    else {
                        continue;
                    };
                    let exact_resolution = format!("{name}@npm:{}", conversion.base_specifier);
                    if selector != &exact_resolution && selector != &conversion.selector {
                        diagnostics.push(Diagnostic::blocking(
                            "PATCH_SELECTOR_UNSUPPORTED",
                            "A Yarn patch resolution selector does not match its exact patch locator.",
                            vec!["Use the exact name@npm:version selector generated by Yarn.".to_owned()],
                        ));
                        continue;
                    }
                    if let Some(existing) = patched_dependencies.get(&conversion.selector)
                        && existing.as_str() != Some(conversion.path.as_str())
                    {
                        diagnostics.push(Diagnostic::blocking(
                            "PATCH_POLICY_CONFLICT",
                            "Multiple patch declarations target the same exact package version with different files.",
                            vec!["Keep one patch file for each exact package version.".to_owned()],
                        ));
                        continue;
                    }
                    patched_dependencies
                        .insert(conversion.selector.clone(), Value::String(conversion.path));
                }
            }
        }
    }
    let patch_resolutions = yarn_patch_resolutions(&patched_dependencies);
    let trusted_dependencies = source_trusted_dependencies(
        &root_manifest_before,
        &pnpm_manifest,
        &pnpm_configuration,
        &yarn_configuration,
    );
    let lifecycle_policy_present = project_ir
        .features
        .iter()
        .any(|feature| feature.id == "lifecycle.trusted-dependencies");
    let remove_yarn_build_policy = inspection.package_manager.selected
        == Some(PackageManagerId::YarnModern)
        && yarn_configuration
            .get("enableScripts")
            .and_then(Value::as_bool)
            == Some(false);
    if inspection.package_manager.selected == Some(PackageManagerId::YarnModern)
        && !remove_yarn_build_policy
        && root_manifest_before
            .get("dependenciesMeta")
            .and_then(Value::as_object)
            .is_some_and(|entries| {
                entries.values().any(|metadata| {
                    metadata
                        .as_object()
                        .and_then(|entry| entry.get("built"))
                        .and_then(Value::as_bool)
                        == Some(false)
                })
            })
    {
        diagnostics.push(Diagnostic::blocking(
            "YARN_BUILD_POLICY_UNSUPPORTED",
            "The target cannot preserve Yarn per-dependency build denials safely.",
            vec![
                "Remove the denial or convert it to a reviewed lifecycle allow-list before retrying."
                    .to_owned(),
            ],
        ));
    }
    let source_overrides = if source == PackageManagerId::Vlt {
        let modifiers = vlt_configuration
            .get("modifiers")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        if let Some(overrides) = vlt_modifiers_to_overrides(&modifiers) {
            overrides
        } else {
            diagnostics.push(Diagnostic::blocking(
                "VLT_MODIFIER_UNSUPPORTED",
                "vlt modifiers exceed the deterministic selector subset.",
                vec![
                    "Use bare package or one-level :root parent-child selectors before retrying."
                        .to_owned(),
                ],
            ));
            Map::new()
        }
    } else {
        pnpm_configuration
            .get("overrides")
            .and_then(Value::as_object)
            .or_else(|| pnpm_manifest.get("overrides").and_then(Value::as_object))
            .cloned()
            .filter(|overrides| !overrides.is_empty())
            .unwrap_or(manifest_overrides)
    };
    let source_resolutions = root_manifest_before
        .get("resolutions")
        .and_then(Value::as_object)
        .map(|resolutions| {
            resolutions
                .iter()
                .filter(|(_, value)| {
                    !value
                        .as_str()
                        .is_some_and(|specifier| specifier.starts_with("patch:"))
                })
                .map(|(selector, value)| (selector.clone(), value.clone()))
                .collect()
        })
        .unwrap_or_default();
    let yarn_patch_resolution_present = root_manifest_before
        .get("resolutions")
        .and_then(Value::as_object)
        .is_some_and(|resolutions| {
            resolutions.values().any(|value| {
                value
                    .as_str()
                    .is_some_and(|specifier| specifier.starts_with("patch:"))
            })
        });
    let selected_policy = if source_overrides.is_empty() {
        &source_resolutions
    } else {
        &source_overrides
    };
    let mut pnpm_overrides = Map::new();
    let mut vlt_modifiers = Map::new();
    let rendered_policy = match target {
        PackageManagerId::Vlt if !selected_policy.is_empty() => {
            if source_overrides.is_empty() {
                resolutions_to_vlt_modifiers(selected_policy)
            } else {
                overrides_to_vlt_modifiers(selected_policy)
            }
        }
        PackageManagerId::Pnpm if !selected_policy.is_empty() => {
            if source_overrides.is_empty() {
                compatible_resolutions(selected_policy)
            } else {
                flatten_nested_overrides(selected_policy, ">")
            }
        }
        PackageManagerId::YarnClassic | PackageManagerId::YarnModern
            if !source_overrides.is_empty() =>
        {
            flatten_nested_overrides(selected_policy, "/")
        }
        PackageManagerId::Npm | PackageManagerId::Deno
            if source_overrides.is_empty() && !source_resolutions.is_empty() =>
        {
            compatible_resolutions(selected_policy)
        }
        PackageManagerId::Npm | PackageManagerId::Bun | PackageManagerId::Deno
            if !source_overrides.is_empty() =>
        {
            Some(source_overrides.clone())
        }
        _ => Some(Map::new()),
    };
    if let Some(policy) = rendered_policy.as_ref() {
        if target == PackageManagerId::Pnpm {
            pnpm_overrides = policy.clone();
        } else if target == PackageManagerId::Vlt {
            vlt_modifiers = policy.clone();
        }
    } else if source_overrides.is_empty() {
        diagnostics.push(Diagnostic::blocking(
            "RESOLUTION_SELECTOR_UNSUPPORTED",
            "Yarn resolution selectors cannot be translated without reducing selector fidelity.",
            vec!["Review the root resolutions policy before retrying the migration.".to_owned()],
        ));
    } else {
        diagnostics.push(Diagnostic::blocking(
            "NESTED_OVERRIDE_UNSUPPORTED",
            "Nested overrides exceed the deterministic target selector subset.",
            vec!["Reduce nested overrides to one parent-child level before retrying.".to_owned()],
        ));
    }
    for package in &project_ir.packages {
        let Some(mut manifest) = read_json_object(&root.join(&package.manifest_path))? else {
            continue;
        };
        if package.path == "." {
            rewrite_manifest_toolchain_pins(
                &mut manifest,
                source,
                target,
                &package.manifest_path,
                &mut diagnostics,
            );
            remove_source_lifecycle_policy(&mut manifest, remove_yarn_build_policy);
            manifest.remove("packageExtensions");
            manifest.remove("patchedDependencies");
            manifest.insert(
                "packageManager".to_owned(),
                Value::String(get_package_manager(target).package_manager_pin.to_owned()),
            );
            if let Some(pnpm) = manifest.get_mut("pnpm").and_then(Value::as_object_mut) {
                pnpm.remove("packageExtensions");
                pnpm.remove("patchedDependencies");
                if target != PackageManagerId::Pnpm {
                    pnpm.remove("overrides");
                }
                if pnpm.is_empty() {
                    manifest.remove("pnpm");
                }
            }
            match target {
                PackageManagerId::Pnpm | PackageManagerId::Vlt => {
                    manifest.remove("overrides");
                    manifest.remove("resolutions");
                }
                PackageManagerId::YarnClassic | PackageManagerId::YarnModern
                    if !source_overrides.is_empty() =>
                {
                    manifest.remove("overrides");
                    manifest.remove("resolutions");
                    if let Some(policy) = rendered_policy.as_ref() {
                        manifest.insert("resolutions".to_owned(), Value::Object(policy.clone()));
                    }
                }
                PackageManagerId::Npm | PackageManagerId::Deno
                    if source_overrides.is_empty() && !source_resolutions.is_empty() =>
                {
                    manifest.remove("resolutions");
                    if let Some(policy) = rendered_policy.as_ref() {
                        manifest.insert("overrides".to_owned(), Value::Object(policy.clone()));
                    }
                }
                PackageManagerId::Npm | PackageManagerId::Bun | PackageManagerId::Deno
                    if !source_overrides.is_empty() =>
                {
                    manifest.remove("resolutions");
                    if let Some(policy) = rendered_policy.as_ref() {
                        manifest.insert("overrides".to_owned(), Value::Object(policy.clone()));
                    }
                }
                _ => {}
            }
            if source == PackageManagerId::YarnModern
                && target != PackageManagerId::YarnModern
                && yarn_patch_resolution_present
            {
                manifest.remove("resolutions");
                if target == PackageManagerId::Bun && !source_resolutions.is_empty() {
                    manifest.insert(
                        "resolutions".to_owned(),
                        Value::Object(source_resolutions.clone()),
                    );
                }
            }
            if target == PackageManagerId::Npm && !package_extensions.is_empty() {
                manifest.insert(
                    "packageExtensions".to_owned(),
                    Value::Object(package_extensions.clone()),
                );
            }
            if target == PackageManagerId::Bun && !patched_dependencies.is_empty() {
                manifest.insert(
                    "patchedDependencies".to_owned(),
                    Value::Object(patched_dependencies.clone()),
                );
            }
            if target == PackageManagerId::YarnModern && !patch_resolutions.is_empty() {
                let resolutions = manifest
                    .entry("resolutions")
                    .or_insert_with(|| Value::Object(Map::new()))
                    .as_object_mut()
                    .expect("resolutions is initialized as an object");
                for (selector, resolution) in &patch_resolutions {
                    if let Some(existing) = resolutions.get(selector)
                        && existing != resolution
                    {
                        diagnostics.push(Diagnostic::blocking(
                            "PATCH_RESOLUTION_CONFLICT",
                            "A Yarn resolution conflicts with a migrated patched dependency.",
                            vec![
                                "Remove the conflicting resolution before retrying the migration."
                                    .to_owned(),
                            ],
                        ));
                        continue;
                    }
                    resolutions.insert(selector.clone(), resolution.clone());
                }
            }
            if !project_ir.workspace_patterns.is_empty()
                && !matches!(
                    target,
                    PackageManagerId::Pnpm | PackageManagerId::Vlt | PackageManagerId::Deno
                )
            {
                manifest.insert(
                    "workspaces".to_owned(),
                    Value::Array(
                        project_ir
                            .workspace_patterns
                            .iter()
                            .cloned()
                            .map(Value::String)
                            .collect(),
                    ),
                );
            }
            if matches!(target, PackageManagerId::Vlt | PackageManagerId::Deno) {
                manifest.remove("workspaces");
            }
            if target == PackageManagerId::Bun {
                if !pnpm_catalog.is_empty() {
                    manifest.insert("catalog".to_owned(), Value::Object(pnpm_catalog.clone()));
                }
                if !pnpm_catalogs.is_empty() {
                    manifest.insert("catalogs".to_owned(), Value::Object(pnpm_catalogs.clone()));
                }
                if !trusted_dependencies.is_empty() {
                    manifest.insert(
                        "trustedDependencies".to_owned(),
                        Value::Array(
                            trusted_dependencies
                                .iter()
                                .cloned()
                                .map(Value::String)
                                .collect(),
                        ),
                    );
                }
            }
            if target == PackageManagerId::YarnModern {
                configure_yarn_lifecycle_policy(&mut manifest, &trusted_dependencies);
            }
        }
        if let Some(scripts) = manifest.get_mut("scripts").and_then(Value::as_object_mut) {
            for value in scripts.values_mut() {
                if let Some(command) = value.as_str() {
                    *value =
                        Value::String(rewrite_package_manager_commands(command, source, target));
                }
            }
        }
        for section in [
            "dependencies",
            "devDependencies",
            "optionalDependencies",
            "peerDependencies",
        ] {
            let Some(entries) = manifest.get_mut(section).and_then(Value::as_object_mut) else {
                continue;
            };
            for (name, value) in entries {
                let Some(specifier) = value.as_str() else {
                    continue;
                };
                let feature = if specifier.starts_with("workspace:") {
                    "dependency.workspace-protocol"
                } else if specifier.starts_with("catalog:") {
                    "dependency.catalog-protocol"
                } else if specifier.starts_with("portal:") {
                    "dependency.portal-protocol"
                } else if specifier.starts_with("link:") {
                    "dependency.link-protocol"
                } else if specifier.starts_with("patch:") {
                    "dependency.patch-protocol"
                } else {
                    continue;
                };
                let location = format!("{}#/{section}/{name}", package.manifest_path);
                if let Some(conversion) = yarn_patch_conversions.get(&location) {
                    *value = Value::String(conversion.base_specifier.clone());
                    continue;
                }
                let Some(decision) = decisions.get(feature) else {
                    continue;
                };
                if decision.transformation_id.as_deref() == Some("catalog.expand-to-range") {
                    let key = specifier.strip_prefix("catalog:").unwrap_or_default();
                    let catalog_key = if key.is_empty() { name.as_str() } else { key };
                    if let Some(range) = pnpm_catalog.get(catalog_key).and_then(Value::as_str) {
                        *value = Value::String(range.to_owned());
                    } else {
                        diagnostics.push(Diagnostic::blocking(
                            "CATALOG_ENTRY_NOT_FOUND",
                            format!("No catalog entry was found for {name}."),
                            vec![
                                "Define the catalog entry before retrying the migration."
                                    .to_owned(),
                            ],
                        ));
                    }
                } else if let Some(transformed) =
                    transform_specifier(name, specifier, decision, &versions)
                {
                    *value = Value::String(transformed);
                }
            }
        }
        let content = json_content(&manifest)?;
        if let Some(change) = mutation(
            root,
            &package.manifest_path,
            MutationAction::Write,
            Some(content),
            format!("Render {target} manifest semantics."),
            analysis
                .decisions
                .iter()
                .map(|decision| decision.feature_id.clone())
                .collect(),
        )? {
            manifest_mutations.push(change);
        }
    }

    let pnp = project_ir
        .features
        .iter()
        .any(|feature| feature.id == "install.pnp-linker");
    let isolated = project_ir
        .features
        .iter()
        .any(|feature| feature.id == "install.isolated-linker");
    let pnpm_node_linker = pnp
        .then_some("pnp")
        .or_else(|| isolated.then_some("isolated"));
    let mut configuration_mutations = Vec::new();
    if target == PackageManagerId::Pnpm
        && (!project_ir.workspace_patterns.is_empty()
            || !pnpm_overrides.is_empty()
            || !pnpm_catalog.is_empty()
            || !pnpm_catalogs.is_empty()
            || !package_extensions.is_empty()
            || !patched_dependencies.is_empty()
            || pnpm_node_linker.is_some()
            || lifecycle_policy_present)
        && let Some(change) = mutation(
            root,
            "pnpm-workspace.yaml",
            MutationAction::Write,
            Some(render_pnpm_workspace(
                &project_ir.workspace_patterns,
                &pnpm_catalog,
                &pnpm_catalogs,
                &pnpm_overrides,
                &package_extensions,
                &patched_dependencies,
                pnpm_node_linker,
                &trusted_dependencies,
                lifecycle_policy_present,
            )),
            "Render pnpm workspace and policy configuration.",
            vec![
                "workspace.manifest".to_owned(),
                "resolution.overrides".to_owned(),
                "resolution.package-extensions".to_owned(),
                "patch.patched-dependencies".to_owned(),
                "install.pnp-linker".to_owned(),
                "install.isolated-linker".to_owned(),
                "lifecycle.trusted-dependencies".to_owned(),
            ],
        )?
    {
        configuration_mutations.push(change);
    }
    if target == PackageManagerId::YarnModern {
        let npmrc = read_text(&root.join(".npmrc"))?;
        let mut registry = npmrc
            .as_deref()
            .map(|content| npmrc_for_yarn(content, &mut diagnostics))
            .unwrap_or_default();
        if source == PackageManagerId::Vlt {
            apply_vlt_registry_to_yarn(&vlt_configuration, &mut registry);
        }
        let yarn_node_linker = if pnp {
            "pnp"
        } else if isolated {
            "pnpm"
        } else {
            "node-modules"
        };
        if let Some(change) = mutation(
            root,
            ".yarnrc.yml",
            MutationAction::Write,
            Some(render_yarn_configuration(
                yarn_node_linker,
                lifecycle_policy_present,
                &registry,
                &package_extensions,
            )),
            "Render Yarn Modern linker, policy, lifecycle, and registry configuration.",
            vec![
                "install.pnp-linker".to_owned(),
                "install.isolated-linker".to_owned(),
                "registry.npmrc".to_owned(),
                "resolution.package-extensions".to_owned(),
                "lifecycle.trusted-dependencies".to_owned(),
            ],
        )? {
            configuration_mutations.push(change);
        }
    }
    if target == PackageManagerId::Bun && (pnp || isolated) {
        let before = read_text(&root.join("bunfig.toml"))?;
        if let Some(after) = render_bun_configuration(before.as_deref(), true) {
            if let Some(change) = mutation(
                root,
                "bunfig.toml",
                MutationAction::Write,
                Some(after),
                "Select Bun isolated linking for a reviewed linker migration.",
                vec![
                    "install.pnp-linker".to_owned(),
                    "install.isolated-linker".to_owned(),
                ],
            )? {
                configuration_mutations.push(change);
            }
        } else {
            diagnostics.push(Diagnostic::blocking(
                "CONFIGURATION_PARSE_FAILED",
                "bunfig.toml contains ambiguous install linker configuration.",
                vec![
                    "Keep one [install] section and one linker setting before retrying.".to_owned(),
                ],
            ));
        }
    }
    if target == PackageManagerId::Vlt {
        let npmrc = read_text(&root.join(".npmrc"))?;
        let mut configuration = npmrc_for_vlt(npmrc.as_deref(), &mut diagnostics);
        if !project_ir.workspace_patterns.is_empty() {
            configuration.insert(
                "workspaces".to_owned(),
                Value::Array(
                    project_ir
                        .workspace_patterns
                        .iter()
                        .cloned()
                        .map(Value::String)
                        .collect(),
                ),
            );
        }
        if !pnpm_catalog.is_empty() {
            configuration.insert("catalog".to_owned(), Value::Object(pnpm_catalog.clone()));
        }
        if !pnpm_catalogs.is_empty() {
            configuration.insert("catalogs".to_owned(), Value::Object(pnpm_catalogs.clone()));
        }
        if !vlt_modifiers.is_empty() {
            configuration.insert("modifiers".to_owned(), Value::Object(vlt_modifiers.clone()));
        }
        if let Some(change) = mutation(
            root,
            "vlt.json",
            MutationAction::Write,
            Some(json_content(&configuration)?),
            "Render vlt workspace, catalog, modifier, and registry configuration.",
            vec![
                "workspace.manifest".to_owned(),
                "policy.catalogs".to_owned(),
                "resolution.overrides".to_owned(),
                "registry.npmrc".to_owned(),
            ],
        )? {
            configuration_mutations.push(change);
        }
    }
    if target == PackageManagerId::Deno {
        if !project_ir.workspace_patterns.is_empty() {
            deno_configuration.insert(
                "workspace".to_owned(),
                Value::Array(
                    project_ir
                        .workspace_patterns
                        .iter()
                        .cloned()
                        .map(Value::String)
                        .collect(),
                ),
            );
        }
        if pnp || isolated {
            deno_configuration.insert(
                "nodeModulesDir".to_owned(),
                Value::String("manual".to_owned()),
            );
            deno_configuration.insert(
                "nodeModulesLinker".to_owned(),
                Value::String("isolated".to_owned()),
            );
        }
        if let Some(change) = mutation(
            root,
            deno_location,
            MutationAction::Write,
            Some(json_content(&deno_configuration)?),
            "Render Deno dependency workspace and linker configuration while preserving runtime settings.",
            vec![
                "workspace.manifest".to_owned(),
                "install.pnp-linker".to_owned(),
                "install.isolated-linker".to_owned(),
            ],
        )? {
            configuration_mutations.push(change);
        }
    }
    if source == PackageManagerId::Vlt
        && matches!(
            target,
            PackageManagerId::Npm
                | PackageManagerId::Pnpm
                | PackageManagerId::YarnClassic
                | PackageManagerId::Bun
                | PackageManagerId::Deno
        )
        && let Some(content) = npmrc_from_vlt(&vlt_configuration)
        && let Some(change) = mutation(
            root,
            ".npmrc",
            MutationAction::Write,
            Some(content),
            "Render npm-compatible public registry configuration from vlt.json.",
            vec!["registry.npmrc".to_owned()],
        )?
    {
        configuration_mutations.push(change);
    }

    let integration_mutations =
        transform_integrations(root, inspection, source, target, &mut diagnostics)?;

    let target_definition = get_package_manager(target);
    let source_definition = get_package_manager(source);
    let target_artifacts = target_definition
        .lockfiles
        .iter()
        .chain(target_definition.configuration_files.iter())
        .copied()
        .collect::<BTreeSet<_>>();
    let mut cleanup_mutations = Vec::new();
    for path in source_definition
        .lockfiles
        .iter()
        .chain(source_definition.configuration_files.iter())
        .copied()
    {
        if target_artifacts.contains(path) {
            continue;
        }
        if source == PackageManagerId::Deno && matches!(path, "deno.json" | "deno.jsonc") {
            continue;
        }
        if let Some(change) = mutation(
            root,
            path,
            MutationAction::Delete,
            None,
            format!("Retire source-only {source} artifact."),
            Vec::new(),
        )? {
            cleanup_mutations.push(change);
        }
    }
    Ok(Transformation {
        manifest_mutations,
        configuration_mutations,
        integration_mutations,
        cleanup_mutations,
        diagnostics,
    })
}
