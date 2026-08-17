# Architecture Concepts

* [Migration Engine](migration-engine.md) - Defines the deterministic core, Project IR, plans, execution journal, and verification model.
* [Agent Interface](agent-interface.md) - Defines commands, structured output, approval boundaries, and process behavior.
* [Project IR](project-ir.md) - Defines the versioned semantic model extracted from repository evidence.
* [Capability Engine](capability-engine.md) - Defines source-feature to target-capability decisions and blocking semantics.
* [Artifact Store](artifact-store.md) - Defines opt-in immutable plan persistence and integrity checks.
* [Run Journal](run-journal.md) - Defines run and operation state transitions, revisions, and persistence.
* [Recovery and Verification](recovery-and-verification.md) - Defines snapshot integrity, post-apply checks, rollback, and external-effect limits.
* [Lock Graph Proof](lock-graph-proof.md) - Defines normalized lock graphs, native import paths, and blocking resolution drift policy.
* [Platform, Edge, and Executable Verification](verification-policies.md) - Defines target matrices, edge equivalence, and exact target executable proof.
* [Repository Integrations](repository-integrations.md) - Defines deterministic CI, container, toolchain, documentation, and development-environment migration.
* [Target Comparison](target-comparison.md) - Defines aggregate approval and independent isolated evidence for two or more candidate targets.
* [Bun to Deno Runtime Recipes](runtime-migration-recipes.md) - Defines the dedicated deterministic runtime transformation, permission, transaction, and residue-verification boundary.
* [Repository Layout](repository-layout.md) - Maps the MVP implementation to architecture boundaries and tests.
* [Skill Distribution](skill-distribution.md) - Defines portable Agent Skill locations and compatibility strategy.
