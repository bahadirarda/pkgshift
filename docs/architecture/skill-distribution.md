---
type: Distribution Architecture
title: Agent Skill Distribution
description: Defines the portable Agent Skill source and safe Codex and Claude Code installation paths.
tags: [architecture, agent-skills, codex, claude-code, distribution]
status: draft
stale_after: 2026-11-15
generated: { by: bahadirarda, at: 2026-08-15T19:53:59Z}
sources:
  - id: agent-skills-spec
    resource: https://agentskills.io/specification
    title: Agent Skills specification
  - id: openai-skills
    resource: https://learn.chatgpt.com/docs/build-skills
    title: OpenAI Skills documentation
  - id: claude-skills
    resource: https://code.claude.com/docs/en/slash-commands
    title: Claude Code skills documentation
---

# Portable Source

The distributable source is:

```text
skills/pkgshift/
```

It contains portable `SKILL.md` instructions, optional references, and OpenAI presentation metadata under `agents/openai.yaml`. The `SKILL.md` frontmatter contains only `name` and `description`.

# Supported Destinations

| Client | Scope | Destination |
| --- | --- | --- |
| Codex | Project | `.agents/skills/pkgshift/` |
| Codex | User | `~/.agents/skills/pkgshift/` |
| Claude Code | Project | `.claude/skills/pkgshift/` |
| Claude Code | User | `~/.claude/skills/pkgshift/` |

Codex scans `.agents/skills` from the current working directory through the repository root and reads user skills from `~/.agents/skills`.[^openai-skills] Claude Code uses `.claude/skills` for project discovery and `~/.claude/skills` for personal skills.[^claude-skills]

# Installer Commands

```text
pkgshift skill install --scope project --client codex --mode copy --approve skill:pkgshift:project:codex
pkgshift skill install --scope user --client codex --mode link --approve skill:pkgshift:user:codex
pkgshift skill install --scope project --client claude --mode copy --approve skill:pkgshift:project:claude
pkgshift skill status --scope project --client codex
pkgshift skill doctor --scope project --client claude
pkgshift skill uninstall --scope project --client codex --approve skill:pkgshift:project:codex
```

Project, Codex, and managed-copy are the CLI defaults. User scope changes only the client root; it never modifies a repository.

# Modes

`copy` atomically prepares a managed copy and compares its content digest with the portable source. `link` creates one directory symlink to the resolved portable source. Status and doctor identify the mode, destination, source digest, installed digest, health, and local modification state.

# Safety Invariants

- Require an exact scope- and client-bound approval token for install and uninstall.
- Never replace an existing directory, file, or link with different ownership.
- Resolve and validate the portable source before mutation.
- Reject destination paths whose parent directories traverse symbolic links outside the declared project or user scope.
- Refuse to uninstall a managed copy that differs from its source.
- Refuse to uninstall an unverifiable managed copy when the portable source is missing.
- Remove an exact source link without traversing or deleting its target.
- Keep vendor-specific metadata outside portable `SKILL.md` frontmatter.
- Re-check client discovery documentation when `stale_after` is reached.

# Post-MVP Compatibility

Additional clients may be added as explicit adapter destinations. The installer does not assume that every Agent Skills implementation scans `.agents/skills`; each destination needs current authoritative discovery evidence and fixture coverage.

[^agent-skills-spec]: Agent Skills specification
[^openai-skills]: OpenAI Skills documentation
[^claude-skills]: Claude Code skills documentation
