import { afterEach, describe, expect, test } from "bun:test";
import { mkdir, symlink, writeFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import { PlanArtifactStore } from "../src/artifacts/plan-artifact-store.ts";
import type { PlanArtifactBundle } from "../src/artifacts/models.ts";
import { analyzeCapabilities } from "../src/capabilities/analyze-capabilities.ts";
import { applyPlan, ApplyFailure } from "../src/execution/apply-plan.ts";
import type {
  ProcessExecutionRecord,
  ProcessRunner,
} from "../src/execution/process-runner.ts";
import { BunProcessRunner } from "../src/execution/process-runner.ts";
import { ExecutionStore } from "../src/execution/execution-store.ts";
import { RepositoryLock } from "../src/execution/repository-lock.ts";
import { inspectProject } from "../src/inspect/inspect-project.ts";
import { buildProjectIR } from "../src/ir/build-project-ir.ts";
import { planPackageManagerMigration } from "../src/plan/plan-package-manager.ts";
import { rollbackRun } from "../src/recovery/rollback-run.ts";
import { SnapshotStore } from "../src/recovery/snapshot-store.ts";
import { verifyRun } from "../src/verification/verify-run.ts";
import { createProject, removeTemporaryProjects } from "./helpers/project.ts";

afterEach(removeTemporaryProjects);

class FixtureRunner implements ProcessRunner {
  constructor(
    private readonly exitCode: number,
    private readonly mutateSourceLock = false,
  ) {}

  async run(argv: string[], cwd: string): Promise<ProcessExecutionRecord> {
    if (this.exitCode === 0) {
      await writeFile(join(cwd, "bun.lock"), "fixture-lock\n", "utf8");
      if (this.mutateSourceLock) {
        await writeFile(join(cwd, "package-lock.json"), "changed during install\n", "utf8");
      }
    }
    return {
      argv,
      exitCode: this.exitCode,
      signal: null,
      timedOut: false,
      durationMs: 1,
      stdout: "",
      stderr: this.exitCode === 0 ? "" : "fixture install failure",
    };
  }
}

async function persistedBunPlan(): Promise<{
  root: string;
  stateDirectory: string;
  bundle: PlanArtifactBundle;
}> {
  const root = await createProject({
    "package.json": `${JSON.stringify({
      name: "fixture",
      version: "1.0.0",
      packageManager: "npm@11.0.0",
    }, null, 2)}\n`,
    "package-lock.json": "{}\n",
  });
  const inspection = await inspectProject(root);
  const projectIr = await buildProjectIR(inspection);
  const capabilityAnalysis = analyzeCapabilities(projectIr!, "bun");
  const plan = await planPackageManagerMigration(
    inspection,
    projectIr!,
    capabilityAnalysis!,
    "bun",
  );
  const bundle: PlanArtifactBundle = {
    schemaVersion: "1.0",
    plan: plan!,
    projectIr: projectIr!,
    capabilityAnalysis: capabilityAnalysis!,
  };
  const stateDirectory = join(root, "state");
  await new PlanArtifactStore(stateDirectory).save(root, bundle);
  return { root, stateDirectory, bundle };
}

describe("transaction execution", () => {
  test("redacts sensitive environment values from process records", async () => {
    process.env.PKGSHIFT_FIXTURE_SECRET_TOKEN = "sensitive-fixture-value";
    try {
      const record = await new BunProcessRunner().run([
        process.execPath,
        "-e",
        "console.log(process.env.PKGSHIFT_FIXTURE_SECRET_TOKEN)",
      ], process.cwd());
      expect(record.exitCode).toBe(0);
      expect(record.stdout).toContain("<redacted>");
      expect(record.stdout).not.toContain("sensitive-fixture-value");
    } finally {
      delete process.env.PKGSHIFT_FIXTURE_SECRET_TOKEN;
    }
  });

  test("applies, verifies, and rolls back an immutable plan", async () => {
    const { root, stateDirectory, bundle } = await persistedBunPlan();
    const applied = await applyPlan({
      projectRoot: root,
      stateDirectory,
      planId: bundle.plan.planId,
      approval: bundle.plan.planId,
      runId: "run_success123",
    }, new FixtureRunner(0));

    expect(applied.journal.status).toBe("verifying");
    expect((await new ExecutionStore(stateDirectory).load(applied.journal.runId)).records).toHaveLength(2);
    expect((await Bun.file(join(root, "package.json")).json()).packageManager).toBe("bun@1.3.14");
    expect(await Bun.file(join(root, "package-lock.json")).exists()).toBeFalse();
    expect(await Bun.file(join(root, "bun.lock")).exists()).toBeTrue();

    const verified = await verifyRun({
      projectRoot: root,
      stateDirectory,
      runId: applied.journal.runId,
    });
    expect(verified.report.status).toBe("passed");
    expect(verified.journal.status).toBe("succeeded");

    const rolledBack = await rollbackRun({
      projectRoot: root,
      stateDirectory,
      runId: applied.journal.runId,
      approval: applied.journal.runId,
    });
    expect(rolledBack.journal.status).toBe("rolled-back");
    expect((await Bun.file(join(root, "package.json")).json()).packageManager).toBe("npm@11.0.0");
    expect(await Bun.file(join(root, "package-lock.json")).exists()).toBeTrue();
    expect(await Bun.file(join(root, "bun.lock")).exists()).toBeFalse();
  });

  test("keeps a failed install recoverable through the same snapshot", async () => {
    const { root, stateDirectory, bundle } = await persistedBunPlan();
    const applied = await applyPlan({
      projectRoot: root,
      stateDirectory,
      planId: bundle.plan.planId,
      approval: bundle.plan.planId,
      runId: "run_failure123",
    }, new FixtureRunner(1));

    expect(applied.journal.status).toBe("failed");
    expect(applied.diagnostics[0]?.code).toBe("INSTALL_COMMAND_FAILED");
    expect((await Bun.file(join(root, "package.json")).json()).packageManager).toBe("bun@1.3.14");

    const rolledBack = await rollbackRun({
      projectRoot: root,
      stateDirectory,
      runId: applied.journal.runId,
      approval: applied.journal.runId,
    });
    expect(rolledBack.journal.status).toBe("rolled-back");
    expect((await Bun.file(join(root, "package.json")).json()).packageManager).toBe("npm@11.0.0");
  });

  test("rejects missing approval and repository drift before creating a run", async () => {
    const { root, stateDirectory, bundle } = await persistedBunPlan();
    await expect(applyPlan({
      projectRoot: root,
      stateDirectory,
      planId: bundle.plan.planId,
      approval: null,
    }, new FixtureRunner(0))).rejects.toBeInstanceOf(ApplyFailure);

    const manifest = await Bun.file(join(root, "package.json")).json();
    manifest.description = "changed after planning";
    await writeFile(join(root, "package.json"), `${JSON.stringify(manifest, null, 2)}\n`, "utf8");
    await expect(applyPlan({
      projectRoot: root,
      stateDirectory,
      planId: bundle.plan.planId,
      approval: bundle.plan.planId,
    }, new FixtureRunner(0))).rejects.toMatchObject({
      diagnostic: { code: "PLAN_PRECONDITION_FAILED" },
    });
  });

  test("stops on a mid-run precondition conflict and restores partial changes", async () => {
    const { root, stateDirectory, bundle } = await persistedBunPlan();
    const applied = await applyPlan({
      projectRoot: root,
      stateDirectory,
      planId: bundle.plan.planId,
      approval: bundle.plan.planId,
      runId: "run_partial123",
    }, new FixtureRunner(0, true));

    expect(applied.journal.status).toBe("failed");
    expect(applied.diagnostics[0]?.code).toBe("PLAN_PRECONDITION_FAILED");
    const rolledBack = await rollbackRun({
      projectRoot: root,
      stateDirectory,
      runId: applied.journal.runId,
      approval: applied.journal.runId,
    });
    expect(rolledBack.journal.status).toBe("rolled-back");
    expect(await Bun.file(join(root, "package-lock.json")).text()).toBe("{}\n");
  });

  test("rejects a tampered recovery backup", async () => {
    const { root, stateDirectory, bundle } = await persistedBunPlan();
    const applied = await applyPlan({
      projectRoot: root,
      stateDirectory,
      planId: bundle.plan.planId,
      approval: bundle.plan.planId,
      runId: "run_tamper123",
    }, new FixtureRunner(0));
    const snapshot = await new SnapshotStore(stateDirectory).load(applied.journal.runId);
    const backup = snapshot.entries.find((entry) => entry.backupPath)?.backupPath;
    await writeFile(join(stateDirectory, "runs", applied.journal.runId, backup!), "tampered", "utf8");

    const rolledBack = await rollbackRun({
      projectRoot: root,
      stateDirectory,
      runId: applied.journal.runId,
      approval: applied.journal.runId,
    });
    expect(rolledBack.journal.status).toBe("rollback-failed");
    expect(rolledBack.diagnostics[0]?.code).toBe("ROLLBACK_FAILED");
  });

  test("rejects snapshot paths that traverse a symbolic-link directory", async () => {
    const root = await createProject({ "package.json": "{}\n" });
    const outside = await createProject({ "escape.txt": "outside\n" });
    await symlink(outside, join(root, "linked"), "dir");

    await expect(new SnapshotStore(join(root, "state")).create(
      "run_symlink123",
      root,
      ["linked/escape.txt"],
    )).rejects.toMatchObject({ code: "SNAPSHOT_PATH_UNSAFE" });
    expect(await Bun.file(join(outside, "escape.txt")).text()).toBe("outside\n");
  });

  test("serializes migration transactions for the same repository", async () => {
    const { root, stateDirectory, bundle } = await persistedBunPlan();
    let signalEntered!: () => void;
    let signalResume!: () => void;
    const entered = new Promise<void>((resolve) => { signalEntered = resolve; });
    const resume = new Promise<void>((resolve) => { signalResume = resolve; });
    const runner: ProcessRunner = {
      async run(argv, cwd) {
        signalEntered();
        await resume;
        await writeFile(join(cwd, "bun.lock"), "fixture-lock\n", "utf8");
        return {
          argv,
          exitCode: 0,
          signal: null,
          timedOut: false,
          durationMs: 1,
          stdout: "",
          stderr: "",
        };
      },
    };
    const first = applyPlan({
      projectRoot: root,
      stateDirectory,
      planId: bundle.plan.planId,
      approval: bundle.plan.planId,
      runId: "run_concurrent1",
    }, runner);
    await entered;
    try {
      await expect(applyPlan({
        projectRoot: root,
        stateDirectory,
        planId: bundle.plan.planId,
        approval: bundle.plan.planId,
        runId: "run_concurrent2",
      }, new FixtureRunner(0))).rejects.toMatchObject({
        diagnostic: { code: "REPOSITORY_TRANSACTION_BUSY" },
      });
    } finally {
      signalResume();
    }
    const applied = await first;
    expect(applied.journal.status).toBe("verifying");
    const rolledBack = await rollbackRun({
      projectRoot: root,
      stateDirectory,
      runId: applied.journal.runId,
      approval: applied.journal.runId,
    });
    expect(rolledBack.journal.status).toBe("rolled-back");
  });

  test("recovers a repository transaction lock owned by a dead process", async () => {
    const { root, stateDirectory } = await persistedBunPlan();
    const repositoryKey = new PlanArtifactStore(stateDirectory).repositoryKey(root);
    const lockPath = join(stateDirectory, "repositories", repositoryKey, "transaction.lock");
    await mkdir(dirname(lockPath), { recursive: true });
    await writeFile(lockPath, `${JSON.stringify({ pid: 999_999_999, operation: "apply" })}\n`, "utf8");

    const lock = await RepositoryLock.acquire({
      stateDirectory,
      projectRoot: root,
      operation: "verify",
    });
    await lock.release();
    expect(await Bun.file(lockPath).exists()).toBeFalse();
  });
});
