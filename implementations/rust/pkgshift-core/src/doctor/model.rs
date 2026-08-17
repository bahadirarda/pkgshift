use serde::{Deserialize, Serialize};

use crate::VerificationPolicy;
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
    pub verification_policy: VerificationPolicy,
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

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadinessMatrixSummary {
    pub targets: usize,
    pub migration_available_targets: usize,
    pub available_after_review_targets: usize,
    pub ready_targets: usize,
    pub review_required_targets: usize,
    pub blocked_targets: usize,
    pub already_selected_targets: usize,
}

impl ReadinessMatrixSummary {
    pub fn from_reports(reports: &[MigrationReadiness]) -> Self {
        let mut summary = Self {
            targets: reports.len(),
            migration_available_targets: reports
                .iter()
                .filter(|report| report.migration_available)
                .count(),
            available_after_review_targets: reports
                .iter()
                .filter(|report| report.available_after_review)
                .count(),
            ..Self::default()
        };
        for report in reports {
            match report.verdict {
                ReadinessVerdict::Ready => summary.ready_targets += 1,
                ReadinessVerdict::ReviewRequired => summary.review_required_targets += 1,
                ReadinessVerdict::Blocked => summary.blocked_targets += 1,
                ReadinessVerdict::AlreadySelected => summary.already_selected_targets += 1,
            }
        }
        summary
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationReadinessMatrix {
    pub schema_version: String,
    pub matrix_id: String,
    pub read_only: bool,
    pub accepted_lossy: bool,
    pub verification_policy: VerificationPolicy,
    pub source: Option<PackageManagerId>,
    pub repository_fingerprint: String,
    pub summary: ReadinessMatrixSummary,
    pub reports: Vec<MigrationReadiness>,
}
