import { lstat } from "node:fs/promises";
import { isAbsolute, join, posix, relative, resolve } from "node:path";

export class UnsafeProjectPathError extends Error {
  constructor(readonly path: string, message: string) {
    super(message);
    this.name = "UnsafeProjectPathError";
  }
}

async function pathState(path: string): Promise<Awaited<ReturnType<typeof lstat>> | null> {
  try {
    return await lstat(path);
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === "ENOENT") return null;
    throw error;
  }
}

export async function safeProjectFilePath(
  projectRoot: string,
  relativePath: string,
): Promise<string> {
  const normalized = posix.normalize(relativePath);
  if (
    !relativePath
    || relativePath.includes("\\")
    || posix.isAbsolute(relativePath)
    || normalized !== relativePath
    || normalized === "."
    || normalized === ".."
    || normalized.startsWith("../")
  ) {
    throw new UnsafeProjectPathError(relativePath, `Unsafe project-relative path: ${relativePath}`);
  }

  const root = resolve(projectRoot);
  const absolutePath = resolve(root, ...relativePath.split("/"));
  const confinedPath = relative(root, absolutePath);
  if (!confinedPath || confinedPath.startsWith("..") || isAbsolute(confinedPath)) {
    throw new UnsafeProjectPathError(relativePath, `Path is outside the project root: ${relativePath}`);
  }

  let current = root;
  for (const segment of relativePath.split("/").slice(0, -1)) {
    current = join(current, segment);
    const state = await pathState(current);
    if (!state) break;
    if (state.isSymbolicLink()) {
      throw new UnsafeProjectPathError(
        relativePath,
        `Project path traverses a symbolic link: ${relativePath}`,
      );
    }
    if (!state.isDirectory()) {
      throw new UnsafeProjectPathError(
        relativePath,
        `Project path parent is not a directory: ${relativePath}`,
      );
    }
  }
  return absolutePath;
}
