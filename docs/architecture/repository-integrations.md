---
type: Integration Architecture
title: Repository Integration Migration
description: Defines deterministic command, setup, cache, toolchain, container, documentation, and development-environment migration outside dependency manifests.
tags: [architecture, integrations, ci, containers, toolchains, verification]
status: draft
generated: { by: bahadirarda, at: 2026-08-17T10:40:00Z }
sources:
  - id: github-dependency-caching
    resource: https://docs.github.com/en/actions/reference/workflows-and-actions/dependency-caching
    title: GitHub Actions dependency caching reference
  - id: pnpm-action-setup
    resource: https://github.com/pnpm/action-setup
    title: pnpm action-setup
  - id: bun-install-ci
    resource: https://bun.sh/docs/pm/cli/install
    title: Bun install and CI
  - id: deno-task
    resource: https://docs.deno.com/runtime/reference/cli/task/
    title: Deno task command
---

# Repository Integration Migration

Package manager state extends beyond manifests and lockfiles. Repository integrations select executables, cache dependency state, pin tool versions, and repeat installation or script commands. pkgshift inspects these files as accepted repository evidence, includes them in the repository fingerprint, and emits digest-bound mutations in the immutable plan.

# Registered Surfaces

The Rust primary engine recognizes:

- GitHub Actions, GitLab CI, Azure Pipelines, Docker Compose, and Taskfile YAML command fields.
- GitHub setup actions for pnpm, Bun, and Deno, plus setup-node cache inputs for npm, pnpm, and Yarn.
- Dockerfile and Containerfile `RUN`, `CMD`, and `ENTRYPOINT` command contexts.
- Make, Just, Jenkins, and other registered automation recipes.
- Shell command fences and inline command spans in Markdown without rewriting ordinary prose.
- Devcontainer lifecycle command strings.
- Root package scripts, `packageManager`, Volta, engines, and npm-compatible `devEngines.packageManager` pins.
- `.tool-versions` and mise `[tools]` package-manager pins.
- Source lockfile names used by registered cache and automation contexts.

# Command Boundary

Command translation occurs only at a shell command position. Text passed to `echo`, prose mentioning a package manager, quoted data, and unregistered syntax remain unchanged. npm, pnpm, Yarn, and vlt script shorthands become an explicit target `run` or Deno `task` invocation. Bun and Deno runtime commands are not inferred to be package-manager commands; for example, `bun test` remains a runtime reference while `bun run test` is eligible for package-script translation.

The command renderer maps only registered install, script, dependency mutation, execution, and maintenance verbs. A source command that remains in an executable context because its target semantics are unimplemented produces `INTEGRATION_COMMAND_AMBIGUOUS` and blocks apply.

# Setup and Cache Boundary

Registered dedicated setup actions translate only when both source and target have an explicit action contract. A source setup action without a registered target equivalent blocks with `INTEGRATION_SETUP_ACTION_UNSUPPORTED`.

GitHub setup-node cache values translate among npm, pnpm, and Yarn because the action provides those cache contracts. Bun, Deno, and vlt require a separately reviewed cache step; preserving a setup-node source cache would be misleading, so `INTEGRATION_CACHE_UNSUPPORTED` blocks the plan. Lockfile references in registered CI and automation files follow the selected target lockfile.

# Toolchain Boundary

Node runtime pins remain unchanged during package-manager migration. Package-manager entries in Volta, engines, `devEngines`, `.tool-versions`, and mise are updated when the target has a registered representation. If a toolchain manager cannot represent the target, pkgshift removes the stale source package-manager pin, retains the canonical `packageManager` field, and emits a non-blocking diagnostic requesting a reviewed target installation mechanism.

# Safety and Verification

Integration inspection and planning are read-only. Every integration write carries exact before and after digests, participates in snapshots and rollback, and is verified by the planned-digest check. Unsupported object-shaped devcontainer commands, cache modes, setup actions, shell forms, or executable residues fail closed rather than receiving a textual best guess.

Integration command translation does not imply runtime migration. Runtime-specific imports, globals, images, test runners, or application APIs remain preserved evidence for a separate runtime plan.
