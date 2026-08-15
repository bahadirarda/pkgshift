import { resolve, relative, join } from "node:path";
import { atomicWriteJson } from "../core/atomic-json.ts";
import {
  pathExists,
  readJsonObject,
  sha256Json,
} from "../core/files.ts";
import type {
  PlanArtifactBundle,
  StoredArtifactEnvelope,
  StoredArtifactReference,
} from "./models.ts";

const ARTIFACT_ID = /^[a-z][a-z0-9-]*_[a-z0-9]+$/;

export class ArtifactStoreError extends Error {
  constructor(
    readonly code: string,
    message: string,
  ) {
    super(message);
    this.name = "ArtifactStoreError";
  }
}

function assertArtifactId(id: string): void {
  if (!ARTIFACT_ID.test(id)) {
    throw new ArtifactStoreError(
      "ARTIFACT_ID_INVALID",
      `Artifact identifier is invalid: ${id}`,
    );
  }
}

function artifactDigest(
  id: string,
  type: string,
  mediaType: string,
  content: unknown,
): string {
  return `sha256:${sha256Json({ id, type, mediaType, content })}`;
}

function parseEnvelope(
  value: Record<string, unknown>,
): StoredArtifactEnvelope<PlanArtifactBundle> {
  if (
    value.storeSchemaVersion !== "1.0"
    || typeof value.id !== "string"
    || typeof value.type !== "string"
    || typeof value.mediaType !== "string"
    || typeof value.createdAt !== "string"
    || typeof value.digest !== "string"
    || !value.content
    || typeof value.content !== "object"
  ) {
    throw new ArtifactStoreError(
      "ARTIFACT_INTEGRITY_FAILED",
      "Stored artifact envelope is malformed.",
    );
  }
  return value as unknown as StoredArtifactEnvelope<PlanArtifactBundle>;
}

export class PlanArtifactStore {
  readonly root: string;

  constructor(stateDirectory: string) {
    this.root = resolve(stateDirectory);
  }

  repositoryKey(projectRoot: string): string {
    return `repo_${sha256Json({ root: resolve(projectRoot) }).slice(0, 24)}`;
  }

  private artifactPath(projectRoot: string, planId: string): string {
    assertArtifactId(planId);
    return join(
      this.root,
      "repositories",
      this.repositoryKey(projectRoot),
      "plans",
      `${planId}.json`,
    );
  }

  async save(
    projectRoot: string,
    bundle: PlanArtifactBundle,
    createdAt = new Date().toISOString(),
  ): Promise<StoredArtifactReference> {
    const planId = bundle.plan.planId;
    assertArtifactId(planId);
    if (
      bundle.schemaVersion !== "1.0"
      || bundle.projectIr.projectIrId !== bundle.plan.projectIrId
      || bundle.capabilityAnalysis.analysisId !== bundle.plan.capabilityAnalysisId
      || bundle.capabilityAnalysis.projectIrId !== bundle.projectIr.projectIrId
    ) {
      throw new ArtifactStoreError(
        "ARTIFACT_BUNDLE_INVALID",
        "Plan, Project IR, and capability analysis identifiers do not agree.",
      );
    }
    const type = "package-manager-plan-bundle";
    const mediaType = "application/vnd.pkgshift.plan-bundle+json";
    const digest = artifactDigest(planId, type, mediaType, bundle);
    const envelope: StoredArtifactEnvelope<PlanArtifactBundle> = {
      storeSchemaVersion: "1.0",
      id: planId,
      type,
      mediaType,
      createdAt,
      digest,
      content: bundle,
    };
    const path = this.artifactPath(projectRoot, planId);
    if (await pathExists(path)) {
      const existing = await this.load(projectRoot, planId);
      if (existing.digest !== digest) {
        throw new ArtifactStoreError(
          "ARTIFACT_COLLISION",
          `Stored artifact ${planId} has different content.`,
        );
      }
      return {
        id: existing.id,
        type: existing.type,
        digest: existing.digest,
        repositoryKey: this.repositoryKey(projectRoot),
        relativePath: relative(this.root, path).replaceAll("\\", "/"),
      };
    }
    await atomicWriteJson(path, envelope);
    return {
      id: planId,
      type,
      digest,
      repositoryKey: this.repositoryKey(projectRoot),
      relativePath: relative(this.root, path).replaceAll("\\", "/"),
    };
  }

  async load(
    projectRoot: string,
    planId: string,
  ): Promise<StoredArtifactEnvelope<PlanArtifactBundle>> {
    const path = this.artifactPath(projectRoot, planId);
    let value: Record<string, unknown> | null;
    try {
      value = await readJsonObject(path);
    } catch {
      throw new ArtifactStoreError(
        "ARTIFACT_INTEGRITY_FAILED",
        `Stored artifact is not valid JSON: ${planId}`,
      );
    }
    if (!value) {
      throw new ArtifactStoreError(
        "ARTIFACT_NOT_FOUND",
        `Plan artifact was not found: ${planId}`,
      );
    }
    const envelope = parseEnvelope(value);
    if (envelope.id !== planId || envelope.type !== "package-manager-plan-bundle") {
      throw new ArtifactStoreError(
        "ARTIFACT_INTEGRITY_FAILED",
        "Stored artifact identity does not match its path.",
      );
    }
    const digest = artifactDigest(
      envelope.id,
      envelope.type,
      envelope.mediaType,
      envelope.content,
    );
    if (digest !== envelope.digest) {
      throw new ArtifactStoreError(
        "ARTIFACT_INTEGRITY_FAILED",
        `Stored artifact failed digest verification: ${planId}`,
      );
    }
    return envelope;
  }
}
