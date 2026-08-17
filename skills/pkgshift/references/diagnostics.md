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
| `PM_SOURCE_AMBIGUOUS` | Repository evidence supports multiple source package managers. |
| `PM_TARGET_UNSUPPORTED` | The requested target is outside the supported adapter boundary. |
| `CAPABILITY_LOSSY` | A source capability requires a semantic compromise. |
| `CAPABILITY_UNSUPPORTED` | No safe target representation exists. |
| `CAPABILITY_UNKNOWN` | Target behavior lacks enough authoritative evidence for a safe decision. |
| `LOSSY_ACCEPTANCE_REQUIRED` | Lossy decisions were not accepted while creating the immutable plan. |
| `NATIVE_IMPORT_UNAVAILABLE` | No verified target-native importer exists for the selected direction; target graph proof remains required. |
| `SOURCE_RUNTIME_REFERENCES_PRESERVED` | Bun runtime dependencies, scripts, globals, or module imports remain outside package-manager cleanup and require intentional retention or a separate runtime migration. |
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
| `INSTALL_COMMAND_FAILED` | The target install command failed after journaling began. |
| `VERIFICATION_FAILED` | One or more structural post-apply checks failed. |
| `ROLLBACK_FAILED` | Recovery data did not restore and verify the repository baseline. |
| `ROLLBACK_EXTERNAL_EFFECTS_REMAIN` | Repository files were restored but dependency state outside the snapshot remains. |
| `SECRET_REDACTION_FAILED` | Output or persistence cannot be proven safe for sensitive values. |

Treat `SECRET_REDACTION_FAILED` and internal trust failures as hard stops.
