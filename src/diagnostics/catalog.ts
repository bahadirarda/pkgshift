export interface DiagnosticExplanation {
  code: string;
  title: string;
  explanation: string;
  remediation: string[];
}

const EXPLANATIONS: DiagnosticExplanation[] = [
  {
    code: "CLI_INVALID_INPUT",
    title: "Invalid command input",
    explanation: "The command or one of its options does not match the supported CLI grammar.",
    remediation: ["Run pkgshift help and retry with a documented command."],
  },
  {
    code: "DIAGNOSTIC_CODE_UNKNOWN",
    title: "Diagnostic code unknown",
    explanation: "The requested code is not registered in the current diagnostic catalog.",
    remediation: ["Use a code returned by the same CLI schema version."],
  },
  {
    code: "PKGSHIFT_CLI_NOT_FOUND",
    title: "pkgshift CLI not found",
    explanation: "No trusted project-provided or installed pkgshift executable could be resolved.",
    remediation: ["Install pkgshift through a trusted project or user workflow, then retry inspection."],
  },
  {
    code: "PKGSHIFT_INTERNAL_ERROR",
    title: "Internal error",
    explanation: "The CLI could not produce a trustworthy domain result for the requested operation.",
    remediation: ["Stop before mutation, retain the command context, and report the failure for investigation."],
  },
  {
    code: "ARTIFACT_STORE_FAILED",
    title: "Artifact persistence failed",
    explanation: "The plan was created but could not be persisted to the explicitly selected state directory.",
    remediation: ["Check the state directory, preserve the JSON result, and retry persistence before apply."],
  },
  {
    code: "REPOSITORY_ROOT_NOT_FOUND",
    title: "Repository root not found",
    explanation: "The selected working directory does not exist or is not a directory.",
    remediation: ["Pass --cwd with an existing repository directory."],
  },
  {
    code: "MANIFEST_NOT_FOUND",
    title: "Project manifest not found",
    explanation: "The selected repository root does not contain a package.json manifest.",
    remediation: ["Run the command from a JavaScript project root or pass --cwd with the intended root."],
  },
  {
    code: "MANIFEST_INVALID_JSON",
    title: "Project manifest is invalid",
    explanation: "The package.json file could not be parsed as a JSON object.",
    remediation: ["Repair package.json syntax and run inspection again."],
  },
  {
    code: "PM_SOURCE_NOT_DETECTED",
    title: "Source package manager not detected",
    explanation: "Repository evidence is insufficient to select a source package manager safely.",
    remediation: ["Add or repair an explicit packageManager field, or provide additional source evidence."],
  },
  {
    code: "PM_PACKAGE_MANAGER_FIELD_UNKNOWN",
    title: "Package manager field unknown",
    explanation: "The packageManager manifest field does not identify a supported adapter and version.",
    remediation: ["Use a supported package manager name with an explicit version."],
  },
  {
    code: "PM_SOURCE_AMBIGUOUS",
    title: "Source package manager is ambiguous",
    explanation: "Strong repository evidence points to more than one source package manager.",
    remediation: ["Review the reported evidence, remove stale configuration, or select a source explicitly in a future plan option."],
  },
  {
    code: "PM_CONFLICTING_EVIDENCE",
    title: "Package manager evidence conflicts",
    explanation: "A source was selected, but other strong package manager evidence remains in the repository.",
    remediation: ["Review whether the additional lockfiles or configuration are stale before applying a migration."],
  },
  {
    code: "PM_TARGET_UNSUPPORTED",
    title: "Target package manager unsupported",
    explanation: "The requested target does not match a registered production or preview adapter.",
    remediation: ["Run pkgshift support --json and select an adapter identifier from the result."],
  },
  {
    code: "PM_TARGET_PREVIEW",
    title: "Target adapter is in preview",
    explanation: "The target adapter does not yet carry production migration guarantees.",
    remediation: ["Review capability gaps and require an explicit preview gate when execution becomes available."],
  },
  {
    code: "CAPABILITY_LOSSY",
    title: "Capability transformation is lossy",
    explanation: "The target can accept a deterministic transformation, but some source semantics or centralized policy will be lost.",
    remediation: ["Review the capability decision and explicitly accept the semantic compromise before apply."],
  },
  {
    code: "CAPABILITY_UNSUPPORTED",
    title: "Capability unsupported",
    explanation: "The selected target has no safe representation for an observed source capability.",
    remediation: ["Remove the source capability, select another target, or add a verified adapter rule."],
  },
  {
    code: "CAPABILITY_UNKNOWN",
    title: "Capability support unknown",
    explanation: "The adapter lacks authoritative evidence to classify an observed capability for the selected target.",
    remediation: ["Gather authoritative target evidence or select a target with a known result."],
  },
  {
    code: "CONFIGURATION_PARSE_FAILED",
    title: "Configuration parsing failed",
    explanation: "A migration-relevant configuration file could not be parsed into a trustworthy semantic model.",
    remediation: ["Repair the configuration file and inspect the repository again."],
  },
  {
    code: "DEPENDENCY_SECTION_INVALID",
    title: "Dependency section invalid",
    explanation: "A package manifest dependency section is not a JSON object.",
    remediation: ["Repair the manifest dependency section before planning."],
  },
  {
    code: "DEPENDENCY_SPECIFIER_INVALID",
    title: "Dependency specifier invalid",
    explanation: "A package manifest dependency value is not a supported string specifier.",
    remediation: ["Replace the value with a supported dependency string."],
  },
  {
    code: "WORKSPACE_MANIFEST_INVALID",
    title: "Workspace manifest invalid",
    explanation: "A manifest selected by workspace membership could not be parsed safely.",
    remediation: ["Repair the workspace manifest before planning."],
  },
  {
    code: "PM_TARGET_ALREADY_SELECTED",
    title: "Target already selected",
    explanation: "The detected source and requested target are the same adapter.",
    remediation: ["Select a different target or use verification instead of migration."],
  },
  {
    code: "PLAN_PRECONDITION_FAILED",
    title: "Plan precondition failed",
    explanation: "Migration-relevant repository evidence changed after the plan was created.",
    remediation: ["Inspect and create a new plan from the current repository state."],
  },
  {
    code: "REPOSITORY_TRANSACTION_BUSY",
    title: "Repository transaction already active",
    explanation: "Another apply, verify, or rollback transaction owns the repository-scoped migration lock.",
    remediation: ["Wait for the active transaction to finish, then inspect repository state before retrying."],
  },
  {
    code: "LOSSY_ACCEPTANCE_REQUIRED",
    title: "Lossy capability acceptance required",
    explanation: "The plan contains reviewed transformations that cannot preserve all source semantics.",
    remediation: ["Review every lossy decision and create a new plan with --accept-lossy."],
  },
  {
    code: "TRANSFORMATION_UNIMPLEMENTED",
    title: "Transformation unavailable",
    explanation: "A capability exists in the target, but the current execution adapter cannot render it deterministically.",
    remediation: ["Remove the feature, select another target, or wait for an adapter implementation."],
  },
  {
    code: "TRANSFORMATION_MANUAL_REQUIRED",
    title: "Manual transformation required",
    explanation: "The source policy cannot be reduced automatically without creating an unsafe target configuration.",
    remediation: ["Resolve the policy manually and create a new plan."],
  },
  {
    code: "WORKSPACE_VERSION_REQUIRED",
    title: "Workspace version required",
    explanation: "A workspace protocol must become a semver range, but the referenced workspace has no version.",
    remediation: ["Add a package version or use a target that supports the workspace protocol."],
  },
  {
    code: "WORKSPACE_PATTERN_UNSUPPORTED",
    title: "Workspace pattern unsupported",
    explanation: "Workspace membership uses glob syntax outside the deterministic MVP matcher.",
    remediation: ["Expand the pattern into literal, *, **, ?, or leading-exclusion entries before planning."],
  },
  {
    code: "WORKSPACE_SPECIFIER_UNSUPPORTED",
    title: "Workspace specifier unsupported",
    explanation: "A workspace dependency uses an alias, path, or range outside the deterministic semver expansion subset.",
    remediation: ["Use workspace:*, workspace:^, workspace:~, or a simple explicit semver range before planning."],
  },
  {
    code: "CATALOG_ENTRY_NOT_FOUND",
    title: "Catalog entry missing",
    explanation: "A catalog dependency does not resolve through the source default or named catalog.",
    remediation: ["Repair the source catalog and inspect again."],
  },
  {
    code: "REGISTRY_SECRET_REQUIRES_ENVIRONMENT_REFERENCE",
    title: "Registry secret requires environment reference",
    explanation: "A literal registry token cannot enter a persisted migration plan.",
    remediation: ["Replace the literal value with an ${ENV_VAR} reference and re-plan."],
  },
  {
    code: "NPMRC_SETTING_UNSUPPORTED",
    title: "npm configuration setting unsupported",
    explanation: "Yarn Modern translation cannot preserve an observed .npmrc setting deterministically.",
    remediation: ["Move the setting to an equivalent Yarn configuration or remove it before planning."],
  },
  {
    code: "APPROVAL_REQUIRED",
    title: "Exact approval required",
    explanation: "A mutating operation did not receive the exact plan, run, or skill approval token.",
    remediation: ["Review the artifact and retry with the exact --approve value reported by the CLI."],
  },
  {
    code: "PLAN_NOT_EXECUTABLE",
    title: "Plan is not executable",
    explanation: "The plan is blocked, targets a preview adapter, or represents a no-op migration.",
    remediation: ["Resolve blocking diagnostics and create a production-target plan."],
  },
  {
    code: "PLAN_ARTIFACT_INVALID",
    title: "Plan artifact invalid",
    explanation: "The stored plan is missing required execution or verification data.",
    remediation: ["Discard the artifact and create a new plan."],
  },
  {
    code: "INSTALL_COMMAND_FAILED",
    title: "Target installation failed",
    explanation: "The target package manager exited unsuccessfully while generating dependency state.",
    remediation: ["Inspect the redacted process artifact, then repair or roll back the run."],
  },
  {
    code: "INSTALL_COMMAND_TIMEOUT",
    title: "Target installation timed out",
    explanation: "The target package manager exceeded the bounded execution time.",
    remediation: ["Inspect environment health and roll back before retrying."],
  },
  {
    code: "SNAPSHOT_CREATE_FAILED",
    title: "Recovery snapshot failed",
    explanation: "pkgshift could not persist recovery material before repository mutation.",
    remediation: ["Repair state storage and do not bypass the snapshot boundary."],
  },
  {
    code: "VERIFICATION_FAILED",
    title: "Migration verification failed",
    explanation: "One or more post-apply checks do not match the approved plan.",
    remediation: ["Inspect the verification report, then repair or roll back the run."],
  },
  {
    code: "VERIFICATION_RUN_STATE_INVALID",
    title: "Run cannot be verified",
    explanation: "The run is not waiting for verification.",
    remediation: ["Select a run whose apply phase completed successfully."],
  },
  {
    code: "ROLLBACK_FAILED",
    title: "Rollback failed",
    explanation: "Recovery data could not restore or verify the repository baseline.",
    remediation: ["Preserve the state directory and inspect snapshot integrity before retrying."],
  },
  {
    code: "ROLLBACK_EXTERNAL_EFFECTS_REMAIN",
    title: "External dependency effects remain",
    explanation: "Repository files were restored, but node_modules and package-manager caches are outside snapshot scope.",
    remediation: ["Reinstall the source dependency state when exact local dependency parity is required."],
  },
  {
    code: "SKILL_INSTALL_CONFLICT",
    title: "Skill installation conflict",
    explanation: "The selected Agent Skill destination contains a different or unsafe installation.",
    remediation: ["Review the existing destination before installing or uninstalling."],
  },
  {
    code: "SKILL_TARGET_PATH_UNSAFE",
    title: "Skill destination path unsafe",
    explanation: "The selected Agent Skill destination traverses a symbolic-link parent or escapes its declared scope root.",
    remediation: ["Use a project or user skill directory whose parent path remains confined to the declared scope."],
  },
  {
    code: "SKILL_INSTALL_MODIFIED",
    title: "Installed skill modified",
    explanation: "A managed skill copy differs from the portable source bundled with pkgshift.",
    remediation: ["Preserve or review local edits before updating the installation."],
  },
  {
    code: "SKILL_UNINSTALL_MODIFIED",
    title: "Modified skill cannot be removed",
    explanation: "pkgshift refuses to delete a managed skill that contains local changes.",
    remediation: ["Back up or remove local changes manually before uninstalling."],
  },
  {
    code: "SECRET_REDACTION_FAILED",
    title: "Secret redaction failed",
    explanation: "The operation cannot prove that sensitive values are safe to render or persist.",
    remediation: ["Stop the operation and correct the redaction boundary before continuing."],
  },
];

export function explainDiagnostic(
  code: string,
): DiagnosticExplanation | null {
  return EXPLANATIONS.find((candidate) => candidate.code === code) ?? null;
}
