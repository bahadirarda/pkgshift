export interface SnapshotEntry {
  path: string;
  existed: boolean;
  digest: string | null;
  mode: number | null;
  backupPath: string | null;
}

export interface RecoverySnapshot {
  schemaVersion: "1.0";
  runId: string;
  createdAt: string;
  entries: SnapshotEntry[];
}

export interface SnapshotEnvelope {
  storeSchemaVersion: "1.0";
  digest: string;
  snapshot: RecoverySnapshot;
}
