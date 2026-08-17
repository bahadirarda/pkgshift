---
type: Codebase Architecture
title: Repository Layout
description: Maps the MVP source tree to migration planning, execution, verification, recovery, CLI, documentation, and skill boundaries.
tags: [architecture, codebase, rust, typescript, monorepo, testing]
status: draft
generated: { by: bahadirarda, at: 2026-08-17T00:38:12Z}
sources:
  - id: migration-engine
    resource: /architecture/migration-engine.md
    title: Migration Engine
  - id: agent-interface
    resource: /architecture/agent-interface.md
    title: Agent Interface
  - id: repository-source
    resource: "repository source tree at 2026-08-17"
    title: Repository source tree
---

# Layout

```text
implementations/
  rust/
    pkgshift-core/         deterministic domain engine and stable JSON models
      src/
        capability.rs      feature classification rules
        cleanup.rs         clean-install planning, execution, and residue proof
        runtime/           dedicated runtime inspection, recipes, transactions, and rollback
        plan.rs            immutable plan orchestration
        plan/tests.rs      planner regression suite
        transaction.rs     apply, trial, verify, and rollback orchestration
        transformation.rs  shared deterministic renderer policies
        transformation/
          project.rs       project mutation composition
          registry.rs      registry configuration translation
        verification.rs    structural and lock-graph verification
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

The primary runtime is Rust 1.97.1. `pkgshift-core` owns detection, Project IR, capability decisions, immutable planning, integrity-checked state, repository locking, execution, verification, and recovery. These responsibilities are separated into focused modules: planning composes operations, transformation modules render target semantics, cleanup owns generated dependency-state retirement, verification owns post-apply proof, and transaction owns orchestration only. `pkgshift-cli` owns the keyword command grammar and presentation boundary. Target processes run directly without a shell, lifecycle scripts are disabled, and process output is withheld from persistent Rust artifacts.

Production Rust modules are limited to 1,000 physical lines by repository validation. Test-only modules are excluded from that mechanical limit and remain colocated under their owning module directory. A file below the limit must still represent one cohesive responsibility; the limit is a regression gate rather than a design target.

The TypeScript engine remains under `implementations/typescript` as an executable reference implementation. It has no third-party runtime dependencies and uses Bun 1.3.14 for runtime, YAML parsing, building, and tests. It remains a behavior oracle for shared capability renderers, while the Rust primary owns production execution, runtime recipes, Skill lifecycle, and stored-artifact explanation.

A Rust distribution compiles to one standalone CLI, but apply still requires the selected target package manager executable.

# Implemented Command Path

`implementations/rust/pkgshift-cli/src/main.rs` delegates domain work to `pkgshift-core`. Inspection collects weighted evidence and creates a redacted repository fingerprint. Project IR extracts workspace, dependency, policy, linker, registry-reference, and integration semantics. Capability analysis classifies every observed feature for the selected target. Planning renders deterministic target content and binds exact file mutations to before and after digests.

The guided `to` command keeps its first plan read-only, requests approval, then uses `.pkgshift/state` to persist and execute the exact plan before invoking verification. `to --trial` instead copies the repository into a disposable boundary, executes cleanup, importer, installer, and verifier there, and returns without source state. The dedicated `runtime to deno` surface uses its own fingerprint, recipe planner, content-redacted artifact, private state subtree, verification report, and rollback command without entering the package-manager adapter pipeline. The advanced staged interface persists a package-manager plan only when `--state-dir` is explicit. Rust stored plans and runs use digest-verified envelopes. Apply validates exact approval and the baseline fingerprint, creates recovery snapshots, atomically writes planned content, removes pre-migration package-local dependency state, and executes target-native import plus installation operations. Repository locks recover a dead Linux writer and serialize mutation per repository.

Verify checks planned digests, clean-install records, source-artifact residue, package-manager selection, lockfile behavior, workspace membership, installer completion, and normalized source-to-target resolution parity. Rollback validates every backup digest, restores snapshot entries, and requires the repository fingerprint to match the plan baseline. The Rust primary path owns Agent Skill copy, link, status, doctor, and protected uninstall behavior; release tooling must install the same portable source into the shared data path resolved by that lifecycle.

# Test Boundary

Tests create isolated temporary repositories and remove only generated fixtures. Coverage includes:

- Explicit, ambiguous, and conflicting package-manager detection.
- Project IR extraction and secret-safe repository fingerprints.
- Native, transformed, lossy, unsupported, and unknown capability decisions.
- Deterministic target rendering for npm, pnpm, both Yarn families, Bun, vlt, and Deno dependency mode.
- Rust policy fixtures for npm and pnpm overrides, Yarn resolutions, scoped packages, unsupported selectors, and policy discovered in `pnpm-workspace.yaml`.
- Exact approval and stale-plan rejection before mutation.
- Artifact, snapshot, execution report, and journal integrity.
- Journal revision conflicts and orphan-lock recovery.
- Successful and failed target installation paths.
- Package-local dependency-state cleanup, symbolic-link refusal, cleanup journaling, and source-artifact residue proof.
- Mid-run precondition conflicts and partial-failure rollback.
- Rust subprocess migrations for pnpm-to-Bun with rollback and npm-to-pnpm with nested override rendering.
- Rust subprocess trial with no source writes, native importer ordering, intentional target graph drift, and fail-closed lock format fixtures.
- Rust planning coverage for all 42 basic production-adapter directions.
- Live Rust runs with Bun 1.3.14 covering dependency-bearing npm-to-Bun trial, native migration, install, graph proof, apply, and rollback.
- Live Rust runs with vlt 1.0.2 and Deno 2.9.5 covering dependency-bearing workspaces, clean target installation, source-state retirement, and graph proof.
- Dedicated Bun-to-Deno runtime subprocess coverage for permission blocking, source and script recipes, type residue cleanup, redacted artifacts, verification, rollback, and a real Hono type-check and test on Deno 2.9.5.
- Pinned upstream corpus runs covering executable plans, capability blockers, installer failures, post-install graph rejection, and source preservation.
- TypeScript end-to-end guided and staged CLI plan, approval, apply, verify, and rollback.
- Rust and TypeScript Codex and Claude Code skill copy, link, conflict, project/user confinement, read-only preview, and protected uninstall behavior.
- Rust diagnostic catalog coverage plus package-manager and runtime artifact explanation, identity, tamper, bounded-scan, and traversal-refusal fixtures.
- rustfmt, warning-free Clippy, strict TypeScript, OKF, link, Agent Skill, and English-only validation.

# Post-MVP Extensions

The next architecture slices add configurable target-platform matrices and edge-equivalence graph policies, expand runtime recipes beyond the verified Bun-to-Deno subset, and add target executable version resolution. These extensions must preserve [Migration Engine](/architecture/migration-engine.md), [Agent Interface](/architecture/agent-interface.md), and [Rust-Primary Polyglot Monorepo](/decisions/rust-primary-polyglot-monorepo.md).
