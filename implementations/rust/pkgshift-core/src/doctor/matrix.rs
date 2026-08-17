use serde::Serialize;

use crate::VerificationPolicy;
use crate::catalog::PACKAGE_MANAGERS;
use crate::doctor::context::ReadinessContext;
use crate::doctor::model::{MigrationReadinessMatrix, ReadinessMatrixSummary};
use crate::model::SCHEMA_VERSION;
use crate::util::{Result, short_digest};

use super::assessment::assess;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MatrixIdentity<'a> {
    repository_fingerprint: &'a str,
    accepted_lossy: bool,
    verification_scripts: &'a [String],
    verification_policy: &'a VerificationPolicy,
    report_ids: Vec<&'a str>,
}

pub(crate) fn assess_all(
    context: &ReadinessContext,
    accepted_lossy: bool,
    verification_scripts: &[String],
    verification_policy: &VerificationPolicy,
) -> Result<MigrationReadinessMatrix> {
    let reports = PACKAGE_MANAGERS
        .iter()
        .map(|definition| {
            assess(
                context,
                definition.id,
                accepted_lossy,
                verification_scripts,
                verification_policy,
            )
            .map(|assessment| assessment.report)
        })
        .collect::<Result<Vec<_>>>()?;
    let summary = ReadinessMatrixSummary::from_reports(&reports);
    let matrix_id = short_digest(
        "doctor_matrix_",
        &MatrixIdentity {
            repository_fingerprint: &context.inspection.fingerprint,
            accepted_lossy,
            verification_scripts,
            verification_policy,
            report_ids: reports
                .iter()
                .map(|report| report.report_id.as_str())
                .collect(),
        },
    )?;
    Ok(MigrationReadinessMatrix {
        schema_version: SCHEMA_VERSION.to_owned(),
        matrix_id,
        read_only: true,
        accepted_lossy,
        verification_policy: verification_policy.clone(),
        source: context
            .project_ir
            .as_ref()
            .and_then(|project| project.source),
        repository_fingerprint: context.inspection.fingerprint.clone(),
        summary,
        reports,
    })
}
