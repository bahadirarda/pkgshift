---
type: Architecture Decision
title: Agent-first CLI
description: Chooses a deterministic keyword-based CLI with structured output as the primary product interface.
tags: [decision, cli, agents, json]
status: draft
decision_status: accepted
generated: { by: bahadirarda, at: 2026-08-15T19:53:59Z}
sources:
  - id: founding-discussion
    resource: "founding product discussion on 2026-08-15"
    title: Founding product discussion
    author: human:project-founder
---

# Context

Most expected usage will occur through coding agents. These callers need deterministic commands, stable schemas, explicit side-effect metadata, and machine-actionable remediation. Human maintainers still need concise commands and readable plans.

# Decision

Make the CLI agent-first without making it agent-only.[^founding-discussion]

- Use established migration keywords: inspect, plan, apply, verify, explain, and rollback.
- Provide `--json`, noninteractive operation, stable diagnostic codes, and structured next actions.
- Keep standard output parseable and send progress or logs to standard error.
- Make `pkgshift to <target>` the primary current-directory workflow for humans and agents.
- Let the guided command orchestrate persistence, apply, and verification only after interactive confirmation or exact plan approval.
- Keep staged commands available as the advanced interface.
- Make the plan artifact, not conversational context, the input to apply.

# Consequences

- JSON schema compatibility becomes a public API obligation.
- Human text rendering and agent output must share one result model.
- Diagnostics require stable identifiers and separate explanatory content.
- Interactive prompts remain optional presentation; they cannot be required for automation.
- The guided command must re-plan before persistence and require the approved identifier to match current repository evidence.

# Related Concepts

- [Agent Interface](/architecture/agent-interface.md)
- [Transactional Migrations](/decisions/transactional-migrations.md)

[^founding-discussion]: Founding product discussion
