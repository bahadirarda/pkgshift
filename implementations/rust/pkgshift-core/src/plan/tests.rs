use std::fs;

use serde_json::{Map, Value, json};
use tempfile::tempdir;

use crate::inspect::{build_project_ir, inspect_project};
use crate::model::MutationAction;
use crate::transformation::json_content;

use super::*;

fn plan_manifest_policy(
    source: PackageManagerId,
    target: PackageManagerId,
    policy: &Value,
    accepted_lossy: bool,
) -> MigrationPlan {
    let directory = tempdir().expect("temporary directory");
    let source_definition = get_package_manager(source);
    let mut manifest = Map::from_iter([
        ("name".to_owned(), Value::String("fixture".to_owned())),
        ("private".to_owned(), Value::Bool(true)),
        (
            "packageManager".to_owned(),
            Value::String(source_definition.package_manager_pin.to_owned()),
        ),
    ]);
    let policy = policy.as_object().expect("policy object");
    manifest.extend(policy.clone());
    fs::write(
        directory.path().join("package.json"),
        json_content(&manifest).expect("manifest serialization"),
    )
    .expect("manifest");
    fs::write(
        directory.path().join(source_definition.lockfiles[0]),
        "fixture\n",
    )
    .expect("source lockfile");
    let inspection = inspect_project(directory.path()).expect("inspection");
    let ir = build_project_ir(&inspection)
        .expect("IR build")
        .expect("project IR");
    let analysis = analyze_capabilities(&ir, target)
        .expect("analysis")
        .expect("capability analysis");
    plan_package_manager_migration(&inspection, &ir, &analysis, None, target, accepted_lossy)
        .expect("planning")
        .expect("migration plan")
}

fn mutation_content<'a>(plan: &'a MigrationPlan, path: &str) -> &'a str {
    plan.operations
        .iter()
        .flat_map(|operation| &operation.mutations)
        .find(|mutation| mutation.path == path && mutation.action == MutationAction::Write)
        .and_then(|mutation| mutation.content.as_deref())
        .expect("planned mutation content")
}

fn plan_fixture(
    files: &[(&str, &str)],
    target: PackageManagerId,
    accepted_lossy: bool,
) -> MigrationPlan {
    let directory = tempdir().expect("temporary directory");
    for (path, content) in files {
        let path = directory.path().join(path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("fixture parent");
        }
        fs::write(path, content).expect("fixture file");
    }
    let inspection = inspect_project(directory.path()).expect("inspection");
    let ir = build_project_ir(&inspection)
        .expect("IR build")
        .expect("project IR");
    let analysis = analyze_capabilities(&ir, target)
        .expect("analysis")
        .expect("capability analysis");
    plan_package_manager_migration(&inspection, &ir, &analysis, None, target, accepted_lossy)
        .expect("planning")
        .expect("migration plan")
}

#[test]
fn plans_a_pnpm_to_bun_workspace() {
    let directory = tempdir().expect("temporary directory");
    fs::create_dir_all(directory.path().join("packages/app")).expect("workspace directory");
    fs::write(
            directory.path().join("package.json"),
            r#"{"name":"fixture","private":true,"packageManager":"pnpm@11.21.0","workspaces":["packages/*"]}"#,
        )
        .expect("root manifest");
    fs::write(
        directory.path().join("packages/app/package.json"),
        r#"{"name":"@fixture/app","version":"1.0.0"}"#,
    )
    .expect("package manifest");
    fs::write(
        directory.path().join("pnpm-lock.yaml"),
        "lockfileVersion: '9.0'\n",
    )
    .expect("lockfile");
    fs::write(
        directory.path().join("pnpm-workspace.yaml"),
        "packages:\n  - 'packages/*'\n",
    )
    .expect("workspace configuration");
    let inspection = inspect_project(directory.path()).expect("inspection");
    let ir = build_project_ir(&inspection)
        .expect("IR build")
        .expect("project IR");
    let analysis = analyze_capabilities(&ir, PackageManagerId::Bun)
        .expect("analysis")
        .expect("capability analysis");
    let plan = plan_package_manager_migration(
        &inspection,
        &ir,
        &analysis,
        None,
        PackageManagerId::Bun,
        false,
    )
    .expect("planning")
    .expect("migration plan");
    assert!(plan.executable);
    assert!(plan.operations.iter().any(|operation| {
        operation.kind == "dependency.install-target"
            && operation
                .command
                .first()
                .is_some_and(|value| value == "bun")
    }));
    assert!(plan.operations.iter().any(|operation| {
        operation
            .mutations
            .iter()
            .any(|mutation| mutation.path == "pnpm-lock.yaml")
    }));
}

#[test]
fn plans_all_basic_production_directions() {
    let production = [
        PackageManagerId::Npm,
        PackageManagerId::Pnpm,
        PackageManagerId::YarnClassic,
        PackageManagerId::YarnModern,
        PackageManagerId::Bun,
        PackageManagerId::Vlt,
        PackageManagerId::Deno,
    ];
    for source in production {
        for target in production {
            if source == target {
                continue;
            }
            let directory = tempdir().expect("temporary directory");
            let definition = get_package_manager(source);
            fs::write(
                directory.path().join("package.json"),
                format!(
                    "{{\"name\":\"fixture\",\"private\":true,\"packageManager\":\"{}\"}}",
                    definition.package_manager_pin
                ),
            )
            .expect("manifest");
            fs::write(directory.path().join(definition.lockfiles[0]), "fixture\n")
                .expect("source lockfile");
            let inspection = inspect_project(directory.path()).expect("inspection");
            let ir = build_project_ir(&inspection)
                .expect("IR build")
                .expect("project IR");
            let analysis = analyze_capabilities(&ir, target)
                .expect("analysis")
                .expect("capability analysis");
            let plan =
                plan_package_manager_migration(&inspection, &ir, &analysis, None, target, false)
                    .expect("planning")
                    .expect("migration plan");
            assert!(plan.executable, "{source} to {target} should be executable");
            let cleanup = plan
                .operations
                .iter()
                .position(|operation| operation.kind == crate::cleanup::OPERATION_KIND)
                .expect("clean dependency-state operation");
            let install = plan
                .operations
                .iter()
                .position(|operation| operation.kind.contains("install-target"))
                .expect("target install operation");
            assert!(
                cleanup < install,
                "{source} to {target} must clean before install"
            );
        }
    }
}

#[test]
fn renders_vlt_workspace_modifiers_and_registry_configuration() {
    let plan = plan_fixture(
        &[
            (
                "package.json",
                r#"{"name":"fixture","private":true,"packageManager":"npm@12.0.2","workspaces":["packages/*"],"overrides":{"parent":{"child":"2.0.0"}}}"#,
            ),
            (
                "packages/app/package.json",
                r#"{"name":"app","version":"1.0.0"}"#,
            ),
            ("package-lock.json", "{}"),
            (
                ".npmrc",
                "registry=https://registry.example.test/\n@internal:registry=https://scope.example.test/\n",
            ),
        ],
        PackageManagerId::Vlt,
        false,
    );

    assert!(plan.executable);
    let configuration: Value =
        serde_json::from_str(mutation_content(&plan, "vlt.json")).expect("vlt configuration JSON");
    assert_eq!(configuration["workspaces"], json!(["packages/*"]));
    assert_eq!(
        configuration["modifiers"][":root > #parent > #child"],
        "2.0.0"
    );
    assert_eq!(
        configuration["config"]["scoped-registries"]["@internal"],
        "https://scope.example.test/"
    );
    assert!(plan.operations.iter().any(|operation| {
        operation.kind == "dependency.install-target" && operation.command == ["vlt", "install"]
    }));
}

#[test]
fn renders_vlt_dependency_policy_back_to_pnpm() {
    let plan = plan_fixture(
        &[
            (
                "package.json",
                r#"{"name":"fixture","private":true,"packageManager":"vlt@1.0.2"}"#,
            ),
            (
                "packages/app/package.json",
                r#"{"name":"app","dependencies":{"react":"catalog:","lib":"workspace:*"}}"#,
            ),
            (
                "packages/lib/package.json",
                r#"{"name":"lib","version":"1.2.3"}"#,
            ),
            (
                "vlt.json",
                r##"{"config":{"registry":"https://registry.example.test/"},"workspaces":["packages/*"],"catalog":{"react":"^19.0.0"},"modifiers":{"#lodash":"4.17.21",":root > #parent > #child":"2.0.0"}}"##,
            ),
            ("vlt-lock.json", r#"{"lockfileVersion":1,"nodes":{}}"#),
        ],
        PackageManagerId::Pnpm,
        false,
    );

    assert!(plan.executable);
    let configuration = mutation_content(&plan, "pnpm-workspace.yaml");
    assert!(configuration.contains("'parent>child': '2.0.0'"));
    assert!(configuration.contains("'lodash': '4.17.21'"));
    assert!(
        configuration.contains("  react: '^19.0.0'"),
        "{configuration}"
    );
    assert_eq!(
        mutation_content(&plan, ".npmrc"),
        "registry=https://registry.example.test/\n"
    );
}

#[test]
fn renders_deno_dependency_configuration_and_blocks_unsupported_protocols() {
    let plan = plan_fixture(
        &[
            (
                "package.json",
                r#"{"name":"fixture","private":true,"packageManager":"pnpm@11.21.0","overrides":{"parent":{"child":"2.0.0"}}}"#,
            ),
            (
                "packages/app/package.json",
                r#"{"name":"app","version":"1.0.0"}"#,
            ),
            ("pnpm-lock.yaml", "lockfileVersion: '9.0'\n"),
            (
                "pnpm-workspace.yaml",
                "packages:\n  - 'packages/*'\nnodeLinker: isolated\n",
            ),
        ],
        PackageManagerId::Deno,
        false,
    );
    assert!(plan.executable);
    let configuration: Value = serde_json::from_str(mutation_content(&plan, "deno.json"))
        .expect("Deno configuration JSON");
    assert_eq!(configuration["workspace"], json!(["packages/*"]));
    assert_eq!(configuration["nodeModulesLinker"], "isolated");

    let blocked = plan_fixture(
        &[
            (
                "package.json",
                r#"{"name":"fixture","packageManager":"npm@12.0.2","dependencies":{"repository":"git+https://example.test/repository.git"}}"#,
            ),
            ("package-lock.json", "{}"),
        ],
        PackageManagerId::Deno,
        false,
    );
    assert!(!blocked.executable);
    assert!(
        blocked
            .diagnostics
            .iter()
            .any(|entry| entry.code == "DENO_DEPENDENCY_PROTOCOL_UNSUPPORTED")
    );
}

#[test]
fn blocks_vlt_registry_credentials_without_persisting_them() {
    let plan = plan_fixture(
        &[
            (
                "package.json",
                r#"{"name":"fixture","packageManager":"npm@12.0.2"}"#,
            ),
            ("package-lock.json", "{}"),
            (
                ".npmrc",
                "registry=https://registry.npmjs.org/\n//registry.npmjs.org/:_authToken=${NPM_TOKEN}\n",
            ),
        ],
        PackageManagerId::Vlt,
        false,
    );

    assert!(!plan.executable);
    assert!(
        plan.diagnostics
            .iter()
            .any(|entry| entry.code == "VLT_REGISTRY_AUTH_MANUAL_REQUIRED")
    );
    assert!(
        !serde_json::to_string(&plan)
            .expect("plan JSON")
            .contains("NPM_TOKEN")
    );
}

#[test]
fn renders_nested_npm_overrides_as_pnpm_selectors() {
    let plan = plan_manifest_policy(
        PackageManagerId::Npm,
        PackageManagerId::Pnpm,
        &json!({
            "overrides": {
                "parent": {
                    ".": "2.0.0",
                    "child": "1.2.3"
                }
            }
        }),
        false,
    );

    assert!(plan.executable);
    assert!(
        !plan
            .diagnostics
            .iter()
            .any(|entry| entry.code == "TRANSFORMATION_UNIMPLEMENTED")
    );
    let manifest: Value =
        serde_json::from_str(mutation_content(&plan, "package.json")).expect("manifest JSON");
    assert!(manifest.get("overrides").is_none());
    let configuration = mutation_content(&plan, "pnpm-workspace.yaml");
    assert!(configuration.contains("'parent': '2.0.0'"));
    assert!(configuration.contains("'parent>child': '1.2.3'"));
}

#[test]
fn renders_nested_npm_overrides_as_yarn_resolutions() {
    let plan = plan_manifest_policy(
        PackageManagerId::Npm,
        PackageManagerId::YarnModern,
        &json!({
            "overrides": {
                "parent": {
                    "child": "1.2.3"
                }
            }
        }),
        true,
    );

    assert!(plan.executable);
    assert!(plan.accepted_lossy);
    let manifest: Value =
        serde_json::from_str(mutation_content(&plan, "package.json")).expect("manifest JSON");
    assert_eq!(manifest["resolutions"]["parent/child"], "1.2.3");
    assert!(manifest.get("overrides").is_none());
}

#[test]
fn renders_compatible_yarn_resolutions_as_npm_overrides() {
    let plan = plan_manifest_policy(
        PackageManagerId::YarnClassic,
        PackageManagerId::Npm,
        &json!({
            "resolutions": {
                "@scope/package": "1.2.3",
                "lodash": "4.17.21"
            }
        }),
        false,
    );

    assert!(plan.executable);
    let manifest: Value =
        serde_json::from_str(mutation_content(&plan, "package.json")).expect("manifest JSON");
    assert_eq!(manifest["overrides"]["@scope/package"], "1.2.3");
    assert_eq!(manifest["overrides"]["lodash"], "4.17.21");
    assert!(manifest.get("resolutions").is_none());
}

#[test]
fn renders_npm_package_extensions_in_pnpm_workspace_configuration() {
    let plan = plan_manifest_policy(
        PackageManagerId::Npm,
        PackageManagerId::Pnpm,
        &json!({
            "packageExtensions": {
                "bare-package": {
                    "dependencies": { "bare-runtime-dep": "1.0.0" }
                },
                "broken-package@^1": {
                    "dependencies": { "missing-runtime-dep": "^2.0.0" },
                    "peerDependencies": { "react": "*" },
                    "peerDependenciesMeta": { "react": { "optional": true } }
                }
            }
        }),
        false,
    );

    assert!(plan.executable);
    let manifest: Value =
        serde_json::from_str(mutation_content(&plan, "package.json")).expect("manifest JSON");
    assert!(manifest.get("packageExtensions").is_none());
    let configuration: Value = noyalib::from_str(mutation_content(&plan, "pnpm-workspace.yaml"))
        .expect("pnpm configuration YAML");
    assert_eq!(
        configuration["packageExtensions"]["broken-package@^1"]["dependencies"]["missing-runtime-dep"],
        "^2.0.0"
    );
    assert_eq!(
        configuration["packageExtensions"]["bare-package"]["dependencies"]["bare-runtime-dep"],
        "1.0.0"
    );
    assert_eq!(
        configuration["packageExtensions"]["broken-package@^1"]["peerDependenciesMeta"]["react"]["optional"],
        true
    );
}

#[test]
fn renders_pnpm_package_extensions_in_yarn_configuration() {
    let plan = plan_fixture(
        &[
            (
                "package.json",
                r#"{"name":"fixture","private":true,"packageManager":"pnpm@11.21.0"}"#,
            ),
            ("pnpm-lock.yaml", "lockfileVersion: '9.0'\n"),
            (
                "pnpm-workspace.yaml",
                "packageExtensions:\n  'broken-package@1':\n    optionalDependencies:\n      optional-runtime: '^3.0.0'\n",
            ),
        ],
        PackageManagerId::YarnModern,
        false,
    );

    assert!(plan.executable);
    let configuration: Value =
        noyalib::from_str(mutation_content(&plan, ".yarnrc.yml")).expect("Yarn configuration YAML");
    assert_eq!(
        configuration["packageExtensions"]["broken-package@1"]["optionalDependencies"]["optional-runtime"],
        "^3.0.0"
    );
}

#[test]
fn reads_yarn_package_extensions_and_renders_them_for_npm() {
    let plan = plan_fixture(
        &[
            (
                "package.json",
                r#"{"name":"fixture","private":true,"packageManager":"yarn@4.18.0"}"#,
            ),
            ("yarn.lock", "# fixture\n"),
            (
                ".yarnrc.yml",
                "nodeLinker: node-modules\npackageExtensions:\n  '@scope/broken@^2':\n    peerDependencies:\n      react: '>=18'\n",
            ),
        ],
        PackageManagerId::Npm,
        false,
    );

    assert!(plan.executable);
    let manifest: Value =
        serde_json::from_str(mutation_content(&plan, "package.json")).expect("manifest JSON");
    assert_eq!(
        manifest["packageExtensions"]["@scope/broken@^2"]["peerDependencies"]["react"],
        ">=18"
    );
}

#[test]
fn blocks_package_extensions_outside_the_shared_schema() {
    let plan = plan_manifest_policy(
        PackageManagerId::Npm,
        PackageManagerId::Pnpm,
        &json!({
            "packageExtensions": {
                "broken-package@1": {
                    "scripts": { "postinstall": "node build.js" }
                }
            }
        }),
        false,
    );

    assert!(!plan.executable);
    assert!(
        plan.diagnostics
            .iter()
            .any(|entry| { entry.code == "PACKAGE_EXTENSIONS_UNSUPPORTED" && entry.blocking })
    );
    assert!(
        !serde_json::to_string(&plan)
            .expect("serialized plan")
            .contains("postinstall")
    );
}

#[test]
fn converts_a_yarn_patch_protocol_dependency_to_bun_policy() {
    let plan = plan_fixture(
        &[
            (
                "package.json",
                r#"{"name":"fixture","private":true,"packageManager":"yarn@4.18.0","dependencies":{"left-pad":"patch:left-pad@npm%3A1.3.0#~/.yarn/patches/left-pad.patch"}}"#,
            ),
            ("yarn.lock", "# fixture\n"),
            (".yarnrc.yml", "nodeLinker: node-modules\n"),
            (
                ".yarn/patches/left-pad.patch",
                "diff --git a/index.js b/index.js\n--- a/index.js\n+++ b/index.js\n@@ -1 +1 @@\n-old\n+new\n",
            ),
        ],
        PackageManagerId::Bun,
        false,
    );

    assert!(plan.executable);
    let manifest: Value =
        serde_json::from_str(mutation_content(&plan, "package.json")).expect("manifest JSON");
    assert_eq!(manifest["dependencies"]["left-pad"], "1.3.0");
    assert_eq!(
        manifest["patchedDependencies"]["left-pad@1.3.0"],
        ".yarn/patches/left-pad.patch"
    );
    assert!(
        !plan
            .diagnostics
            .iter()
            .any(|entry| entry.code == "TRANSFORMATION_UNIMPLEMENTED")
    );
}

#[test]
fn converts_a_yarn_patch_protocol_dependency_to_pnpm_policy() {
    let plan = plan_fixture(
        &[
            (
                "package.json",
                r#"{"name":"fixture","private":true,"packageManager":"yarn@4.18.0","devDependencies":{"@scope/tool":"patch:@scope/tool@npm%3A2.1.0#~/.yarn/patches/tool.patch"}}"#,
            ),
            ("yarn.lock", "# fixture\n"),
            (".yarnrc.yml", "nodeLinker: node-modules\n"),
            (
                ".yarn/patches/tool.patch",
                "diff --git a/index.js b/index.js\n--- a/index.js\n+++ b/index.js\n@@ -1 +1 @@\n-old\n+new\n",
            ),
        ],
        PackageManagerId::Pnpm,
        false,
    );

    assert!(plan.executable);
    let manifest: Value =
        serde_json::from_str(mutation_content(&plan, "package.json")).expect("manifest JSON");
    assert_eq!(manifest["devDependencies"]["@scope/tool"], "2.1.0");
    let configuration: Value = noyalib::from_str(mutation_content(&plan, "pnpm-workspace.yaml"))
        .expect("pnpm configuration YAML");
    assert_eq!(
        configuration["patchedDependencies"]["@scope/tool@2.1.0"],
        ".yarn/patches/tool.patch"
    );
}

#[test]
fn converts_a_transitive_yarn_patch_resolution_to_bun_policy() {
    let plan = plan_fixture(
        &[
            (
                "package.json",
                r#"{"name":"fixture","private":true,"packageManager":"yarn@4.18.0","dependencies":{"parent":"1.0.0"},"resolutions":{"left-pad@npm:1.3.0":"patch:left-pad@npm%3A1.3.0#~/.yarn/patches/left-pad.patch"}}"#,
            ),
            ("yarn.lock", "# fixture\n"),
            (".yarnrc.yml", "nodeLinker: node-modules\n"),
            (
                ".yarn/patches/left-pad.patch",
                "diff --git a/index.js b/index.js\n--- a/index.js\n+++ b/index.js\n@@ -1 +1 @@\n-old\n+new\n",
            ),
        ],
        PackageManagerId::Bun,
        false,
    );

    assert!(plan.executable);
    let manifest: Value =
        serde_json::from_str(mutation_content(&plan, "package.json")).expect("manifest JSON");
    assert!(manifest.get("resolutions").is_none());
    assert_eq!(
        manifest["patchedDependencies"]["left-pad@1.3.0"],
        ".yarn/patches/left-pad.patch"
    );
}

#[test]
fn converts_pnpm_patched_dependencies_to_yarn_resolutions() {
    let plan = plan_fixture(
        &[
            (
                "package.json",
                r#"{"name":"fixture","private":true,"packageManager":"pnpm@11.21.0","dependencies":{"left-pad":"^1.3.0"}}"#,
            ),
            ("pnpm-lock.yaml", "lockfileVersion: '9.0'\n"),
            (
                "pnpm-workspace.yaml",
                "patchedDependencies:\n  'left-pad@1.3.0': 'patches/left-pad.patch'\n",
            ),
            (
                "patches/left-pad.patch",
                "diff --git a/index.js b/index.js\n--- a/index.js\n+++ b/index.js\n@@ -1 +1 @@\n-old\n+new\n",
            ),
        ],
        PackageManagerId::YarnModern,
        false,
    );

    assert!(plan.executable);
    let manifest: Value =
        serde_json::from_str(mutation_content(&plan, "package.json")).expect("manifest JSON");
    assert_eq!(manifest["dependencies"]["left-pad"], "^1.3.0");
    assert_eq!(
        manifest["resolutions"]["left-pad@npm:1.3.0"],
        "patch:left-pad@npm%3A1.3.0#~/patches/left-pad.patch"
    );
}

#[test]
fn carries_bun_patched_dependencies_into_pnpm_configuration() {
    let plan = plan_fixture(
        &[
            (
                "package.json",
                r#"{"name":"fixture","private":true,"packageManager":"bun@1.3.14","patchedDependencies":{"left-pad@1.3.0":"patches/left-pad.patch"}}"#,
            ),
            ("bun.lock", "{\"lockfileVersion\":1,\"packages\":{}}\n"),
            (
                "patches/left-pad.patch",
                "diff --git a/index.js b/index.js\n--- a/index.js\n+++ b/index.js\n@@ -1 +1 @@\n-old\n+new\n",
            ),
        ],
        PackageManagerId::Pnpm,
        false,
    );

    assert!(plan.executable);
    let manifest: Value =
        serde_json::from_str(mutation_content(&plan, "package.json")).expect("manifest JSON");
    assert!(manifest.get("patchedDependencies").is_none());
    let configuration = mutation_content(&plan, "pnpm-workspace.yaml");
    assert!(configuration.contains("patchedDependencies:"));
    assert!(configuration.contains("'left-pad@1.3.0': 'patches/left-pad.patch'"));
}

#[test]
fn blocks_patch_ranges_and_missing_patch_files() {
    let range = plan_fixture(
        &[
            (
                "package.json",
                r#"{"name":"fixture","private":true,"packageManager":"yarn@4.18.0","dependencies":{"left-pad":"patch:left-pad@npm%3A%5E1.3.0#~/.yarn/patches/left-pad.patch"}}"#,
            ),
            ("yarn.lock", "# fixture\n"),
            (".yarnrc.yml", "nodeLinker: node-modules\n"),
        ],
        PackageManagerId::Bun,
        false,
    );
    assert!(!range.executable);
    assert!(
        range
            .diagnostics
            .iter()
            .any(|entry| { entry.code == "PATCH_SELECTOR_UNSUPPORTED" && entry.blocking })
    );

    let missing = plan_fixture(
        &[
            (
                "package.json",
                r#"{"name":"fixture","private":true,"packageManager":"bun@1.3.14","patchedDependencies":{"left-pad@1.3.0":"patches/missing.patch"}}"#,
            ),
            ("bun.lock", "{\"lockfileVersion\":1,\"packages\":{}}\n"),
        ],
        PackageManagerId::Pnpm,
        false,
    );
    assert!(!missing.executable);
    assert!(
        missing
            .diagnostics
            .iter()
            .any(|entry| { entry.code == "PATCH_FILE_NOT_FOUND" && entry.blocking })
    );
}

#[test]
fn blocks_yarn_resolution_selectors_that_cannot_preserve_fidelity() {
    let plan = plan_manifest_policy(
        PackageManagerId::YarnClassic,
        PackageManagerId::Npm,
        &json!({ "resolutions": { "parent/child": "1.2.3" } }),
        false,
    );

    assert!(!plan.executable);
    assert!(
        plan.diagnostics
            .iter()
            .any(|entry| entry.code == "RESOLUTION_SELECTOR_UNSUPPORTED" && entry.blocking)
    );
}

#[test]
fn carries_pnpm_workspace_overrides_into_npm() {
    let directory = tempdir().expect("temporary directory");
    fs::write(
        directory.path().join("package.json"),
        r#"{"name":"fixture","private":true,"packageManager":"pnpm@11.21.0"}"#,
    )
    .expect("manifest");
    fs::write(
        directory.path().join("pnpm-workspace.yaml"),
        "overrides:\n  parent:\n    child: 1.2.3\n",
    )
    .expect("pnpm configuration");
    fs::write(
        directory.path().join("pnpm-lock.yaml"),
        "lockfileVersion: '9.0'\n",
    )
    .expect("source lockfile");

    let inspection = inspect_project(directory.path()).expect("inspection");
    let ir = build_project_ir(&inspection)
        .expect("IR build")
        .expect("project IR");
    assert!(
        ir.features
            .iter()
            .any(|feature| feature.id == "resolution.overrides")
    );
    assert!(
        ir.features
            .iter()
            .any(|feature| feature.id == "resolution.nested-overrides")
    );
    let analysis = analyze_capabilities(&ir, PackageManagerId::Npm)
        .expect("analysis")
        .expect("capability analysis");
    let plan = plan_package_manager_migration(
        &inspection,
        &ir,
        &analysis,
        None,
        PackageManagerId::Npm,
        false,
    )
    .expect("planning")
    .expect("migration plan");

    assert!(plan.executable);
    let manifest: Value =
        serde_json::from_str(mutation_content(&plan, "package.json")).expect("manifest JSON");
    assert_eq!(manifest["overrides"]["parent"]["child"], "1.2.3");
    assert!(
        plan.operations
            .iter()
            .flat_map(|entry| &entry.mutations)
            .any(|mutation| mutation.path == "pnpm-workspace.yaml"
                && mutation.action == MutationAction::Delete)
    );
}

#[test]
fn blocks_override_nesting_beyond_the_deterministic_subset() {
    let plan = plan_manifest_policy(
        PackageManagerId::Npm,
        PackageManagerId::Pnpm,
        &json!({
            "overrides": {
                "grandparent": {
                    "parent": {
                        "child": "1.2.3"
                    }
                }
            }
        }),
        false,
    );

    assert!(!plan.executable);
    assert!(
        plan.diagnostics
            .iter()
            .any(|entry| entry.code == "NESTED_OVERRIDE_UNSUPPORTED" && entry.blocking)
    );
}

#[test]
fn renders_environment_backed_registry_configuration_for_yarn_modern() {
    let plan = plan_fixture(
        &[
            (
                "package.json",
                r#"{"name":"fixture","private":true,"packageManager":"npm@12.0.2"}"#,
            ),
            ("package-lock.json", "{}\n"),
            (
                ".npmrc",
                "registry=https://registry.npmjs.org\n@company:registry=https://npm.company.test\n//npm.company.test/:_authToken=${COMPANY_NPM_TOKEN}\nalways-auth=true\n",
            ),
        ],
        PackageManagerId::YarnModern,
        false,
    );

    assert!(plan.executable);
    let configuration = mutation_content(&plan, ".yarnrc.yml");
    assert!(configuration.contains("nodeLinker: node-modules"));
    assert!(configuration.contains("npmRegistryServer: 'https://registry.npmjs.org'"));
    assert!(configuration.contains("'company':"));
    assert!(configuration.contains("'//npm.company.test/':"));
    assert!(configuration.contains("'${COMPANY_NPM_TOKEN}'"));
    assert!(
        plan.operations
            .iter()
            .flat_map(|entry| &entry.mutations)
            .any(|mutation| mutation.path == ".npmrc" && mutation.action == MutationAction::Delete)
    );
}

#[test]
fn keeps_literal_registry_tokens_out_of_persisted_plans() {
    let secret = "literal-token-must-not-persist";
    let npmrc = format!("//registry.npmjs.org/:_authToken={secret}\n");
    let plan = plan_fixture(
        &[
            (
                "package.json",
                r#"{"name":"fixture","private":true,"packageManager":"npm@12.0.2"}"#,
            ),
            ("package-lock.json", "{}\n"),
            (".npmrc", &npmrc),
        ],
        PackageManagerId::YarnModern,
        false,
    );

    assert!(!plan.executable);
    assert!(plan.diagnostics.iter().any(|entry| {
        entry.code == "REGISTRY_SECRET_REQUIRES_ENVIRONMENT_REFERENCE" && entry.blocking
    }));
    assert!(
        !serde_json::to_string(&plan)
            .expect("serialized plan")
            .contains(secret)
    );
}

#[test]
fn renders_isolated_linking_and_current_pnpm_build_policy() {
    let plan = plan_fixture(
        &[
            (
                "package.json",
                r#"{"name":"fixture","private":true,"packageManager":"bun@1.3.14","trustedDependencies":["esbuild","sharp"]}"#,
            ),
            ("bun.lock", "{\"lockfileVersion\":1,\"packages\":{}}\n"),
            ("bunfig.toml", "[install]\nlinker = \"isolated\"\n"),
        ],
        PackageManagerId::Pnpm,
        false,
    );

    assert!(plan.executable);
    let configuration = mutation_content(&plan, "pnpm-workspace.yaml");
    assert!(configuration.contains("nodeLinker: isolated"));
    assert!(configuration.contains("allowBuilds:"));
    assert!(configuration.contains("'esbuild': true"));
    assert!(configuration.contains("'sharp': true"));
    assert!(!configuration.contains("onlyBuiltDependencies"));
    let manifest: Value =
        serde_json::from_str(mutation_content(&plan, "package.json")).expect("manifest JSON");
    assert!(manifest.get("trustedDependencies").is_none());
}

#[test]
fn renders_a_yarn_lifecycle_allow_list_with_scripts_disabled() {
    let plan = plan_fixture(
        &[
            (
                "package.json",
                r#"{"name":"fixture","private":true,"packageManager":"pnpm@11.21.0"}"#,
            ),
            ("pnpm-lock.yaml", "lockfileVersion: '9.0'\n"),
            (
                "pnpm-workspace.yaml",
                "nodeLinker: isolated\nallowBuilds:\n  esbuild: true\n  blocked-package: false\n",
            ),
        ],
        PackageManagerId::YarnModern,
        false,
    );

    assert!(plan.executable);
    let configuration = mutation_content(&plan, ".yarnrc.yml");
    assert!(configuration.contains("nodeLinker: pnpm"));
    assert!(configuration.contains("enableScripts: false"));
    let manifest: Value =
        serde_json::from_str(mutation_content(&plan, "package.json")).expect("manifest JSON");
    assert_eq!(manifest["dependenciesMeta"]["esbuild"]["built"], true);
    assert!(
        manifest["dependenciesMeta"]
            .get("blocked-package")
            .is_none()
    );
}

#[test]
fn reads_a_yarn_lifecycle_allow_list_when_migrating_to_bun() {
    let plan = plan_fixture(
        &[
            (
                "package.json",
                r#"{"name":"fixture","private":true,"packageManager":"yarn@4.18.0","dependenciesMeta":{"esbuild":{"built":true},"sharp":{"built":false}}}"#,
            ),
            ("yarn.lock", "# fixture\n"),
            (
                ".yarnrc.yml",
                "nodeLinker: node-modules\nenableScripts: false\n",
            ),
        ],
        PackageManagerId::Bun,
        false,
    );

    assert!(plan.executable);
    let manifest: Value =
        serde_json::from_str(mutation_content(&plan, "package.json")).expect("manifest JSON");
    assert_eq!(manifest["trustedDependencies"], json!(["esbuild"]));
    assert!(manifest.get("dependenciesMeta").is_none());
}

#[test]
fn blocks_yarn_build_denials_outside_allow_list_mode() {
    let plan = plan_fixture(
        &[
            (
                "package.json",
                r#"{"name":"fixture","private":true,"packageManager":"yarn@4.18.0","dependenciesMeta":{"native-addon":{"built":false}}}"#,
            ),
            ("yarn.lock", "# fixture\n"),
            (".yarnrc.yml", "nodeLinker: node-modules\n"),
        ],
        PackageManagerId::Bun,
        false,
    );

    assert!(!plan.executable);
    assert!(
        plan.diagnostics
            .iter()
            .any(|entry| { entry.code == "YARN_BUILD_POLICY_UNSUPPORTED" && entry.blocking })
    );
}

#[test]
fn blocks_unknown_legacy_node_linker_values() {
    let plan = plan_fixture(
        &[
            (
                "package.json",
                r#"{"name":"fixture","private":true,"packageManager":"pnpm@11.21.0"}"#,
            ),
            ("pnpm-lock.yaml", "lockfileVersion: '9.0'\n"),
            (".npmrc", "node-linker=mystery\n"),
        ],
        PackageManagerId::YarnModern,
        false,
    );

    assert!(!plan.executable);
    assert!(
        plan.diagnostics
            .iter()
            .any(|entry| entry.code == "NPMRC_SETTING_UNSUPPORTED" && entry.blocking)
    );
}

#[test]
fn preserves_an_empty_yarn_lifecycle_allow_list_in_pnpm() {
    let plan = plan_fixture(
        &[
            (
                "package.json",
                r#"{"name":"fixture","private":true,"packageManager":"yarn@4.18.0"}"#,
            ),
            ("yarn.lock", "# fixture\n"),
            (
                ".yarnrc.yml",
                "nodeLinker: node-modules\nenableScripts: false\n",
            ),
        ],
        PackageManagerId::Pnpm,
        false,
    );

    assert!(plan.executable);
    assert!(mutation_content(&plan, "pnpm-workspace.yaml").contains("allowBuilds: {}"));
}
