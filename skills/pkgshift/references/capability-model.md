# Capability Model

Use capability results to describe feasibility without assuming that similar syntax has identical semantics.

## Classifications

| Classification | Agent behavior |
| --- | --- |
| `native` | Describe the direct target representation. |
| `transform` | Describe the deterministic transformation and expected graph effect. |
| `lossy` | Present the semantic compromise and require approval through the plan. |
| `unsupported` | Stop the affected migration unless an explicit supported alternative exists. |
| `unknown` | Gather more evidence or report the coverage gap; do not guess. |
| `not-applicable` | Exclude the capability from the selected adapter mode. |

## Capability Groups

Review identity and version pins, manifests and protocols, workspaces, resolution policy, install modes, patches and plugins, registries, command execution, repository integrations, and verification support.

## Target Selection

When the user has not selected a target:

1. Inspect the repository.
2. Exclude targets with blocking unsupported capabilities.
3. Rank remaining targets by semantic preservation, adapter release tier, and verification coverage.
4. Present meaningful tradeoffs rather than a universal recommendation.
5. Keep preview targets visibly labeled and require the preview gate for apply.

Do not equate dependency graph differences with failure automatically. Use the plan's graph policy to distinguish expected peer, optional, or platform-specific drift from blocking version, source, or integrity changes.

