---
type: Decision Engine
title: Capability Engine
description: Defines how observed Project IR features become source-to-target compatibility decisions.
tags: [architecture, capabilities, adapters, diagnostics]
status: draft
stale_after: 2026-11-15
generated: { by: bahadirarda, at: 2026-08-15T19:53:59Z}
sources:
  - id: capability-matrix
    resource: /support/capability-matrix.md
    title: Package Manager Capability Matrix
  - id: project-ir
    resource: /architecture/project-ir.md
    title: Project IR
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
  - id: yarn-linkers
    resource: https://yarnpkg.com/features/linkers
    title: Yarn linker documentation
  - id: bun-install
    resource: https://bun.sh/docs/pm/cli/install
    title: Bun install documentation
  - id: bun-overrides
    resource: https://bun.sh/docs/pm/overrides
    title: Bun override documentation
---

# Decision Model

The engine evaluates only observed Project IR features. Each decision records:

- Stable feature identifier and human-readable title.
- Target adapter.
- Classification and risk.
- Deterministic transformation identifier when one exists.
- Evidence references and official documentation basis.

The analysis identifier is derived from Project IR, source, target, decisions, and summary counts.

# Classifications

| Classification | Planning effect |
| --- | --- |
| `native` | Preserve the feature through the target's native representation. |
| `transform` | Add a deterministic transformation operation. |
| `lossy` | Add a transformation operation and `CAPABILITY_LOSSY`; require `--accept-lossy` while creating an executable plan. |
| `unsupported` | Add blocking `CAPABILITY_UNSUPPORTED`. |
| `unknown` | Add blocking `CAPABILITY_UNKNOWN`; never infer support. |
| `not-applicable` | Record that the target mode does not use the source mechanism. |

# Implemented Feature Families

- Workspace membership, exclusions, and workspace protocol.
- Catalog definitions and catalog dependency protocol.
- Link, portal, and patch dependency protocols.
- Overrides, nested overrides, resolutions, and package extensions.
- Patched dependency policy.
- Plug and Play and isolated linker assumptions.
- Yarn constraints and pnpm hook modules.
- npm-compatible registry configuration.
- Dependency lifecycle allow-list policy.

# Safety Behavior

Lossy decisions remain reviewable because the semantic compromise is explicit. Unsupported and unknown decisions block the plan from execution. Preview targets default to unknown when their adapter has not verified a rule.

Rules cite authoritative package manager documentation and carry a freshness deadline. Similar field names do not establish semantic compatibility. For example, Bun supports top-level npm overrides but not nested override objects, so nested npm overrides to Bun are classified as unsupported.[^bun-overrides]

[^bun-overrides]: Bun override documentation
