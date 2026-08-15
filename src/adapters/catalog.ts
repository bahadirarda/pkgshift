import type { PackageManagerId, SupportTier } from "../domain/models.ts";

export interface PackageManagerDefinition {
  id: PackageManagerId;
  displayName: string;
  tier: SupportTier;
  aliases: string[];
  lockfiles: string[];
  configurationFiles: string[];
  installCommand: string[];
  implementationStatus: "production" | "preview";
  packageManagerPin: string;
  scope: string;
}

export const PACKAGE_MANAGERS: readonly PackageManagerDefinition[] = [
  {
    id: "npm",
    displayName: "npm",
    tier: "production-target",
    aliases: ["npm"],
    lockfiles: ["package-lock.json", "npm-shrinkwrap.json"],
    configurationFiles: [".npmrc"],
    installCommand: ["npm", "install"],
    implementationStatus: "production",
    packageManagerPin: "npm@12.0.2",
    scope: "npm manifests, lockfiles, workspaces, overrides, scripts, registries, CI, and containers",
  },
  {
    id: "pnpm",
    displayName: "pnpm",
    tier: "production-target",
    aliases: ["pnpm"],
    lockfiles: ["pnpm-lock.yaml"],
    configurationFiles: ["pnpm-workspace.yaml", ".npmrc", ".pnpmfile.cjs"],
    installCommand: ["pnpm", "install"],
    implementationStatus: "production",
    packageManagerPin: "pnpm@11.21.0",
    scope: "pnpm workspaces, catalogs, overrides, patches, linker policy, registries, CI, and containers",
  },
  {
    id: "yarn-classic",
    displayName: "Yarn Classic",
    tier: "production-target",
    aliases: ["yarn-classic", "yarn@1"],
    lockfiles: ["yarn.lock"],
    configurationFiles: [".yarnrc", ".npmrc"],
    installCommand: ["yarn", "install"],
    implementationStatus: "production",
    packageManagerPin: "yarn@1.22.22",
    scope: "Yarn 1 lockfiles, workspaces, resolutions, scripts, registries, CI, and containers",
  },
  {
    id: "yarn-modern",
    displayName: "Yarn Modern",
    tier: "production-target",
    aliases: ["yarn-modern", "yarn-berry"],
    lockfiles: ["yarn.lock"],
    configurationFiles: [".yarnrc.yml", ".pnp.cjs", ".pnp.loader.mjs"],
    installCommand: ["yarn", "install"],
    implementationStatus: "production",
    packageManagerPin: "yarn@4.18.0",
    scope: "Yarn modern workspaces, protocols, constraints, patches, plugins, linker modes, CI, and containers",
  },
  {
    id: "bun",
    displayName: "Bun",
    tier: "production-target",
    aliases: ["bun"],
    lockfiles: ["bun.lock", "bun.lockb"],
    configurationFiles: ["bunfig.toml", ".npmrc"],
    installCommand: ["bun", "install"],
    implementationStatus: "production",
    packageManagerPin: "bun@1.3.14",
    scope: "Bun lockfiles, workspaces, overrides, catalogs, scripts, registries, CI, and containers",
  },
  {
    id: "vlt",
    displayName: "vlt",
    tier: "preview-target",
    aliases: ["vlt"],
    lockfiles: ["vlt-lock.json"],
    configurationFiles: ["vlt.json", ".npmrc"],
    installCommand: ["vlt", "install"],
    implementationStatus: "preview",
    packageManagerPin: "vlt@1.0.2",
    scope: "preview detection, capability reporting, and guarded migration planning",
  },
  {
    id: "deno",
    displayName: "Deno dependency mode",
    tier: "preview-target",
    aliases: ["deno"],
    lockfiles: ["deno.lock"],
    configurationFiles: ["deno.json", "deno.jsonc"],
    installCommand: ["deno", "install"],
    implementationStatus: "preview",
    packageManagerPin: "deno@2.9.5",
    scope: "preview npm and JSR dependency declarations and workspaces, excluding runtime migration",
  },
] as const;

export function getPackageManager(
  id: PackageManagerId,
): PackageManagerDefinition {
  const definition = PACKAGE_MANAGERS.find((candidate) => candidate.id === id);
  if (!definition) {
    throw new Error(`Unknown package manager definition: ${id}`);
  }
  return definition;
}

export function normalizePackageManagerId(
  value: string,
): PackageManagerId | null {
  const normalized = value.trim().toLowerCase();
  const exact = PACKAGE_MANAGERS.find((candidate) => candidate.id === normalized);
  if (exact) {
    return exact.id;
  }
  if (normalized === "yarn@1" || normalized === "yarn-1") {
    return "yarn-classic";
  }
  if (normalized === "yarn@modern" || normalized === "yarn-berry") {
    return "yarn-modern";
  }
  return null;
}
