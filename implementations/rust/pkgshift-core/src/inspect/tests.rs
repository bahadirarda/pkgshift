use super::*;
use std::fs;
use tempfile::tempdir;
#[test]
fn builds_workspace_ir_and_redacts_registry_secrets() {
    let directory = tempdir().expect("temporary directory");
    fs::create_dir_all(directory.path().join("packages/app")).expect("workspace directory");
    fs::write(
        directory.path().join("package.json"),
        r#"{"name":"fixture","private":true,"packageManager":"pnpm@11.21.0","workspaces":["packages/*"]}"#,
    )
    .expect("root manifest");
    fs::write(
        directory.path().join("packages/app/package.json"),
        r#"{"name":"@fixture/app","dependencies":{"lib":"workspace:*"}}"#,
    )
    .expect("package manifest");
    fs::write(
        directory.path().join("pnpm-lock.yaml"),
        "lockfileVersion: '9.0'\n",
    )
    .expect("lockfile");
    fs::write(
        directory.path().join(".npmrc"),
        "//registry.npmjs.org/:_authToken=secret-value\n",
    )
    .expect("npm configuration");
    let inspection = inspect_project(directory.path()).expect("inspection");
    let ir = build_project_ir(&inspection)
        .expect("IR build")
        .expect("project IR");
    assert_eq!(ir.packages.len(), 2);
    assert!(
        ir.features
            .iter()
            .any(|feature| { feature.id == "dependency.workspace-protocol" })
    );
    assert!(
        !serde_json::to_string(&inspection)
            .expect("serialized inspection")
            .contains("secret-value")
    );
}
#[test]
fn accepts_utf8_bom_in_workspace_manifests() {
    let directory = tempdir().expect("temporary directory");
    fs::create_dir_all(directory.path().join("packages/app")).expect("workspace directory");
    fs::write(
        directory.path().join("package.json"),
        "\u{feff}{\"name\":\"fixture\",\"private\":true,\"packageManager\":\"npm@12.0.2\",\"workspaces\":[\"packages/*\"]}",
    )
    .expect("root manifest");
    fs::write(
        directory.path().join("packages/app/package.json"),
        "\u{feff}{\"name\":\"@fixture/app\",\"version\":\"1.0.0\"}",
    )
    .expect("package manifest");
    fs::write(directory.path().join("package-lock.json"), "{}").expect("lockfile");
    let inspection = inspect_project(directory.path()).expect("inspection");
    let ir = build_project_ir(&inspection)
        .expect("IR build")
        .expect("project IR");
    assert_eq!(ir.packages.len(), 2);
    assert_eq!(ir.packages[1].name.as_deref(), Some("@fixture/app"));
}
#[test]
fn secret_rotation_does_not_change_fingerprint() {
    let directory = tempdir().expect("temporary directory");
    fs::write(
        directory.path().join("package.json"),
        r#"{"name":"fixture","packageManager":"npm@12.0.2"}"#,
    )
    .expect("manifest");
    fs::write(directory.path().join("package-lock.json"), "{}").expect("lockfile");
    fs::write(directory.path().join(".npmrc"), "token=first\n").expect("npm configuration");
    let first = inspect_project(directory.path()).expect("first inspection");
    fs::write(directory.path().join(".npmrc"), "token=second\n").expect("npm configuration");
    let second = inspect_project(directory.path()).expect("second inspection");
    assert_eq!(first.fingerprint, second.fingerprint);
}
#[test]
fn project_patch_files_participate_in_repository_fingerprints() {
    let directory = tempdir().expect("temporary directory");
    fs::create_dir_all(directory.path().join("patches")).expect("patch directory");
    fs::write(
        directory.path().join("package.json"),
        r#"{"name":"fixture","packageManager":"bun@1.3.14","patchedDependencies":{"left-pad@1.3.0":"patches/left-pad.patch"}}"#,
    )
    .expect("manifest");
    fs::write(directory.path().join("bun.lock"), "{}\n").expect("lockfile");
    let patch_path = directory.path().join("patches/left-pad.patch");
    fs::write(
        &patch_path,
        "diff --git a/index.js b/index.js\n--- a/index.js\n+++ b/index.js\n",
    )
    .expect("patch");
    let first = inspect_project(directory.path()).expect("first inspection");
    fs::write(
        &patch_path,
        "diff --git a/index.js b/index.js\n--- a/index.js\n+++ b/index.js\n+changed\n",
    )
    .expect("changed patch");
    let second = inspect_project(directory.path()).expect("second inspection");
    assert!(
        first
            .relevant_files
            .contains(&"patches/left-pad.patch".to_owned())
    );
    assert_ne!(first.fingerprint, second.fingerprint);
}
