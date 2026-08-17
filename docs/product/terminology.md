---
type: Glossary
title: Product Terminology
description: Defines the stable terms used across pkgshift commands, artifacts, and documentation.
tags: [product, glossary, cli]
status: draft
generated: { by: bahadirarda, at: 2026-08-17T07:24:55Z}
sources:
  - id: product-vision
    resource: /product/vision.md
    title: pkgshift Product Vision
---

# Core Terms

| Term | Meaning |
| --- | --- |
| Adapter | A package-manager-specific boundary that detects, reads, plans, renders, and verifies supported behavior. |
| Apply | The explicit operation that executes an approved plan and writes a run journal. |
| Artifact | A durable, addressable output such as an inspection report, plan, graph diff, run journal, or verification report. |
| Capability | A behavior a package manager or integration can represent, such as workspaces, catalogs, overrides, patches, or registry configuration. |
| Clean target install | Target installation performed only after every package-local pre-migration dependency-state directory has been removed or proven absent. |
| Diagnostic | A structured observation with a stable code, severity, evidence, explanation, and possible remediation. |
| Evidence | A repository fact captured during inspection, including its location and relevant fingerprint. |
| Explain | A read-only operation that expands a diagnostic code or artifact decision into human-readable reasoning. |
| Guided migration | The `pkgshift to <target>` orchestration that plans, requests exact approval, persists state, applies, and verifies without exposing staged command paths in normal use. |
| Inspect | A read-only operation that discovers repository structure, package manager evidence, and migration-relevant integrations. |
| Integration | Repository behavior outside package manager manifests, such as CI, containers, toolchain managers, or documentation commands. |
| Lock graph | A redacted normalized set of resolved packages, comparable integrity evidence, and logical dependency edges extracted from one lockfile. |
| Native importer | A target package manager's documented command or install path for translating source dependency state. |
| Plan | An immutable, reviewable set of proposed operations with preconditions, expected effects, risks, and verification requirements. |
| Project IR | The normalized project intermediate representation shared by all adapters. |
| Reachable resolution set | The lockfile resolutions connected to external manifest roots through normalized dependency topology. |
| Rollback | A compensating operation derived from a run journal that attempts to restore the pre-apply state. |
| Run | One apply attempt against one plan and repository fingerprint. |
| Source artifact residue | A source-only lockfile or package-manager configuration file that remains after the planned retirement phase. |
| Source runtime reference | Application semantics tied to the source runtime, reported during package-manager migration but never removed without a dedicated runtime transformation. |
| Runtime recipe | A registered deterministic source, script, type, or configuration transformation owned by the dedicated runtime migration boundary. |
| Side effect | Any operation that changes repository files, dependency state, caches, processes, or external systems. |
| Transaction | The bounded lifecycle from an approved plan through apply, verification, and commit or rollback status. |
| Trial | Exact-plan execution and verification in a disposable repository copy without source repository mutation or persistent source run state. |
| Verify | A read-only evaluation of persisted artifacts and repository postconditions after apply. |

# Command Language

Commands use established migration terms instead of conversational synonyms:

```text
pkgshift to bun
pkgshift to bun --trial
pkgshift inspect
pkgshift plan package-manager --to bun
pkgshift apply <plan-id>
pkgshift verify <run-id>
pkgshift explain <diagnostic-code>
pkgshift rollback <run-id>
pkgshift runtime to deno --deno-permission net
pkgshift runtime rollback <runtime-run-id>
```

The primary `pkgshift to bun` command is guided and crosses into mutation only after exact approval. The `pkgshift pm to bun` shortcut resolves only to a read-only package manager plan.

# Naming Rules

- Use `source` and `target` for package managers, not `old` and `new`.
- Use `operation` for one planned unit and `run` for an apply attempt.
- Use `warning` for a reviewable risk and `error` for a condition that blocks the current operation.
- Use `unsupported` only when the capability model has a known negative result; use `unknown` when evidence is insufficient.
- Keep diagnostic codes stable even when their messages improve.
