---
type: Workflow
title: Isolated Migration Trial
description: Defines how to execute an approved migration in a disposable sandbox without changing or persisting state in the source repository.
tags: [workflow, trial, sandbox, verification, agents]
status: draft
generated: { by: bahadirarda, at: 2026-08-16T19:53:18Z}
sources:
  - id: agent-interface
    resource: /architecture/agent-interface.md
    title: Agent Interface
  - id: lock-graph-proof
    resource: /architecture/lock-graph-proof.md
    title: Lock Graph Proof
  - id: migration-workflow
    resource: /workflows/pkgshift.md
    title: Package Manager Migration
---

# Purpose

An isolated trial answers a narrower question than dry-run: can the exact deterministic plan execute and verify with the available target package manager while the selected source repository remains unchanged?

Dry-run executes no target process. Trial executes importer and installer processes in a disposable copy. Apply executes the approved plan in the source repository and creates persistent recovery state.

# Human Workflow

From the repository root:

```text
pkgshift to bun --trial
```

The first call remains read-only and presents the exact plan identifier. Approval authorizes process execution in the isolated sandbox, not repository mutation. A successful trial returns `status: completed`, a `trial-report` artifact, `repositoryUnchanged: true`, and no `runId`.

Trial approval does not authorize apply. To perform the migration afterward, run the normal `pkgshift to bun` preview and approve its repository-write next action.

# Agent Workflow

```text
pkgshift to bun --trial --json --no-color --non-interactive
```

Exit code `7` is the expected approval boundary. Present that the next action has `sideEffect: process-execution` and will use package manager caches and network access as needed. After approval, execute the returned `nextActions[0].argv` unchanged.

Treat the trial as successful only when:

- The command returns `status: completed`.
- The trial report returns `status: passed`.
- `repositoryUnchanged` is true.
- The nested verification report passes, including lock graph proof when a source graph exists.
- No blocking diagnostic remains.

# Isolation Boundary

The Rust CLI copies regular repository files into a new operating-system temporary directory and executes the same accepted plan there. The copy excludes `.git`, `.pkgshift`, `node_modules`, and Rust `target` directories at every depth, matching the inspector's repository walk boundary. Symbolic links are rejected instead of followed. The sandbox owns its own private `.pkgshift/state` and is removed automatically when the trial returns.

The source repository is inspected immediately before and after the sandbox run. Its migration-relevant fingerprint must remain unchanged. The command does not persist a plan, run journal, snapshot, target lockfile, or default state directory in the source repository.

# External Effects

Trial is repository-isolated, not machine-isolated. Target package managers may read network configuration, download registry metadata, and update global or user-scoped caches. Lifecycle scripts remain disabled and processes run without a shell. Process output is withheld from Rust artifacts.

Do not describe a trial as network-free, cache-free, or a security sandbox for untrusted package manager executables. Its contract is deterministic migration execution away from the source repository.
