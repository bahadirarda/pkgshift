---
type: Product Vision
title: pkgshift Product Vision
description: Defines an agent-first product for safe and explainable JavaScript package manager migrations.
tags: [product, migration, package-management, agents]
status: draft
generated: { by: bahadirarda, at: 2026-08-17T00:38:12Z}
sources:
  - id: founding-discussion
    resource: "founding product discussion on 2026-08-15"
    title: Founding product discussion
    author: human:project-founder
---

# Problem

Changing a JavaScript package manager is not a lockfile replacement. A real migration may affect manifests, workspace declarations, dependency protocols, overrides, catalogs, patches, lifecycle scripts, registry settings, runtime pins, CI commands, container builds, caches, and contributor instructions.

Existing automation often handles only the happy path. Coding agents can reason across a repository, but they need a deterministic tool that exposes evidence, boundaries, and reversible actions instead of inviting ad hoc edits.

# Product Promise

pkgshift converts repository evidence into a transactional migration:[^founding-discussion]

1. Inspect the project without changing it.
2. Produce a deterministic and reviewable plan.
3. Require approval at the mutation boundary.
4. Apply the approved plan with a journal.
5. Verify install, graph, scripts, and repository integrations.
6. Explain every diagnostic and support rollback when verification fails.

The command-line interface is optimized for coding agents while remaining comfortable for humans. Structured JSON is a first-class interface, not a rendering of human-readable output.

# Primary Users

- Coding agents operating through Codex, Claude Code, Gemini CLI, Cursor, GitHub Copilot, Windsurf, and similar tools.
- Maintainers modernizing an application or monorepo.
- Platform teams standardizing package manager policy across repositories.
- Migration authors extending support through adapters and capability rules.

# MVP Scope

The production adapter set targets npm, pnpm, Yarn Classic, Yarn Modern, Bun, vlt, and Deno's npm-compatible dependency mode. Each adapter is executable only inside its documented deterministic capability subset. Runtime conversion is never an implicit package-manager side effect; a separate Rust-owned Bun-to-Deno command applies only registered deterministic recipes under its own approval and recovery boundary.

The MVP covers:

- Repository inspection and package manager detection.
- A shared project intermediate representation.
- Source and target capability comparison.
- Reviewable plan artifacts with preconditions and risk annotations.
- Transactional file changes and a run journal.
- Native lockfile import where supported, install completion, planned digest, target lockfile behavior, workspace, integration, and normalized resolution-set verification.
- Approved isolated migration trials that execute outside the source repository.
- Structured diagnostics, explanations, and rollback.
- Project and user installation of a portable Agent Skill.
- Dedicated, permission-aware Bun-to-Deno runtime recipes for verified source and script shapes.
- Explicitly selected, bounded representative root-script execution with withheld output and journal-backed verification.
- Non-ranking readiness assessment for one target or every production adapter without creating a plan.

Configurable target-platform matrices and strict edge-equivalence policies extend the MVP's blocking reachable-resolution proof.

# Non-goals

- Rewriting application source merely to make a target package manager succeed.
- Migrating the JavaScript runtime, framework, or build system as an implicit package-manager side effect.
- Silently resolving dependency conflicts with unverifiable guesses.
- Guaranteeing byte-identical dependency trees when package manager semantics differ.
- Treating repository cleanliness as proof that a migration is safe.

# Success Criteria

- An agent can discover the current state and produce a plan with one read-only command sequence.
- The same repository state and options produce the same normalized plan.
- No mutation occurs without an explicit apply operation referencing a concrete plan.
- Verification identifies blocking structural and resolution drift; graph comparison is skipped only when no source lockfile existed.
- Every failure returns a stable diagnostic code and an actionable next step.
- A failed or rejected run leaves an inspectable journal and, where possible, a tested rollback path.

# Related Concepts

- [Terminology](/product/terminology.md)
- [Migration Engine](/architecture/migration-engine.md)
- [Agent Interface](/architecture/agent-interface.md)
- [Package Manager Migration Workflow](/workflows/pkgshift.md)
- [Bun to Deno Runtime Migration](/workflows/runtime-migration.md)

[^founding-discussion]: Founding product discussion
