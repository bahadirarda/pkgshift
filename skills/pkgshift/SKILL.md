---
name: pkgshift
description: Inspect, plan, apply, verify, explain, or roll back deterministic JavaScript repository migrations through the pkgshift CLI. Use when Codex needs to move a repository between npm, pnpm, Yarn Classic, Yarn Modern, Bun, vlt, or Deno dependency mode; migrate supported Bun application runtime APIs to Deno; assess migration feasibility; produce a reviewable plan; or operate a migration safely with explicit approval.
---

# JavaScript Repository Migration

Use pkgshift as the deterministic execution boundary. Prefer the guided `pkgshift to <target>` workflow and never replace a missing CLI with ad hoc migration edits. The CLI, not the model, performs repository detection, semantic analysis, transformation planning, mutation, and verification.

## Resolve the Operation

Map the request to the narrowest operation:

- Assess a repository or identify its package manager: inspect.
- Preview a specific migration without execution: guided dry-run.
- Prove importer, installer, and verification behavior without source writes: guided trial.
- Compare two or more candidate targets without source writes: aggregate comparison preview, exact approval, then independent target trials.
- Prove user-named root scripts after migration: add one explicit `--verify-script <name>` per requested script, preferably to a trial first.
- Perform a migration: guided preview, exact approval, then guided execution.
- Produce independently stored stage artifacts: use the advanced plan, apply, and verify commands.
- Investigate a code or failed artifact: explain.
- Recover a failed run: explain, request approval, rollback, then verify the rollback.
- Migrate supported Bun application APIs to Deno: use the dedicated runtime preview, explicit permissions, exact approval, runtime verification, and runtime rollback flow below.

If the target is missing, inspect first. Suggest only targets compatible with the detected repository evidence and capability report.

## Establish the CLI Boundary

Resolve the repository-provided or installed `pkgshift` executable without installing arbitrary packages. If it is unavailable, report `PKGSHIFT_CLI_NOT_FOUND` and stop before mutation. Do not simulate apply by editing manifests or deleting lockfiles.

Run agent-facing commands with:

```text
--json --no-color --non-interactive
```

Treat standard output as the result document and standard error as logs. Never merge the streams before parsing JSON.

## Inspect

Run:

```text
pkgshift inspect package-manager --json --no-color --non-interactive
```

Confirm the repository root, source candidates, confidence, workspace topology, relevant integrations, and diagnostics. Preserve unrelated user changes. Never install dependencies during inspection.

When source detection is ambiguous, present the evidence and request a source selection. Do not choose based on one lockfile when other evidence conflicts.

## Preview a Migration

When the target is known, run from the repository root:

```text
pkgshift to <target> --json --no-color --non-interactive
```

Add repeatable `--verify-script <name>` arguments only when the user explicitly selects root `package.json` scripts. Never infer defaults such as `test`, `lint`, or `build`, and never select a workspace-only script by name.

This first invocation is read-only. An executable, unapproved plan returns exit code `7`, `status: planned`, a `planId`, artifacts, diagnostics, and an exact approval-bound next action. Exit code `7` is the expected approval boundary, not a migration failure.

For a human-requested preview that should never proceed to approval, `--dry-run` is also available:

```text
pkgshift to <target> --dry-run --json --no-color --non-interactive
```

Read the result by fields, not by prose. Check:

- `schemaVersion` is supported.
- `status` permits the proposed next action.
- `planId` is present.
- Capability losses and blocking diagnostics are understood.
- Operations, commands, side effects, and rollback limits are visible.
- Package-local dependency-state cleanup is visible as a non-reversible generated-state operation.
- Verification checks match the repository shape.
- Source lock graph and native importer artifacts are understood when present.
- `summary.repositoryChanged` is false before approval.
- The plan is executable before requesting approval.

If lossy decisions exist, explain each one before creating a replacement preview with `--accept-lossy`. Never add that option without explicit user acceptance.

Load [references/capability-model.md](references/capability-model.md) when capability classifications or target selection require interpretation. Load [references/diagnostics.md](references/diagnostics.md) when presenting or handling a diagnostic.

## Trial an Accepted Plan

When the user requests execution proof before repository mutation, preview with:

```text
pkgshift to <target> --trial --json --no-color --non-interactive
```

The first call remains read-only and returns exit code `7`. Present that its next action has `sideEffect: process-execution`: it runs native import, target installation, explicitly selected representative scripts, and verification in a disposable repository copy, but may use the network and package manager caches. After exact approval, execute the returned argument array unchanged.

When representative scripts are selected, explain that each script runs repository-defined code without a shell under the plan's timeout. Require its exact target argv to appear as a `verification.run-script` operation. Script-created files are not part of pkgshift's migration rollback snapshot.

Require `status: completed`, a passing `trial-report`, `repositoryUnchanged: true`, and passing nested verification. A trial returns no source `runId`. Trial approval never authorizes apply; obtain a new normal guided preview and separate repository-write approval before migration.

## Compare Candidate Targets

When the user has not selected a target but names two or more candidates, run:

```text
pkgshift compare <target> <target>... --json --no-color --non-interactive
```

The first call is read-only and returns one `target-comparison-plan`, aggregate plan identifier, and approval-bound next action. Present executable and blocked candidate counts plus each complete plan. After exact approval, execute the returned argv unchanged. Every executable candidate runs in its own disposable copy; blocked candidates run no process.

Require `repositoryUnchanged: true` and read each candidate as `passed`, `failed`, or `blocked`. Top-level `completed` means the comparison report is trustworthy, not that every candidate passed. Do not calculate or assert a winner. Present evidence and obtain a separate ordinary migration preview after the user selects a target. Add `--verify-script` only for exact root scripts the user explicitly wants proven across candidates.

## Migrate the Bun Runtime to Deno

Keep application runtime conversion separate from package-manager selection. Preview the dedicated runtime plan with only permissions the user has reviewed:

```text
pkgshift runtime to deno --deno-permission net --json --no-color --non-interactive
```

Repeat `--deno-permission` for `read`, `write`, `net`, `env`, `run`, `sys`, `ffi`, or `hrtime` only when required. Never add `-A`, infer broad access, or treat a package-manager migration to Deno dependency mode as runtime approval. A safe `Bun.serve` recipe requires `net`, and Bun text and JSON reads require `read`. Missing permissions and unsupported Bun APIs produce blocking diagnostics.

The first call is read-only. Review recipe identifiers, affected paths, before and after digests, permissions, diagnostics, and `nextActions[0].argv`. Runtime plan artifacts omit source contents intentionally. After exact approval, execute the returned argv unchanged. Require `status: completed`, a `runtime_run_` identifier, and a passing `runtime-verification-report` with both `planned-after-digests` and `bun-runtime-residue` passed.

This command does not change `packageManager`, remove `bun.lock`, install dependencies, or execute project code. `Bun.serve` routes, WebSockets, two-argument handlers, Bun shell behavior, SQLite, macros, advanced APIs, symbolic-link source boundaries, oversized runtime inputs, and `bunfig.toml` remain blocking. Do not fill a missing recipe with model-authored edits inside this workflow.

Use the approval-bound rollback action returned by apply, or:

```text
pkgshift runtime rollback <runtime-run-id> --state-dir .pkgshift/state --approve <runtime-run-id> --json --no-color --non-interactive
```

## Request Approval

Summarize the source, target, plan identifier, file and operation counts, command side effects, warnings, capability losses, rollback boundary, and verification scope. Request explicit approval for that plan before execution.

Treat any `nextActions` entry with `requiresApproval: true` as a hard boundary. Never infer execution approval from permission to inspect, preview, research, or discuss a migration.

## Execute the Approved Migration

After approval, execute the exact `nextActions[0].argv` argument array returned by the guided preview. It has this shape:

```text
pkgshift to <target> --approve <plan-id> --json --no-color --non-interactive
```

Do not reconstruct the command, invent paths, or add flags that were not approved. The CLI re-plans against current repository evidence, requires the identifier to remain exact, persists state under its default location, applies the migration, and verifies the run in one invocation. If preconditions conflict, stop and create a new preview. Preserve the `runId` on success, partial failure, or cancellation.

Treat the migration as successful only when the approved invocation returns `status: completed` with no blocking verification diagnostic. Require passed `clean-target-install` and `source-artifact-residue` checks for newly created plans. Report passed, failed, and skipped checks separately. When a source lock graph exists, require a passing `lockGraphComparison`; graph comparison is skipped only when no source lockfile existed. Require `representative-scripts: passed` when scripts were explicitly selected; otherwise require the check to be explicitly skipped. A later `verify` reads the journal and never reruns those scripts. Do not claim that skipped checks passed.

## Use Advanced Stages Only When Needed

Use [references/cli-contract.md](references/cli-contract.md) for the explicit `plan`, `apply`, and `verify` commands when an integration requires separate persisted stage artifacts or when diagnosing an interrupted run. The ordinary migration path must not expose or request repository and state-directory paths when the command is already running from the intended repository root.

## Manage the Portable Skill Safely

Use `pkgshift skill status|doctor --scope <project|user> --client <codex|claude>` for read-only installation evidence. For install or uninstall, run the unapproved command first, present its `skill-status` artifact, and execute only the returned argument array after approval. Do not copy, replace, or delete a skill directory manually when the managed lifecycle is available. A modified managed copy is user-owned state and must remain protected.

## Explain and Roll Back

Explain unknown diagnostics or failed artifacts before proposing recovery:

```text
pkgshift explain <diagnostic-code-or-artifact-id> --state-dir .pkgshift/state --json --no-color --non-interactive
```

The command is read-only and returns no next action. Omit `--state-dir` for a diagnostic code or when the artifact lives under the repository default. Treat `ARTIFACT_INVALID` as a trust failure; do not summarize the stored content or infer recovery steps from it. Treat `ARTIFACT_NOT_FOUND` as a state-root or identity mismatch and ask for the original state location when it cannot be discovered safely.

Request explicit rollback approval tied to the run. Then execute the returned rollback action or:

```text
pkgshift rollback <run-id> --state-dir .pkgshift/state --approve <run-id> --json --no-color --non-interactive
```

Read the rollback fingerprint verification result. Describe `node_modules`, package-manager stores, downloads, caches, and representative-script outputs outside planned mutation paths as external effects that remain.

## Consume Results Safely

Follow [references/cli-contract.md](references/cli-contract.md) for the result envelope and exit semantics.

- Prefer `nextActions[].argv` over reconstructing commands from messages.
- Reject unsupported schema major versions.
- Preserve unknown fields for forward compatibility.
- Redact tokens, credentials, registry authentication, and matching environment values.
- Do not paste full configuration files when a concise redacted summary is sufficient.
- Stop when a result reports an internal trust failure.
