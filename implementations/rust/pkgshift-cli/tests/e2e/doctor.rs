use super::*;

fn readiness(result: &Value) -> &Value {
    artifact_content(result, "migration-readiness")
}

#[test]
fn reports_deterministic_readiness_and_repository_effects_without_writes() {
    let directory = tempfile::tempdir().expect("fixture directory");
    let binaries = fake_package_managers(&directory);
    let root = directory.path().join("project");
    write(
        &root.join("package.json"),
        r#"{
  "name": "doctor-ready-fixture",
  "private": true,
  "packageManager": "pnpm@11.21.0",
  "workspaces": ["packages/*"],
  "scripts": { "check": "node check.js" }
}
"#,
    );
    write(
        &root.join("packages/app/package.json"),
        r#"{"name":"@fixture/app","private":true}"#,
    );
    write(
        &root.join("pnpm-lock.yaml"),
        "lockfileVersion: '9.0'\npackages: {}\nsnapshots: {}\n",
    );
    write(
        &root.join("pnpm-workspace.yaml"),
        "packages:\n  - 'packages/*'\n",
    );
    write(
        &root.join(".github/workflows/ci.yml"),
        "steps:\n  - run: pnpm install\n",
    );
    write(&root.join("Dockerfile"), "RUN pnpm install\n");
    write(&root.join("README.md"), "Run `pnpm install`.\n");
    write(&root.join("node_modules/.pnpm/source"), "preserve\n");

    let first = run(
        &root,
        &binaries,
        &["doctor", "--to", "bun", "--verify-script", "check"],
    );
    assert!(
        first.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&first.stdout),
        String::from_utf8_lossy(&first.stderr)
    );
    let first = json_output(&first);
    assert_eq!(first["command"], "doctor");
    assert_eq!(first["status"], "completed");
    assert_eq!(first["planId"], Value::Null);
    assert_eq!(first["runId"], Value::Null);
    assert_eq!(first["summary"]["verdict"], "ready");
    assert_eq!(first["summary"]["migrationAvailable"], true);
    assert_eq!(first["summary"]["repositoryChanged"], false);
    assert!(
        first["artifacts"]
            .as_array()
            .expect("artifacts")
            .iter()
            .all(|artifact| artifact["type"] != "package-manager-plan")
    );
    let report = readiness(&first);
    assert!(
        report["reportId"]
            .as_str()
            .is_some_and(|value| value.starts_with("doctor_"))
    );
    assert_eq!(report["readOnly"], true);
    assert_eq!(report["source"], "pnpm");
    assert_eq!(report["target"], "bun");
    assert_eq!(report["packageCount"], 2);
    assert_eq!(report["workspaceConfigured"], true);
    assert_eq!(
        report["integrations"]["ci"],
        serde_json::json!([".github/workflows/ci.yml"])
    );
    assert_eq!(
        report["integrations"]["containers"],
        serde_json::json!(["Dockerfile"])
    );
    assert_eq!(
        report["integrations"]["documentation"],
        serde_json::json!(["README.md"])
    );
    assert_eq!(
        report["effects"]["dependencyStateCleanups"],
        serde_json::json!(["node_modules", "packages/app/node_modules"])
    );
    assert_eq!(
        report["effects"]["sourceArtifactRetirements"],
        serde_json::json!(["pnpm-lock.yaml", "pnpm-workspace.yaml"])
    );
    assert!(
        report["effects"]["fileWrites"]
            .as_array()
            .expect("file writes")
            .contains(&serde_json::json!("package.json"))
    );
    assert!(
        report["effects"]["processCommands"]
            .as_array()
            .expect("process commands")
            .contains(&serde_json::json!(["bun", "run", "check"]))
    );
    assert_eq!(
        first["nextActions"][0]["argv"],
        serde_json::json!([
            "pkgshift",
            "plan",
            "package-manager",
            "--to",
            "bun",
            "--verify-script",
            "check",
            "--json",
            "--no-color",
            "--non-interactive"
        ])
    );
    assert_eq!(first["nextActions"][0]["sideEffect"], "none");
    assert_eq!(first["nextActions"][0]["requiresApproval"], false);

    let second = json_output(&run(
        &root,
        &binaries,
        &["doctor", "--to", "bun", "--verify-script", "check"],
    ));
    assert_eq!(
        readiness(&first)["reportId"],
        readiness(&second)["reportId"]
    );
    assert!(root.join("pnpm-lock.yaml").is_file());
    assert!(root.join("pnpm-workspace.yaml").is_file());
    assert!(root.join("node_modules/.pnpm/source").is_file());
    assert!(!root.join("bun.lock").exists());
    assert!(!root.join(".pkgshift").exists());
}

#[test]
fn reports_capability_blockers_without_exposing_a_plan() {
    let directory = tempfile::tempdir().expect("fixture directory");
    let binaries = fake_package_managers(&directory);
    let root = directory.path().join("project");
    write(
        &root.join("package.json"),
        r#"{
  "name": "doctor-blocked-fixture",
  "private": true,
  "packageManager": "pnpm@11.21.0",
  "patchedDependencies": { "left-pad@1.3.0": "patches/left-pad.patch" }
}
"#,
    );
    write(
        &root.join("patches/left-pad.patch"),
        "diff --git a/index.js b/index.js\n--- a/index.js\n+++ b/index.js\n@@ -1 +1 @@\n-old\n+new\n",
    );
    write(
        &root.join("pnpm-lock.yaml"),
        "lockfileVersion: '9.0'\npackages: {}\nsnapshots: {}\n",
    );

    let output = run(&root, &binaries, &["doctor", "--to", "vlt"]);
    assert_eq!(output.status.code(), Some(3));
    let output = json_output(&output);
    assert_eq!(output["status"], "blocked");
    assert_eq!(output["summary"]["verdict"], "blocked");
    assert_eq!(output["summary"]["migrationAvailable"], false);
    assert!(
        output["nextActions"]
            .as_array()
            .expect("next actions")
            .is_empty()
    );
    assert!(
        output["diagnostics"]
            .as_array()
            .expect("diagnostics")
            .iter()
            .any(|diagnostic| diagnostic["code"] == "CAPABILITY_UNSUPPORTED")
    );
    assert_eq!(readiness(&output)["capabilities"]["unsupported"], 1);
    assert!(
        output["artifacts"]
            .as_array()
            .expect("artifacts")
            .iter()
            .all(|artifact| artifact["type"] != "package-manager-plan")
    );
    assert!(root.join("pnpm-lock.yaml").is_file());
    assert!(!root.join("vlt-lock.json").exists());
}

#[test]
fn distinguishes_lossy_review_from_hard_blockers() {
    let directory = tempfile::tempdir().expect("fixture directory");
    let binaries = fake_package_managers(&directory);
    let root = directory.path().join("project");
    write(
        &root.join("package.json"),
        r#"{
  "name": "doctor-lossy-fixture",
  "private": true,
  "packageManager": "bun@1.3.14",
  "catalog": { "react": "^19.0.0" },
  "dependencies": { "react": "catalog:" }
}
"#,
    );
    write(
        &root.join("bun.lock"),
        "{\"lockfileVersion\":1,\"packages\":{}}\n",
    );

    let review = run(&root, &binaries, &["doctor", "--to", "npm"]);
    assert_eq!(review.status.code(), Some(3));
    let review = json_output(&review);
    assert_eq!(review["summary"]["verdict"], "review-required");
    assert_eq!(review["summary"]["migrationAvailable"], false);
    assert_eq!(review["summary"]["availableAfterReview"], true);
    assert_eq!(
        review["nextActions"][0]["argv"],
        serde_json::json!([
            "pkgshift",
            "plan",
            "package-manager",
            "--to",
            "npm",
            "--accept-lossy",
            "--json",
            "--no-color",
            "--non-interactive"
        ])
    );

    let accepted = run(
        &root,
        &binaries,
        &["doctor", "--to", "npm", "--accept-lossy"],
    );
    assert!(accepted.status.success());
    let accepted = json_output(&accepted);
    assert_eq!(accepted["summary"]["verdict"], "review-required");
    assert_eq!(accepted["summary"]["migrationAvailable"], true);
    assert_eq!(accepted["summary"]["availableAfterReview"], false);
    assert_eq!(readiness(&accepted)["acceptedLossy"], true);
    assert_eq!(readiness(&accepted)["capabilities"]["lossy"], 2);
    assert!(root.join("bun.lock").is_file());
    assert!(!root.join("package-lock.json").exists());
}
