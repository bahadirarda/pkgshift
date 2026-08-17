use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::model::{Diagnostic, PlannedOperation, SnapshotEntry, VerificationStatus};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DenoPermission {
    Read,
    Write,
    Net,
    Env,
    Run,
    Sys,
    Ffi,
    Hrtime,
}

impl DenoPermission {
    pub(crate) fn flag(self) -> &'static str {
        match self {
            Self::Read => "--allow-read",
            Self::Write => "--allow-write",
            Self::Net => "--allow-net",
            Self::Env => "--allow-env",
            Self::Run => "--allow-run",
            Self::Sys => "--allow-sys",
            Self::Ffi => "--allow-ffi",
            Self::Hrtime => "--allow-hrtime",
        }
    }
}

impl fmt::Display for DenoPermission {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Read => "read",
            Self::Write => "write",
            Self::Net => "net",
            Self::Env => "env",
            Self::Run => "run",
            Self::Sys => "sys",
            Self::Ffi => "ffi",
            Self::Hrtime => "hrtime",
        })
    }
}

impl FromStr for DenoPermission {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "read" => Ok(Self::Read),
            "write" => Ok(Self::Write),
            "net" => Ok(Self::Net),
            "env" => Ok(Self::Env),
            "run" => Ok(Self::Run),
            "sys" => Ok(Self::Sys),
            "ffi" => Ok(Self::Ffi),
            "hrtime" => Ok(Self::Hrtime),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct RuntimeFile {
    pub path: String,
    pub content: String,
}

#[derive(Debug, Clone)]
pub(crate) struct RuntimeInspection {
    pub fingerprint: String,
    pub files: Vec<RuntimeFile>,
    pub bun_evidence: Vec<String>,
    pub input_diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RuntimeRecipeApplication {
    pub recipe_id: String,
    pub path: String,
    pub replacements: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RuntimeMigrationPlan {
    pub schema_version: String,
    pub plan_id: String,
    pub executable: bool,
    pub source: String,
    pub target: String,
    pub repository_fingerprint: String,
    pub permissions: Vec<DenoPermission>,
    pub recipes: Vec<RuntimeRecipeApplication>,
    pub operations: Vec<PlannedOperation>,
    pub diagnostics: Vec<Diagnostic>,
    pub verification: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RuntimeMutationSummary {
    pub path: String,
    pub action: crate::model::MutationAction,
    pub before_digest: Option<String>,
    pub after_digest: Option<String>,
    pub reason: String,
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RuntimeOperationSummary {
    pub id: String,
    pub phase: String,
    pub kind: String,
    pub description: String,
    pub paths: Vec<String>,
    pub side_effect: crate::model::SideEffect,
    pub reversible: bool,
    pub mutations: Vec<RuntimeMutationSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RuntimePlanArtifact {
    pub schema_version: String,
    pub plan_id: String,
    pub executable: bool,
    pub source: String,
    pub target: String,
    pub repository_fingerprint: String,
    pub permissions: Vec<DenoPermission>,
    pub recipes: Vec<RuntimeRecipeApplication>,
    pub operations: Vec<RuntimeOperationSummary>,
    pub diagnostics: Vec<Diagnostic>,
    pub verification: Vec<String>,
}

impl From<&RuntimeMigrationPlan> for RuntimePlanArtifact {
    fn from(plan: &RuntimeMigrationPlan) -> Self {
        Self {
            schema_version: plan.schema_version.clone(),
            plan_id: plan.plan_id.clone(),
            executable: plan.executable,
            source: plan.source.clone(),
            target: plan.target.clone(),
            repository_fingerprint: plan.repository_fingerprint.clone(),
            permissions: plan.permissions.clone(),
            recipes: plan.recipes.clone(),
            operations: plan
                .operations
                .iter()
                .map(|operation| RuntimeOperationSummary {
                    id: operation.id.clone(),
                    phase: operation.phase.clone(),
                    kind: operation.kind.clone(),
                    description: operation.description.clone(),
                    paths: operation.paths.clone(),
                    side_effect: operation.side_effect,
                    reversible: operation.reversible,
                    mutations: operation
                        .mutations
                        .iter()
                        .map(|mutation| RuntimeMutationSummary {
                            path: mutation.path.clone(),
                            action: mutation.action,
                            before_digest: mutation.before_digest.clone(),
                            after_digest: mutation.after_digest.clone(),
                            reason: mutation.reason.clone(),
                            capabilities: mutation.capabilities.clone(),
                        })
                        .collect(),
                })
                .collect(),
            diagnostics: plan.diagnostics.clone(),
            verification: plan.verification.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RuntimeVerificationCheck {
    pub id: String,
    pub status: VerificationStatus,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RuntimeVerificationReport {
    pub schema_version: String,
    pub report_id: String,
    pub plan_id: String,
    pub run_id: String,
    pub status: VerificationStatus,
    pub checks: Vec<RuntimeVerificationCheck>,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StoredRuntimeRun {
    pub schema_version: String,
    pub run_id: String,
    pub plan: RuntimePlanArtifact,
    pub state: String,
    pub snapshot_directory: String,
    pub snapshot_entries: Vec<SnapshotEntry>,
    pub diagnostics: Vec<Diagnostic>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verification: Option<RuntimeVerificationReport>,
}

#[derive(Debug, Clone)]
pub(crate) struct RuntimeApplyOutcome {
    pub run: StoredRuntimeRun,
    pub verification: Option<RuntimeVerificationReport>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RuntimeRunArtifact {
    pub schema_version: String,
    pub run_id: String,
    pub plan: RuntimePlanArtifact,
    pub state: String,
    pub snapshot_directory: String,
    pub snapshot_entries: Vec<SnapshotEntry>,
    pub diagnostics: Vec<Diagnostic>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verification: Option<RuntimeVerificationReport>,
}

impl From<&StoredRuntimeRun> for RuntimeRunArtifact {
    fn from(run: &StoredRuntimeRun) -> Self {
        Self {
            schema_version: run.schema_version.clone(),
            run_id: run.run_id.clone(),
            plan: run.plan.clone(),
            state: run.state.clone(),
            snapshot_directory: run.snapshot_directory.clone(),
            snapshot_entries: run.snapshot_entries.clone(),
            diagnostics: run.diagnostics.clone(),
            verification: run.verification.clone(),
        }
    }
}
