use super::*;

#[test]
fn compares_bun_and_deno_in_independent_isolated_trials() {
    let directory = tempfile::tempdir().expect("fixture directory");
    let binaries = fake_package_managers(&directory);
    let root = directory.path().join("project");
    let manifest = r#"{"name":"comparison-fixture","private":true,"packageManager":"pnpm@11.21.0","scripts":{"smoke":"node smoke.js"}}
"#;
    let lockfile = "lockfileVersion: '9.0'\npackages: {}\nsnapshots: {}\n";
    write(&root.join("package.json"), manifest);
    write(&root.join("pnpm-lock.yaml"), lockfile);

    let planned = run(
        &root,
        &binaries,
        &["compare", "deno", "bun", "--verify-script", "smoke"],
    );
    assert_eq!(planned.status.code(), Some(7));
    let planned = json_output(&planned);
    assert_eq!(planned["status"], "planned");
    assert_eq!(
        planned["summary"]["targets"],
        serde_json::json!(["bun", "deno"])
    );
    assert_eq!(planned["summary"]["repositoryChanged"], false);
    let comparison_id = planned["planId"].as_str().expect("comparison identifier");
    assert!(comparison_id.starts_with("plan_compare_"));
    assert_eq!(
        planned["nextActions"][0]["argv"],
        serde_json::json!([
            "pkgshift",
            "compare",
            "bun",
            "deno",
            "--verify-script",
            "smoke",
            "--approve",
            comparison_id,
            "--json",
            "--no-color",
            "--non-interactive"
        ])
    );
    let comparison_plan = artifact_content(&planned, "target-comparison-plan");
    assert_eq!(
        comparison_plan["candidates"]
            .as_array()
            .map(|candidates| candidates
                .iter()
                .map(|candidate| candidate["target"].clone())
                .collect::<Vec<_>>()),
        Some(vec![serde_json::json!("bun"), serde_json::json!("deno")])
    );

    let compared = run(
        &root,
        &binaries,
        &[
            "compare",
            "bun",
            "deno",
            "--verify-script",
            "smoke",
            "--approve",
            comparison_id,
        ],
    );
    assert!(
        compared.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&compared.stdout),
        String::from_utf8_lossy(&compared.stderr)
    );
    let compared = json_output(&compared);
    assert_eq!(compared["status"], "completed");
    assert_eq!(compared["summary"]["passedTargets"], 2);
    assert_eq!(compared["summary"]["failedTargets"], 0);
    assert_eq!(compared["summary"]["blockedTargets"], 0);
    assert_eq!(compared["summary"]["repositoryUnchanged"], true);
    let report = artifact_content(&compared, "target-comparison-report");
    let candidates = report["candidates"].as_array().expect("candidate reports");
    assert_eq!(candidates.len(), 2);
    for candidate in candidates {
        assert_eq!(candidate["status"], "passed");
        assert_eq!(candidate["trial"]["repositoryUnchanged"], true);
        let verification = &candidate["trial"]["verification"];
        let script_check = verification["checks"]
            .as_array()
            .expect("verification checks")
            .iter()
            .find(|check| check["id"] == "representative-scripts")
            .expect("representative script check");
        assert_eq!(script_check["status"], "passed");
    }
    assert_eq!(
        fs::read_to_string(root.join("package.json")).expect("source manifest"),
        manifest
    );
    assert_eq!(
        fs::read_to_string(root.join("pnpm-lock.yaml")).expect("source lockfile"),
        lockfile
    );
    assert!(!root.join("bun.lock").exists());
    assert!(!root.join("deno.lock").exists());
    assert!(!root.join(".pkgshift").exists());
    assert!(!root.join(".pkgshift-script-ran").exists());
    assert!(!compared.to_string().contains("fixture-secret-value"));
}

#[test]
fn retains_capability_blocked_candidates_without_executing_them() {
    let directory = tempfile::tempdir().expect("fixture directory");
    let binaries = fake_package_managers(&directory);
    let root = directory.path().join("project");
    let manifest = r#"{"name":"comparison-fixture","private":true,"packageManager":"bun@1.3.14","trustedDependencies":["esbuild"]}
"#;
    write(&root.join("package.json"), manifest);
    write(
        &root.join("bun.lock"),
        "{\"lockfileVersion\":1,\"packages\":{}}\n",
    );

    let planned = json_output(&run(&root, &binaries, &["compare", "pnpm", "deno"]));
    assert_eq!(planned["summary"]["executableTargets"], 1);
    assert_eq!(planned["summary"]["blockedTargets"], 1);
    let comparison_id = planned["planId"].as_str().expect("comparison identifier");
    let compared = run(
        &root,
        &binaries,
        &["compare", "pnpm", "deno", "--approve", comparison_id],
    );
    assert!(compared.status.success());
    let compared = json_output(&compared);
    assert_eq!(compared["status"], "completed");
    assert_eq!(compared["summary"]["passedTargets"], 1);
    assert_eq!(compared["summary"]["blockedTargets"], 1);
    let candidates = artifact_content(&compared, "target-comparison-report")["candidates"]
        .as_array()
        .expect("candidate reports");
    let deno = candidates
        .iter()
        .find(|candidate| candidate["target"] == "deno")
        .expect("Deno candidate");
    assert_eq!(deno["status"], "blocked");
    assert!(deno.get("trial").is_none());
    assert!(deno["diagnostics"].as_array().is_some_and(|diagnostics| {
        diagnostics.iter().any(|diagnostic| {
            diagnostic["code"] == "CAPABILITY_UNSUPPORTED" && diagnostic["blocking"] == true
        })
    }));
    assert_eq!(
        fs::read_to_string(root.join("package.json")).expect("source manifest"),
        manifest
    );
    assert!(!root.join(".pkgshift").exists());
}

#[test]
fn reports_failed_candidates_as_comparison_evidence() {
    let directory = tempfile::tempdir().expect("fixture directory");
    let binaries = fake_package_managers(&directory);
    let root = directory.path().join("project");
    write(
        &root.join("package.json"),
        r#"{"name":"comparison-fixture","private":true,"packageManager":"pnpm@11.21.0","scripts":{"fail":"exit 9"}}
"#,
    );
    write(&root.join("pnpm-lock.yaml"), "lockfileVersion: '9.0'\n");

    let planned = json_output(&run(
        &root,
        &binaries,
        &["compare", "bun", "deno", "--verify-script", "fail"],
    ));
    let comparison_id = planned["planId"].as_str().expect("comparison identifier");
    let compared = run(
        &root,
        &binaries,
        &[
            "compare",
            "bun",
            "deno",
            "--verify-script",
            "fail",
            "--approve",
            comparison_id,
        ],
    );
    assert!(compared.status.success());
    let compared = json_output(&compared);
    assert_eq!(compared["status"], "completed");
    assert_eq!(compared["summary"]["passedTargets"], 0);
    assert_eq!(compared["summary"]["failedTargets"], 2);
    assert_eq!(compared["summary"]["repositoryUnchanged"], true);
    let candidates = artifact_content(&compared, "target-comparison-report")["candidates"]
        .as_array()
        .expect("candidate reports");
    assert!(
        candidates
            .iter()
            .all(|candidate| candidate["status"] == "failed")
    );
    assert!(!root.join(".pkgshift-script-ran").exists());
}

#[test]
fn rejects_a_comparison_without_two_distinct_targets() {
    let directory = tempfile::tempdir().expect("fixture directory");
    let binaries = fake_package_managers(&directory);
    let root = directory.path().join("project");
    write(
        &root.join("package.json"),
        r#"{"name":"comparison-fixture","private":true,"packageManager":"pnpm@11.21.0"}
"#,
    );
    write(&root.join("pnpm-lock.yaml"), "lockfileVersion: '9.0'\n");

    let compared = run(&root, &binaries, &["compare", "bun", "bun"]);
    assert_eq!(compared.status.code(), Some(2));
    let compared = json_output(&compared);
    assert_eq!(compared["status"], "blocked");
    assert_eq!(
        compared["diagnostics"][0]["code"],
        "COMPARISON_TARGET_COUNT_INVALID"
    );
}

#[test]
#[ignore = "requires the real Bun executable and pinned Deno package"]
fn compares_real_bun_and_deno_trials() {
    let bun_available = Command::new("bun")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success());
    if !bun_available {
        return;
    }
    let directory = tempfile::tempdir().expect("fixture directory");
    let binaries = bunx_package_manager(&directory, "deno", "deno@2.9.5");
    let root = directory.path().join("project");
    let manifest = r#"{"name":"real-comparison-fixture","private":true,"packageManager":"pnpm@11.21.0","scripts":{"smoke":"node -e \"process.exit(0)\""}}
"#;
    write(&root.join("package.json"), manifest);
    write(
        &root.join("pnpm-lock.yaml"),
        "lockfileVersion: '9.0'\nimporters:\n  .: {}\n",
    );

    let planned = json_output(&run(
        &root,
        &binaries,
        &["compare", "bun", "deno", "--verify-script", "smoke"],
    ));
    let comparison_id = planned["planId"].as_str().expect("comparison identifier");
    let compared = run(
        &root,
        &binaries,
        &[
            "compare",
            "bun",
            "deno",
            "--verify-script",
            "smoke",
            "--approve",
            comparison_id,
        ],
    );
    assert!(
        compared.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&compared.stdout),
        String::from_utf8_lossy(&compared.stderr)
    );
    let compared = json_output(&compared);
    assert_eq!(
        compared["summary"]["passedTargets"],
        2,
        "comparison: {}",
        serde_json::to_string_pretty(&compared).expect("comparison JSON")
    );
    assert_eq!(compared["summary"]["repositoryUnchanged"], true);
    assert_eq!(
        fs::read_to_string(root.join("package.json")).expect("source manifest"),
        manifest
    );
    assert!(!root.join("bun.lock").exists());
    assert!(!root.join("deno.lock").exists());
    assert!(!root.join(".pkgshift").exists());
}
