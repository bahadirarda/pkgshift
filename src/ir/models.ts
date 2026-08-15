import type {
  Diagnostic,
  PackageManagerId,
} from "../domain/models.ts";

export type DependencySection =
  | "dependencies"
  | "devDependencies"
  | "optionalDependencies"
  | "peerDependencies";

export type DependencyProtocol =
  | "semver"
  | "tag"
  | "workspace"
  | "catalog"
  | "npm-alias"
  | "file"
  | "link"
  | "portal"
  | "patch"
  | "git"
  | "url"
  | "jsr"
  | "unknown";

export interface EvidenceReference {
  location: string;
  pointer?: string;
  detail: string;
}

export interface DependencyIR {
  packagePath: string;
  section: DependencySection;
  name: string;
  specifier: string;
  protocol: DependencyProtocol;
  evidence: EvidenceReference;
}

export interface PackageIR {
  path: string;
  manifestPath: string;
  name: string | null;
  version: string | null;
  private: boolean | null;
  dependencyCount: number;
  dependencies: DependencyIR[];
  scriptNames: string[];
}

export interface WorkspaceIR {
  configured: boolean;
  patterns: string[];
  packagePaths: string[];
  sources: EvidenceReference[];
}

export type PolicyKind =
  | "overrides"
  | "resolutions"
  | "package-extensions"
  | "patched-dependencies"
  | "catalog"
  | "catalogs"
  | "trusted-dependencies";

export interface PolicyIR {
  kind: PolicyKind;
  location: string;
  pointer: string;
  entries: number;
  nested: boolean;
}

export type FeatureId =
  | "workspace.manifest"
  | "workspace.negative-patterns"
  | "dependency.workspace-protocol"
  | "dependency.catalog-protocol"
  | "dependency.patch-protocol"
  | "dependency.portal-protocol"
  | "dependency.link-protocol"
  | "policy.catalogs"
  | "resolution.overrides"
  | "resolution.nested-overrides"
  | "resolution.resolutions"
  | "resolution.package-extensions"
  | "patch.patched-dependencies"
  | "install.pnp-linker"
  | "install.isolated-linker"
  | "policy.yarn-constraints"
  | "hook.pnpmfile"
  | "registry.npmrc"
  | "lifecycle.trusted-dependencies";

export interface ObservedFeature {
  id: FeatureId;
  count: number;
  evidence: EvidenceReference[];
}

export interface ProjectIR {
  schemaVersion: "1.0";
  projectIrId: string;
  repositoryFingerprint: string;
  source: PackageManagerId | null;
  rootPackagePath: string;
  packages: PackageIR[];
  workspace: WorkspaceIR;
  policies: PolicyIR[];
  features: ObservedFeature[];
  integrations: Array<{
    kind: string;
    path: string;
    packageManagerTokens: string[];
  }>;
  diagnostics: Diagnostic[];
}

