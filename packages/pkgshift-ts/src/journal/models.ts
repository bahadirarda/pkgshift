import type { MigrationPlan } from "../domain/models.ts";

export type RunStatus =
  | "initialized"
  | "applying"
  | "verifying"
  | "succeeded"
  | "failed"
  | "rolling-back"
  | "rolled-back"
  | "rollback-failed";

export type OperationStatus =
  | "pending"
  | "running"
  | "succeeded"
  | "failed"
  | "rolled-back";

export interface JournalOperation {
  operationId: string;
  kind: string;
  status: OperationStatus;
  attempts: number;
  recoveryReferences: string[];
}

export interface JournalEvent {
  sequence: number;
  at: string;
  type: string;
  detail: string;
  operationId?: string;
}

export interface RunJournal {
  schemaVersion: "1.0";
  runId: string;
  revision: number;
  planId: string;
  projectIrId: string;
  repositoryFingerprint: string;
  status: RunStatus;
  createdAt: string;
  updatedAt: string;
  operations: JournalOperation[];
  events: JournalEvent[];
}

export interface JournalEnvelope {
  storeSchemaVersion: "1.0";
  digest: string;
  journal: RunJournal;
}

export interface CreateJournalOptions {
  runId: string;
  at: string;
}

export function createRunJournal(
  plan: MigrationPlan,
  options: CreateJournalOptions,
): RunJournal {
  return {
    schemaVersion: "1.0",
    runId: options.runId,
    revision: 0,
    planId: plan.planId,
    projectIrId: plan.projectIrId,
    repositoryFingerprint: plan.repositoryFingerprint,
    status: "initialized",
    createdAt: options.at,
    updatedAt: options.at,
    operations: plan.operations.map((operation) => ({
      operationId: operation.id,
      kind: operation.kind,
      status: "pending",
      attempts: 0,
      recoveryReferences: [],
    })),
    events: [{
      sequence: 1,
      at: options.at,
      type: "run.initialized",
      detail: `Run initialized for plan ${plan.planId}.`,
    }],
  };
}

