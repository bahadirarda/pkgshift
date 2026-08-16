use std::collections::BTreeMap;
use std::path::Path;

use serde_json::{Map, Value};

use crate::model::{
    Confidence, Diagnostic, DiagnosticSeverity, EvidenceDetail, EvidenceKind,
    PackageManagerCandidate, PackageManagerDetection, PackageManagerEvidence, PackageManagerId,
};
use crate::util::Result;

#[derive(Clone, Copy)]
struct FileSignal {
    manager: PackageManagerId,
    kind: EvidenceKind,
    location: &'static str,
    detail: &'static str,
    weight: u16,
}

const FILE_SIGNALS: &[FileSignal] = &[
    FileSignal {
        manager: PackageManagerId::Npm,
        kind: EvidenceKind::Lockfile,
        location: "package-lock.json",
        detail: "npm lockfile exists",
        weight: 80,
    },
    FileSignal {
        manager: PackageManagerId::Npm,
        kind: EvidenceKind::Lockfile,
        location: "npm-shrinkwrap.json",
        detail: "npm shrinkwrap exists",
        weight: 90,
    },
    FileSignal {
        manager: PackageManagerId::Pnpm,
        kind: EvidenceKind::Lockfile,
        location: "pnpm-lock.yaml",
        detail: "pnpm lockfile exists",
        weight: 80,
    },
    FileSignal {
        manager: PackageManagerId::Pnpm,
        kind: EvidenceKind::Workspace,
        location: "pnpm-workspace.yaml",
        detail: "pnpm workspace configuration exists",
        weight: 45,
    },
    FileSignal {
        manager: PackageManagerId::Pnpm,
        kind: EvidenceKind::Configuration,
        location: ".pnpmfile.cjs",
        detail: "pnpm hook configuration exists",
        weight: 45,
    },
    FileSignal {
        manager: PackageManagerId::YarnClassic,
        kind: EvidenceKind::Configuration,
        location: ".yarnrc",
        detail: "Yarn Classic configuration exists",
        weight: 75,
    },
    FileSignal {
        manager: PackageManagerId::YarnModern,
        kind: EvidenceKind::Configuration,
        location: ".yarnrc.yml",
        detail: "Yarn Modern configuration exists",
        weight: 90,
    },
    FileSignal {
        manager: PackageManagerId::YarnModern,
        kind: EvidenceKind::Configuration,
        location: ".pnp.cjs",
        detail: "Yarn Plug and Play loader exists",
        weight: 70,
    },
    FileSignal {
        manager: PackageManagerId::Bun,
        kind: EvidenceKind::Lockfile,
        location: "bun.lock",
        detail: "Bun text lockfile exists",
        weight: 85,
    },
    FileSignal {
        manager: PackageManagerId::Bun,
        kind: EvidenceKind::Lockfile,
        location: "bun.lockb",
        detail: "Bun binary lockfile exists",
        weight: 80,
    },
    FileSignal {
        manager: PackageManagerId::Bun,
        kind: EvidenceKind::Configuration,
        location: "bunfig.toml",
        detail: "Bun configuration exists",
        weight: 40,
    },
    FileSignal {
        manager: PackageManagerId::Vlt,
        kind: EvidenceKind::Lockfile,
        location: "vlt-lock.json",
        detail: "vlt lockfile exists",
        weight: 85,
    },
    FileSignal {
        manager: PackageManagerId::Vlt,
        kind: EvidenceKind::Configuration,
        location: "vlt.json",
        detail: "vlt configuration exists",
        weight: 40,
    },
    FileSignal {
        manager: PackageManagerId::Deno,
        kind: EvidenceKind::Lockfile,
        location: "deno.lock",
        detail: "Deno lockfile exists",
        weight: 55,
    },
    FileSignal {
        manager: PackageManagerId::Deno,
        kind: EvidenceKind::Configuration,
        location: "deno.json",
        detail: "Deno JSON configuration exists",
        weight: 55,
    },
    FileSignal {
        manager: PackageManagerId::Deno,
        kind: EvidenceKind::Configuration,
        location: "deno.jsonc",
        detail: "Deno JSONC configuration exists",
        weight: 55,
    },
];

pub fn manager_from_package_manager_field(value: &str) -> Option<PackageManagerId> {
    let (name, version) = value.rsplit_once('@')?;
    if name.is_empty() || version.is_empty() {
        return None;
    }
    match name.to_ascii_lowercase().as_str() {
        "npm" => Some(PackageManagerId::Npm),
        "pnpm" => Some(PackageManagerId::Pnpm),
        "bun" => Some(PackageManagerId::Bun),
        "vlt" => Some(PackageManagerId::Vlt),
        "deno" => Some(PackageManagerId::Deno),
        "yarn" if version == "1" || version.starts_with("1.") => {
            Some(PackageManagerId::YarnClassic)
        }
        "yarn" => Some(PackageManagerId::YarnModern),
        _ => None,
    }
}

fn confidence(score: u16) -> Confidence {
    if score >= 100 {
        Confidence::High
    } else if score >= 70 {
        Confidence::Medium
    } else {
        Confidence::Low
    }
}

fn evidence(signal: FileSignal) -> PackageManagerEvidence {
    PackageManagerEvidence {
        manager: signal.manager,
        kind: signal.kind,
        location: signal.location.to_owned(),
        detail: signal.detail.to_owned(),
        weight: signal.weight,
    }
}

pub fn detect_package_manager(
    root: &Path,
    manifest: Option<&Map<String, Value>>,
) -> Result<PackageManagerDetection> {
    let mut all_evidence = Vec::new();
    let mut diagnostics = Vec::new();

    if let Some(value) = manifest
        .and_then(|value| value.get("packageManager"))
        .and_then(Value::as_str)
    {
        if let Some(manager) = manager_from_package_manager_field(value) {
            all_evidence.push(PackageManagerEvidence {
                manager,
                kind: EvidenceKind::Manifest,
                location: "package.json".to_owned(),
                detail: format!("packageManager declares {value}"),
                weight: 120,
            });
        } else {
            diagnostics.push(Diagnostic {
                code: "PM_PACKAGE_MANAGER_FIELD_UNKNOWN".to_owned(),
                severity: DiagnosticSeverity::Warning,
                summary: format!("The packageManager field is not recognized: {value}"),
                blocking: false,
                evidence: vec![EvidenceDetail {
                    location: "package.json".to_owned(),
                    detail: "Unrecognized packageManager value".to_owned(),
                }],
                remediation: vec![
                    "Use a supported package manager name with an explicit version.".to_owned(),
                ],
            });
        }
    }

    for signal in FILE_SIGNALS {
        if root.join(signal.location).try_exists().map_err(|source| {
            crate::util::PkgshiftError::Io {
                path: root.join(signal.location),
                source,
            }
        })? {
            all_evidence.push(evidence(*signal));
        }
    }

    if root
        .join("yarn.lock")
        .try_exists()
        .map_err(|source| crate::util::PkgshiftError::Io {
            path: root.join("yarn.lock"),
            source,
        })?
    {
        let modern = all_evidence.iter().any(|item| {
            item.manager == PackageManagerId::YarnModern && item.kind == EvidenceKind::Configuration
        });
        let classic = all_evidence.iter().any(|item| {
            item.manager == PackageManagerId::YarnClassic
                && item.kind == EvidenceKind::Configuration
        });
        if modern {
            all_evidence.push(PackageManagerEvidence {
                manager: PackageManagerId::YarnModern,
                kind: EvidenceKind::Lockfile,
                location: "yarn.lock".to_owned(),
                detail: "Yarn lockfile is paired with modern configuration".to_owned(),
                weight: 70,
            });
        } else if classic {
            all_evidence.push(PackageManagerEvidence {
                manager: PackageManagerId::YarnClassic,
                kind: EvidenceKind::Lockfile,
                location: "yarn.lock".to_owned(),
                detail: "Yarn lockfile is paired with classic configuration".to_owned(),
                weight: 70,
            });
        } else {
            for manager in [PackageManagerId::YarnClassic, PackageManagerId::YarnModern] {
                all_evidence.push(PackageManagerEvidence {
                    manager,
                    kind: EvidenceKind::Lockfile,
                    location: "yarn.lock".to_owned(),
                    detail: "Yarn lockfile version is not disambiguated".to_owned(),
                    weight: 60,
                });
            }
        }
    }

    let mut grouped = BTreeMap::<PackageManagerId, Vec<PackageManagerEvidence>>::new();
    for item in &all_evidence {
        grouped.entry(item.manager).or_default().push(item.clone());
    }
    let mut candidates = grouped
        .into_iter()
        .map(|(manager, mut items)| {
            items.sort_by(|left, right| left.location.cmp(&right.location));
            let score = items.iter().map(|item| item.weight).sum();
            PackageManagerCandidate {
                manager,
                score,
                confidence: confidence(score),
                evidence: items,
            }
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| left.manager.cmp(&right.manager))
    });

    let selected = match (candidates.first(), candidates.get(1)) {
        (None, _) => {
            diagnostics.push(Diagnostic::blocking(
                "PM_SOURCE_NOT_DETECTED",
                "No supported package manager evidence was detected.",
                vec![
                    "Add an explicit packageManager field or select a source when that option becomes available."
                        .to_owned(),
                ],
            ));
            None
        }
        (Some(first), second)
            if first.score < 60
                || second.is_some_and(|runner_up| first.score - runner_up.score < 25) =>
        {
            diagnostics.push(Diagnostic {
                code: "PM_SOURCE_AMBIGUOUS".to_owned(),
                severity: DiagnosticSeverity::Error,
                summary: "Package manager evidence is ambiguous.".to_owned(),
                blocking: true,
                evidence: candidates
                    .iter()
                    .take(3)
                    .flat_map(|candidate| {
                        candidate.evidence.iter().map(|item| EvidenceDetail {
                            location: item.location.clone(),
                            detail: format!("{}: {}", candidate.manager, item.detail),
                        })
                    })
                    .collect(),
                remediation: vec![
                    "Review conflicting evidence and make the intended source explicit.".to_owned(),
                ],
            });
            None
        }
        (Some(first), _) => {
            let selected = first.manager;
            let conflicts = candidates
                .iter()
                .filter(|candidate| candidate.manager != selected && candidate.score >= 70)
                .collect::<Vec<_>>();
            if !conflicts.is_empty() {
                diagnostics.push(Diagnostic {
                    code: "PM_CONFLICTING_EVIDENCE".to_owned(),
                    severity: DiagnosticSeverity::Warning,
                    summary: format!(
                        "{selected} was selected, but other strong package manager evidence exists."
                    ),
                    blocking: false,
                    evidence: conflicts
                        .iter()
                        .flat_map(|candidate| {
                            candidate.evidence.iter().map(|item| EvidenceDetail {
                                location: item.location.clone(),
                                detail: format!("{}: {}", candidate.manager, item.detail),
                            })
                        })
                        .collect(),
                    remediation: vec![
                        "Confirm that additional lockfiles or configuration are stale before apply."
                            .to_owned(),
                    ],
                });
            }
            Some(selected)
        }
    };

    all_evidence.sort_by(|left, right| {
        left.location
            .cmp(&right.location)
            .then_with(|| left.manager.cmp(&right.manager))
    });
    Ok(PackageManagerDetection {
        selected,
        candidates,
        evidence: all_evidence,
        diagnostics,
    })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use serde_json::json;
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn explicit_manifest_evidence_wins() {
        let directory = tempdir().expect("temporary directory");
        fs::write(
            directory.path().join("pnpm-lock.yaml"),
            "lockfileVersion: '9.0'\n",
        )
        .expect("fixture lockfile");
        let manifest = json!({"packageManager": "pnpm@11.21.0"})
            .as_object()
            .cloned()
            .expect("manifest object");
        let detection = detect_package_manager(directory.path(), Some(&manifest))
            .expect("package manager detection");
        assert_eq!(detection.selected, Some(PackageManagerId::Pnpm));
        assert_eq!(detection.candidates[0].score, 200);
    }

    #[test]
    fn bare_yarn_lock_is_ambiguous() {
        let directory = tempdir().expect("temporary directory");
        fs::write(directory.path().join("yarn.lock"), "").expect("fixture lockfile");
        let detection =
            detect_package_manager(directory.path(), None).expect("package manager detection");
        assert_eq!(detection.selected, None);
        assert!(
            detection.diagnostics.iter().any(|diagnostic| {
                diagnostic.code == "PM_SOURCE_AMBIGUOUS" && diagnostic.blocking
            })
        );
    }
}
