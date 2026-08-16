import type { CapabilityAnalysis } from "../capabilities/models.ts";
import type { MigrationPlan } from "../domain/models.ts";
import type { ProjectIR } from "../ir/models.ts";

export interface PlanArtifactBundle {
  schemaVersion: "1.0";
  plan: MigrationPlan;
  projectIr: ProjectIR;
  capabilityAnalysis: CapabilityAnalysis;
}

export interface StoredArtifactEnvelope<T> {
  storeSchemaVersion: "1.0";
  id: string;
  type: string;
  mediaType: string;
  createdAt: string;
  digest: string;
  content: T;
}

export interface StoredArtifactReference {
  id: string;
  type: string;
  digest: string;
  repositoryKey: string;
  relativePath: string;
}

