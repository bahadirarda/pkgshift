export const SCHEMA_VERSION = "1.0";

export type PackageManagerId =
  | "npm"
  | "pnpm"
  | "yarn-classic"
  | "yarn-modern"
  | "bun"
  | "vlt"
  | "deno";

export type SupportTier = "production-target" | "preview-target";

export type DiagnosticSeverity = "info" | "warning" | "error";

export interface Diagnostic {
  code: string;
  severity: DiagnosticSeverity;
  summary: string;
  blocking: boolean;
  evidence?: Array<{
    location: string;
    detail: string;
  }>;
  remediation?: string[];
}

export interface PackageManagerEvidence {
  manager: PackageManagerId;
  kind: "manifest" | "lockfile" | "configuration" | "workspace";
  location: string;
  detail: string;
  weight: number;
}

export interface PackageManagerCandidate {
  manager: PackageManagerId;
  score: number;
  confidence: "high" | "medium" | "low";
  evidence: PackageManagerEvidence[];
}

export interface PackageManagerDetection {
  selected: PackageManagerId | null;
  candidates: PackageManagerCandidate[];
  evidence: PackageManagerEvidence[];
  diagnostics: Diagnostic[];
}

export interface WorkspaceInspection {
  configured: boolean;
  sources: Array<{
    location: string;
    patterns: string[];
  }>;
}

export interface IntegrationInspection {
  kind: "ci" | "container" | "documentation" | "automation";
  path: string;
  packageManagerTokens: string[];
}

export interface ProjectInspection {
  root: string;
  fingerprint: string;
  relevantFiles: string[];
  manifest: {
    path: string;
    name: string | null;
    private: boolean | null;
    packageManager: string | null;
  } | null;
  packageManager: PackageManagerDetection;
  workspace: WorkspaceInspection;
  integrations: IntegrationInspection[];
  diagnostics: Diagnostic[];
}

export type SideEffect =
  | "none"
  | "repository-write"
  | "filesystem-write"
  | "dependency-state"
  | "process-execution";

export interface PlannedOperation {
  id: string;
  phase: "configure" | "integrate" | "install" | "cleanup" | "verify";
  kind: string;
  description: string;
  paths: string[];
  command?: string[];
  capabilities?: string[];
  sideEffect: SideEffect;
  reversible: boolean;
  preconditions: string[];
  postconditions: string[];
  mutations?: PlannedFileMutation[];
}

export interface PlannedFileMutation {
  path: string;
  action: "write" | "delete";
  beforeDigest: string | null;
  afterDigest: string | null;
  content?: string;
  reason: string;
  capabilities: string[];
}

export type NativeImportMode = "dedicated-command" | "install-integrated";

export interface NativeImportStrategy {
  id: string;
  source: PackageManagerId;
  target: PackageManagerId;
  mode: NativeImportMode;
  command: string[];
  summary: string;
}

export interface MigrationPlan {
  schemaVersion: "1.0";
  planId: string;
  executable: boolean;
  acceptedLossy: boolean;
  source: PackageManagerId;
  target: PackageManagerId;
  targetTier: SupportTier;
  repositoryFingerprint: string;
  projectIrId: string;
  capabilityAnalysisId: string;
  capabilitySummary: {
    native: number;
    transform: number;
    lossy: number;
    unsupported: number;
    unknown: number;
    notApplicable: number;
  };
  sourceLockGraphId?: string;
  nativeImport?: NativeImportStrategy;
  operations: PlannedOperation[];
  diagnostics: Diagnostic[];
  verification: string[];
}

export interface ResultArtifact<T = unknown> {
  id: string;
  type: string;
  mediaType: string;
  content: T;
}

export interface NextAction {
  argv: string[];
  requiresApproval: boolean;
  sideEffect: SideEffect;
}

export interface CommandResult {
  schemaVersion: "1.0";
  command: string;
  status: "completed" | "planned" | "blocked" | "unsupported" | "failed" | "rolled-back";
  planId: string | null;
  runId: string | null;
  summary: Record<string, unknown>;
  artifacts: ResultArtifact[];
  diagnostics: Diagnostic[];
  nextActions: NextAction[];
}

export interface CommandExecution {
  exitCode: number;
  result: CommandResult;
}
