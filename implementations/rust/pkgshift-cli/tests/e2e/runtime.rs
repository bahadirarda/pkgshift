use super::*;

fn runtime_fixture(root: &Path) {
    write(
        &root.join("package.json"),
        r#"{
  "name": "hono-bun-runtime-fixture",
  "private": true,
  "packageManager": "bun@1.3.14",
  "scripts": {
    "dev": "bun run --hot src/index.ts",
    "test": "bun test"
  },
  "dependencies": {
    "hono": "4.9.2"
  },
  "devDependencies": {
    "@types/bun": "latest"
  }
}
"#,
    );
    write(
        &root.join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "strict": true,
    "types": ["bun-types"]
  }
}
"#,
    );
    write(
        &root.join("src/index.ts"),
        r#"import { Hono } from "hono";

const app = new Hono();
app.get("/", (context) => context.text("fixture-secret-value"));

Bun.serve({ port: 3000, fetch: app.fetch });
"#,
    );
    write(
        &root.join("src/index.test.ts"),
        r#"import { describe, it, expect } from "bun:test";

describe("runtime", () => {
  it("responds", () => expect(true).toBe(true));
});
"#,
    );
}

#[test]
fn migrates_hono_bun_runtime_to_deno_and_rolls_back() {
    let directory = tempfile::tempdir().expect("fixture directory");
    let binaries = fake_package_managers(&directory);
    let root = directory.path().join("project");
    runtime_fixture(&root);
    let original_manifest = fs::read_to_string(root.join("package.json")).expect("manifest");
    let original_source = fs::read_to_string(root.join("src/index.ts")).expect("source");

    let missing_permission = run(&root, &binaries, &["runtime", "to", "deno"]);
    assert_eq!(missing_permission.status.code(), Some(3));
    assert!(
        json_output(&missing_permission)["diagnostics"]
            .as_array()
            .is_some_and(|diagnostics| diagnostics
                .iter()
                .any(|diagnostic| diagnostic["code"] == "DENO_PERMISSION_REQUIRED"))
    );

    let dry_run = run(
        &root,
        &binaries,
        &[
            "runtime",
            "to",
            "deno",
            "--deno-permission",
            "net",
            "--dry-run",
        ],
    );
    assert!(dry_run.status.success());
    let dry_run = json_output(&dry_run);
    assert_eq!(dry_run["status"], "planned");
    assert_eq!(dry_run["nextActions"], serde_json::json!([]));
    assert!(!root.join(".pkgshift").exists());

    let planned = run(
        &root,
        &binaries,
        &["runtime", "to", "deno", "--deno-permission", "net"],
    );
    assert_eq!(planned.status.code(), Some(7));
    let planned = json_output(&planned);
    let plan_id = planned["planId"].as_str().expect("runtime plan identifier");
    assert!(plan_id.starts_with("runtime_plan_"));
    assert_eq!(planned["summary"]["repositoryChanged"], false);
    assert!(!root.join(".pkgshift").exists());
    assert_eq!(
        planned["nextActions"][0]["argv"],
        serde_json::json!([
            "pkgshift",
            "runtime",
            "to",
            "deno",
            "--deno-permission",
            "net",
            "--approve",
            plan_id,
            "--json",
            "--no-color",
            "--non-interactive"
        ])
    );
    let plan = artifact_content(&planned, "runtime-migration-plan");
    assert_eq!(plan["source"], "bun");
    assert_eq!(plan["target"], "deno");
    assert!(plan.to_string().contains("bun.serve-to-deno.serve"));
    assert!(!planned.to_string().contains("fixture-secret-value"));
    assert_eq!(
        fs::read_to_string(root.join("src/index.ts")).expect("unchanged source"),
        original_source
    );

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
    assert_eq!(applied["status"], "completed");
    assert_eq!(applied["summary"]["runStatus"], "succeeded");
    assert!(!applied.to_string().contains("fixture-secret-value"));
    let verification = artifact_content(&applied, "runtime-verification-report");
    assert_eq!(verification["status"], "passed");

    let source = fs::read_to_string(root.join("src/index.ts")).expect("migrated source");
    assert!(source.contains("Deno.serve({ port: 3000 }, app.fetch)"));
    assert!(!source.contains("Bun."));
    let test = fs::read_to_string(root.join("src/index.test.ts")).expect("migrated test");
    assert!(test.contains("from \"node:test\""));
    assert!(test.contains("from \"jsr:@std/expect\""));
    assert!(!test.contains("bun:test"));
    let manifest = fs::read_to_string(root.join("package.json")).expect("migrated manifest");
    assert!(manifest.contains("deno run --watch --allow-net src/index.ts"));
    assert!(manifest.contains("deno test --allow-net"));
    assert!(manifest.contains("\"packageManager\": \"bun@1.3.14\""));
    assert!(!manifest.contains("@types/bun"));
    assert!(
        !fs::read_to_string(root.join("tsconfig.json"))
            .expect("migrated tsconfig")
            .contains("bun-types")
    );

    let run_id = applied["runId"].as_str().expect("runtime run identifier");
    assert!(run_id.starts_with("runtime_run_"));
    let run_path = root
        .join(".pkgshift/state/runtime/runs")
        .join(run_id)
        .join("run.json");
    let persisted_run = fs::read_to_string(&run_path).expect("persisted runtime run");
    assert!(!persisted_run.contains("fixture-secret-value"));
    assert!(!persisted_run.contains("const app = new Hono"));
    assert_eq!(
        fs::metadata(&run_path)
            .expect("runtime run metadata")
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    let snapshot = root
        .join(".pkgshift/state/runtime/runs")
        .join(run_id)
        .join("snapshots/0001.bin");
    assert_eq!(
        fs::metadata(snapshot)
            .expect("runtime snapshot metadata")
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    let rolled_back = run(
        &root,
        &binaries,
        &["runtime", "rollback", run_id, "--approve", run_id],
    );
    assert!(rolled_back.status.success());
    assert_eq!(json_output(&rolled_back)["status"], "rolled-back");
    assert_eq!(
        fs::read_to_string(root.join("package.json")).expect("restored manifest"),
        original_manifest
    );
    assert_eq!(
        fs::read_to_string(root.join("src/index.ts")).expect("restored source"),
        original_source
    );
}

#[test]
fn blocks_bun_routes_without_mutating_the_repository() {
    let directory = tempfile::tempdir().expect("fixture directory");
    let binaries = fake_package_managers(&directory);
    let root = directory.path().join("project");
    write(
        &root.join("src/index.ts"),
        "Bun.serve({ routes: { '/': new Response('ok') }, fetch: app.fetch });\n",
    );
    let original = fs::read_to_string(root.join("src/index.ts")).expect("source");
    let planned = run(
        &root,
        &binaries,
        &["runtime", "to", "deno", "--deno-permission", "net"],
    );
    assert_eq!(planned.status.code(), Some(3));
    let planned = json_output(&planned);
    assert_eq!(planned["status"], "blocked");
    assert!(
        planned["diagnostics"]
            .as_array()
            .is_some_and(|diagnostics| diagnostics
                .iter()
                .any(|diagnostic| diagnostic["code"] == "RUNTIME_BUN_SERVE_UNSUPPORTED"))
    );
    assert_eq!(
        fs::read_to_string(root.join("src/index.ts")).expect("unchanged source"),
        original
    );
    assert!(!root.join(".pkgshift").exists());
}

#[test]
#[ignore = "requires the real Bun executable and pinned Deno package"]
fn migrates_a_real_hono_bun_project_and_runs_it_with_deno() {
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
    write(
        &root.join("package.json"),
        r#"{
  "name": "real-hono-runtime-fixture",
  "private": true,
  "packageManager": "bun@1.3.14",
  "scripts": {
    "dev": "bun run --hot src/server.ts",
    "test": "bun test"
  },
  "dependencies": {
    "hono": "4.9.2"
  },
  "devDependencies": {
    "@types/bun": "1.2.20",
    "@types/node": "22.17.2"
  }
}
"#,
    );
    write(
        &root.join("src/app.ts"),
        r#"import { Hono } from "hono";

export const app = new Hono();
app.get("/", (context) => context.json({ runtime: "deno" }));
"#,
    );
    write(
        &root.join("src/server.ts"),
        r#"import { Hono } from "hono";

const app = new Hono();
app.get("/", (context) => context.json({ runtime: "deno" }));
Bun.serve({ port: 3000, fetch: app.fetch });
"#,
    );
    write(
        &root.join("src/app.test.ts"),
        r#"import { describe, it } from "bun:test";
import assert from "node:assert";
import { app } from "./app.ts";

describe("Hono on Deno", () => {
  it("handles a request", async () => {
    const response = await app.request("/");
    assert.strictEqual(response.status, 200);
    assert.deepStrictEqual(await response.json(), { runtime: "deno" });
  });
});
"#,
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

    for arguments in [
        ["deno@2.9.5", "install", "", ""],
        ["deno@2.9.5", "check", "src/server.ts", ""],
        ["deno@2.9.5", "test", "--allow-net", ""],
    ] {
        let output = Command::new("bunx")
            .arg("--bun")
            .args(arguments.into_iter().filter(|value| !value.is_empty()))
            .current_dir(&root)
            .output()
            .expect("pinned Deno command");
        assert!(
            output.status.success(),
            "command: bunx --bun {}\nstdout: {}\nstderr: {}",
            arguments.join(" "),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    assert!(root.join("deno.lock").is_file());
    assert!(
        !fs::read_to_string(root.join("src/server.ts"))
            .expect("migrated server")
            .contains("Bun.")
    );
}
