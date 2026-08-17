# pkgshift

`pkgshift` is a deterministic, transactional package manager migration CLI for JavaScript repositories.

```bash
cargo install pkgshift --locked
cd /path/to/project
pkgshift to bun --dry-run
pkgshift to bun --trial
pkgshift to bun --trial --verify-script test
pkgshift compare bun deno --verify-script test
```

The CLI inspects the current repository, creates an immutable plan, requires approval for the exact plan, can execute it in a disposable trial copy, compares multiple targets in independent sandboxes, applies approved repository mutations, proves the target resolution set, optionally runs explicitly selected root scripts with bounded shell-free execution, and preserves recovery state for rollback.

See the [project repository](https://github.com/bahadirarda/pkgshift) for the support matrix, safety contract, prebuilt binaries, and full documentation.
