---
type: Validation Evidence
title: Real-World Validation Corpus
description: Records pinned upstream repositories used by automated planning, real installers, and lock graph verification.
tags: [support, validation, corpus, vlt, deno, safety]
status: draft
stale_after: 2026-09-17
generated: { by: bahadirarda, at: 2026-08-17T07:24:55Z}
sources:
  - id: hono-source
    resource: https://github.com/honojs/hono/tree/ef0739d5f6e83242ba2b64c1365c9f96738933a1
    title: Hono pinned source revision
  - id: vite-source
    resource: https://github.com/vitejs/vite/tree/dcf88bd2ad2b1a8845f9029587cc8c825e382d42
    title: Vite pinned source revision
  - id: vltpkg-source
    resource: https://github.com/vltpkg/vltpkg/tree/2e790b2fc6a19c3b6ab332cea1b8f7fbdcb3768d
    title: vlt pinned source revision
  - id: vlt-configuring
    resource: https://docs.vlt.sh/cli/configuring
    title: vlt configuration documentation
  - id: deno-migration
    resource: https://docs.deno.com/runtime/migrate/migrate_from_npm/
    title: Deno migration from npm documentation
  - id: repository-tests
    resource: "implementations/rust/pkgshift-cli/tests/e2e.rs at 2026-08-17"
    title: pkgshift real-installer acceptance tests
  - id: corpus-manifest
    resource: "validation/real-world-corpus.json at 2026-08-17"
    title: Machine-readable real-world corpus contract
---

# Purpose

Synthetic fixtures prove exact transformations. This corpus adds pinned upstream repositories to expose repository shapes, lockfile variants, installer behavior, and verification outcomes that small fixtures do not reproduce. Corpus failures are evidence: pkgshift must distinguish an unsupported capability, an external installer failure, and a post-install dependency graph mismatch instead of reporting all three as a successful migration.

# Pinned Repositories

| Repository | Revision | Detected source | Why it is included |
| --- | --- | --- | --- |
| Hono | `ef0739d5f6e83242ba2b64c1365c9f96738933a1` | Bun | Large dependency-bearing package with CI and documentation integrations. |
| Vite | `dcf88bd2ad2b1a8845f9029587cc8c825e382d42` | pnpm | Large workspace with local fixture packages, patches, lifecycle policy, package extensions, and a UTF-8 BOM manifest. |
| vlt | `2e790b2fc6a19c3b6ab332cea1b8f7fbdcb3768d` | vlt | Native vlt workspace, catalog, modifier, and configuration evidence. |

# Recorded Outcomes

The results below were produced on 2026-08-17 with the pinned pkgshift source revision preceding this document, vlt 1.0.2 on Node 22.22.0, Deno 2.9.5, and the declared source package managers.

| Case | Planning result | Execution or verification evidence |
| --- | --- | --- |
| Hono, Bun to vlt | Executable production plan, seven operations including clean target installation, no capability blocker, one `NATIVE_IMPORT_UNAVAILABLE` warning, and bounded `SOURCE_RUNTIME_REFERENCES_PRESERVED` evidence. | Isolated trial preserved the source. vlt failed while resolving a transitive npm package as a malformed SSH git URL, so pkgshift reported installer failure and did not claim verification success. |
| Hono, Bun to Deno | Executable production plan, seven operations including clean target installation, no capability blocker, one `NATIVE_IMPORT_UNAVAILABLE` warning, and bounded `SOURCE_RUNTIME_REFERENCES_PRESERVED` evidence. | Deno installation succeeded. `reachable-resolution-set-v2` tolerated three optional-only package-name absences but rejected 48 reachable source-only versions. No source resolution was topology-proven stale, so the isolated trial correctly remained failed and reported `repositoryUnchanged: true`. |
| Vite, pnpm to vlt | Blocked production plan with one native decision, two transforms, and four unsupported capabilities. | Link protocol, lifecycle allow-list, patched dependency, and package-extension semantics remain blocking. The source lock graph parses after excluding local pnpm package locators from the registry graph. |
| Vite, pnpm to Deno | Blocked production plan with two native decisions, one transform, four unsupported capabilities, and 153 unsupported local dependency specifiers. | The UTF-8 BOM fixture is parsed correctly; unsupported local fixture dependencies fail closed before execution. |
| vlt, vlt to pnpm | Executable production plan with five native capability decisions. | Source vlt configuration and the vlt v1 lock graph are accepted; no target-native importer is claimed. |
| vlt, vlt to Deno | Executable production plan after explicit lossy acceptance, with two native, one transformed, and two lossy decisions. | Catalog policy expansion is visible in the plan, and no target-native importer is claimed. |

# Controlled Real-Installer Baseline

The Rust acceptance suite creates a two-package Bun workspace with a `workspace:*` edge and a registry dependency. It migrates the same source independently to vlt 1.0.2 and Deno 2.9.5, proves that Bun-owned package-local dependency state was removed before the real target installer, extracts the generated target lockfile, and requires normalized graph equivalence. Both migrations pass. This separates adapter correctness from upstream repository or installer limitations recorded in the corpus.

# Automated Planning Gate

`validation/real-world-corpus.json` pins all three repository revisions and the expected source, target, plan status, executability, operation count, capability counts, and required diagnostic codes for six migration directions. `bun run corpus:real-world` performs shallow detached checkouts, executes read-only Rust plans, asserts the contracts, verifies that every checkout remains clean, and emits one bounded JSON summary.

The `real-world-corpus` GitHub Actions workflow runs this gate every Monday and on manual dispatch. Default branches never enter the test, a plan identifier is not treated as a stable expectation, and the workflow uploads the summary for 30 days. Installer trials remain in the separately pinned acceptance suite because planning regressions and external registry failures have different ownership.

# Parser Findings

The corpus added regression coverage for three production inputs:

- JSON and JSONC object reads accept one leading UTF-8 BOM in both engines.
- pnpm `file:`, `link:`, and `workspace:` package locators are local dependency state and do not enter the cross-manager registry resolution set.
- Deno v5 peer-context locators normalize to the base npm or JSR package version before graph comparison.
- Bun text lock edges retain exact locator-derived `name@version` targets when available, allowing V2 to distinguish obsolete same-name versions from reachable state.

# Reproduction Contract

Run the complete pinned, read-only planning corpus from the pkgshift repository root:

```bash
cargo build --package pkgshift --release --locked
bun run corpus:real-world
```

To reproduce one case manually, check out the exact revision from the table, build the current Rust CLI, and run the plan from that repository root:

```bash
pkgshift to vlt --json --no-color --non-interactive
pkgshift to deno --json --no-color --non-interactive
```

Use `--accept-lossy` only when the first plan reports reviewed lossy decisions. Use `--trial` and approve its exact plan identifier to run installers in isolation. Upstream default branches are not corpus inputs; update a pinned revision only with a new recorded outcome and documentation log entry.
