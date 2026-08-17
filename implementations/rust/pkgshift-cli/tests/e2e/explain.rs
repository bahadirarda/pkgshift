use super::*;

fn package_fixture(root: &Path) {
    write(
        &root.join("package.json"),
        r#"{"name":"explain-fixture","private":true,"packageManager":"pnpm@11.21.0"}
"#,
    );
    write(&root.join("pnpm-lock.yaml"), "lockfileVersion: '9.0'\n");
}

#[test]
fn explains_diagnostics_and_integrity_checked_package_artifacts() {
    let directory = tempfile::tempdir().expect("fixture directory");
    let binaries = fake_package_managers(&directory);
    let root = directory.path().join("project");
    package_fixture(&root);

    let diagnostic = run(
        &root,
        &binaries,
        &["explain", "RUNTIME_BUN_RESIDUE_REMAINS"],
    );
    assert!(diagnostic.status.success());
    let diagnostic = json_output(&diagnostic);
    assert_eq!(diagnostic["status"], "completed");
    assert_eq!(diagnostic["summary"]["readOnly"], true);
    assert_eq!(
        artifact_content(&diagnostic, "diagnostic-explanation")["code"],
        "RUNTIME_BUN_RESIDUE_REMAINS"
    );

    let unknown = run(&root, &binaries, &["explain", "NOT_A_REAL_DIAGNOSTIC"]);
    assert_eq!(unknown.status.code(), Some(2));
    assert_eq!(
        json_output(&unknown)["diagnostics"][0]["code"],
        "DIAGNOSTIC_CODE_UNKNOWN"
    );

    let planned = run(
        &root,
        &binaries,
        &[
            "plan",
            "package-manager",
            "--to",
            "bun",
            "--state-dir",
            ".pkgshift/state",
        ],
    );
    assert!(planned.status.success());
    let planned = json_output(&planned);
    let plan_id = planned["planId"].as_str().expect("plan identifier");

    let explained_plan = run(&root, &binaries, &["explain", plan_id]);
    assert!(explained_plan.status.success());
    let explained_plan = json_output(&explained_plan);
    assert_eq!(
        explained_plan["summary"]["type"],
        "package-manager-plan-bundle"
    );
    assert_eq!(explained_plan["nextActions"], serde_json::json!([]));
    assert_eq!(
        artifact_content(&explained_plan, "package-manager-plan-bundle")["plan"]["planId"],
        plan_id
    );

    let applied = run(&root, &binaries, &["apply", plan_id, "--approve", plan_id]);
    assert!(
        applied.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&applied.stdout),
        String::from_utf8_lossy(&applied.stderr)
    );
    let applied = json_output(&applied);
    let run_id = applied["runId"].as_str().expect("run identifier");
    let verification_id = artifact_content(&applied, "verification-report")["reportId"]
        .as_str()
        .expect("verification identifier");

    let explained_run = json_output(&run(&root, &binaries, &["explain", run_id]));
    assert_eq!(explained_run["planId"], plan_id);
    assert_eq!(explained_run["runId"], run_id);
    assert!(
        explained_run["artifacts"]
            .as_array()
            .is_some_and(|artifacts| artifacts.len() == 2)
    );

    let explained_verification = run(&root, &binaries, &["explain", verification_id]);
    assert!(explained_verification.status.success());
    assert_eq!(
        json_output(&explained_verification)["summary"]["type"],
        "verification-report"
    );

    let verification_path = root
        .join(".pkgshift/state/runs")
        .join(run_id)
        .join("verification.json");
    let mut tampered: Value = serde_json::from_str(
        &fs::read_to_string(&verification_path).expect("verification artifact"),
    )
    .expect("verification JSON");
    tampered["status"] = serde_json::json!("failed");
    write(
        &verification_path,
        &serde_json::to_string_pretty(&tampered).expect("tampered report"),
    );
    let rejected = run(&root, &binaries, &["explain", verification_id]);
    assert_eq!(rejected.status.code(), Some(4));
    assert_eq!(
        json_output(&rejected)["diagnostics"][0]["code"],
        "ARTIFACT_INVALID"
    );

    let traversal = run(&root, &binaries, &["explain", "plan_../../etc/passwd"]);
    assert_eq!(traversal.status.code(), Some(4));
    assert_eq!(
        json_output(&traversal)["diagnostics"][0]["code"],
        "ARTIFACT_NOT_FOUND"
    );
}

#[test]
fn explains_runtime_plan_run_and_verification_from_one_stored_run() {
    let directory = tempfile::tempdir().expect("fixture directory");
    let binaries = fake_package_managers(&directory);
    let root = directory.path().join("project");
    write(
        &root.join("package.json"),
        r#"{"name":"runtime-explain-fixture","private":true,"packageManager":"bun@1.3.14"}
"#,
    );
    write(
        &root.join("src/index.ts"),
        "Bun.serve({ port: 3000, fetch(request) { return new Response(request.url); } });\n",
    );

    let planned = json_output(&run(
        &root,
        &binaries,
        &["runtime", "to", "deno", "--deno-permission", "net"],
    ));
    let plan_id = planned["planId"].as_str().expect("runtime plan identifier");
    let applied = run(
        &root,
        &binaries,
        &[
            "runtime",
            "to",
            "deno",
            "--deno-permission",
            "net",
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
    let run_id = applied["runId"].as_str().expect("runtime run identifier");
    let verification_id = artifact_content(&applied, "runtime-verification-report")["reportId"]
        .as_str()
        .expect("runtime verification identifier");

    for (identifier, expected_type) in [
        (plan_id, "runtime-migration-plan"),
        (run_id, "runtime-run"),
        (verification_id, "runtime-verification-report"),
    ] {
        let explained = run(&root, &binaries, &["explain", identifier]);
        assert!(
            explained.status.success(),
            "stdout: {}\nstderr: {}",
            String::from_utf8_lossy(&explained.stdout),
            String::from_utf8_lossy(&explained.stderr)
        );
        let explained = json_output(&explained);
        assert_eq!(explained["summary"]["type"], expected_type);
        assert_eq!(explained["summary"]["readOnly"], true);
        assert_eq!(explained["nextActions"], serde_json::json!([]));
    }
}
