---
type: Storage Architecture
title: Plan Artifact Store
description: Defines approval-gated atomic persistence and integrity verification for immutable migration plan bundles.
tags: [architecture, artifacts, persistence, integrity]
status: draft
generated: { by: bahadirarda, at: 2026-08-15T19:53:59Z}
sources:
  - id: transactional-decision
    resource: /decisions/transactional-migrations.md
    title: Transactional Migrations
  - id: project-ir
    resource: /architecture/project-ir.md
    title: Project IR
  - id: capability-engine
    resource: /architecture/capability-engine.md
    title: Capability Engine
---

# Plan Bundle

One immutable bundle stores the mutually bound migration plan, Project IR, and capability analysis. Persistence rejects a bundle when their identifiers disagree.

# Persistence Boundary

Planning remains repository-read-only. The first guided call emits the complete plan bundle only through the result. After exact approval, guided execution selects `.pkgshift/state` and persists the bundle before mutation.

Advanced staged planning persists only when `--state-dir <path>` is explicitly supplied:

```text
pkgshift plan package-manager --to bun --state-dir <path> --json
```

This keeps inspection, preview, and unapproved planning free of persistent writes while allowing the ordinary approved workflow to hide storage plumbing.

# Storage Layout

```text
<state-dir>/
  repositories/
    repo_<root-hash>/
      plans/
        plan_<content-hash>.json
```

The repository key derives from the resolved root without placing the root path in the directory name. The plan identifier remains derived from normalized semantic content.

# Integrity

Each envelope records schema version, identity, media type, creation time, digest, and content. Reads recompute the digest and reject malformed, relocated, or modified artifacts. Saving an existing identifier is idempotent only when the verified digest matches.

# Atomicity

Writes create a private temporary file, flush it, and atomically rename it into place. State directories use owner-only directory permissions and files use owner-only file permissions where the platform honors POSIX modes.

Artifact persistence does not make a plan executable or approve it. Apply separately requires an executable production plan, exact plan approval, a matching repository fingerprint, and healthy journal and snapshot storage.

# Run Artifacts

Apply stores the run journal, recovery snapshot manifest, private backup files, and redacted process execution report under `<state-dir>/runs/<run-id>/`. Verify adds an integrity-checked verification report. Explain loads these artifacts without changing them.
