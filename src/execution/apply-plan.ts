import { randomUUID } from "node:crypto";
import { chmod, lstat, readFile, unlink } from "node:fs/promises";
import { getPackageManager } from "../adapters/catalog.ts";
import { PlanArtifactStore } from "../artifacts/plan-artifact-store.ts";
import { atomicWriteFile } from "../core/atomic-json.ts";
import { sha256Text } from "../core/files.ts";
import { safeProjectFilePath } from "../core/project-path.ts";
import type {
  Diagnostic,
  PlannedFileMutation,
  PlannedOperation,
} from "../domain/models.ts";
import { inspectProject } from "../inspect/inspect-project.ts";
import { JournalStore } from "../journal/journal-store.ts";
import { createRunJournal, type RunJournal } from "../journal/models.ts";
import {
  transitionOperation,
  transitionRun,
} from "../journal/transitions.ts";
import { SnapshotStore } from "../recovery/snapshot-store.ts";
import type { ApplyExecutionResult, ApplyOptions } from "./models.ts";
import {
  BunProcessRunner,
  type ProcessRunner,
} from "./process-runner.ts";
import { ExecutionStore } from "./execution-store.ts";
import { RepositoryLock, RepositoryLockError } from "./repository-lock.ts";

class ApplyFailure extends Error {
  constructor(readonly diagnostic: Diagnostic) {
    super(diagnostic.summary);
    this.name = "ApplyFailure";
  }
}

function failure(code: string, summary: string, remediation: string[]): ApplyFailure {
  return new ApplyFailure({
    code,
    severity: "error",
    summary,
    blocking: true,
    remediation,
  });
}

function createRunId(): string {
  return `run_${randomUUID().replaceAll("-", "").slice(0, 24)}`;
}

async function fileState(path: string): Promise<Awaited<ReturnType<typeof lstat>> | null> {
  try {
    return await lstat(path);
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === "ENOENT") return null;
    throw error;
  }
}

async function executeMutation(
  projectRoot: string,
  mutation: PlannedFileMutation,
): Promise<void> {
  const path = await safeProjectFilePath(projectRoot, mutation.path);
  const state = await fileState(path);
  if (state?.isSymbolicLink() || (state && !state.isFile())) {
    throw failure(
      "EXECUTION_PATH_TYPE_UNSAFE",
      `Mutation target is not a regular file: ${mutation.path}`,
      ["Replace the path with a regular file and create a new plan."],
    );
  }
  const beforeDigest = state ? sha256Text(await readFile(path)) : null;
  if (beforeDigest !== mutation.beforeDigest) {
    throw failure(
      "PLAN_PRECONDITION_FAILED",
      `Mutation precondition changed after planning: ${mutation.path}`,
      ["Inspect the repository and create a new plan."],
    );
  }
  if (mutation.action === "delete") {
    if (state) await unlink(path);
  } else {
    if (mutation.content === undefined || mutation.afterDigest === null) {
      throw failure(
        "PLAN_ARTIFACT_INVALID",
        `Write mutation is incomplete: ${mutation.path}`,
        ["Discard the plan artifact and create a new plan."],
      );
    }
    const mode = state ? Number(state.mode) & 0o777 : 0o644;
    await atomicWriteFile(path, mutation.content, mode);
    await chmod(path, mode);
  }
  const afterState = await fileState(path);
  const afterDigest = afterState ? sha256Text(await readFile(path)) : null;
  if (afterDigest !== mutation.afterDigest) {
    throw failure(
      "EXECUTION_POSTCONDITION_FAILED",
      `Mutation postcondition failed: ${mutation.path}`,
      ["Preserve the run identifier and roll back the run."],
    );
  }
}

async function persist(
  store: JournalStore,
  current: RunJournal,
  next: RunJournal,
): Promise<RunJournal> {
  await store.update(next, current.revision);
  return next;
}

function recoveryReferences(operation: PlannedOperation): string[] {
  return (operation.mutations ?? operation.paths.map((path) => ({ path })))
    .map((entry) => `snapshot:${entry.path}`);
}

async function applyPlanLocked(
  options: ApplyOptions,
  runner: ProcessRunner = new BunProcessRunner(),
): Promise<ApplyExecutionResult> {
  const now = options.now ?? (() => new Date().toISOString());
  const artifactStore = new PlanArtifactStore(options.stateDirectory);
  const journalStore = new JournalStore(options.stateDirectory);
  const snapshotStore = new SnapshotStore(options.stateDirectory);
  const executionStore = new ExecutionStore(options.stateDirectory);
  const processes: ApplyExecutionResult["processes"] = [];
  const diagnostics: Diagnostic[] = [];

  const stored = await artifactStore.load(options.projectRoot, options.planId);
  const plan = stored.content.plan;
  if (options.approval !== plan.planId) {
    throw failure(
      "APPROVAL_REQUIRED",
      `Apply requires exact approval for ${plan.planId}.`,
      [`Retry with --approve ${plan.planId}.`],
    );
  }
  if (!plan.executable) {
    throw failure(
      "PLAN_NOT_EXECUTABLE",
      `Plan ${plan.planId} is not executable.`,
      ["Resolve blocking diagnostics and create a new production-target plan."],
    );
  }
  const inspection = await inspectProject(options.projectRoot);
  if (inspection.fingerprint !== plan.repositoryFingerprint) {
    throw failure(
      "PLAN_PRECONDITION_FAILED",
      "Migration-relevant repository evidence changed after planning.",
      ["Inspect the repository and create a new plan."],
    );
  }

  const runId = options.runId ?? createRunId();
  let journal = createRunJournal(plan, { runId, at: now() });
  await journalStore.create(journal);
  journal = await persist(
    journalStore,
    journal,
    transitionRun(journal, "applying", now(), "Approved plan execution started."),
  );

  const mutationPaths = plan.operations.flatMap((operation) =>
    (operation.mutations ?? []).map((entry) => entry.path)
  );
  const targetLockfiles = getPackageManager(plan.target).lockfiles;
  try {
    await snapshotStore.create(
      runId,
      options.projectRoot,
      [...mutationPaths, ...targetLockfiles],
      now(),
    );
  } catch (error) {
    diagnostics.push({
      code: "SNAPSHOT_CREATE_FAILED",
      severity: "error",
      summary: error instanceof Error ? error.message : "Recovery snapshot creation failed.",
      blocking: true,
      remediation: ["Do not retry mutation until the snapshot boundary is healthy."],
    });
    journal = await persist(
      journalStore,
      journal,
      transitionRun(journal, "failed", now(), "Recovery snapshot creation failed."),
    );
    return { journal, diagnostics, processes };
  }

  for (const operation of plan.operations.filter((entry) => entry.phase !== "verify")) {
    let running = false;
    try {
      journal = await persist(
        journalStore,
        journal,
        transitionOperation(
          journal,
          operation.id,
          "running",
          now(),
          `Started ${operation.kind}.`,
          recoveryReferences(operation),
        ),
      );
      running = true;
      for (const plannedMutation of operation.mutations ?? []) {
        await executeMutation(options.projectRoot, plannedMutation);
      }
      if (operation.command) {
        const record = await runner.run(operation.command, options.projectRoot);
        processes.push(record);
        await executionStore.save(runId, processes);
        if (record.exitCode !== 0 || record.timedOut) {
          throw failure(
            record.timedOut ? "INSTALL_COMMAND_TIMEOUT" : "INSTALL_COMMAND_FAILED",
            `${operation.command.join(" ")} did not complete successfully.`,
            ["Inspect the redacted process record and roll back or repair before retrying."],
          );
        }
      }
      journal = await persist(
        journalStore,
        journal,
        transitionOperation(
          journal,
          operation.id,
          "succeeded",
          now(),
          `Completed ${operation.kind}.`,
        ),
      );
    } catch (error) {
      const diagnostic = error instanceof ApplyFailure
        ? error.diagnostic
        : {
            code: "EXECUTION_FAILED",
            severity: "error" as const,
            summary: error instanceof Error ? error.message : "Plan execution failed.",
            blocking: true,
            remediation: ["Preserve the run identifier and roll back the run."],
          };
      diagnostics.push(diagnostic);
      if (running) {
        journal = await persist(
          journalStore,
          journal,
          transitionOperation(
            journal,
            operation.id,
            "failed",
            now(),
            `Failed ${operation.kind}: ${diagnostic.code}.`,
          ),
        );
      }
      journal = await persist(
        journalStore,
        journal,
        transitionRun(journal, "failed", now(), `Apply stopped at ${operation.kind}.`),
      );
      return { journal, diagnostics, processes };
    }
  }

  journal = await persist(
    journalStore,
    journal,
    transitionRun(journal, "verifying", now(), "Apply completed and the run is ready for verification."),
  );
  return { journal, diagnostics, processes };
}

export async function applyPlan(
  options: ApplyOptions,
  runner: ProcessRunner = new BunProcessRunner(),
): Promise<ApplyExecutionResult> {
  let lock: RepositoryLock;
  try {
    lock = await RepositoryLock.acquire({
      stateDirectory: options.stateDirectory,
      projectRoot: options.projectRoot,
      operation: "apply",
    });
  } catch (error) {
    if (error instanceof RepositoryLockError) {
      throw failure(error.code, error.message, [
        "Wait for the active migration transaction to finish, then inspect repository state before retrying.",
      ]);
    }
    throw error;
  }
  try {
    return await applyPlanLocked(options, runner);
  } finally {
    await lock.release();
  }
}

export { ApplyFailure };
