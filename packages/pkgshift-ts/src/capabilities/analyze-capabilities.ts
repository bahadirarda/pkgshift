import { sha256Json } from "../core/files.ts";
import type {
  Diagnostic,
  PackageManagerId,
} from "../domain/models.ts";
import type { ProjectIR } from "../ir/models.ts";
import type {
  CapabilityAnalysis,
  CapabilityClassification,
  CapabilityDecision,
} from "./models.ts";
import { CAPABILITY_RULES, unknownOutcome } from "./rules.ts";

const SUMMARY_KEY: Record<CapabilityClassification, keyof CapabilityAnalysis["summary"]> = {
  native: "native",
  transform: "transform",
  lossy: "lossy",
  unsupported: "unsupported",
  unknown: "unknown",
  "not-applicable": "notApplicable",
};

function diagnosticForDecision(
  decision: CapabilityDecision,
): Diagnostic | null {
  if (decision.classification === "lossy") {
    return {
      code: "CAPABILITY_LOSSY",
      severity: "warning",
      summary: decision.summary,
      blocking: false,
      evidence: decision.evidence.map((item) => ({
        location: item.location,
        detail: `${decision.featureId}: ${item.detail}`,
      })),
      remediation: ["Review and explicitly accept the semantic compromise before apply."],
    };
  }
  if (decision.classification === "unsupported") {
    return {
      code: "CAPABILITY_UNSUPPORTED",
      severity: "error",
      summary: decision.summary,
      blocking: true,
      evidence: decision.evidence.map((item) => ({
        location: item.location,
        detail: `${decision.featureId}: ${item.detail}`,
      })),
      remediation: ["Remove the source capability, choose another target, or add a verified adapter rule."],
    };
  }
  if (decision.classification === "unknown") {
    return {
      code: "CAPABILITY_UNKNOWN",
      severity: "error",
      summary: decision.summary,
      blocking: true,
      evidence: decision.evidence.map((item) => ({
        location: item.location,
        detail: `${decision.featureId}: ${item.detail}`,
      })),
      remediation: ["Gather authoritative target evidence or choose a target with a known capability result."],
    };
  }
  return null;
}

export function analyzeCapabilities(
  projectIr: ProjectIR,
  target: PackageManagerId,
): CapabilityAnalysis | null {
  const source = projectIr.source;
  if (!source) {
    return null;
  }

  const decisions: CapabilityDecision[] = projectIr.features.map((feature) => {
    const rule = CAPABILITY_RULES[feature.id];
    const outcome = source === target
      ? {
          classification: "native" as const,
          risk: "none" as const,
          summary: `${rule.title} remains on its source package manager.`,
        }
      : rule.targets[target] ?? unknownOutcome(feature.id, target);
    return {
      featureId: feature.id,
      title: rule.title,
      target,
      classification: outcome.classification,
      risk: outcome.risk,
      transformationId: outcome.transformationId ?? null,
      summary: outcome.summary,
      evidence: feature.evidence,
      basis: rule.basis,
    };
  }).sort((left, right) => left.featureId.localeCompare(right.featureId));

  const summary: CapabilityAnalysis["summary"] = {
    native: 0,
    transform: 0,
    lossy: 0,
    unsupported: 0,
    unknown: 0,
    notApplicable: 0,
  };
  for (const decision of decisions) {
    summary[SUMMARY_KEY[decision.classification]] += 1;
  }
  const diagnostics = decisions
    .map(diagnosticForDecision)
    .filter((diagnostic): diagnostic is Diagnostic => diagnostic !== null);
  const identity = {
    schemaVersion: "1.0",
    projectIrId: projectIr.projectIrId,
    source,
    target,
    decisions,
    summary,
  };
  return {
    schemaVersion: "1.0",
    analysisId: `cap_${sha256Json(identity).slice(0, 24)}`,
    projectIrId: projectIr.projectIrId,
    source,
    target,
    decisions,
    summary,
    diagnostics,
  };
}

