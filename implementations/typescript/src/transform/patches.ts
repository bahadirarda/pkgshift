import { lstat } from "node:fs/promises";
import { posix } from "node:path";
import { satisfies, validRange } from "semver";
import { readText } from "../core/files.ts";
import { safeProjectFilePath, UnsafeProjectPathError } from "../core/project-path.ts";
import type { Diagnostic, PackageManagerId } from "../domain/models.ts";
import type { ProjectIR } from "../ir/models.ts";

export interface YarnPatchConversion {
  baseSpecifier: string;
  selector: string;
  path: string;
}

export interface PatchState {
  patchedDependencies: Record<string, string>;
  patchConversions: Map<string, YarnPatchConversion>;
  patchResolutions: Record<string, string>;
  remainingResolutions: Record<string, unknown>;
}

function diagnostic(code: string, summary: string, location?: string): Diagnostic {
  return {
    code,
    severity: "error",
    summary,
    blocking: true,
    ...(location ? { evidence: [{ location, detail: summary }] } : {}),
    remediation: ["Resolve the reported transformation boundary and create a new plan."],
  };
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

function exactSemver(value: string): boolean {
  return /^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$/.test(value);
}

function portableSemverRange(value: string): boolean {
  const normalized = value.trim();
  return normalized.length > 0
    && normalized.length <= 256
    && /[0-9*xX]/.test(normalized)
    && /^[0-9A-Za-z\s.\-+*^~<>=|]+$/.test(normalized)
    && !normalized.includes("|||")
    && normalized.split("||").every((branch) => branch.trim().length > 0)
    && validRange(normalized) !== null;
}

function packageSelector(selector: string): { name: string; range: string | null } | null {
  const match = selector.startsWith("@")
    ? selector.match(/^(@[^/@\s]+\/[^/@\s]+)(?:@(.+))?$/)
    : selector.match(/^([^@/\s]+)(?:@(.+))?$/);
  if (!match) return null;
  const range = match[2]?.trim() ?? null;
  if (range !== null && !portableSemverRange(range)) return null;
  return { name: match[1]!, range };
}

async function normalizePatchPath(
  root: string,
  rawPath: string,
  diagnostics: Diagnostic[],
): Promise<string | null> {
  const path = rawPath.replace(/^~\//, "").replace(/^\.\//, "");
  if (!safePath(path) || posix.extname(path) !== ".patch") {
    diagnostics.push(diagnostic(
      "PATCH_PATH_UNSUPPORTED",
      "A patch path is outside the deterministic project-relative subset.",
      path,
    ));
    return null;
  }
  let content: string | null = null;
  try {
    const absolutePath = await safeProjectFilePath(root, path);
    const state = await lstat(absolutePath);
    if (state.isSymbolicLink() || !state.isFile()) {
      diagnostics.push(diagnostic(
        "PATCH_PATH_UNSUPPORTED",
        "A patch path traverses a symbolic link or non-file project entry.",
        path,
      ));
      return null;
    }
    content = await readText(absolutePath);
  } catch (error) {
    if (error instanceof UnsafeProjectPathError) {
      diagnostics.push(diagnostic(
        "PATCH_PATH_UNSUPPORTED",
        "A patch path traverses a symbolic link or non-file project entry.",
        path,
      ));
      return null;
    }
  }
  if (content === null) {
    diagnostics.push(diagnostic(
      "PATCH_FILE_NOT_FOUND",
      "A configured patch file does not exist in the project.",
      path,
    ));
    return null;
  }
  const lines = content.split("\n");
  if (
    !lines.some((line) => line.startsWith("--- "))
    || !lines.some((line) => line.startsWith("+++ "))
    || !lines.some((line) => line.startsWith("@@ "))
    || content.includes("\0")
    || content.includes("GIT binary patch")
    || content.includes("Binary files ")
  ) {
    diagnostics.push(diagnostic(
      "PATCH_FORMAT_UNSUPPORTED",
      "A patch file is outside the portable text unified-diff subset.",
      path,
    ));
    return null;
  }
  return path;
}

function decodeReference(value: string): string | null {
  try {
    return decodeURIComponent(value);
  } catch {
    return null;
  }
}

function encodeReference(value: string): string {
  return [...value].map((character) =>
    /[0-9A-Za-z._~-]/.test(character)
      ? character
      : `%${character.charCodeAt(0).toString(16).toUpperCase().padStart(2, "0")}`
  ).join("");
}

async function yarnPatchConversion(
  root: string,
  name: string,
  specifier: string,
  target: PackageManagerId,
  diagnostics: Diagnostic[],
): Promise<YarnPatchConversion | null> {
  const value = specifier.slice("patch:".length);
  const hash = value.indexOf("#");
  if (hash < 1) {
    diagnostics.push(diagnostic(
      "PATCH_LOCATOR_UNSUPPORTED",
      "A Yarn patch locator does not include one local patch file.",
    ));
    return null;
  }
  const source = value.slice(0, hash);
  const rawPath = value.slice(hash + 1);
  if (/[&!%]/.test(rawPath) || rawPath.includes("::")) {
    diagnostics.push(diagnostic(
      "PATCH_LOCATOR_UNSUPPORTED",
      "A Yarn patch locator uses multiple, optional, encoded, or parameterized patch sources.",
    ));
    return null;
  }
  const prefix = `${name}@`;
  if (!source.startsWith(prefix)) {
    diagnostics.push(diagnostic(
      "PATCH_LOCATOR_UNSUPPORTED",
      "A Yarn patch locator aliases a different package identity.",
    ));
    return null;
  }
  const encoded = source.slice(prefix.length).replace(/^npm(?:%3A|%3a|:)/, "");
  const range = decodeReference(encoded);
  if (range === null || !portableSemverRange(range) || (target === "bun" && !exactSemver(range))) {
    diagnostics.push(diagnostic(
      "PATCH_SELECTOR_UNSUPPORTED",
      target === "bun"
        ? "Bun patch conversion requires one exact package version."
        : "A Yarn patch source is not a portable registry semver selector.",
    ));
    return null;
  }
  const path = await normalizePatchPath(root, rawPath, diagnostics);
  return path ? { baseSpecifier: range, selector: `${name}@${range}`, path } : null;
}

export async function buildPatchState(
  root: string,
  source: PackageManagerId,
  target: PackageManagerId,
  projectIr: ProjectIR,
  configured: Record<string, unknown>,
  configuredResolutions: Record<string, unknown>,
  diagnostics: Diagnostic[],
): Promise<PatchState> {
  const patchedDependencies: Record<string, string> = {};
  for (const [selector, rawPath] of Object.entries(configured)) {
    const parsed = packageSelector(selector);
    if (!parsed) {
      diagnostics.push(diagnostic(
        "PATCH_SELECTOR_UNSUPPORTED",
        "A patched dependency has an invalid package or semver selector.",
      ));
      continue;
    }
    if (target === "bun" && (parsed.range === null || !exactSemver(parsed.range))) {
      diagnostics.push(diagnostic(
        "PATCH_SELECTOR_UNSUPPORTED",
        "Bun patch conversion requires one exact package version.",
      ));
      continue;
    }
    if (typeof rawPath !== "string") {
      diagnostics.push(diagnostic(
        "PATCH_POLICY_UNSUPPORTED",
        "A patched dependency value is not a patch file path.",
      ));
      continue;
    }
    const path = await normalizePatchPath(root, rawPath, diagnostics);
    if (path) patchedDependencies[selector] = path;
  }

  const patchConversions = new Map<string, YarnPatchConversion>();
  const patchDependencies = projectIr.packages.flatMap((entry) => entry.dependencies)
    .filter((entry) => entry.protocol === "patch");
  const yarnPatchResolutions = Object.entries(configuredResolutions).filter(
    (entry): entry is [string, string] => typeof entry[1] === "string" && entry[1].startsWith("patch:"),
  );
  if ((patchDependencies.length > 0 || yarnPatchResolutions.length > 0) && source !== "yarn-modern") {
    diagnostics.push(diagnostic(
      "PATCH_SOURCE_UNSUPPORTED",
      "A Yarn patch protocol dependency was found outside a Yarn Modern project.",
    ));
  } else {
    for (const dependency of patchDependencies) {
      const conversion = await yarnPatchConversion(
        root,
        dependency.name,
        dependency.specifier,
        target,
        diagnostics,
      );
      if (!conversion) continue;
      const existing = patchedDependencies[conversion.selector];
      if (existing && existing !== conversion.path) {
        diagnostics.push(diagnostic(
          "PATCH_POLICY_CONFLICT",
          "Multiple patch declarations target the same package selector with different files.",
          dependency.evidence.location,
        ));
        continue;
      }
      patchedDependencies[conversion.selector] = conversion.path;
      patchConversions.set(
        `${dependency.evidence.location}#/${dependency.section}/${dependency.name}`,
        conversion,
      );
    }
    for (const [selector, specifier] of yarnPatchResolutions) {
      const match = specifier.match(/^patch:((?:@[^/@]+\/)?[^@]+)@/);
      const name = match?.[1];
      if (!name) {
        diagnostics.push(diagnostic(
          "PATCH_LOCATOR_UNSUPPORTED",
          "A Yarn patch resolution does not identify one registry package.",
          "package.json",
        ));
        continue;
      }
      const conversion = await yarnPatchConversion(root, name, specifier, target, diagnostics);
      if (!conversion) continue;
      const expectedResolution = `${name}@npm:${conversion.baseSpecifier}`;
      if (selector !== expectedResolution && selector !== conversion.selector) {
        diagnostics.push(diagnostic(
          "PATCH_SELECTOR_UNSUPPORTED",
          "A Yarn patch resolution selector does not match its patch locator.",
          "package.json",
        ));
        continue;
      }
      const existing = patchedDependencies[conversion.selector];
      if (existing && existing !== conversion.path) {
        diagnostics.push(diagnostic(
          "PATCH_POLICY_CONFLICT",
          "Multiple patch declarations target the same package selector with different files.",
          "package.json",
        ));
        continue;
      }
      patchedDependencies[conversion.selector] = conversion.path;
    }
  }

  const observedVersions = new Map<string, Set<string>>();
  for (const dependency of projectIr.packages.flatMap((entry) => entry.dependencies)) {
    if (dependency.protocol !== "semver" || !exactSemver(dependency.specifier)) continue;
    const versions = observedVersions.get(dependency.name) ?? new Set<string>();
    versions.add(dependency.specifier);
    observedVersions.set(dependency.name, versions);
  }
  const selected = new Map<string, { name: string; version: string; path: string; specificity: number }>();
  for (const [selector, path] of target === "yarn-modern" ? Object.entries(patchedDependencies) : []) {
    const parsed = packageSelector(selector);
    if (!parsed) continue;
    const specificity = parsed.range === null ? 0 : exactSemver(parsed.range) ? 2 : 1;
    const candidates = parsed.range !== null && exactSemver(parsed.range)
      ? [parsed.range]
      : [...(observedVersions.get(parsed.name) ?? [])].filter((version) =>
        parsed.range === null || satisfies(version, parsed.range)
      );
    if (candidates.length === 0) {
      diagnostics.push(diagnostic(
        "PATCH_RESOLUTION_EVIDENCE_MISSING",
        `The patch selector ${selector} cannot be expanded to an exact Yarn locator.`,
      ));
      continue;
    }
    for (const version of candidates) {
      const key = `${parsed.name}@npm:${version}`;
      const existing = selected.get(key);
      if (existing?.specificity === specificity && existing.path !== path) {
        diagnostics.push(diagnostic(
          "PATCH_POLICY_CONFLICT",
          `Equal-priority patch selectors map ${parsed.name}@${version} to different files.`,
        ));
      } else if (!existing || existing.specificity < specificity) {
        selected.set(key, { name: parsed.name, version, path, specificity });
      }
    }
  }
  const patchResolutions = Object.fromEntries([...selected.entries()].map(([selector, patch]) => [
    selector,
    `patch:${patch.name}@npm%3A${encodeReference(patch.version)}#~/${patch.path}`,
  ]));
  const remainingResolutions = Object.fromEntries(
    Object.entries(configuredResolutions).filter(([, value]) =>
      typeof value !== "string" || !value.startsWith("patch:")
    ),
  );
  return { patchedDependencies, patchConversions, patchResolutions, remainingResolutions };
}
