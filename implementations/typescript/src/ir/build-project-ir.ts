import { dirname, join } from "node:path";
import { matchesWorkspacePatterns } from "../core/glob.ts";
import { parseJsoncObject } from "../core/jsonc.ts";
import {
  pathExists,
  readJsonObject,
  readText,
  sha256Json,
} from "../core/files.ts";
import { redactSensitiveText } from "../core/redaction.ts";
import type {
  Diagnostic,
  ProjectInspection,
} from "../domain/models.ts";
import type {
  DependencyIR,
  DependencyProtocol,
  DependencySection,
  EvidenceReference,
  FeatureId,
  ObservedFeature,
  PackageIR,
  PolicyIR,
  PolicyKind,
  ProjectIR,
  WorkspaceIR,
} from "./models.ts";

const DEPENDENCY_SECTIONS: DependencySection[] = [
  "dependencies",
  "devDependencies",
  "optionalDependencies",
  "peerDependencies",
];

function isObject(value: unknown): value is Record<string, unknown> {
  return Boolean(value) && typeof value === "object" && !Array.isArray(value);
}

function stringArray(value: unknown): string[] {
  return Array.isArray(value)
    ? value.filter((entry): entry is string => typeof entry === "string")
    : [];
}

function workspacePatternsFromManifest(
  manifest: Record<string, unknown>,
): string[] {
  if (Array.isArray(manifest.workspaces)) {
    return stringArray(manifest.workspaces);
  }
  if (isObject(manifest.workspaces)) {
    return stringArray(manifest.workspaces.packages);
  }
  return [];
}

function workspacePatternSupported(pattern: string): boolean {
  const candidate = pattern.trim().replace(/^!/, "");
  return Boolean(candidate)
    && !candidate.startsWith("/")
    && !candidate.includes("\\")
    && !candidate.split("/").includes("..")
    && !/[\[\]{}()]/.test(candidate);
}

function classifyDependencyProtocol(specifier: string): DependencyProtocol {
  const normalized = specifier.trim();
  if (normalized.startsWith("workspace:")) return "workspace";
  if (normalized.startsWith("catalog:")) return "catalog";
  if (normalized.startsWith("patch:")) return "patch";
  if (normalized.startsWith("portal:")) return "portal";
  if (normalized.startsWith("link:")) return "link";
  if (normalized.startsWith("file:")) return "file";
  if (normalized.startsWith("npm:")) return "npm-alias";
  if (normalized.startsWith("jsr:")) return "jsr";
  if (/^(?:git\+|git@|github:|gitlab:|bitbucket:|ssh:)/i.test(normalized)) return "git";
  if (/^https?:\/\//i.test(normalized)) return "url";
  if (/^(?:[~^<>=*]|\d|v\d)/.test(normalized)) return "semver";
  if (/^[a-z][a-z0-9._-]*$/i.test(normalized)) return "tag";
  return "unknown";
}

function safeSpecifier(specifier: string): string {
  return redactSensitiveText(specifier);
}

function buildPackage(
  manifestPath: string,
  manifest: Record<string, unknown>,
  diagnostics: Diagnostic[],
): PackageIR {
  const packagePath = manifestPath === "package.json"
    ? "."
    : dirname(manifestPath).replaceAll("\\", "/");
  const dependencies: DependencyIR[] = [];
  for (const section of DEPENDENCY_SECTIONS) {
    const values = manifest[section];
    if (values === undefined) {
      continue;
    }
    if (!isObject(values)) {
      diagnostics.push({
        code: "DEPENDENCY_SECTION_INVALID",
        severity: "error",
        summary: `${manifestPath} has a non-object ${section} field.`,
        blocking: true,
        evidence: [{ location: manifestPath, detail: `${section} must be a JSON object` }],
        remediation: ["Repair the dependency section before planning a migration."],
      });
      continue;
    }
    for (const [name, value] of Object.entries(values).sort(([left], [right]) => left.localeCompare(right))) {
      if (typeof value !== "string") {
        diagnostics.push({
          code: "DEPENDENCY_SPECIFIER_INVALID",
          severity: "error",
          summary: `${manifestPath} contains a non-string dependency specifier for ${name}.`,
          blocking: true,
          evidence: [{ location: manifestPath, detail: `${section}.${name} must be a string` }],
          remediation: ["Replace the dependency value with a supported string specifier."],
        });
        continue;
      }
      const specifier = safeSpecifier(value);
      dependencies.push({
        packagePath,
        section,
        name,
        specifier,
        protocol: classifyDependencyProtocol(specifier),
        evidence: {
          location: manifestPath,
          pointer: `/${section}/${name.replaceAll("~", "~0").replaceAll("/", "~1")}`,
          detail: `${name} uses ${classifyDependencyProtocol(specifier)} protocol`,
        },
      });
    }
  }

  const scripts = isObject(manifest.scripts)
    ? Object.keys(manifest.scripts).sort()
    : [];
  return {
    path: packagePath,
    manifestPath,
    name: typeof manifest.name === "string" ? manifest.name : null,
    version: typeof manifest.version === "string" ? manifest.version : null,
    private: typeof manifest.private === "boolean" ? manifest.private : null,
    dependencyCount: dependencies.length,
    dependencies,
    scriptNames: scripts,
  };
}

function entryCount(value: unknown): number {
  if (Array.isArray(value)) return value.length;
  if (isObject(value)) return Object.keys(value).length;
  return value === undefined || value === null ? 0 : 1;
}

function containsNestedObject(value: unknown): boolean {
  return isObject(value) && Object.values(value).some((entry) => isObject(entry));
}

function addPolicy(
  policies: PolicyIR[],
  kind: PolicyKind,
  location: string,
  pointer: string,
  value: unknown,
): void {
  if (value === undefined || value === null) {
    return;
  }
  policies.push({
    kind,
    location,
    pointer,
    entries: entryCount(value),
    nested: containsNestedObject(value),
  });
}

async function readYamlConfiguration(
  root: string,
  location: string,
  diagnostics: Diagnostic[],
): Promise<Record<string, unknown> | null> {
  const content = await readText(join(root, location));
  if (content === null) {
    return null;
  }
  try {
    const parsed: unknown = Bun.YAML.parse(content);
    if (!isObject(parsed)) {
      throw new Error("Configuration root must be a mapping");
    }
    return parsed;
  } catch (error) {
    diagnostics.push({
      code: "CONFIGURATION_PARSE_FAILED",
      severity: "error",
      summary: `${location} could not be parsed safely.`,
      blocking: true,
      evidence: [{
        location,
        detail: error instanceof Error ? error.message : "YAML parsing failed",
      }],
      remediation: ["Repair the configuration before planning a migration."],
    });
    return null;
  }
}

async function readDenoConfiguration(
  root: string,
  diagnostics: Diagnostic[],
): Promise<{ location: string; value: Record<string, unknown> } | null> {
  const location = await pathExists(join(root, "deno.json"))
    ? "deno.json"
    : await pathExists(join(root, "deno.jsonc"))
      ? "deno.jsonc"
      : null;
  if (!location) return null;
  const content = await readText(join(root, location));
  if (content === null) return null;
  try {
    return {
      location,
      value: location.endsWith(".jsonc")
        ? parseJsoncObject(content)
        : JSON.parse(content) as Record<string, unknown>,
    };
  } catch (error) {
    diagnostics.push({
      code: "CONFIGURATION_PARSE_FAILED",
      severity: "error",
      summary: `${location} could not be parsed safely.`,
      blocking: true,
      evidence: [{
        location,
        detail: error instanceof Error ? error.message : "JSON parsing failed",
      }],
      remediation: ["Repair the configuration before planning a migration."],
    });
    return null;
  }
}

function addObservedFeature(
  features: Map<FeatureId, ObservedFeature>,
  id: FeatureId,
  evidence: EvidenceReference,
  increment = 1,
): void {
  const current = features.get(id) ?? { id, count: 0, evidence: [] };
  current.count += increment;
  const evidenceKey = `${evidence.location}\0${evidence.pointer ?? ""}\0${evidence.detail}`;
  const exists = current.evidence.some((item) =>
    `${item.location}\0${item.pointer ?? ""}\0${item.detail}` === evidenceKey,
  );
  if (!exists && current.evidence.length < 50) {
    current.evidence.push(evidence);
  }
  features.set(id, current);
}

function policyEvidence(policy: PolicyIR): EvidenceReference {
  return {
    location: policy.location,
    pointer: policy.pointer,
    detail: `${policy.kind} contains ${policy.entries} top-level entries`,
  };
}

export async function buildProjectIR(
  inspection: ProjectInspection,
): Promise<ProjectIR | null> {
  if (!inspection.manifest) {
    return null;
  }

  const diagnostics = [...inspection.diagnostics];
  const rootManifest = await readJsonObject(join(inspection.root, "package.json"));
  if (!rootManifest) {
    return null;
  }
  const pnpmConfiguration = await readYamlConfiguration(
    inspection.root,
    "pnpm-workspace.yaml",
    diagnostics,
  );
  const yarnConfiguration = await readYamlConfiguration(
    inspection.root,
    ".yarnrc.yml",
    diagnostics,
  );
  const denoConfiguration = await readDenoConfiguration(inspection.root, diagnostics);

  const workspaceSources: EvidenceReference[] = [];
  const patterns: string[] = [];
  const addPatterns = (location: string, values: string[]): void => {
    if (values.length === 0) return;
    patterns.push(...values);
    workspaceSources.push({
      location,
      detail: `Workspace membership defines ${values.length} patterns`,
    });
  };

  addPatterns("package.json", workspacePatternsFromManifest(rootManifest));
  if (pnpmConfiguration) {
    addPatterns("pnpm-workspace.yaml", stringArray(pnpmConfiguration.packages));
  }
  if (denoConfiguration) {
    addPatterns(denoConfiguration.location, stringArray(denoConfiguration.value.workspace));
  }

  const normalizedPatterns = [...new Set(
    patterns.map((pattern) => pattern.trim()).filter(Boolean),
  )];
  for (const pattern of normalizedPatterns.filter((entry) => !workspacePatternSupported(entry))) {
    diagnostics.push({
      code: "WORKSPACE_PATTERN_UNSUPPORTED",
      severity: "error",
      summary: `Workspace pattern is outside the deterministic glob subset: ${pattern}`,
      blocking: true,
      evidence: [{
        location: workspaceSources[0]?.location ?? "package.json",
        detail: "MVP workspace matching supports literal segments, *, **, ?, and a leading exclusion marker.",
      }],
      remediation: ["Expand the pattern into supported deterministic workspace entries before planning."],
    });
  }
  const manifestPaths = inspection.relevantFiles
    .filter((path) => path === "package.json" || path.endsWith("/package.json"))
    .sort();
  const selectedManifestPaths = manifestPaths.filter((manifestPath) => {
    if (manifestPath === "package.json") return true;
    return matchesWorkspacePatterns(dirname(manifestPath).replaceAll("\\", "/"), normalizedPatterns);
  });

  const packages: PackageIR[] = [];
  for (const manifestPath of selectedManifestPaths) {
    try {
      const manifest = await readJsonObject(join(inspection.root, manifestPath));
      if (manifest) {
        packages.push(buildPackage(manifestPath, manifest, diagnostics));
      }
    } catch (error) {
      diagnostics.push({
        code: "WORKSPACE_MANIFEST_INVALID",
        severity: "error",
        summary: `${manifestPath} could not be parsed as a package manifest.`,
        blocking: true,
        evidence: [{
          location: manifestPath,
          detail: error instanceof Error ? error.message : "JSON parsing failed",
        }],
        remediation: ["Repair the workspace manifest before planning a migration."],
      });
    }
  }
  packages.sort((left, right) => left.path.localeCompare(right.path));

  const policies: PolicyIR[] = [];
  addPolicy(policies, "overrides", "package.json", "/overrides", rootManifest.overrides);
  addPolicy(policies, "resolutions", "package.json", "/resolutions", rootManifest.resolutions);
  addPolicy(policies, "package-extensions", "package.json", "/packageExtensions", rootManifest.packageExtensions);
  addPolicy(policies, "patched-dependencies", "package.json", "/patchedDependencies", rootManifest.patchedDependencies);
  addPolicy(policies, "trusted-dependencies", "package.json", "/trustedDependencies", rootManifest.trustedDependencies);
  if (yarnConfiguration?.enableScripts === false && isObject(rootManifest.dependenciesMeta)) {
    const builtDependencies = Object.fromEntries(
      Object.entries(rootManifest.dependenciesMeta)
        .filter((entry) => isObject(entry[1]) && entry[1].built === true),
    );
    addPolicy(
      policies,
      "trusted-dependencies",
      "package.json",
      "/dependenciesMeta",
      builtDependencies,
    );
  }

  if (isObject(rootManifest.pnpm)) {
    addPolicy(policies, "overrides", "package.json", "/pnpm/overrides", rootManifest.pnpm.overrides);
    addPolicy(policies, "package-extensions", "package.json", "/pnpm/packageExtensions", rootManifest.pnpm.packageExtensions);
    addPolicy(policies, "patched-dependencies", "package.json", "/pnpm/patchedDependencies", rootManifest.pnpm.patchedDependencies);
    addPolicy(policies, "trusted-dependencies", "package.json", "/pnpm/onlyBuiltDependencies", rootManifest.pnpm.onlyBuiltDependencies);
  }
  if (pnpmConfiguration) {
    addPolicy(policies, "overrides", "pnpm-workspace.yaml", "/overrides", pnpmConfiguration.overrides);
    addPolicy(policies, "package-extensions", "pnpm-workspace.yaml", "/packageExtensions", pnpmConfiguration.packageExtensions);
    addPolicy(policies, "patched-dependencies", "pnpm-workspace.yaml", "/patchedDependencies", pnpmConfiguration.patchedDependencies);
    addPolicy(policies, "catalog", "pnpm-workspace.yaml", "/catalog", pnpmConfiguration.catalog);
    addPolicy(policies, "catalogs", "pnpm-workspace.yaml", "/catalogs", pnpmConfiguration.catalogs);
    addPolicy(policies, "trusted-dependencies", "pnpm-workspace.yaml", "/onlyBuiltDependencies", pnpmConfiguration.onlyBuiltDependencies);
    addPolicy(policies, "trusted-dependencies", "pnpm-workspace.yaml", "/allowBuilds", pnpmConfiguration.allowBuilds);
  }
  if (isObject(rootManifest.workspaces)) {
    addPolicy(policies, "catalog", "package.json", "/workspaces/catalog", rootManifest.workspaces.catalog);
    addPolicy(policies, "catalogs", "package.json", "/workspaces/catalogs", rootManifest.workspaces.catalogs);
  }
  policies.sort((left, right) =>
    left.location.localeCompare(right.location) || left.pointer.localeCompare(right.pointer),
  );

  const features = new Map<FeatureId, ObservedFeature>();
  if (normalizedPatterns.length > 0) {
    addObservedFeature(features, "workspace.manifest", {
      location: workspaceSources[0]?.location ?? "package.json",
      detail: `${packages.length} packages are represented by workspace membership`,
    }, packages.length);
  }
  for (const pattern of normalizedPatterns.filter((pattern) => pattern.startsWith("!"))) {
    addObservedFeature(features, "workspace.negative-patterns", {
      location: workspaceSources.find((source) => source.location !== "package.json")?.location ?? "package.json",
      detail: `Workspace exclusion pattern ${pattern}`,
    });
  }

  for (const dependency of packages.flatMap((entry) => entry.dependencies)) {
    const featureByProtocol: Partial<Record<DependencyProtocol, FeatureId>> = {
      workspace: "dependency.workspace-protocol",
      catalog: "dependency.catalog-protocol",
      patch: "dependency.patch-protocol",
      portal: "dependency.portal-protocol",
      link: "dependency.link-protocol",
    };
    const featureId = featureByProtocol[dependency.protocol];
    if (featureId) {
      addObservedFeature(features, featureId, dependency.evidence);
    }
  }

  for (const policy of policies) {
    const evidence = policyEvidence(policy);
    if (policy.kind === "catalog" || policy.kind === "catalogs") {
      addObservedFeature(features, "policy.catalogs", evidence, policy.entries);
    } else if (policy.kind === "overrides") {
      addObservedFeature(features, "resolution.overrides", evidence, policy.entries);
      if (policy.nested) {
        addObservedFeature(features, "resolution.nested-overrides", evidence, policy.entries);
      }
    } else if (policy.kind === "resolutions") {
      addObservedFeature(features, "resolution.resolutions", evidence, policy.entries);
    } else if (policy.kind === "package-extensions") {
      addObservedFeature(features, "resolution.package-extensions", evidence, policy.entries);
    } else if (policy.kind === "patched-dependencies") {
      addObservedFeature(features, "patch.patched-dependencies", evidence, policy.entries);
    } else if (policy.kind === "trusted-dependencies") {
      addObservedFeature(features, "lifecycle.trusted-dependencies", evidence, policy.entries);
    }
  }

  const explicitLinker = typeof yarnConfiguration?.nodeLinker === "string"
    ? { location: ".yarnrc.yml", value: yarnConfiguration.nodeLinker }
    : typeof pnpmConfiguration?.nodeLinker === "string"
      ? { location: "pnpm-workspace.yaml", value: pnpmConfiguration.nodeLinker }
      : null;
  if (explicitLinker?.value === "pnp" || inspection.relevantFiles.includes(".pnp.cjs")) {
    addObservedFeature(features, "install.pnp-linker", {
      location: explicitLinker?.location ?? ".pnp.cjs",
      detail: "Project uses a Plug and Play linker",
    });
  }
  if (explicitLinker?.value === "isolated" || explicitLinker?.value === "pnpm") {
    addObservedFeature(features, "install.isolated-linker", {
      location: explicitLinker.location,
      detail: `Project explicitly selects ${explicitLinker.value} linking`,
    });
  }
  const bunfig = await readText(join(inspection.root, "bunfig.toml"));
  if (bunfig && /\blinker\s*=\s*["']isolated["']/.test(bunfig)) {
    addObservedFeature(features, "install.isolated-linker", {
      location: "bunfig.toml",
      detail: "Bun configuration selects isolated linking",
    });
  }
  if (inspection.relevantFiles.includes("yarn.config.cjs")) {
    addObservedFeature(features, "policy.yarn-constraints", {
      location: "yarn.config.cjs",
      detail: "Yarn JavaScript constraints are present",
    });
  }
  if (inspection.relevantFiles.includes(".pnpmfile.cjs")) {
    addObservedFeature(features, "hook.pnpmfile", {
      location: ".pnpmfile.cjs",
      detail: "pnpm hooks are present",
    });
  }
  const npmrc = await readText(join(inspection.root, ".npmrc"));
  let npmrcHasRegistryConfiguration = false;
  if (npmrc !== null) {
    for (const rawLine of npmrc.split(/\r?\n/)) {
      const line = rawLine.trim();
      if (!line || line.startsWith("#") || line.startsWith(";")) continue;
      const [setting, ...valueParts] = line.split("=");
      const value = valueParts.join("=").trim();
      if (setting?.trim() === "node-linker") {
        if (value === "pnp") {
          addObservedFeature(features, "install.pnp-linker", {
            location: ".npmrc",
            detail: "Legacy pnpm configuration selects Plug and Play linking",
          });
        } else if (value === "isolated") {
          addObservedFeature(features, "install.isolated-linker", {
            location: ".npmrc",
            detail: "Legacy pnpm configuration selects isolated linking",
          });
        }
      } else {
        npmrcHasRegistryConfiguration = true;
      }
    }
  }
  if (npmrcHasRegistryConfiguration) {
    addObservedFeature(features, "registry.npmrc", {
      location: ".npmrc",
      detail: "npm-compatible registry configuration is present; values remain redacted",
    });
  }

  const workspace: WorkspaceIR = {
    configured: normalizedPatterns.length > 0,
    patterns: normalizedPatterns,
    packagePaths: packages.map((entry) => entry.path),
    sources: workspaceSources.sort((left, right) => left.location.localeCompare(right.location)),
  };
  const observedFeatures = [...features.values()]
    .map((feature) => ({
      ...feature,
      evidence: feature.evidence.sort((left, right) =>
        left.location.localeCompare(right.location)
        || (left.pointer ?? "").localeCompare(right.pointer ?? ""),
      ),
    }))
    .sort((left, right) => left.id.localeCompare(right.id));

  const identity = {
    schemaVersion: "1.0",
    repositoryFingerprint: inspection.fingerprint,
    source: inspection.packageManager.selected,
    packages,
    workspace,
    policies,
    features: observedFeatures,
    integrations: inspection.integrations,
    diagnostics,
  };
  return {
    schemaVersion: "1.0",
    projectIrId: `ir_${sha256Json(identity).slice(0, 24)}`,
    repositoryFingerprint: inspection.fingerprint,
    source: inspection.packageManager.selected,
    rootPackagePath: ".",
    packages,
    workspace,
    policies,
    features: observedFeatures,
    integrations: inspection.integrations,
    diagnostics,
  };
}
