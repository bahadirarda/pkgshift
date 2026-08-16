# Contributing to pkgshift

Thank you for helping improve deterministic package manager migrations.

## Development setup

Install Rust 1.97.1 and Bun 1.3.14 or newer, then run:

```bash
bun install --frozen-lockfile
bun run check
```

The complete validation suite must pass before a change is merged. Product changes must preserve the safety contract in `AGENTS.md` and update the OKF knowledge bundle when behavior or architecture changes.

Every user-visible implementation change also needs release intent:

```bash
bun run changeset
bun run changeset:status
```

Select the affected implementation packages, choose the appropriate `patch`, `minor`, or `major` impact, and write a concise user-facing summary. Documentation-only and internal maintenance changes outside implementation package boundaries do not require a Changeset.

## Commit and pull request titles

Use [Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/) for commit messages and pull request titles:

```text
feat(cli): add a migration preview option
fix(core): reject an ambiguous workspace root
docs(release): clarify package ownership
feat(core)!: revise the plan schema
```

Allowed types are `feat`, `fix`, `docs`, `refactor`, `perf`, `test`, `build`, `ci`, `chore`, and `revert`. A squash merge should use the validated pull request title.

## Version policy

Changesets aggregates committed release intent and maintains the three implementation descriptors as one fixed release group. Its automated version pull request runs `bun run version:packages`, which synchronizes the calculated version into Cargo, Bun, the internal Rust dependency, both lockfiles, and the dated root changelog.

The canonical checked-in version is `[workspace.package].version` in `Cargo.toml`. `bun run version:check` rejects drift across every descriptor.

Before 1.0, a minor release may contain breaking changes and a patch release remains backward compatible within its minor line. From 1.0 onward, pkgshift follows standard Semantic Versioning rules. Every release requires a curated `CHANGELOG.md` entry.

See the [release system](docs/governance/release-system.md) for package names, artifacts, and publication order.
