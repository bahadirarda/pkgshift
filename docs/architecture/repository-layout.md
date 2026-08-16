---
type: Codebase Architecture
title: Repository Layout
description: Maps the MVP source tree to migration planning, execution, verification, recovery, CLI, documentation, and skill boundaries.
tags: [architecture, codebase, rust, typescript, monorepo, testing]
status: draft
generated: { by: bahadirarda, at: 2026-08-16T19:53:18Z}
sources:
  - id: migration-engine
    resource: /architecture/migration-engine.md
    title: Migration Engine
  - id: agent-interface
    resource: /architecture/agent-interface.md
    title: Agent Interface
  - id: repository-source
    resource: "repository source tree at 2026-08-16"
    title: Repository source tree
---

# Layout

```text
implementations/
  rust/
    pkgshift-core/         deterministic domain engine and stable JSON models
    pkgshift-cli/          Rust command grammar, terminal reporter, and E2E fixtures
  typescript/              executable compatibility and parity-reference implementation
Cargo.toml                 root Rust workspace orchestration and shared lint contract
rust-toolchain.toml        pinned compiler, rustfmt, and Clippy toolchain
skills/
  pkgshift/                distributable portable Agent Skill source
docs/                      OKF v0.2 knowledge bundle and shared brand assets
package.json               polyglot orchestration scripts
```

The root is an orchestration boundary, not a third implementation. Shared product terminology, schema version `1.0`, package-manager baselines, safety invariants, documentation, and Agent Skill source apply to both engines.

# Runtime Boundary

The primary runtime is Rust 1.97.1. `pkgshift-core` owns detection, Project IR, capability decisions, immutable planning, integrity-checked state, repository locking, execution, verification, and recovery. `pkgshift-cli` owns the keyword command grammar and presentation boundary. Target processes run directly without a shell, lifecycle scripts are disabled, and process output is withheld from persistent Rust artifacts.

The TypeScript engine remains under `implementations/typescript` as an executable reference implementation. It has no third-party runtime dependencies and uses Bun 1.3.14 for runtime, YAML parsing, building, and tests. It remains the behavior oracle for capability renderers and ancillary commands that have not crossed the Rust parity gate.

A Rust distribution compiles to one standalone CLI, but apply still requires the selected target package manager executable.

# Implemented Command Path

`implementations/rust/pkgshift-cli/src/main.rs` delegates domain work to `pkgshift-core`. Inspection collects weighted evidence and creates a redacted repository fingerprint. Project IR extracts workspace, dependency, policy, linker, registry-reference, and integration semantics. Capability analysis classifies every observed feature for the selected target. Planning renders deterministic target content and binds exact file mutations to before and after digests.

The guided `to` command keeps its first plan read-only, requests approval, then uses `.pkgshift/state` to persist and execute the exact plan before invoking verification. `to --trial` instead copies the repository into a disposable boundary, executes importer, installer, and verifier there, and returns without source state. The advanced staged interface persists a plan only when `--state-dir` is explicit. Rust stored plans and runs use digest-verified envelopes. Apply validates exact approval and the baseline fingerprint, creates recovery snapshots, atomically writes planned content, and executes target-native import plus installation operations. Repository locks recover a dead Linux writer and serialize mutation per repository.

Verify checks planned digests, package-manager selection, lockfile behavior, workspace membership, installer completion, and normalized source-to-target resolution parity. Rollback validates every backup digest, restores snapshot entries, and requires the repository fingerprint to match the plan baseline. The TypeScript reference retains managed Agent Skill lifecycle commands until that ancillary interface is ported or replaced by distribution tooling.

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
- Rust subprocess migrations for pnpm-to-Bun with rollback and npm-to-pnpm.
- Rust subprocess trial with no source writes, native importer ordering, intentional target graph drift, and fail-closed lock format fixtures.
- Rust planning coverage for all 20 basic production-adapter directions.
- Live Rust runs with Bun 1.3.14 covering dependency-bearing npm-to-Bun trial, native migration, install, graph proof, apply, and rollback.
- TypeScript end-to-end guided and staged CLI plan, approval, apply, verify, and rollback.
- Codex and Claude Code skill copy, link, conflict, and protected uninstall behavior.
- rustfmt, warning-free Clippy, strict TypeScript, OKF, link, Agent Skill, and English-only validation.

# Post-MVP Extensions

The next architecture slice closes advanced renderer parity, adds platform-aware and edge-aware graph policies, explicit representative-script checks, target executable version resolution, and a final ancillary-command ownership decision. These extensions must preserve [Migration Engine](/architecture/migration-engine.md), [Agent Interface](/architecture/agent-interface.md), and [Rust-Primary Polyglot Monorepo](/decisions/rust-primary-polyglot-monorepo.md).
