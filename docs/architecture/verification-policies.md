---
type: Verification Architecture
title: Platform, Edge, and Executable Verification
description: Defines target-platform matrices, dependency-edge equivalence, and exact target executable resolution for package-manager migrations.
tags: [architecture, verification, lock-graph, platform, executable]
status: draft
generated: { by: bahadirarda, at: 2026-08-17T15:14:32Z }
sources:
  - id: recovery-verification
    resource: /architecture/recovery-and-verification.md
    title: Recovery and Verification
  - id: package-manager-support
    resource: /support/package-managers.md
    title: Package Manager Support
---

# Plan-Bound Verification Policy

Package-manager plans carry a normalized `verificationPolicy`. The policy participates in the immutable plan identifier, is preserved by every generated next action, and is copied into lock-graph comparison evidence. Repeating a command without the same policy cannot approve or execute the original plan.

The default policy is compatible edge comparison with no explicit platform matrix. This preserves the established behavior for repositories that have not declared deployment targets.

# Target-Platform Matrix

Add one or more repeatable targets with `--target-platform OS/CPU[/LIBC]`:

```text
pkgshift to bun \
  --target-platform darwin/arm64 \
  --target-platform linux/x64/glibc
```

Values are validated, normalized to lowercase, sorted, and deduplicated before planning. The optional libc component accepts `glibc` or `musl` only for Linux.

With no matrix, a package-name absence reached only through optional dependency edges retains the compatibility tolerance. With a matrix, absence is tolerated only when the lockfile records platform constraints and every selected target is incompatible with that optional resolution. Missing or compatible constraint evidence fails closed.

# Dependency-Edge Equivalence

Use `--edge-equivalence strict` to make semantic reachable-edge drift blocking:

```text
pkgshift to bun --edge-equivalence strict
```

Compatible mode still reports edge changes but blocks on reachable resolution, integrity, or required-path drift. Strict mode additionally requires the same normalized `(parent resolution, dependency name, dependency kind)` set. Package-manager-specific target locators are excluded because their encoding differs even when the dependency relation is equivalent.

Topology-limited lock formats continue to use the explicit `resolution-set-v1` fallback. Topology-capable formats use `reachable-resolution-set-v3`, which records the selected policy and platform decisions in its comparison identity.

# Exact Target Executable

Every new package-manager plan declares a `targetExecutable` requirement derived from the pinned adapter baseline. Apply resolves that program from `PATH`, canonicalizes the executable path, runs a bounded shell-free `--version` probe, and requires the exact planned version before snapshots or repository mutation begin.

The resolved program, canonical path, exact version, and package-manager pin are stored in the run journal. Verification includes a blocking `target-executable-version` check against that stored evidence. Probe output is withheld, size-bounded, and never persisted.

An unavailable executable returns `TARGET_EXECUTABLE_UNAVAILABLE`. A failed probe or version mismatch returns `TARGET_EXECUTABLE_VERSION_MISMATCH`. Both are precondition failures with exit code `4`; the operator must activate the exact pin and retry the unchanged approved command.
