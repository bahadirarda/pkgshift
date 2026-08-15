import { join, resolve } from "node:path";
import type {
  Diagnostic,
  IntegrationInspection,
  ProjectInspection,
  WorkspaceInspection,
} from "../domain/models.ts";
import {
  directoryExists,
  fingerprintFiles,
  pathExists,
  readJsonObject,
  readText,
  walkFiles,
} from "../core/files.ts";
import { detectPackageManager } from "./detect-package-manager.ts";

function workspacePatterns(manifest: Record<string, unknown> | null): string[] {
  const workspaces = manifest?.workspaces;
  if (Array.isArray(workspaces)) {
    return workspaces.filter((item): item is string => typeof item === "string");
  }
  if (workspaces && typeof workspaces === "object" && !Array.isArray(workspaces)) {
    const packages = (workspaces as Record<string, unknown>).packages;
    if (Array.isArray(packages)) {
      return packages.filter((item): item is string => typeof item === "string");
    }
  }
  return [];
}

function integrationKind(path: string): IntegrationInspection["kind"] {
  if (path.startsWith(".github/workflows/") || path === ".gitlab-ci.yml" || path === "azure-pipelines.yml") {
    return "ci";
  }
  if (path.toLowerCase().includes("docker") || path === "Containerfile") {
    return "container";
  }
  if (path.toLowerCase().startsWith("readme")) {
    return "documentation";
  }
  return "automation";
}

function isIntegrationFile(path: string): boolean {
  const basename = path.split("/").at(-1) ?? path;
  return (
    path.startsWith(".github/workflows/") && /\.ya?ml$/i.test(path)
  ) || [
    ".gitlab-ci.yml",
    "azure-pipelines.yml",
    "Jenkinsfile",
    "Containerfile",
    "Dockerfile",
    "docker-compose.yml",
    "docker-compose.yaml",
    "README.md",
    "readme.md",
  ].includes(path) || /^Dockerfile\./.test(basename);
}

function isMigrationRelevantFile(path: string): boolean {
  if (path === "package.json" || path.endsWith("/package.json")) {
    return true;
  }
  if (path.startsWith(".yarn/patches/") && path.endsWith(".patch")) {
    return true;
  }
  return [
    ".npmrc",
    ".pnpmfile.cjs",
    ".pnp.cjs",
    ".pnp.loader.mjs",
    ".yarnrc",
    ".yarnrc.yml",
    "bunfig.toml",
    "deno.json",
    "deno.jsonc",
    "npm-shrinkwrap.json",
    "package-lock.json",
    "pnpm-lock.yaml",
    "pnpm-workspace.yaml",
    "vlt-lock.json",
    "vlt.json",
    "yarn.config.cjs",
    "yarn.lock",
    "bun.lock",
    "bun.lockb",
    "deno.lock",
  ].includes(path);
}

async function inspectIntegrations(root: string): Promise<IntegrationInspection[]> {
  const files = await walkFiles(root, isIntegrationFile);
  const integrations: IntegrationInspection[] = [];
  for (const path of files) {
    const content = await readText(join(root, path));
    if (content === null || content.length > 512_000) {
      continue;
    }
    const tokens = ["npm", "pnpm", "yarn", "bun", "vlt", "deno"].filter((token) =>
      new RegExp(`\\b${token}\\b`, "i").test(content),
    );
    if (tokens.length > 0) {
      integrations.push({
        kind: integrationKind(path),
        path,
        packageManagerTokens: tokens,
      });
    }
  }
  return integrations;
}

async function inspectWorkspace(
  root: string,
  manifest: Record<string, unknown> | null,
): Promise<WorkspaceInspection> {
  const sources: WorkspaceInspection["sources"] = [];
  const manifestPatterns = workspacePatterns(manifest);
  if (manifestPatterns.length > 0) {
    sources.push({ location: "package.json", patterns: manifestPatterns });
  }
  if (await pathExists(join(root, "pnpm-workspace.yaml"))) {
    sources.push({ location: "pnpm-workspace.yaml", patterns: [] });
  }
  if (await pathExists(join(root, "deno.json"))) {
    sources.push({ location: "deno.json", patterns: [] });
  } else if (await pathExists(join(root, "deno.jsonc"))) {
    sources.push({ location: "deno.jsonc", patterns: [] });
  }
  return { configured: sources.length > 0, sources };
}

export async function inspectProject(cwd: string): Promise<ProjectInspection> {
  const root = resolve(cwd);
  if (!(await directoryExists(root))) {
    const diagnostic: Diagnostic = {
      code: "REPOSITORY_ROOT_NOT_FOUND",
      severity: "error",
      summary: "The selected repository root does not exist or is not a directory.",
      blocking: true,
      evidence: [{ location: root, detail: "Directory lookup failed" }],
      remediation: ["Pass --cwd with an existing repository directory."],
    };
    return {
      root,
      fingerprint: await fingerprintFiles(root, []),
      relevantFiles: [],
      manifest: null,
      packageManager: {
        selected: null,
        candidates: [],
        evidence: [],
        diagnostics: [],
      },
      workspace: { configured: false, sources: [] },
      integrations: [],
      diagnostics: [diagnostic],
    };
  }
  const manifestPath = join(root, "package.json");
  const diagnostics: Diagnostic[] = [];
  let manifest: Record<string, unknown> | null = null;

  try {
    manifest = await readJsonObject(manifestPath);
  } catch (error) {
    diagnostics.push({
      code: "MANIFEST_INVALID_JSON",
      severity: "error",
      summary: error instanceof Error ? error.message : "package.json is invalid.",
      blocking: true,
      evidence: [{ location: "package.json", detail: "JSON parsing failed" }],
      remediation: ["Repair package.json and inspect the repository again."],
    });
  }

  if (manifest === null && diagnostics.length === 0) {
    diagnostics.push({
      code: "MANIFEST_NOT_FOUND",
      severity: "error",
      summary: "No package.json was found at the repository root.",
      blocking: true,
      remediation: ["Run from a JavaScript project root or pass --cwd."],
    });
  }

  const packageManager = await detectPackageManager(root, manifest);
  const workspace = await inspectWorkspace(root, manifest);
  const integrations = await inspectIntegrations(root);
  const relevantFiles = await walkFiles(root, isMigrationRelevantFile);
  const relevantPaths = [
    ...relevantFiles,
    ...packageManager.evidence.map((item) => item.location),
    ...workspace.sources.map((item) => item.location),
    ...integrations.map((item) => item.path),
  ];
  const fingerprint = await fingerprintFiles(root, relevantPaths);
  const allDiagnostics = [...diagnostics, ...packageManager.diagnostics];

  return {
    root,
    fingerprint,
    relevantFiles,
    manifest: manifest ? {
      path: "package.json",
      name: typeof manifest.name === "string" ? manifest.name : null,
      private: typeof manifest.private === "boolean" ? manifest.private : null,
      packageManager: typeof manifest.packageManager === "string" ? manifest.packageManager : null,
    } : null,
    packageManager,
    workspace,
    integrations,
    diagnostics: allDiagnostics,
  };
}
