# CLI Contract

Use this reference when parsing a pkgshift result or deciding whether a follow-up action is authorized.

## Canonical Commands

```text
pkgshift to <target> [--dry-run|--trial] [--verify-script <name>]... [--target-platform <target>]... [--edge-equivalence <policy>]
pkgshift doctor [--to <target>] [--verify-script <name>]... [--target-platform <target>]... [--edge-equivalence <policy>]
pkgshift compare <target> <target>... [--verify-script <name>]... [--target-platform <target>]... [--edge-equivalence <policy>]
pkgshift runtime to deno [--deno-permission <name>]...
pkgshift runtime rollback <runtime-run-id> --state-dir <path> --approve <runtime-run-id>
pkgshift inspect [package-manager]
pkgshift plan package-manager --to <target> [--target-platform <target>]... [--edge-equivalence <policy>]
pkgshift apply <plan-id> --state-dir <path> --approve <plan-id>
pkgshift verify <run-id> --state-dir <path>
pkgshift explain <diagnostic-code-or-artifact-id> [--state-dir <path>]
pkgshift rollback <run-id> --state-dir <path> --approve <run-id>
pkgshift skill install --scope <project|user> --client <codex|claude> --mode <copy|link>
pkgshift skill status|doctor|uninstall --scope <project|user> --client <codex|claude>
```

Add `--json --no-color --non-interactive` for agent operation. Prefer `pkgshift to <target>` for an ordinary migration. Its first call is read-only and returns an approval-bound next action; the approved call persists, applies, and verifies without caller-supplied paths. `--trial` returns a separately approved process-execution action that runs in a disposable copy and never authorizes apply. Add repeatable `--verify-script <name>` values only for exact root scripts selected by the user. Add repeatable `--target-platform OS/CPU[/LIBC]` values only for declared deployment targets, and select `--edge-equivalence strict` only when normalized dependency-edge parity is required. Returned next actions preserve all normalized values. Use `--accept-lossy` only after the user accepts every lossy capability decision.

`doctor --to <target>` is the target-specific readiness surface. It reuses canonical inspection, Project IR, capability, lock graph, and planning logic but returns a `migration-readiness` projection instead of a package-manager plan. It never persists state, executes a process, emits mutation contents, or changes the repository. Treat `migrationAvailable` as authoritative. A `doctor_...` identifier addresses evidence only and cannot approve another command. Its optional next action is read-only, requires no approval, and creates the complete plan; when `availableAfterReview` is true, obtain explicit lossy acceptance before executing the returned array.

`doctor` without `--to` assesses all seven production adapters from one repository evidence context and returns one `migration-readiness-matrix`. Its nested reports use the target-specific contract. Top-level completion means the matrix is trustworthy, not that every candidate is executable. Do not rank or select a winner. Candidate planning actions remain independent, read-only, and absent for hard-blocked or already-selected targets.

The explicit `plan`, `apply`, and `verify` commands are the advanced staged interface. Persist an advanced plan with `--state-dir` before apply.

Multi-target `compare` is an aggregate trial interface. Its preview binds every normalized candidate plan to one `plan_compare_...` identifier and process-execution approval. The approved command reports each candidate as passed, failed, or capability-blocked from an independent disposable copy. Top-level success means evidence collection completed and the source repository remained unchanged; it does not mean every candidate passed or select a winner.

`runtime to deno` is a separate Bun application-runtime interface. Its first call is read-only and binds deterministic recipes, file digests, and sorted explicit Deno permissions to `runtime_plan_...`. The approved call writes only reviewed source, script, and type mutations; it does not select a package manager, install dependencies, or execute project code. Runtime results redact mutation content, use `runtime_run_...`, verify after-digests and Bun runtime residue, and return a separately approved `runtime rollback` action.

`skill install` and `skill uninstall` are separately scoped filesystem workflows. Their first call inspects the bundled portable source and exact client destination, emits a `skill-status` artifact, and returns one `filesystem-write` next action bound to a `skill_plan_...` identifier. That identifier covers the operation, scope, client, mode, source and installed digests, ownership state, and exact paths. Status and doctor are read-only. Managed-copy uninstall refuses local modifications, exact-source links are removed without following their target, and `--dry-run` never mutates even with approval.

`explain` is always read-only. Diagnostic codes resolve from the Rust-owned stable catalog. Package-manager and runtime plan, run, and verification identifiers resolve from the default `.pkgshift/state` directory or an explicit `--state-dir`. Canonical digest grammar, integrity envelopes, verification identities, regular-file boundaries, and bounded run scans are validated before content is returned. `ARTIFACT_NOT_FOUND` means the selected state root has no matching identity; `ARTIFACT_INVALID` means present state cannot be trusted.

## Result Envelope

Expect these top-level fields:

| Field | Purpose |
| --- | --- |
| `schemaVersion` | Version of the machine-readable contract. |
| `command` | Canonical command that produced the result. |
| `status` | Domain outcome such as `planned`, `completed`, `blocked`, `failed`, or `rolled-back`. |
| `planId` | Immutable plan identifier when applicable. |
| `runId` | Apply run identifier when applicable. |
| `summary` | Concise, structured operation summary. |
| `artifacts` | Addressable reports, journals, diffs, or plans. |
| `diagnostics` | Structured observations with stable codes. |
| `nextActions` | Machine-actionable follow-up commands. |

Each next action contains an `argv` array. It also declares `requiresApproval` and `sideEffect`. Execute the array directly through the available process tool; do not convert it to a shell string when an array-capable interface is available.

For guided migration, exit code `7` with `status: planned` means the immutable preview is ready for user approval. It must not have changed the repository. The plan declares `targetExecutable`, `verificationPolicy`, and package-local dependency-state cleanup as a non-reversible generated-state side effect: rollback restores repository files but not the removed source `node_modules`. Explicit representative-script operations also declare process execution and may create outputs outside the rollback snapshot. Before execution, activate the exact package-manager pin. Apply resolves it through a bounded version probe before snapshots and persists the resolved identity. After exact approval, execute the returned array; a successful apply returns plan and run identifiers together with executable, cleanup, and verification evidence. A successful trial returns a `trial-report`, `repositoryUnchanged: true`, and a null `runId`.

For migration-readiness doctor, expect inspection, Project IR, capability analysis, an optional source lock graph, and one `migration-readiness` artifact. Its verdict is `ready`, `review-required`, `blocked`, or `already-selected`. Exit code `0` means migration is available or the target is already selected. Exit code `3` means a hard blocker exists or explicit lossy acceptance is still required. Doctor returns null plan and run identifiers in every case.

Aggregate doctor emits one `migration-readiness-matrix` instead of target-specific capability-analysis and readiness artifacts. A detected source produces exit code `0` even when some candidates are blocked; inspect each nested report. Missing or ambiguous source evidence produces exit code `3`. Matrix and report identifiers are deterministic evidence identities, never approval identities.

## Exit Codes

| Code | Meaning |
| --- | --- |
| `0` | Success for the requested operation. |
| `2` | Invalid command input. |
| `3` | Unsupported target or capability. |
| `4` | Repository, artifact, or exact target executable precondition conflict. |
| `5` | Apply or isolated trial execution failure after approval. |
| `6` | Blocking verification failure, including inside a trial. |
| `7` | Approval or user input required. |
| `8` | Internal error or untrustworthy result. |

Use diagnostics for specific handling. Do not infer a detailed cause from an exit code alone.

## Compatibility

- Accept additional fields within a supported major schema version.
- Preserve unknown fields when relaying a result.
- Stop before action when the schema major version is unsupported.
- Never scrape identifiers or commands from prose when structured fields exist.
