---
type: Interface Contract
title: Agent Interface
description: Defines a simple keyword-based CLI and deterministic output contract for coding agents and humans.
tags: [architecture, cli, json, agents]
status: draft
generated: { by: bahadirarda, at: 2026-08-17T11:00:00Z}
sources:
  - id: agent-first-decision
    resource: /decisions/agent-first-cli.md
    title: Agent-first CLI
  - id: workflow
    resource: /workflows/pkgshift.md
    title: Package Manager Migration Workflow
  - id: rust-primary-decision
    resource: /decisions/rust-primary-polyglot-monorepo.md
    title: Rust-Primary Polyglot Monorepo
  - id: runtime-recipes
    resource: /architecture/runtime-migration-recipes.md
    title: Bun to Deno Runtime Recipes
---

# Primary Command

The default interface is a guided, current-directory migration:

```text
pkgshift to <target>
```

It performs deterministic inspection, Project IR construction, capability analysis, and planning before presenting an approval prompt. Planning remains read-only. On approval, the command persists the immutable plan to `.pkgshift/state`, applies it, and verifies the resulting run. `--dry-run` stops after planning.

`pkgshift to <target> --trial` preserves the same plan and approval identifier but changes the authorized side effect to process execution in a disposable repository copy. A trial returns no source run identifier and does not authorize later repository mutation.

`pkgshift to <target> --verify-script <name>` binds one exact root `package.json` script to the plan. The option is repeatable and remains planning-only; staged apply reads the stored operations. pkgshift does not infer script names.

Humans do not need to provide repository or state paths when running from the intended repository root.

# Implementation Availability

The Rust CLI is the primary interface and implements `to`, isolated `to --trial`, isolated multi-target `compare`, `inspect package-manager`, `plan package-manager`, `pm to`, `support`, `apply`, `verify`, `rollback`, dedicated `runtime to deno`, `runtime rollback`, and managed `skill install|status|doctor|uninstall`. The TypeScript reference preserves native-import planning, the established package-manager migration contract, and diagnostic `explain` while that final ancillary read interface is ported. Isolated trial, multi-target comparison, blocking lock-graph comparison, Bun-to-Deno runtime recipes, and Agent Skill lifecycle ownership are Rust-primary trust features.

# Agent Flow

Agents disable prompts and request the same plan as structured data:

```text
pkgshift to bun --json --no-color --non-interactive
```

When the plan is executable but unapproved, the command returns exit code `7`, `status: planned`, and one exact approval-bound `nextActions[].argv`. The agent presents the plan and waits for the user. After approval, it executes that argument array without reconstructing it. The approved call persists, applies, and verifies the exact plan in one invocation.

For `--trial`, `nextActions[].sideEffect` is `process-execution` and the returned argument array retains `--trial`. A passing trial returns a `trial-report`, `repositoryUnchanged: true`, and `runId: null`. Agents must request a separate normal preview and repository-write approval before apply. When the preview contains repeatable `--verify-script` values, the returned argument array also preserves them unchanged.

# Advanced Command Surface

The staged commands remain available for diagnostics, integration, and recovery:

```text
pkgshift inspect [package-manager]
pkgshift compare <target> <target>... [--verify-script <name>]...
pkgshift plan package-manager --to <target>
pkgshift apply <plan-id> --state-dir <path> --approve <plan-id>
pkgshift verify <run-id> --state-dir <path>
pkgshift explain <diagnostic-code-or-artifact-id>
pkgshift rollback <run-id> --state-dir <path> --approve <run-id>
pkgshift runtime to deno [--deno-permission <name>]...
pkgshift runtime rollback <runtime-run-id> --state-dir <path> --approve <runtime-run-id>
pkgshift skill install --scope <project|user> --client <codex|claude> --mode <copy|link>
pkgshift skill status --scope <project|user> --client <codex|claude>
pkgshift skill doctor --scope <project|user> --client <codex|claude>
pkgshift skill uninstall --scope <project|user> --client <codex|claude>
```

The read-only planning shortcut also remains available:

```text
pkgshift pm to bun
```

This shortcut is equivalent to `pkgshift plan package-manager --to bun`. Neither it nor the guided command crosses the approval boundary implicitly; the guided command continues only after an interactive confirmation or an exact plan identifier supplied through `--approve`.

# Process Contract

- `--json` emits one versioned JSON result to standard output.
- Logs and progress go to standard error.
- `--no-color` disables terminal styling.
- `--quiet` suppresses nonessential standard-error output.
- Noninteractive mode never prompts and returns a diagnostic when approval or input is required.
- Signals and cancellation produce a journal status when an apply run has started.
- Authentication values and matching environment variables are redacted before rendering or persistence.
- The guided command uses `.pkgshift/state` only after approval; `--state-dir <path>` can override it.
- Trial uses private state inside its temporary copy and never creates the default state directory in the source repository.
- Advanced planning persists an artifact only when `--state-dir <path>` is explicitly supplied.
- `--verify-script <name>` is accepted only by guided or staged planning commands, validates exact root script membership, and adds a bounded shell-free `verification.run-script` operation.
- `--deno-permission <name>` is repeatable only on the dedicated runtime plan surface; normalized permissions participate in plan identity and render narrow Deno flags instead of `-A`.

# Result Envelope

Every JSON result follows this conceptual shape:

```json
{
  "schemaVersion": "1.0",
  "command": "to bun",
  "status": "planned",
  "planId": "plan_...",
  "runId": null,
  "summary": {
    "source": "npm",
    "target": "bun",
    "files": 4,
    "repositoryChanged": false
  },
  "artifacts": [],
  "diagnostics": [],
  "nextActions": [
    {
      "argv": ["pkgshift", "to", "bun", "--approve", "plan_...", "--json", "--no-color", "--non-interactive"],
      "requiresApproval": true,
      "sideEffect": "repository-write"
    }
  ]
}
```

Fields may be added compatibly within a schema version. Existing fields cannot change meaning without a schema version change. Agents must consume fields rather than scrape prose messages.

# Status and Exit Semantics

The result `status` describes the domain outcome; the process exit code describes whether the requested operation completed successfully.

| Exit code | Meaning |
| --- | --- |
| `0` | Requested operation completed successfully. |
| `2` | Command or option input is invalid. |
| `3` | Requested target or capability is unsupported. |
| `4` | Artifact preconditions conflict with the current repository. |
| `5` | Apply or isolated trial execution failed after approval. |
| `6` | Verification completed with blocking failures, including inside a trial. |
| `7` | Explicit approval or additional user input is required. |
| `8` | An internal error prevented a trustworthy result. |

Diagnostics provide the stable, specific reason. Exit codes remain intentionally coarse.

# Artifact Persistence

Planning emits Project IR, capability analysis, exact file mutations, and plan artifacts through the result envelope. The first guided call does not persist them. Once the exact plan is approved, the command re-plans against current repository evidence, requires the same plan identifier, and stores one integrity-checked bundle in `.pkgshift/state` before apply. Advanced planning stores a bundle only when `--state-dir` is provided. Persistence does not imply approval. A plan is executable only when its target is production, all observed capabilities have implemented safe transformations, every blocking diagnostic is absent, and any lossy decisions were accepted while planning.

Apply persists the run journal, package-local dependency-state cleanup records, recovery snapshot, and redacted process report. Explicitly selected representative scripts record their operation identifier, exact argv, exit code, duration, timeout status, and withheld-output metadata. Verify persists a report tied to the run and plan, including clean-install, source-artifact residue, and representative-script checks; it never reruns repository code. Explain can load diagnostic codes, plan bundles, run journals, process reports, and verification reports without mutation.

Planning with a source lockfile also emits a redacted `source-lock-graph` artifact. Verification emits `lockGraphComparison` inside its report. Trial emits a `trial-report` containing withheld process records and nested verification.

Multi-target comparison emits one `target-comparison-plan` before approval and one `target-comparison-report` afterward. Its aggregate plan identifier binds every normalized candidate plan. Each executable candidate owns an independent nested trial report; blocked candidates retain their plan diagnostics without process execution. Candidate failures are comparison data, so top-level completion means the report is trustworthy and the source stayed unchanged, not that every target passed.

Runtime planning emits a content-redacted `runtime-migration-plan`; source mutation content is persisted only in an owner-readable envelope after exact approval. Apply emits a redacted `runtime-run-journal` and a `runtime-verification-report` that proves after-digests and Bun runtime residue. Runtime identifiers use `runtime_plan_` and `runtime_run_` prefixes so agents cannot confuse package-manager and runtime approval domains.

# Approval Contract

An agent may run a guided preview, inspect, plan, explain, status, and doctor operations without migration approval. It must present the plan summary, warnings, and side effects before executing an approval-bound next action. Guided execution and advanced apply require `--approve <plan-id>`; rollback requires `--approve <run-id>`. Skill install and uninstall first emit a read-only status artifact and require the exact returned `skill_plan_...` identifier. That identity binds the operation, scope, client, mode, source and installed digests, ownership state, and exact paths. `--dry-run` suppresses skill mutation even when that identifier is present.

Apply and trial remove accepted package-local source dependency state before running declared native import and target installation commands without lifecycle scripts. Explicit representative scripts run repository-defined code after installation and may create output outside the rollback snapshot; agents should prefer a trial before normal apply. Verify is filesystem- and artifact-read-only and therefore does not need a second approval. Rollback does not recreate the removed source `node_modules` state or remove unplanned script output.
