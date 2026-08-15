---
type: Architecture Decision
title: Transactional Migrations
description: Requires immutable plans, explicit approval, journaled apply runs, verification, and rollback data.
tags: [decision, transactions, safety, rollback]
status: draft
decision_status: accepted
generated: { by: bahadirarda, at: 2026-08-15T19:53:59Z}
sources:
  - id: founding-discussion
    resource: "founding product discussion on 2026-08-15"
    title: Founding product discussion
    author: human:project-founder
---

# Context

Package manager migrations touch several coupled files and may run commands that rewrite dependency state. A partially completed migration can be harder to diagnose than the original repository. Coding agents also need a clear boundary between analysis and authorized mutation.

# Decision

Model each migration as a transaction with five explicit stages:[^founding-discussion]

```text
inspect -> plan -> approve -> apply -> verify
```

- Inspection and planning are read-only.
- A plan is immutable and bound to repository evidence through preconditions.
- Apply requires an explicit plan identifier and creates a run journal before mutation.
- Verification evaluates declared postconditions and dependency graph policy.
- Rollback uses recorded recovery material and reports its own verification status.

The guided `pkgshift to <target>` command may orchestrate these stages in one process, but it must expose the immutable plan before approval and cannot persist or mutate until approval is explicit.

# Consequences

- The system stores artifacts even when an apply attempt fails.
- Operations require forward and compensating behavior where recovery is possible.
- Stale plans fail closed when relevant repository evidence changes.
- Dependency installation and similar effects must be disclosed in the plan.
- Rollback is best-effort for external or irreversible effects and must never claim a clean recovery without verification.

# Rejected Alternatives

## Unplanned direct conversion

A conversion that derives and writes changes without exposing an immutable plan hides the approval boundary and makes preview, audit, and recovery difficult. A guided command is acceptable only when it preserves every transaction stage and exact approval internally.

## Agent-authored edits without a plan

Free-form edits are flexible but do not provide deterministic replay, stable diagnostics, or a trustworthy rollback boundary.

# Related Concepts

- [Migration Engine](/architecture/migration-engine.md)
- [Package Manager Migration Workflow](/workflows/pkgshift.md)

[^founding-discussion]: Founding product discussion
