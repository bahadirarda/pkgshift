use serde::{Deserialize, Serialize};

use crate::model::{CapabilitySummary, Diagnostic, PackageManagerId, SCHEMA_VERSION, SupportTier};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReadinessVerdict {
    Ready,
    ReviewRequired,
    Blocked,
    AlreadySelected,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegrationImpact {
    pub ci: Vec<String>,
    pub containers: Vec<String>,
    pub documentation: Vec<String>,
    pub automation: Vec<String>,
}

impl IntegrationImpact {
    pub fn total(&self) -> usize {
        self.ci.len() + self.containers.len() + self.documentation.len() + self.automation.len()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationEffects {
    pub file_writes: Vec<String>,
    pub file_deletions: Vec<String>,
    pub dependency_state_cleanups: Vec<String>,
    pub source_artifact_retirements: Vec<String>,
    pub process_commands: Vec<Vec<String>>,
    pub verification_scripts: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationReadiness {
    pub schema_version: String,
    pub report_id: String,
    pub verdict: ReadinessVerdict,
    pub read_only: bool,
    pub migration_available: bool,
    pub available_after_review: bool,
    pub accepted_lossy: bool,
    pub source: Option<PackageManagerId>,
    pub target: PackageManagerId,
    pub target_tier: SupportTier,
    pub repository_fingerprint: String,
    pub project_ir_id: Option<String>,
    pub capability_analysis_id: Option<String>,
    pub package_count: usize,
    pub workspace_configured: bool,
    pub workspace_patterns: Vec<String>,
    pub available_root_scripts: Vec<String>,
    pub integrations: IntegrationImpact,
    pub capabilities: CapabilitySummary,
    pub effects: MigrationEffects,
    pub diagnostics: Vec<Diagnostic>,
}

impl MigrationReadiness {
    pub fn schema_version() -> String {
        SCHEMA_VERSION.to_owned()
    }
}
