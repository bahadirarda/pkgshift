use super::DiagnosticExplanation;

pub(super) static ENTRIES: &[DiagnosticExplanation] = &[
    DiagnosticExplanation::new(
        "SKILL_SOURCE_NOT_FOUND",
        "Portable Skill source not found",
        "The executable could not resolve its canonical bundled Agent Skill directory.",
        &["Use a native release archive or set PKGSHIFT_DATA_DIR to trusted release data."],
    ),
    DiagnosticExplanation::new(
        "SKILL_SOURCE_INVALID",
        "Portable Skill source invalid",
        "The canonical Skill bundle failed structure, frontmatter, file-type, or content validation.",
        &["Reinstall pkgshift from a verified release archive."],
    ),
    DiagnosticExplanation::new(
        "SKILL_SOURCE_CHANGED",
        "Portable Skill source changed",
        "The canonical source digest changed after the approval-bound install plan was created.",
        &["Inspect the new source and create a new install plan."],
    ),
    DiagnosticExplanation::new(
        "SKILL_TARGET_PATH_UNSAFE",
        "Skill destination path unsafe",
        "The destination escapes its scope or traverses a symbolic-link parent.",
        &["Use a regular client Skill directory confined to the selected scope."],
    ),
    DiagnosticExplanation::new(
        "SKILL_PATH_TYPE_UNSAFE",
        "Skill path type unsafe",
        "A managed Skill path has an unexpected file type.",
        &["Review the exact path without following links and resolve it manually."],
    ),
    DiagnosticExplanation::new(
        "SKILL_INSTALL_CONFLICT",
        "Skill installation conflict",
        "The selected destination contains a different or unmanaged installation.",
        &["Review the destination before installing or uninstalling."],
    ),
    DiagnosticExplanation::new(
        "SKILL_INSTALL_MODIFIED",
        "Installed Skill modified",
        "A managed copy differs from the canonical portable source.",
        &["Preserve or review local edits before replacing it."],
    ),
    DiagnosticExplanation::new(
        "SKILL_UNINSTALL_MODIFIED",
        "Modified Skill cannot be removed",
        "pkgshift refuses to delete a managed copy containing local changes.",
        &["Back up or remove local changes manually before uninstalling."],
    ),
    DiagnosticExplanation::new(
        "SKILL_UNINSTALL_SOURCE_UNVERIFIED",
        "Skill source ownership unverified",
        "The installed link or copy cannot be proven to belong to the current canonical source.",
        &["Review the destination and remove it manually only if ownership is known."],
    ),
    DiagnosticExplanation::new(
        "SKILL_USER_ROOT_NOT_FOUND",
        "User Skill root unavailable",
        "No supported user-home directory could be resolved for user-scope installation.",
        &["Set the platform user-home environment and retry."],
    ),
    DiagnosticExplanation::new(
        "SKILL_OPERATION_FAILED",
        "Skill operation failed",
        "A validated install or uninstall filesystem transaction could not complete.",
        &["Inspect the destination and retry from a fresh approval plan."],
    ),
];
