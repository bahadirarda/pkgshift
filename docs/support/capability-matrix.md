---
type: Capability Matrix
title: Package Manager Capability Matrix
description: Defines normalized capability dimensions and how planning decisions gate MVP execution.
tags: [support, capabilities, adapters, testing]
status: draft
stale_after: 2026-11-15
generated: { by: bahadirarda, at: 2026-08-17T15:14:32Z }
sources:
  - id: package-managers
    resource: /support/package-managers.md
    title: Package Manager Support
---

# Classification

Each adapter declares every capability using one of these values:

| Value | Meaning |
| --- | --- |
| `native` | The adapter represents the behavior directly. |
| `transform` | The behavior can be preserved through a deterministic transformation. |
| `lossy` | A transformation exists but changes semantics or removes information. |
| `unsupported` | The target has no safe representation in the supported boundary. |
| `unknown` | The adapter lacks enough evidence or coverage to decide. |
| `not-applicable` | The capability does not apply to the adapter mode. |

# Required Capability Dimensions

| Group | Capabilities |
| --- | --- |
| Identity | Detection evidence, version selection, package manager pin, runtime compatibility. |
| Manifests | Dependency sections, aliases, URL and VCS sources, local paths, optional dependencies, peer metadata. |
| Workspaces | Membership, exclusions, workspace protocol, root behavior, focused commands, catalogs. |
| Resolution policy | Overrides, resolutions, constraints, deduplication policy, peer policy, lockfile semantics. |
| Installation | Frozen mode, offline mode, production mode, script policy, linker and install layout. |
| Extensions | Patches, plugins, hooks, package extensions, custom fetchers. |
| Registries | Default registry, scoped registries, authentication references, certificates, proxies. |
| Execution | Script invocation, binary execution, recursive or filtered execution, environment behavior. |
| Repository integration | CI setup, caches, containers, release tooling, task runners, contributor commands. |
| Verification | Install checks, graph extraction, integrity evidence, script checks, rollback support. |

# MVP Execution Matrix

This matrix identifies the implemented planning focus. Individual plans still fail closed when a feature-specific execution renderer is unavailable.

| Adapter | Workspaces | Catalog-like policy | Override policy | Patches or plugins | Linker or layout modes | Apply capable |
| --- | --- | --- | --- | --- | --- | --- |
| npm | Required | Analyze/transform | Required | Detect and diagnose | Detect assumptions | Yes, capability-gated |
| pnpm | Required | Required | Required | Required | Required | Yes, capability-gated |
| Yarn Classic | Required | Analyze/transform | Required | Detect and diagnose | Detect assumptions | Yes, capability-gated |
| Yarn Modern | Required | Analyze/transform | Required | Required | Required | Yes, capability-gated |
| Bun | Required | Required where available | Required | Required for exact text patches | Detect assumptions | Yes, capability-gated |
| vlt | Required | Required | Required through graph modifiers | Unsupported and blocking | Native isolated layout | Yes |
| Deno dependency mode | Required | Lossy expansion | Required through npm overrides | Unsupported and blocking | Native isolated layout | Yes |

vlt apply is limited to the deterministic workspace, catalog, modifier, public registry, integration, installer, and lock graph subset. Deno apply is limited to npm-compatible package metadata and registry behavior, workspace configuration, override policy, catalog expansion, isolated linking, preserved Deno configuration, integration commands, installation, and lock proof. Neither production tier converts unsupported protocols or lifecycle allow-lists, and Deno dependency mode does not imply a runtime migration.

# Rule Shape

A capability rule should contain:

- A stable capability identifier.
- Source evidence requirements.
- Source and target classifications.
- A deterministic transformation identifier when available.
- Risk level and user-facing diagnostic codes.
- Preconditions, postconditions, and fixture references.
- A statement of expected dependency graph effects.

Adapters must not infer support from syntax similarity alone. A shared manifest field may have different resolution, peer, lifecycle, or workspace semantics. Capability support in the target does not imply renderer coverage: `TRANSFORMATION_UNIMPLEMENTED` blocks apply when the MVP cannot render an otherwise valid target feature safely.

The production patch boundary transports one project-relative text unified diff among Yarn Modern, pnpm, and Bun. Yarn Modern and pnpm retain exact, portable semver-range, and name-only semantics. When Yarn is the target, range and name-only declarations expand into exact locators observed in project dependencies or the source lock graph; missing resolution evidence blocks planning instead of producing an ineffective wildcard patch. Bun conversion requires an exact `name@version`. Binary patches, missing files, parent-directory or symbolic-link paths, remote paths, multiple patch sources, and optional or parameterized Yarn locators fail closed.
