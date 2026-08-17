---
type: Execution Architecture
title: Isolated Target Comparison
description: Defines deterministic multi-target planning, aggregate approval, independent trial execution, and evidence-only comparison semantics.
tags: [architecture, comparison, trial, approval, agents]
status: draft
generated: { by: bahadirarda, at: 2026-08-17T11:30:00Z }
sources:
  - id: isolated-trial
    resource: /workflows/isolated-trial.md
    title: Isolated Migration Trial
  - id: agent-interface
    resource: /architecture/agent-interface.md
    title: Agent Interface
  - id: transactional-decision
    resource: /decisions/transactional-migrations.md
    title: Transactional Migrations
---

# Comparison Contract

`pkgshift compare <target> <target>...` compares at least two distinct production adapter candidates from the current repository root. Target aliases are normalized, duplicates are removed, and candidates are ordered by stable adapter identity before planning. Unknown targets or fewer than two distinct targets fail before process execution.

The first call is read-only. pkgshift creates a complete immutable migration plan for every target, including capability decisions, mutations, installer commands, representative-script operations, diagnostics, and graph expectations. `plan_compare_<digest>` binds the ordered target and plan identifier pairs into one aggregate artifact. A candidate may remain capability-blocked while another is executable.

# Aggregate Approval

When at least one candidate is executable, the preview returns one exact `nextActions[0].argv` with `sideEffect: process-execution`. Approval of the aggregate comparison identifier authorizes only the candidate trials represented by that artifact. It does not authorize a later source repository migration.

`--dry-run` returns the same comparison plan without an approval action. `--accept-lossy` and every explicit `--verify-script` selection participate in each candidate plan and remain present in the returned argv.

# Independent Trials

Each executable candidate runs through the ordinary accepted-plan trial engine in its own operating-system temporary directory. Candidates never share generated manifests, lockfiles, `node_modules`, run journals, or verification state. Execution is deterministic and sequential; machine-level package manager caches and network access remain external effects.

Blocked candidates do not execute. Their complete plans and diagnostics remain in the comparison evidence. Explicit representative scripts run only for executable candidates and only inside their corresponding disposable copies.

# Result Semantics

The `target-comparison-report` contains one candidate entry with `passed`, `failed`, or `blocked` status:

- `passed` means the isolated trial and nested verification passed.
- `failed` means an executable candidate produced a failed trial or verification report.
- `blocked` means capability analysis did not produce an executable plan, so no process ran.

Candidate failure is valid comparison evidence. The top-level command returns `completed` when it produced a trustworthy report and the source repository fingerprint remained unchanged, even when individual candidates failed or were blocked. Source repository drift makes the comparison itself fail with `COMPARISON_REPOSITORY_CHANGED`.

pkgshift reports evidence and never ranks or recommends a winner. A human or agent may compare capability loss, operations, diagnostics, process evidence, and verification outcomes, then create a new ordinary preview for the selected target. Aggregate comparison approval never crosses that repository-write boundary.
