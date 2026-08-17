# pkgshift

`pkgshift` is a deterministic, transactional package manager and runtime migration CLI for JavaScript repositories.

```bash
cargo install pkgshift --locked
cd /path/to/project
pkgshift to bun --dry-run
pkgshift to bun --trial
pkgshift to bun --trial --verify-script test
pkgshift compare bun deno --verify-script test
pkgshift runtime to deno --deno-permission net --dry-run
```

The CLI inspects the current repository, creates an immutable plan, requires approval for the exact plan, can execute package-manager plans in disposable trial copies, compares multiple targets in independent sandboxes, applies approved repository mutations, proves the target resolution set, optionally runs explicitly selected root scripts with bounded shell-free execution, and preserves recovery state for rollback. A separate Rust-owned runtime surface applies reviewed Bun-to-Deno source recipes with explicit Deno permissions, residue verification, and its own rollback command. Managed Agent Skill commands install, inspect, diagnose, and safely remove project or user copies and exact-source links for Codex and Claude Code.

See the [project repository](https://github.com/bahadirarda/pkgshift) for the support matrix, safety contract, prebuilt binaries, and full documentation.
