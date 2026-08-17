mod migration;
mod operation;
mod runtime;
mod skill;

use super::model::DiagnosticExplanation;

pub(super) fn find(code: &str) -> Option<&'static DiagnosticExplanation> {
    operation::ENTRIES
        .iter()
        .chain(migration::ENTRIES)
        .chain(runtime::ENTRIES)
        .chain(skill::ENTRIES)
        .find(|entry| entry.code == code)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    #[test]
    fn catalog_codes_are_unique_and_stable() {
        let entries = operation::ENTRIES
            .iter()
            .chain(migration::ENTRIES)
            .chain(runtime::ENTRIES)
            .chain(skill::ENTRIES)
            .collect::<Vec<_>>();
        let codes = entries
            .iter()
            .map(|entry| entry.code)
            .collect::<BTreeSet<_>>();
        assert_eq!(codes.len(), entries.len());
        assert!(entries.iter().all(|entry| {
            !entry.title.is_empty()
                && !entry.explanation.is_empty()
                && !entry.remediation.is_empty()
        }));
    }
}
