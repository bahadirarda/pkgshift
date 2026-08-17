import type {
  NativeImportStrategy,
  PackageManagerId,
  SupportTier,
} from "../domain/models.ts";

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
    lockfiles: ["npm-shrinkwrap.json", "package-lock.json"],
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
    tier: "production-target",
    aliases: ["vlt"],
    lockfiles: ["vlt-lock.json"],
    configurationFiles: ["vlt.json"],
    installCommand: ["vlt", "install"],
    implementationStatus: "production",
    packageManagerPin: "vlt@1.0.2",
    scope: "vlt manifests, lockfiles, workspaces, catalogs, graph modifiers, registries, CI, and containers",
  },
  {
    id: "deno",
    displayName: "Deno dependency mode",
    tier: "production-target",
    aliases: ["deno"],
    lockfiles: ["deno.lock"],
    configurationFiles: ["deno.json", "deno.jsonc", ".npmrc"],
    installCommand: ["deno", "install"],
    implementationStatus: "production",
    packageManagerPin: "deno@2.9.5",
    scope: "Deno npm-compatible dependency declarations, lockfiles, workspaces, registries, CI, and containers; runtime modernization remains out of scope",
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

export function nativeImportStrategy(
  source: PackageManagerId,
  target: PackageManagerId,
  sourceLockfilePresent: boolean,
): NativeImportStrategy | null {
  if (!sourceLockfilePresent) return null;
  if (
    target === "pnpm"
    && ["npm", "yarn-classic", "yarn-modern"].includes(source)
  ) {
    return {
      id: "pnpm-import",
      source,
      target,
      mode: "dedicated-command",
      command: ["pnpm", "import"],
      summary: "Generate pnpm dependency state with pnpm's native lockfile importer.",
    };
  }
  if (
    target === "bun"
    && ["npm", "yarn-classic", "yarn-modern"].includes(source)
  ) {
    return {
      id: "bun-pm-migrate",
      source,
      target,
      mode: "dedicated-command",
      command: ["bun", "pm", "migrate"],
      summary: "Generate Bun dependency state with bun pm migrate.",
    };
  }
  if (source === "pnpm" && target === "bun") {
    return {
      id: "bun-pnpm-install-migration",
      source,
      target,
      mode: "install-integrated",
      command: installCommandFor(target),
      summary: "Use Bun's install-integrated pnpm lockfile migration path.",
    };
  }
  if (source === "npm" && target === "yarn-classic") {
    return {
      id: "yarn-classic-import",
      source,
      target,
      mode: "dedicated-command",
      command: ["yarn", "import"],
      summary: "Generate Yarn Classic dependency state with yarn import.",
    };
  }
  if (source === "yarn-classic" && target === "yarn-modern") {
    return {
      id: "yarn-modern-install-migration",
      source,
      target,
      mode: "install-integrated",
      command: installCommandFor(target),
      summary: "Use Yarn Modern's install-integrated Yarn Classic migration path.",
    };
  }
  if (source === "yarn-classic" && target === "npm") {
    return {
      id: "npm-yarn-lock-install",
      source,
      target,
      mode: "install-integrated",
      command: installCommandFor(target),
      summary: "Use npm's yarn.lock-aware installation path.",
    };
  }
  if (
    target === "deno"
    && source === "npm"
  ) {
    return {
      id: "deno-install-migration",
      source,
      target,
      mode: "install-integrated",
      command: installCommandFor(target),
      summary: "Use Deno's install-integrated Node dependency migration path.",
    };
  }
  return null;
}

function installCommandFor(target: PackageManagerId): string[] {
  const base = [...getPackageManager(target).installCommand];
  if (["npm", "pnpm", "yarn-classic", "bun"].includes(target)) {
    return [...base, "--ignore-scripts"];
  }
  if (target === "yarn-modern") return [...base, "--mode=skip-build"];
  return base;
}
