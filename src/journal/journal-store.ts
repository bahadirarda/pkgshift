import { open, mkdir, readFile, stat, unlink } from "node:fs/promises";
import { join, resolve } from "node:path";
import { atomicWriteJson } from "../core/atomic-json.ts";
import {
  pathExists,
  readJsonObject,
  sha256Json,
} from "../core/files.ts";
import type {
  JournalEnvelope,
  RunJournal,
} from "./models.ts";

const RUN_ID = /^run_[a-z0-9]+$/;

export class JournalStoreError extends Error {
  constructor(
    readonly code: string,
    message: string,
  ) {
    super(message);
    this.name = "JournalStoreError";
  }
}

function assertRunId(runId: string): void {
  if (!RUN_ID.test(runId)) {
    throw new JournalStoreError(
      "JOURNAL_RUN_ID_INVALID",
      `Run identifier is invalid: ${runId}`,
    );
  }
}

function envelopeFor(journal: RunJournal): JournalEnvelope {
  return {
    storeSchemaVersion: "1.0",
    digest: `sha256:${sha256Json(journal)}`,
    journal,
  };
}

function parseEnvelope(value: Record<string, unknown>): JournalEnvelope {
  if (
    value.storeSchemaVersion !== "1.0"
    || typeof value.digest !== "string"
    || !value.journal
    || typeof value.journal !== "object"
  ) {
    throw new JournalStoreError(
      "JOURNAL_INTEGRITY_FAILED",
      "Journal envelope is malformed.",
    );
  }
  const envelope = value as unknown as JournalEnvelope;
  if (`sha256:${sha256Json(envelope.journal)}` !== envelope.digest) {
    throw new JournalStoreError(
      "JOURNAL_INTEGRITY_FAILED",
      `Journal failed digest verification: ${envelope.journal.runId}`,
    );
  }
  return envelope;
}

export class JournalStore {
  readonly root: string;

  constructor(stateDirectory: string) {
    this.root = resolve(stateDirectory);
  }

  private runDirectory(runId: string): string {
    assertRunId(runId);
    return join(this.root, "runs", runId);
  }

  private journalPath(runId: string): string {
    return join(this.runDirectory(runId), "journal.json");
  }

  async create(journal: RunJournal): Promise<void> {
    const directory = this.runDirectory(journal.runId);
    try {
      await mkdir(directory, { recursive: false, mode: 0o700 });
    } catch (error) {
      if ((error as { code?: string }).code === "ENOENT") {
        await mkdir(join(this.root, "runs"), { recursive: true, mode: 0o700 });
        return this.create(journal);
      }
      if ((error as { code?: string }).code === "EEXIST") {
        throw new JournalStoreError(
          "JOURNAL_EXISTS",
          `Run journal already exists: ${journal.runId}`,
        );
      }
      throw error;
    }
    await atomicWriteJson(this.journalPath(journal.runId), envelopeFor(journal));
  }

  async load(runId: string): Promise<RunJournal> {
    let value: Record<string, unknown> | null;
    try {
      value = await readJsonObject(this.journalPath(runId));
    } catch {
      throw new JournalStoreError(
        "JOURNAL_INTEGRITY_FAILED",
        `Run journal is not valid JSON: ${runId}`,
      );
    }
    if (!value) {
      throw new JournalStoreError(
        "JOURNAL_NOT_FOUND",
        `Run journal was not found: ${runId}`,
      );
    }
    const envelope = parseEnvelope(value);
    if (envelope.journal.runId !== runId) {
      throw new JournalStoreError(
        "JOURNAL_INTEGRITY_FAILED",
        "Journal run identifier does not match its path.",
      );
    }
    return envelope.journal;
  }

  async update(
    journal: RunJournal,
    expectedRevision: number,
  ): Promise<void> {
    const path = this.journalPath(journal.runId);
    if (!(await pathExists(path))) {
      throw new JournalStoreError(
        "JOURNAL_NOT_FOUND",
        `Run journal was not found: ${journal.runId}`,
      );
    }
    const lockPath = `${path}.lock`;
    const lock = await this.acquireLock(lockPath, journal.runId);
    try {
      const current = await this.load(journal.runId);
      if (
        current.revision !== expectedRevision
        || journal.revision !== expectedRevision + 1
      ) {
        throw new JournalStoreError(
          "JOURNAL_REVISION_CONFLICT",
          `Expected revision ${expectedRevision}, found ${current.revision}, received ${journal.revision}.`,
        );
      }
      await atomicWriteJson(path, envelopeFor(journal));
    } finally {
      await lock?.close();
      await unlink(lockPath).catch(() => undefined);
    }
  }

  private async acquireLock(
    lockPath: string,
    runId: string,
    retry = true,
  ): Promise<Awaited<ReturnType<typeof open>>> {
    let handle: Awaited<ReturnType<typeof open>>;
    try {
      handle = await open(lockPath, "wx", 0o600);
    } catch (error) {
      if ((error as { code?: string }).code !== "EEXIST") throw error;
      if (retry && await this.removeStaleLock(lockPath)) {
        return this.acquireLock(lockPath, runId, false);
      }
      throw new JournalStoreError(
        "JOURNAL_WRITE_CONFLICT",
        `Run journal is being updated: ${runId}`,
      );
    }
    try {
      await handle.writeFile(`${JSON.stringify({ pid: process.pid, createdAt: new Date().toISOString() })}\n`);
      await handle.sync();
      return handle;
    } catch (error) {
      await handle.close().catch(() => undefined);
      await unlink(lockPath).catch(() => undefined);
      throw error;
    }
  }

  private async removeStaleLock(lockPath: string): Promise<boolean> {
    try {
      const content = JSON.parse(await readFile(lockPath, "utf8")) as { pid?: unknown };
      if (typeof content.pid === "number") {
        try {
          process.kill(content.pid, 0);
          return false;
        } catch (error) {
          if ((error as NodeJS.ErrnoException).code === "EPERM") return false;
          if ((error as NodeJS.ErrnoException).code === "ESRCH") {
            await unlink(lockPath);
            return true;
          }
        }
      }
    } catch {
      const age = Date.now() - (await stat(lockPath)).mtimeMs;
      if (age < 300_000) return false;
    }
    return false;
  }
}
