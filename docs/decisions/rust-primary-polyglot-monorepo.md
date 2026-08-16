---
type: Architecture Decision
title: Rust-Primary Polyglot Monorepo
description: Establishes Rust as the primary pkgshift engine while retaining the TypeScript implementation as an executable parity reference in one repository.
tags: [decision, rust, typescript, monorepo, parity]
status: draft
decision_status: accepted
generated: { by: bahadirarda, at: 2026-08-16T16:00:00Z}
sources:
  - id: migration-engine
    resource: /architecture/migration-engine.md
    title: Migration Engine
  - id: repository-source
    resource: "repository source tree at 2026-08-16"
    title: Repository source tree
  - id: rust-release
    resource: https://blog.rust-lang.org/2026/07/16/Rust-1.97.1/
    title: Announcing Rust 1.97.1
---

# Context

pkgshift requires deterministic filesystem behavior, a small distributable executable, explicit state transitions, and a reliable safety boundary for coding agents. The existing TypeScript MVP already defines the product behavior and covers advanced capability fixtures. Replacing it in place would remove the working oracle before the new implementation proves parity.

# Decision

Use one polyglot monorepo with two explicit implementation boundaries:

- `crates/pkgshift-core` is the primary domain engine.
- `crates/pkgshift-cli` is the primary executable and agent-facing command surface.
- `packages/pkgshift-ts` is an executable compatibility and parity reference.
- `docs`, `skills`, schema terminology, support baselines, and safety rules remain shared product assets.

Rust 1.97.1 is pinned for reproducible formatting, linting, testing, and release builds.[^rust-release] The port advances by behavioral slices rather than file-for-file translation. A slice crosses the parity gate only after deterministic planning, exact approval, apply, verification, recovery, and failure behavior are covered at the appropriate level.

The engines may have different internal types and storage layouts. Agent-visible JSON terms, command meaning, diagnostic intent, approval boundaries, and side-effect classifications must remain compatible within schema version `1.0`.

# Consequences

- The repository root orchestrates Cargo and Bun workspaces without containing a third engine.
- The Rust CLI can ship independently of Bun once documentation validation and the reference suite are separated from release packaging.
- TypeScript remains runnable until advanced renderers and ancillary command ownership are resolved.
- Rust must fail closed when a capability renderer has not crossed the parity gate.
- Cross-engine fixtures are preferred over source-shape similarity as evidence of port correctness.
- Neither engine asks an AI model to infer or author migration edits.

# Rejected Alternatives

## Destructive in-place rewrite

Removing the TypeScript engine before Rust parity would discard the strongest executable specification and make regressions harder to localize.

## Independent repositories

Separate repositories would allow schema, package-manager baselines, Agent Skill guidance, and safety decisions to drift during the port.

## Thin Rust launcher

A Rust binary that only invokes the TypeScript process would not improve the deterministic execution boundary or create an independent runtime.

# Related Concepts

- [Repository Layout](/architecture/repository-layout.md)
- [Migration Engine](/architecture/migration-engine.md)
- [MVP Status](/product/mvp-status.md)

[^rust-release]: Rust 1.97.1 release announcement
