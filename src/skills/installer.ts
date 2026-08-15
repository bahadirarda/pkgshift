import { createHash } from "node:crypto";
import {
  cp,
  lstat,
  mkdir,
  readFile,
  readlink,
  readdir,
  rename,
  rm,
  symlink,
} from "node:fs/promises";
import { homedir } from "node:os";
import { basename, dirname, join, relative, resolve } from "node:path";
import { safeProjectFilePath } from "../core/project-path.ts";
import type { Diagnostic } from "../domain/models.ts";
import type {
  SkillInstallMode,
  SkillClient,
  SkillScope,
  SkillStatus,
} from "./models.ts";

const SKILL_NAME = "pkgshift";

export class SkillInstallerError extends Error {
  constructor(readonly diagnostic: Diagnostic) {
    super(diagnostic.summary);
    this.name = "SkillInstallerError";
  }
}

function error(code: string, summary: string, remediation: string[]): SkillInstallerError {
  return new SkillInstallerError({
    code,
    severity: "error",
    summary,
    blocking: true,
    remediation,
  });
}

async function state(path: string): Promise<Awaited<ReturnType<typeof lstat>> | null> {
  try {
    return await lstat(path);
  } catch (caught) {
    if ((caught as NodeJS.ErrnoException).code === "ENOENT") return null;
    throw caught;
  }
}

async function skillFiles(root: string): Promise<string[]> {
  const files: string[] = [];
  async function visit(directory: string): Promise<void> {
    const entries = await readdir(directory, { withFileTypes: true });
    entries.sort((left, right) => left.name.localeCompare(right.name));
    for (const entry of entries) {
      const absolutePath = join(directory, entry.name);
      if (entry.isDirectory()) await visit(absolutePath);
      else if (entry.isFile()) files.push(relative(root, absolutePath).replaceAll("\\", "/"));
      else throw error(
        "SKILL_PATH_TYPE_UNSAFE",
        `Skill content contains an unsupported path type: ${entry.name}`,
        ["Use only regular files and directories inside the portable skill source."],
      );
    }
  }
  await visit(root);
  return files;
}

async function directoryDigest(root: string): Promise<string> {
  const hash = createHash("sha256");
  for (const path of await skillFiles(root)) {
    hash.update(path);
    hash.update("\0");
    hash.update(await readFile(join(root, path)));
    hash.update("\0");
  }
  return `sha256:${hash.digest("hex")}`;
}

function targetRoot(
  projectRoot: string,
  scope: SkillScope,
  client: SkillClient,
  userRoot = homedir(),
): string {
  const directory = client === "codex" ? ".agents" : ".claude";
  return scope === "project"
    ? join(resolve(projectRoot), directory, "skills")
    : join(resolve(userRoot), directory, "skills");
}

function targetPath(
  projectRoot: string,
  scope: SkillScope,
  client: SkillClient,
  userRoot?: string,
): string {
  return join(targetRoot(projectRoot, scope, client, userRoot), SKILL_NAME);
}

async function sourceDiagnostics(sourcePath: string): Promise<Diagnostic[]> {
  const diagnostics: Diagnostic[] = [];
  const sourceState = await state(sourcePath);
  if (!sourceState?.isDirectory()) {
    diagnostics.push({
      code: "SKILL_SOURCE_NOT_FOUND",
      severity: "error",
      summary: `Portable skill source was not found: ${sourcePath}`,
      blocking: true,
      remediation: ["Run the command from a complete pkgshift distribution."],
    });
    return diagnostics;
  }
  const skillPath = join(sourcePath, "SKILL.md");
  try {
    const content = await readFile(skillPath, "utf8");
    const match = content.match(/^---\n([\s\S]*?)\n---(?:\n|$)/);
    const metadata = match?.[1] ? Bun.YAML.parse(match[1]) as Record<string, unknown> : null;
    if (
      !metadata
      || metadata.name !== SKILL_NAME
      || typeof metadata.description !== "string"
      || Object.keys(metadata).some((entry) => !["name", "description"].includes(entry))
    ) {
      throw new Error("SKILL.md frontmatter must contain only the expected name and description.");
    }
  } catch (caught) {
    diagnostics.push({
      code: "SKILL_SOURCE_INVALID",
      severity: "error",
      summary: caught instanceof Error ? caught.message : "SKILL.md validation failed.",
      blocking: true,
      remediation: ["Repair the portable skill source before installation."],
    });
  }
  return diagnostics;
}

export async function inspectSkill(options: {
  projectRoot: string;
  sourcePath: string;
  scope: SkillScope;
  client: SkillClient;
  userRoot?: string;
}): Promise<SkillStatus> {
  const sourcePath = resolve(options.sourcePath);
  const destination = targetPath(
    options.projectRoot,
    options.scope,
    options.client,
    options.userRoot,
  );
  const diagnostics = await sourceDiagnostics(sourcePath);
  const destinationRoot = options.scope === "project"
    ? resolve(options.projectRoot)
    : resolve(options.userRoot ?? homedir());
  const destinationRelative = relative(destinationRoot, destination).replaceAll("\\", "/");
  try {
    await safeProjectFilePath(destinationRoot, destinationRelative);
  } catch (caught) {
    diagnostics.push({
      code: "SKILL_TARGET_PATH_UNSAFE",
      severity: "error",
      summary: caught instanceof Error ? caught.message : `Unsafe Agent Skill destination: ${destination}`,
      blocking: true,
      remediation: ["Replace symbolic-link parent directories with a confined skill destination."],
    });
  }
  const sourceDigest = diagnostics.some((entry) => entry.blocking)
    ? null
    : await directoryDigest(sourcePath);
  const destinationState = diagnostics.some((entry) => entry.code === "SKILL_TARGET_PATH_UNSAFE")
    ? null
    : await state(destination);
  let mode: SkillInstallMode | null = null;
  let installedDigest: string | null = null;
  let installed = false;
  if (destinationState?.isSymbolicLink()) {
    mode = "link";
    const linked = resolve(dirname(destination), await readlink(destination));
    installed = linked === sourcePath;
    if (!installed) {
      diagnostics.push({
        code: "SKILL_INSTALL_CONFLICT",
        severity: "error",
        summary: `${destination} links to a different skill source.`,
        blocking: true,
        remediation: ["Move the conflicting installation before installing this skill."],
      });
    } else {
      installedDigest = sourceDigest;
    }
  } else if (destinationState?.isDirectory()) {
    mode = "copy";
    installed = true;
    installedDigest = await directoryDigest(destination);
  } else if (destinationState) {
    diagnostics.push({
      code: "SKILL_INSTALL_CONFLICT",
      severity: "error",
      summary: `${destination} is not a skill directory or symlink.`,
      blocking: true,
      remediation: ["Move the conflicting path before installing this skill."],
    });
  }
  const modified = installed
    && sourceDigest !== null
    && installedDigest !== null
    && sourceDigest !== installedDigest;
  if (modified) {
    diagnostics.push({
      code: "SKILL_INSTALL_MODIFIED",
      severity: "warning",
      summary: "The installed managed copy differs from the bundled portable skill.",
      blocking: false,
      remediation: ["Review local edits before updating or uninstalling the skill."],
    });
  }
  return {
    schemaVersion: "1.0",
    name: SKILL_NAME,
    client: options.client,
    scope: options.scope,
    sourcePath,
    targetPath: destination,
    sourceDigest,
    installedDigest,
    installed,
    mode,
    healthy: installed && !diagnostics.some((entry) => entry.blocking) && !modified,
    modified,
    diagnostics,
  };
}

export async function installSkill(options: {
  projectRoot: string;
  sourcePath: string;
  scope: SkillScope;
  client: SkillClient;
  userRoot?: string;
  mode: SkillInstallMode;
  approval: string | null;
}): Promise<SkillStatus> {
  const approval = `skill:${SKILL_NAME}:${options.scope}:${options.client}`;
  if (options.approval !== approval) {
    throw error("APPROVAL_REQUIRED", `Skill installation requires exact approval for ${approval}.`, [
      `Retry with --approve ${approval}.`,
    ]);
  }
  const before = await inspectSkill(options);
  if (before.diagnostics.some((entry) => entry.blocking)) {
    throw new SkillInstallerError(before.diagnostics.find((entry) => entry.blocking)!);
  }
  if (before.installed) {
    if (before.healthy && before.mode === options.mode) return before;
    throw error("SKILL_INSTALL_CONFLICT", "An existing skill installation cannot be replaced safely.", [
      "Review or uninstall the existing installation first.",
    ]);
  }
  await mkdir(dirname(before.targetPath), { recursive: true, mode: 0o700 });
  if (options.mode === "link") {
    await symlink(before.sourcePath, before.targetPath, "dir");
  } else {
    const temporary = join(
      dirname(before.targetPath),
      `.${basename(before.targetPath)}.${randomSuffix()}.tmp`,
    );
    try {
      await cp(before.sourcePath, temporary, { recursive: true, errorOnExist: true });
      await rename(temporary, before.targetPath);
    } catch (caught) {
      await rm(temporary, { recursive: true, force: true }).catch(() => undefined);
      throw caught;
    }
  }
  return inspectSkill(options);
}

export async function uninstallSkill(options: {
  projectRoot: string;
  sourcePath: string;
  scope: SkillScope;
  client: SkillClient;
  userRoot?: string;
  approval: string | null;
}): Promise<SkillStatus> {
  const approval = `skill:${SKILL_NAME}:${options.scope}:${options.client}`;
  if (options.approval !== approval) {
    throw error("APPROVAL_REQUIRED", `Skill uninstall requires exact approval for ${approval}.`, [
      `Retry with --approve ${approval}.`,
    ]);
  }
  const before = await inspectSkill(options);
  if (!before.installed) return before;
  if (before.mode === "copy" && before.sourceDigest === null) {
    throw error("SKILL_UNINSTALL_SOURCE_UNVERIFIED", "The managed copy cannot be compared with its portable source.", [
      "Restore the pkgshift skill source before uninstalling the managed copy.",
    ]);
  }
  if (before.modified) {
    throw error("SKILL_UNINSTALL_MODIFIED", "The installed skill contains local modifications.", [
      "Preserve or remove the local changes manually before uninstalling.",
    ]);
  }
  const destinationState = await state(before.targetPath);
  if (destinationState?.isSymbolicLink()) await rm(before.targetPath);
  else await rm(before.targetPath, { recursive: true });
  return inspectSkill(options);
}

function randomSuffix(): string {
  return crypto.randomUUID().replaceAll("-", "").slice(0, 12);
}
