# Documentation Update Log

## 2026-08-16

* **Architecture**: Established a Rust-primary polyglot monorepo with `pkgshift-core`, `pkgshift-cli`, and an isolated TypeScript compatibility reference.
* **Decision**: Added the accepted Rust-primary monorepo decision, parity-gate policy, and shared schema boundary.
* **Implementation**: Ported weighted detection, Project IR, capability analysis, deterministic planning, exact approval, state persistence, apply, verification, and rollback into Rust.
* **Safety**: Added digest-verified plan and run envelopes, repository-scoped locking with dead-writer recovery on Linux, byte-level snapshots, atomic mutations, lifecycle-script suppression, and withheld installer output.
* **Verification**: Added Rust coverage for all 20 basic production planning directions and two independent subprocess migrations, including pnpm-to-Bun rollback.
* **Verification**: Ran a live multi-package pnpm-to-Bun migration with Bun 1.3.14, verified the generated dependency state, and restored the original repository fingerprint.
* **Tooling**: Moved the TypeScript engine into `packages/pkgshift-ts`, added root polyglot orchestration, pinned Rust 1.97.1, and expanded CI gates for rustfmt, Clippy, Cargo tests, Bun tests, OKF, and production builds.

## 2026-08-15

* **Branding**: Renamed the product, executable, state boundary, media types, diagnostics, distribution bundle, and Agent Skill to the lowercase `pkgshift` identity.
* **Presentation**: Added an original editorial hero asset and rebuilt the repository README as a product-oriented technical landing page.
* **Automation**: Added a least-privilege GitHub Actions workflow for type checking, tests, OKF and skill validation, English-only validation, and production build output.
* **Interface**: Added the current-directory `pkgshift to <target>` workflow with read-only preview, interactive confirmation, exact noninteractive approval, hidden default state, apply, and verification orchestration.
* **Verification**: Added unit and end-to-end coverage for unapproved, declined, agent-approved, and interactively approved guided migrations.
* **Verification**: Added two real pnpm-to-Bun migration fixtures covering workspace catalogs, isolated linking, trusted dependencies, exclusion patterns, local dependencies, repository integrations, successful installation, structural verification, and rollback.
* **Governance**: Standardized generated provenance on the `bahadirarda` project author identifier and prohibited agent, tool, model, or version names in knowledge metadata.
* **Initialization**: Established the OKF v0.2 knowledge bundle.
* **Creation**: Added the product vision, terminology, architecture, support model, decisions, and package manager migration workflow.
* **Creation**: Added the portable package manager migration Agent Skill outside the OKF bundle.
* **Implementation**: Added the dependency-free read-only CLI foundation with detection, support discovery, planning, explanations, and structured output.
* **Verification**: Added fixtures for deterministic planning, package manager ambiguity, Yarn generation detection, CLI shortcuts, and mutation boundaries.
* **Governance**: Added durable OKF, internal-link, Agent Skill, and English-only validation.
* **Implementation**: Added versioned Project IR extraction across workspace manifests, dependency protocols, policy shapes, linker settings, and redacted registry evidence.
* **Implementation**: Added source-to-target capability decisions with native, transform, lossy, unsupported, unknown, and not-applicable classifications.
* **Implementation**: Added opt-in atomic plan persistence, integrity verification, and a revisioned run journal state machine.
* **Verification**: Expanded the suite to cover IR semantics, capability blockers, secret-safe fingerprints, artifact tampering, journal transitions, and stale revision conflicts.
* **Implementation**: Added deterministic target renderers with exact before and after digests for manifests, workspace configuration, policy translation, registry references, and recognized integration commands.
* **Safety**: Added lossy-plan acceptance, exact approval tokens, stale-plan rejection, owner-only recovery snapshots, lifecycle-script suppression, shell-free target execution, and secret-safe persisted process records.
* **Implementation**: Enabled journaled apply, structural verification, successful and failed run recovery, restored-fingerprint checks, and artifact or run explanation.
* **Implementation**: Added Codex and Claude Code Agent Skill installation for project and user scopes with copy, link, status, doctor, conflict, and protected-uninstall behavior.
* **Verification**: Added strict TypeScript checking and end-to-end fixtures for plan, apply, verify, rollback, failed install, partial failure, orphaned locks, snapshot tampering, and skill installation.
* **Documentation**: Marked the technical MVP complete, documented explicit graph-diff and external-state boundaries, and added the recovery and verification architecture concept.
* **Safety**: Rejected mutation and recovery paths that traverse symbolic-link directories outside the selected project root.
* **Research**: Refreshed exact adapter baselines against official package registries and release feeds, and documented the executable-version boundary.
* **Safety**: Added a recoverable repository-scoped transaction lock so concurrent agents cannot race apply, verification, or rollback.
* **Safety**: Changed unsupported workspace glob syntax from silent partial matching to a blocking diagnostic.
* **Safety**: Blocked unsupported workspace protocol and npm configuration variants instead of silently reducing them during target rendering.
* **Safety**: Expanded persisted-plan secret checks to sensitive manifest fields, known token formats, and private-key material.
* **Safety**: Confined Agent Skill destinations to their declared project or user roots, including parent-directory symlink checks.
* **Verification**: Added the complete 20-direction basic planning matrix across npm, pnpm, both Yarn families, and Bun.
* **Tooling**: Added reproducible build and full validation scripts for the MVP handoff.
* **Research**: Corrected the Yarn Modern lifecycle-suppression command to the documented `--mode=skip-build` contract.
