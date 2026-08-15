---
type: Documentation Standard
title: Documentation System
description: Defines how the project authors and validates its Open Knowledge Format v0.2 bundle.
tags: [governance, documentation, okf, provenance]
status: draft
stale_after: 2027-02-15
generated: { by: bahadirarda, at: 2026-08-15T19:53:59Z}
sources:
  - id: okf-spec
    resource: https://github.com/GoogleCloudPlatform/knowledge-catalog/blob/main/okf/SPEC.md
    title: Open Knowledge Format v0.2 specification
---

# Bundle Boundary

`docs/` is one Open Knowledge Format v0.2 knowledge bundle.[^okf-spec] Markdown outside `docs/` is not part of that bundle.

- `docs/index.md` is the bundle root and may contain only `okf_version: "0.2"` in frontmatter.
- Any `index.md` below the root is a progressive-disclosure listing and has no frontmatter.
- Any `log.md` is a newest-first update history with ISO 8601 date headings and no frontmatter.
- Every other Markdown file is one concept and requires parseable YAML frontmatter with a non-empty `type`.

# Concept Metadata

Concepts should include `title`, `description`, `tags`, and lifecycle `status`. Use `sources` for provenance and stable source identifiers when a body footnote attributes a specific claim.

Generated content records the project author identifier `bahadirarda` and a timestamp. Never embed an agent, tool, model name, or model version in knowledge metadata. Content remains `status: draft` with no `verified` field until an actual machine process or human reviewer confirms it against its sources. Never add a human verification event on behalf of a user.

Use `stale_after` for volatile facts such as tool discovery paths, package manager behavior, compatibility, and delivery status.

# Linking

Use bundle-relative links beginning with `/` between concepts. Use relative links in index files so normal Markdown navigation and progressive disclosure remain readable. Broken links are tolerated by OKF consumers but blocked by this repository's stricter validation policy.

# Format Separation

Agent Skills are a separate standard. Files under `skills/` follow the Agent Skills specification and do not receive OKF `type`, provenance, lifecycle, or verification fields in `SKILL.md` frontmatter.

# Validation

Run:

```text
bun run validate
```

The validator checks:

- YAML parsing and required OKF fields.
- Reserved index and log structures.
- Supported lifecycle values and source entries.
- Bundle-relative and repository-relative links.
- Agent Skill frontmatter and OpenAI interface metadata.
- The English-only repository content rule.

Validation proves structural conformance, not factual correctness. Factual trust remains visible through `generated`, `verified`, provenance, lifecycle, and freshness metadata.

[^okf-spec]: Open Knowledge Format v0.2 specification
