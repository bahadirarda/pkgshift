---
type: Workflow
title: Bun to Deno Runtime Migration
description: Defines preview, explicit permission review, exact approval, verification, and rollback for dedicated Bun-to-Deno runtime recipes.
tags: [workflow, runtime, bun, deno, approval]
status: draft
generated: { by: bahadirarda, at: 2026-08-17T11:44:43Z }
sources:
  - id: runtime-recipes
    resource: /architecture/runtime-migration-recipes.md
    title: Bun to Deno Runtime Recipes
  - id: agent-interface
    resource: /architecture/agent-interface.md
    title: Agent Interface
---

# Preview

Run from the project root:

```text
pkgshift runtime to deno --deno-permission net --json --no-color --non-interactive
```

The first invocation changes no repository file and returns exit code `7` when the plan is executable but unapproved. Review the recipe identifiers, affected paths, before and after digests, requested Deno permissions, diagnostics, verification scope, and exact `nextActions[0].argv`. Plan artifacts intentionally omit source content.

Use `--dry-run` when no approval action should be returned. Repeat `--deno-permission` only for access the migrated program requires. Missing permissions inferred from a safe recipe are blocking; pkgshift never grants all access automatically.

# Apply

After a person approves the exact runtime plan, execute the returned argv unchanged. pkgshift re-plans, requires the same `runtime_plan_` identifier, persists private recovery state, snapshots every mutation target, applies the deterministic recipes, and verifies planned digests plus Bun runtime residue.

Successful apply returns `status: completed`, a `runtime_run_` identifier, a redacted runtime run journal, a passing runtime verification report, and an approval-bound rollback action. It does not install packages or execute application code.

# Blocked Plans

Do not work around a blocking recipe diagnostic with model-authored source edits inside the pkgshift operation. Review and migrate unsupported Bun routes, WebSockets, shell behavior, macros, test APIs, configuration, or other globals explicitly; then create a new read-only plan. Package-manager migration and runtime migration remain separate approval boundaries.

# Roll Back

Use the exact returned rollback action, or:

```text
pkgshift runtime rollback <runtime-run-id> --approve <runtime-run-id> --json --no-color --non-interactive
```

Rollback verifies snapshot integrity and the restored runtime fingerprint. The runtime transaction executes no dependency installation or project process, so its recovery boundary is limited to the planned repository files.
