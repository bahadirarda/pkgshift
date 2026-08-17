use crate::model::{IntegrationInspection, IntegrationKind, PackageManagerId};

use super::*;

fn integration(path: &str, kind: IntegrationKind) -> IntegrationInspection {
    IntegrationInspection {
        kind,
        path: path.to_owned(),
        package_manager_tokens: vec!["pnpm".to_owned()],
    }
}

#[test]
fn rewrites_registered_github_actions_surfaces() {
    let source = "steps:\n  - uses: pnpm/action-setup@v4\n  - run: pnpm install --frozen-lockfile\n  - run: pnpm test\n";
    let rewritten = rewrite_integration(
        &integration(".github/workflows/ci.yml", IntegrationKind::Ci),
        source,
        PackageManagerId::Pnpm,
        PackageManagerId::Bun,
    );
    assert_eq!(
        rewritten.content,
        "steps:\n  - uses: oven-sh/setup-bun@v2\n  - run: bun install --frozen-lockfile\n  - run: bun run test\n"
    );
    assert!(rewritten.diagnostics.is_empty());
}

#[test]
fn blocks_an_unrepresentable_setup_node_cache() {
    let source = "steps:\n  - uses: actions/setup-node@v6\n    with:\n      cache: pnpm\n  - run: pnpm install\n";
    let rewritten = rewrite_integration(
        &integration(".github/workflows/ci.yml", IntegrationKind::Ci),
        source,
        PackageManagerId::Pnpm,
        PackageManagerId::Deno,
    );
    assert_eq!(rewritten.diagnostics.len(), 1);
    assert_eq!(
        rewritten.diagnostics[0].code,
        "INTEGRATION_CACHE_UNSUPPORTED"
    );
    assert!(rewritten.diagnostics[0].blocking);
}

#[test]
fn rewrites_only_markdown_command_spans() {
    let source = "pnpm is mentioned in prose. Run `pnpm test`.\n\n```sh\npnpm install\n```\n";
    let rewritten = rewrite_integration(
        &integration("README.md", IntegrationKind::Documentation),
        source,
        PackageManagerId::Pnpm,
        PackageManagerId::Deno,
    );
    assert_eq!(
        rewritten.content,
        "pnpm is mentioned in prose. Run `deno task test`.\n\n```sh\ndeno install\n```\n"
    );
}

#[test]
fn preserves_bun_runtime_commands_in_dockerfiles() {
    let source = "FROM oven/bun:1\nRUN bun test && bun run build\n";
    let rewritten = rewrite_integration(
        &integration("Dockerfile", IntegrationKind::Container),
        source,
        PackageManagerId::Bun,
        PackageManagerId::Deno,
    );
    assert_eq!(
        rewritten.content,
        "FROM oven/bun:1\nRUN bun test && deno task build\n"
    );
    assert!(rewritten.diagnostics.is_empty());
}

#[test]
fn rewrites_manifest_toolchain_pins_without_removing_node() {
    let mut manifest = serde_json::json!({
        "volta": { "node": "22.22.0", "pnpm": "11.21.0" },
        "engines": { "node": ">=22", "pnpm": ">=11" },
        "devEngines": {
            "packageManager": { "name": "pnpm", "version": "11.21.0", "onFail": "error" }
        }
    })
    .as_object()
    .cloned()
    .expect("manifest object");
    let mut diagnostics = Vec::new();
    rewrite_manifest_toolchain_pins(
        &mut manifest,
        PackageManagerId::Pnpm,
        PackageManagerId::YarnModern,
        "package.json",
        &mut diagnostics,
    );
    assert_eq!(manifest["volta"]["node"], "22.22.0");
    assert_eq!(manifest["volta"]["yarn"], "4.18.0");
    assert_eq!(manifest["engines"]["yarn"], ">=4.18.0");
    assert_eq!(manifest["devEngines"]["packageManager"]["name"], "yarn");
    assert!(diagnostics.is_empty());
}

#[test]
fn rewrites_registered_toolchain_files() {
    let versions = rewrite_integration(
        &integration(".tool-versions", IntegrationKind::Automation),
        "nodejs 22.22.0\npnpm 11.21.0\n",
        PackageManagerId::Pnpm,
        PackageManagerId::Deno,
    );
    assert_eq!(versions.content, "nodejs 22.22.0\ndeno 2.9.5\n");

    let mise = rewrite_integration(
        &integration("mise.toml", IntegrationKind::Automation),
        "[tools]\nnode = \"22.22.0\"\npnpm = \"11.21.0\"\n",
        PackageManagerId::Pnpm,
        PackageManagerId::Bun,
    );
    assert_eq!(
        mise.content,
        "[tools]\nnode = \"22.22.0\"\nbun = \"1.3.14\"\n"
    );
}

#[test]
fn rewrites_devcontainer_lifecycle_commands() {
    let source =
        "{\n  \"name\": \"fixture\",\n  \"postCreateCommand\": \"pnpm install && pnpm test\"\n}\n";
    let rewritten = rewrite_integration(
        &integration(
            ".devcontainer/devcontainer.json",
            IntegrationKind::Automation,
        ),
        source,
        PackageManagerId::Pnpm,
        PackageManagerId::Bun,
    );
    assert_eq!(
        rewritten.content,
        "{\n  \"name\": \"fixture\",\n  \"postCreateCommand\": \"bun install && bun run test\"\n}\n"
    );
    assert!(rewritten.diagnostics.is_empty());
}
