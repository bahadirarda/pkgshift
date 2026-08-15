---
type: Architecture Decision
title: Shared Agent Skills Path
description: Uses client-native Codex and Claude Code skill roots while preserving one portable source.
tags: [decision, agent-skills, distribution]
status: draft
decision_status: accepted
stale_after: 2026-11-15
generated: { by: bahadirarda, at: 2026-08-15T19:53:59Z}
sources:
  - id: agent-skills-spec
    resource: https://agentskills.io/specification
    title: Agent Skills specification
  - id: skill-distribution
    resource: /architecture/skill-distribution.md
    title: Agent Skill Distribution
---

# Context

Modern coding agents increasingly consume the Agent Skills directory format, but discovery paths are not fully uniform. Duplicating independently edited skills for every client would create drift.

# Decision

Keep the distributable skill source at `skills/pkgshift/`. Use `.agents/skills/pkgshift/` and `~/.agents/skills/pkgshift/` for Codex. Use `.claude/skills/pkgshift/` and `~/.claude/skills/pkgshift/` for Claude Code. Require the caller to select the client explicitly when it differs from the Codex default.

Keep portable instructions in `SKILL.md` and isolate vendor-specific interface metadata under `agents/`.

# Consequences

- One distributable skill source can serve several coding agents and installation scopes.
- The installer needs source validation, conflict detection, links, copy digests, status, doctor, and protected uninstall behavior.
- Client compatibility documentation is volatile and requires scheduled review.
- A client-specific extension cannot become a required field in the portable skill core.

# Related Concepts

- [Agent Skill Distribution](/architecture/skill-distribution.md)
