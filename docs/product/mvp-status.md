---
type: Delivery Status
title: MVP Status
description: Records the completed technical MVP, its validation evidence, and explicit post-MVP boundaries.
tags: [product, mvp, status, delivery]
status: draft
stale_after: 2026-09-15
generated: { by: bahadirarda, at: 2026-08-17T11:00:00Z}
sources:
  - id: product-vision
    resource: /product/vision.md
    title: pkgshift Product Vision
  - id: repository-layout
    resource: /architecture/repository-layout.md
    title: Repository Layout
  - id: repository-source
    resource: "repository source tree at 2026-08-17"
    title: Repository source tree
---

# Technical MVP

The repository contains a Rust-primary polyglot MVP. Rust owns the primary migration engine and CLI; the dependency-free TypeScript runtime remains executable as a compatibility and parity reference. The shared product scope includes:

- npm, pnpm, Yarn Classic, Yarn Modern, Bun, vlt, and Deno dependency-mode adapter definitions.
- Weighted package manager detection from manifest, lockfile, workspace, and configuration evidence.
- Explicit ambiguity and conflicting-evidence diagnostics.
- Repository fingerprints over migration-relevant evidence.
- Normalized source lock graphs bound to immutable plans and independently extracted target graphs.
- Blocking `reachable-resolution-set-v2` verification with proven-unreachable pruning, optional-only platform absence handling, and explicit `resolution-set-v1` fallback for topology-limited formats.
- Target-native importer selection for verified pnpm, Bun, Yarn Classic, Yarn Modern, and npm migration paths.
- Versioned Project IR across workspace packages, dependency protocols, policy shapes, linker settings, and integrations.
- Source-to-target capability analysis backed by explicit rules and authoritative documentation.
- Real `inspect`, `support`, `plan`, `apply`, `verify`, `rollback`, `explain`, `skill`, and `help` commands.
- A current-directory `pkgshift to <target>` workflow with read-only preview, interactive confirmation, hidden default state, apply, and verification orchestration.
- An approved `--trial` workflow that executes the exact plan and verification in a disposable copy without persisting source repository state.
- An aggregate `compare` workflow that binds two or more normalized target plans to one approval and runs each executable candidate in an independent disposable copy without ranking them.
- The `pkgshift pm to <target>` planning shortcut.
- A separate `pkgshift runtime to deno` workflow with read-only preview, explicit Deno permission review, exact approval, deterministic recipes, private recovery state, residue verification, and runtime rollback.
- Versioned JSON results with artifacts, diagnostics, and side-effect metadata.
- Deterministic target rendering and digest-bound file mutations.
- Context-aware repository integration rendering for package scripts, registered CI command fields, setup actions, cache lockfile references, Docker and automation commands, Markdown command spans, devcontainer lifecycle strings, Volta and engine metadata, `.tool-versions`, and mise pins.
- Bidirectional vlt rendering for workspaces, catalogs, graph modifiers, public registry configuration, integrations, and package manager pins.
- Deno dependency-mode rendering for workspaces, overrides, catalog expansion, isolated linking, preserved runtime configuration, integrations, and package manager pins.
- Rust-primary npm and pnpm override plus Yarn resolution rendering for the deterministic selector subset, including policy detected in `pnpm-workspace.yaml`.
- Rust-primary Plug and Play and isolated linker translation across pnpm, Yarn Modern, Bun, npm, and Yarn Classic target layouts.
- Secret-safe `.npmrc` translation into Yarn Modern registry, scope, authentication-policy, and environment-token configuration.
- Bidirectional Bun, pnpm, and Yarn Modern lifecycle allow-list rendering using current `trustedDependencies`, `allowBuilds`, and `dependenciesMeta` contracts.
- Shared-schema `packageExtensions` rendering among npm, pnpm, and Yarn Modern.
- Exact-version, text-only patch conversion among Yarn Modern, pnpm, and Bun for direct dependencies and transitive Yarn resolutions.
- Repository fingerprints and exact approval preconditions that include project patch files regardless of their directory.
- Opt-in atomic plan bundle persistence with digest verification.
- Exact plan and run approval tokens before mutation.
- Owner-only recovery snapshots created before the first repository write.
- A revisioned run and operation journal with stale-writer rejection and orphan-lock recovery.
- A repository-scoped transaction lock that serializes competing agents and runs.
- Shell-free package-manager execution with lifecycle scripts disabled and secret-safe process records.
- Explicit root representative-script selection with immutable target argv, bounded shell-free execution, withheld output, and journal-backed verification that never reruns scripts.
- Structural verification tied to the exact run and plan.
- Repository rollback with snapshot integrity and restored-fingerprint verification.
- Project and user Agent Skill installation for Codex and Claude Code through managed copies or links.
- Conflict detection and local-modification protection for skill uninstall.
- End-to-end fixtures for guided interactive and noninteractive migrations, exact approval, success, failed installation, partial failure, tampering, verification, and rollback.
- Real pnpm-to-Bun execution fixtures covering multi-package workspaces, workspace protocols, default and named catalogs, isolated linking, trusted dependencies, exclusion patterns, local dependencies, registry configuration, and CI, container, and documentation integrations.
- Rust pnpm-to-Bun execution coverage that migrates and rolls back GitHub Actions setup and commands, cache lockfile references, package scripts, Dockerfile, Makefile, devcontainer, Volta, engines, `.tool-versions`, and mise state in one transaction.
- Direction-matrix fixtures for every basic migration pair across the seven production adapters.
- An OKF v0.2 knowledge bundle and a portable Agent Skill source.
- A pinned Rust 1.97.1 workspace with separate core and CLI crates.
- Digest-verified Rust plan and run envelopes, repository-scoped locking, byte-level snapshots, atomic mutations, installer output withholding, structural verification, and restored-fingerprint rollback.
- Checksum-verifying Linux, macOS, and Windows release installers with metadata validation, staged binary and portable-Skill replacement, smoke checks, and previous-version restoration on failure.
- Rust subprocess fixtures for pnpm-to-Bun-to-rollback, npm-to-pnpm, npm-to-Yarn Modern registry and lifecycle conversion, npm-to-vlt, and pnpm-to-Deno, plus 42-direction basic planning coverage.
- Rust subprocess fixtures for isolated trial, native importer ordering, successful graph proof, intentional graph drift, and source repository preservation.
- Deterministic package-local dependency-state cleanup before target installation, with fail-closed path validation, cleanup journal evidence, and explicit source-artifact residue verification.
- Bounded Bun runtime-reference scanning for type dependencies, manifest scripts, global API use, and `bun:` imports, reported without conflating runtime migration with package-manager cleanup.
- Deterministic Bun-to-Deno recipes for official Hono `Bun.serve` handlers, `bun:test`, Bun text and JSON reads, direct runtime scripts, and Bun type residue, with SQLite and other unsupported APIs blocked instead of delegated to a model.
- A real-installer Rust acceptance run for a multi-package pnpm workspace migrated to Bun and rolled back to its original fingerprint.
- Real Bun 1.3.14 acceptance runs for dependency-bearing npm-to-Bun trial, apply, graph proof, and rollback.
- Real vlt 1.0.2 and Deno 2.9.5 acceptance runs for dependency-bearing multi-package Bun workspaces, workspace protocols, target installation, and graph proof.
- A real Hono Bun-to-Deno runtime acceptance run that applies the approved source plan, installs with pinned Deno 2.9.5, type-checks the migrated server, and passes the migrated test suite.
- A machine-readable pinned upstream corpus covering executable Bun-to-vlt and Bun-to-Deno plans, vlt-to-pnpm and accepted-lossy vlt-to-Deno plans, Vite capability blockers, clean-checkout enforcement, weekly CI, external vlt installer failure, and strict Deno post-install graph rejection without source writes.
- Sibling Rust and TypeScript engines under `implementations/`, with shared root orchestration for both implementations.

# Delivery Gates

| Gate | State | Evidence |
| --- | --- | --- |
| Interface and knowledge contracts | MVP complete | Versioned results, exact approvals, OKF validation, and portable skill validation are automated. |
| Inspection and planning | MVP complete | Production targets render common manifest, workspace, catalog, override, linker, registry-reference, and integration semantics or fail closed. |
| Plan artifact persistence | MVP complete | Persistence is explicit, atomic, immutable, digest-checked, and repository-bound. |
| Transaction executor | MVP complete | Recovery snapshots, precondition rechecks, atomic writes, journal transitions, and partial-failure fixtures pass. |
| Verification | MVP complete | Planned digests, clean-install evidence, source-artifact residue, target selection, lockfile behavior, workspace membership, install completion, and reachable resolution-set parity are checked. |
| Isolated trial | MVP complete | Exact approval executes the accepted plan in a disposable copy and reports repository preservation plus nested verification. |
| Multi-target comparison | MVP complete | Aggregate approval binds candidate plan identities; independent disposable trials report passed, failed, and blocked evidence while preserving the source repository. |
| Bun-to-Deno runtime recipes | Initial production subset complete | Explicit permissions, exact approval, source-content redaction, digest-bound apply, residue verification, rollback, and a real Hono Deno run pass. |
| Rollback | MVP complete | Successful, failed, and partially applied runs restore repository files and verify the baseline fingerprint. |
| Skill installer | MVP complete | Codex and Claude Code project destinations pass copy, link, conflict, status, and protected-uninstall fixtures. |
| Native distribution | MVP complete | Unix fixture archives pass install, stale-data cleanup, and smoke-failure rollback; Windows fixtures additionally prove checksum rejection on `windows-2025`. |
| Production target baseline | MVP complete | npm, pnpm, Yarn Classic, Yarn Modern, Bun, vlt, and Deno dependency mode produce executable plans when every observed feature has a safe implemented path. |
| Rust primary path | MVP complete | Inspect, plan, exact approval, apply, verify, and rollback pass subprocess and real-installer acceptance coverage. |
| Polyglot workspace | MVP complete | Cargo crates and the TypeScript reference run from isolated workspace boundaries under one CI contract. |
| Advanced Rust renderer parity | Production parity complete | Override, resolution, package-extension, exact text-patch, linker, registry, lifecycle, vlt, and Deno rendering pass Rust and TypeScript parity fixtures; unsupported and manual-only transformations remain fail-closed. |

# Explicit Boundaries

- vlt and Deno dependency mode are production targets only inside their documented deterministic subsets; unsupported protocols, lifecycle policy, patching, package extensions, or configuration remain blocking.
- Unknown, unsupported, or adapter-unimplemented transformations block apply.
- Lossy decisions require `--accept-lossy` when the immutable plan is created.
- Literal registry credentials, sensitive manifest fields, known token formats, and private keys cannot enter persisted plan content; environment references remain supported.
- Dependency lifecycle scripts remain disabled during target installation.
- Rollback does not restore pre-migration `node_modules`, global stores, downloads, or package-manager caches; successful migration removes package-local source dependency state before target installation and never deletes global stores.
- Binary `bun.lockb` graph extraction fails closed until the repository converts it to the current text `bun.lock` format.
- Reachable-resolution proof does not yet make dependency edge shape blocking because peer placement, hoisting, and deduplication representations differ between managers.
- Representative project scripts are not selected automatically; only exact root script names supplied with `--verify-script` are executed, and their repository side effects remain outside rollback snapshots.
- Artifact explanation is read-only and covers registered diagnostics plus persisted package-manager and runtime plans, runs, and verification reports; ephemeral trial and comparison reports remain available only in their originating command result.
- Unsupported and manual-only transformations emit blocking diagnostics instead of delegating edits to a model.
- Runtime migration is a separate Rust-only command and currently targets Deno from Bun; package-manager selection, lockfiles, installation, advanced Bun APIs, `bunfig.toml`, shell behavior, macros, routes, and WebSockets remain outside its deterministic recipe subset.
- Registry tokens must use `${NAME}` references for Yarn Modern translation; literal credentials and unrecognized `.npmrc` settings fail closed without entering plan artifacts.
- pnpm output uses the current `allowBuilds` map while inspection continues to accept legacy `onlyBuiltDependencies` input.
- Yarn per-dependency build denials outside global allow-list mode remain blocking because other targets cannot preserve them safely.
- Override nesting beyond one deterministic parent-child selector level and Yarn resolution selectors that cannot map to a bare npm package remain blocking.
- Patch conversion requires one exact `name@version`, one project-relative `.patch` file, and a text-only git unified diff; ranges, binary patches, missing files, and multiple or parameterized Yarn patch sources remain blocking.
- Documentation remains `draft` until a human verifier records review evidence.

# Post-MVP Work

Expand patch conversion beyond the exact text-only subset, grow runtime recipes beyond Bun-to-Deno safe shapes, add configurable target-platform matrices and edge-equivalence policies, and add target executable version resolution.
