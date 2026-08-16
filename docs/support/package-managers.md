---
type: Support Policy
title: Package Manager Support
description: Defines implemented package manager target tiers and execution boundaries for the MVP.
tags: [support, npm, pnpm, yarn, bun, vlt, deno]
status: draft
stale_after: 2026-11-15
generated: { by: bahadirarda, at: 2026-08-16T22:38:36Z}
sources:
  - id: npm-install
    resource: https://docs.npmjs.com/cli/install/
    title: npm install documentation
  - id: pnpm-workspaces
    resource: https://pnpm.io/workspaces
    title: pnpm workspace documentation
  - id: pnpm-catalogs
    resource: https://pnpm.io/catalogs
    title: pnpm catalog documentation
  - id: pnpm-import
    resource: https://pnpm.io/cli/import
    title: pnpm import documentation
  - id: pnpm-node-linker
    resource: https://pnpm.io/settings/node-modules
    title: pnpm node linker documentation
  - id: pnpm-build-policy
    resource: https://pnpm.io/settings/build
    title: pnpm build policy documentation
  - id: yarn-modern
    resource: https://yarnpkg.com/
    title: Yarn documentation
  - id: yarn-configuration
    resource: https://yarnpkg.com/configuration/yarnrc
    title: Yarn configuration documentation
  - id: yarn-manifest
    resource: https://yarnpkg.com/configuration/manifest
    title: Yarn manifest documentation
  - id: yarn-classic
    resource: https://classic.yarnpkg.com/lang/en/docs/workspaces/
    title: Yarn Classic workspace documentation
  - id: bun-install
    resource: https://bun.com/docs/pm/cli/install
    title: Bun install documentation
  - id: bun-pm-migrate
    resource: https://bun.sh/docs/pm/cli/pm#migrate
    title: Bun package manager migration documentation
  - id: yarn-classic-import
    resource: https://classic.yarnpkg.com/lang/en/docs/cli/import/
    title: Yarn Classic import documentation
  - id: bun-workspaces
    resource: https://bun.sh/docs/pm/workspaces
    title: Bun workspace documentation
  - id: vlt-migration
    resource: https://docs.vlt.sh/cli/migration
    title: vlt migration documentation
  - id: deno-packages
    resource: https://docs.deno.com/runtime/packages/
    title: Deno package documentation
  - id: deno-workspaces
    resource: https://docs.deno.com/runtime/fundamentals/workspaces/
    title: Deno workspace documentation
  - id: npm-releases
    resource: https://github.com/npm/cli/releases
    title: npm CLI releases
  - id: yarn-releases
    resource: https://github.com/yarnpkg/berry/releases
    title: Yarn releases
  - id: bun-releases
    resource: https://github.com/oven-sh/bun/releases
    title: Bun releases
  - id: deno-releases
    resource: https://github.com/denoland/deno/releases
    title: Deno releases
---

# Support Tiers

The labels below describe current technical MVP behavior. Production target means a plan may execute only when every observed repository capability is native, deterministically rendered, or explicitly accepted as lossy. Unsupported, unknown, unsafe, and unimplemented transformations remain blocking.

| Adapter | MVP tier | Boundary |
| --- | --- | --- |
| npm | Production target | Manifests, lockfile, workspaces, overrides, registry configuration, scripts, CI, and containers. |
| pnpm | Production target | npm-compatible semantics plus workspace files, catalogs, overrides, patches, explicit linker selection, and `allowBuilds` lifecycle policy. |
| Yarn Classic | Production target | Classic lockfile, workspace behavior, resolutions, registry configuration, and script commands. |
| Yarn Modern | Production target | Modern lockfile, workspace tools, protocols, constraints, patches, plugins, portable linker modes, environment-backed registry translation, and lifecycle allow-lists. |
| Bun | Production target | Bun lockfile, install behavior, workspaces, overrides, catalogs where supported, isolated linking, trusted dependencies, registry configuration, and script commands. |
| vlt | Preview target | Detection, capability reporting, and guarded migration planning before production guarantees. |
| Deno dependency mode | Preview target | npm and JSR dependency declarations plus workspace dependency behavior; not a runtime migration. |

Preview target means the versioned adapter may inspect and plan, but apply remains unavailable.

# Version Baselines

The adapter catalog pins the following researched baselines as of 2026-08-15. These are explicit capability baselines, not a promise to resolve `latest` dynamically.

| Adapter | Baseline |
| --- | --- |
| npm | `npm@12.0.2` |
| pnpm | `pnpm@11.21.0` |
| Yarn Classic | `yarn@1.22.22` |
| Yarn Modern | `yarn@4.18.0` |
| Bun | `bun@1.3.14` |
| vlt | `vlt@1.0.2` |
| Deno dependency mode | `deno@2.9.5` |

Apply invokes the target executable available to the repository environment. The operator must provide a version compatible with the declared baseline; automatic executable acquisition and version enforcement remain post-MVP work.

# Detection Evidence

Detection combines multiple signals:

- The `packageManager` field and toolchain manager configuration.
- Lockfiles and package-manager-specific configuration.
- Workspace configuration and dependency protocols.
- CI, container, task runner, and documentation commands.
- Installed metadata when present and safe to inspect.

Conflicting evidence is a diagnostic, not an invitation to guess. The user or calling agent must select a source when evidence cannot be resolved deterministically.

# Distinct Adapter Families

Yarn Classic and Yarn Modern are separate adapters because their configuration, linker, plugin, protocol, and lockfile semantics differ materially. Deno dependency mode is separate from Deno runtime modernization. Corepack, Volta, CI providers, container systems, Nx, Turborepo, Rush, and Lerna are integrations rather than package manager targets.

# MVP Adapter Guarantees

Production targets provide:

- Versioned capability declarations.
- Detection fixtures for positive, negative, and conflicting evidence.
- Source parsing and target rendering fixtures.
- Deterministic render plans for the implemented semantic subset.
- Representative single-package and workspace planning scenarios.
- Redaction tests for registry and environment configuration.
- Apply failure and rollback tests.
- Structural verification tied to the plan and apply journal.
- Normalized source lock graph extraction and blocking target resolution-set verification in the Rust primary path.
- Registered target-native importer or install-integrated migration selection where official behavior supports it.
- Approved isolated execution trials through the Rust primary CLI.
- Bidirectional lifecycle allow-list translation among Bun `trustedDependencies`, pnpm `allowBuilds`, and Yarn Modern `dependenciesMeta` with `enableScripts: false`.
- Plug and Play or isolated linker translation to pnpm, Yarn Modern, Bun, or an explicitly accepted hoisted layout.
- Secret-safe `.npmrc` registry translation to Yarn Modern for default and scoped registries, boolean authentication policy, and environment-backed tokens.

Advanced source features such as arbitrary pnpm hooks, Yarn JavaScript constraints, Yarn build denials outside allow-list mode, unsupported patch conversions, workspace glob or protocol syntax outside the deterministic subset, unrecognized npm configuration, unsafe literal registry credentials, dependency-level lifecycle policy targeting npm or Yarn Classic, or selectors outside the implemented subset block execution. Binary `bun.lockb` graph proof also blocks until the lockfile is converted to text. Edge-level and platform-aware graph policies remain release-hardening extensions beyond the blocking `resolution-set-v1` policy.

# Native Migration Paths

The Rust planner preserves source lockfiles through import and installation, then retires source-only artifacts after success.

| Source | Target | Selected path |
| --- | --- | --- |
| npm or Yarn | pnpm | `pnpm import`, then lifecycle-script-disabled `pnpm install`. |
| npm or Yarn | Bun | `bun pm migrate`, then lifecycle-script-disabled `bun install`. |
| pnpm | Bun | Bun's install-integrated pnpm migration path. |
| npm | Yarn Classic | `yarn import`, then lifecycle-script-disabled `yarn install`. |
| Yarn Classic | Yarn Modern | Install-integrated Yarn migration. |
| Yarn Classic | npm | npm's `yarn.lock`-aware install path. |

Other directions generate target dependency state and emit `NATIVE_IMPORT_UNAVAILABLE` when a source lockfile exists. Every production direction still passes or fails on target verification; a native importer is not treated as proof by itself.

# Freshness

Package manager behavior changes frequently. When this concept reaches `stale_after`, re-check all production and preview boundaries against official documentation before treating the matrix as current.
