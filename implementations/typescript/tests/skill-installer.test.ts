import { afterEach, describe, expect, test } from "bun:test";
import { appendFile, symlink } from "node:fs/promises";
import { join, resolve } from "node:path";
import {
  inspectSkill,
  installSkill,
  SkillInstallerError,
  uninstallSkill,
} from "../src/skills/installer.ts";
import { createProject, removeTemporaryProjects } from "./helpers/project.ts";

afterEach(removeTemporaryProjects);

const sourcePath = resolve(import.meta.dir, "../../../skills/pkgshift");

describe("Agent Skill installer", () => {
  test("installs and removes a healthy project-scoped managed copy", async () => {
    const root = await createProject({});
    const approval = "skill:pkgshift:project:codex";
    const before = await inspectSkill({ projectRoot: root, sourcePath, scope: "project", client: "codex" });
    expect(before.installed).toBeFalse();

    await expect(installSkill({
      projectRoot: root,
      sourcePath,
      scope: "project",
      client: "codex",
      mode: "copy",
      approval: null,
    })).rejects.toBeInstanceOf(SkillInstallerError);

    const installed = await installSkill({
      projectRoot: root,
      sourcePath,
      scope: "project",
      client: "codex",
      mode: "copy",
      approval,
    });
    expect(installed.installed).toBeTrue();
    expect(installed.healthy).toBeTrue();
    expect(installed.mode).toBe("copy");

    const removed = await uninstallSkill({
      projectRoot: root,
      sourcePath,
      scope: "project",
      client: "codex",
      approval,
    });
    expect(removed.installed).toBeFalse();
  });

  test("preserves a locally modified managed copy", async () => {
    const root = await createProject({});
    const approval = "skill:pkgshift:project:codex";
    const installed = await installSkill({
      projectRoot: root,
      sourcePath,
      scope: "project",
      client: "codex",
      mode: "copy",
      approval,
    });
    await appendFile(`${installed.targetPath}/SKILL.md`, "\nLocal customization.\n", "utf8");

    const modified = await inspectSkill({ projectRoot: root, sourcePath, scope: "project", client: "codex" });
    expect(modified.modified).toBeTrue();
    await expect(uninstallSkill({
      projectRoot: root,
      sourcePath,
      scope: "project",
      client: "codex",
      approval,
    })).rejects.toMatchObject({
      diagnostic: { code: "SKILL_UNINSTALL_MODIFIED" },
    });
  });

  test("refuses to replace a conflicting project skill directory", async () => {
    const root = await createProject({
      ".agents/skills/pkgshift/SKILL.md": [
        "---",
        "name: pkgshift",
        "description: Unmanaged local skill.",
        "---",
        "",
        "# Local Skill",
        "",
      ].join("\n"),
    });
    await expect(installSkill({
      projectRoot: root,
      sourcePath,
      scope: "project",
      client: "codex",
      mode: "copy",
      approval: "skill:pkgshift:project:codex",
    })).rejects.toMatchObject({
      diagnostic: { code: "SKILL_INSTALL_CONFLICT" },
    });
  });

  test("supports a Claude-compatible project symlink installation", async () => {
    const root = await createProject({});
    const approval = "skill:pkgshift:project:claude";
    const installed = await installSkill({
      projectRoot: root,
      sourcePath,
      scope: "project",
      client: "claude",
      mode: "link",
      approval,
    });
    expect(installed.healthy).toBeTrue();
    expect(installed.mode).toBe("link");
    expect(installed.targetPath).toContain("/.claude/skills/");

    const removed = await uninstallSkill({
      projectRoot: root,
      sourcePath,
      scope: "project",
      client: "claude",
      approval,
    });
    expect(removed.installed).toBeFalse();
  });

  test("supports an isolated user-scoped Codex installation", async () => {
    const root = await createProject({});
    const userRoot = await createProject({});
    const approval = "skill:pkgshift:user:codex";
    const installed = await installSkill({
      projectRoot: root,
      sourcePath,
      scope: "user",
      client: "codex",
      userRoot,
      mode: "copy",
      approval,
    });
    expect(installed.healthy).toBeTrue();
    expect(installed.targetPath).toBe(`${userRoot}/.agents/skills/pkgshift`);

    const removed = await uninstallSkill({
      projectRoot: root,
      sourcePath,
      scope: "user",
      client: "codex",
      userRoot,
      approval,
    });
    expect(removed.installed).toBeFalse();
  });

  test("refuses a project skill destination through a symbolic-link parent", async () => {
    const root = await createProject({});
    const outside = await createProject({});
    await symlink(outside, join(root, ".agents"), "dir");

    await expect(installSkill({
      projectRoot: root,
      sourcePath,
      scope: "project",
      client: "codex",
      mode: "copy",
      approval: "skill:pkgshift:project:codex",
    })).rejects.toMatchObject({
      diagnostic: { code: "SKILL_TARGET_PATH_UNSAFE" },
    });
    expect(await Bun.file(join(outside, "skills", "pkgshift", "SKILL.md")).exists()).toBeFalse();
  });
});
