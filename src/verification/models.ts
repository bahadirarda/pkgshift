import type { Diagnostic } from "../domain/models.ts";

export type VerificationCheckStatus = "passed" | "failed" | "skipped";

export interface VerificationCheck {
  id: string;
  status: VerificationCheckStatus;
  summary: string;
  evidence: string[];
}

export interface VerificationReport {
  schemaVersion: "1.0";
  reportId: string;
  runId: string;
  planId: string;
  createdAt: string;
  status: "passed" | "failed";
  checks: VerificationCheck[];
  diagnostics: Diagnostic[];
}

export interface VerificationEnvelope {
  storeSchemaVersion: "1.0";
  digest: string;
  report: VerificationReport;
}
