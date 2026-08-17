#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;
use tempfile::TempDir;

const TEXT_PATCH: &str = "diff --git a/index.js b/index.js\n--- a/index.js\n+++ b/index.js\n@@ -1 +1 @@\n-old\n+// pkgshift patch fixture\n+new\n";

#[path = "e2e/comparison.rs"]
mod comparison;
#[path = "e2e/doctor.rs"]
mod doctor;
#[path = "e2e/explain.rs"]
mod explain;
#[path = "e2e/runtime.rs"]
mod runtime;
#[path = "e2e/skill.rs"]
mod skill;

fn write(path: &Path, content: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("fixture parent directory");
    }
    fs::write(path, content).expect("fixture file");
}

fn fake_package_managers(directory: &TempDir) -> PathBuf {
    let binary_directory = directory.path().join("bin");
    fs::create_dir_all(&binary_directory).expect("fake binary directory");
    for (name, version, script_subcommand, lockfile_command) in [
        (
            "npm",
            "12.0.2",
            "run",
            "printf '%s\\n' '{\"lockfileVersion\":3,\"packages\":{}}' > package-lock.json",
        ),
        (
            "bun",
            "1.3.14",
            "run",
            "printf '%s\\n' '{\"lockfileVersion\":1,\"packages\":{}}' > bun.lock",
        ),
        (
            "pnpm",
            "11.21.0",
            "run",
            "printf '%s\\n' \"lockfileVersion: '9.0'\" 'packages: {}' 'snapshots: {}' > pnpm-lock.yaml",
        ),
        (
            "yarn",
            "1.22.22 4.18.0",
            "run",
            "printf '%s\\n' '__metadata:' '  version: 8' > yarn.lock",
        ),
        (
            "vlt",
            "1.0.2",
            "run",
            "printf '%s\\n' '{\"lockfileVersion\":1,\"nodes\":{},\"edges\":{}}' > vlt-lock.json",
        ),
        (
            "deno",
            "2.9.5",
            "task",
            "printf '%s\\n' '{\"version\":\"5\",\"npm\":{}}' > deno.lock",
        ),
    ] {
        let path = binary_directory.join(name);
        write(
            &path,
            &format!(
                "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then printf '%s\\n' '{version}'; exit 0; fi\nprintf 'fixture-secret-value\\n'\nprintf 'fixture-secret-value\\n' >&2\nif [ \"$1\" = \"{script_subcommand}\" ]; then printf '%s\\n' \"$2\" > .pkgshift-script-ran; if [ \"$2\" = \"fail\" ]; then exit 9; fi; exit 0; fi\n{lockfile_command}\n"
            ),
        );
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755))
            .expect("fake binary permissions");
    }
    binary_directory
}

#[test]
fn binds_verification_policy_and_exact_target_executable_to_the_run() {
    let directory = tempfile::tempdir().expect("fixture directory");
    let binaries = fake_package_managers(&directory);
    let root = directory.path().join("project");
    write(
        &root.join("package.json"),
        r#"{"name":"policy-fixture","private":true,"packageManager":"pnpm@11.21.0"}
"#,
    );
    write(&root.join("pnpm-lock.yaml"), "lockfileVersion: '9.0'\n");

    let default_plan = json_output(&run(&root, &binaries, &["to", "bun"]));
    let planned = json_output(&run(
        &root,
        &binaries,
        &[
            "to",
            "bun",
            "--target-platform",
            "linux/x64/glibc",
            "--target-platform",
            "darwin/arm64",
            "--edge-equivalence",
            "strict",
        ],
    ));
    let plan_id = planned["planId"].as_str().expect("plan identifier");
    assert_ne!(default_plan["planId"], planned["planId"]);
    let plan = artifact_content(&planned, "package-manager-plan");
    assert_eq!(
        plan["verificationPolicy"],
        serde_json::json!({
            "targetPlatforms": [
                { "os": "darwin", "cpu": "arm64" },
                { "os": "linux", "cpu": "x64", "libc": "glibc" }
            ],
            "edgeEquivalence": "strict"
        })
    );
    assert_eq!(plan["targetExecutable"]["requiredVersion"], "1.3.14");
    assert_eq!(plan["targetExecutable"]["packageManagerPin"], "bun@1.3.14");
    let approved_argv = planned["nextActions"][0]["argv"]
        .as_array()
        .expect("approval argv");
    assert!(
        approved_argv
            .windows(2)
            .any(|values| { values == ["--target-platform", "darwin/arm64"] })
    );
    assert!(
        approved_argv
            .windows(2)
            .any(|values| { values == ["--target-platform", "linux/x64/glibc"] })
    );
    assert!(
        approved_argv
            .windows(2)
            .any(|values| { values == ["--edge-equivalence", "strict"] })
    );

    let bun = binaries.join("bun");
    write(
        &bun,
        "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then printf '%s\\n' '9.9.9'; exit 0; fi\nexit 99\n",
    );
    fs::set_permissions(&bun, fs::Permissions::from_mode(0o755)).expect("fake binary permissions");
    let rejected = run(
        &root,
        &binaries,
        &[
            "to",
            "bun",
            "--target-platform",
            "linux/x64/glibc",
            "--target-platform",
            "darwin/arm64",
            "--edge-equivalence",
            "strict",
            "--approve",
            plan_id,
        ],
    );
    assert_eq!(rejected.status.code(), Some(4));
    assert!(
        json_output(&rejected)["diagnostics"]
            .as_array()
            .is_some_and(|diagnostics| diagnostics
                .iter()
                .any(|diagnostic| { diagnostic["code"] == "TARGET_EXECUTABLE_VERSION_MISMATCH" }))
    );
    assert!(!root.join("bun.lock").exists());
    assert!(!root.join(".pkgshift/state/runs").exists());

    fake_package_managers(&directory);
    let applied = run(
        &root,
        &binaries,
        &[
            "to",
            "bun",
            "--target-platform",
            "linux/x64/glibc",
            "--target-platform",
            "darwin/arm64",
            "--edge-equivalence",
            "strict",
            "--approve",
            plan_id,
        ],
    );
    assert!(
        applied.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&applied.stdout),
        String::from_utf8_lossy(&applied.stderr)
    );
    let applied = json_output(&applied);
    let run = artifact_content(&applied, "run-journal");
    assert_eq!(run["targetExecutable"]["program"], "bun");
    assert_eq!(run["targetExecutable"]["version"], "1.3.14");
    assert_eq!(run["targetExecutable"]["packageManagerPin"], "bun@1.3.14");
    let verification = artifact_content(&applied, "verification-report");
    assert!(
        verification["checks"]
            .as_array()
            .is_some_and(|checks| checks.iter().any(|check| {
                check["id"] == "target-executable-version" && check["status"] == "passed"
            }))
    );
}

#[test]
fn runs_only_an_explicitly_planned_representative_script() {
    let directory = tempfile::tempdir().expect("fixture directory");
    let binaries = fake_package_managers(&directory);
    let root = directory.path().join("project");
    write(
        &root.join("package.json"),
        r#"{
  "name": "script-verification-fixture",
  "private": true,
  "packageManager": "pnpm@11.21.0",
  "scripts": { "smoke": "node smoke.js", "unselected": "node unselected.js" }
}
"#,
    );
    write(&root.join("pnpm-lock.yaml"), "lockfileVersion: '9.0'\n");

    let planned = run(&root, &binaries, &["to", "bun", "--verify-script", "smoke"]);
    assert_eq!(planned.status.code(), Some(7));
    let planned = json_output(&planned);
    let plan_id = planned["planId"].as_str().expect("plan identifier");
    assert_eq!(
        planned["nextActions"][0]["argv"],
        serde_json::json!([
            "pkgshift",
            "to",
            "bun",
            "--verify-script",
            "smoke",
            "--approve",
            plan_id,
            "--json",
            "--no-color",
            "--non-interactive"
        ])
    );
    let plan = artifact_content(&planned, "package-manager-plan");
    let script_operation = plan["operations"]
        .as_array()
        .expect("plan operations")
        .iter()
        .find(|operation| operation["kind"] == "verification.run-script")
        .expect("representative script operation");
    assert_eq!(
        script_operation["command"],
        serde_json::json!(["bun", "run", "smoke"])
    );
    assert_eq!(script_operation["timeoutSeconds"], 300);

    let applied = run(
        &root,
        &binaries,
        &[
            "to",
            "bun",
            "--verify-script",
            "smoke",
            "--approve",
            plan_id,
        ],
    );
    assert!(
        applied.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&applied.stdout),
        String::from_utf8_lossy(&applied.stderr)
    );
    let applied = json_output(&applied);
    let journal = artifact_content(&applied, "run-journal");
    let script_process = journal["processes"]
        .as_array()
        .expect("process journal")
        .iter()
        .find(|process| process["argv"] == serde_json::json!(["bun", "run", "smoke"]))
        .expect("representative script process");
    assert_eq!(script_process["success"], true);
    assert_eq!(script_process["timedOut"], false);
    assert_eq!(
        fs::read_to_string(root.join(".pkgshift-script-ran")).expect("script marker"),
        "smoke\n"
    );
    let verification = artifact_content(&applied, "verification-report");
    let script_check = verification["checks"]
        .as_array()
        .expect("verification checks")
        .iter()
        .find(|check| check["id"] == "representative-scripts")
        .expect("representative script check");
    assert_eq!(script_check["status"], "passed");
    assert_eq!(
        script_check["summary"],
        "All 1 explicitly selected representative scripts passed."
    );
    assert!(!applied.to_string().contains("fixture-secret-value"));
}

#[test]
fn reports_a_failed_representative_script_as_a_verification_failure() {
    let directory = tempfile::tempdir().expect("fixture directory");
    let binaries = fake_package_managers(&directory);
    let root = directory.path().join("project");
    write(
        &root.join("package.json"),
        r#"{"name":"script-failure-fixture","private":true,"packageManager":"pnpm@11.21.0","scripts":{"fail":"exit 9"}}
"#,
    );
    write(&root.join("pnpm-lock.yaml"), "lockfileVersion: '9.0'\n");

    let planned = json_output(&run(
        &root,
        &binaries,
        &["to", "bun", "--verify-script", "fail"],
    ));
    let plan_id = planned["planId"].as_str().expect("plan identifier");
    let applied = run(
        &root,
        &binaries,
        &["to", "bun", "--verify-script", "fail", "--approve", plan_id],
    );
    assert_eq!(applied.status.code(), Some(5));
    let applied = json_output(&applied);
    assert_eq!(applied["status"], "failed");
    let journal = artifact_content(&applied, "run-journal");
    let failed_process = journal["processes"]
        .as_array()
        .expect("process journal")
        .iter()
        .find(|process| process["argv"] == serde_json::json!(["bun", "run", "fail"]))
        .expect("failed representative script process");
    assert_eq!(failed_process["exitCode"], 9);
    assert_eq!(failed_process["success"], false);
    let verification = artifact_content(&applied, "verification-report");
    let script_check = verification["checks"]
        .as_array()
        .expect("verification checks")
        .iter()
        .find(|check| check["id"] == "representative-scripts")
        .expect("representative script check");
    assert_eq!(script_check["status"], "failed");
    assert_eq!(
        script_check["evidence"],
        serde_json::json!(["script:fail;status:failed;exitCode:9"])
    );
    assert!(!applied.to_string().contains("fixture-secret-value"));
}

fn bunx_package_manager(directory: &TempDir, name: &str, package: &str) -> PathBuf {
    let binary_directory = directory.path().join("real-bin");
    fs::create_dir_all(&binary_directory).expect("real binary directory");
    let path = binary_directory.join(name);
    write(
        &path,
        &format!("#!/bin/sh\nexec bunx --bun {package} \"$@\"\n"),
    );
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755))
        .expect("real wrapper permissions");
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
    plan_and_apply_with_options(root, binaries, target, &[])
}

fn plan_and_apply_with_options(
    root: &Path,
    binaries: &Path,
    target: &str,
    options: &[&str],
) -> Value {
    let mut preview_arguments = vec!["to", target];
    preview_arguments.extend_from_slice(options);
    let planned = run(root, binaries, &preview_arguments);
    assert_eq!(planned.status.code(), Some(7));
    let planned = json_output(&planned);
    assert_eq!(planned["status"], "planned");
    assert_eq!(planned["summary"]["repositoryChanged"], false);
    let plan_id = planned["planId"]
        .as_str()
        .expect("planned result identifier");

    let mut apply_arguments = preview_arguments;
    apply_arguments.extend(["--approve", plan_id]);
    let applied = run(root, binaries, &apply_arguments);
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
  "scripts": { "test": "pnpm test", "prepare": "echo pnpm install" },
  "volta": { "node": "22.22.0", "pnpm": "11.21.0" },
  "engines": { "node": ">=22", "pnpm": ">=11" },
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
        &root.join("node_modules/.pnpm/source-marker"),
        "source dependency state\n",
    );
    write(
        &root.join(".github/workflows/ci.yml"),
        "steps:\n  - uses: pnpm/action-setup@v4\n  - run: pnpm install --frozen-lockfile\n  - run: pnpm test\n  - run: echo ${{ hashFiles('pnpm-lock.yaml') }}\n",
    );
    write(&root.join("Dockerfile"), "FROM node:22\nRUN pnpm install\n");
    write(&root.join("Makefile"), "test:\n\tpnpm test\n");
    write(
        &root.join(".tool-versions"),
        "nodejs 22.22.0\npnpm 11.21.0\n",
    );
    write(
        &root.join("mise.toml"),
        "[tools]\nnode = \"22.22.0\"\npnpm = \"11.21.0\"\n",
    );
    write(
        &root.join(".devcontainer/devcontainer.json"),
        "{\n  \"postCreateCommand\": \"pnpm install && pnpm test\"\n}\n",
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
    let manifest = fs::read_to_string(root.join("package.json")).expect("migrated manifest");
    assert!(manifest.contains("\"test\": \"bun run test\""));
    assert!(manifest.contains("\"prepare\": \"echo pnpm install\""));
    assert!(manifest.contains("\"node\": \"22.22.0\""));
    assert!(!manifest.contains("\"pnpm\": \"11.21.0\""));
    assert!(manifest.contains("\"bun\": \">=1.3.14\""));
    let workflow =
        fs::read_to_string(root.join(".github/workflows/ci.yml")).expect("migrated workflow");
    assert!(workflow.contains("uses: oven-sh/setup-bun@v2"));
    assert!(workflow.contains("bun install"));
    assert!(workflow.contains("bun run test"));
    assert!(workflow.contains("hashFiles('bun.lock')"));
    assert!(
        fs::read_to_string(root.join("Dockerfile"))
            .expect("migrated Dockerfile")
            .contains("RUN bun install")
    );
    assert!(
        fs::read_to_string(root.join("Makefile"))
            .expect("migrated Makefile")
            .contains("\tbun run test")
    );
    assert_eq!(
        fs::read_to_string(root.join(".tool-versions")).expect("migrated tool versions"),
        "nodejs 22.22.0\nbun 1.3.14\n"
    );
    assert_eq!(
        fs::read_to_string(root.join("mise.toml")).expect("migrated mise configuration"),
        "[tools]\nnode = \"22.22.0\"\nbun = \"1.3.14\"\n"
    );
    assert!(
        fs::read_to_string(root.join(".devcontainer/devcontainer.json"))
            .expect("migrated devcontainer")
            .contains("bun install && bun run test")
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
fn migrates_an_npm_workspace_to_vlt() {
    let directory = tempfile::tempdir().expect("fixture directory");
    let binaries = fake_package_managers(&directory);
    let root = directory.path().join("project");
    write(
        &root.join("package.json"),
        r#"{
  "name": "npm-to-vlt-fixture",
  "private": true,
  "packageManager": "npm@12.0.2",
  "workspaces": ["packages/*"],
  "overrides": { "parent": { "child": "2.0.0" } }
}
"#,
    );
    write(
        &root.join("packages/app/package.json"),
        r#"{"name":"app","version":"1.0.0"}
"#,
    );
    write(
        &root.join("package-lock.json"),
        "{\"lockfileVersion\":3,\"packages\":{\"\":{}}}\n",
    );
    write(
        &root.join(".npmrc"),
        "registry=https://registry.example.test/\n",
    );

    let applied = plan_and_apply(&root, &binaries, "vlt");
    assert_eq!(applied["status"], "completed");
    assert!(root.join("vlt-lock.json").is_file());
    assert!(!root.join("package-lock.json").exists());
    assert!(!root.join(".npmrc").exists());
    let configuration: Value = serde_json::from_str(
        &fs::read_to_string(root.join("vlt.json")).expect("vlt configuration"),
    )
    .expect("vlt configuration JSON");
    assert_eq!(
        configuration["workspaces"],
        serde_json::json!(["packages/*"])
    );
    assert_eq!(
        configuration["modifiers"][":root > #parent > #child"],
        "2.0.0"
    );
}

#[test]
fn migrates_a_pnpm_workspace_to_deno_dependency_mode() {
    let directory = tempfile::tempdir().expect("fixture directory");
    let binaries = fake_package_managers(&directory);
    let root = directory.path().join("project");
    write(
        &root.join("package.json"),
        r#"{
  "name": "pnpm-to-deno-fixture",
  "private": true,
  "packageManager": "pnpm@11.21.0",
  "overrides": { "parent": { "child": "2.0.0" } }
}

"#,
    );
    write(
        &root.join("packages/app/package.json"),
        r#"{"name":"app","version":"1.0.0"}
"#,
    );
    write(
        &root.join("pnpm-workspace.yaml"),
        "packages:\n  - 'packages/*'\nnodeLinker: isolated\n",
    );
    write(
        &root.join("pnpm-lock.yaml"),
        "lockfileVersion: '9.0'\npackages: {}\nsnapshots: {}\n",
    );
    write(&root.join("README.md"), "Run `pnpm run test`.\n");

    let applied = plan_and_apply(&root, &binaries, "deno");
    assert_eq!(applied["status"], "completed");
    assert!(root.join("deno.lock").is_file());
    assert!(!root.join("pnpm-lock.yaml").exists());
    assert!(!root.join("pnpm-workspace.yaml").exists());
    let configuration: Value = serde_json::from_str(
        &fs::read_to_string(root.join("deno.json")).expect("Deno configuration"),
    )
    .expect("Deno configuration JSON");
    assert_eq!(
        configuration["workspace"],
        serde_json::json!(["packages/*"])
    );
    assert_eq!(configuration["nodeModulesLinker"], "isolated");
    assert!(
        fs::read_to_string(root.join("README.md"))
            .expect("migrated documentation")
            .contains("deno task test")
    );
}

#[test]
fn migrates_bun_to_deno_after_removing_source_dependency_state() {
    let directory = tempfile::tempdir().expect("fixture directory");
    let binaries = fake_package_managers(&directory);
    let root = directory.path().join("project");
    write(
        &root.join("package.json"),
        r#"{
  "name": "bun-cleanup-fixture",
  "private": true,
  "packageManager": "bun@1.3.14",
  "workspaces": ["packages/*"]
}
"#,
    );
    write(
        &root.join("packages/app/package.json"),
        r#"{"name":"app","version":"1.0.0"}
"#,
    );
    write(
        &root.join("bun.lock"),
        "{\"lockfileVersion\":1,\"packages\":{}}\n",
    );
    write(
        &root.join("bunfig.toml"),
        "[install]\nlinker = \"isolated\"\n",
    );
    write(&root.join("node_modules/.bun/source-marker"), "bun\n");
    write(
        &root.join("packages/app/node_modules/.bun/source-marker"),
        "bun\n",
    );

    let applied = plan_and_apply(&root, &binaries, "deno");
    assert_eq!(applied["status"], "completed");
    assert_eq!(applied["summary"]["runStatus"], "succeeded");
    assert!(!root.join("node_modules").exists());
    assert!(!root.join("packages/app/node_modules").exists());
    assert!(!root.join("bun.lock").exists());
    assert!(!root.join("bunfig.toml").exists());
    assert!(root.join("deno.lock").is_file());

    let journal = artifact_content(&applied, "run-journal");
    assert_eq!(
        journal["dependencyStateCleanups"][0]["removedPaths"],
        serde_json::json!(["node_modules", "packages/app/node_modules"])
    );
    let verification = artifact_content(&applied, "verification-report");
    for check_id in ["clean-target-install", "source-artifact-residue"] {
        let check = verification["checks"]
            .as_array()
            .expect("verification checks")
            .iter()
            .find(|check| check["id"] == check_id)
            .unwrap_or_else(|| panic!("missing {check_id} check"));
        assert_eq!(check["status"], "passed");
    }
}

#[test]
#[ignore = "requires registry access and the pinned vlt package"]
fn migrates_a_dependency_with_real_vlt() {
    let directory = tempfile::tempdir().expect("fixture directory");
    let binaries = PathBuf::from(
        std::env::var_os("PKGSHIFT_REAL_VLT_BIN")
            .expect("PKGSHIFT_REAL_VLT_BIN must point to the pinned vlt bin directory"),
    );
    let root = directory.path().join("project");
    let probe = directory.path().join("probe");
    write(
        &probe.join("package.json"),
        r#"{"name":"vlt-runtime-probe","private":true,"dependencies":{"is-number":"7.0.0"}}
"#,
    );
    write(
        &probe.join("vlt.json"),
        "{\"config\":{\"registry\":\"https://registry.npmjs.org/\"}}\n",
    );
    let probed = Command::new(binaries.join("vlt"))
        .arg("install")
        .current_dir(&probe)
        .output()
        .expect("vlt runtime probe");
    assert!(
        probed.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&probed.stdout),
        String::from_utf8_lossy(&probed.stderr)
    );
    write(
        &root.join("package.json"),
        r#"{
  "name": "real-vlt-fixture",
  "private": true,
  "packageManager": "bun@1.3.14",
  "workspaces": ["packages/*"]
}
"#,
    );
    write(
        &root.join("packages/app/package.json"),
        r#"{"name":"@fixture/app","version":"1.0.0","dependencies":{"@fixture/lib":"workspace:*"}}
"#,
    );
    write(
        &root.join("packages/lib/package.json"),
        r#"{"name":"@fixture/lib","version":"1.0.0","dependencies":{"is-number":"7.0.0"}}
"#,
    );
    let installed = Command::new("bun")
        .args(["install", "--ignore-scripts"])
        .current_dir(&root)
        .output()
        .expect("Bun source installation");
    assert!(
        installed.status.success(),
        "{}",
        String::from_utf8_lossy(&installed.stderr)
    );

    let applied = plan_and_apply(&root, &binaries, "vlt");
    assert_eq!(applied["status"], "completed");
    assert_eq!(applied["summary"]["runStatus"], "succeeded");
    assert!(root.join("vlt-lock.json").is_file());
    assert!(!root.join("bun.lock").exists());
    let configuration: Value = serde_json::from_str(
        &fs::read_to_string(root.join("vlt.json")).expect("vlt workspace configuration"),
    )
    .expect("vlt workspace configuration JSON");
    assert_eq!(
        configuration["workspaces"],
        serde_json::json!(["packages/*"])
    );
}

#[test]
#[ignore = "requires registry access and the pinned Deno package"]
fn migrates_a_dependency_with_real_deno() {
    let directory = tempfile::tempdir().expect("fixture directory");
    let binaries = bunx_package_manager(&directory, "deno", "deno@2.9.5");
    let root = directory.path().join("project");
    write(
        &root.join("package.json"),
        r#"{
  "name": "real-deno-fixture",
  "private": true,
  "packageManager": "bun@1.3.14",
  "workspaces": ["packages/*"]
}
"#,
    );
    write(
        &root.join("packages/app/package.json"),
        r#"{"name":"@fixture/app","version":"1.0.0","dependencies":{"@fixture/lib":"workspace:*"}}
"#,
    );
    write(
        &root.join("packages/lib/package.json"),
        r#"{"name":"@fixture/lib","version":"1.0.0","dependencies":{"is-number":"7.0.0"}}
"#,
    );
    let installed = Command::new("bun")
        .args(["install", "--ignore-scripts"])
        .current_dir(&root)
        .output()
        .expect("Bun source installation");
    assert!(
        installed.status.success(),
        "{}",
        String::from_utf8_lossy(&installed.stderr)
    );
    assert!(root.join("node_modules/.bun").is_dir());

    let applied = plan_and_apply(&root, &binaries, "deno");
    assert_eq!(applied["status"], "completed");
    assert_eq!(applied["summary"]["runStatus"], "succeeded");
    assert!(root.join("deno.lock").is_file());
    assert!(!root.join("bun.lock").exists());
    assert!(!root.join("node_modules/.bun").exists());
    let journal = artifact_content(&applied, "run-journal");
    assert!(
        journal["dependencyStateCleanups"][0]["removedPaths"]
            .as_array()
            .is_some_and(|paths| paths.iter().any(|path| path == "node_modules"))
    );
    let verification = artifact_content(&applied, "verification-report");
    assert!(verification["checks"].as_array().is_some_and(|checks| {
        checks
            .iter()
            .any(|check| check["id"] == "clean-target-install" && check["status"] == "passed")
    }));
    let configuration: Value = serde_json::from_str(
        &fs::read_to_string(root.join("deno.json")).expect("Deno workspace configuration"),
    )
    .expect("Deno workspace configuration JSON");
    assert_eq!(
        configuration["workspace"],
        serde_json::json!(["packages/*"])
    );
}

#[test]
fn carries_package_extensions_from_npm_through_pnpm_to_yarn_modern() {
    let directory = tempfile::tempdir().expect("fixture directory");
    let binaries = fake_package_managers(&directory);
    let root = directory.path().join("project");
    write(
        &root.join("package.json"),
        r#"{
  "name": "package-extensions-fixture",
  "private": true,
  "packageManager": "npm@12.0.2",
  "packageExtensions": {
    "broken-package@^1": {
      "dependencies": { "missing-runtime-dep": "^2.0.0" },
      "peerDependencies": { "react": "*" },
      "peerDependenciesMeta": { "react": { "optional": true } }
    }
  }
}
"#,
    );
    write(
        &root.join("package-lock.json"),
        "{\"lockfileVersion\":3,\"packages\":{\"\":{}}}\n",
    );

    let pnpm = plan_and_apply(&root, &binaries, "pnpm");
    assert_eq!(pnpm["status"], "completed");
    let pnpm_configuration =
        fs::read_to_string(root.join("pnpm-workspace.yaml")).expect("pnpm workspace configuration");
    assert!(pnpm_configuration.contains("packageExtensions:"));
    assert!(pnpm_configuration.contains("'broken-package@^1':"));
    assert!(pnpm_configuration.contains("missing-runtime-dep: '^2.0.0'"));

    let yarn = plan_and_apply(&root, &binaries, "yarn-modern");
    assert_eq!(yarn["status"], "completed");
    let yarn_configuration =
        fs::read_to_string(root.join(".yarnrc.yml")).expect("Yarn Modern configuration");
    assert!(yarn_configuration.contains("packageExtensions:"));
    assert!(yarn_configuration.contains("peerDependenciesMeta:"));
    assert!(yarn_configuration.contains("optional: true"));
    let manifest: Value = serde_json::from_str(
        &fs::read_to_string(root.join("package.json")).expect("migrated manifest"),
    )
    .expect("migrated manifest JSON");
    assert!(manifest.get("packageExtensions").is_none());
}

#[test]
fn migrates_a_yarn_patch_protocol_dependency_to_bun_policy() {
    let directory = tempfile::tempdir().expect("fixture directory");
    let binaries = fake_package_managers(&directory);
    let root = directory.path().join("project");
    write(
        &root.join("package.json"),
        r#"{
  "name": "yarn-patch-fixture",
  "private": true,
  "packageManager": "yarn@4.18.0",
  "dependencies": {
    "left-pad": "patch:left-pad@npm%3A1.3.0#~/.yarn/patches/left-pad.patch"
  }
}
"#,
    );
    write(&root.join(".yarnrc.yml"), "nodeLinker: node-modules\n");
    write(&root.join(".yarn/patches/left-pad.patch"), TEXT_PATCH);

    let applied = plan_and_apply(&root, &binaries, "bun");
    assert_eq!(applied["status"], "completed");
    let manifest: Value = serde_json::from_str(
        &fs::read_to_string(root.join("package.json")).expect("migrated manifest"),
    )
    .expect("migrated manifest JSON");
    assert_eq!(manifest["dependencies"]["left-pad"], "1.3.0");
    assert_eq!(
        manifest["patchedDependencies"]["left-pad@1.3.0"],
        ".yarn/patches/left-pad.patch"
    );
    assert!(root.join(".yarn/patches/left-pad.patch").is_file());
    assert!(!root.join(".yarnrc.yml").exists());
}

#[test]
fn rejects_a_patch_change_after_exact_plan_approval() {
    let directory = tempfile::tempdir().expect("fixture directory");
    let binaries = fake_package_managers(&directory);
    let root = directory.path().join("project");
    write(
        &root.join("package.json"),
        r#"{"name":"fixture","private":true,"packageManager":"yarn@4.18.0","dependencies":{"left-pad":"patch:left-pad@npm%3A1.3.0#~/.yarn/patches/left-pad.patch"}}
"#,
    );
    write(&root.join(".yarnrc.yml"), "nodeLinker: node-modules\n");
    let patch_path = root.join(".yarn/patches/left-pad.patch");
    write(&patch_path, TEXT_PATCH);

    let planned = run(&root, &binaries, &["to", "bun"]);
    assert_eq!(planned.status.code(), Some(7));
    let planned = json_output(&planned);
    let plan_id = planned["planId"].as_str().expect("plan identifier");

    write(
        &patch_path,
        &TEXT_PATCH.replace("pkgshift patch fixture", "changed patch fixture"),
    );
    let applied = run(&root, &binaries, &["to", "bun", "--approve", plan_id]);
    assert!(!applied.status.success());
    assert!(!root.join("bun.lock").exists());
    let manifest = fs::read_to_string(root.join("package.json")).expect("source manifest");
    assert!(manifest.contains("patch:left-pad"));
    assert!(!manifest.contains("patchedDependencies"));
}

#[test]
fn blocks_a_symbolic_link_patch_path_before_approval() {
    use std::os::unix::fs::symlink;

    let directory = tempfile::tempdir().expect("fixture directory");
    let binaries = fake_package_managers(&directory);
    let root = directory.path().join("project");
    write(
        &root.join("package.json"),
        r#"{"name":"fixture","private":true,"packageManager":"bun@1.3.14","patchedDependencies":{"left-pad@1.3.0":"patches/linked.patch"}}
"#,
    );
    write(&root.join("bun.lock"), "{}\n");
    write(&root.join("patches/real.patch"), TEXT_PATCH);
    symlink("real.patch", root.join("patches/linked.patch")).expect("patch symlink");

    let planned = run(&root, &binaries, &["to", "pnpm"]);
    assert!(!planned.status.success());
    let planned = json_output(&planned);
    assert!(
        planned["diagnostics"]
            .as_array()
            .is_some_and(|diagnostics| {
                diagnostics
                    .iter()
                    .any(|entry| entry["code"] == "PATCH_PATH_UNSUPPORTED")
            })
    );
    assert!(!root.join("pnpm-lock.yaml").exists());
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
        r#"{"name":"live-bun-fixture","private":true,"packageManager":"pnpm@11.21.0","scripts":{"smoke":"bun -e \"if (typeof Bun.version !== 'string') process.exit(1)\""},"dependencies":{"local-package":"file:./local-package"}}
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

    let applied =
        plan_and_apply_with_options(&root, binaries.path(), "bun", &["--verify-script", "smoke"]);
    assert_eq!(applied["status"], "completed");
    assert!(root.join("bun.lock").is_file());
    assert!(!root.join("pnpm-lock.yaml").exists());
    let verification = artifact_content(&applied, "verification-report");
    let script_check = verification["checks"]
        .as_array()
        .expect("verification checks")
        .iter()
        .find(|check| check["id"] == "representative-scripts")
        .expect("representative script check");
    assert_eq!(script_check["status"], "passed");
}

#[test]
#[ignore = "requires the real Bun executable and registry access"]
fn applies_a_migrated_yarn_patch_with_real_bun() {
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
        r#"{
  "name": "live-yarn-patch-fixture",
  "private": true,
  "packageManager": "yarn@4.18.0",
  "dependencies": {
    "left-pad": "patch:left-pad@npm%3A1.3.0#~/.yarn/patches/left-pad.patch"
  }
}
"#,
    );
    write(&root.join(".yarnrc.yml"), "nodeLinker: node-modules\n");
    write(
        &root.join(".yarn/patches/left-pad.patch"),
        "diff --git a/index.js b/index.js\nindex e90aec35d979c42dcd4ddfacb4768c00d7102349..ecd9cc2c51b74be58db5df6d039b797a7c617e0e 100644\n--- a/index.js\n+++ b/index.js\n@@ -4,6 +4,7 @@\n      * To Public License, Version 2, as published by Sam Hocevar. See\n      * http://www.wtfpl.net/ for more details. */\n 'use strict';\n+// pkgshift live patch fixture\n module.exports = leftPad;\n \n var cache = [\n",
    );

    let applied = plan_and_apply(&root, binaries.path(), "bun");
    assert_eq!(applied["status"], "completed");
    let installed =
        fs::read_to_string(root.join("node_modules/left-pad/index.js")).expect("patched package");
    assert!(installed.contains("pkgshift live patch fixture"));
}

#[test]
#[ignore = "requires registry access and the pinned Yarn package"]
fn applies_a_pnpm_name_only_patch_after_real_yarn_migration() {
    let bun_available = Command::new("bun")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success());
    if !bun_available {
        return;
    }
    let directory = tempfile::tempdir().expect("fixture directory");
    let binaries = bunx_package_manager(&directory, "yarn", "@yarnpkg/cli-dist@4.18.0");
    let root = directory.path().join("project");
    write(
        &root.join("package.json"),
        r#"{
  "name": "live-pnpm-name-patch-fixture",
  "private": true,
  "packageManager": "pnpm@11.21.0",
  "dependencies": { "left-pad": "1.3.0" }
}
"#,
    );
    write(
        &root.join("pnpm-workspace.yaml"),
        "patchedDependencies:\n  'left-pad': 'patches/left-pad.patch'\n",
    );
    write(
        &root.join("patches/left-pad.patch"),
        "--- a/index.js\n+++ b/index.js\n@@ -4,6 +4,7 @@\n      * To Public License, Version 2, as published by Sam Hocevar. See\n      * http://www.wtfpl.net/ for more details. */\n 'use strict';\n+// pkgshift live name-only patch fixture\n module.exports = leftPad;\n \n var cache = [\n",
    );

    let applied = plan_and_apply(&root, &binaries, "yarn-modern");
    assert_eq!(applied["status"], "completed");
    let manifest: Value = serde_json::from_str(
        &fs::read_to_string(root.join("package.json")).expect("target manifest"),
    )
    .expect("target manifest JSON");
    assert_eq!(
        manifest["resolutions"]["left-pad@npm:1.3.0"],
        "patch:left-pad@npm%3A1.3.0#~/patches/left-pad.patch"
    );
    let installed =
        fs::read_to_string(root.join("node_modules/left-pad/index.js")).expect("patched package");
    assert!(installed.contains("pkgshift live name-only patch fixture"));
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
    write(
        &root.join("node_modules/.pnpm/source-marker"),
        "source dependency state\n",
    );

    let planned = run(&root, &binaries, &["to", "bun"]);
    assert_eq!(planned.status.code(), Some(7));
    let planned = json_output(&planned);
    let plan_id = planned["planId"].as_str().expect("plan identifier");
    write(
        &binaries.join("bun"),
        "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then printf '1.3.14\\n'; exit 0; fi\nprintf 'partial lock\\n' > bun.lock\nexit 42\n",
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
    assert!(!root.join("node_modules").exists());

    let rolled_back = run(&root, &binaries, &["rollback", run_id, "--approve", run_id]);
    assert!(rolled_back.status.success());
    let rolled_back = json_output(&rolled_back);
    assert_eq!(rolled_back["status"], "rolled-back");
    assert_eq!(
        rolled_back["diagnostics"][0]["code"],
        "ROLLBACK_EXTERNAL_EFFECTS_REMAIN"
    );
    assert!(root.join("pnpm-lock.yaml").is_file());
    assert!(!root.join("bun.lock").exists());
    assert!(!root.join("node_modules").exists());
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
