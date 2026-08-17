use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::verification_policy::{PackagePlatformConstraint, VerificationPolicy};

pub const SCHEMA_VERSION: &str = "1.0";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PackageManagerId {
    Npm,
    Pnpm,
    YarnClassic,
    YarnModern,
    Bun,
    Vlt,
    Deno,
}

impl fmt::Display for PackageManagerId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Npm => "npm",
            Self::Pnpm => "pnpm",
            Self::YarnClassic => "yarn-classic",
            Self::YarnModern => "yarn-modern",
            Self::Bun => "bun",
            Self::Vlt => "vlt",
            Self::Deno => "deno",
        })
    }
}

impl FromStr for PackageManagerId {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "npm" => Ok(Self::Npm),
            "pnpm" => Ok(Self::Pnpm),
            "yarn-classic" | "yarn@1" | "yarn-1" => Ok(Self::YarnClassic),
            "yarn-modern" | "yarn-berry" | "yarn@modern" => Ok(Self::YarnModern),
            "bun" => Ok(Self::Bun),
            "vlt" => Ok(Self::Vlt),
            "deno" => Ok(Self::Deno),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SupportTier {
    ProductionTarget,
    PreviewTarget,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DiagnosticSeverity {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceDetail {
    pub location: String,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostic {
    pub code: String,
    pub severity: DiagnosticSeverity,
    pub summary: String,
    pub blocking: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence: Vec<EvidenceDetail>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub remediation: Vec<String>,
}

impl Diagnostic {
    pub fn blocking(code: &str, summary: impl Into<String>, remediation: Vec<String>) -> Self {
        Self {
            code: code.to_owned(),
            severity: DiagnosticSeverity::Error,
            summary: summary.into(),
            blocking: true,
            evidence: Vec::new(),
            remediation,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EvidenceKind {
    Manifest,
    Lockfile,
    Configuration,
    Workspace,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageManagerEvidence {
    pub manager: PackageManagerId,
    pub kind: EvidenceKind,
    pub location: String,
    pub detail: String,
    pub weight: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Confidence {
    High,
    Medium,
    Low,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageManagerCandidate {
    pub manager: PackageManagerId,
    pub score: u16,
    pub confidence: Confidence,
    pub evidence: Vec<PackageManagerEvidence>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageManagerDetection {
    pub selected: Option<PackageManagerId>,
    pub candidates: Vec<PackageManagerCandidate>,
    pub evidence: Vec<PackageManagerEvidence>,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestInspection {
    pub path: String,
    pub name: Option<String>,
    pub private: Option<bool>,
    pub package_manager: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceSource {
    pub location: String,
    pub patterns: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceInspection {
    pub configured: bool,
    pub sources: Vec<WorkspaceSource>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum IntegrationKind {
    Ci,
    Container,
    Documentation,
    Automation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegrationInspection {
    pub kind: IntegrationKind,
    pub path: String,
    pub package_manager_tokens: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectInspection {
    pub root: String,
    pub fingerprint: String,
    pub relevant_files: Vec<String>,
    pub manifest: Option<ManifestInspection>,
    pub package_manager: PackageManagerDetection,
    pub workspace: WorkspaceInspection,
    pub integrations: Vec<IntegrationInspection>,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DependencyProtocol {
    Semver,
    Tag,
    Workspace,
    Catalog,
    NpmAlias,
    File,
    Link,
    Portal,
    Patch,
    Git,
    Url,
    Jsr,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DependencyIr {
    pub package_path: String,
    pub section: String,
    pub name: String,
    pub specifier: String,
    pub protocol: DependencyProtocol,
    pub location: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageIr {
    pub path: String,
    pub manifest_path: String,
    pub name: Option<String>,
    pub version: Option<String>,
    pub private: Option<bool>,
    pub dependencies: Vec<DependencyIr>,
    pub script_names: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ObservedFeature {
    pub id: String,
    pub count: usize,
    pub locations: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectIr {
    pub schema_version: String,
    pub project_ir_id: String,
    pub repository_fingerprint: String,
    pub source: Option<PackageManagerId>,
    pub root_package_path: String,
    pub packages: Vec<PackageIr>,
    pub workspace_patterns: Vec<String>,
    pub features: Vec<ObservedFeature>,
    pub integrations: Vec<IntegrationInspection>,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LockGraphNode {
    pub locator: String,
    pub name: String,
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub integrity: Option<String>,
    #[serde(default, skip_serializing_if = "PackagePlatformConstraint::is_empty")]
    pub platform: PackagePlatformConstraint,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LockGraphEdge {
    pub from: String,
    pub dependency: String,
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LockGraph {
    pub schema_version: String,
    pub graph_id: String,
    pub manager: PackageManagerId,
    pub lockfile_path: String,
    pub lockfile_digest: String,
    pub format: String,
    pub complete: bool,
    pub nodes: Vec<LockGraphNode>,
    pub edges: Vec<LockGraphEdge>,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CapabilityClassification {
    Native,
    Transform,
    Lossy,
    Unsupported,
    Unknown,
    NotApplicable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityDecision {
    pub feature_id: String,
    pub target: PackageManagerId,
    pub classification: CapabilityClassification,
    pub risk: String,
    pub transformation_id: Option<String>,
    pub summary: String,
    pub locations: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilitySummary {
    pub native: usize,
    pub transform: usize,
    pub lossy: usize,
    pub unsupported: usize,
    pub unknown: usize,
    pub not_applicable: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityAnalysis {
    pub schema_version: String,
    pub analysis_id: String,
    pub project_ir_id: String,
    pub source: PackageManagerId,
    pub target: PackageManagerId,
    pub decisions: Vec<CapabilityDecision>,
    pub summary: CapabilitySummary,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SideEffect {
    None,
    RepositoryWrite,
    FilesystemWrite,
    DependencyState,
    ProcessExecution,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MutationAction {
    Write,
    Delete,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlannedFileMutation {
    pub path: String,
    pub action: MutationAction,
    pub before_digest: Option<String>,
    pub after_digest: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    pub reason: String,
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlannedOperation {
    pub id: String,
    pub phase: String,
    pub kind: String,
    pub description: String,
    pub paths: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub command: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_seconds: Option<u64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capabilities: Vec<String>,
    pub side_effect: SideEffect,
    pub reversible: bool,
    pub preconditions: Vec<String>,
    pub postconditions: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mutations: Vec<PlannedFileMutation>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum NativeImportMode {
    DedicatedCommand,
    InstallIntegrated,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeImportStrategy {
    pub id: String,
    pub source: PackageManagerId,
    pub target: PackageManagerId,
    pub mode: NativeImportMode,
    pub command: Vec<String>,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutableRequirement {
    pub program: String,
    pub required_version: String,
    pub version_command: Vec<String>,
    pub package_manager_pin: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedExecutable {
    pub program: String,
    pub path: String,
    pub version: String,
    pub package_manager_pin: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationPlan {
    pub schema_version: String,
    pub plan_id: String,
    pub executable: bool,
    pub accepted_lossy: bool,
    pub source: PackageManagerId,
    pub target: PackageManagerId,
    pub target_tier: SupportTier,
    pub repository_fingerprint: String,
    pub project_ir_id: String,
    pub capability_analysis_id: String,
    pub capability_summary: CapabilitySummary,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_lock_graph_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub native_import: Option<NativeImportStrategy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_executable: Option<ExecutableRequirement>,
    #[serde(default)]
    pub verification_policy: VerificationPolicy,
    pub operations: Vec<PlannedOperation>,
    pub diagnostics: Vec<Diagnostic>,
    pub verification: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultArtifact {
    pub id: String,
    pub r#type: String,
    pub media_type: String,
    pub content: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NextAction {
    pub argv: Vec<String>,
    pub requires_approval: bool,
    pub side_effect: SideEffect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CommandStatus {
    Completed,
    Planned,
    Blocked,
    Unsupported,
    Failed,
    RolledBack,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandResult {
    pub schema_version: String,
    pub command: String,
    pub status: CommandStatus,
    pub plan_id: Option<String>,
    pub run_id: Option<String>,
    pub summary: BTreeMap<String, Value>,
    pub artifacts: Vec<ResultArtifact>,
    pub diagnostics: Vec<Diagnostic>,
    pub next_actions: Vec<NextAction>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandExecution {
    pub exit_code: u8,
    pub result: CommandResult,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredRun {
    pub schema_version: String,
    pub run_id: String,
    pub plan: MigrationPlan,
    pub state: String,
    pub snapshot_directory: String,
    pub snapshot_entries: Vec<SnapshotEntry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dependency_state_cleanups: Vec<DependencyStateCleanupRecord>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_executable: Option<ResolvedExecutable>,
    pub processes: Vec<ProcessExecutionRecord>,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredPlan {
    pub schema_version: String,
    pub plan: MigrationPlan,
    pub project_ir: ProjectIr,
    pub capability_analysis: CapabilityAnalysis,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_lock_graph: Option<LockGraph>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotEntry {
    pub path: String,
    pub existed: bool,
    pub digest: Option<String>,
    pub mode: Option<u32>,
    pub backup_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessExecutionRecord {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub operation_id: String,
    pub argv: Vec<String>,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub success: bool,
    #[serde(default)]
    pub timed_out: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_millis: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DependencyStateCleanupRecord {
    pub operation_id: String,
    pub removed_paths: Vec<String>,
    pub absent_paths: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum VerificationStatus {
    Passed,
    Failed,
    Skipped,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerificationCheck {
    pub id: String,
    pub status: VerificationStatus,
    pub summary: String,
    pub evidence: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LockGraphComparison {
    pub comparison_id: String,
    pub policy: String,
    pub status: VerificationStatus,
    pub source_graph_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_graph_id: Option<String>,
    pub source_resolutions: usize,
    pub target_resolutions: usize,
    pub added_resolutions: Vec<String>,
    pub removed_resolutions: Vec<String>,
    pub integrity_mismatches: Vec<String>,
    pub edge_changes: Vec<String>,
    #[serde(default)]
    pub verification_policy: VerificationPolicy,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pruned_source_resolutions: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pruned_target_resolutions: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub optional_platform_differences: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reachability_issues: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerificationReport {
    pub schema_version: String,
    pub report_id: String,
    pub run_id: String,
    pub plan_id: String,
    pub status: VerificationStatus,
    pub checks: Vec<VerificationCheck>,
    pub diagnostics: Vec<Diagnostic>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lock_graph_comparison: Option<LockGraphComparison>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyOutcome {
    pub run: StoredRun,
    pub verification: Option<VerificationReport>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrialReport {
    pub schema_version: String,
    pub report_id: String,
    pub plan_id: String,
    pub status: VerificationStatus,
    pub repository_unchanged: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dependency_state_cleanups: Vec<DependencyStateCleanupRecord>,
    pub processes: Vec<ProcessExecutionRecord>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verification: Option<VerificationReport>,
    pub diagnostics: Vec<Diagnostic>,
}
