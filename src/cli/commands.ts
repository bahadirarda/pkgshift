import {
  PACKAGE_MANAGERS,
  normalizePackageManagerId,
} from "../adapters/catalog.ts";
import {
  ArtifactStoreError,
  PlanArtifactStore,
} from "../artifacts/plan-artifact-store.ts";
import { analyzeCapabilities } from "../capabilities/analyze-capabilities.ts";
import { explainDiagnostic } from "../diagnostics/catalog.ts";
import { applyPlan, ApplyFailure } from "../execution/apply-plan.ts";
import type {
  CommandExecution,
  CommandResult,
  Diagnostic,
  MigrationPlan,
  ResultArtifact,
} from "../domain/models.ts";
import { SCHEMA_VERSION } from "../domain/models.ts";
import { inspectProject } from "../inspect/inspect-project.ts";
import { buildProjectIR } from "../ir/build-project-ir.ts";
import { planPackageManagerMigration } from "../plan/plan-package-manager.ts";
import { rollbackRun, RollbackFailure } from "../recovery/rollback-run.ts";
import { verifyRun, VerificationFailure } from "../verification/verify-run.ts";
import { directoryExists } from "../core/files.ts";
import { resolve } from "node:path";
import {
  inspectSkill,
  installSkill,
  SkillInstallerError,
  uninstallSkill,
} from "../skills/installer.ts";
import type { SkillClient, SkillInstallMode, SkillScope } from "../skills/models.ts";
import { JournalStore } from "../journal/journal-store.ts";
import { VerificationStore } from "../verification/verification-store.ts";
import { ExecutionStore } from "../execution/execution-store.ts";
import type { ParsedArguments } from "./parse-arguments.ts";

export interface GuidedApprovalRequest {
  planId: string;
  source: string;
  target: string;
  files: number;
  operations: number;
  warnings: number;
  lossyDecisions: number;
}

export interface CommandContext {
  requestApproval?: (request: GuidedApprovalRequest) => Promise<boolean>;
}

function result(
  command: string,
  status: CommandResult["status"],
  summary: Record<string, unknown>,
  diagnostics: Diagnostic[] = [],
  artifacts: ResultArtifact[] = [],
  planId: string | null = null,
  runId: string | null = null,
  nextActions: CommandResult["nextActions"] = [],
): CommandResult {
  return {
    schemaVersion: SCHEMA_VERSION,
    command,
    status,
    planId,
    runId,
    summary,
    artifacts,
    diagnostics,
    nextActions,
  };
}

function invalidInput(command: string, messages: string[]): CommandExecution {
  const diagnostics = messages.map((summary) => ({
    code: "CLI_INVALID_INPUT",
    severity: "error" as const,
    summary,
    blocking: true,
    remediation: ["Run pkgshift help to review the command grammar."],
  }));
  return {
    exitCode: 2,
    result: result(command, "blocked", { errors: diagnostics.length }, diagnostics),
  };
}

async function inspectCommand(parsed: ParsedArguments): Promise<CommandExecution> {
  const inspection = await inspectProject(parsed.options.cwd);
  const projectIr = await buildProjectIR(inspection);
  const diagnostics = projectIr?.diagnostics ?? inspection.diagnostics;
  const blocked = diagnostics.some((diagnostic) => diagnostic.blocking);
  const artifactId = `inspection_${inspection.fingerprint.slice("sha256:".length, "sha256:".length + 24)}`;
  const artifacts: ResultArtifact[] = [{
    id: artifactId,
    type: "project-inspection",
    mediaType: "application/vnd.pkgshift.inspection+json",
    content: inspection,
  }];
  if (projectIr) {
    artifacts.push({
      id: projectIr.projectIrId,
      type: "project-ir",
      mediaType: "application/vnd.pkgshift.project-ir+json",
      content: projectIr,
    });
  }
  return {
    exitCode: blocked ? 3 : 0,
    result: result(
      "inspect package-manager",
      blocked ? "blocked" : "completed",
      {
        root: inspection.root,
        fingerprint: inspection.fingerprint,
        selected: inspection.packageManager.selected,
        candidates: inspection.packageManager.candidates.length,
        workspace: inspection.workspace.configured,
        integrations: inspection.integrations.length,
        packages: projectIr?.packages.length ?? 0,
        features: projectIr?.features.length ?? 0,
      },
      diagnostics,
      artifacts,
    ),
  };
}

async function planCommand(parsed: ParsedArguments): Promise<CommandExecution> {
  if (!parsed.options.target) {
    return invalidInput("plan package-manager", ["--to is required for a package manager plan."]);
  }
  const target = normalizePackageManagerId(parsed.options.target);
  if (!target) {
    const diagnostic: Diagnostic = {
      code: "PM_TARGET_UNSUPPORTED",
      severity: "error",
      summary: `Unsupported package manager target: ${parsed.options.target}`,
      blocking: true,
      remediation: ["Run pkgshift support --json and use a listed adapter identifier."],
    };
    return {
      exitCode: 3,
      result: result("plan package-manager", "unsupported", { target: parsed.options.target }, [diagnostic]),
    };
  }

  const inspection = await inspectProject(parsed.options.cwd);
  const projectIr = await buildProjectIR(inspection);
  const capabilityAnalysis = projectIr
    ? analyzeCapabilities(projectIr, target)
    : null;
  const plan = projectIr && capabilityAnalysis
    ? await planPackageManagerMigration(
        inspection,
        projectIr,
        capabilityAnalysis,
        target,
        { acceptedLossy: parsed.options.acceptLossy },
      )
    : null;
  if (!projectIr || !capabilityAnalysis || !plan) {
    const diagnostics = projectIr?.diagnostics ?? inspection.diagnostics;
    return {
      exitCode: 3,
      result: result(
        "plan package-manager",
        "blocked",
        { target, fingerprint: inspection.fingerprint },
        diagnostics,
        [{
          id: `inspection_${inspection.fingerprint.slice("sha256:".length, "sha256:".length + 24)}`,
          type: "project-inspection",
          mediaType: "application/vnd.pkgshift.inspection+json",
          content: inspection,
        }],
      ),
    };
  }

  const blocked = plan.diagnostics.some((diagnostic) => diagnostic.blocking);
  const artifacts: ResultArtifact[] = [
    {
      id: projectIr.projectIrId,
      type: "project-ir",
      mediaType: "application/vnd.pkgshift.project-ir+json",
      content: projectIr,
    },
    {
      id: capabilityAnalysis.analysisId,
      type: "capability-analysis",
      mediaType: "application/vnd.pkgshift.capability-analysis+json",
      content: capabilityAnalysis,
    },
    {
      id: plan.planId,
      type: "package-manager-plan",
      mediaType: "application/vnd.pkgshift.plan+json",
      content: plan,
    },
  ];
  let artifactStored = false;
  if (parsed.options.stateDirectory) {
    try {
      const store = new PlanArtifactStore(parsed.options.stateDirectory);
      const reference = await store.save(inspection.root, {
        schemaVersion: "1.0",
        plan,
        projectIr,
        capabilityAnalysis,
      });
      artifacts.push({
        id: `stored_${plan.planId.slice("plan_".length)}`,
        type: "stored-artifact-reference",
        mediaType: "application/vnd.pkgshift.artifact-reference+json",
        content: reference,
      });
      artifactStored = true;
    } catch (error) {
      const diagnostic: Diagnostic = {
        code: "ARTIFACT_STORE_FAILED",
        severity: "error",
        summary: error instanceof ArtifactStoreError
          ? error.message
          : "The plan artifact could not be persisted.",
        blocking: true,
        remediation: ["Check --state-dir and retry persistence before apply."],
      };
      return {
        exitCode: 8,
        result: result(
          "plan package-manager",
          "failed",
          {
            source: plan.source,
            target: plan.target,
            artifactStored: false,
            executionAvailable: plan.executable,
          },
          [...plan.diagnostics, diagnostic],
          artifacts,
          plan.planId,
        ),
      };
    }
  }
  const nextActions = artifactStored && plan.executable && parsed.options.stateDirectory
    ? [{
        argv: [
          "pkgshift",
          "apply",
          plan.planId,
          "--state-dir",
          parsed.options.stateDirectory,
          "--approve",
          plan.planId,
          "--json",
          "--no-color",
          "--non-interactive",
        ],
        requiresApproval: true,
        sideEffect: "repository-write" as const,
      }]
    : [];
  return {
    exitCode: blocked ? 3 : 0,
    result: result(
      "plan package-manager",
      blocked ? "blocked" : "planned",
      {
        source: plan.source,
        target: plan.target,
        targetTier: plan.targetTier,
        operations: plan.operations.length,
        warnings: plan.diagnostics.filter((diagnostic) => diagnostic.severity === "warning").length,
        capabilities: plan.capabilitySummary,
        artifactStored,
        executionAvailable: plan.executable,
      },
      plan.diagnostics,
      artifacts,
      plan.planId,
      null,
      nextActions,
    ),
  };
}

function guidedNextAction(
  parsed: ParsedArguments,
  target: string,
  planId: string,
): CommandResult["nextActions"][number] {
  const argv = ["pkgshift", "to", target];
  if (parsed.options.acceptLossy) argv.push("--accept-lossy");
  if (parsed.options.stateDirectory) {
    argv.push("--state-dir", parsed.options.stateDirectory);
  }
  argv.push(
    "--approve",
    planId,
    "--json",
    "--no-color",
    "--non-interactive",
  );
  return {
    argv,
    requiresApproval: true,
    sideEffect: "repository-write",
  };
}

function guidedResult(
  execution: CommandExecution,
  target: string,
  additions: Record<string, unknown> = {},
  nextActions = execution.result.nextActions,
): CommandExecution {
  return {
    exitCode: execution.exitCode,
    result: {
      ...execution.result,
      command: `to ${target}`,
      summary: { ...execution.result.summary, ...additions },
      nextActions,
    },
  };
}

async function guidedMigrationCommand(
  parsed: ParsedArguments,
  context: CommandContext,
): Promise<CommandExecution> {
  const targetValue = parsed.options.target;
  if (!targetValue) return invalidInput("to", ["A target package manager is required."]);
  if (parsed.positional.length !== 1) {
    return invalidInput("to", ["Use pkgshift to <target> without additional positional arguments."]);
  }
  if (parsed.options.dryRun && parsed.options.approval) {
    return invalidInput("to", ["--dry-run cannot be combined with --approve."]);
  }

  const planningInput: ParsedArguments = {
    ...parsed,
    options: {
      ...parsed.options,
      approval: null,
      stateDirectory: null,
    },
    positional: ["plan", "package-manager"],
  };
  const planned = await planCommand(planningInput);
  const planArtifact = planned.result.artifacts.find((entry) =>
    entry.type === "package-manager-plan"
  );
  const plan = planArtifact?.content as MigrationPlan | undefined;
  if (!plan || planned.exitCode !== 0 || !plan.executable) {
    return guidedResult(planned, targetValue, {
      guided: true,
      dryRun: parsed.options.dryRun,
      repositoryChanged: false,
    }, []);
  }

  const files = new Set(
    plan.operations.flatMap((operation) =>
      (operation.mutations ?? []).map((mutation) => mutation.path)
    ),
  ).size;
  const request: GuidedApprovalRequest = {
    planId: plan.planId,
    source: plan.source,
    target: plan.target,
    files,
    operations: plan.operations.length,
    warnings: plan.diagnostics.filter((entry) => entry.severity === "warning").length,
    lossyDecisions: plan.capabilitySummary.lossy,
  };
  const nextAction = guidedNextAction(parsed, plan.target, plan.planId);
  if (parsed.options.dryRun) {
    return guidedResult(planned, plan.target, {
      guided: true,
      dryRun: true,
      files,
      repositoryChanged: false,
    }, [nextAction]);
  }

  let approved = parsed.options.approval === plan.planId;
  if (!parsed.options.approval && context.requestApproval) {
    approved = await context.requestApproval(request);
    if (!approved) {
      return guidedResult(planned, plan.target, {
        guided: true,
        approval: "declined",
        files,
        repositoryChanged: false,
      }, [nextAction]);
    }
  }
  if (!approved) {
    const diagnostic: Diagnostic = {
      code: "APPROVAL_REQUIRED",
      severity: "error",
      summary: `Guided migration requires exact approval for ${plan.planId}.`,
      blocking: true,
      remediation: [`Retry with pkgshift to ${plan.target} --approve ${plan.planId}.`],
    };
    return {
      exitCode: 7,
      result: result(
        `to ${plan.target}`,
        "planned",
        {
          source: plan.source,
          target: plan.target,
          guided: true,
          files,
          repositoryChanged: false,
        },
        [...plan.diagnostics, diagnostic],
        planned.result.artifacts,
        plan.planId,
        null,
        [nextAction],
      ),
    };
  }

  const stateDirectory = parsed.options.stateDirectory
    ?? resolve(parsed.options.cwd, ".pkgshift/state");
  const executionInput: ParsedArguments = {
    ...parsed,
    options: {
      ...parsed.options,
      approval: plan.planId,
      stateDirectory,
    },
  };
  const persisted = await planCommand({
    ...executionInput,
    positional: ["plan", "package-manager"],
  });
  if (persisted.exitCode !== 0 || persisted.result.planId !== plan.planId) {
    return guidedResult(persisted, plan.target, {
      guided: true,
      repositoryChanged: false,
    });
  }

  const applied = await applyCommand(executionInput, plan.planId);
  if (applied.exitCode !== 0 || !applied.result.runId) {
    return guidedResult(applied, plan.target, {
      source: plan.source,
      target: plan.target,
      guided: true,
      files,
    });
  }
  const verified = await verifyCommand(executionInput, applied.result.runId);
  if (verified.exitCode !== 0) {
    return guidedResult(verified, plan.target, {
      source: plan.source,
      target: plan.target,
      guided: true,
      files,
    });
  }

  return {
    exitCode: 0,
    result: result(
      `to ${plan.target}`,
      "completed",
      {
        source: plan.source,
        target: plan.target,
        guided: true,
        files,
        operations: plan.operations.length,
        runStatus: verified.result.summary.runStatus,
        checks: verified.result.summary.checks,
        passed: verified.result.summary.passed,
        failed: verified.result.summary.failed,
        skipped: verified.result.summary.skipped,
      },
      [
        ...plan.diagnostics,
        ...applied.result.diagnostics,
        ...verified.result.diagnostics,
      ],
      [
        ...persisted.result.artifacts,
        ...applied.result.artifacts,
        ...verified.result.artifacts,
      ],
      plan.planId,
      applied.result.runId,
    ),
  };
}

function stateDirectoryRequired(command: string): CommandExecution {
  return invalidInput(command, ["--state-dir is required for persisted migration state."]);
}

function executionFailure(
  command: string,
  error: unknown,
  planId: string | null = null,
  runId: string | null = null,
): CommandExecution {
  const diagnostic = error instanceof ApplyFailure
    || error instanceof VerificationFailure
    || error instanceof RollbackFailure
    ? error.diagnostic
    : {
        code: error instanceof ArtifactStoreError ? error.code : "PKGSHIFT_INTERNAL_ERROR",
        severity: "error" as const,
        summary: error instanceof Error ? error.message : "The operation failed before producing a trustworthy artifact.",
        blocking: true,
        remediation: ["Preserve the state directory and inspect the reported diagnostic."],
      };
  const approvalRequired = diagnostic.code === "APPROVAL_REQUIRED";
  return {
    exitCode: approvalRequired ? 7 : diagnostic.code === "PLAN_PRECONDITION_FAILED" ? 4 : 8,
    result: result(
      command,
      "failed",
      { trustworthyResult: diagnostic.code !== "PKGSHIFT_INTERNAL_ERROR" },
      [diagnostic],
      [],
      planId,
      runId,
    ),
  };
}

async function applyCommand(
  parsed: ParsedArguments,
  planId: string | undefined,
): Promise<CommandExecution> {
  if (!planId) return invalidInput("apply", ["A plan identifier is required."]);
  if (!parsed.options.stateDirectory) return stateDirectoryRequired("apply");
  try {
    const execution = await applyPlan({
      projectRoot: parsed.options.cwd,
      stateDirectory: parsed.options.stateDirectory,
      planId,
      approval: parsed.options.approval,
    });
    const failed = execution.journal.status === "failed";
    const recoverable = !execution.diagnostics.some((entry) => entry.code === "SNAPSHOT_CREATE_FAILED");
    const nextActions: CommandResult["nextActions"] = failed && recoverable
      ? [{
          argv: [
            "pkgshift", "rollback", execution.journal.runId,
            "--state-dir", parsed.options.stateDirectory,
            "--approve", execution.journal.runId,
            "--json", "--no-color", "--non-interactive",
          ],
          requiresApproval: true,
          sideEffect: "repository-write",
        }]
      : failed
        ? []
        : [{
          argv: [
            "pkgshift", "verify", execution.journal.runId,
            "--state-dir", parsed.options.stateDirectory,
            "--json", "--no-color", "--non-interactive",
          ],
          requiresApproval: false,
          sideEffect: "none",
        }];
    return {
      exitCode: failed ? 5 : 0,
      result: result(
        "apply",
        failed ? "failed" : "completed",
        {
          runStatus: execution.journal.status,
          operations: execution.journal.operations.length,
          processes: execution.processes.length,
          verificationRequired: !failed,
          rollbackAvailable: failed && recoverable,
        },
        execution.diagnostics,
        [
          {
            id: execution.journal.runId,
            type: "run-journal",
            mediaType: "application/vnd.pkgshift.run-journal+json",
            content: execution.journal,
          },
          ...execution.processes.map((process, index) => ({
            id: `process_${execution.journal.runId.slice("run_".length)}_${index + 1}`,
            type: "process-execution",
            mediaType: "application/vnd.pkgshift.process+json",
            content: process,
          })),
        ],
        planId,
        execution.journal.runId,
        nextActions,
      ),
    };
  } catch (error) {
    return executionFailure("apply", error, planId);
  }
}

async function verifyCommand(
  parsed: ParsedArguments,
  runId: string | undefined,
): Promise<CommandExecution> {
  if (!runId) return invalidInput("verify", ["A run identifier is required."]);
  if (!parsed.options.stateDirectory) return stateDirectoryRequired("verify");
  try {
    const execution = await verifyRun({
      projectRoot: parsed.options.cwd,
      stateDirectory: parsed.options.stateDirectory,
      runId,
    });
    const failed = execution.report.status === "failed";
    const nextActions: CommandResult["nextActions"] = failed
      ? [{
          argv: [
            "pkgshift", "rollback", runId,
            "--state-dir", parsed.options.stateDirectory,
            "--approve", runId,
            "--json", "--no-color", "--non-interactive",
          ],
          requiresApproval: true,
          sideEffect: "repository-write",
        }]
      : [];
    return {
      exitCode: failed ? 6 : 0,
      result: result(
        "verify",
        failed ? "failed" : "completed",
        {
          runStatus: execution.journal.status,
          checks: execution.report.checks.length,
          passed: execution.report.checks.filter((entry) => entry.status === "passed").length,
          failed: execution.report.checks.filter((entry) => entry.status === "failed").length,
          skipped: execution.report.checks.filter((entry) => entry.status === "skipped").length,
        },
        execution.report.diagnostics,
        [{
          id: execution.report.reportId,
          type: "verification-report",
          mediaType: "application/vnd.pkgshift.verification+json",
          content: execution.report,
        }],
        execution.journal.planId,
        runId,
        nextActions,
      ),
    };
  } catch (error) {
    return executionFailure("verify", error, null, runId);
  }
}

async function rollbackCommand(
  parsed: ParsedArguments,
  runId: string | undefined,
): Promise<CommandExecution> {
  if (!runId) return invalidInput("rollback", ["A run identifier is required."]);
  if (!parsed.options.stateDirectory) return stateDirectoryRequired("rollback");
  try {
    const execution = await rollbackRun({
      projectRoot: parsed.options.cwd,
      stateDirectory: parsed.options.stateDirectory,
      runId,
      approval: parsed.options.approval,
    });
    const failed = execution.journal.status === "rollback-failed";
    return {
      exitCode: failed ? 5 : 0,
      result: result(
        "rollback",
        failed ? "failed" : "rolled-back",
        {
          runStatus: execution.journal.status,
          repositoryFilesRestored: !failed,
          externalDependencyStateRestored: false,
        },
        execution.diagnostics,
        [{
          id: execution.journal.runId,
          type: "run-journal",
          mediaType: "application/vnd.pkgshift.run-journal+json",
          content: execution.journal,
        }],
        execution.journal.planId,
        runId,
      ),
    };
  } catch (error) {
    return executionFailure("rollback", error, null, runId);
  }
}

async function resolveSkillSource(cwd: string): Promise<string> {
  const candidates = [
    resolve(import.meta.dir, "../../skills/pkgshift"),
    resolve(import.meta.dir, "../skills/pkgshift"),
    resolve(cwd, "skills/pkgshift"),
  ];
  for (const candidate of candidates) {
    if (await directoryExists(candidate)) return candidate;
  }
  return candidates[0]!;
}

async function skillCommand(
  parsed: ParsedArguments,
  operation: string | undefined,
): Promise<CommandExecution> {
  const command = `skill ${operation ?? "status"}`;
  if (!(["project", "user"] as string[]).includes(parsed.options.scope)) {
    return invalidInput(command, ["--scope must be project or user."]);
  }
  if (!(["copy", "link"] as string[]).includes(parsed.options.installMode)) {
    return invalidInput(command, ["--mode must be copy or link."]);
  }
  if (!(["codex", "claude"] as string[]).includes(parsed.options.client)) {
    return invalidInput(command, ["--client must be codex or claude."]);
  }
  const scope = parsed.options.scope as SkillScope;
  const mode = parsed.options.installMode as SkillInstallMode;
  const client = parsed.options.client as SkillClient;
  const sourcePath = await resolveSkillSource(parsed.options.cwd);
  try {
    const mutating = operation === "install" || operation === "uninstall";
    const before = mutating
      ? await inspectSkill({ projectRoot: parsed.options.cwd, sourcePath, scope, client })
      : null;
    const status = operation === "install"
      ? await installSkill({
          projectRoot: parsed.options.cwd,
          sourcePath,
          scope,
          client,
          mode,
          approval: parsed.options.approval,
        })
      : operation === "uninstall"
        ? await uninstallSkill({
            projectRoot: parsed.options.cwd,
            sourcePath,
            scope,
            client,
            approval: parsed.options.approval,
          })
        : operation === "status" || operation === "doctor" || !operation
          ? await inspectSkill({ projectRoot: parsed.options.cwd, sourcePath, scope, client })
          : null;
    if (!status) return invalidInput(command, [`Unknown skill operation: ${operation}`]);
    const blocked = status.diagnostics.some((entry) => entry.blocking);
    const mutationPerformed = operation === "install"
      ? before?.installed === false && status.installed
      : operation === "uninstall"
        ? before?.installed === true && !status.installed
        : false;
    return {
      exitCode: blocked ? 3 : 0,
      result: result(
        command,
        blocked ? "blocked" : "completed",
        {
          scope,
          client,
          installed: status.installed,
          healthy: status.healthy,
          modified: status.modified,
          mode: status.mode,
          targetPath: status.targetPath,
          mutationPerformed,
        },
        status.diagnostics,
        [{
          id: `skill_${status.name}_${scope}_${client}`,
          type: "skill-status",
          mediaType: "application/vnd.pkgshift.skill-status+json",
          content: status,
        }],
      ),
    };
  } catch (error) {
    const diagnostic = error instanceof SkillInstallerError
      ? error.diagnostic
      : {
          code: "SKILL_OPERATION_FAILED",
          severity: "error" as const,
          summary: error instanceof Error ? error.message : "Skill operation failed.",
          blocking: true,
          remediation: ["Run pkgshift skill doctor and resolve the reported installation state."],
        };
    return {
      exitCode: diagnostic.code === "APPROVAL_REQUIRED" ? 7 : 8,
      result: result(command, "failed", { scope }, [diagnostic]),
    };
  }
}

function supportCommand(): CommandExecution {
  return {
    exitCode: 0,
    result: result(
      "support package-managers",
      "completed",
      {
        productionTargets: PACKAGE_MANAGERS.filter((manager) => manager.tier === "production-target").length,
        previewTargets: PACKAGE_MANAGERS.filter((manager) => manager.tier === "preview-target").length,
        implementationStatus: "mvp",
      },
      [],
      [{
        id: "package-manager-support",
        type: "package-manager-support",
        mediaType: "application/vnd.pkgshift.support+json",
        content: PACKAGE_MANAGERS,
      }],
    ),
  };
}

async function explainCommand(
  parsed: ParsedArguments,
  code: string | undefined,
): Promise<CommandExecution> {
  if (!code) {
    return invalidInput("explain", ["A diagnostic code is required."]);
  }
  const explanation = explainDiagnostic(code);
  if (!explanation && parsed.options.stateDirectory && code.startsWith("plan_")) {
    try {
      const stored = await new PlanArtifactStore(parsed.options.stateDirectory).load(
        parsed.options.cwd,
        code,
      );
      return {
        exitCode: 0,
        result: result(
          "explain",
          "completed",
          {
            artifact: code,
            type: stored.type,
            digest: stored.digest,
            executable: stored.content.plan.executable,
          },
          stored.content.plan.diagnostics,
          [{
            id: code,
            type: "package-manager-plan-bundle",
            mediaType: stored.mediaType,
            content: stored.content,
          }],
          code,
        ),
      };
    } catch (error) {
      return executionFailure("explain", error, code);
    }
  }
  if (!explanation && parsed.options.stateDirectory && code.startsWith("run_")) {
    try {
      const journal = await new JournalStore(parsed.options.stateDirectory).load(code);
      const artifacts: ResultArtifact[] = [{
        id: code,
        type: "run-journal",
        mediaType: "application/vnd.pkgshift.run-journal+json",
        content: journal,
      }];
      try {
        const execution = await new ExecutionStore(parsed.options.stateDirectory).load(code);
        artifacts.push({
          id: `execution_${code.slice("run_".length)}`,
          type: "execution-report",
          mediaType: "application/vnd.pkgshift.execution+json",
          content: execution,
        });
      } catch {
        // A run may fail before launching an external process.
      }
      try {
        const verification = await new VerificationStore(parsed.options.stateDirectory).load(code);
        artifacts.push({
          id: verification.reportId,
          type: "verification-report",
          mediaType: "application/vnd.pkgshift.verification+json",
          content: verification,
        });
      } catch {
        // Verification is optional for incomplete and failed apply runs.
      }
      return {
        exitCode: 0,
        result: result(
          "explain",
          "completed",
          { artifact: code, type: "run-journal", runStatus: journal.status },
          [],
          artifacts,
          journal.planId,
          code,
        ),
      };
    } catch (error) {
      return executionFailure("explain", error, null, code);
    }
  }
  if (!explanation) {
    const diagnostic: Diagnostic = {
      code: "DIAGNOSTIC_CODE_UNKNOWN",
      severity: "error",
      summary: `Unknown diagnostic code: ${code}`,
      blocking: true,
      remediation: ["Use a diagnostic code returned by the current CLI schema."],
    };
    return {
      exitCode: 2,
      result: result("explain", "blocked", { code }, [diagnostic]),
    };
  }
  return {
    exitCode: 0,
    result: result(
      "explain",
      "completed",
      { code: explanation.code, title: explanation.title },
      [],
      [{
        id: `explanation_${explanation.code}`,
        type: "diagnostic-explanation",
        mediaType: "application/vnd.pkgshift.diagnostic+json",
        content: explanation,
      }],
    ),
  };
}

function helpCommand(): CommandExecution {
  return {
    exitCode: 0,
    result: result(
      "help",
      "completed",
      {
        usage: [
          "pkgshift to <target> [--dry-run]",
          "pkgshift inspect [package-manager]",
          "pkgshift plan package-manager --to <target>",
          "pkgshift pm to <target>",
          "pkgshift apply <plan-id> --state-dir <path> --approve <plan-id>",
          "pkgshift verify <run-id> --state-dir <path>",
          "pkgshift rollback <run-id> --state-dir <path> --approve <run-id>",
          "pkgshift skill install|status|doctor|uninstall [--scope project|user] [--client codex|claude]",
          "pkgshift support [package-managers]",
          "pkgshift explain <diagnostic-code>",
        ],
        unavailable: [],
        globalOptions: [
          "--cwd <path>",
          "--json",
          "--no-color",
          "--non-interactive",
          "--quiet",
          "--state-dir <path>",
          "--approve <identifier>",
          "--accept-lossy",
          "--dry-run",
          "--scope <project|user>",
          "--mode <copy|link>",
          "--client <codex|claude>",
          "--help",
          "--version",
        ],
      },
    ),
  };
}

function versionCommand(): CommandExecution {
  return {
    exitCode: 0,
    result: result("version", "completed", { version: "0.1.0" }),
  };
}

export async function executeCommand(
  parsed: ParsedArguments,
  context: CommandContext = {},
): Promise<CommandExecution> {
  const [command, subject] = parsed.positional;
  if (parsed.errors.length > 0) {
    return invalidInput(command ?? "unknown", parsed.errors);
  }
  if (parsed.options.help) {
    return helpCommand();
  }
  if (parsed.options.version) {
    return versionCommand();
  }
  if (!command || command === "help") {
    return helpCommand();
  }
  if (command === "inspect" && (!subject || subject === "package-manager")) {
    return inspectCommand(parsed);
  }
  if (command === "plan" && subject === "package-manager") {
    return planCommand(parsed);
  }
  if (command === "to") {
    return guidedMigrationCommand(parsed, context);
  }
  if (command === "support" && (!subject || subject === "package-managers")) {
    return supportCommand();
  }
  if (command === "explain") {
    return explainCommand(parsed, subject);
  }
  if (command === "apply") {
    return applyCommand(parsed, subject);
  }
  if (command === "verify") {
    return verifyCommand(parsed, subject);
  }
  if (command === "rollback") {
    return rollbackCommand(parsed, subject);
  }
  if (command === "skill") {
    return skillCommand(parsed, subject);
  }
  return invalidInput(command, [`Unknown command: ${parsed.positional.join(" ")}`]);
}
