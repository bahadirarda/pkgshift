---
type: Interface Contract
title: Agent Interface
description: Defines a simple keyword-based CLI and deterministic output contract for coding agents and humans.
tags: [architecture, cli, json, agents]
status: draft
generated: { by: bahadirarda, at: 2026-08-16T16:00:00Z}
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
---

# Primary Command

The default interface is a guided, current-directory migration:

```text
pkgshift to <target>
```

It performs deterministic inspection, Project IR construction, capability analysis, and planning before presenting an approval prompt. Planning remains read-only. On approval, the command persists the immutable plan to `.pkgshift/state`, applies it, and verifies the resulting run. `--dry-run` stops after planning.

Humans do not need to provide repository or state paths when running from the intended repository root.

# Implementation Availability

The Rust CLI is the primary interface and implements `to`, `inspect package-manager`, `plan package-manager`, `pm to`, `support`, `apply`, `verify`, and `rollback`. The TypeScript reference preserves the same migration contract and additionally retains `explain` plus managed `skill` lifecycle commands while their long-term ownership is decided. This temporary command difference does not change approval, result-envelope, or side-effect semantics.

# Agent Flow

Agents disable prompts and request the same plan as structured data:

```text
pkgshift to bun --json --no-color --non-interactive
```

When the plan is executable but unapproved, the command returns exit code `7`, `status: planned`, and one exact approval-bound `nextActions[].argv`. The agent presents the plan and waits for the user. After approval, it executes that argument array without reconstructing it. The approved call persists, applies, and verifies the exact plan in one invocation.

# Advanced Command Surface

The staged commands remain available for diagnostics, integration, and recovery:

```text
pkgshift inspect [package-manager]
pkgshift plan package-manager --to <target>
pkgshift apply <plan-id> --state-dir <path> --approve <plan-id>
pkgshift verify <run-id> --state-dir <path>
pkgshift explain <diagnostic-code-or-artifact-id>
pkgshift rollback <run-id> --state-dir <path> --approve <run-id>
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
- Advanced planning persists an artifact only when `--state-dir <path>` is explicitly supplied.

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
| `5` | Apply failed after a run journal was created. |
| `6` | Verification completed with blocking failures. |
| `7` | Explicit approval or additional user input is required. |
| `8` | An internal error prevented a trustworthy result. |

Diagnostics provide the stable, specific reason. Exit codes remain intentionally coarse.

# Artifact Persistence

Planning emits Project IR, capability analysis, exact file mutations, and plan artifacts through the result envelope. The first guided call does not persist them. Once the exact plan is approved, the command re-plans against current repository evidence, requires the same plan identifier, and stores one integrity-checked bundle in `.pkgshift/state` before apply. Advanced planning stores a bundle only when `--state-dir` is provided. Persistence does not imply approval. A plan is executable only when its target is production, all observed capabilities have implemented safe transformations, every blocking diagnostic is absent, and any lossy decisions were accepted while planning.

Apply persists the run journal, recovery snapshot, and redacted process report. Verify persists a report tied to the run and plan. Explain can load diagnostic codes, plan bundles, run journals, process reports, and verification reports without mutation.

# Approval Contract

An agent may run a guided preview, inspect, plan, explain, status, and doctor operations without migration approval. It must present the plan summary, warnings, and side effects before executing an approval-bound next action. Guided execution and advanced apply require `--approve <plan-id>`; rollback requires `--approve <run-id>`. Skill install and uninstall require the exact `skill:pkgshift:<scope>:<client>` approval token.

Apply runs the declared target installation without lifecycle scripts. Verify is filesystem- and artifact-read-only in the MVP and therefore does not need a second approval.
