import {
  chmod,
  lstat,
  mkdir,
  readFile,
  unlink,
} from "node:fs/promises";
import { join, resolve } from "node:path";
import { atomicWriteFile, atomicWriteJson } from "../core/atomic-json.ts";
import { readJsonObject, sha256Json, sha256Text } from "../core/files.ts";
import { safeProjectFilePath } from "../core/project-path.ts";
import type {
  RecoverySnapshot,
  SnapshotEnvelope,
  SnapshotEntry,
} from "./models.ts";

export class SnapshotStoreError extends Error {
  constructor(readonly code: string, message: string) {
    super(message);
    this.name = "SnapshotStoreError";
  }
}

function assertRunId(runId: string): void {
  if (!/^run_[a-z0-9]+$/.test(runId)) {
    throw new SnapshotStoreError("SNAPSHOT_RUN_ID_INVALID", `Invalid run identifier: ${runId}`);
  }
}

async function fileState(path: string): Promise<Awaited<ReturnType<typeof lstat>> | null> {
  try {
    return await lstat(path);
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === "ENOENT") return null;
    throw error;
  }
}

function envelope(snapshot: RecoverySnapshot): SnapshotEnvelope {
  return {
    storeSchemaVersion: "1.0",
    digest: `sha256:${sha256Json(snapshot)}`,
    snapshot,
  };
}

export class SnapshotStore {
  readonly root: string;

  constructor(stateDirectory: string) {
    this.root = resolve(stateDirectory);
  }

  private runDirectory(runId: string): string {
    assertRunId(runId);
    return join(this.root, "runs", runId);
  }

  private manifestPath(runId: string): string {
    return join(this.runDirectory(runId), "snapshot.json");
  }

  async create(
    runId: string,
    projectRoot: string,
    paths: string[],
    createdAt = new Date().toISOString(),
  ): Promise<RecoverySnapshot> {
    const directory = this.runDirectory(runId);
    await mkdir(join(directory, "snapshots"), { recursive: true, mode: 0o700 });
    const entries: SnapshotEntry[] = [];
    const normalizedPaths = [...new Set(paths)].sort();
    for (let index = 0; index < normalizedPaths.length; index += 1) {
      const path = normalizedPaths[index]!;
      let absolutePath: string;
      try {
        absolutePath = await safeProjectFilePath(projectRoot, path);
      } catch (error) {
        throw new SnapshotStoreError(
          "SNAPSHOT_PATH_UNSAFE",
          error instanceof Error ? error.message : `Unsafe snapshot path: ${path}`,
        );
      }
      const state = await fileState(absolutePath);
      if (!state) {
        entries.push({ path, existed: false, digest: null, mode: null, backupPath: null });
        continue;
      }
      if (state.isSymbolicLink() || !state.isFile()) {
        throw new SnapshotStoreError(
          "SNAPSHOT_PATH_TYPE_UNSAFE",
          `Snapshot targets must be regular files: ${path}`,
        );
      }
      const content = await readFile(absolutePath);
      const backupPath = `snapshots/${String(index + 1).padStart(4, "0")}.bin`;
      await atomicWriteFile(join(directory, backupPath), content, 0o600);
      entries.push({
        path,
        existed: true,
        digest: sha256Text(content),
        mode: Number(state.mode) & 0o777,
        backupPath,
      });
    }
    const snapshot: RecoverySnapshot = {
      schemaVersion: "1.0",
      runId,
      createdAt,
      entries,
    };
    await atomicWriteJson(this.manifestPath(runId), envelope(snapshot));
    return snapshot;
  }

  async load(runId: string): Promise<RecoverySnapshot> {
    let value: Record<string, unknown> | null;
    try {
      value = await readJsonObject(this.manifestPath(runId));
    } catch {
      throw new SnapshotStoreError("SNAPSHOT_INTEGRITY_FAILED", "Snapshot manifest is not valid JSON.");
    }
    if (!value || value.storeSchemaVersion !== "1.0" || typeof value.digest !== "string") {
      throw new SnapshotStoreError("SNAPSHOT_INTEGRITY_FAILED", "Snapshot envelope is missing or malformed.");
    }
    const snapshot = value.snapshot as RecoverySnapshot | undefined;
    if (!snapshot || snapshot.runId !== runId || !Array.isArray(snapshot.entries)) {
      throw new SnapshotStoreError("SNAPSHOT_INTEGRITY_FAILED", "Snapshot identity is malformed.");
    }
    if (`sha256:${sha256Json(snapshot)}` !== value.digest) {
      throw new SnapshotStoreError("SNAPSHOT_INTEGRITY_FAILED", "Snapshot manifest failed digest verification.");
    }
    return snapshot;
  }

  async restore(runId: string, projectRoot: string): Promise<RecoverySnapshot> {
    const snapshot = await this.load(runId);
    const directory = this.runDirectory(runId);
    for (const entry of snapshot.entries) {
      let absolutePath: string;
      try {
        absolutePath = await safeProjectFilePath(projectRoot, entry.path);
      } catch (error) {
        throw new SnapshotStoreError(
          "SNAPSHOT_RESTORE_PATH_UNSAFE",
          error instanceof Error ? error.message : `Unsafe restore path: ${entry.path}`,
        );
      }
      const state = await fileState(absolutePath);
      if (state?.isSymbolicLink() || (state && !state.isFile())) {
        throw new SnapshotStoreError(
          "SNAPSHOT_RESTORE_PATH_UNSAFE",
          `Restore target is not a regular file: ${entry.path}`,
        );
      }
      if (!entry.existed) {
        if (state) await unlink(absolutePath);
        continue;
      }
      if (!entry.backupPath || !entry.digest) {
        throw new SnapshotStoreError("SNAPSHOT_INTEGRITY_FAILED", `Backup metadata is missing for ${entry.path}.`);
      }
      if (!/^snapshots\/\d{4,}\.bin$/.test(entry.backupPath)) {
        throw new SnapshotStoreError("SNAPSHOT_INTEGRITY_FAILED", `Backup path is invalid for ${entry.path}.`);
      }
      const content = await readFile(join(directory, entry.backupPath));
      if (sha256Text(content) !== entry.digest) {
        throw new SnapshotStoreError("SNAPSHOT_INTEGRITY_FAILED", `Backup failed digest verification: ${entry.path}`);
      }
      await atomicWriteFile(absolutePath, content, entry.mode ?? 0o644);
      if (entry.mode !== null) await chmod(absolutePath, entry.mode);
    }
    return snapshot;
  }
}
