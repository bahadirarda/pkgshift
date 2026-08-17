# Documentation Update Log

## 2026-08-17

* **Post-MVP roadmap**: Completed the accepted patch, runtime-recipe, target-platform, dependency-edge, and target-executable roadmap with plan-bound policy identities, exact pre-mutation version enforcement, verification evidence, Agent Skill guidance, product-site coverage, and end-to-end fixtures.
* **Patch portability**: Expanded Yarn Modern and pnpm text unified-diff transport to exact, semver-range, and name-only selectors while retaining Bun's exact-version boundary, unsafe-path rejection, and binary or multi-source fail-closed behavior in both engines.
* **Runtime recipes**: Added modular Bun `Database` to `node:sqlite` `DatabaseSync` and Bun `$` to pinned dax recipes, explicit `env` plus `run` permission inference, unsupported-member diagnostics, apply, verification, residue, and rollback coverage, and a passing real Deno 2.9.5 execution.
* **Verification policy**: Added normalized repeatable target-platform matrices, compatible or strict reachable dependency-edge equivalence, `reachable-resolution-set-v3`, lockfile platform constraint extraction, and policy-preserving doctor, comparison, and approval actions.
* **Executable identity**: Added exact target executable requirements to plans, bounded shell-free version probes before snapshots, resolved executable run evidence, blocking verification, stable diagnostics, and mismatch-without-mutation acceptance coverage.
* **Readiness matrix**: Added targets-optional `pkgshift doctor` assessment across all seven production adapters from one repository evidence context, with deterministic non-ranking reports, target-scoped blockers, independent read-only next actions, Agent Skill guidance, and pinned upstream parity enforcement.
* **Migration readiness**: Added deterministic `pkgshift doctor --to <target>` assessment with stable verdicts, capability and integration evidence, projected cleanup and process effects, no plan or persisted state, Agent Skill guidance, and pinned upstream corpus enforcement.
* **Windows distribution**: Added a checksum-verifying PowerShell installer for the Windows x86-64 release with release-metadata validation, staged binary and Skill replacement, smoke-check rollback, stale-data cleanup, fixture acceptance on `windows-2025`, release assets, and an accessible website platform switcher; upgraded the Unix installer to the same rollback contract.
* **Native explanation**: Ported diagnostic and persisted-artifact explanation to the Rust primary CLI with complete emitted-code catalog validation, canonical identifier grammar, bounded state scans, plan/run/runtime identity checks, verification digest reconstruction, symbolic-link refusal, and read-only end-to-end fixtures.
* **Agent Skill lifecycle**: Ported copy, exact-source link, status, doctor, dry-run, and protected uninstall behavior to the Rust primary CLI with project/user confinement, Codex/Claude destinations, content digests, symbolic-link parent refusal, read-only approval previews, and exact machine-actionable next actions.
* **Runtime migration**: Added dedicated `pkgshift runtime to deno` planning and execution with reviewed Hono `Bun.serve`, `bun:test`, Bun file, script, and type-cleanup recipes; explicit Deno permissions; content-redacted artifacts; owner-only recovery state; residue verification; rollback; and a passing real Hono run on Deno 2.9.5. SQLite remains fail-closed because similarly named APIs do not prove behavioral compatibility.
* **Target comparison**: Added deterministic `pkgshift compare` planning, one aggregate process-execution approval, independent candidate sandboxes, nested trial evidence, capability-blocked candidates, explicit no-ranking semantics, and a passing real Bun 1.3.14 plus Deno 2.9.5 comparison run.
* **Representative verification**: Added explicit repeatable root-script selection, immutable target argv, bounded shell-free execution, operation-bound withheld process records, verification checks that never rerun repository code, and a passing real pnpm-to-Bun acceptance run.
* **Repository integrations**: Added context-aware package script, CI setup and command, cache lockfile, container, automation, Markdown, devcontainer, Volta, engine, `.tool-versions`, and mise migration with fail-closed ambiguous-command diagnostics.
* **Module Boundaries**: Split lock graph extraction, comparison, and inspection tests into focused modules and added a blocking production Rust module-size validation gate.
* **Clean migration state**: Added deterministic package-local `node_modules` retirement before target installation, journaled removed and already-absent paths, made both clean-install evidence and source-only artifact absence blocking verification checks, and surfaced bounded Bun runtime-reference warnings without deleting application semantics.
* **Modular Rust core**: Split capability analysis, cleanup, verification, registry translation, project transformation composition, and planner tests out of the former planner and transaction god files.
* **Reachable graph proof**: Added `reachable-resolution-set-v2` with manifest-root traversal, exact Bun and Deno edge targets, proven-unreachable pruning, optional-only platform absence handling, and fail-closed required-path diagnostics while preserving `resolution-set-v1` for topology-limited formats.
* **Corpus automation**: Converted the six pinned Hono, Vite, and vltpkg planning cases into a machine-readable contract with clean-checkout enforcement, bounded JSON summaries, and a weekly or manually dispatched GitHub Actions gate.
* **Corpus correction**: Re-ran Hono Bun-to-Deno through a real isolated Deno 2.9.5 trial; V2 tolerated three optional-only absences but proved that the remaining 48 source-only versions were reachable drift rather than stale lockfile debris.
* **Adapters**: Promoted vlt 1.0.2 and Deno 2.9.5 dependency mode to capability-gated production targets in both engines, expanding basic planning coverage to all 42 directed adapter pairs.
* **vlt**: Added workspace, workspace protocol, catalog, graph modifier, public registry and scope, integration command, installer, and v1 lock graph support.
* **Deno**: Added workspace, override, catalog expansion, isolated linker, preserved runtime configuration, import-map evidence, `deno task`, installer, and v5 npm and JSR lock graph support without expanding into runtime migration.
* **Acceptance**: Added pinned real vlt and Deno installer runs for dependency-bearing Bun workspaces and a real-world Hono, Vite, and vlt source corpus covering successful plans, capability blockers, external installer failure, graph rejection, and source preservation.
* **Parsing**: Added UTF-8 BOM normalization, excluded local pnpm locators from the registry graph, and normalized Deno peer contexts before resolution comparison.
* **CI**: Added a pinned adapter acceptance job for Node 22.22.0, vlt 1.0.2, Deno 2.9.5, Bun 1.3.14, and Rust 1.97.1.
* **Parity**: Added validated `packageExtensions` translation among npm, pnpm, and Yarn Modern in both engines.
* **Patching**: Added exact-version text patch conversion among Yarn Modern, pnpm, and Bun, including transitive Yarn resolutions and current pnpm workspace policy output.
* **Safety**: Added fail-closed patch selector, path, format, and conflict diagnostics, and bound every project `.patch` file to repository fingerprints and exact approvals.
* **Verification**: Added bidirectional renderer fixtures, CLI subprocess migrations, post-plan patch drift rejection, and a real Bun patch-application acceptance run.
* **Parity**: Ported Plug and Play and isolated linker rendering plus Yarn Modern registry translation into the Rust primary engine.
* **Lifecycle policy**: Added bidirectional Bun, pnpm, and Yarn Modern allow-list conversion, emitting current pnpm `allowBuilds` output and Yarn `enableScripts: false` with per-dependency build metadata.
* **Safety**: Kept literal registry credentials out of persisted Rust plans, required environment references for Yarn authentication, and blocked unsupported `.npmrc` settings.
* **Verification**: Added Rust unit and subprocess coverage, TypeScript parity fixtures, and a real Bun installer acceptance migration for linker and lifecycle behavior.
* **Parity**: Ported deterministic npm and pnpm override plus Yarn resolution rendering to the Rust primary engine.
* **Safety**: Added pnpm workspace policy discovery and blocking diagnostics for nested or scoped selector forms that cannot preserve target semantics.
* **Verification**: Added Rust unit and subprocess coverage for nested override translation, Yarn-to-npm policy migration, scoped package resolutions, pnpm workspace policy retention, and unsupported selector rejection.

## 2026-08-16

* **Calendar versioning**: Replaced counter-only pre-1 package versions with deterministic `0.YYYYMMDD.REVISION` identities while preserving Changesets as the compatibility-impact and release-note ledger.
* **Release synchronization**: Added calendar release pull request curation and made Bun workspace lock versions part of the blocking metadata parity check.

* **Trust**: Added normalized npm, pnpm, Yarn Classic, Yarn Modern, and text Bun lock graphs with blocking resolution-set and comparable-integrity verification.
* **Execution**: Added target-native importer selection, including `pnpm import`, `bun pm migrate`, Bun's pnpm migration path, and `yarn import`, while preserving source locks until target installation completes.
* **Trial**: Added exact-approval `pkgshift to <target> --trial` execution in a disposable repository copy with no source state, a nested verification report, and repository-preservation evidence.
* **Safety**: Added fail-closed malformed and binary lockfile diagnostics, symbolic-link rejection in trial copies, lifecycle-script suppression, and empty-resolution proof when a target legitimately omits an empty lockfile.
* **Parity**: Added native-import planning to the TypeScript reference and preserved source workspace pattern order in both engines.
* **Verification**: Added isolated trial, importer ordering, graph drift, lock format, empty graph, and live Bun 1.3.14 npm-to-Bun trial, apply, proof, and rollback coverage.
* **Documentation**: Added the lock graph proof and isolated trial concepts, refreshed the Agent Skill, and surfaced both trust features in the README and product website.
* **Website**: Added a responsive product website with semantic content, canonical metadata, social previews, structured software data, crawl directives, a root sitemap, and a curated agent discovery index.
* **Distribution**: Added a version-pinnable Linux and macOS shell installer that verifies GitHub Release checksums before installing a native executable.
* **Deployment**: Added a least-privilege GitHub Pages artifact workflow with repository-owned website validation.
* **Release**: Defined synchronized Semantic Versioning, Conventional Commit titles, curated changelogs, public crate identities, native artifact names, checksums, provenance, and ordered crates.io publication.
* **Supply Chain**: Prepared complete draft releases before publication and enabled immutable tags and assets for future releases.
* **Versioning foundation**: Added fixed-group Changesets release intent, automated version pull requests, synchronized Cargo and Bun metadata, and source-revision build provenance before calendar package identities were adopted.
* **Architecture**: Established a Rust-primary polyglot monorepo with sibling `implementations/rust` and `implementations/typescript` boundaries.
* **Decision**: Added the accepted Rust-primary monorepo decision, parity-gate policy, and shared schema boundary.
* **Implementation**: Ported weighted detection, Project IR, capability analysis, deterministic planning, exact approval, state persistence, apply, verification, and rollback into Rust.
* **Safety**: Added digest-verified plan and run envelopes, repository-scoped locking with dead-writer recovery on Linux, byte-level snapshots, atomic mutations, lifecycle-script suppression, and withheld installer output.
* **Verification**: Added Rust coverage for all 20 basic production planning directions and two independent subprocess migrations, including pnpm-to-Bun rollback.
* **Verification**: Ran a live multi-package pnpm-to-Bun migration with Bun 1.3.14, verified the generated dependency state, and restored the original repository fingerprint.
* **Tooling**: Grouped both engines under `implementations`, added root polyglot orchestration, pinned Rust 1.97.1, and expanded CI gates for rustfmt, Clippy, Cargo tests, Bun tests, OKF, and production builds.

## 2026-08-15

* **Branding**: Renamed the product, executable, state boundary, media types, diagnostics, distribution bundle, and Agent Skill to the lowercase `pkgshift` identity.
* **Presentation**: Added an original editorial hero asset and rebuilt the repository README as a product-oriented technical landing page.
* **Automation**: Added a least-privilege GitHub Actions workflow for type checking, tests, OKF and skill validation, English-only validation, and production build output.
* **Interface**: Added the current-directory `pkgshift to <target>` workflow with read-only preview, interactive confirmation, exact noninteractive approval, hidden default state, apply, and verification orchestration.
* **Verification**: Added unit and end-to-end coverage for unapproved, declined, agent-approved, and interactively approved guided migrations.
* **Verification**: Added two real pnpm-to-Bun migration fixtures covering workspace catalogs, isolated linking, trusted dependencies, exclusion patterns, local dependencies, repository integrations, successful installation, structural verification, and rollback.
* **Governance**: Standardized generated provenance on the `bahadirarda` project author identifier and prohibited agent, tool, model, or version names in knowledge metadata.
* **Initialization**: Established the OKF v0.2 knowledge bundle.
* **Creation**: Added the product vision, terminology, architecture, support model, decisions, and package manager migration workflow.
* **Creation**: Added the portable package manager migration Agent Skill outside the OKF bundle.
* **Implementation**: Added the dependency-free read-only CLI foundation with detection, support discovery, planning, explanations, and structured output.
* **Verification**: Added fixtures for deterministic planning, package manager ambiguity, Yarn generation detection, CLI shortcuts, and mutation boundaries.
* **Governance**: Added durable OKF, internal-link, Agent Skill, and English-only validation.
* **Implementation**: Added versioned Project IR extraction across workspace manifests, dependency protocols, policy shapes, linker settings, and redacted registry evidence.
* **Implementation**: Added source-to-target capability decisions with native, transform, lossy, unsupported, unknown, and not-applicable classifications.
* **Implementation**: Added opt-in atomic plan persistence, integrity verification, and a revisioned run journal state machine.
* **Verification**: Expanded the suite to cover IR semantics, capability blockers, secret-safe fingerprints, artifact tampering, journal transitions, and stale revision conflicts.
* **Implementation**: Added deterministic target renderers with exact before and after digests for manifests, workspace configuration, policy translation, registry references, and recognized integration commands.
* **Safety**: Added lossy-plan acceptance, exact approval tokens, stale-plan rejection, owner-only recovery snapshots, lifecycle-script suppression, shell-free target execution, and secret-safe persisted process records.
* **Implementation**: Enabled journaled apply, structural verification, successful and failed run recovery, restored-fingerprint checks, and artifact or run explanation.
* **Implementation**: Added Codex and Claude Code Agent Skill installation for project and user scopes with copy, link, status, doctor, conflict, and protected-uninstall behavior.
* **Verification**: Added strict TypeScript checking and end-to-end fixtures for plan, apply, verify, rollback, failed install, partial failure, orphaned locks, snapshot tampering, and skill installation.
* **Documentation**: Marked the technical MVP complete, documented explicit graph-diff and external-state boundaries, and added the recovery and verification architecture concept.
* **Safety**: Rejected mutation and recovery paths that traverse symbolic-link directories outside the selected project root.
* **Research**: Refreshed exact adapter baselines against official package registries and release feeds, and documented the executable-version boundary.
* **Safety**: Added a recoverable repository-scoped transaction lock so concurrent agents cannot race apply, verification, or rollback.
* **Safety**: Changed unsupported workspace glob syntax from silent partial matching to a blocking diagnostic.
* **Safety**: Blocked unsupported workspace protocol and npm configuration variants instead of silently reducing them during target rendering.
* **Safety**: Expanded persisted-plan secret checks to sensitive manifest fields, known token formats, and private-key material.
* **Safety**: Confined Agent Skill destinations to their declared project or user roots, including parent-directory symlink checks.
* **Verification**: Added the complete 20-direction basic planning matrix across npm, pnpm, both Yarn families, and Bun.
* **Tooling**: Added reproducible build and full validation scripts for the MVP handoff.
* **Research**: Corrected the Yarn Modern lifecycle-suppression command to the documented `--mode=skip-build` contract.
