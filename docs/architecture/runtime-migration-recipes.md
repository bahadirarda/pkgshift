---
type: Execution Architecture
title: Bun to Deno Runtime Recipes
description: Defines the deterministic, permission-aware, transactional boundary for dedicated Bun-to-Deno application runtime migration.
tags: [architecture, runtime, bun, deno, recipes, verification]
status: draft
generated: { by: bahadirarda, at: 2026-08-17T11:44:43Z }
sources:
  - id: deno-bun-migration
    resource: https://docs.deno.com/runtime/migrate/migrate_from_bun/
    title: Migrate from Bun
  - id: deno-security
    resource: https://docs.deno.com/runtime/fundamentals/security/
    title: Deno security and permissions
  - id: hono-deno
    resource: https://hono.dev/docs/getting-started/deno
    title: Hono on Deno
  - id: transactional-decision
    resource: /decisions/transactional-migrations.md
    title: Transactional Migrations
---

# Separate Runtime Boundary

`pkgshift runtime to deno` is a dedicated application-runtime migration. It does not run the package-manager adapter pipeline, change the `packageManager` field, retire `bun.lock`, install dependencies, or imply that Deno dependency mode has been selected. The ordinary `pkgshift to <target>` path continues to preserve and report application runtime references.

The first runtime call is read-only. It fingerprints bounded source, manifest, TypeScript configuration, and Bun runtime configuration evidence; creates exact before and after digests; returns a content-redacted `runtime-migration-plan`; and requires approval for `runtime_plan_<digest>`. After approval, only the redacted plan summary, journal, verification, and recovery snapshots are persisted under `.pkgshift/state/runtime`.

# Deterministic Recipe Set

The initial Bun-to-Deno set applies only these reviewed transformations:

- Official Hono fetch-handler `Bun.serve` calls, or one-argument method handlers, containing `fetch` and an optional `port` become `Deno.serve` calls.
- Supported named `bun:test` imports become `node:test`; `expect` becomes `jsr:@std/expect`.
- `Bun.file(...).text()` and directly awaited `.json()` reads become Deno text-file APIs.
- Direct Bun entrypoint, hot-reload, and test scripts become explicit `deno run`, `deno run --watch`, or `deno test` commands.
- `@types/bun`, `bun-types`, and matching TypeScript `types` entries are removed after supported source recipes are available.

Recipe parsing ignores comments, respects strings and nested delimiters, and fails closed outside the registered shapes. `Bun.serve` routes, WebSockets, lifecycle hooks, two-argument handlers, unsupported test APIs, SQLite imports, Bun shell APIs, macros, `HTMLRewriter`, `bunfig.toml`, mixed shell scripts, oversized inputs, symbolic-link source boundaries, and unknown Bun globals produce blocking diagnostics. A coding model never supplies a missing rewrite.

# Permission Contract

Deno is secure by default, so pkgshift never inserts `-A`. Every supported recipe declares the narrow permission it requires. The user or calling agent must add repeatable `--deno-permission <name>` values, and those sorted values participate in the immutable plan identity. The first set accepts `read`, `write`, `net`, `env`, `run`, `sys`, `ffi`, and `hrtime`; `Bun.serve` requires `net`, and Bun file reads require `read`.

Directly transformed scripts receive the reviewed Deno permission flags. Missing permissions block execution with `DENO_PERMISSION_REQUIRED` instead of producing a command that would prompt or fail later.

# Transaction and Verification

Approved runtime apply shares the repository-scoped transaction lock, rechecks the runtime fingerprint, writes private byte-level snapshots, validates every mutation precondition, and applies files atomically. Transformed source content is never copied into the persisted plan or run journal. Recovery snapshots contain only the original affected bytes and use owner-only permissions; JSON result artifacts expose mutation summaries and digests.

Verification requires every planned after-digest and zero Bun runtime references inside the supported inspection boundary. `pkgshift runtime rollback <runtime-run-id> --approve <runtime-run-id>` validates snapshot digests, restores original file modes and bytes, and requires the baseline runtime fingerprint to match. Runtime apply executes no project process; a caller may run reviewed Deno checks or tests separately after migration.
