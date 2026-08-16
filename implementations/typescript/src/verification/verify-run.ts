import { lstat, readFile } from "node:fs/promises";
import { join } from "node:path";
import { getPackageManager } from "../adapters/catalog.ts";
import { PlanArtifactStore } from "../artifacts/plan-artifact-store.ts";
import { pathExists, sha256Json, sha256Text } from "../core/files.ts";
import { safeProjectFilePath } from "../core/project-path.ts";
import type { Diagnostic, PlannedFileMutation } from "../domain/models.ts";
import { RepositoryLock, RepositoryLockError } from "../execution/repository-lock.ts";
import { inspectProject } from "../inspect/inspect-project.ts";
import { buildProjectIR } from "../ir/build-project-ir.ts";
import { JournalStore } from "../journal/journal-store.ts";
import type { RunJournal } from "../journal/models.ts";
import { transitionOperation, transitionRun } from "../journal/transitions.ts";
import type {
  VerificationCheck,
  VerificationReport,
} from "./models.ts";
import { VerificationStore } from "./verification-store.ts";

export class VerificationFailure extends Error {
  constructor(readonly diagnostic: Diagnostic) {
    super(diagnostic.summary);
    this.name = "VerificationFailure";
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

async function mutationCheck(
  root: string,
  mutation: PlannedFileMutation,
): Promise<{ passed: boolean; evidence: string }> {
  const path = await safeProjectFilePath(root, mutation.path);
  try {
    const state = await lstat(path);
    if (state.isSymbolicLink() || !state.isFile()) {
      return { passed: false, evidence: `${mutation.path} is not a regular file` };
    }
    const digest = sha256Text(await readFile(path));
    return {
      passed: digest === mutation.afterDigest,
      evidence: `${mutation.path} digest ${digest}`,
    };
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === "ENOENT") {
      return {
        passed: mutation.afterDigest === null,
        evidence: `${mutation.path} is absent`,
      };
    }
    throw error;
  }
}

function failedDiagnostic(checks: VerificationCheck[]): Diagnostic {
  const failed = checks.filter((check) => check.status === "failed");
  return {
    code: "VERIFICATION_FAILED",
    severity: "error",
    summary: `${failed.length} blocking verification check${failed.length === 1 ? "" : "s"} failed.`,
    blocking: true,
    evidence: failed.map((check) => ({
      location: check.id,
      detail: check.summary,
    })),
    remediation: ["Inspect the verification report, then repair or roll back the run."],
  };
}

async function verifyRunLocked(options: {
  projectRoot: string;
  stateDirectory: string;
  runId: string;
  now?: () => string;
}): Promise<{ journal: RunJournal; report: VerificationReport }> {
  const now = options.now ?? (() => new Date().toISOString());
  const journalStore = new JournalStore(options.stateDirectory);
  const verificationStore = new VerificationStore(options.stateDirectory);
  let journal = await journalStore.load(options.runId);
  if (journal.status !== "verifying") {
    throw new VerificationFailure({
      code: "VERIFICATION_RUN_STATE_INVALID",
      severity: "error",
      summary: `Run ${options.runId} cannot be verified from ${journal.status}.`,
      blocking: true,
      remediation: ["Verify a run whose apply phase completed successfully."],
    });
  }
  const bundle = await new PlanArtifactStore(options.stateDirectory).load(
    options.projectRoot,
    journal.planId,
  );
  const plan = bundle.content.plan;
  const verifyOperation = plan.operations.find((operation) => operation.phase === "verify");
  if (!verifyOperation) {
    throw new VerificationFailure({
      code: "PLAN_ARTIFACT_INVALID",
      severity: "error",
      summary: `Plan ${plan.planId} does not contain a verification operation.`,
      blocking: true,
      remediation: ["Discard the plan and create a new plan."],
    });
  }
  journal = await persist(
    journalStore,
    journal,
    transitionOperation(journal, verifyOperation.id, "running", now(), "Verification started."),
  );
  try {
  const checks: VerificationCheck[] = [];
  const mutations = plan.operations.flatMap((operation) => operation.mutations ?? []);
  const mutationResults = await Promise.all(
    mutations.map((entry) => mutationCheck(options.projectRoot, entry)),
  );
  const badMutations = mutationResults.filter((entry) => !entry.passed);
  checks.push({
    id: "planned-file-digests",
    status: badMutations.length === 0 ? "passed" : "failed",
    summary: badMutations.length === 0
      ? "Every planned file mutation matches its post-apply digest."
      : `${badMutations.length} planned file mutations do not match.`,
    evidence: mutationResults.map((entry) => entry.evidence),
  });

  const inspection = await inspectProject(options.projectRoot);
  checks.push({
    id: "target-selection",
    status: inspection.packageManager.selected === plan.target ? "passed" : "failed",
    summary: inspection.packageManager.selected === plan.target
      ? `${plan.target} is the selected package manager.`
      : `Expected ${plan.target}, detected ${inspection.packageManager.selected ?? "none"}.`,
    evidence: inspection.packageManager.candidates.map((entry) => `${entry.manager}:${entry.score}`),
  });

  const targetLocks = getPackageManager(plan.target).lockfiles;
  const existingTargetLocks = [];
  for (const path of targetLocks) {
    if (await pathExists(join(options.projectRoot, path))) existingTargetLocks.push(path);
  }
  checks.push({
    id: "target-lockfile",
    status: existingTargetLocks.length > 0 ? "passed" : "failed",
    summary: existingTargetLocks.length > 0
      ? `Target dependency state exists: ${existingTargetLocks.join(", ")}.`
      : `No ${plan.target} lockfile was generated.`,
    evidence: existingTargetLocks,
  });

  const currentIr = await buildProjectIR(inspection);
  const expectedPackages = bundle.content.projectIr.packages.map((entry) => entry.path);
  const actualPackages = currentIr?.packages.map((entry) => entry.path) ?? [];
  const workspaceMatches = JSON.stringify(expectedPackages) === JSON.stringify(actualPackages);
  checks.push({
    id: "workspace-membership",
    status: workspaceMatches ? "passed" : "failed",
    summary: workspaceMatches
      ? "Workspace package membership is preserved."
      : "Workspace package membership changed.",
    evidence: [`expected:${expectedPackages.join(",")}`, `actual:${actualPackages.join(",")}`],
  });

  const installOperation = journal.operations.find((entry) =>
    entry.kind === "dependency.install-target"
    || entry.kind === "dependency.import-and-install-target"
  );
  checks.push({
    id: "target-install",
    status: installOperation?.status === "succeeded" ? "passed" : "failed",
    summary: installOperation?.status === "succeeded"
      ? "The target installation operation completed successfully."
      : "The target installation operation is not successful.",
    evidence: [`status:${installOperation?.status ?? "missing"}`],
  });

  checks.push({
    id: "dependency-graph-drift",
    status: "skipped",
    summary: "The TypeScript reference does not persist the Rust primary path's source lock graph.",
    evidence: ["Use the Rust primary CLI when blocking resolution-set proof is required."],
  });

  const diagnostics = checks.some((check) => check.status === "failed")
    ? [failedDiagnostic(checks)]
    : [];
  const status: VerificationReport["status"] = diagnostics.length === 0 ? "passed" : "failed";
  const identity = {
    schemaVersion: "1.0" as const,
    runId: options.runId,
    planId: plan.planId,
    createdAt: now(),
    status,
    checks,
    diagnostics,
  };
  const report: VerificationReport = {
    ...identity,
    reportId: `verification_${sha256Json(identity).slice(0, 24)}`,
  };
  await verificationStore.save(report);

  journal = await persist(
    journalStore,
    journal,
    transitionOperation(
      journal,
      verifyOperation.id,
      status === "passed" ? "succeeded" : "failed",
      now(),
      status === "passed" ? "Verification passed." : "Verification failed.",
    ),
  );
  journal = await persist(
    journalStore,
    journal,
    transitionRun(
      journal,
      status === "passed" ? "succeeded" : "failed",
      now(),
      status === "passed" ? "Migration verified successfully." : "Migration verification failed.",
    ),
  );
  return { journal, report };
  } catch (error) {
    const currentOperation = journal.operations.find((entry) => entry.operationId === verifyOperation.id);
    if (currentOperation?.status === "running") {
      journal = await persist(
        journalStore,
        journal,
        transitionOperation(
          journal,
          verifyOperation.id,
          "failed",
          now(),
          "Verification stopped because an internal check failed.",
        ),
      );
    }
    if (journal.status === "verifying") {
      journal = await persist(
        journalStore,
        journal,
        transitionRun(journal, "failed", now(), "Verification could not complete."),
      );
    }
    throw new VerificationFailure({
      code: "VERIFICATION_INTERNAL_ERROR",
      severity: "error",
      summary: error instanceof Error ? error.message : "Verification could not complete.",
      blocking: true,
      remediation: ["Preserve the run state and inspect or roll back the run."],
    });
  }
}

export async function verifyRun(options: {
  projectRoot: string;
  stateDirectory: string;
  runId: string;
  now?: () => string;
}): Promise<{ journal: RunJournal; report: VerificationReport }> {
  let lock: RepositoryLock;
  try {
    lock = await RepositoryLock.acquire({
      stateDirectory: options.stateDirectory,
      projectRoot: options.projectRoot,
      operation: "verify",
    });
  } catch (error) {
    if (error instanceof RepositoryLockError) {
      throw new VerificationFailure({
        code: error.code,
        severity: "error",
        summary: error.message,
        blocking: true,
        remediation: ["Wait for the active migration transaction to finish before retrying verification."],
      });
    }
    throw error;
  }
  try {
    return await verifyRunLocked(options);
  } finally {
    await lock.release();
  }
}
