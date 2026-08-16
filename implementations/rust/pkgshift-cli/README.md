# pkgshift

`pkgshift` is a deterministic, transactional package manager migration CLI for JavaScript repositories.

```bash
cargo install pkgshift --locked
cd /path/to/project
pkgshift to bun --dry-run
pkgshift to bun --trial
```

The CLI inspects the current repository, creates an immutable plan, requires approval for the exact plan, can execute it in a disposable trial copy, applies approved repository mutations, proves the target resolution set, and preserves recovery state for rollback.

See the [project repository](https://github.com/bahadirarda/pkgshift) for the support matrix, safety contract, prebuilt binaries, and full documentation.
