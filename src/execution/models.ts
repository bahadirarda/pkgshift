import type { Diagnostic } from "../domain/models.ts";
import type { RunJournal } from "../journal/models.ts";
import type { ProcessExecutionRecord } from "./process-runner.ts";

export interface ApplyExecutionResult {
  journal: RunJournal;
  diagnostics: Diagnostic[];
  processes: ProcessExecutionRecord[];
}

export interface ApplyOptions {
  projectRoot: string;
  stateDirectory: string;
  planId: string;
  approval: string | null;
  runId?: string;
  now?: () => string;
}
