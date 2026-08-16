# Changelog

All notable changes to pkgshift are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Releases use the Semantic Version-compatible calendar shape `0.YYYYMMDD.REVISION`.

## [Unreleased]

## [0.20260817.0] - 2026-08-17

### Changed

- Render the deterministic npm and pnpm override plus Yarn resolution subset in the Rust primary path. Detect policy in `pnpm-workspace.yaml`, preserve bare scoped package resolutions, and block selector shapes whose semantics cannot be retained.

## [0.20260816.0] - 2026-08-16

### Changed

- Adopt deterministic calendar release identities shaped as `0.YYYYMMDD.REVISION` while retaining Changesets for reviewed compatibility impact and release notes. Synchronize Cargo, package metadata, the Bun workspace lock, changelogs, tags, archives, and release pull requests around the calculated calendar version.

## [0.2.0] - 2026-08-16

### Changed

- Add isolated migration trials, target-native lockfile importer selection, and blocking source-to-target resolution graph verification to the Rust primary path. Preserve native importer planning in the TypeScript reference, retain workspace pattern order, and publish the new trust workflow through the Agent Skill and product documentation.

## [0.1.2] - 2026-08-16

### Changed

- Add the official product website and checksum-verified shell installer.

## [0.1.1] - 2026-08-16

### Changed

- Add coordinated Changesets release planning, immutable publication preparation, and dated source-revision build identities.
- Ensure automated version pull requests receive full Rust and TypeScript validation.

## [0.1.0] - 2026-08-16

### Added

- Added the Rust-first `pkgshift` CLI and deterministic `pkgshift-core` migration engine.
- Added production planning support for npm, pnpm, Yarn Classic, Yarn Modern, and Bun.
- Added immutable plans, exact approval, transactional execution, structural verification, and integrity-checked rollback.
- Added stable JSON output and a portable Agent Skill for agent-driven workflows.
- Added a TypeScript compatibility implementation and parity test suite.
- Added cross-platform GitHub Release archives, SHA-256 checksums, and build provenance attestations.
- Added crates.io-ready package metadata and an explicitly approved publication workflow.

[Unreleased]: https://github.com/bahadirarda/pkgshift/compare/v0.20260817.0...HEAD
[0.20260817.0]: https://github.com/bahadirarda/pkgshift/compare/v0.20260816.0...v0.20260817.0
[0.20260816.0]: https://github.com/bahadirarda/pkgshift/compare/v0.2.0...v0.20260816.0
[0.2.0]: https://github.com/bahadirarda/pkgshift/compare/v0.1.2...v0.2.0
[0.1.2]: https://github.com/bahadirarda/pkgshift/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/bahadirarda/pkgshift/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/bahadirarda/pkgshift/releases/tag/v0.1.0
