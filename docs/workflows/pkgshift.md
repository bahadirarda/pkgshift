---
type: Workflow
title: Package Manager Migration
description: Defines the safe operational workflow for planning, approving, applying, and verifying a package manager migration.
tags: [workflow, package-management, verification, rollback]
status: draft
generated: { by: bahadirarda, at: 2026-08-17T11:00:00Z}
sources:
  - id: transactional-decision
    resource: /decisions/transactional-migrations.md
    title: Transactional Migrations
  - id: agent-interface
    resource: /architecture/agent-interface.md
    title: Agent Interface
---

# Default Human Workflow

From the repository root, run:

```text
pkgshift to bun
```

The command detects the source, builds a plan, displays its identifier, file and operation counts, warnings, and lossy decisions, then asks `Apply this migration? [y/N]`. Declining or pressing Enter leaves the repository unchanged. Approval causes the command to persist recovery state, apply the exact plan, and verify the resulting run.

Use a read-only preview when no execution is intended:

```text
pkgshift to bun --dry-run
```

Use an isolated execution trial when importer, installer, and verification behavior should be proven before repository mutation:

```text
pkgshift to bun --trial
```

Trial has its own exact approval boundary. It executes in a disposable copy, returns a `trial-report`, and does not authorize or perform the later repository apply. See [Isolated Migration Trial](/workflows/isolated-trial.md).

# Default Agent Workflow

Run:

```text
pkgshift to bun --json --no-color --non-interactive
```

The expected approval boundary is exit code `7` with a complete plan and one `nextActions` entry. Present the source, target, plan identifier, file and operation counts, warnings, capability losses, side effects, and verification scope. If the user approves that exact plan, execute `nextActions[0].argv` as an argument array. Do not add a repository path or state path, and do not reconstruct the command from prose.

The approved invocation automatically persists the plan under `.pkgshift/state`, applies it, and verifies the run. Treat the migration as complete only when the returned status is `completed` and verification has no blocking failure.

When the user requests a trial first, add `--trial` to the initial preview and execute only its returned action after approval. A passing trial reports `repositoryUnchanged: true` and no `runId`. Return to a normal preview and approval before apply.

When the user names representative root scripts, add one `--verify-script <name>` per script to the initial preview. Do not infer defaults such as `test`, `lint`, or `build`. The returned next action preserves the selection, target argv, and timeout in the immutable plan.

# Preconditions

- Run from the intended repository root.
- Preserve unrelated user changes and report their presence.
- Know the target package manager, or inspect first and present supported candidates.
- Use noninteractive structured output when a coding agent is the caller.

# Advanced Staged Procedure

Use the staged commands when an integration needs independent artifacts at each phase or when diagnosing and recovering a run.

## 1. Inspect

```text
pkgshift inspect package-manager --json --no-color
```

Review detected source candidates, confidence, workspace shape, integrations, secrets-redaction status, and blocking diagnostics. Inspection must not modify files or install dependencies.

## 2. Plan and Persist

```text
pkgshift plan package-manager --to bun --state-dir .pkgshift/state --json --no-color
```

Add repeatable `--verify-script <name>` options during this planning step when representative root scripts should run after installation.

Review:

- The source and target adapter versions.
- Repository fingerprint and plan identifier.
- File and command operations in execution order.
- Capability transformations and losses.
- Dependency graph expectations.
- Declared side effects, warnings, and rollback limits.
- Verification checks.

Resolve blocking diagnostics by changing options or repository state, then create a new plan. Review every `CAPABILITY_LOSSY` decision before using `--accept-lossy`. Do not edit a stored plan manually.

## 3. Approve

Present the plan summary to the user. Approval must identify the exact plan. General authorization to inspect, persist, or discuss a repository is not authorization to apply a migration.

## 4. Apply

```text
pkgshift apply <plan-id> --state-dir .pkgshift/state --approve <plan-id> --json --no-color --non-interactive
```

The engine rechecks preconditions and creates owner-only recovery snapshots before mutation. It removes accepted package-local `node_modules` paths, journals removed and already-absent dependency state, runs the target installer without a shell or lifecycle scripts, retires source-only repository artifacts, and persists a redacted process report. Explicitly planned representative scripts then run without a shell under a fixed timeout. Cleanup and script-owned output outside planned mutation paths are intentionally not rollback-snapshotted. If the repository fingerprint conflicts, stop and re-plan. Preserve the returned run identifier even when apply fails.

## 5. Verify

```text
pkgshift verify <run-id> --state-dir .pkgshift/state --json --no-color
```

Assess planned digests, the clean-install journal, source-artifact residue, target selection, lockfile behavior, installation completion, workspace behavior, integrations, explicitly selected representative-script records, and the source-to-target lock graph comparison. Graph comparison is skipped only when no source lockfile existed. Verification never selects or reruns representative scripts.

## 6. Complete or Roll Back

If verification passes, summarize changed artifacts and any non-blocking follow-up. If apply or verification fails, explain the relevant diagnostics before requesting rollback approval.

```text
pkgshift rollback <run-id> --state-dir .pkgshift/state --approve <run-id> --json --no-color --non-interactive
```

Rollback verifies the restored repository fingerprint. Do not describe external dependency state as restored: `node_modules`, downloads, global stores, and caches remain outside snapshot scope.

# Agent Behavior

- Prefer `nextActions[].argv` over constructing follow-up commands from text.
- Never execute a next action whose `requiresApproval` value is true without approval.
- Treat unknown fields as forward-compatible data and unknown schema major versions as incompatible.
- Show concise plan risk and side-effect information before asking for approval.
- Use `pkgshift explain` for unfamiliar diagnostic codes instead of guessing.
- Stop when the CLI cannot produce a trustworthy artifact; do not emulate apply with ad hoc edits.
- Prefer the guided command for ordinary migrations; use staged commands only when their additional control is needed.

# Completion Evidence

A completed migration has:

- A plan artifact.
- A successful apply run journal.
- A verification report tied to that run.
- A passing lock graph comparison when a source lock graph exists.
- No unresolved blocking diagnostic.
- A concise record of expected semantic drift, if any.
- An explicit skipped-check record for capabilities outside the MVP boundary.
