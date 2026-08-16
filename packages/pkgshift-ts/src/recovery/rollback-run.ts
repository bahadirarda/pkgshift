import { PlanArtifactStore } from "../artifacts/plan-artifact-store.ts";
import type { Diagnostic } from "../domain/models.ts";
import { inspectProject } from "../inspect/inspect-project.ts";
import { JournalStore } from "../journal/journal-store.ts";
import type { RunJournal } from "../journal/models.ts";
import { transitionOperation, transitionRun } from "../journal/transitions.ts";
import { RepositoryLock, RepositoryLockError } from "../execution/repository-lock.ts";
import { SnapshotStore } from "./snapshot-store.ts";

export class RollbackFailure extends Error {
  constructor(readonly diagnostic: Diagnostic) {
    super(diagnostic.summary);
    this.name = "RollbackFailure";
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

async function rollbackRunLocked(options: {
  projectRoot: string;
  stateDirectory: string;
  runId: string;
  approval: string | null;
  now?: () => string;
}): Promise<{ journal: RunJournal; diagnostics: Diagnostic[] }> {
  const now = options.now ?? (() => new Date().toISOString());
  if (options.approval !== options.runId) {
    throw new RollbackFailure({
      code: "APPROVAL_REQUIRED",
      severity: "error",
      summary: `Rollback requires exact approval for ${options.runId}.`,
      blocking: true,
      remediation: [`Retry with --approve ${options.runId}.`],
    });
  }
  const journalStore = new JournalStore(options.stateDirectory);
  const snapshotStore = new SnapshotStore(options.stateDirectory);
  let journal = await journalStore.load(options.runId);
  if (!["applying", "verifying", "succeeded", "failed", "rollback-failed"].includes(journal.status)) {
    throw new RollbackFailure({
      code: "ROLLBACK_RUN_STATE_INVALID",
      severity: "error",
      summary: `Run ${options.runId} cannot roll back from ${journal.status}.`,
      blocking: true,
      remediation: ["Select a run that has recoverable applied state."],
    });
  }
  const bundle = await new PlanArtifactStore(options.stateDirectory).load(
    options.projectRoot,
    journal.planId,
  );
  journal = await persist(
    journalStore,
    journal,
    transitionRun(journal, "rolling-back", now(), "Approved rollback started."),
  );
  try {
    await snapshotStore.restore(options.runId, options.projectRoot);
    for (const operation of [...journal.operations].reverse()) {
      if (!["running", "succeeded", "failed"].includes(operation.status)) continue;
      journal = await persist(
        journalStore,
        journal,
        transitionOperation(
          journal,
          operation.operationId,
          "rolled-back",
          now(),
          `Recovery snapshot restored for ${operation.kind}.`,
        ),
      );
    }
    const inspection = await inspectProject(options.projectRoot);
    if (inspection.fingerprint !== bundle.content.plan.repositoryFingerprint) {
      throw new Error("Restored repository fingerprint does not match the approved plan baseline.");
    }
    journal = await persist(
      journalStore,
      journal,
      transitionRun(journal, "rolled-back", now(), "Repository files were restored and verified."),
    );
    return {
      journal,
      diagnostics: [{
        code: "ROLLBACK_EXTERNAL_EFFECTS_REMAIN",
        severity: "warning",
        summary: "Repository files were restored; dependency cache and node_modules side effects are not reverted.",
        blocking: false,
        remediation: ["Reinstall the source dependency state if node_modules parity is required."],
      }],
    };
  } catch (error) {
    journal = await persist(
      journalStore,
      journal,
      transitionRun(
        journal,
        "rollback-failed",
        now(),
        error instanceof Error ? error.message : "Rollback failed.",
      ),
    );
    return {
      journal,
      diagnostics: [{
        code: "ROLLBACK_FAILED",
        severity: "error",
        summary: error instanceof Error ? error.message : "Rollback failed.",
        blocking: true,
        remediation: ["Preserve the state directory and inspect snapshot integrity before retrying."],
      }],
    };
  }
}

export async function rollbackRun(options: {
  projectRoot: string;
  stateDirectory: string;
  runId: string;
  approval: string | null;
  now?: () => string;
}): Promise<{ journal: RunJournal; diagnostics: Diagnostic[] }> {
  let lock: RepositoryLock;
  try {
    lock = await RepositoryLock.acquire({
      stateDirectory: options.stateDirectory,
      projectRoot: options.projectRoot,
      operation: "rollback",
    });
  } catch (error) {
    if (error instanceof RepositoryLockError) {
      throw new RollbackFailure({
        code: error.code,
        severity: "error",
        summary: error.message,
        blocking: true,
        remediation: ["Wait for the active migration transaction to finish before retrying rollback."],
      });
    }
    throw error;
  }
  try {
    return await rollbackRunLocked(options);
  } finally {
    await lock.release();
  }
}
