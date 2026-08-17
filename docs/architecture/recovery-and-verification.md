---
type: Reliability Architecture
title: Recovery and Verification
description: Defines snapshot integrity, verification checks, rollback scope, and failure behavior for migration runs.
tags: [architecture, recovery, verification, rollback, integrity]
status: draft
generated: { by: bahadirarda, at: 2026-08-17T15:14:32Z }
sources:
  - id: transactional-decision
    resource: /decisions/transactional-migrations.md
    title: Transactional Migrations
  - id: run-journal
    resource: /architecture/run-journal.md
    title: Run Journal
  - id: migration-workflow
    resource: /workflows/pkgshift.md
    title: Package Manager Migration
---

# Recovery Snapshot

Apply snapshots every planned mutation path and every possible target lockfile before the first repository write. Each entry records whether the path existed, its content digest, file mode, and private backup reference. Missing paths are significant because rollback must remove target files created during apply.

Snapshot manifests and backup files live under the run directory. Manifests are integrity-checked JSON; backup files are verified against their recorded digests before restoration. Symbolic links, symbolic-link parent traversal, non-regular files, and paths outside the selected project root fail closed.

# Apply Preconditions

Apply requires:

- A verified immutable plan bundle.
- An executable production-target plan.
- Exact `--approve <plan-id>` authorization.
- Exclusive ownership of the repository-scoped transaction lock.
- A current repository fingerprint equal to the plan baseline.
- A target executable that resolves from `PATH` and reports the exact planned version.
- Mutation paths whose current digests equal their planned before digests.
- Writable journal and snapshot state.

Every write uses atomic replacement and verifies its after digest. Delete operations verify absence. After configuration mutations, a dedicated dependency-state cleanup operation removes each package-local `node_modules` directory from the accepted Project IR. It rejects symbolic links, non-directory targets, paths outside the repository, and paths not ending in `node_modules`. The run journal records removed and already-absent paths. Target import and installation run only after this cleanup; source lockfiles remain available until those operations complete, then source-only repository artifacts are retired.

Apply, verify, and rollback share a non-blocking repository-scoped transaction lock. A second agent receives `REPOSITORY_TRANSACTION_BUSY` instead of racing snapshots, journal transitions, or repository writes. A lock whose recorded process no longer exists can be recovered safely.

# Verification Report

Verify reads the plan, journal, and repository. It records these MVP checks:

| Check | Blocking condition |
| --- | --- |
| Planned file digests | Any write or deletion differs from the plan. |
| Target executable version | The resolved program, version, or package-manager pin differs from the exact plan requirement. |
| Clean target install | A planned package-local dependency-state path has no matching removed or already-absent journal record. |
| Target selection | Detection does not select the planned target. |
| Target lockfile | No registered target lockfile exists. |
| Source artifact residue | A source-only lockfile or configuration file remains after migration. |
| Workspace membership | Package paths differ from the source Project IR. |
| Target install | The journaled install operation is not successful. |
| Representative scripts | Any explicitly selected root script was not run, timed out, or exited unsuccessfully. The check is skipped when no script was selected. |
| Dependency graph drift | Added or removed resolutions, comparable integrity mismatches, strict normalized edge drift, a missing non-empty target graph, or incomplete parsing. |

A failed check moves the verification operation and run to `failed`. A successful report moves both to `succeeded`. Reports carry their own identity and integrity digest.

The source graph is extracted before planning and persisted with the accepted plan. The target graph is extracted after installation. `reachable-resolution-set-v3` prunes topology-proven unreachable entries and evaluates optional-only package-name absence against the plan's normalized target-platform matrix. Reachable version and comparable integrity drift remain blocking. Formats without sufficient topology report and apply `resolution-set-v1`. Edge-shape differences remain evidence in compatible mode and become blocking in strict mode. A dependency-free target may omit its lockfile only when the accepted source graph proves an empty resolved set. See [Lock Graph Proof](/architecture/lock-graph-proof.md) and [Platform, Edge, and Executable Verification](/architecture/verification-policies.md).

# Representative Script Boundary

pkgshift never selects project scripts automatically. Each repeatable `--verify-script <name>` value must exactly match a script in the root `package.json`; missing, malformed, or workspace-only names block the plan. The immutable operation stores its exact target argv as `npm|pnpm|yarn|bun|vlt run <name>` or `deno task <name>`, declares `process-execution`, and applies a 300-second ceiling. Execution does not pass through a shell.

Representative scripts run after target installation and before structural report finalization. Their operation identifier, argv, exit code, duration, timeout state, and withheld-output byte counts are stored in the run journal. A later `verify` command reads this evidence and does not execute the script again.

Unlike target installation, an explicitly selected script intentionally runs repository-defined code and is not given lifecycle-suppression environment overrides. It may create or modify paths outside the migration plan. Those script-owned effects are not included in rollback snapshots; use an isolated trial first and review the selected script before repository apply.

# Isolated Trial

`pkgshift to <target> --trial` executes the accepted plan and verifier in a disposable repository copy. It returns a `trial-report` containing process records, nested verification, and `repositoryUnchanged`. It creates no source run identifier or recovery state because the source repository is not the mutation target. Package manager network and cache effects remain external.

# Rollback

Rollback requires exact `--approve <run-id>` authorization. It may recover applying, verifying, succeeded, failed, or retryable rollback-failed runs. Recovery restores or removes every snapshot entry, marks completed or failed operations as rolled back, and re-inspects the repository.

The run reaches `rolled-back` only when the restored repository fingerprint equals the plan baseline. Snapshot corruption, unsafe path types, missing backup metadata, or fingerprint mismatch produces `rollback-failed`.

# External Effects

The repository transaction deliberately does not snapshot `node_modules`, package-manager caches, global stores, downloaded content, or outputs created by explicitly selected representative scripts. Clean target installation removes pre-migration package-local `node_modules`, and the target installer may recreate it with target-owned state. A successful rollback therefore emits `ROLLBACK_EXTERNAL_EFFECTS_REMAIN`; reinstall the source dependency state when exact local dependency parity is required. pkgshift never deletes global package-manager caches or stores during migration.
