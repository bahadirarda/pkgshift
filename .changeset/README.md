# Changesets

Every user-visible change to the Rust CLI, Rust engine, or TypeScript parity implementation requires a Changeset.

```bash
bun run changeset
bun run changeset:status
```

Select the affected package names and the correct Semantic Versioning impact. The three implementation descriptors form one fixed release group, so the final release version remains synchronized across Cargo and Bun metadata.

Changeset files record release intent; they do not publish packages. The automated version pull request consumes them, updates all manifests and lockfiles through `bun run version:packages`, and writes the dated root changelog entry. GitHub Releases and crates.io publication remain separate protected workflows.
