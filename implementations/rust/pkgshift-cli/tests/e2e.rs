#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;
use tempfile::TempDir;

fn write(path: &Path, content: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("fixture parent directory");
    }
    fs::write(path, content).expect("fixture file");
}

fn fake_package_managers(directory: &TempDir) -> PathBuf {
    let binary_directory = directory.path().join("bin");
    fs::create_dir_all(&binary_directory).expect("fake binary directory");
    for (name, lockfile_command) in [
        (
            "bun",
            "printf '%s\\n' '{\"lockfileVersion\":1,\"packages\":{}}' > bun.lock",
        ),
        (
            "pnpm",
            "printf '%s\\n' \"lockfileVersion: '9.0'\" 'packages: {}' 'snapshots: {}' > pnpm-lock.yaml",
        ),
        (
            "yarn",
            "printf '%s\\n' '__metadata:' '  version: 8' > yarn.lock",
        ),
    ] {
        let path = binary_directory.join(name);
        write(
            &path,
            &format!(
                "#!/bin/sh\nprintf 'fixture-secret-value\\n'\nprintf 'fixture-secret-value\\n' >&2\n{lockfile_command}\n"
            ),
        );
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755))
            .expect("fake binary permissions");
    }
    binary_directory
}

fn run(root: &Path, binaries: &Path, arguments: &[&str]) -> Output {
    let inherited_path = std::env::var_os("PATH").unwrap_or_default();
    let mut path = binaries.as_os_str().to_os_string();
    path.push(":");
    path.push(inherited_path);
    Command::new(env!("CARGO_BIN_EXE_pkgshift"))
        .args(arguments)
        .arg("--cwd")
        .arg(root)
        .arg("--json")
        .arg("--non-interactive")
        .env("PATH", path)
        .output()
        .expect("pkgshift process")
}

fn json_output(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "expected JSON output: {error}\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

fn artifact_content<'a>(result: &'a Value, artifact_type: &str) -> &'a Value {
    result["artifacts"]
        .as_array()
        .expect("result artifacts")
        .iter()
        .find(|artifact| artifact["type"] == artifact_type)
        .unwrap_or_else(|| panic!("missing {artifact_type} artifact"))
        .get("content")
        .expect("artifact content")
}

fn plan_and_apply(root: &Path, binaries: &Path, target: &str) -> Value {
    let planned = run(root, binaries, &["to", target]);
    assert_eq!(planned.status.code(), Some(7));
    let planned = json_output(&planned);
    assert_eq!(planned["status"], "planned");
    assert_eq!(planned["summary"]["repositoryChanged"], false);
    let plan_id = planned["planId"]
        .as_str()
        .expect("planned result identifier");

    let applied = run(root, binaries, &["to", target, "--approve", plan_id]);
    assert!(
        applied.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&applied.stdout),
        String::from_utf8_lossy(&applied.stderr)
    );
    json_output(&applied)
}

#[test]
fn migrates_a_pnpm_workspace_to_bun_and_rolls_it_back() {
    let directory = tempfile::tempdir().expect("fixture directory");
    let binaries = fake_package_managers(&directory);
    let root = directory.path().join("project");
    write(
        &root.join("package.json"),
        r#"{
  "name": "pnpm-workspace-fixture",
  "private": true,
  "packageManager": "pnpm@11.21.0",
  "workspaces": ["apps/*", "packages/*"],
  "dependencies": { "shared": "workspace:^" }
}
"#,
    );
    write(
        &root.join("apps/web/package.json"),
        r#"{"name":"web","version":"1.0.0","dependencies":{"shared":"workspace:*"}}
"#,
    );
    write(
        &root.join("packages/shared/package.json"),
        r#"{"name":"shared","version":"2.4.0"}
"#,
    );
    write(
        &root.join("pnpm-workspace.yaml"),
        "packages:\n  - 'apps/*'\n  - 'packages/*'\n",
    );
    write(&root.join("pnpm-lock.yaml"), "lockfileVersion: '9.0'\n");
    write(
        &root.join(".github/workflows/ci.yml"),
        "steps:\n  - run: pnpm install --frozen-lockfile\n  - run: pnpm test\n",
    );

    let applied = plan_and_apply(&root, &binaries, "bun");
    assert_eq!(applied["status"], "completed");
    assert_eq!(applied["summary"]["runStatus"], "succeeded");
    let plan = artifact_content(&applied, "package-manager-plan");
    assert_eq!(plan["nativeImport"]["id"], "bun-pnpm-install-migration");
    let journal = artifact_content(&applied, "run-journal");
    assert_eq!(journal["processes"].as_array().map(Vec::len), Some(1));
    assert!(!applied.to_string().contains("fixture-secret-value"));
    assert!(root.join("bun.lock").is_file());
    assert!(!root.join("pnpm-lock.yaml").exists());
    assert!(!root.join("pnpm-workspace.yaml").exists());
    assert!(
        fs::read_to_string(root.join("package.json"))
            .expect("migrated manifest")
            .contains("\"packageManager\": \"bun@1.3.14\"")
    );
    assert!(
        fs::read_to_string(root.join(".github/workflows/ci.yml"))
            .expect("migrated workflow")
            .contains("bun install")
    );

    let run_id = applied["runId"].as_str().expect("run identifier");
    let rolled_back = run(&root, &binaries, &["rollback", run_id, "--approve", run_id]);
    assert!(rolled_back.status.success());
    assert_eq!(json_output(&rolled_back)["status"], "rolled-back");
    assert!(root.join("pnpm-lock.yaml").is_file());
    assert!(root.join("pnpm-workspace.yaml").is_file());
    assert!(!root.join("bun.lock").exists());
    assert!(
        fs::read_to_string(root.join("package.json"))
            .expect("restored manifest")
            .contains("\"packageManager\": \"pnpm@11.21.0\"")
    );
}

#[test]
fn migrates_an_npm_workspace_to_pnpm() {
    let directory = tempfile::tempdir().expect("fixture directory");
    let binaries = fake_package_managers(&directory);
    let root = directory.path().join("project");
    write(
        &root.join("package.json"),
        r#"{
  "name": "npm-workspace-fixture",
  "private": true,
  "packageManager": "npm@12.0.2",
  "workspaces": ["packages/*"],
  "overrides": {
    "parent": {
      "child": "1.2.3"
    }
  }
}
"#,
    );
    write(
        &root.join("packages/api/package.json"),
        r#"{"name":"api","version":"1.0.0"}
"#,
    );
    write(
        &root.join("package-lock.json"),
        "{\"lockfileVersion\":3,\"packages\":{\"\":{}}}\n",
    );

    let applied = plan_and_apply(&root, &binaries, "pnpm");
    assert_eq!(applied["status"], "completed");
    let plan = artifact_content(&applied, "package-manager-plan");
    assert_eq!(plan["nativeImport"]["id"], "pnpm-import");
    assert!(plan["operations"].as_array().is_some_and(|operations| {
        operations
            .iter()
            .any(|operation| operation["kind"] == "dependency.import-target")
    }));
    let run = artifact_content(&applied, "run-journal");
    assert_eq!(run["processes"].as_array().map(Vec::len), Some(2));
    assert!(root.join("pnpm-lock.yaml").is_file());
    assert!(root.join("pnpm-workspace.yaml").is_file());
    assert!(!root.join("package-lock.json").exists());
    assert!(
        fs::read_to_string(root.join("pnpm-workspace.yaml"))
            .expect("pnpm policy configuration")
            .contains("'parent>child': '1.2.3'")
    );
    let manifest: Value = serde_json::from_str(
        &fs::read_to_string(root.join("package.json")).expect("migrated manifest"),
    )
    .expect("migrated manifest JSON");
    assert!(manifest.get("overrides").is_none());
    assert!(
        manifest["packageManager"] == "pnpm@11.21.0",
        "target package manager pin should be rendered"
    );
}

#[test]
fn migrates_registry_and_lifecycle_policy_to_yarn_modern() {
    let directory = tempfile::tempdir().expect("fixture directory");
    let binaries = fake_package_managers(&directory);
    let root = directory.path().join("project");
    write(
        &root.join("package.json"),
        r#"{
  "name": "npm-yarn-fixture",
  "private": true,
  "packageManager": "npm@12.0.2",
  "trustedDependencies": ["esbuild"]
}
"#,
    );
    write(
        &root.join("package-lock.json"),
        "{\"lockfileVersion\":3,\"packages\":{\"\":{}}}\n",
    );
    write(
        &root.join(".npmrc"),
        "registry=https://registry.npmjs.org\n//registry.npmjs.org/:_authToken=${NPM_TOKEN}\n",
    );

    let applied = plan_and_apply(&root, &binaries, "yarn-modern");
    assert_eq!(applied["status"], "completed");
    assert!(root.join("yarn.lock").is_file());
    assert!(!root.join("package-lock.json").exists());
    assert!(!root.join(".npmrc").exists());
    let configuration =
        fs::read_to_string(root.join(".yarnrc.yml")).expect("Yarn Modern configuration");
    assert!(configuration.contains("nodeLinker: node-modules"));
    assert!(configuration.contains("enableScripts: false"));
    assert!(configuration.contains("'${NPM_TOKEN}'"));
    let manifest: Value = serde_json::from_str(
        &fs::read_to_string(root.join("package.json")).expect("migrated manifest"),
    )
    .expect("migrated manifest JSON");
    assert_eq!(manifest["dependenciesMeta"]["esbuild"]["built"], true);
    assert!(manifest.get("trustedDependencies").is_none());
}

#[test]
#[ignore = "requires the real Bun executable"]
fn migrates_a_real_pnpm_fixture_with_bun() {
    let bun_available = Command::new("bun")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success());
    if !bun_available {
        return;
    }
    let directory = tempfile::tempdir().expect("fixture directory");
    let binaries = tempfile::tempdir().expect("empty binary directory");
    let root = directory.path().join("project");
    write(
        &root.join("package.json"),
        r#"{"name":"live-bun-fixture","private":true,"packageManager":"pnpm@11.21.0","dependencies":{"local-package":"file:./local-package"}}
"#,
    );
    write(
        &root.join("local-package/package.json"),
        r#"{"name":"local-package","version":"1.0.0"}
"#,
    );
    write(
        &root.join("pnpm-lock.yaml"),
        "lockfileVersion: '9.0'\nimporters:\n  .: {}\n",
    );

    let applied = plan_and_apply(&root, binaries.path(), "bun");
    assert_eq!(applied["status"], "completed");
    assert!(root.join("bun.lock").is_file());
    assert!(!root.join("pnpm-lock.yaml").exists());
}

#[test]
fn trials_a_migration_without_changing_the_source_repository() {
    let directory = tempfile::tempdir().expect("fixture directory");
    let binaries = fake_package_managers(&directory);
    let root = directory.path().join("project");
    write(
        &root.join("package.json"),
        r#"{"name":"fixture","private":true,"packageManager":"pnpm@11.21.0"}
"#,
    );
    write(
        &root.join("pnpm-lock.yaml"),
        "lockfileVersion: '9.0'\npackages: {}\nsnapshots: {}\n",
    );
    let manifest_before = fs::read(root.join("package.json")).expect("source manifest");
    let lockfile_before = fs::read(root.join("pnpm-lock.yaml")).expect("source lockfile");

    let planned = run(&root, &binaries, &["to", "bun", "--trial"]);
    assert_eq!(planned.status.code(), Some(7));
    let planned = json_output(&planned);
    assert_eq!(planned["summary"]["trial"], true);
    let plan_id = planned["planId"].as_str().expect("plan identifier");

    let trial = run(
        &root,
        &binaries,
        &["to", "bun", "--trial", "--approve", plan_id],
    );
    assert!(
        trial.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&trial.stdout),
        String::from_utf8_lossy(&trial.stderr)
    );
    let trial = json_output(&trial);
    assert_eq!(trial["status"], "completed");
    assert_eq!(trial["summary"]["repositoryChanged"], false);
    assert_eq!(trial["summary"]["repositoryUnchanged"], true);
    assert!(trial["runId"].is_null());
    let report = artifact_content(&trial, "trial-report");
    assert_eq!(report["status"], "passed");
    assert_eq!(report["repositoryUnchanged"], true);
    assert_eq!(report["processes"].as_array().map(Vec::len), Some(1));

    assert_eq!(
        fs::read(root.join("package.json")).expect("source manifest after trial"),
        manifest_before
    );
    assert_eq!(
        fs::read(root.join("pnpm-lock.yaml")).expect("source lockfile after trial"),
        lockfile_before
    );
    assert!(!root.join("bun.lock").exists());
    assert!(!root.join(".pkgshift").exists());
}

#[test]
fn fails_verification_when_the_target_resolution_set_drifts() {
    let directory = tempfile::tempdir().expect("fixture directory");
    let binaries = fake_package_managers(&directory);
    let root = directory.path().join("project");
    write(
        &root.join("package.json"),
        r#"{
  "name": "fixture",
  "private": true,
  "packageManager": "npm@12.0.2",
  "dependencies": { "left-pad": "1.3.0" }
}
"#,
    );
    write(
        &root.join("package-lock.json"),
        r#"{
  "lockfileVersion": 3,
  "packages": {
    "": { "name": "fixture", "dependencies": { "left-pad": "1.3.0" } },
    "node_modules/left-pad": {
      "version": "1.3.0",
      "integrity": "sha512-source-integrity"
    }
  }
}
"#,
    );

    let planned = run(&root, &binaries, &["to", "pnpm"]);
    assert_eq!(planned.status.code(), Some(7));
    let planned = json_output(&planned);
    let plan_id = planned["planId"].as_str().expect("plan identifier");
    let applied = run(&root, &binaries, &["to", "pnpm", "--approve", plan_id]);
    assert_eq!(applied.status.code(), Some(5));
    let applied = json_output(&applied);
    assert_eq!(applied["status"], "failed");
    let verification = artifact_content(&applied, "verification-report");
    assert_eq!(verification["status"], "failed");
    assert_eq!(verification["lockGraphComparison"]["status"], "failed");
    assert_eq!(
        verification["lockGraphComparison"]["removedResolutions"],
        serde_json::json!(["left-pad@1.3.0"])
    );
    assert!(verification["checks"].as_array().is_some_and(|checks| {
        checks
            .iter()
            .any(|check| check["id"] == "dependency-graph-drift" && check["status"] == "failed")
    }));
}

#[test]
fn rejects_a_tampered_persisted_plan_before_mutation() {
    let directory = tempfile::tempdir().expect("fixture directory");
    let binaries = fake_package_managers(&directory);
    let root = directory.path().join("project");
    write(
        &root.join("package.json"),
        r#"{"name":"fixture","private":true,"packageManager":"pnpm@11.21.0"}
"#,
    );
    write(&root.join("pnpm-lock.yaml"), "lockfileVersion: '9.0'\n");

    let planned = run(
        &root,
        &binaries,
        &[
            "plan",
            "package-manager",
            "--to",
            "bun",
            "--state-dir",
            "state",
        ],
    );
    assert!(planned.status.success());
    let planned = json_output(&planned);
    let plan_id = planned["planId"].as_str().expect("plan identifier");
    let stored_path = root.join("state/plans").join(format!("{plan_id}.json"));
    let stored = fs::read_to_string(&stored_path).expect("stored plan");
    assert!(stored.contains("\"executable\": true"));
    fs::write(
        &stored_path,
        stored.replacen("\"executable\": true", "\"executable\": false", 1),
    )
    .expect("tampered plan");

    let applied = run(
        &root,
        &binaries,
        &[
            "apply",
            plan_id,
            "--state-dir",
            "state",
            "--approve",
            plan_id,
        ],
    );
    assert_eq!(applied.status.code(), Some(8));
    let applied = json_output(&applied);
    assert_eq!(applied["diagnostics"][0]["code"], "PKGSHIFT_INTERNAL_ERROR");
    assert!(
        applied["diagnostics"][0]["summary"]
            .as_str()
            .is_some_and(|value| value.contains("integrity verification"))
    );
    assert!(root.join("pnpm-lock.yaml").is_file());
    assert!(!root.join("bun.lock").exists());
}

#[test]
fn keeps_a_failed_install_recoverable() {
    let directory = tempfile::tempdir().expect("fixture directory");
    let binaries = fake_package_managers(&directory);
    let root = directory.path().join("project");
    write(
        &root.join("package.json"),
        r#"{"name":"fixture","private":true,"packageManager":"pnpm@11.21.0"}
"#,
    );
    write(&root.join("pnpm-lock.yaml"), "lockfileVersion: '9.0'\n");

    let planned = run(&root, &binaries, &["to", "bun"]);
    assert_eq!(planned.status.code(), Some(7));
    let planned = json_output(&planned);
    let plan_id = planned["planId"].as_str().expect("plan identifier");
    write(
        &binaries.join("bun"),
        "#!/bin/sh\nprintf 'partial lock\\n' > bun.lock\nexit 42\n",
    );
    fs::set_permissions(binaries.join("bun"), fs::Permissions::from_mode(0o755))
        .expect("failing binary permissions");

    let applied = run(&root, &binaries, &["to", "bun", "--approve", plan_id]);
    assert_eq!(applied.status.code(), Some(5));
    let applied = json_output(&applied);
    assert_eq!(applied["status"], "failed");
    assert_eq!(applied["summary"]["runStatus"], "failed");
    let run_id = applied["runId"].as_str().expect("failed run identifier");
    assert!(root.join("bun.lock").is_file());

    let rolled_back = run(&root, &binaries, &["rollback", run_id, "--approve", run_id]);
    assert!(rolled_back.status.success());
    assert_eq!(json_output(&rolled_back)["status"], "rolled-back");
    assert!(root.join("pnpm-lock.yaml").is_file());
    assert!(!root.join("bun.lock").exists());
    assert!(
        fs::read_to_string(root.join("package.json"))
            .expect("restored manifest")
            .contains("\"packageManager\":\"pnpm@11.21.0\"")
    );
}

#[test]
fn rejects_repository_drift_before_creating_a_run() {
    let directory = tempfile::tempdir().expect("fixture directory");
    let binaries = fake_package_managers(&directory);
    let root = directory.path().join("project");
    write(
        &root.join("package.json"),
        r#"{"name":"fixture","private":true,"packageManager":"pnpm@11.21.0"}
"#,
    );
    write(&root.join("pnpm-lock.yaml"), "lockfileVersion: '9.0'\n");

    let planned = run(
        &root,
        &binaries,
        &[
            "plan",
            "package-manager",
            "--to",
            "bun",
            "--state-dir",
            "state",
        ],
    );
    assert!(planned.status.success());
    let planned = json_output(&planned);
    let plan_id = planned["planId"].as_str().expect("plan identifier");
    write(&root.join("pnpm-lock.yaml"), "lockfileVersion: '9.1'\n");

    let applied = run(
        &root,
        &binaries,
        &[
            "apply",
            plan_id,
            "--state-dir",
            "state",
            "--approve",
            plan_id,
        ],
    );
    assert_eq!(applied.status.code(), Some(4));
    let applied = json_output(&applied);
    assert_eq!(
        applied["diagnostics"][0]["code"],
        "PLAN_PRECONDITION_FAILED"
    );
    assert!(!root.join("state/runs").exists());
    assert!(!root.join("bun.lock").exists());
}
