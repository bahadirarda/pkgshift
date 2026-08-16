import type {
  JournalEvent,
  OperationStatus,
  RunJournal,
  RunStatus,
} from "./models.ts";

const RUN_TRANSITIONS: Record<RunStatus, RunStatus[]> = {
  initialized: ["applying", "failed"],
  applying: ["verifying", "failed", "rolling-back"],
  verifying: ["succeeded", "failed", "rolling-back"],
  succeeded: ["rolling-back"],
  failed: ["rolling-back"],
  "rolling-back": ["rolled-back", "rollback-failed"],
  "rolled-back": [],
  "rollback-failed": ["rolling-back"],
};

const OPERATION_TRANSITIONS: Record<OperationStatus, OperationStatus[]> = {
  pending: ["running"],
  running: ["succeeded", "failed", "rolled-back"],
  succeeded: ["rolled-back"],
  failed: ["running", "rolled-back"],
  "rolled-back": [],
};

export class JournalTransitionError extends Error {
  constructor(
    readonly code: string,
    message: string,
  ) {
    super(message);
    this.name = "JournalTransitionError";
  }
}

function appendEvent(
  journal: RunJournal,
  event: Omit<JournalEvent, "sequence">,
): JournalEvent[] {
  return [
    ...journal.events,
    { ...event, sequence: journal.events.length + 1 },
  ];
}

export function transitionRun(
  journal: RunJournal,
  status: RunStatus,
  at: string,
  detail: string,
): RunJournal {
  if (!RUN_TRANSITIONS[journal.status].includes(status)) {
    throw new JournalTransitionError(
      "JOURNAL_TRANSITION_INVALID",
      `Run cannot transition from ${journal.status} to ${status}.`,
    );
  }
  return {
    ...journal,
    revision: journal.revision + 1,
    status,
    updatedAt: at,
    events: appendEvent(journal, {
      at,
      type: `run.${status}`,
      detail,
    }),
  };
}

export function transitionOperation(
  journal: RunJournal,
  operationId: string,
  status: OperationStatus,
  at: string,
  detail: string,
  recoveryReferences: string[] = [],
): RunJournal {
  const operation = journal.operations.find((candidate) =>
    candidate.operationId === operationId
  );
  if (!operation) {
    throw new JournalTransitionError(
      "JOURNAL_OPERATION_NOT_FOUND",
      `Journal operation was not found: ${operationId}`,
    );
  }
  if (!OPERATION_TRANSITIONS[operation.status].includes(status)) {
    throw new JournalTransitionError(
      "JOURNAL_OPERATION_TRANSITION_INVALID",
      `Operation ${operationId} cannot transition from ${operation.status} to ${status}.`,
    );
  }
  return {
    ...journal,
    revision: journal.revision + 1,
    updatedAt: at,
    operations: journal.operations.map((candidate) =>
      candidate.operationId === operationId
        ? {
            ...candidate,
            status,
            attempts: status === "running"
              ? candidate.attempts + 1
              : candidate.attempts,
            recoveryReferences: [
              ...new Set([...candidate.recoveryReferences, ...recoveryReferences]),
            ],
          }
        : candidate
    ),
    events: appendEvent(journal, {
      at,
      type: `operation.${status}`,
      detail,
      operationId,
    }),
  };
}
