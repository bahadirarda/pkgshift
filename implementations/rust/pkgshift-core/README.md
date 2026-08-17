# pkgshift-core

`pkgshift-core` is the deterministic migration engine used by the `pkgshift` CLI.

It owns repository inspection, Project IR, capability analysis, normalized lock graphs, native importer selection, immutable planning, integrity-checked state, transactional execution, isolated single-target and multi-target trials, explicit representative-script verification, dedicated Bun-to-Deno runtime recipes, Agent Skill lifecycle safety, and rollback. The Rust API is pre-1.0 and may evolve between minor releases.

Most users should install the [pkgshift CLI](https://crates.io/crates/pkgshift). Engine documentation is available on [docs.rs](https://docs.rs/pkgshift-core), and the source lives in the [project repository](https://github.com/bahadirarda/pkgshift).
