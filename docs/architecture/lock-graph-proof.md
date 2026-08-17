---
type: Reliability Architecture
title: Lock Graph Proof
description: Defines normalized source and target lock graphs, native importer selection, and blocking reachable-resolution verification.
tags: [architecture, lockfile, dependency-graph, verification, integrity]
status: draft
stale_after: 2026-09-15
generated: { by: bahadirarda, at: 2026-08-17T07:24:55Z}
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
  - id: vlt-lockfile
    resource: https://docs.vlt.sh/cli/migration/from-npm
    title: vlt migration from npm documentation
  - id: deno-lockfile
    resource: https://docs.deno.com/examples/dependency_lockfile_tutorial/
    title: Deno lockfile documentation
---

# Proof Boundary

The Rust engine extracts the accepted source lockfile into a normalized `LockGraph` before planning. The stored plan binds to the graph identifier and persists the graph beside Project IR and capability analysis. After target installation, verification extracts the target lockfile independently and compares both graphs. A target install process exiting successfully is necessary but no longer sufficient.

The graph intentionally contains dependency evidence rather than raw lockfile content:

- Package name, resolved version, source locator, and integrity value when present.
- Logical dependency, optional dependency, and peer dependency edges where the format exposes them, including an exact `name@version` target when the lockfile provides enough locator evidence.
- Source manager, format, lockfile path, content digest, completeness, and diagnostics.

Registry URLs, credentials, and arbitrary lockfile fields do not enter the graph artifact.

# Supported Extraction Formats

| Adapter | Format | Trust behavior |
| --- | --- | --- |
| npm | `package-lock.json` and `npm-shrinkwrap.json` JSON package maps | Complete graph for regular resolved packages. |
| pnpm | `pnpm-lock.yaml` package and snapshot maps | Complete graph for registry resolutions; local `file:`, `link:`, and `workspace:` package locators are excluded from the cross-manager registry set. |
| Yarn Classic | Yarn v1 lock entries | Complete graph for resolved entries and declared edges. |
| Yarn Modern | Yarn lock YAML entries | Complete graph for resolved entries, checksums, and declared edges. |
| Bun | Text `bun.lock` JSONC-style package entries | Complete graph for resolved entries and declared edges. |
| Bun | Binary `bun.lockb` | Blocking `LOCK_GRAPH_FORMAT_UNSUPPORTED`; convert to text before planning. |
| vlt | `vlt-lock.json` v1 node map | Complete graph for registry nodes, including the current registry-qualified locator form. |
| Deno | `deno.lock` v5 npm and JSR maps | Complete graph for registry entries and declared edges; peer-context suffixes normalize to their base registry versions. |

Malformed, non-UTF-8, structurally unsupported, or incomplete production lockfiles produce blocking diagnostics. pkgshift does not silently fall back to a manifest-only success claim when a source lockfile exists.

# Comparison Policy

`resolution-set-v1` remains a stable whole-lockfile policy. It compares every unique `name@version` resolution across the source and target graphs and remains active for formats, including vlt v1, that do not expose enough topology for reachability proof.

`reachable-resolution-set-v2` is the default when both formats expose dependency topology. It starts from external dependencies declared by every Project IR package, excludes local workspace and filesystem protocols, and traverses normalized edges. Exact lockfile targets are followed when available. When only a dependency name is available, every matching version remains reachable; this conservative expansion may retain obsolete same-name entries but cannot silently discard a possible dependency.

- Added resolutions block verification.
- Removed resolutions block verification.
- Different integrity values block verification when both formats expose comparable integrity families.
- Resolutions not reachable from a manifest root are pruned and recorded separately under V2.
- A package reached only through optional or peer edges may be absent on one platform when that package name is entirely absent on the other graph. An optional package present on both sides with different versions still blocks verification.
- Missing required roots, edges, or exact targets block V2 instead of falling back to a manifest-only success claim.
- Edge changes are reported as evidence but do not block in this policy because package managers encode peer placement, optional dependencies, hoisting, and deduplication differently.
- An incomplete target graph blocks verification.
- When the accepted source resolution set is empty and the target manager legitimately omits an empty lockfile, verification records an absent target graph and passes the explicit empty-set proof.

The comparison artifact records counts, bounded drift lists, graph identifiers, policy identifier, pruned resolutions, tolerated optional platform differences, reachability issues, and status. V2 does not change the meaning of `resolution-set-v1`, and neither policy treats edge-shape differences as blocking equivalence yet.

# Native Import Selection

The planner chooses a registered target-native migration path when official package manager behavior supports one:

- `pnpm import` for npm or Yarn sources.
- `bun pm migrate` for npm or Yarn sources, followed by a lifecycle-script-disabled install.
- Bun's install-integrated pnpm migration path for pnpm sources.
- `yarn import` for npm to Yarn Classic.
- Install-integrated Yarn Classic migration for Yarn Modern and npm where documented behavior applies.
- Install-integrated npm migration for Deno dependency mode.

A dedicated importer runs after deterministic target configuration is rendered and before the target install. Source lockfiles remain present through import and install, then source-only artifacts are retired. When no verified native importer exists, planning emits `NATIVE_IMPORT_UNAVAILABLE`; installation may continue only with the same blocking graph proof afterward.

# Failure Semantics

Graph extraction diagnostics participate in plan executability. Graph comparison participates in run verification. Failure preserves the run journal and recovery snapshot so the operator can inspect evidence and approve rollback. No graph mismatch is converted into an automatic manifest edit, dependency upgrade, or AI-authored repair.

Both policies remain fail-closed. V2 removes only entries that topology proves unreachable, tolerates only package-name absence on optional-only paths, and keeps reachable version or integrity drift blocking. Isolated trial exposes those distinctions before repository writes. If a format lacks topology, the report names `resolution-set-v1` explicitly rather than implying V2 coverage.
