import type { PackageManagerId } from "../domain/models.ts";
import type { FeatureId } from "../ir/models.ts";
import type {
  CapabilityClassification,
  CapabilityRisk,
} from "./models.ts";

export interface RuleOutcome {
  classification: CapabilityClassification;
  risk: CapabilityRisk;
  transformationId?: string;
  summary: string;
}

export interface CapabilityRule {
  featureId: FeatureId;
  title: string;
  basis: string[];
  targets: Partial<Record<PackageManagerId, RuleOutcome>>;
}

const NPM_MANIFEST = "https://docs.npmjs.com/cli/configuring-npm/package-json/";
const PNPM_SETTINGS = "https://pnpm.io/settings";
const PNPM_CATALOGS = "https://pnpm.io/catalogs";
const PNPM_WORKSPACES = "https://pnpm.io/workspaces";
const YARN_MANIFEST = "https://yarnpkg.com/configuration/manifest";
const YARN_PATCHING = "https://yarnpkg.com/features/patching";
const YARN_LINKERS = "https://yarnpkg.com/features/linkers";
const YARN_CONSTRAINTS = "https://yarnpkg.com/features/constraints";
const BUN_WORKSPACES = "https://bun.sh/docs/pm/workspaces";
const BUN_OVERRIDES = "https://bun.sh/docs/pm/overrides";
const BUN_INSTALL = "https://bun.sh/docs/pm/cli/install";
const VLT_MIGRATION = "https://docs.vlt.sh/cli/migration";
const VLT_WORKSPACES = "https://docs.vlt.sh/cli/workspaces";
const VLT_CATALOGS = "https://docs.vlt.sh/cli/catalogs";
const VLT_MODIFIERS = "https://docs.vlt.sh/cli/graph-modifiers";
const DENO_MIGRATION = "https://docs.deno.com/runtime/migrate/migrate_from_npm/";
const DENO_WORKSPACES = "https://docs.deno.com/runtime/fundamentals/workspaces/";
const DENO_CONFIGURATION = "https://docs.deno.com/runtime/reference/deno_json/";

const native = (summary: string): RuleOutcome => ({
  classification: "native",
  risk: "none",
  summary,
});

const transform = (
  transformationId: string,
  summary: string,
  risk: CapabilityRisk = "low",
): RuleOutcome => ({
  classification: "transform",
  risk,
  transformationId,
  summary,
});

const lossy = (
  transformationId: string,
  summary: string,
  risk: CapabilityRisk = "medium",
): RuleOutcome => ({
  classification: "lossy",
  risk,
  transformationId,
  summary,
});

const unsupported = (summary: string): RuleOutcome => ({
  classification: "unsupported",
  risk: "high",
  summary,
});

const unknown = (summary: string): RuleOutcome => ({
  classification: "unknown",
  risk: "high",
  summary,
});

export const CAPABILITY_RULES: Record<FeatureId, CapabilityRule> = {
  "workspace.manifest": {
    featureId: "workspace.manifest",
    title: "Workspace membership",
    basis: [NPM_MANIFEST, PNPM_WORKSPACES, YARN_MANIFEST, BUN_WORKSPACES, VLT_WORKSPACES, DENO_WORKSPACES],
    targets: {
      npm: native("npm represents workspace membership in package.json."),
      pnpm: native("pnpm represents workspace membership in pnpm-workspace.yaml."),
      "yarn-classic": native("Yarn Classic represents workspace membership in package.json."),
      "yarn-modern": native("Yarn Modern represents workspace membership in package.json."),
      bun: native("Bun represents workspace membership in package.json."),
      vlt: transform("workspace.to-vlt-workspace", "Move workspace membership into vlt.json."),
      deno: transform("workspace.to-deno-workspace", "Deno dependency mode requires workspace membership in Deno configuration.", "medium"),
    },
  },
  "workspace.negative-patterns": {
    featureId: "workspace.negative-patterns",
    title: "Workspace exclusion patterns",
    basis: [PNPM_SETTINGS, BUN_WORKSPACES],
    targets: {
      pnpm: native("pnpm workspace patterns support exclusions."),
      bun: native("Bun workspace patterns support exclusions."),
      npm: unknown("Equivalent exclusion behavior has not been verified for npm workspaces."),
      "yarn-classic": unknown("Equivalent exclusion behavior has not been verified for Yarn Classic."),
      "yarn-modern": unknown("Equivalent exclusion behavior has not been verified for Yarn Modern."),
      vlt: unknown("Equivalent exclusion behavior has not been verified for vlt workspaces."),
      deno: unknown("Equivalent exclusion behavior has not been verified for Deno workspaces."),
    },
  },
  "dependency.workspace-protocol": {
    featureId: "dependency.workspace-protocol",
    title: "Workspace dependency protocol",
    basis: [PNPM_WORKSPACES, YARN_MANIFEST, BUN_WORKSPACES],
    targets: {
      pnpm: native("pnpm natively supports workspace: dependency specifiers."),
      "yarn-modern": native("Yarn Modern natively supports workspace: dependency specifiers."),
      bun: native("Bun natively supports workspace: dependency specifiers."),
      npm: transform("workspace.expand-to-semver", "Resolve workspace: specifiers to publish-compatible semver ranges."),
      "yarn-classic": transform("workspace.expand-to-semver", "Resolve workspace: specifiers to semver ranges for Yarn Classic."),
      vlt: native("vlt supports workspace: dependency specifiers."),
      deno: native("Deno supports workspace: dependency specifiers in package.json."),
    },
  },
  "dependency.catalog-protocol": {
    featureId: "dependency.catalog-protocol",
    title: "Catalog dependency protocol",
    basis: [PNPM_CATALOGS, BUN_WORKSPACES, VLT_CATALOGS],
    targets: {
      pnpm: native("pnpm natively supports catalog: dependency specifiers."),
      bun: native("Bun natively supports catalog: dependency specifiers."),
      npm: lossy("catalog.expand-to-range", "Expand catalog references to ranges and lose centralized catalog policy."),
      "yarn-classic": lossy("catalog.expand-to-range", "Expand catalog references to ranges and lose centralized catalog policy."),
      "yarn-modern": lossy("catalog.expand-to-range", "Expand catalog references to ranges and lose centralized catalog policy."),
      vlt: native("vlt natively supports pnpm-compatible catalog: dependency specifiers."),
      deno: lossy("catalog.expand-to-range", "Expand catalog references to ranges for Deno dependency declarations."),
    },
  },
  "dependency.patch-protocol": {
    featureId: "dependency.patch-protocol",
    title: "Yarn patch protocol",
    basis: [YARN_PATCHING, PNPM_SETTINGS, BUN_INSTALL],
    targets: {
      "yarn-modern": native("Yarn Modern natively supports patch: dependency specifiers."),
      pnpm: transform("patch.yarn-to-pnpm", "Convert Yarn patch protocol entries into pnpm patched dependencies.", "medium"),
      bun: transform("patch.yarn-to-bun", "Convert Yarn patch protocol entries into Bun patched dependencies.", "medium"),
      npm: unsupported("npm has no supported equivalent for Yarn patch protocol entries."),
      "yarn-classic": unsupported("Yarn Classic has no native patch protocol."),
      vlt: unsupported("vlt has no supported patch protocol mapping in this adapter."),
      deno: unsupported("Deno dependency mode does not provide an equivalent patch workflow in the MVP boundary."),
    },
  },
  "dependency.portal-protocol": {
    featureId: "dependency.portal-protocol",
    title: "Yarn portal protocol",
    basis: ["https://yarnpkg.com/protocols"],
    targets: {
      "yarn-modern": native("Yarn Modern natively supports portal: dependency specifiers."),
      npm: lossy("portal.to-file", "Convert portal dependencies to file references and lose portal transitive semantics.", "high"),
      pnpm: lossy("portal.to-link", "Convert portal dependencies to link references and review peer behavior.", "high"),
      "yarn-classic": lossy("portal.to-link", "Convert portal dependencies to link references and lose portal semantics.", "high"),
      bun: unknown("Equivalent portal semantics have not been verified for Bun."),
      vlt: unsupported("vlt has no supported portal mapping in this adapter."),
      deno: unsupported("Deno dependency mode has no supported portal equivalent in the MVP boundary."),
    },
  },
  "dependency.link-protocol": {
    featureId: "dependency.link-protocol",
    title: "Link dependency protocol",
    basis: ["https://yarnpkg.com/protocols", PNPM_WORKSPACES, BUN_INSTALL],
    targets: {
      pnpm: native("pnpm supports link: dependency references."),
      "yarn-classic": native("Yarn Classic supports link: dependency references."),
      "yarn-modern": native("Yarn Modern supports link: dependency references."),
      npm: lossy("link.to-file", "Convert link references to file references and review packing behavior."),
      bun: unknown("Bun link: protocol parity has not been verified for this adapter."),
      vlt: unsupported("vlt has no supported link protocol mapping in this adapter."),
      deno: unsupported("Deno dependency mode has no supported link protocol mapping in the MVP boundary."),
    },
  },
  "dependency.deno-import-map": {
    featureId: "dependency.deno-import-map",
    title: "Deno import map dependencies",
    basis: [DENO_CONFIGURATION],
    targets: {
      deno: native("Deno natively preserves imports and scopes in its runtime configuration."),
      npm: unsupported("Deno import maps are outside npm package metadata."),
      pnpm: unsupported("Deno import maps are outside pnpm package metadata."),
      "yarn-classic": unsupported("Deno import maps are outside Yarn Classic package metadata."),
      "yarn-modern": unsupported("Deno import maps are outside Yarn Modern package metadata."),
      bun: unsupported("Deno import map migration is outside the package-manager boundary."),
      vlt: unsupported("Deno import map migration is outside the package-manager boundary."),
    },
  },
  "policy.catalogs": {
    featureId: "policy.catalogs",
    title: "Central dependency catalogs",
    basis: [PNPM_CATALOGS, BUN_WORKSPACES, VLT_CATALOGS],
    targets: {
      pnpm: native("pnpm natively represents default and named catalogs."),
      bun: native("Bun natively represents default and named catalogs."),
      npm: lossy("catalog.expand-policy", "Expand catalog policy into manifests and lose centralized version governance."),
      "yarn-classic": lossy("catalog.expand-policy", "Expand catalog policy into manifests and lose centralized version governance."),
      "yarn-modern": lossy("catalog.expand-policy", "Expand catalog policy into manifests and lose centralized version governance."),
      vlt: native("vlt natively represents default and named dependency catalogs."),
      deno: lossy("catalog.expand-policy", "Expand catalog policy into Deno-compatible dependency declarations."),
    },
  },
  "resolution.overrides": {
    featureId: "resolution.overrides",
    title: "Dependency overrides",
    basis: [NPM_MANIFEST, PNPM_SETTINGS, BUN_OVERRIDES, YARN_MANIFEST, VLT_MODIFIERS, DENO_CONFIGURATION],
    targets: {
      npm: native("npm natively represents dependency overrides."),
      pnpm: transform("overrides.to-pnpm", "Move compatible overrides into pnpm workspace settings."),
      bun: native("Bun natively supports top-level npm overrides."),
      "yarn-classic": lossy("overrides.to-resolutions", "Convert compatible overrides to Yarn resolutions and review selector differences."),
      "yarn-modern": lossy("overrides.to-resolutions", "Convert compatible overrides to Yarn resolutions and review selector differences."),
      vlt: transform("overrides.to-vlt-modifiers", "Translate compatible overrides into vlt graph modifiers.", "medium"),
      deno: native("Deno honors npm-compatible overrides in package.json."),
    },
  },
  "resolution.nested-overrides": {
    featureId: "resolution.nested-overrides",
    title: "Nested dependency overrides",
    basis: [NPM_MANIFEST, PNPM_SETTINGS, BUN_OVERRIDES],
    targets: {
      npm: native("npm natively supports nested override objects."),
      pnpm: transform("overrides.nested-to-selector", "Translate nested overrides to pnpm selector rules and verify graph equivalence.", "medium"),
      "yarn-classic": lossy("overrides.nested-to-resolutions", "Flatten nested overrides into Yarn resolutions with reduced selector fidelity.", "high"),
      "yarn-modern": lossy("overrides.nested-to-resolutions", "Flatten nested overrides into Yarn resolutions with reduced selector fidelity.", "high"),
      bun: unsupported("Bun currently supports only top-level overrides and resolutions."),
      vlt: transform("overrides.to-vlt-modifiers", "Translate one-level nested overrides into vlt dependency selectors.", "medium"),
      deno: native("Deno honors nested npm-compatible overrides in package.json."),
    },
  },
  "resolution.resolutions": {
    featureId: "resolution.resolutions",
    title: "Yarn-style resolutions",
    basis: [YARN_MANIFEST, BUN_OVERRIDES, NPM_MANIFEST, PNPM_SETTINGS],
    targets: {
      "yarn-classic": native("Yarn Classic natively supports resolutions."),
      "yarn-modern": native("Yarn Modern natively supports resolutions."),
      bun: native("Bun natively supports top-level Yarn resolutions."),
      npm: transform("resolutions.to-overrides", "Translate compatible resolutions into npm overrides.", "medium"),
      pnpm: transform("resolutions.to-pnpm-overrides", "Translate compatible resolutions into pnpm override selectors.", "medium"),
      vlt: transform("resolutions.to-vlt-modifiers", "Translate compatible Yarn resolutions into vlt graph modifiers.", "medium"),
      deno: transform("resolutions.to-overrides", "Translate compatible Yarn resolutions into Deno-compatible npm overrides.", "medium"),
    },
  },
  "resolution.package-extensions": {
    featureId: "resolution.package-extensions",
    title: "Package extensions",
    basis: [NPM_MANIFEST, PNPM_SETTINGS, YARN_MANIFEST],
    targets: {
      npm: native("npm natively supports root packageExtensions policy."),
      pnpm: native("pnpm natively supports package extensions."),
      "yarn-modern": native("Yarn Modern natively supports package extensions."),
      "yarn-classic": unsupported("Yarn Classic has no native package extensions mechanism."),
      bun: unknown("Bun package extensions parity has not been verified."),
      vlt: unsupported("vlt has no supported package extensions mapping in this adapter."),
      deno: unsupported("Deno dependency mode has no supported package extensions mapping."),
    },
  },
  "patch.patched-dependencies": {
    featureId: "patch.patched-dependencies",
    title: "Patched dependencies policy",
    basis: [PNPM_SETTINGS, BUN_INSTALL, YARN_PATCHING],
    targets: {
      pnpm: native("pnpm natively represents patched dependencies."),
      bun: native("Bun natively represents and migrates patched dependencies."),
      "yarn-modern": transform("patch.patched-to-yarn", "Convert patched dependency entries into Yarn patch protocol references.", "medium"),
      npm: unsupported("npm has no supported patched dependencies mechanism."),
      "yarn-classic": unsupported("Yarn Classic has no native patched dependencies mechanism."),
      vlt: unsupported("vlt has no supported patched dependency mapping in this adapter."),
      deno: unsupported("Deno dependency mode has no supported patch workflow in the MVP boundary."),
    },
  },
  "install.pnp-linker": {
    featureId: "install.pnp-linker",
    title: "Plug and Play installation layout",
    basis: [YARN_LINKERS, PNPM_SETTINGS],
    targets: {
      pnpm: native("pnpm can represent an explicit Plug and Play linker."),
      "yarn-modern": native("Yarn Modern natively supports Plug and Play."),
      npm: lossy("linker.pnp-to-node-modules", "Switch to node_modules and lose Plug and Play dependency enforcement.", "high"),
      "yarn-classic": lossy("linker.pnp-to-node-modules", "Switch to node_modules and lose Plug and Play dependency enforcement.", "high"),
      bun: lossy("linker.pnp-to-isolated", "Switch from Plug and Play to Bun isolated linking and verify ghost dependency behavior.", "high"),
      vlt: lossy("linker.pnp-to-isolated", "Switch from Plug and Play to vlt's isolated dependency layout.", "high"),
      deno: lossy("linker.pnp-to-isolated", "Switch from Plug and Play to Deno's isolated node_modules linker.", "high"),
    },
  },
  "install.isolated-linker": {
    featureId: "install.isolated-linker",
    title: "Isolated installation layout",
    basis: [PNPM_SETTINGS, YARN_LINKERS, BUN_INSTALL],
    targets: {
      pnpm: native("pnpm natively supports isolated linking."),
      bun: native("Bun natively supports isolated linking."),
      "yarn-modern": transform("linker.isolated-to-yarn-pnpm", "Select Yarn's pnpm linker and verify layout assumptions."),
      npm: lossy("linker.isolated-to-hoisted", "Switch to hoisted node_modules and lose strict dependency isolation.", "high"),
      "yarn-classic": lossy("linker.isolated-to-hoisted", "Switch to hoisted node_modules and lose strict dependency isolation.", "high"),
      vlt: native("vlt uses an isolated dependency layout."),
      deno: native("Deno supports an isolated node_modules linker."),
    },
  },
  "policy.yarn-constraints": {
    featureId: "policy.yarn-constraints",
    title: "Yarn JavaScript constraints",
    basis: [YARN_CONSTRAINTS],
    targets: {
      "yarn-modern": native("Yarn Modern natively executes JavaScript constraints."),
      npm: unsupported("Arbitrary Yarn constraint logic cannot be translated safely."),
      pnpm: unsupported("Arbitrary Yarn constraint logic cannot be translated safely."),
      "yarn-classic": unsupported("Yarn Classic has no equivalent JavaScript constraint engine."),
      bun: unsupported("Arbitrary Yarn constraint logic cannot be translated safely."),
      vlt: unsupported("Arbitrary Yarn constraint logic cannot be translated safely."),
      deno: unsupported("Deno dependency mode has no equivalent Yarn constraint engine."),
    },
  },
  "hook.pnpmfile": {
    featureId: "hook.pnpmfile",
    title: "pnpm hook module",
    basis: [PNPM_SETTINGS],
    targets: {
      pnpm: native("pnpm natively executes pnpmfile hooks."),
      npm: unsupported("Arbitrary pnpm hook code cannot be translated safely."),
      "yarn-classic": unsupported("Arbitrary pnpm hook code cannot be translated safely."),
      "yarn-modern": unsupported("Arbitrary pnpm hook code cannot be translated safely."),
      bun: unsupported("Arbitrary pnpm hook code cannot be translated safely."),
      vlt: unsupported("Arbitrary pnpm hook code cannot be translated safely."),
      deno: unsupported("Deno dependency mode has no equivalent pnpm hook boundary."),
    },
  },
  "registry.npmrc": {
    featureId: "registry.npmrc",
    title: "npm-compatible registry configuration",
    basis: [NPM_MANIFEST, PNPM_SETTINGS, YARN_MANIFEST, BUN_INSTALL, VLT_MIGRATION, DENO_MIGRATION],
    targets: {
      npm: native("npm natively consumes .npmrc registry configuration."),
      pnpm: native("pnpm consumes authentication and registry settings from .npmrc."),
      "yarn-classic": native("Yarn Classic consumes npm-compatible registry configuration."),
      bun: native("Bun consumes npm-compatible registry configuration."),
      "yarn-modern": transform("registry.npmrc-to-yarnrc", "Translate registry scopes into Yarn Modern configuration while preserving secret references.", "medium"),
      vlt: transform("registry.npmrc-to-vlt", "Move public registry and scope mappings into vlt.json; credentials remain external.", "medium"),
      deno: native("Deno consumes npm registry configuration from .npmrc."),
    },
  },
  "lifecycle.trusted-dependencies": {
    featureId: "lifecycle.trusted-dependencies",
    title: "Dependency lifecycle allow-list",
    basis: [PNPM_SETTINGS, YARN_MANIFEST, BUN_INSTALL],
    targets: {
      bun: native("Bun natively represents trusted dependency lifecycle policy."),
      pnpm: transform("lifecycle.to-pnpm-build-policy", "Translate trusted dependencies into pnpm build policy."),
      "yarn-modern": transform("lifecycle.to-yarn-build-policy", "Translate lifecycle policy into Yarn settings and dependency metadata.", "medium"),
      npm: lossy("lifecycle.to-global-script-policy", "Reduce per-dependency policy to npm's broader script controls.", "high"),
      "yarn-classic": lossy("lifecycle.to-global-script-policy", "Reduce per-dependency policy to Yarn Classic's broader script controls.", "high"),
      vlt: unsupported("pkgshift cannot preserve a lifecycle allow-list while guaranteeing a script-free migration install."),
      deno: unsupported("pkgshift cannot preserve allowScripts while guaranteeing a script-free migration install."),
    },
  },
};

export function unknownOutcome(featureId: FeatureId, target: PackageManagerId): RuleOutcome {
  return unknown(`No capability rule is registered for ${featureId} on ${target}.`);
}
