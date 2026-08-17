use super::DiagnosticExplanation;

pub(super) static ENTRIES: &[DiagnosticExplanation] = &[
    DiagnosticExplanation::new(
        "CLI_INVALID_INPUT",
        "Invalid command input",
        "The command or one of its options does not match the supported CLI grammar.",
        &["Run pkgshift help and retry with a documented command."],
    ),
    DiagnosticExplanation::new(
        "DIAGNOSTIC_CODE_UNKNOWN",
        "Diagnostic code unknown",
        "The requested code is not registered in the current diagnostic catalog.",
        &["Use a code returned by the same CLI schema version."],
    ),
    DiagnosticExplanation::new(
        "ARTIFACT_NOT_FOUND",
        "Stored artifact not found",
        "The selected state directory does not contain the requested artifact identifier.",
        &["Confirm the identifier and pass the state directory that created the artifact."],
    ),
    DiagnosticExplanation::new(
        "ARTIFACT_INVALID",
        "Stored artifact invalid",
        "The artifact failed schema, identity, path, or content-integrity validation.",
        &["Preserve the state directory, discard untrusted output, and create a new artifact."],
    ),
    DiagnosticExplanation::new(
        "ARTIFACT_STORE_FAILED",
        "Artifact persistence failed",
        "A result was created but could not be persisted to the selected state directory.",
        &["Repair state-directory access and persist a new plan before apply."],
    ),
    DiagnosticExplanation::new(
        "PKGSHIFT_CLI_NOT_FOUND",
        "pkgshift CLI not found",
        "No trusted project-provided or installed pkgshift executable could be resolved.",
        &["Install pkgshift through a trusted project or user workflow, then retry."],
    ),
    DiagnosticExplanation::new(
        "PKGSHIFT_INTERNAL_ERROR",
        "Internal error",
        "The CLI could not produce a trustworthy domain result for the requested operation.",
        &["Stop before mutation, retain the command context, and report the failure."],
    ),
    DiagnosticExplanation::new(
        "REPOSITORY_ROOT_NOT_FOUND",
        "Repository root not found",
        "The selected working directory does not exist or is not a directory.",
        &["Pass --cwd with an existing repository directory."],
    ),
    DiagnosticExplanation::new(
        "APPROVAL_REQUIRED",
        "Exact approval required",
        "A mutating operation did not receive the exact content-bound plan or run identifier.",
        &["Review the artifact and execute the returned nextActions argv unchanged."],
    ),
    DiagnosticExplanation::new(
        "PLAN_PRECONDITION_FAILED",
        "Plan precondition failed",
        "Migration-relevant evidence changed after the approved plan was created.",
        &["Inspect the current repository and create a new plan."],
    ),
    DiagnosticExplanation::new(
        "TARGET_EXECUTABLE_UNAVAILABLE",
        "Target executable unavailable",
        "The exact target program declared by the plan could not be resolved to an executable file.",
        &[
            "Install or activate the plan's package-manager pin on PATH, then retry the unchanged approved command.",
        ],
    ),
    DiagnosticExplanation::new(
        "TARGET_EXECUTABLE_VERSION_MISMATCH",
        "Target executable version mismatch",
        "The bounded version probe failed or the resolved target program did not report the exact planned version.",
        &[
            "Activate the exact package-manager pin declared by targetExecutable, then retry before changing the plan.",
        ],
    ),
    DiagnosticExplanation::new(
        "REPOSITORY_TRANSACTION_BUSY",
        "Repository transaction active",
        "Another apply, verify, or rollback transaction owns the repository lock.",
        &["Wait for the active transaction and inspect repository state before retrying."],
    ),
    DiagnosticExplanation::new(
        "EXECUTION_FAILED",
        "Execution failed",
        "An approved repository operation could not complete its planned mutation sequence.",
        &["Inspect the run journal and use rollback when recovery is available."],
    ),
    DiagnosticExplanation::new(
        "INSTALL_COMMAND_FAILED",
        "Target installation failed",
        "The target package manager exited unsuccessfully while generating dependency state.",
        &["Inspect the redacted process artifact, then repair or roll back the run."],
    ),
    DiagnosticExplanation::new(
        "INSTALL_COMMAND_TIMEOUT",
        "Target installation timed out",
        "The target package manager exceeded the bounded execution deadline.",
        &["Inspect environment health and roll back before retrying."],
    ),
    DiagnosticExplanation::new(
        "SNAPSHOT_CREATE_FAILED",
        "Recovery snapshot failed",
        "Recovery material could not be persisted before repository mutation.",
        &["Repair state storage and do not bypass the snapshot boundary."],
    ),
    DiagnosticExplanation::new(
        "VERIFICATION_FAILED",
        "Migration verification failed",
        "One or more post-apply checks do not match the approved plan.",
        &["Inspect the verification report, then repair or roll back the run."],
    ),
    DiagnosticExplanation::new(
        "VERIFICATION_RUN_STATE_INVALID",
        "Run cannot be verified",
        "The selected run is not in a state that permits verification.",
        &["Select a run whose apply phase completed successfully."],
    ),
    DiagnosticExplanation::new(
        "VERIFICATION_ROOT_PACKAGE_MISSING",
        "Root package missing",
        "Verification could not find the root package represented by the approved plan.",
        &["Restore the expected root manifest or roll back the run."],
    ),
    DiagnosticExplanation::new(
        "VERIFICATION_SCRIPT_INVALID",
        "Representative script invalid",
        "A selected verification script does not have a safe executable manifest value.",
        &["Repair the root script and create a new plan."],
    ),
    DiagnosticExplanation::new(
        "VERIFICATION_SCRIPT_NOT_FOUND",
        "Representative script missing",
        "A requested verification script is absent from the root package manifest.",
        &["Select an existing root script and create a new plan."],
    ),
    DiagnosticExplanation::new(
        "SCRIPT_VERIFICATION_EXECUTION_FAILED",
        "Representative script failed",
        "An explicitly selected post-migration script exited unsuccessfully or timed out.",
        &["Inspect the withheld process metadata and roll back or repair the migration."],
    ),
    DiagnosticExplanation::new(
        "ROLLBACK_FAILED",
        "Rollback failed",
        "Recovery data could not restore or verify the repository baseline.",
        &["Preserve the state directory and inspect snapshot integrity before retrying."],
    ),
    DiagnosticExplanation::new(
        "ROLLBACK_EXTERNAL_EFFECTS_REMAIN",
        "External dependency effects remain",
        "Repository files were restored, but caches and generated dependency directories are outside the snapshot.",
        &["Reinstall source dependency state when exact local parity is required."],
    ),
    DiagnosticExplanation::new(
        "SECRET_REDACTION_FAILED",
        "Secret redaction failed",
        "The operation cannot prove that sensitive values are safe to render or persist.",
        &["Stop the operation and correct the redaction boundary before continuing."],
    ),
];
