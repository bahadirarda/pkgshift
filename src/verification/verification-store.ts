import { join, resolve } from "node:path";
import { atomicWriteJson } from "../core/atomic-json.ts";
import { readJsonObject, sha256Json } from "../core/files.ts";
import type {
  VerificationEnvelope,
  VerificationReport,
} from "./models.ts";

export class VerificationStoreError extends Error {
  constructor(readonly code: string, message: string) {
    super(message);
    this.name = "VerificationStoreError";
  }
}

export class VerificationStore {
  readonly root: string;

  constructor(stateDirectory: string) {
    this.root = resolve(stateDirectory);
  }

  private path(runId: string): string {
    if (!/^run_[a-z0-9]+$/.test(runId)) {
      throw new VerificationStoreError("VERIFICATION_RUN_ID_INVALID", `Invalid run identifier: ${runId}`);
    }
    return join(this.root, "runs", runId, "verification.json");
  }

  async save(report: VerificationReport): Promise<void> {
    const envelope: VerificationEnvelope = {
      storeSchemaVersion: "1.0",
      digest: `sha256:${sha256Json(report)}`,
      report,
    };
    await atomicWriteJson(this.path(report.runId), envelope);
  }

  async load(runId: string): Promise<VerificationReport> {
    let value: Record<string, unknown> | null;
    try {
      value = await readJsonObject(this.path(runId));
    } catch {
      throw new VerificationStoreError("VERIFICATION_INTEGRITY_FAILED", "Verification report is not valid JSON.");
    }
    if (!value || value.storeSchemaVersion !== "1.0" || typeof value.digest !== "string") {
      throw new VerificationStoreError("VERIFICATION_NOT_FOUND", `Verification report was not found: ${runId}`);
    }
    const report = value.report as VerificationReport | undefined;
    if (!report || report.runId !== runId || `sha256:${sha256Json(report)}` !== value.digest) {
      throw new VerificationStoreError("VERIFICATION_INTEGRITY_FAILED", "Verification report failed integrity checks.");
    }
    return report;
  }
}
