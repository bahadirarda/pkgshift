import { join, posix } from "node:path";
import type { CapabilityAnalysis } from "../capabilities/models.ts";
import {
  pathExists,
  readJsonObject,
  readText,
  sha256Text,
} from "../core/files.ts";
import { redactSensitiveText } from "../core/redaction.ts";
import type {
  Diagnostic,
  PackageManagerId,
  PlannedFileMutation,
  ProjectInspection,
} from "../domain/models.ts";
import type { ProjectIR } from "../ir/models.ts";
import { getPackageManager } from "../adapters/catalog.ts";
import type { TransformationResult } from "./models.ts";
import { stringifyYaml } from "./yaml.ts";

const IMPLEMENTED_TRANSFORMATIONS = new Set([
  "workspace.expand-to-semver",
  "catalog.expand-to-range",
  "catalog.expand-policy",
  "portal.to-file",
  "portal.to-link",
  "link.to-file",
  "overrides.to-pnpm",
  "overrides.to-resolutions",
  "overrides.nested-to-selector",
  "overrides.nested-to-resolutions",
  "resolutions.to-overrides",
  "resolutions.to-pnpm-overrides",
  "linker.pnp-to-node-modules",
  "linker.pnp-to-isolated",
  "linker.isolated-to-yarn-pnpm",
  "linker.isolated-to-hoisted",
  "registry.npmrc-to-yarnrc",
  "lifecycle.to-pnpm-build-policy",
]);

const SOURCE_CONFIGURATION: Record<PackageManagerId, string[]> = {
  npm: [],
  pnpm: ["pnpm-workspace.yaml", ".pnpmfile.cjs"],
  "yarn-classic": [".yarnrc"],
  "yarn-modern": [".yarnrc.yml", ".pnp.cjs", ".pnp.loader.mjs"],
  bun: ["bunfig.toml"],
  vlt: ["vlt.json"],
  deno: ["deno.json", "deno.jsonc"],
};

function isObject(value: unknown): value is Record<string, unknown> {
  return Boolean(value) && typeof value === "object" && !Array.isArray(value);
}

function cloneObject(value: Record<string, unknown>): Record<string, unknown> {
  return structuredClone(value);
}

function json(value: Record<string, unknown>): string {
  return `${JSON.stringify(value, null, 2)}\n`;
}

function safePath(path: string): boolean {
  const normalized = posix.normalize(path);
  return path.length > 0
    && !posix.isAbsolute(path)
    && normalized === path
    && normalized !== "."
    && normalized !== ".."
    && !normalized.startsWith("../")
    && !path.includes("\\");
}

function diagnostic(
  code: string,
  summary: string,
  location?: string,
): Diagnostic {
  return {
    code,
    severity: "error",
    summary,
    blocking: true,
    ...(location ? { evidence: [{ location, detail: summary }] } : {}),
    remediation: ["Resolve the reported transformation boundary and create a new plan."],
  };
}

const SELECTOR_MAP_KEYS = new Set([
  "catalog",
  "catalogs",
  "dependencies",
  "devDependencies",
  "optionalDependencies",
  "overrides",
  "packageExtensions",
  "peerDependencies",
  "resolutions",
  "scripts",
]);

function sensitiveJsonKey(key: string): boolean {
  const compact = key.replace(/[-_.]/g, "").toLowerCase();
  return [
    "apikey",
    "authtoken",
    "clientsecret",
    "credential",
    "credentials",
    "password",
    "passwd",
    "privatekey",
    "refreshtoken",
    "secret",
    "token",
  ].some((name) => compact === name || compact.endsWith(name));
}

function jsonContainsSensitiveLiteral(
  value: unknown,
  selectorMap = false,
): boolean {
  if (Array.isArray(value)) {
    return value.some((entry) => jsonContainsSensitiveLiteral(entry));
  }
  if (!isObject(value)) return false;
  return Object.entries(value).some(([key, entry]) => {
    if (
      !selectorMap
      && sensitiveJsonKey(key)
      && typeof entry === "string"
      && !/^\$\{[A-Za-z_][A-Za-z0-9_]*\}$/.test(entry)
    ) {
      return true;
    }
    return jsonContainsSensitiveLiteral(entry, selectorMap || SELECTOR_MAP_KEYS.has(key));
  });
}

function containsSensitiveLiteral(path: string, content: string): boolean {
  const environmentNeutral = content.replace(/\$\{[A-Za-z_][A-Za-z0-9_]*\}/g, "***");
  if (redactSensitiveText(environmentNeutral) !== environmentNeutral) return true;
  if (/-----BEGIN [A-Z ]*PRIVATE KEY-----/.test(environmentNeutral)) return true;
  if (/\b(?:github_pat_[A-Za-z0-9_]{20,}|gh[pousr]_[A-Za-z0-9]{20,}|npm_[A-Za-z0-9]{20,}|AKIA[A-Z0-9]{16})\b/.test(environmentNeutral)) {
    return true;
  }
  if (!path.endsWith(".json")) return false;
  try {
    return jsonContainsSensitiveLiteral(JSON.parse(content));
  } catch {
    return false;
  }
}

async function mutation(
  root: string,
  path: string,
  content: string | null,
  reason: string,
  capabilities: string[],
  diagnostics: Diagnostic[],
): Promise<PlannedFileMutation | null> {
  if (!safePath(path)) {
    diagnostics.push(diagnostic("TRANSFORMATION_PATH_UNSAFE", `Unsafe transformation path: ${path}`));
    return null;
  }
  const before = await readText(join(root, path));
  if (
    content !== null
    && containsSensitiveLiteral(path, content)
  ) {
    diagnostics.push(diagnostic(
      "SECRET_REDACTION_FAILED",
      `${path} would place sensitive material inside a persisted plan.`,
      path,
    ));
    return null;
  }
  if (before === content) return null;
  return {
    path,
    action: content === null ? "delete" : "write",
    beforeDigest: before === null ? null : sha256Text(before),
    afterDigest: content === null ? null : sha256Text(content),
    ...(content === null ? {} : { content }),
    reason,
    capabilities: [...new Set(capabilities)].sort(),
  };
}

function workspaceVersion(projectIr: ProjectIR, dependencyName: string): string | null {
  return projectIr.packages.find((entry) => entry.name === dependencyName)?.version ?? null;
}

function expandWorkspaceSpecifier(
  specifier: string,
  dependencyName: string,
  projectIr: ProjectIR,
): { value: string | null; reason: "missing-version" | "unsupported" | null } {
  const version = workspaceVersion(projectIr, dependencyName);
  if (!version) return { value: null, reason: "missing-version" };
  const range = specifier.slice("workspace:".length);
  if (range === "*" || range === "") return { value: version, reason: null };
  if (range === "^") return { value: `^${version}`, reason: null };
  if (range === "~") return { value: `~${version}`, reason: null };
  if (/^[~^]?\d+(?:\.(?:\d+|x|\*)){0,2}(?:-[0-9A-Za-z.-]+)?$/.test(range)) {
    return { value: range, reason: null };
  }
  return { value: null, reason: "unsupported" };
}

interface CatalogState {
  default: Record<string, string>;
  named: Record<string, Record<string, string>>;
}

function stringRecord(value: unknown): Record<string, string> {
  if (!isObject(value)) return {};
  return Object.fromEntries(
    Object.entries(value)
      .filter((entry): entry is [string, string] => typeof entry[1] === "string")
      .sort(([left], [right]) => left.localeCompare(right)),
  );
}

function catalogState(
  rootManifest: Record<string, unknown>,
  pnpmConfiguration: Record<string, unknown> | null,
): CatalogState {
  const workspaces = isObject(rootManifest.workspaces) ? rootManifest.workspaces : {};
  const namedSource = isObject(pnpmConfiguration?.catalogs)
    ? pnpmConfiguration.catalogs
    : isObject(workspaces.catalogs)
      ? workspaces.catalogs
      : {};
  return {
    default: {
      ...stringRecord(workspaces.catalog),
      ...stringRecord(pnpmConfiguration?.catalog),
    },
    named: Object.fromEntries(
      Object.entries(namedSource)
        .filter((entry): entry is [string, Record<string, unknown>] => isObject(entry[1]))
        .map(([name, values]): [string, Record<string, string>] => [name, stringRecord(values)])
        .sort((left, right) => left[0].localeCompare(right[0])),
    ),
  };
}

function resolveCatalogSpecifier(
  specifier: string,
  dependencyName: string,
  catalogs: CatalogState,
): string | null {
  const name = specifier.slice("catalog:".length);
  return name
    ? catalogs.named[name]?.[dependencyName] ?? null
    : catalogs.default[dependencyName] ?? null;
}

function transformDependencies(
  manifest: Record<string, unknown>,
  path: string,
  projectIr: ProjectIR,
  catalogs: CatalogState,
  target: PackageManagerId,
  diagnostics: Diagnostic[],
): void {
  const sections = [
    "dependencies",
    "devDependencies",
    "optionalDependencies",
    "peerDependencies",
  ];
  for (const section of sections) {
    const dependencies = manifest[section];
    if (!isObject(dependencies)) continue;
    for (const [name, value] of Object.entries(dependencies)) {
      if (typeof value !== "string") continue;
      if (
        value.startsWith("workspace:")
        && (target === "npm" || target === "yarn-classic")
      ) {
        const expanded = expandWorkspaceSpecifier(value, name, projectIr);
        if (expanded.reason === "missing-version") {
          diagnostics.push(diagnostic(
            "WORKSPACE_VERSION_REQUIRED",
            `${path} cannot expand ${name} because the workspace package has no version.`,
            path,
          ));
        } else if (expanded.reason === "unsupported") {
          diagnostics.push(diagnostic(
            "WORKSPACE_SPECIFIER_UNSUPPORTED",
            `${path} uses a workspace specifier outside the deterministic semver subset for ${name}.`,
            path,
          ));
        } else {
          dependencies[name] = expanded.value!;
        }
      } else if (
        value.startsWith("catalog:")
        && !["pnpm", "bun"].includes(target)
      ) {
        const expanded = resolveCatalogSpecifier(value, name, catalogs);
        if (!expanded) {
          diagnostics.push(diagnostic(
            "CATALOG_ENTRY_NOT_FOUND",
            `${path} cannot resolve the catalog entry for ${name}.`,
            path,
          ));
        } else {
          dependencies[name] = expanded;
        }
      } else if (value.startsWith("portal:") && target !== "yarn-modern") {
        const suffix = value.slice("portal:".length);
        dependencies[name] = `${target === "npm" ? "file" : "link"}:${suffix}`;
      } else if (value.startsWith("link:") && target === "npm") {
        dependencies[name] = `file:${value.slice("link:".length)}`;
      }
    }
  }
}

function flattenNestedOverrides(
  value: Record<string, unknown>,
  separator: string,
): Record<string, string> | null {
  const output: Record<string, string> = {};
  for (const [parent, nested] of Object.entries(value)) {
    if (typeof nested === "string") {
      output[parent] = nested;
      continue;
    }
    if (!isObject(nested)) return null;
    for (const [child, childValue] of Object.entries(nested)) {
      if (child === "." && typeof childValue === "string") {
        output[parent] = childValue;
      } else if (typeof childValue === "string") {
        output[`${parent}${separator}${child}`] = childValue;
      } else {
        return null;
      }
    }
  }
  return output;
}

function sourcePolicies(
  rootManifest: Record<string, unknown>,
  pnpmConfiguration: Record<string, unknown> | null,
): {
  overrides: Record<string, unknown>;
  resolutions: Record<string, unknown>;
  packageExtensions: Record<string, unknown>;
  patchedDependencies: Record<string, unknown>;
  trustedDependencies: string[];
} {
  const pnpmManifest = isObject(rootManifest.pnpm) ? rootManifest.pnpm : {};
  const trusted = [
    rootManifest.trustedDependencies,
    pnpmManifest.onlyBuiltDependencies,
    pnpmConfiguration?.onlyBuiltDependencies,
  ].find(Array.isArray);
  const allowBuilds = isObject(pnpmConfiguration?.allowBuilds)
    ? Object.entries(pnpmConfiguration.allowBuilds)
        .filter((entry) => entry[1] === true)
        .map(([name]) => name)
    : [];
  return {
    overrides: isObject(pnpmConfiguration?.overrides)
      ? pnpmConfiguration.overrides
      : isObject(pnpmManifest.overrides)
        ? pnpmManifest.overrides
        : isObject(rootManifest.overrides)
          ? rootManifest.overrides
          : {},
    resolutions: isObject(rootManifest.resolutions) ? rootManifest.resolutions : {},
    packageExtensions: isObject(pnpmConfiguration?.packageExtensions)
      ? pnpmConfiguration.packageExtensions
      : isObject(pnpmManifest.packageExtensions)
        ? pnpmManifest.packageExtensions
        : isObject(rootManifest.packageExtensions)
          ? rootManifest.packageExtensions
          : {},
    patchedDependencies: isObject(pnpmConfiguration?.patchedDependencies)
      ? pnpmConfiguration.patchedDependencies
      : isObject(pnpmManifest.patchedDependencies)
        ? pnpmManifest.patchedDependencies
        : isObject(rootManifest.patchedDependencies)
          ? rootManifest.patchedDependencies
          : {},
    trustedDependencies: [...new Set([
      ...(Array.isArray(trusted)
        ? trusted.filter((entry): entry is string => typeof entry === "string")
        : []),
      ...allowBuilds,
    ])].sort(),
  };
}

function compatibleResolutions(
  resolutions: Record<string, unknown>,
): Record<string, string> | null {
  const output: Record<string, string> = {};
  for (const [selector, value] of Object.entries(resolutions)) {
    if (typeof value !== "string" || selector.includes("**") || selector.includes("/")) {
      return null;
    }
    output[selector] = value;
  }
  return output;
}

function configurePolicies(
  manifest: Record<string, unknown>,
  targetConfiguration: Record<string, unknown>,
  policies: ReturnType<typeof sourcePolicies>,
  catalogs: CatalogState,
  target: PackageManagerId,
  diagnostics: Diagnostic[],
): void {
  delete manifest.pnpm;
  delete manifest.overrides;
  delete manifest.resolutions;
  delete manifest.packageExtensions;
  delete manifest.patchedDependencies;
  delete manifest.trustedDependencies;

  let overrides = policies.overrides;
  if (Object.keys(overrides).length === 0 && Object.keys(policies.resolutions).length > 0) {
    const compatible = compatibleResolutions(policies.resolutions);
    if (!compatible) {
      diagnostics.push(diagnostic(
        "RESOLUTION_SELECTOR_UNSUPPORTED",
        "Yarn resolution selectors cannot be translated without reducing selector fidelity.",
        "package.json",
      ));
    } else {
      overrides = compatible;
    }
  }

  if (target === "pnpm") {
    if (Object.keys(overrides).length > 0) {
      const flattened = flattenNestedOverrides(overrides, ">");
      if (!flattened) {
        diagnostics.push(diagnostic(
          "NESTED_OVERRIDE_UNSUPPORTED",
          "Nested overrides exceed the deterministic pnpm selector subset.",
          "package.json",
        ));
      } else {
        targetConfiguration.overrides = flattened;
      }
    }
    if (Object.keys(policies.packageExtensions).length > 0) {
      targetConfiguration.packageExtensions = policies.packageExtensions;
    }
    if (Object.keys(policies.patchedDependencies).length > 0) {
      targetConfiguration.patchedDependencies = policies.patchedDependencies;
    }
    if (policies.trustedDependencies.length > 0) {
      targetConfiguration.onlyBuiltDependencies = policies.trustedDependencies;
    }
    if (Object.keys(catalogs.default).length > 0) targetConfiguration.catalog = catalogs.default;
    if (Object.keys(catalogs.named).length > 0) targetConfiguration.catalogs = catalogs.named;
  } else if (target === "npm" || target === "bun") {
    if (Object.keys(overrides).length > 0) manifest.overrides = overrides;
    if (target === "npm" && Object.keys(policies.packageExtensions).length > 0) {
      manifest.packageExtensions = policies.packageExtensions;
    }
    if (target === "bun" && Object.keys(policies.patchedDependencies).length > 0) {
      manifest.patchedDependencies = policies.patchedDependencies;
    }
    if (target === "bun" && policies.trustedDependencies.length > 0) {
      manifest.trustedDependencies = policies.trustedDependencies;
    }
  } else {
    if (Object.keys(overrides).length > 0) {
      const flattened = flattenNestedOverrides(overrides, "/");
      if (!flattened) {
        diagnostics.push(diagnostic(
          "NESTED_OVERRIDE_UNSUPPORTED",
          "Nested overrides exceed the deterministic Yarn resolution subset.",
          "package.json",
        ));
      } else {
        manifest.resolutions = flattened;
      }
    } else if (Object.keys(policies.resolutions).length > 0) {
      manifest.resolutions = policies.resolutions;
    }
    if (target === "yarn-modern" && Object.keys(policies.packageExtensions).length > 0) {
      targetConfiguration.packageExtensions = policies.packageExtensions;
    }
    if (target === "yarn-modern" && policies.trustedDependencies.length > 0) {
      manifest.dependenciesMeta = {
        ...(isObject(manifest.dependenciesMeta) ? manifest.dependenciesMeta : {}),
        ...Object.fromEntries(policies.trustedDependencies.map((name) => [name, { built: true }])),
      };
    }
  }
}

function configureWorkspaces(
  manifest: Record<string, unknown>,
  projectIr: ProjectIR,
  catalogs: CatalogState,
  target: PackageManagerId,
  targetConfiguration: Record<string, unknown>,
  diagnostics: Diagnostic[],
): void {
  if (!projectIr.workspace.configured) {
    delete manifest.workspaces;
    return;
  }
  if ((target === "yarn-classic" || target === "yarn-modern") && manifest.private !== true) {
    diagnostics.push(diagnostic(
      "YARN_WORKSPACE_PRIVATE_REQUIRED",
      "Yarn workspace roots must remain explicitly private for this migration.",
      "package.json",
    ));
  }
  if (target === "pnpm") {
    delete manifest.workspaces;
    targetConfiguration.packages = projectIr.workspace.patterns;
  } else if (target === "bun" && (
    Object.keys(catalogs.default).length > 0 || Object.keys(catalogs.named).length > 0
  )) {
    manifest.workspaces = {
      packages: projectIr.workspace.patterns,
      ...(Object.keys(catalogs.default).length > 0 ? { catalog: catalogs.default } : {}),
      ...(Object.keys(catalogs.named).length > 0 ? { catalogs: catalogs.named } : {}),
    };
  } else {
    manifest.workspaces = projectIr.workspace.patterns;
  }
}

function parseNpmrcForYarn(
  content: string,
  diagnostics: Diagnostic[],
): Record<string, unknown> {
  const output: Record<string, unknown> = {};
  const scopes: Record<string, Record<string, unknown>> = {};
  const registries: Record<string, Record<string, unknown>> = {};
  for (const rawLine of content.split(/\r?\n/)) {
    const line = rawLine.trim();
    if (!line || line.startsWith("#") || line.startsWith(";")) continue;
    const separator = line.indexOf("=");
    if (separator < 1) continue;
    const setting = line.slice(0, separator).trim();
    const value = line.slice(separator + 1).trim();
    if (setting === "registry") {
      output.npmRegistryServer = value;
    } else if (/^@[^:]+:registry$/.test(setting)) {
      const scope = setting.slice(1, setting.indexOf(":"));
      scopes[scope] = { npmRegistryServer: value };
    } else if (/^\/\/[^:]+(?::\d+)?\/?:_authToken$/.test(setting)) {
      if (!/^\$\{[A-Za-z_][A-Za-z0-9_]*\}$/.test(value)) {
        diagnostics.push(diagnostic(
          "REGISTRY_SECRET_REQUIRES_ENVIRONMENT_REFERENCE",
          "Yarn Modern registry migration requires authentication tokens to use ${ENV_VAR} references.",
          ".npmrc",
        ));
        continue;
      }
      const registry = setting.slice(0, setting.indexOf(":_authToken"));
      registries[registry] = { npmAuthToken: value, npmAlwaysAuth: true };
    } else if (setting === "always-auth" && /^(?:true|false)$/.test(value)) {
      output.npmAlwaysAuth = value === "true";
    } else {
      diagnostics.push(diagnostic(
        "NPMRC_SETTING_UNSUPPORTED",
        `Yarn Modern translation does not support the .npmrc setting ${setting}.`,
        ".npmrc",
      ));
    }
  }
  if (Object.keys(scopes).length > 0) output.npmScopes = scopes;
  if (Object.keys(registries).length > 0) output.npmRegistries = registries;
  return output;
}

function bunConfiguration(
  before: string | null,
  isolated: boolean,
): string | null {
  if (!isolated) return before;
  const content = before ?? "";
  if (/^\[install\]\s*$/m.test(content)) {
    if (/^linker\s*=/m.test(content)) {
      return content.replace(/^linker\s*=.*$/m, 'linker = "isolated"');
    }
    return content.replace(/^\[install\]\s*$/m, '[install]\nlinker = "isolated"');
  }
  return `${content.trimEnd()}${content.trim() ? "\n\n" : ""}[install]\nlinker = "isolated"\n`;
}

function integrationContent(
  content: string,
  source: PackageManagerId,
  target: PackageManagerId,
): string {
  const sourceToken = source.startsWith("yarn-") ? "yarn" : source;
  const targetToken = target.startsWith("yarn-") ? "yarn" : target;
  if (sourceToken === targetToken) return content;
  const expression = new RegExp(`\\b${sourceToken}\\s+(install|ci|run|add|remove)\\b`, "g");
  return content.replace(expression, (_match, command: string) =>
    `${targetToken} ${command === "ci" ? "install" : command}`
  );
}

export async function transformProject(
  inspection: ProjectInspection,
  projectIr: ProjectIR,
  capabilityAnalysis: CapabilityAnalysis,
  target: PackageManagerId,
): Promise<TransformationResult> {
  const diagnostics: Diagnostic[] = [];
  const result: TransformationResult = {
    manifestMutations: [],
    configurationMutations: [],
    integrationMutations: [],
    cleanupMutations: [],
    diagnostics,
  };
  const source = inspection.packageManager.selected;
  if (!source) return result;

  for (const decision of capabilityAnalysis.decisions) {
    if (
      (decision.classification === "transform" || decision.classification === "lossy")
      && decision.transformationId
      && !IMPLEMENTED_TRANSFORMATIONS.has(decision.transformationId)
    ) {
      diagnostics.push(diagnostic(
        "TRANSFORMATION_UNIMPLEMENTED",
        `The execution adapter does not yet implement ${decision.transformationId}.`,
        decision.evidence[0]?.location,
      ));
    }
    if (
      decision.transformationId === "lifecycle.to-global-script-policy"
    ) {
      diagnostics.push(diagnostic(
        "TRANSFORMATION_MANUAL_REQUIRED",
        "The target cannot preserve a dependency-level lifecycle allow-list safely.",
        decision.evidence[0]?.location,
      ));
    }
  }

  const rootManifest = await readJsonObject(join(inspection.root, "package.json"));
  if (!rootManifest) {
    diagnostics.push(diagnostic("MANIFEST_NOT_FOUND", "package.json is required for transformation."));
    return result;
  }
  const pnpmText = await readText(join(inspection.root, "pnpm-workspace.yaml"));
  let pnpmConfiguration: Record<string, unknown> | null = null;
  if (pnpmText !== null) {
    try {
      const parsed: unknown = Bun.YAML.parse(pnpmText);
      pnpmConfiguration = isObject(parsed) ? parsed : null;
    } catch (error) {
      diagnostics.push(diagnostic(
        "CONFIGURATION_PARSE_FAILED",
        error instanceof Error ? error.message : "pnpm-workspace.yaml could not be parsed.",
        "pnpm-workspace.yaml",
      ));
    }
  }
  const catalogs = catalogState(rootManifest, pnpmConfiguration);
  const policies = sourcePolicies(rootManifest, pnpmConfiguration);
  const targetConfiguration: Record<string, unknown> = {};
  const targetDefinition = getPackageManager(target);

  for (const packageEntry of projectIr.packages) {
    const current = await readJsonObject(join(inspection.root, packageEntry.manifestPath));
    if (!current) continue;
    const next = cloneObject(current);
    transformDependencies(
      next,
      packageEntry.manifestPath,
      projectIr,
      catalogs,
      target,
      diagnostics,
    );
    if (packageEntry.path === ".") {
      next.packageManager = targetDefinition.packageManagerPin;
      configurePolicies(next, targetConfiguration, policies, catalogs, target, diagnostics);
      configureWorkspaces(next, projectIr, catalogs, target, targetConfiguration, diagnostics);
    }
    const planned = await mutation(
      inspection.root,
      packageEntry.manifestPath,
      json(next),
      `Render ${targetDefinition.displayName}-compatible package metadata.`,
      capabilityAnalysis.decisions.map((decision) => decision.featureId),
      diagnostics,
    );
    if (planned) result.manifestMutations.push(planned);
  }

  const pnp = projectIr.features.some((feature) => feature.id === "install.pnp-linker");
  const isolated = projectIr.features.some((feature) => feature.id === "install.isolated-linker");
  if (target === "pnpm") {
    if (pnp) targetConfiguration.nodeLinker = "pnp";
    else if (isolated) targetConfiguration.nodeLinker = "isolated";
    if (Object.keys(targetConfiguration).length > 0) {
      const planned = await mutation(
        inspection.root,
        "pnpm-workspace.yaml",
        stringifyYaml(targetConfiguration),
        "Render pnpm workspace and policy configuration.",
        capabilityAnalysis.decisions.map((decision) => decision.featureId),
        diagnostics,
      );
      if (planned) result.configurationMutations.push(planned);
    }
  } else if (target === "yarn-modern") {
    targetConfiguration.nodeLinker = pnp ? "pnp" : isolated ? "pnpm" : "node-modules";
    const npmrc = await readText(join(inspection.root, ".npmrc"));
    if (npmrc !== null) {
      Object.assign(targetConfiguration, parseNpmrcForYarn(npmrc, diagnostics));
    }
    const planned = await mutation(
      inspection.root,
      ".yarnrc.yml",
      stringifyYaml(targetConfiguration),
      "Render Yarn Modern linker, policy, and registry configuration.",
      capabilityAnalysis.decisions.map((decision) => decision.featureId),
      diagnostics,
    );
    if (planned) result.configurationMutations.push(planned);
  } else if (target === "bun") {
    const before = await readText(join(inspection.root, "bunfig.toml"));
    const after = bunConfiguration(before, pnp || isolated);
    if (after !== before) {
      const planned = await mutation(
        inspection.root,
        "bunfig.toml",
        after,
        "Render Bun linker configuration.",
        capabilityAnalysis.decisions.map((decision) => decision.featureId),
        diagnostics,
      );
      if (planned) result.configurationMutations.push(planned);
    }
  }

  for (const integration of inspection.integrations) {
    const before = await readText(join(inspection.root, integration.path));
    if (before === null) continue;
    const after = integrationContent(before, source, target);
    if (after === before) continue;
    const planned = await mutation(
      inspection.root,
      integration.path,
      after,
      `Translate ${source} package-manager commands to ${target}.`,
      [],
      diagnostics,
    );
    if (planned) result.integrationMutations.push(planned);
  }

  const retainedConfiguration = new Set(
    targetDefinition.configurationFiles.filter((path) => path !== ".npmrc"),
  );
  const sourceConfiguration = new Set([
    ...SOURCE_CONFIGURATION[source],
    ...(target === "yarn-modern" ? [".npmrc"] : []),
  ]);
  for (const path of sourceConfiguration) {
    if (retainedConfiguration.has(path) || !(await pathExists(join(inspection.root, path)))) continue;
    const planned = await mutation(
      inspection.root,
      path,
      null,
      `Retire source-only ${source} configuration after target installation.`,
      [],
      diagnostics,
    );
    if (planned) result.cleanupMutations.push(planned);
  }

  const targetLocks = new Set(targetDefinition.lockfiles);
  for (const path of getPackageManager(source).lockfiles) {
    if (targetLocks.has(path) || !(await pathExists(join(inspection.root, path)))) continue;
    const planned = await mutation(
      inspection.root,
      path,
      null,
      `Retire the ${source} lockfile after target installation.`,
      [],
      diagnostics,
    );
    if (planned) result.cleanupMutations.push(planned);
  }

  for (const collection of [
    result.manifestMutations,
    result.configurationMutations,
    result.integrationMutations,
    result.cleanupMutations,
  ]) {
    collection.sort((left, right) => left.path.localeCompare(right.path));
  }
  return result;
}
