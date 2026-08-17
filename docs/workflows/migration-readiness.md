---
type: Workflow
title: Migration Readiness
description: Defines the deterministic, read-only target assessment performed before a package manager plan is created.
tags: [workflow, readiness, doctor, agents, package-management]
status: draft
generated: { by: bahadirarda, at: 2026-08-17T13:52:52Z}
sources:
  - id: agent-interface
    resource: /architecture/agent-interface.md
    title: Agent Interface
  - id: package-manager-workflow
    resource: /workflows/pkgshift.md
    title: Package Manager Migration
---

# Purpose

Assess every production target from one repository scan when the target is undecided:

```text
pkgshift doctor
```

The command returns one `migration-readiness-matrix` containing independent reports for npm, pnpm, Yarn Classic, Yarn Modern, Bun, vlt, and Deno dependency mode. It preserves blocked candidates and never calculates a winner.

Assess one package-manager target from the repository root before creating or persisting a plan:

```text
pkgshift doctor --to bun
```

The command runs the same inspection, Project IR, capability analysis, lock graph extraction, and deterministic planning logic used by the migration engine. It returns a bounded `migration-readiness` projection instead of a package-manager plan. It does not persist state, write repository files, run package-manager processes, or create an approval identity.

# Agent Invocation

Use structured output:

```text
pkgshift doctor --json --no-color --non-interactive
pkgshift doctor --to bun --json --no-color --non-interactive
```

Add `--verify-script <name>` only for exact root scripts selected by the user. The report lists their target commands as anticipated effects; doctor never executes them.

# Readiness Contract

Aggregate doctor reads inspection, Project IR, integration, and source lock graph evidence once. Its `migration-readiness-matrix` contains a deterministic `doctor_matrix_...` identity, summary counts, and one complete readiness report per catalog target. Top-level completion means evidence collection succeeded; it does not mean every target can migrate. Candidate order is stable catalog order, not a ranking.

The `migration-readiness` artifact includes:

- A deterministic `doctor_...` report identifier and repository fingerprint.
- Detected source, requested target, target support tier, package count, workspace patterns, and available root scripts.
- Capability counts and the complete stable diagnostic set.
- CI, container, documentation, and automation paths affected by the target.
- Projected file writes, deletions, dependency-state cleanup, source-artifact retirement, process commands, and selected verification scripts.

It intentionally omits mutation content, `planId`, `runId`, persisted state, and approval-bound actions. The report identifier addresses readiness evidence only and cannot authorize planning, trial, apply, or rollback.

# Verdicts

| Verdict | `migrationAvailable` | Meaning |
| --- | --- | --- |
| `ready` | `true` | The deterministic plan is executable without a reported warning. |
| `review-required` | `true` | The plan is executable, but warnings or accepted lossy behavior require review. |
| `review-required` | `false` | Only explicit lossy acceptance prevents an executable plan; `availableAfterReview` is `true`. |
| `blocked` | `false` | At least one unsupported, unknown, invalid, or otherwise blocking condition prevents migration. |
| `already-selected` | `false` | The requested target already owns the repository. |

Treat `migrationAvailable`, not the verdict alone, as the executable-planning signal. Exit code `0` means migration is available or the target is already selected. Exit code `3` means the target is blocked or requires lossy acceptance before planning. Diagnostics remain the specific source of truth.

# Next Action

When an executable plan is available, doctor returns a read-only, non-approval next action for:

```text
pkgshift plan package-manager --to bun --json --no-color --non-interactive
```

When only lossy acceptance is missing, the returned argument array includes `--accept-lossy`. Present every lossy decision and obtain user acceptance before executing that array. The next command creates a complete plan; it does not apply the migration.

# Recommended Flow

1. Run aggregate doctor when the target is undecided, or target-specific doctor when it is known.
2. Present every candidate independently and let the user select; never infer a winner.
3. Present the verdict, blockers, warnings, integration impact, cleanup, source-artifact retirement, and declared process effects.
4. Resolve blockers or obtain explicit acceptance for reviewed lossy behavior.
5. Execute the selected target's returned read-only planning argument array unchanged.
6. Present the immutable plan and wait for its separate exact approval.
7. Apply, verify, and roll back only through the ordinary [Package Manager Migration](/workflows/pkgshift.md) contract.

`pkgshift doctor` assesses migration readiness. `pkgshift skill doctor` is a different read-only command that inspects a managed Agent Skill installation.
