---
type: Delivery Status
title: MVP Status
description: Records the completed technical MVP, its validation evidence, and explicit post-MVP boundaries.
tags: [product, mvp, status, delivery]
status: draft
stale_after: 2026-09-15
generated: { by: bahadirarda, at: 2026-08-15T19:53:59Z}
sources:
  - id: product-vision
    resource: /product/vision.md
    title: pkgshift Product Vision
  - id: repository-layout
    resource: /architecture/repository-layout.md
    title: Repository Layout
  - id: repository-source
    resource: "repository source tree at 2026-08-15"
    title: Repository source tree
---

# Technical MVP

The repository contains an executable TypeScript MVP with no third-party runtime dependencies:

- npm, pnpm, Yarn Classic, Yarn Modern, Bun, vlt, and Deno dependency-mode adapter definitions.
- Weighted package manager detection from manifest, lockfile, workspace, and configuration evidence.
- Explicit ambiguity and conflicting-evidence diagnostics.
- Repository fingerprints over migration-relevant evidence.
- Versioned Project IR across workspace packages, dependency protocols, policy shapes, linker settings, and integrations.
- Source-to-target capability analysis backed by explicit rules and authoritative documentation.
- Real `inspect`, `support`, `plan`, `apply`, `verify`, `rollback`, `explain`, `skill`, and `help` commands.
- A current-directory `pkgshift to <target>` workflow with read-only preview, interactive confirmation, hidden default state, apply, and verification orchestration.
- The `pkgshift pm to <target>` planning shortcut.
- Versioned JSON results with artifacts, diagnostics, and side-effect metadata.
- Deterministic target rendering and digest-bound file mutations.
- Opt-in atomic plan bundle persistence with digest verification.
- Exact plan and run approval tokens before mutation.
- Owner-only recovery snapshots created before the first repository write.
- A revisioned run and operation journal with stale-writer rejection and orphan-lock recovery.
- A repository-scoped transaction lock that serializes competing agents and runs.
- Shell-free package-manager execution with lifecycle scripts disabled and secret-safe process records.
- Structural verification tied to the exact run and plan.
- Repository rollback with snapshot integrity and restored-fingerprint verification.
- Project and user Agent Skill installation for Codex and Claude Code through managed copies or links.
- Conflict detection and local-modification protection for skill uninstall.
- End-to-end fixtures for guided interactive and noninteractive migrations, exact approval, success, failed installation, partial failure, tampering, verification, and rollback.
- Real pnpm-to-Bun execution fixtures covering multi-package workspaces, workspace protocols, default and named catalogs, isolated linking, trusted dependencies, exclusion patterns, local dependencies, registry configuration, and CI, container, and documentation integrations.
- Direction-matrix fixtures for every basic migration pair across the five production adapters.
- An OKF v0.2 knowledge bundle and a portable Agent Skill source.

# Delivery Gates

| Gate | State | Evidence |
| --- | --- | --- |
| Interface and knowledge contracts | MVP complete | Versioned results, exact approvals, OKF validation, and portable skill validation are automated. |
| Inspection and planning | MVP complete | Production targets render common manifest, workspace, catalog, override, linker, registry-reference, and integration semantics or fail closed. |
| Plan artifact persistence | MVP complete | Persistence is explicit, atomic, immutable, digest-checked, and repository-bound. |
| Transaction executor | MVP complete | Recovery snapshots, precondition rechecks, atomic writes, journal transitions, and partial-failure fixtures pass. |
| Verification | MVP complete | Planned digests, target selection, lockfile creation, workspace membership, and install completion are checked. |
| Rollback | MVP complete | Successful, failed, and partially applied runs restore repository files and verify the baseline fingerprint. |
| Skill installer | MVP complete | Codex and Claude Code project destinations pass copy, link, conflict, status, and protected-uninstall fixtures. |
| Production target baseline | MVP complete | npm, pnpm, Yarn Classic, Yarn Modern, and Bun produce executable plans when every observed feature has a safe implemented path. |

# Explicit Boundaries

- vlt and Deno dependency mode remain preview, planning-only targets.
- Unknown, unsupported, or adapter-unimplemented transformations block apply.
- Lossy decisions require `--accept-lossy` when the immutable plan is created.
- Literal registry credentials, sensitive manifest fields, known token formats, and private keys cannot enter persisted plan content; environment references remain supported.
- Dependency lifecycle scripts remain disabled during target installation.
- Rollback does not restore `node_modules`, global stores, downloads, or package-manager caches.
- Resolved lock-graph drift is recorded as skipped because the MVP does not persist a normalized source lock graph.
- Representative project scripts are not selected or executed automatically.
- Documentation remains `draft` until a human verifier records review evidence.

# Post-MVP Work

Add normalized source and target lock-graph extraction, configurable graph-drift policy, explicit representative-script selection, target executable version resolution, distribution packaging, and broader client-specific Agent Skill compatibility.
