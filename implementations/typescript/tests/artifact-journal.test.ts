import { afterEach, describe, expect, test } from "bun:test";
import { writeFile } from "node:fs/promises";
import { join } from "node:path";
import { PlanArtifactStore } from "../src/artifacts/plan-artifact-store.ts";
import type { PlanArtifactBundle } from "../src/artifacts/models.ts";
import { analyzeCapabilities } from "../src/capabilities/analyze-capabilities.ts";
import { inspectProject } from "../src/inspect/inspect-project.ts";
import { buildProjectIR } from "../src/ir/build-project-ir.ts";
import { JournalStore } from "../src/journal/journal-store.ts";
import { createRunJournal } from "../src/journal/models.ts";
import {
  JournalTransitionError,
  transitionOperation,
  transitionRun,
} from "../src/journal/transitions.ts";
import { planPackageManagerMigration } from "../src/plan/plan-package-manager.ts";
import { createProject, removeTemporaryProjects } from "./helpers/project.ts";

afterEach(removeTemporaryProjects);

async function createPlanBundle(): Promise<{
  root: string;
  bundle: PlanArtifactBundle;
}> {
  const root = await createProject({
    "package.json": JSON.stringify({
      name: "fixture",
      packageManager: "npm@11.0.0",
    }),
    "package-lock.json": "{}",
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
  return {
    root,
    bundle: {
      schemaVersion: "1.0",
      plan: plan!,
      projectIr: projectIr!,
      capabilityAnalysis: capabilityAnalysis!,
    },
  };
}

describe("artifact persistence", () => {
  test("atomically persists and verifies an immutable plan bundle", async () => {
    const { root, bundle } = await createPlanBundle();
    const stateDirectory = join(root, "state");
    const store = new PlanArtifactStore(stateDirectory);

    const first = await store.save(root, bundle, "2026-08-15T16:00:00Z");
    const second = await store.save(root, bundle, "2026-08-15T16:01:00Z");
    const loaded = await store.load(root, bundle.plan.planId);

    expect(first.digest).toBe(second.digest);
    expect(first.relativePath).toEndWith(`${bundle.plan.planId}.json`);
    expect(loaded.content.plan.planId).toBe(bundle.plan.planId);
    expect(loaded.content.projectIr.projectIrId).toBe(bundle.projectIr.projectIrId);
    expect(loaded.digest).toBe(first.digest);
  });

  test("rejects a plan artifact whose content no longer matches its digest", async () => {
    const { root, bundle } = await createPlanBundle();
    const stateDirectory = join(root, "state");
    const store = new PlanArtifactStore(stateDirectory);
    const reference = await store.save(root, bundle, "2026-08-15T16:00:00Z");
    const path = join(stateDirectory, reference.relativePath);
    const envelope = await Bun.file(path).json();
    envelope.content.plan.target = "pnpm";
    await writeFile(path, `${JSON.stringify(envelope, null, 2)}\n`, "utf8");

    await expect(store.load(root, bundle.plan.planId)).rejects.toMatchObject({
      code: "ARTIFACT_INTEGRITY_FAILED",
    });
  });

  test("rejects a plan artifact that is not valid JSON", async () => {
    const { root, bundle } = await createPlanBundle();
    const stateDirectory = join(root, "state");
    const store = new PlanArtifactStore(stateDirectory);
    const reference = await store.save(root, bundle, "2026-08-15T16:00:00Z");
    await writeFile(join(stateDirectory, reference.relativePath), "{invalid", "utf8");

    await expect(store.load(root, bundle.plan.planId)).rejects.toMatchObject({
      code: "ARTIFACT_INTEGRITY_FAILED",
    });
  });
});

describe("run journal", () => {
  test("persists valid run and operation transitions with revision checks", async () => {
    const { root, bundle } = await createPlanBundle();
    const store = new JournalStore(join(root, "state"));
    const initial = createRunJournal(bundle.plan, {
      runId: "run_abc123",
      at: "2026-08-15T16:00:00Z",
    });
    await store.create(initial);

    const applying = transitionRun(
      initial,
      "applying",
      "2026-08-15T16:00:01Z",
      "Approved plan execution started.",
    );
    await store.update(applying, 0);
    const running = transitionOperation(
      applying,
      applying.operations[0]!.operationId,
      "running",
      "2026-08-15T16:00:02Z",
      "Operation preconditions passed.",
      ["backup_manifest_abc123"],
    );
    await store.update(running, 1);
    const loaded = await store.load(initial.runId);

    expect(loaded.revision).toBe(2);
    expect(loaded.status).toBe("applying");
    expect(loaded.operations[0]?.status).toBe("running");
    expect(loaded.operations[0]?.attempts).toBe(1);
    expect(loaded.operations[0]?.recoveryReferences).toEqual([
      "backup_manifest_abc123",
    ]);
    expect(loaded.events.map((event) => event.sequence)).toEqual([1, 2, 3]);
  });

  test("rejects stale journal revisions and invalid transitions", async () => {
    const { root, bundle } = await createPlanBundle();
    const store = new JournalStore(join(root, "state"));
    const initial = createRunJournal(bundle.plan, {
      runId: "run_stale123",
      at: "2026-08-15T16:00:00Z",
    });
    await store.create(initial);
    const applying = transitionRun(
      initial,
      "applying",
      "2026-08-15T16:00:01Z",
      "Execution started.",
    );
    await store.update(applying, 0);

    await expect(store.update(applying, 0)).rejects.toMatchObject({
      code: "JOURNAL_REVISION_CONFLICT",
    });
    expect(() => transitionRun(
      applying,
      "succeeded",
      "2026-08-15T16:00:02Z",
      "Invalid shortcut.",
    )).toThrow(JournalTransitionError);
  });

  test("rejects a journal that is not valid JSON", async () => {
    const { root, bundle } = await createPlanBundle();
    const stateDirectory = join(root, "state");
    const store = new JournalStore(stateDirectory);
    const initial = createRunJournal(bundle.plan, {
      runId: "run_invalid123",
      at: "2026-08-15T16:00:00Z",
    });
    await store.create(initial);
    await writeFile(
      join(stateDirectory, "runs", initial.runId, "journal.json"),
      "{invalid",
      "utf8",
    );

    await expect(store.load(initial.runId)).rejects.toMatchObject({
      code: "JOURNAL_INTEGRITY_FAILED",
    });
  });

  test("recovers an orphaned journal lock from a dead writer", async () => {
    const { root, bundle } = await createPlanBundle();
    const stateDirectory = join(root, "state");
    const store = new JournalStore(stateDirectory);
    const initial = createRunJournal(bundle.plan, {
      runId: "run_orphan123",
      at: "2026-08-15T16:00:00Z",
    });
    await store.create(initial);
    await writeFile(
      join(stateDirectory, "runs", initial.runId, "journal.json.lock"),
      `${JSON.stringify({ pid: 2_147_483_647, createdAt: "2026-08-15T16:00:00Z" })}\n`,
      "utf8",
    );
    const applying = transitionRun(
      initial,
      "applying",
      "2026-08-15T16:00:01Z",
      "Execution started after lock recovery.",
    );

    await store.update(applying, 0);
    expect((await store.load(initial.runId)).status).toBe("applying");
  });
});
