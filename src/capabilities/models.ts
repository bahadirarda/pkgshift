import type {
  Diagnostic,
  PackageManagerId,
} from "../domain/models.ts";
import type {
  EvidenceReference,
  FeatureId,
} from "../ir/models.ts";

export type CapabilityClassification =
  | "native"
  | "transform"
  | "lossy"
  | "unsupported"
  | "unknown"
  | "not-applicable";

export type CapabilityRisk = "none" | "low" | "medium" | "high";

export interface CapabilityDecision {
  featureId: FeatureId;
  title: string;
  target: PackageManagerId;
  classification: CapabilityClassification;
  risk: CapabilityRisk;
  transformationId: string | null;
  summary: string;
  evidence: EvidenceReference[];
  basis: string[];
}

export interface CapabilityAnalysis {
  schemaVersion: "1.0";
  analysisId: string;
  projectIrId: string;
  source: PackageManagerId;
  target: PackageManagerId;
  decisions: CapabilityDecision[];
  summary: {
    native: number;
    transform: number;
    lossy: number;
    unsupported: number;
    unknown: number;
    notApplicable: number;
  };
  diagnostics: Diagnostic[];
}

