# Repository Instructions

## Language

- Write all repository content in English, including source code, comments, tests, fixtures, examples, diagnostics, commit messages, and documentation.
- Keep user-facing terminology consistent with `docs/product/terminology.md`.

## Product Safety Contract

- Preserve the `inspect -> plan -> approve -> apply -> verify` workflow.
- Treat inspection and planning as read-only operations.
- Require explicit approval before any command mutates a project.
- Prefer deterministic transformations and machine-readable output over prose-only behavior.
- Never print secrets. Redact registry credentials, environment values, and authentication material.

## Documentation

- Treat `docs/` as an Open Knowledge Format v0.2 bundle.
- Keep `docs/index.md` limited to the optional `okf_version` frontmatter exception.
- Do not add frontmatter to any `index.md` or `log.md` below the bundle root.
- Add parseable YAML frontmatter with a non-empty `type` to every other Markdown file under `docs/`.
- Use bundle-relative links beginning with `/` for links between OKF concepts.
- Record generated concepts with `generated.by: bahadirarda`; never include agent, tool, or model names in knowledge metadata.
- Keep generated concepts as `status: draft` and omit `verified` until an actual verifier reviews the content.
- Update `docs/log.md` when the bundle changes materially.

## Agent Skills

- Keep distributable portable skill sources under `skills/`.
- Treat `.agents/skills/` as the standard project installation destination managed by the installer.
- Follow the Agent Skills specification for every `SKILL.md` file.
- Use only `name` and `description` in `SKILL.md` frontmatter.
- Keep vendor-specific metadata under the skill's `agents/` directory.
- Do not apply OKF frontmatter rules to Agent Skill files.
