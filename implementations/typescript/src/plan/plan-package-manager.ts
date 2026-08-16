import {
  getPackageManager,
  nativeImportStrategy,
} from "../adapters/catalog.ts";
import type { CapabilityAnalysis } from "../capabilities/models.ts";
import { sha256Json } from "../core/files.ts";
import type {
  Diagnostic,
  MigrationPlan,
  PackageManagerId,
  PlannedFileMutation,
  PlannedOperation,
  ProjectInspection,
} from "../domain/models.ts";
import type { ProjectIR } from "../ir/models.ts";
import { transformProject } from "../transform/transform-project.ts";

export interface PlanOptions {
  acceptedLossy?: boolean;
}

function operationId(index: number): string {
  return `op_${String(index + 1).padStart(3, "0")}`;
}

function installCommand(target: PackageManagerId): string[] {
  const base = [...getPackageManager(target).installCommand];
  if (["npm", "pnpm", "yarn-classic", "bun"].includes(target)) {
    return [...base, "--ignore-scripts"];
  }
  if (target === "yarn-modern") {
    return [...base, "--mode=skip-build"];
  }
  return base;
}

function operationForMutations(
  phase: PlannedOperation["phase"],
  kind: string,
  description: string,
  mutations: PlannedFileMutation[],
  preconditions: string[],
  postconditions: string[],
): Omit<PlannedOperation, "id"> | null {
  if (mutations.length === 0) return null;
  return {
    phase,
    kind,
    description,
    paths: mutations.map((entry) => entry.path),
    capabilities: [...new Set(mutations.flatMap((entry) => entry.capabilities))].sort(),
    sideEffect: "repository-write",
    reversible: true,
    preconditions,
    postconditions,
    mutations,
  };
}

export async function planPackageManagerMigration(
  inspection: ProjectInspection,
  projectIr: ProjectIR,
  capabilityAnalysis: CapabilityAnalysis,
  target: PackageManagerId,
  options: PlanOptions = {},
): Promise<MigrationPlan | null> {
  const source = inspection.packageManager.selected;
  if (!source) return null;

  const targetDefinition = getPackageManager(target);
  const sourceDefinition = getPackageManager(source);
  const acceptedLossy = options.acceptedLossy ?? false;
  const sourceLockfilePresent = sourceDefinition.lockfiles.some((path) =>
    inspection.relevantFiles.includes(path)
  );
  const nativeImport = nativeImportStrategy(source, target, sourceLockfilePresent);
  const transformation = await transformProject(
    inspection,
    projectIr,
    capabilityAnalysis,
    target,
  );
  const diagnostics: Diagnostic[] = [
    ...projectIr.diagnostics,
    ...capabilityAnalysis.diagnostics,
    ...transformation.diagnostics,
  ];

  if (source !== target && sourceLockfilePresent && !nativeImport) {
    diagnostics.push({
      code: "NATIVE_IMPORT_UNAVAILABLE",
      severity: "warning",
      summary: `No verified target-native lockfile importer is registered for ${source} to ${target}.`,
      blocking: false,
      remediation: [
        "pkgshift will generate target dependency state and require target verification.",
      ],
    });
  }

  if (targetDefinition.tier === "preview-target") {
    diagnostics.push({
      code: "PM_TARGET_PREVIEW",
      severity: "warning",
      summary: `${targetDefinition.displayName} is a preview migration target.`,
      blocking: false,
      remediation: ["Use the plan for assessment; preview targets cannot be applied."],
    });
  }
  if (source === target) {
    diagnostics.push({
      code: "PM_TARGET_ALREADY_SELECTED",
      severity: "warning",
      summary: `${targetDefinition.displayName} is already the detected package manager.`,
      blocking: false,
      remediation: ["Select another target or verify the current dependency state."],
    });
  }
  if (capabilityAnalysis.summary.lossy > 0 && !acceptedLossy) {
    diagnostics.push({
      code: "LOSSY_ACCEPTANCE_REQUIRED",
      severity: "error",
      summary: "Lossy capability decisions require explicit acceptance in the plan.",
      blocking: true,
      remediation: ["Review every CAPABILITY_LOSSY diagnostic and re-plan with --accept-lossy."],
    });
  }

  const operations: PlannedOperation[] = [];
  const push = (operation: Omit<PlannedOperation, "id"> | null): void => {
    if (operation) operations.push({ id: operationId(operations.length), ...operation });
  };

  if (source !== target) {
    push(operationForMutations(
      "configure",
      "manifest.render-target",
      `Render ${targetDefinition.displayName}-compatible package manifests.`,
      transformation.manifestMutations,
      [`Repository fingerprint equals ${inspection.fingerprint}.`],
      ["Every package manifest matches its planned digest."],
    ));
    push(operationForMutations(
      "configure",
      "configuration.render-target",
      `Render deterministic ${targetDefinition.displayName} configuration.`,
      transformation.configurationMutations,
      ["Source configuration still matches its planned digest."],
      ["Target configuration parses and matches accepted capability decisions."],
    ));
    push(operationForMutations(
      "integrate",
      "integration.translate-commands",
      `Translate recognized ${sourceDefinition.displayName} commands in repository integrations.`,
      transformation.integrationMutations,
      ["Integration files still match their planned digests."],
      ["Recognized source package-manager commands use the target executable."],
    ));

    if (nativeImport?.mode === "dedicated-command") {
      operations.push({
        id: operationId(operations.length),
        phase: "install",
        kind: "dependency.import-target",
        description: nativeImport.summary,
        paths: targetDefinition.lockfiles,
        command: nativeImport.command,
        sideEffect: "dependency-state",
        capabilities: capabilityAnalysis.decisions.map((decision) => decision.featureId),
        reversible: true,
        preconditions: [
          "Source dependency state and target configuration match the accepted plan.",
        ],
        postconditions: ["The target-native importer exits successfully."],
      });
    }

    operations.push({
      id: operationId(operations.length),
      phase: "install",
      kind: nativeImport?.mode === "install-integrated"
        ? "dependency.import-and-install-target"
        : "dependency.install-target",
      description: nativeImport?.mode === "install-integrated"
        ? nativeImport.summary
        : `Generate ${targetDefinition.displayName} dependency state without lifecycle scripts.`,
      paths: targetDefinition.lockfiles,
      command: installCommand(target),
      sideEffect: "dependency-state",
      capabilities: capabilityAnalysis.decisions.map((decision) => decision.featureId),
      reversible: true,
      preconditions: ["Target configuration passes structural validation."],
      postconditions: ["The target install command exits successfully."],
    });

    push(operationForMutations(
      "cleanup",
      "source.retire",
      `Retire source-only ${sourceDefinition.displayName} configuration and lockfiles.`,
      transformation.cleanupMutations,
      ["Target dependency installation completed successfully."],
      ["No planned source-only artifact remains."],
    ));

    operations.push({
      id: operationId(operations.length),
      phase: "verify",
      kind: "migration.verify",
      description: "Verify planned digests, target dependency state, workspace membership, and integrations.",
      paths: [],
      sideEffect: "none",
      capabilities: capabilityAnalysis.decisions.map((decision) => decision.featureId),
      reversible: false,
      preconditions: ["All apply operations completed."],
      postconditions: ["No blocking verification check remains."],
    });
  }

  const executable = source !== target
    && targetDefinition.tier === "production-target"
    && !diagnostics.some((entry) => entry.blocking);
  const identity = {
    schemaVersion: "1.0",
    source,
    target,
    targetTier: targetDefinition.tier,
    repositoryFingerprint: inspection.fingerprint,
    projectIrId: projectIr.projectIrId,
    capabilityAnalysisId: capabilityAnalysis.analysisId,
    capabilitySummary: capabilityAnalysis.summary,
    nativeImport,
    acceptedLossy,
    executable,
    operations,
    diagnostics,
  };
  const planId = `plan_${sha256Json(identity).slice(0, 24)}`;

  return {
    schemaVersion: "1.0",
    planId,
    executable,
    acceptedLossy,
    source,
    target,
    targetTier: targetDefinition.tier,
    repositoryFingerprint: inspection.fingerprint,
    projectIrId: projectIr.projectIrId,
    capabilityAnalysisId: capabilityAnalysis.analysisId,
    capabilitySummary: capabilityAnalysis.summary,
    ...(nativeImport ? { nativeImport } : {}),
    operations,
    diagnostics,
    verification: [
      "planned file digests match",
      "target package manager is selected",
      "target lockfile exists",
      "source-only artifacts are retired",
      "workspace membership is preserved",
      "recognized integration commands use the target",
      "target installation operation succeeded",
    ],
  };
}
