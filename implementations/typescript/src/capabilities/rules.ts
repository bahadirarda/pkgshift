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

const notApplicable = (summary: string): RuleOutcome => ({
  classification: "not-applicable",
  risk: "none",
  summary,
});

export const CAPABILITY_RULES: Record<FeatureId, CapabilityRule> = {
  "workspace.manifest": {
    featureId: "workspace.manifest",
    title: "Workspace membership",
    basis: [NPM_MANIFEST, PNPM_WORKSPACES, YARN_MANIFEST, BUN_WORKSPACES],
    targets: {
      npm: native("npm represents workspace membership in package.json."),
      pnpm: native("pnpm represents workspace membership in pnpm-workspace.yaml."),
      "yarn-classic": native("Yarn Classic represents workspace membership in package.json."),
      "yarn-modern": native("Yarn Modern represents workspace membership in package.json."),
      bun: native("Bun represents workspace membership in package.json."),
      vlt: unknown("The preview adapter has not verified complete workspace membership semantics."),
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
      vlt: unknown("The preview adapter has not verified exclusion pattern behavior."),
      deno: unknown("The preview adapter has not verified exclusion pattern behavior."),
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
      vlt: unknown("The preview adapter has not verified workspace: protocol behavior."),
      deno: unknown("Deno dependency-mode workspace protocol mapping requires adapter verification."),
    },
  },
  "dependency.catalog-protocol": {
    featureId: "dependency.catalog-protocol",
    title: "Catalog dependency protocol",
    basis: [PNPM_CATALOGS, BUN_WORKSPACES],
    targets: {
      pnpm: native("pnpm natively supports catalog: dependency specifiers."),
      bun: native("Bun natively supports catalog: dependency specifiers."),
      npm: lossy("catalog.expand-to-range", "Expand catalog references to ranges and lose centralized catalog policy."),
      "yarn-classic": lossy("catalog.expand-to-range", "Expand catalog references to ranges and lose centralized catalog policy."),
      "yarn-modern": lossy("catalog.expand-to-range", "Expand catalog references to ranges and lose centralized catalog policy."),
      vlt: unknown("The preview adapter has not verified catalog protocol behavior."),
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
      vlt: unknown("The preview adapter has not verified patch protocol behavior."),
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
      vlt: unknown("The preview adapter has not verified portal semantics."),
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
      vlt: unknown("The preview adapter has not verified link protocol behavior."),
      deno: unsupported("Deno dependency mode has no supported link protocol mapping in the MVP boundary."),
    },
  },
  "policy.catalogs": {
    featureId: "policy.catalogs",
    title: "Central dependency catalogs",
    basis: [PNPM_CATALOGS, BUN_WORKSPACES],
    targets: {
      pnpm: native("pnpm natively represents default and named catalogs."),
      bun: native("Bun natively represents default and named catalogs."),
      npm: lossy("catalog.expand-policy", "Expand catalog policy into manifests and lose centralized version governance."),
      "yarn-classic": lossy("catalog.expand-policy", "Expand catalog policy into manifests and lose centralized version governance."),
      "yarn-modern": lossy("catalog.expand-policy", "Expand catalog policy into manifests and lose centralized version governance."),
      vlt: unknown("The preview adapter has not verified catalog policy behavior."),
      deno: lossy("catalog.expand-policy", "Expand catalog policy into Deno-compatible dependency declarations."),
    },
  },
  "resolution.overrides": {
    featureId: "resolution.overrides",
    title: "Dependency overrides",
    basis: [NPM_MANIFEST, PNPM_SETTINGS, BUN_OVERRIDES, YARN_MANIFEST],
    targets: {
      npm: native("npm natively represents dependency overrides."),
      pnpm: transform("overrides.to-pnpm", "Move compatible overrides into pnpm workspace settings."),
      bun: native("Bun natively supports top-level npm overrides."),
      "yarn-classic": lossy("overrides.to-resolutions", "Convert compatible overrides to Yarn resolutions and review selector differences."),
      "yarn-modern": lossy("overrides.to-resolutions", "Convert compatible overrides to Yarn resolutions and review selector differences."),
      vlt: unknown("The preview adapter has not verified override selector parity."),
      deno: unsupported("Deno dependency mode has no supported override mapping in the MVP boundary."),
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
      vlt: unknown("The preview adapter has not verified nested override behavior."),
      deno: unsupported("Deno dependency mode has no supported nested override mapping."),
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
      vlt: unknown("The preview adapter has not verified resolution selector parity."),
      deno: unsupported("Deno dependency mode has no supported resolution policy mapping."),
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
      vlt: unknown("The preview adapter has not verified package extensions."),
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
      vlt: unknown("The preview adapter has not verified patched dependency behavior."),
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
      vlt: unknown("The preview adapter has not verified Plug and Play behavior."),
      deno: notApplicable("Deno dependency mode does not use a Node installation linker."),
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
      vlt: unknown("The preview adapter has not verified isolated linker behavior."),
      deno: notApplicable("Deno dependency mode does not use a Node installation linker."),
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
      vlt: unknown("The preview adapter has not verified constraint policy behavior."),
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
      vlt: unknown("The preview adapter has not verified hook extensibility."),
      deno: unsupported("Deno dependency mode has no equivalent pnpm hook boundary."),
    },
  },
  "registry.npmrc": {
    featureId: "registry.npmrc",
    title: "npm-compatible registry configuration",
    basis: [NPM_MANIFEST, PNPM_SETTINGS, YARN_MANIFEST, BUN_INSTALL],
    targets: {
      npm: native("npm natively consumes .npmrc registry configuration."),
      pnpm: native("pnpm consumes authentication and registry settings from .npmrc."),
      "yarn-classic": native("Yarn Classic consumes npm-compatible registry configuration."),
      bun: native("Bun consumes npm-compatible registry configuration."),
      "yarn-modern": transform("registry.npmrc-to-yarnrc", "Translate registry scopes into Yarn Modern configuration while preserving secret references.", "medium"),
      vlt: unknown("The preview adapter has not verified registry configuration parity."),
      deno: unknown("Deno registry and credential mapping requires preview adapter verification."),
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
      vlt: unknown("The preview adapter has not verified lifecycle allow-list behavior."),
      deno: unsupported("Deno dependency mode has no equivalent lifecycle allow-list in the MVP boundary."),
    },
  },
};

export function unknownOutcome(featureId: FeatureId, target: PackageManagerId): RuleOutcome {
  return unknown(`No capability rule is registered for ${featureId} on ${target}.`);
}

