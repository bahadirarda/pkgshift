import type { Diagnostic } from "../domain/models.ts";

export type SkillScope = "project" | "user";
export type SkillInstallMode = "copy" | "link";
export type SkillClient = "codex" | "claude";

export interface SkillStatus {
  schemaVersion: "1.0";
  name: string;
  client: SkillClient;
  scope: SkillScope;
  sourcePath: string;
  targetPath: string;
  sourceDigest: string | null;
  installedDigest: string | null;
  installed: boolean;
  mode: SkillInstallMode | null;
  healthy: boolean;
  modified: boolean;
  diagnostics: Diagnostic[];
}
