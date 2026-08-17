use std::collections::BTreeSet;

use crate::model::{
    Diagnostic, DiagnosticSeverity, EvidenceDetail, PackageManagerId, PlannedOperation,
    ProcessExecutionRecord, ProjectIr, SideEffect, VerificationCheck, VerificationStatus,
};

pub(crate) const OPERATION_KIND: &str = "verification.run-script";
const DEFAULT_TIMEOUT_SECONDS: u64 = 300;

fn runner(target: PackageManagerId) -> &'static str {
    match target {
        PackageManagerId::YarnClassic | PackageManagerId::YarnModern => "yarn",
        PackageManagerId::Npm => "npm",
        PackageManagerId::Pnpm => "pnpm",
        PackageManagerId::Bun => "bun",
        PackageManagerId::Vlt => "vlt",
        PackageManagerId::Deno => "deno",
    }
}

fn command(target: PackageManagerId, script: &str) -> Vec<String> {
    let subcommand = if target == PackageManagerId::Deno {
        "task"
    } else {
        "run"
    };
    vec![
        runner(target).to_owned(),
        subcommand.to_owned(),
        script.to_owned(),
    ]
}

fn diagnostic(code: &str, summary: String, evidence: Vec<EvidenceDetail>) -> Diagnostic {
    Diagnostic {
        code: code.to_owned(),
        severity: DiagnosticSeverity::Error,
        summary,
        blocking: true,
        evidence,
        remediation: vec![
            "Choose a root package.json script name with --verify-script and create a new plan."
                .to_owned(),
        ],
    }
}

pub(crate) fn plan_operations(
    start_index: usize,
    project: &ProjectIr,
    target: PackageManagerId,
    requested: &[String],
) -> (Vec<PlannedOperation>, Vec<Diagnostic>) {
    let requested = requested.iter().cloned().collect::<BTreeSet<_>>();
    if requested.is_empty() {
        return (Vec::new(), Vec::new());
    }
    let Some(root_package) = project
        .packages
        .iter()
        .find(|package| package.path == project.root_package_path)
    else {
        return (
            Vec::new(),
            vec![diagnostic(
                "VERIFICATION_ROOT_PACKAGE_MISSING",
                "The root package could not be resolved for representative script verification."
                    .to_owned(),
                vec![EvidenceDetail {
                    location: project.root_package_path.clone(),
                    detail: "configured root package path".to_owned(),
                }],
            )],
        );
    };
    let available = root_package
        .script_names
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let mut operations = Vec::new();
    let mut diagnostics = Vec::new();
    for script in requested {
        if script.is_empty()
            || script.starts_with('-')
            || script.len() > 128
            || script.chars().any(char::is_control)
            || script.trim() != script
        {
            diagnostics.push(diagnostic(
                "VERIFICATION_SCRIPT_INVALID",
                "A representative script name is not safe to store in a migration plan.".to_owned(),
                vec![EvidenceDetail {
                    location: root_package.manifest_path.clone(),
                    detail: format!("invalid requested script: {script:?}"),
                }],
            ));
            continue;
        }
        if !available.contains(script.as_str()) {
            diagnostics.push(diagnostic(
                "VERIFICATION_SCRIPT_NOT_FOUND",
                format!("The root package does not define the {script:?} script."),
                vec![EvidenceDetail {
                    location: root_package.manifest_path.clone(),
                    detail: format!("missing requested script: {script}"),
                }],
            ));
            continue;
        }
        let argv = command(target, &script);
        operations.push(PlannedOperation {
            id: format!("op_{:03}", start_index + operations.len()),
            phase: "verify".to_owned(),
            kind: OPERATION_KIND.to_owned(),
            description: format!("Run the explicitly selected {script:?} script with {target}."),
            paths: vec![root_package.manifest_path.clone()],
            command: argv,
            timeout_seconds: Some(DEFAULT_TIMEOUT_SECONDS),
            capabilities: vec!["verification.representative-script".to_owned()],
            side_effect: SideEffect::ProcessExecution,
            reversible: false,
            preconditions: vec![format!(
                "The root package still defines the {script:?} script from the accepted plan."
            )],
            postconditions: vec![format!(
                "The {script:?} script exits successfully within {DEFAULT_TIMEOUT_SECONDS} seconds."
            )],
            mutations: Vec::new(),
        });
    }
    (operations, diagnostics)
}

pub(crate) fn verification_check(
    plan: &crate::model::MigrationPlan,
    processes: &[ProcessExecutionRecord],
) -> VerificationCheck {
    let operations = plan
        .operations
        .iter()
        .filter(|operation| operation.kind == OPERATION_KIND)
        .collect::<Vec<_>>();
    if operations.is_empty() {
        return VerificationCheck {
            id: "representative-scripts".to_owned(),
            status: VerificationStatus::Skipped,
            summary: "No representative scripts were explicitly selected during planning."
                .to_owned(),
            evidence: vec!["selected:0".to_owned()],
        };
    }

    let mut evidence = Vec::with_capacity(operations.len());
    let mut passed = 0;
    for operation in &operations {
        let script = operation.command.last().map_or("unknown", String::as_str);
        match processes
            .iter()
            .find(|process| process.operation_id == operation.id)
        {
            Some(process) if process.success && !process.timed_out => {
                passed += 1;
                evidence.push(format!(
                    "script:{script};status:passed;durationMillis:{}",
                    process.duration_millis.unwrap_or_default()
                ));
            }
            Some(process) if process.timed_out => evidence.push(format!(
                "script:{script};status:timed-out;timeoutSeconds:{}",
                operation.timeout_seconds.unwrap_or(DEFAULT_TIMEOUT_SECONDS)
            )),
            Some(process) => evidence.push(format!(
                "script:{script};status:failed;exitCode:{}",
                process
                    .exit_code
                    .map_or_else(|| "signal".to_owned(), |code| code.to_string())
            )),
            None => evidence.push(format!("script:{script};status:not-run")),
        }
    }
    let all_passed = passed == operations.len();
    VerificationCheck {
        id: "representative-scripts".to_owned(),
        status: if all_passed {
            VerificationStatus::Passed
        } else {
            VerificationStatus::Failed
        },
        summary: if all_passed {
            format!("All {passed} explicitly selected representative scripts passed.")
        } else {
            format!(
                "{passed} of {} explicitly selected representative scripts passed.",
                operations.len()
            )
        },
        evidence,
    }
}
