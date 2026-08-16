# pkgshift

`pkgshift` is a deterministic, transactional package manager migration CLI for JavaScript repositories.

```bash
cargo install pkgshift --locked
cd /path/to/project
pkgshift to bun --dry-run
```

The CLI inspects the current repository, creates an immutable plan, requires approval for the exact plan, applies the migration, verifies the result, and preserves recovery state for rollback.

See the [project repository](https://github.com/bahadirarda/pkgshift) for the support matrix, safety contract, prebuilt binaries, and full documentation.
