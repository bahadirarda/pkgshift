---
type: Data Model
title: Project IR
description: Defines the versioned, evidence-linked semantic representation consumed by capability analysis and planning.
tags: [architecture, ir, manifests, workspaces, provenance]
status: draft
stale_after: 2026-11-15
generated: { by: bahadirarda, at: 2026-08-16T22:38:36Z}
sources:
  - id: migration-engine
    resource: /architecture/migration-engine.md
    title: Migration Engine
  - id: npm-manifest
    resource: https://docs.npmjs.com/cli/configuring-npm/package-json/
    title: npm package.json documentation
  - id: pnpm-settings
    resource: https://pnpm.io/settings
    title: pnpm settings documentation
  - id: pnpm-catalogs
    resource: https://pnpm.io/catalogs
    title: pnpm catalogs documentation
  - id: yarn-manifest
    resource: https://yarnpkg.com/configuration/manifest
    title: Yarn manifest documentation
  - id: bun-workspaces
    resource: https://bun.sh/docs/pm/workspaces
    title: Bun workspace documentation
---

# Purpose

Project IR is the package-manager-neutral input to capability analysis and planning. It records semantics and evidence without allowing an adapter to mutate files directly.

# Contract

Every Project IR artifact contains:

- `schemaVersion` and a content-derived `projectIrId`.
- The migration-relevant repository fingerprint and detected source adapter.
- Root and workspace packages selected by workspace membership.
- Dependency section, name, redacted specifier, and classified protocol.
- Workspace patterns, selected package paths, and source evidence.
- Policy shapes for overrides, resolutions, package extensions, patches, catalogs, and lifecycle allow-lists.
- Observed features consumed by the capability engine.
- Repository integrations and structured diagnostics.

Integration evidence includes registered CI, container, automation, Markdown command, devcontainer lifecycle, and toolchain-pin files. The Project IR records kind, path, and detected package-manager tokens without treating arbitrary prose as an executable command.

# Evidence Boundary

Workspace packages are selected from root manifest, pnpm workspace, or Deno workspace patterns. The fingerprint covers all discovered package manifests, package manager configuration, lockfiles, patch evidence, and detected integrations.

Authentication material is never stored in Project IR. Registry configuration contributes only file presence and redacted evidence. Fingerprinting redacts credential assignments, URL user information, sensitive query values, and bearer tokens before hashing. Linker evidence is normalized from current pnpm workspace settings, Yarn configuration, Bun install configuration, and legacy pnpm `.npmrc` input. Yarn lifecycle metadata becomes an allow-list feature only when dependency scripts are disabled globally.

# Dependency Protocols

The current model distinguishes semver, tags, workspace, catalog, npm alias, file, link, portal, patch, Git, URL, JSR, and unknown specifiers. Protocol classification is syntactic evidence; capability analysis determines whether the selected target preserves its semantics.

# Policy Shape

Policies retain location, JSON or configuration pointer, entry count, and whether nested objects occur. They do not retain secret-bearing raw configuration. This is sufficient to select a capability rule. During read-only planning, the transformation boundary re-reads accepted source locations and emits exact, secret-safe target mutations bound to source and target digests.

# Parsing Failure

Invalid root or workspace manifests, invalid dependency section shapes, non-string dependency specifiers, and unparseable migration configuration produce blocking diagnostics. The planner may return an inspectable blocked artifact but cannot make it executable.
