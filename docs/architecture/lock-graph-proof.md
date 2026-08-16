---
type: Reliability Architecture
title: Lock Graph Proof
description: Defines normalized source and target lock graphs, native importer selection, and the blocking resolution-set verification policy.
tags: [architecture, lockfile, dependency-graph, verification, integrity]
status: draft
stale_after: 2026-09-15
generated: { by: bahadirarda, at: 2026-08-16T19:53:18Z}
sources:
  - id: pnpm-import
    resource: https://pnpm.io/cli/import
    title: pnpm import documentation
  - id: bun-lockfile
    resource: https://bun.sh/docs/pm/lockfile
    title: Bun lockfile documentation
  - id: bun-pm-migrate
    resource: https://bun.sh/docs/pm/cli/pm#migrate
    title: Bun package manager migration documentation
  - id: yarn-classic-import
    resource: https://classic.yarnpkg.com/lang/en/docs/cli/import/
    title: Yarn Classic import documentation
  - id: npm-install
    resource: https://docs.npmjs.com/cli/install/
    title: npm install documentation
---

# Proof Boundary

The Rust engine extracts the accepted source lockfile into a normalized `LockGraph` before planning. The stored plan binds to the graph identifier and persists the graph beside Project IR and capability analysis. After target installation, verification extracts the target lockfile independently and compares both graphs. A target install process exiting successfully is necessary but no longer sufficient.

The graph intentionally contains dependency evidence rather than raw lockfile content:

- Package name, resolved version, source locator, and integrity value when present.
- Logical dependency, optional dependency, and peer dependency edges where the format exposes them.
- Source manager, format, lockfile path, content digest, completeness, and diagnostics.

Registry URLs, credentials, and arbitrary lockfile fields do not enter the graph artifact.

# Supported Extraction Formats

| Adapter | Format | Trust behavior |
| --- | --- | --- |
| npm | `package-lock.json` and `npm-shrinkwrap.json` JSON package maps | Complete graph for regular resolved packages. |
| pnpm | `pnpm-lock.yaml` package and snapshot maps | Complete graph for regular resolved packages. |
| Yarn Classic | Yarn v1 lock entries | Complete graph for resolved entries and declared edges. |
| Yarn Modern | Yarn lock YAML entries | Complete graph for resolved entries, checksums, and declared edges. |
| Bun | Text `bun.lock` JSONC-style package entries | Complete graph for resolved entries and declared edges. |
| Bun | Binary `bun.lockb` | Blocking `LOCK_GRAPH_FORMAT_UNSUPPORTED`; convert to text before planning. |
| vlt and Deno | Preview formats | Apply remains unavailable, so production graph proof is not claimed. |

Malformed, non-UTF-8, structurally unsupported, or incomplete production lockfiles produce blocking diagnostics. pkgshift does not silently fall back to a manifest-only success claim when a source lockfile exists.

# Comparison Policy

`resolution-set-v1` is the first stable policy. It compares unique `name@version` resolutions across the source and target graphs.

- Added resolutions block verification.
- Removed resolutions block verification.
- Different integrity values block verification when both formats expose comparable integrity families.
- Edge changes are reported as evidence but do not block in this policy because package managers encode peer placement, optional dependencies, hoisting, and deduplication differently.
- An incomplete target graph blocks verification.
- When the accepted source resolution set is empty and the target manager legitimately omits an empty lockfile, verification records an absent target graph and passes the explicit empty-set proof.

The comparison artifact records counts, bounded drift lists, graph identifiers, policy identifier, and status. Future policies may add platform-aware optional dependency rules or stricter edge equivalence without changing the meaning of `resolution-set-v1`.

# Native Import Selection

The planner chooses a registered target-native migration path when official package manager behavior supports one:

- `pnpm import` for npm or Yarn sources.
- `bun pm migrate` for npm or Yarn sources, followed by a lifecycle-script-disabled install.
- Bun's install-integrated pnpm migration path for pnpm sources.
- `yarn import` for npm to Yarn Classic.
- Install-integrated Yarn Classic migration for Yarn Modern and npm where documented behavior applies.

A dedicated importer runs after deterministic target configuration is rendered and before the target install. Source lockfiles remain present through import and install, then source-only artifacts are retired. When no verified native importer exists, planning emits `NATIVE_IMPORT_UNAVAILABLE`; installation may continue only with the same blocking graph proof afterward.

# Failure Semantics

Graph extraction diagnostics participate in plan executability. Graph comparison participates in run verification. Failure preserves the run journal and recovery snapshot so the operator can inspect evidence and approve rollback. No graph mismatch is converted into an automatic manifest edit, dependency upgrade, or AI-authored repair.
