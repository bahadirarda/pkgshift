use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct DiagnosticExplanation {
    pub code: &'static str,
    pub title: &'static str,
    pub explanation: &'static str,
    pub remediation: &'static [&'static str],
}

impl DiagnosticExplanation {
    pub(super) const fn new(
        code: &'static str,
        title: &'static str,
        explanation: &'static str,
        remediation: &'static [&'static str],
    ) -> Self {
        Self {
            code,
            title,
            explanation,
            remediation,
        }
    }
}
