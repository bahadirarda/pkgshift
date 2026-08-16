import { join, resolve } from "node:path";
import { atomicWriteJson } from "../core/atomic-json.ts";
import { readJsonObject, sha256Json } from "../core/files.ts";
import type { ProcessExecutionRecord } from "./process-runner.ts";

export interface ExecutionReport {
  schemaVersion: "1.0";
  runId: string;
  records: ProcessExecutionRecord[];
}

interface ExecutionEnvelope {
  storeSchemaVersion: "1.0";
  digest: string;
  report: ExecutionReport;
}

export class ExecutionStoreError extends Error {
  constructor(readonly code: string, message: string) {
    super(message);
    this.name = "ExecutionStoreError";
  }
}

export class ExecutionStore {
  readonly root: string;

  constructor(stateDirectory: string) {
    this.root = resolve(stateDirectory);
  }

  private path(runId: string): string {
    if (!/^run_[a-z0-9]+$/.test(runId)) {
      throw new ExecutionStoreError("EXECUTION_RUN_ID_INVALID", `Invalid run identifier: ${runId}`);
    }
    return join(this.root, "runs", runId, "execution.json");
  }

  async save(runId: string, records: ProcessExecutionRecord[]): Promise<void> {
    const report: ExecutionReport = { schemaVersion: "1.0", runId, records };
    const envelope: ExecutionEnvelope = {
      storeSchemaVersion: "1.0",
      digest: `sha256:${sha256Json(report)}`,
      report,
    };
    await atomicWriteJson(this.path(runId), envelope);
  }

  async load(runId: string): Promise<ExecutionReport> {
    let value: Record<string, unknown> | null;
    try {
      value = await readJsonObject(this.path(runId));
    } catch {
      throw new ExecutionStoreError("EXECUTION_INTEGRITY_FAILED", "Execution report is not valid JSON.");
    }
    if (!value || value.storeSchemaVersion !== "1.0" || typeof value.digest !== "string") {
      throw new ExecutionStoreError("EXECUTION_NOT_FOUND", `Execution report was not found: ${runId}`);
    }
    const report = value.report as ExecutionReport | undefined;
    if (!report || report.runId !== runId || `sha256:${sha256Json(report)}` !== value.digest) {
      throw new ExecutionStoreError("EXECUTION_INTEGRITY_FAILED", "Execution report failed integrity checks.");
    }
    return report;
  }
}
