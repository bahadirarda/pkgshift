---
type: System Architecture
title: Migration Engine
description: Defines a capability-aware migration engine built around a shared project intermediate representation.
tags: [architecture, engine, ir, transactions]
status: draft
generated: { by: bahadirarda, at: 2026-08-16T19:53:18Z}
sources:
  - id: product-vision
    resource: /product/vision.md
    title: pkgshift Product Vision
  - id: transactional-decision
    resource: /decisions/transactional-migrations.md
    title: Transactional Migrations
---

# Architecture

The engine uses a shared Project IR between source and target adapters. This avoids implementing every package manager pair independently.

```text
repository evidence
        |
        v
source adapter -> Project IR -> capability analysis -> target adapter
       |                                                   |
       v                                                   v
 source lock graph                                native import strategy
                                           |                |
                                           v                v
                                      diagnostics       operations
                                           \                /
                                            v              v
                                                plan
                                                  |
                                         approval boundary
                                                  |
                                                  v
                                       executor -> journal
                                                  |
                                                  v
                                               verifier
```

With `n` package managers, this design aims for approximately `2n` semantic boundaries rather than `n * (n - 1)` pairwise migrators. Pair-specific rules remain possible, but only for behavior that cannot be expressed through shared capabilities.

# Components

## Inspector

Collect repository evidence without mutation. Record file locations, detected versions, relevant content fingerprints, and confidence. Detection must not rely on a lockfile alone.

## Source Adapter

Parse supported source semantics into the Project IR. Preserve unknown or non-portable source features as explicit evidence and diagnostics rather than dropping them.

## Project IR

Represent migration-relevant semantics independently of package manager syntax:

- Project roots and package manifests.
- Workspace membership and package graph.
- Dependency specifications and protocols.
- Overrides, resolutions, catalogs, constraints, and peer policy.
- Registry scopes and redacted authentication references.
- Patches, lifecycle behavior, and linker assumptions.
- Package manager and runtime pins.
- CI, container, cache, release, and contributor integrations.
- Lockfile and resolved dependency graph summaries.

Every IR node that may affect a plan should retain a link to its originating evidence.

## Capability Analyzer

Compare source semantics with target capabilities. Classify each feature as directly representable, transformable, lossy, unsupported, or unknown. Emit diagnostics before rendering file edits.

## Planner

Produce an immutable plan containing:

- Repository fingerprint and selected adapters.
- Normalized options and policy inputs.
- Ordered operations with preconditions and postconditions.
- Exact file actions, before and after digests, safe target content, commands, side effects, and rollback scope.
- Capability losses and risk annotations.
- Verification checks and success criteria.

The plan identifier is derived from normalized plan content. A production-target plan is executable only when no blocking diagnostic remains and lossy decisions were explicitly accepted during planning. Apply rejects a plan when relevant repository evidence no longer matches its preconditions.

When a source lockfile exists, planning also binds its normalized graph identifier and selects a registered target-native importer where official package manager behavior supports one. Dedicated import commands run before target installation. Source artifacts remain available until both import and installation finish.

## Target Adapter

Render target-native configuration and commands from the Project IR and capability decisions. Rendering must be deterministic for the same normalized inputs.

## Executor

Apply operations through a journaled workspace transaction. Require exact plan approval. Before the first mutation, snapshot every planned file and target lockfile with owner-only permissions and content digests. Recheck each mutation digest, use atomic replacement, remove accepted package-local `node_modules` paths without following symbolic links, execute the target installer without a shell or lifecycle scripts, persist cleanup and redacted process evidence, and stop at the first failed required operation.

## Verifier

The MVP evaluates declared postconditions at these levels:

1. Structural validation of generated configuration.
2. Target package manager selection and target lockfile creation.
3. Successful completion of the journaled target install operation.
4. Workspace membership preservation.
5. Planned integration file digests.

The Rust verifier independently extracts the target lock graph. It applies `reachable-resolution-set-v2` when both formats expose dependency topology and retains `resolution-set-v1` for formats without that evidence. V2 prunes proven unreachable entries, distinguishes optional-only package-name absence, and fails closed on unresolved required paths. Added or removed reachable `name@version` resolutions and comparable integrity mismatches block completion. Edge changes remain evidence because package managers encode peer placement, hoisting, and deduplication differently. When no source lockfile existed, graph comparison is explicitly skipped. Explicitly selected representative root scripts execute through bounded, shell-free operations and are verified from their journaled results without rerunning repository code.

## Trial Executor

An approved guided plan may execute through `--trial`. The Rust CLI copies regular repository files into a disposable directory, rejects symbolic links, omits repository metadata and generated dependency or build directories, executes the same plan and verifier there, and deletes the sandbox automatically. The source repository receives no persistent plan or run state. Trial still declares process execution and possible network or cache effects.

## Recovery

Restore repository files from the run snapshot only after exact run approval. Verify every backup digest before writing and verify the restored repository fingerprint afterward. Report dependency caches and `node_modules` as external effects rather than claiming they were restored.

## Reporter

Render the same result model as structured JSON or concise terminal text. Rendering cannot change exit status, diagnostics, or next actions.

# Extension Boundaries

- Add a package manager through an adapter plus declared capabilities and fixtures.
- Add repository behavior through an integration detector and transformer.
- Add policy through versioned configuration interpreted by the planner.
- Add presentation through a reporter without changing core results.

# Invariants

- Inspection and planning do not mutate the repository.
- No adapter writes files directly; it returns operations to the planner.
- Unknown semantics remain visible.
- Plans bind to repository evidence.
- Apply emits a journal even when it fails.
- Verification results reference the exact run they evaluate.
- Trial approval authorizes sandbox process execution but not repository mutation.
- Secrets never enter the Project IR as clear text.
