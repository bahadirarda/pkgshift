---
type: governance
status: draft
generated:
  by: bahadirarda
  at: 2026-08-16
sources:
  - resource: https://semver.org/spec/v2.0.0.html
    relation: normative-versioning
  - resource: https://calver.org/
    relation: calendar-versioning-convention
  - resource: https://www.conventionalcommits.org/en/v1.0.0/
    relation: normative-change-language
  - resource: https://keepachangelog.com/en/1.1.0/
    relation: normative-changelog-shape
  - resource: https://github.com/changesets/changesets
    relation: release-intent-system
  - resource: https://doc.rust-lang.org/cargo/reference/publishing.html
    relation: normative-cargo-publication
  - resource: https://docs.github.com/en/actions/how-tos/secure-your-work/use-artifact-attestations/use-artifact-attestations
    relation: normative-provenance
  - resource: https://docs.github.com/en/code-security/concepts/supply-chain-security/immutable-releases
    relation: normative-release-integrity
  - resource: https://docs.github.com/en/packages/learn-github-packages/introduction-to-github-packages
    relation: distribution-boundary
---

# Release System

## Purpose

The release system gives every pkgshift version one identity across source metadata, registry packages, binaries, checksums, documentation, and Git history. Publication is reproducible, reviewable, and separated from ordinary CI.

## Package identities

| Name | Channel | Role | Published |
| --- | --- | --- | --- |
| `pkgshift` | crates.io | Primary CLI and `pkgshift` executable | Yes |
| `pkgshift-core` | crates.io | Rust migration engine required by the CLI | Yes |
| `pkgshift-workspace` | Local Bun workspace | Polyglot repository orchestration | No |
| `@bahadirarda/pkgshift-typescript` | Local Bun workspace | Executable parity reference | No |

GitHub Packages does not provide a Cargo registry. Rust packages therefore use crates.io, while GitHub Releases distribute compiled binaries. The private TypeScript reference is not published to npm or GitHub Packages because it is not the product distribution surface.

## Canonical version

Each user-visible implementation change carries a committed Changeset with an explicit compatibility impact and user-facing summary. Private package descriptors beside both Rust crates allow Changesets to model non-npm packages, and the Rust crates plus TypeScript parity package form one fixed release group. Changesets remains the review and release-intent ledger; its calculated package number is an intermediate value rather than the published identity.

The automated version pull request aggregates the declarations, calculates the calendar version from the source commit date and current canonical version, synchronizes release files, replaces its generated description with the actual calendar identity, and dispatches the full Rust and TypeScript validation suite against the exact release commit. GitHub token-created pull requests produce approval-only placeholder runs; the automation removes only those redundant runs after dispatching validation.

`[workspace.package].version` in the root `Cargo.toml` remains the canonical checked-in version. The root package metadata, implementation descriptors, TypeScript reference metadata, private Changesets proxies, Bun workspace lock, Cargo lock, and `pkgshift` dependency on `pkgshift-core` repeat that version only where their formats require it. Repository validation rejects drift.

During the pre-stable period, pkgshift uses a Semantic Version-compatible calendar identity:

```text
0.YYYYMMDD.REVISION
```

- `0` declares the pre-stable compatibility epoch.
- `YYYYMMDD` is the release source commit's calendar date.
- `REVISION` starts at `0` on a new date and increments for another release from the same date.
- Dates cannot move backward relative to the checked-in canonical version.
- The final counter-only version, `0.2.0`, remains an immutable historical release and is the only accepted migration source for the first calendar version.

For example, the first release sourced on 2026-08-16 is `0.20260816.0`; another release sourced on that date is `0.20260816.1`; the next release day begins `0.20260817.0`. The numeric form is valid for Cargo, crates.io, npm-compatible metadata, and Semantic Version tooling while making release chronology visible.

Changeset impacts retain their compatibility meaning without directly selecting the numeric release:

- A patch declaration contains backward-compatible fixes within the current release line.
- A minor declaration may add features or revise an unstable interface.
- A major declaration identifies an incompatible change that requires explicit release review.

Conventional Commit types describe the change, while the curated changelog determines the public release narrative. `fix` normally implies a patch, `feat` normally implies a minor, and `!` or `BREAKING CHANGE` explicitly marks an incompatible change.

Official binary archives contain `release.json` with a build identity shaped as `<calendar-version>+sha.<short-sha>`, the full source commit, commit date, annotated tag, and Rust target. The calendar version identifies the release; the commit fields provide exact build provenance without making a commit count part of dependency resolution.

## Release artifacts

An annotated tag named `v<version>` triggers native release builds. The tag must match the canonical version and point to a commit reachable from `main`.

| Artifact | Platform |
| --- | --- |
| `pkgshift-v<version>-x86_64-unknown-linux-gnu.tar.gz` | Linux x86-64 |
| `pkgshift-v<version>-aarch64-unknown-linux-gnu.tar.gz` | Linux ARM64 |
| `pkgshift-v<version>-x86_64-apple-darwin.tar.gz` | macOS Intel |
| `pkgshift-v<version>-aarch64-apple-darwin.tar.gz` | macOS Apple silicon |
| `pkgshift-v<version>-x86_64-pc-windows-msvc.zip` | Windows x86-64 |
| `install.sh` | Verified Linux and macOS installer |
| `install.ps1` | Verified Windows x86-64 installer |
| `SHA256SUMS` | SHA-256 manifest for every archive and installer |

Each archive contains the native executable, README, MIT license, `release.json` build identity, and the canonical `skills/pkgshift` portable Agent Skill tree. GitHub artifact attestations bind the downloadable files to their build workflow and source revision. The workflow assembles a draft with every asset before publication; repository release immutability then prevents published tags and assets from being moved, replaced, or deleted.

The product website and every release distribute the shell and PowerShell installers defined by the [website delivery contract](/governance/website-delivery.md). Each resolves a stable GitHub Release and verifies its archive against the release-owned `SHA256SUMS` before installing. Installers are convenience delivery surfaces, not separate package identities or sources of release metadata; their release copies are also checksummed and attested.

## Publication sequence

1. Add a Changeset to every user-visible implementation pull request.
2. Merge the curated automated version pull request that calculates `0.YYYYMMDD.REVISION` and synchronizes manifests, lockfiles, and changelogs.
3. Run `bun run check`, Cargo package verification, and release archive smoke tests.
4. Create the annotated `v<version>` tag from the version commit on `main`.
5. Let the tag workflow assemble and atomically publish the immutable GitHub Release, archives, checksums, and provenance.
6. Explicitly dispatch the protected crates.io workflow for the same tag.
7. Publish `pkgshift-core` first, wait for registry visibility, then publish `pkgshift`.

crates.io versions are permanent and cannot be overwritten. Registry publication is therefore manual, environment-scoped, and token-gated even though its validation and ordering are automated.

## Failure policy

A tag/version mismatch, a tag outside `main`, failed test, invalid package, missing changelog section, failed native build, or checksum failure blocks release creation. A failed crates.io publication does not rewrite a tag or version; it is diagnosed and safely retried for the same tag, skipping a package version that is already visible in the registry.
