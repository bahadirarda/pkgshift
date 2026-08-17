---
type: State Machine
title: Run Journal
description: Defines revisioned run and operation state transitions for apply, verification, failure, and rollback.
tags: [architecture, journal, state-machine, rollback]
status: draft
generated: { by: bahadirarda, at: 2026-08-15T19:53:59Z}
sources:
  - id: transactional-decision
    resource: /decisions/transactional-migrations.md
    title: Transactional Migrations
  - id: artifact-store
    resource: /architecture/artifact-store.md
    title: Plan Artifact Store
---

# Run States

```text
initialized -> applying -> verifying -> succeeded
      |            |           |
      v            v           v
    failed <-------+-----------+

applying | verifying | succeeded | failed | rollback-failed
                         |
                         v
                  rolling-back -> rolled-back
                         |
                         v
                  rollback-failed
```

The state machine rejects shortcuts such as `applying -> succeeded`. Verification must be a distinct recorded stage.

# Operation States

```text
pending -> running -> succeeded -> rolled-back
              |
              v
            failed -> running
              |
              v
          rolled-back
```

Each operation records attempts and recovery references. Recovery references identify snapshot entries; backup content remains in private run storage and never appears inline in the journal or result envelope.

# Revisions and Events

Every transition increments a journal revision and appends a monotonically sequenced event. Store updates require the caller's expected revision and the exact next revision. A stale writer receives `JOURNAL_REVISION_CONFLICT`.

# Persistence

Each run receives its own directory. Journal envelopes carry a content digest and are replaced atomically. Dependency-state cleanup records bind the cleanup operation identifier to package-local paths that were removed or already absent before target installation. Process records then capture the declared importer and installer commands with output withheld. A per-journal non-blocking exclusive lock prevents concurrent writers from passing the same revision check simultaneously. A separate repository-scoped transaction lock serializes apply, verify, and rollback across runs. Lock metadata records the writer process, allowing a later writer to recover an orphaned lock when the recorded process no longer exists.

# Execution Boundary

Apply creates the journal before recovery snapshots or repository mutation. It persists every run and operation transition, including non-reversible generated dependency-state cleanup. Successful apply stops in `verifying`; verify moves the run to `succeeded` or `failed`. Failed, applying, verifying, succeeded, and retryable rollback-failed runs may enter `rolling-back` after exact approval. Rollback reaches `rolled-back` only after snapshot restoration and baseline fingerprint verification; it never claims that removed source `node_modules` content was restored.
