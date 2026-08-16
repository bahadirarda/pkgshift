---
type: governance
status: draft
generated:
  by: bahadirarda
  at: 2026-08-16
sources:
  - resource: https://semver.org/spec/v2.0.0.html
    relation: normative-versioning
  - resource: https://www.conventionalcommits.org/en/v1.0.0/
    relation: normative-change-language
  - resource: https://keepachangelog.com/en/1.1.0/
    relation: normative-changelog-shape
  - resource: https://doc.rust-lang.org/cargo/reference/publishing.html
    relation: normative-cargo-publication
  - resource: https://docs.github.com/en/actions/how-tos/secure-your-work/use-artifact-attestations/use-artifact-attestations
    relation: normative-provenance
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

`[workspace.package].version` in the root `Cargo.toml` is the canonical Semantic Version. The root package metadata, TypeScript reference metadata, and `pkgshift` dependency on `pkgshift-core` repeat that version only where their package formats require it. Repository validation rejects drift.

During the pre-1.0 period:

- A patch version contains backward-compatible fixes within the current minor line.
- A minor version may add features or revise an unstable interface.
- Version 1.0 establishes the first stable compatibility contract.

Conventional Commit types describe the change, while the curated changelog determines the public release narrative. `fix` normally implies a patch, `feat` normally implies a minor, and `!` or `BREAKING CHANGE` explicitly marks an incompatible change.

## Release artifacts

An annotated tag named `v<version>` triggers native release builds. The tag must match the canonical version and point to a commit reachable from `main`.

| Artifact | Platform |
| --- | --- |
| `pkgshift-v<version>-x86_64-unknown-linux-gnu.tar.gz` | Linux x86-64 |
| `pkgshift-v<version>-aarch64-unknown-linux-gnu.tar.gz` | Linux ARM64 |
| `pkgshift-v<version>-x86_64-apple-darwin.tar.gz` | macOS Intel |
| `pkgshift-v<version>-aarch64-apple-darwin.tar.gz` | macOS Apple silicon |
| `pkgshift-v<version>-x86_64-pc-windows-msvc.zip` | Windows x86-64 |
| `SHA256SUMS` | SHA-256 manifest for every archive |

Each archive contains the native executable, README, and MIT license. GitHub artifact attestations bind the downloadable files to their build workflow and source revision.

## Publication sequence

1. Update the canonical version and all synchronized metadata.
2. Move user-visible changes from `Unreleased` into a dated changelog section.
3. Run `bun run check`, Cargo package verification, and release archive smoke tests.
4. Merge the release commit to `main` and create the annotated `v<version>` tag.
5. Let the tag workflow publish the GitHub Release, archives, checksums, and provenance.
6. Explicitly dispatch the protected crates.io workflow for the same tag.
7. Publish `pkgshift-core` first, wait for registry visibility, then publish `pkgshift`.

crates.io versions are permanent and cannot be overwritten. Registry publication is therefore manual, environment-scoped, and token-gated even though its validation and ordering are automated.

## Failure policy

A tag/version mismatch, a tag outside `main`, failed test, invalid package, missing changelog section, failed native build, or checksum failure blocks release creation. A failed crates.io publication does not rewrite a tag or version; it is diagnosed and safely retried for the same tag, skipping a package version that is already visible in the registry.
