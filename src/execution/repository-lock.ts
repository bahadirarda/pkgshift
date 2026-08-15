import { mkdir, open, readFile, stat, unlink } from "node:fs/promises";
import { join, resolve } from "node:path";
import { sha256Json } from "../core/files.ts";

export class RepositoryLockError extends Error {
  constructor(readonly code: string, message: string) {
    super(message);
    this.name = "RepositoryLockError";
  }
}

export class RepositoryLock {
  private released = false;

  private constructor(
    readonly path: string,
    private readonly handle: Awaited<ReturnType<typeof open>>,
  ) {}

  static async acquire(options: {
    stateDirectory: string;
    projectRoot: string;
    operation: "apply" | "verify" | "rollback";
  }): Promise<RepositoryLock> {
    const repositoryKey = `repo_${sha256Json({ root: resolve(options.projectRoot) }).slice(0, 24)}`;
    const directory = join(resolve(options.stateDirectory), "repositories", repositoryKey);
    const path = join(directory, "transaction.lock");
    await mkdir(directory, { recursive: true, mode: 0o700 });
    return RepositoryLock.open(path, options, true);
  }

  private static async open(
    path: string,
    options: {
      stateDirectory: string;
      projectRoot: string;
      operation: "apply" | "verify" | "rollback";
    },
    retry: boolean,
  ): Promise<RepositoryLock> {
    let handle: Awaited<ReturnType<typeof open>>;
    try {
      handle = await open(path, "wx", 0o600);
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code !== "EEXIST") throw error;
      if (retry && await RepositoryLock.removeOrphan(path)) {
        return RepositoryLock.open(path, options, false);
      }
      throw new RepositoryLockError(
        "REPOSITORY_TRANSACTION_BUSY",
        "Another migration transaction is active for this repository.",
      );
    }
    try {
      await handle.writeFile(`${JSON.stringify({
        pid: process.pid,
        operation: options.operation,
        projectRoot: resolve(options.projectRoot),
        createdAt: new Date().toISOString(),
      })}\n`);
      await handle.sync();
      return new RepositoryLock(path, handle);
    } catch (error) {
      await handle.close().catch(() => undefined);
      await unlink(path).catch(() => undefined);
      throw error;
    }
  }

  private static async removeOrphan(path: string): Promise<boolean> {
    try {
      const content = JSON.parse(await readFile(path, "utf8")) as { pid?: unknown };
      if (typeof content.pid === "number") {
        try {
          process.kill(content.pid, 0);
          return false;
        } catch (error) {
          if ((error as NodeJS.ErrnoException).code === "EPERM") return false;
          if ((error as NodeJS.ErrnoException).code === "ESRCH") {
            await unlink(path);
            return true;
          }
        }
      }
    } catch {
      const age = Date.now() - (await stat(path)).mtimeMs;
      if (age >= 300_000) {
        await unlink(path);
        return true;
      }
    }
    return false;
  }

  async release(): Promise<void> {
    if (this.released) return;
    this.released = true;
    await this.handle.close();
    await unlink(this.path).catch(() => undefined);
  }
}
