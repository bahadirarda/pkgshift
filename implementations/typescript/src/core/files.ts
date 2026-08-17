import { createHash } from "node:crypto";
import { readdir, readFile, stat } from "node:fs/promises";
import { join, relative } from "node:path";
import { redactSensitiveText } from "./redaction.ts";

export async function pathExists(path: string): Promise<boolean> {
  try {
    await stat(path);
    return true;
  } catch {
    return false;
  }
}

export async function directoryExists(path: string): Promise<boolean> {
  try {
    return (await stat(path)).isDirectory();
  } catch {
    return false;
  }
}

export async function readText(path: string): Promise<string | null> {
  try {
    return await readFile(path, "utf8");
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === "ENOENT") {
      return null;
    }
    throw error;
  }
}

export async function readJsonObject(
  path: string,
): Promise<Record<string, unknown> | null> {
  const text = await readText(path);
  if (text === null) {
    return null;
  }
  const normalized = text.startsWith("\uFEFF") ? text.slice(1) : text;
  const parsed: unknown = JSON.parse(normalized);
  if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
    throw new Error(`${path} does not contain a JSON object`);
  }
  return parsed as Record<string, unknown>;
}

const IGNORED_DIRECTORIES = new Set([
  ".git",
  ".pkgshift",
  "coverage",
  "dist",
  "node_modules",
]);

export async function walkFiles(
  root: string,
  accept: (relativePath: string) => boolean,
): Promise<string[]> {
  const output: string[] = [];

  async function visit(directory: string): Promise<void> {
    const entries = await readdir(directory, { withFileTypes: true });
    entries.sort((left, right) => left.name.localeCompare(right.name));
    for (const entry of entries) {
      if (entry.isDirectory() && IGNORED_DIRECTORIES.has(entry.name)) {
        continue;
      }
      const absolutePath = join(directory, entry.name);
      if (entry.isDirectory()) {
        await visit(absolutePath);
        continue;
      }
      if (!entry.isFile()) {
        continue;
      }
      const relativePath = relative(root, absolutePath).replaceAll("\\", "/");
      if (accept(relativePath)) {
        output.push(relativePath);
      }
    }
  }

  await visit(root);
  return output;
}

export async function fingerprintFiles(
  root: string,
  relativePaths: string[],
): Promise<string> {
  const hash = createHash("sha256");
  const uniquePaths = [...new Set(relativePaths)].sort();
  for (const relativePath of uniquePaths) {
    const content = await readText(join(root, relativePath));
    if (content === null) {
      continue;
    }
    hash.update(relativePath);
    hash.update("\0");
    hash.update(redactSensitiveText(content));
    hash.update("\0");
  }
  return `sha256:${hash.digest("hex")}`;
}

export function sha256Json(value: unknown): string {
  const content = JSON.stringify(value);
  return createHash("sha256").update(content).digest("hex");
}

export function sha256Text(value: string | Uint8Array): string {
  return `sha256:${createHash("sha256").update(value).digest("hex")}`;
}
