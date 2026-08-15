---
type: Codebase Architecture
title: Repository Layout
description: Maps the MVP source tree to migration planning, execution, verification, recovery, CLI, documentation, and skill boundaries.
tags: [architecture, codebase, typescript, testing]
status: draft
generated: { by: bahadirarda, at: 2026-08-15T19:53:59Z}
sources:
  - id: migration-engine
    resource: /architecture/migration-engine.md
    title: Migration Engine
  - id: agent-interface
    resource: /architecture/agent-interface.md
    title: Agent Interface
  - id: repository-source
    resource: "repository source tree at 2026-08-15"
    title: Repository source tree
---

# Layout

```text
src/
  adapters/       package manager definitions and release baselines
  artifacts/      immutable plan bundle persistence and integrity checks
  capabilities/   feature rules and source-to-target analysis
  cli/            argument parsing, command routing, and reporters
  core/           deterministic file, redaction, hashing, and serialization utilities
  diagnostics/    stable diagnostic explanations
  domain/         versioned result, inspection, operation, and plan types
  execution/      apply orchestration, process execution, and persisted process records
  inspect/        repository evidence collection and source detection
  ir/             versioned Project IR extraction
  journal/        run state machine and revisioned persistence
  plan/           read-only package manager plan construction
  recovery/       pre-mutation snapshots and rollback orchestration
  skills/         Codex and Claude Code Agent Skill installation
  transform/      deterministic package-manager target rendering
  verification/   run-bound structural verification and report persistence
tests/             isolated unit, failure, integration, and CLI transaction fixtures
skills/            distributable portable Agent Skill source
docs/              OKF v0.2 knowledge bundle
```

# Runtime Boundary

The source uses TypeScript without third-party runtime package dependencies. TypeScript and Bun declarations are development-only dependencies. Bun 1.3.14 is the pinned development runtime, YAML parser, build tool, and test harness. Filesystem, hashing, artifact, journal, snapshot, and verification boundaries use Node-compatible standard library APIs.

A distribution can compile a standalone CLI, but apply still requires the selected target package manager executable. The target process runs directly without a shell and with dependency lifecycle scripts disabled.

# Implemented Command Path

`src/cli.ts` delegates to argument parsing and command routing. Inspection collects evidence and creates a redacted repository fingerprint. Project IR extracts workspace, dependency, policy, linker, registry-reference, and integration semantics. Capability analysis classifies every observed feature for the selected target. Planning renders deterministic target content and binds exact file mutations to before and after digests.

The guided `to` command keeps its first plan read-only, requests approval, then uses `.pkgshift/state` to persist and execute the exact plan before invoking verification. The advanced staged interface persists a plan only when `--state-dir` is explicit. Apply validates exact approval and the baseline fingerprint, creates recovery snapshots, journals each operation transition, atomically writes planned content, and executes the target installer. Process output is bounded, redacted, and persisted with an integrity digest.

Verify checks planned digests, package-manager selection, lockfile creation, workspace membership, and apply completion. Rollback restores snapshot entries and requires the repository fingerprint to match the plan baseline. Skill commands install the portable source into Codex or Claude Code project and user locations without overwriting conflicting or locally modified installations.

# Test Boundary

Tests create isolated temporary repositories and remove only generated fixtures. Coverage includes:

- Explicit, ambiguous, and conflicting package-manager detection.
- Project IR extraction and secret-safe repository fingerprints.
- Native, transformed, lossy, unsupported, and unknown capability decisions.
- Deterministic target rendering for npm, pnpm, both Yarn families, and Bun.
- Exact approval and stale-plan rejection before mutation.
- Artifact, snapshot, execution report, and journal integrity.
- Journal revision conflicts and orphan-lock recovery.
- Successful and failed target installation paths.
- Mid-run precondition conflicts and partial-failure rollback.
- End-to-end guided and staged CLI plan, approval, apply, verify, and rollback.
- Codex and Claude Code skill copy, link, conflict, and protected uninstall behavior.
- Strict TypeScript, OKF, link, Agent Skill, and English-only validation.

# Post-MVP Extensions

The next architecture slice adds normalized resolved-lock graphs, graph-drift policy, explicit representative-script checks, target executable version resolution, packaged binary distribution, and additional Agent Skill client destinations. These extensions must preserve [Migration Engine](/architecture/migration-engine.md) and [Agent Interface](/architecture/agent-interface.md).
