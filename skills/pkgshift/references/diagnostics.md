# Diagnostics

Use stable diagnostic codes as the decision surface. Messages may improve without changing automation behavior.

## Diagnostic Shape

A diagnostic should provide:

- `code`: stable identifier.
- `severity`: informational, warning, or error.
- `summary`: concise human-readable statement.
- `evidence`: redacted repository facts and locations.
- `explanation`: optional expanded reasoning.
- `remediation`: zero or more structured choices.
- `blocking`: whether the current operation may continue.

## Handling Rules

1. Read the structured fields before presenting a message.
2. Use `pkgshift explain <code-or-artifact-id>` when the code is unfamiliar or the evidence is insufficient.
3. Present blocking diagnostics before warnings.
4. Never downgrade a blocking diagnostic based on intuition.
5. Never expose redacted values while gathering more context.
6. Re-plan after remediation changes repository evidence.

## Baseline Codes

The implementation may add codes while preserving the following baseline families:

| Code | Meaning |
| --- | --- |
| `PKGSHIFT_CLI_NOT_FOUND` | No trusted pkgshift executable is available. |
| `DIAGNOSTIC_CODE_UNKNOWN` | The requested explanation code is not registered by this CLI schema. |
| `ARTIFACT_NOT_FOUND` | The selected state directory has no stored artifact with the requested canonical identifier. |
| `ARTIFACT_INVALID` | Stored state failed schema, identity, path, or content-integrity validation and must not be trusted. |
| `PM_SOURCE_AMBIGUOUS` | Repository evidence supports multiple source package managers. |
| `PM_TARGET_UNSUPPORTED` | The requested target is outside the supported adapter boundary. |
| `CAPABILITY_LOSSY` | A source capability requires a semantic compromise. |
| `CAPABILITY_UNSUPPORTED` | No safe target representation exists. |
| `CAPABILITY_UNKNOWN` | Target behavior lacks enough authoritative evidence for a safe decision. |
| `LOSSY_ACCEPTANCE_REQUIRED` | Lossy decisions were not accepted while creating the immutable plan. |
| `NATIVE_IMPORT_UNAVAILABLE` | No verified target-native importer exists for the selected direction; target graph proof remains required. |
| `SOURCE_RUNTIME_REFERENCES_PRESERVED` | Bun runtime dependencies, scripts, globals, or module imports remain outside package-manager cleanup and require intentional retention or a separate runtime migration. |
| `RUNTIME_BUN_SOURCE_NOT_DETECTED` | The dedicated runtime command found no Bun application runtime evidence in its bounded inspection surface. |
| `DENO_PERMISSION_REQUIRED` | A safe runtime recipe requires an explicit plan-bound Deno permission that was not supplied. |
| `RUNTIME_BUN_SERVE_UNSUPPORTED` | A `Bun.serve` call contains routes, WebSockets, lifecycle hooks, or another shape outside the fetch-handler recipe. |
| `RUNTIME_SOURCE_FILE_TOO_LARGE` | A runtime input exceeds the bounded deterministic inspection limit. |
| `RUNTIME_SOURCE_SYMLINK_UNSUPPORTED` | A runtime source or source directory crosses a symbolic-link boundary. |
| `RUNTIME_BUN_MODULE_UNSUPPORTED` | A Bun-specific module import remains outside the registered runtime recipe set. |
| `RUNTIME_BUN_GLOBAL_UNSUPPORTED` | A Bun global API remains after safe recipes are evaluated. |
| `RUNTIME_BUN_SCRIPT_UNSUPPORTED` | A Bun package script contains flags or shell semantics outside direct Deno command translation. |
| `RUNTIME_BUN_RESIDUE_REMAINS` | Verification detected Bun application runtime evidence after approved mutation. |
| `RUNTIME_ROLLBACK_FAILED` | Runtime snapshots did not restore the exact pre-plan fingerprint. |
| `INTEGRATION_COMMAND_AMBIGUOUS` | A source package-manager command remains in an executable integration context outside the deterministic command subset. |
| `INTEGRATION_SETUP_ACTION_UNSUPPORTED` | A source CI setup action has no registered target action replacement. |
| `INTEGRATION_CACHE_UNSUPPORTED` | The source CI cache input cannot represent the selected target safely. |
| `INTEGRATION_DEVCONTAINER_COMMAND_UNSUPPORTED` | A devcontainer lifecycle command uses an object or array shape outside the string-command renderer. |
| `VERIFICATION_SCRIPT_INVALID` | A requested representative root script name cannot be stored safely in the plan. |
| `VERIFICATION_SCRIPT_NOT_FOUND` | The root package does not define the explicitly requested representative script. |
| `SCRIPT_VERIFICATION_EXECUTION_FAILED` | The approved representative script process could not be started, observed, or bounded safely. |
| `COMPARISON_TARGET_COUNT_INVALID` | Fewer than two distinct target adapters remain after normalization. |
| `COMPARISON_CANDIDATE_BLOCKED` | A candidate remains visible in comparison evidence but cannot execute its isolated trial. |
| `COMPARISON_NO_EXECUTABLE_TARGETS` | Every candidate target plan is capability-blocked. |
| `COMPARISON_REPOSITORY_CHANGED` | Source repository evidence changed while isolated candidate trials were running. |
| `LOCK_GRAPH_PARSE_FAILED` | A source or target lockfile could not produce a trustworthy normalized graph. |
| `LOCK_GRAPH_FORMAT_UNSUPPORTED` | The lockfile format, including binary `bun.lockb`, cannot be proven safely. |
| `PLAN_PRECONDITION_FAILED` | Repository evidence changed after planning. |
| `REPOSITORY_TRANSACTION_BUSY` | Another agent or run owns the repository transaction lock. |
| `APPROVAL_REQUIRED` | A mutating command lacks the exact artifact-bound approval token. |
| `SKILL_SOURCE_NOT_FOUND` | The portable Agent Skill source is unavailable from the distribution or source checkout. |
| `SKILL_SOURCE_INVALID` | Portable Agent Skill content or frontmatter failed validation. |
| `SKILL_TARGET_PATH_UNSAFE` | A project or user destination crosses a symbolic-link or non-directory parent. |
| `SKILL_INSTALL_CONFLICT` | The selected destination has different ownership, content type, or concurrent state. |
| `SKILL_INSTALL_MODIFIED` | A managed copy differs from the portable source and requires review. |
| `SKILL_UNINSTALL_MODIFIED` | Protected uninstall refused to remove a locally modified managed copy. |
| `INSTALL_COMMAND_FAILED` | The target install command failed after journaling began. |
| `VERIFICATION_FAILED` | One or more structural post-apply checks failed. |
| `ROLLBACK_FAILED` | Recovery data did not restore and verify the repository baseline. |
| `ROLLBACK_EXTERNAL_EFFECTS_REMAIN` | Repository files were restored but dependency state outside the snapshot remains. |
| `SECRET_REDACTION_FAILED` | Output or persistence cannot be proven safe for sensitive values. |

Treat `SECRET_REDACTION_FAILED` and internal trust failures as hard stops.
