import { join } from "node:path";
import type {
  Diagnostic,
  PackageManagerCandidate,
  PackageManagerDetection,
  PackageManagerEvidence,
  PackageManagerId,
} from "../domain/models.ts";
import { pathExists } from "../core/files.ts";

interface EvidenceInput {
  manager: PackageManagerId;
  kind: PackageManagerEvidence["kind"];
  location: string;
  detail: string;
  weight: number;
}

function managerFromPackageManagerField(
  value: string,
): PackageManagerId | null {
  const separator = value.lastIndexOf("@");
  if (separator <= 0) {
    return null;
  }
  const name = value.slice(0, separator).toLowerCase();
  const version = value.slice(separator + 1);
  if (name === "yarn") {
    return /^1(?:\.|$)/.test(version) ? "yarn-classic" : "yarn-modern";
  }
  if (name === "npm" || name === "pnpm" || name === "bun" || name === "vlt") {
    return name;
  }
  if (name === "deno") {
    return "deno";
  }
  return null;
}

function confidenceForScore(
  score: number,
): PackageManagerCandidate["confidence"] {
  if (score >= 100) {
    return "high";
  }
  if (score >= 70) {
    return "medium";
  }
  return "low";
}

export async function detectPackageManager(
  root: string,
  manifest: Record<string, unknown> | null,
): Promise<PackageManagerDetection> {
  const evidence: PackageManagerEvidence[] = [];
  const diagnostics: Diagnostic[] = [];

  const addEvidence = (input: EvidenceInput): void => {
    evidence.push(input);
  };

  if (typeof manifest?.packageManager === "string") {
    const manager = managerFromPackageManagerField(manifest.packageManager);
    if (manager) {
      addEvidence({
        manager,
        kind: "manifest",
        location: "package.json",
        detail: `packageManager declares ${manifest.packageManager}`,
        weight: 120,
      });
    } else {
      diagnostics.push({
        code: "PM_PACKAGE_MANAGER_FIELD_UNKNOWN",
        severity: "warning",
        summary: `The packageManager field is not recognized: ${manifest.packageManager}`,
        blocking: false,
        evidence: [{ location: "package.json", detail: "Unrecognized packageManager value" }],
        remediation: ["Use a supported package manager name with an explicit version."],
      });
    }
  }

  const fileSignals: EvidenceInput[] = [
    { manager: "npm", kind: "lockfile", location: "package-lock.json", detail: "npm lockfile exists", weight: 80 },
    { manager: "npm", kind: "lockfile", location: "npm-shrinkwrap.json", detail: "npm shrinkwrap exists", weight: 90 },
    { manager: "pnpm", kind: "lockfile", location: "pnpm-lock.yaml", detail: "pnpm lockfile exists", weight: 80 },
    { manager: "pnpm", kind: "workspace", location: "pnpm-workspace.yaml", detail: "pnpm workspace configuration exists", weight: 45 },
    { manager: "pnpm", kind: "configuration", location: ".pnpmfile.cjs", detail: "pnpm hook configuration exists", weight: 45 },
    { manager: "yarn-classic", kind: "configuration", location: ".yarnrc", detail: "Yarn Classic configuration exists", weight: 75 },
    { manager: "yarn-modern", kind: "configuration", location: ".yarnrc.yml", detail: "Yarn Modern configuration exists", weight: 90 },
    { manager: "yarn-modern", kind: "configuration", location: ".pnp.cjs", detail: "Yarn Plug and Play loader exists", weight: 70 },
    { manager: "bun", kind: "lockfile", location: "bun.lock", detail: "Bun text lockfile exists", weight: 85 },
    { manager: "bun", kind: "lockfile", location: "bun.lockb", detail: "Bun binary lockfile exists", weight: 80 },
    { manager: "bun", kind: "configuration", location: "bunfig.toml", detail: "Bun configuration exists", weight: 40 },
    { manager: "vlt", kind: "lockfile", location: "vlt-lock.json", detail: "vlt lockfile exists", weight: 85 },
    { manager: "vlt", kind: "configuration", location: "vlt.json", detail: "vlt configuration exists", weight: 40 },
    { manager: "deno", kind: "lockfile", location: "deno.lock", detail: "Deno lockfile exists", weight: 55 },
    { manager: "deno", kind: "configuration", location: "deno.json", detail: "Deno configuration exists", weight: 55 },
    { manager: "deno", kind: "configuration", location: "deno.jsonc", detail: "Deno JSONC configuration exists", weight: 55 },
  ];

  for (const signal of fileSignals) {
    if (await pathExists(join(root, signal.location))) {
      addEvidence(signal);
    }
  }

  if (await pathExists(join(root, "yarn.lock"))) {
    const modernEvidence = evidence.some(
      (item) => item.manager === "yarn-modern" && item.kind === "configuration",
    );
    const classicEvidence = evidence.some(
      (item) => item.manager === "yarn-classic" && item.kind === "configuration",
    );
    if (modernEvidence) {
      addEvidence({ manager: "yarn-modern", kind: "lockfile", location: "yarn.lock", detail: "Yarn lockfile is paired with modern configuration", weight: 70 });
    } else if (classicEvidence) {
      addEvidence({ manager: "yarn-classic", kind: "lockfile", location: "yarn.lock", detail: "Yarn lockfile is paired with classic configuration", weight: 70 });
    } else {
      addEvidence({ manager: "yarn-classic", kind: "lockfile", location: "yarn.lock", detail: "Yarn lockfile version is not disambiguated", weight: 60 });
      addEvidence({ manager: "yarn-modern", kind: "lockfile", location: "yarn.lock", detail: "Yarn lockfile version is not disambiguated", weight: 60 });
    }
  }

  const candidates = new Map<PackageManagerId, PackageManagerEvidence[]>();
  for (const item of evidence) {
    const existing = candidates.get(item.manager) ?? [];
    existing.push(item);
    candidates.set(item.manager, existing);
  }

  const ranked: PackageManagerCandidate[] = [...candidates.entries()]
    .map(([manager, managerEvidence]) => {
      const score = managerEvidence.reduce((sum, item) => sum + item.weight, 0);
      return {
        manager,
        score,
        confidence: confidenceForScore(score),
        evidence: managerEvidence.sort((left, right) => left.location.localeCompare(right.location)),
      };
    })
    .sort((left, right) => right.score - left.score || left.manager.localeCompare(right.manager));

  const first = ranked[0];
  const second = ranked[1];
  let selected: PackageManagerId | null = null;

  if (!first) {
    diagnostics.push({
      code: "PM_SOURCE_NOT_DETECTED",
      severity: "error",
      summary: "No supported package manager evidence was detected.",
      blocking: true,
      remediation: ["Add an explicit packageManager field or select a source when that option becomes available."],
    });
  } else if (first.score < 60 || (second && first.score - second.score < 25)) {
    diagnostics.push({
      code: "PM_SOURCE_AMBIGUOUS",
      severity: "error",
      summary: "Package manager evidence is ambiguous.",
      blocking: true,
      evidence: ranked.slice(0, 3).flatMap((candidate) => candidate.evidence.map((item) => ({
        location: item.location,
        detail: `${candidate.manager}: ${item.detail}`,
      }))),
      remediation: ["Review conflicting evidence and make the intended source explicit."],
    });
  } else {
    selected = first.manager;
    const conflicting = ranked.filter(
      (candidate) => candidate.manager !== selected && candidate.score >= 70,
    );
    if (conflicting.length > 0) {
      diagnostics.push({
        code: "PM_CONFLICTING_EVIDENCE",
        severity: "warning",
        summary: `${selected} was selected, but other strong package manager evidence exists.`,
        blocking: false,
        evidence: conflicting.flatMap((candidate) => candidate.evidence.map((item) => ({
          location: item.location,
          detail: `${candidate.manager}: ${item.detail}`,
        }))),
        remediation: ["Confirm that the additional lockfiles or configuration are stale before apply."],
      });
    }
  }

  return {
    selected,
    candidates: ranked,
    evidence: evidence.sort((left, right) => left.location.localeCompare(right.location) || left.manager.localeCompare(right.manager)),
    diagnostics,
  };
}

