import { stat } from "node:fs/promises";
import { basename, dirname, join, resolve } from "node:path";
import { isCalendarVersion } from "./release/calendar-version.ts";

const repositoryRoot = resolve(import.meta.dir, "../../..");
process.chdir(repositoryRoot);

const errors: string[] = [];

interface PackageMetadata {
  name?: string;
  version?: string;
  private?: boolean;
}

interface CargoManifest {
  package?: {
    name?: string;
  };
  workspace?: {
    package?: {
      version?: string;
    };
  };
  dependencies?: {
    "pkgshift-core"?: {
      version?: string;
    };
  };
}

interface ChangesetsConfig {
  fixed?: string[][];
  privatePackages?: {
    version?: boolean;
    tag?: boolean;
  };
}

interface BunLock {
  workspaces?: Record<string, { version?: string }>;
}

async function filesMatching(pattern: string): Promise<string[]> {
  return (await Array.fromAsync(
    new Bun.Glob(pattern).scan({ cwd: ".", dot: true, onlyFiles: true }),
  )).sort();
}

function parseFrontmatter(
  path: string,
  content: string,
): Record<string, unknown> | null {
  const match = content.match(/^---\n([\s\S]*?)\n---(?:\n|$)/);
  if (!match?.[1]) {
    errors.push(`${path}: missing or malformed YAML frontmatter`);
    return null;
  }
  try {
    const parsed: unknown = Bun.YAML.parse(match[1]);
    if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
      errors.push(`${path}: frontmatter must be a YAML mapping`);
      return null;
    }
    return parsed as Record<string, unknown>;
  } catch (error) {
    const message = error instanceof Error ? error.message : "unknown YAML error";
    errors.push(`${path}: ${message}`);
    return null;
  }
}

async function targetExists(path: string): Promise<boolean> {
  try {
    await stat(path);
    return true;
  } catch {
    return false;
  }
}

async function validateLinks(
  path: string,
  content: string,
  bundleRoot: string | null,
): Promise<void> {
  for (const match of content.matchAll(/\[[^\]]*\]\(([^)]+)\)/g)) {
    const rawTarget = match[1]?.split("#")[0];
    if (!rawTarget || /^(https?:|mailto:)/.test(rawTarget)) {
      continue;
    }
    const target = rawTarget.startsWith("/") && bundleRoot
      ? join(bundleRoot, rawTarget)
      : resolve(dirname(path), rawTarget);
    if (!(await targetExists(target))) {
      errors.push(`${path}: broken link ${rawTarget}`);
    }
  }
}

async function validateOkf(): Promise<number> {
  const paths = await filesMatching("docs/**/*.md");
  for (const path of paths) {
    const content = await Bun.file(path).text();
    const filename = basename(path);
    if (filename === "index.md") {
      if (path === "docs/index.md") {
        const frontmatter = parseFrontmatter(path, content);
        if (frontmatter) {
          const keys = Object.keys(frontmatter);
          if (frontmatter.okf_version !== "0.2" || keys.some((key) => key !== "okf_version")) {
            errors.push(`${path}: root index may contain only okf_version 0.2`);
          }
        }
      } else if (content.startsWith("---")) {
        errors.push(`${path}: nested index files must not contain frontmatter`);
      }
    } else if (filename === "log.md") {
      if (content.startsWith("---")) {
        errors.push(`${path}: log files must not contain frontmatter`);
      }
      const headings = [...content.matchAll(/^## (.+)$/gm)].map((match) => match[1]);
      if (headings.some((heading) => !/^\d{4}-\d{2}-\d{2}$/.test(heading ?? ""))) {
        errors.push(`${path}: log date headings must use YYYY-MM-DD`);
      }
    } else {
      const frontmatter = parseFrontmatter(path, content);
      if (frontmatter) {
        if (typeof frontmatter.type !== "string" || !frontmatter.type.trim()) {
          errors.push(`${path}: type must be a non-empty string`);
        }
        if (
          frontmatter.status !== undefined
          && !["draft", "stable", "deprecated"].includes(String(frontmatter.status))
        ) {
          errors.push(`${path}: unsupported lifecycle status`);
        }
        if (frontmatter.sources !== undefined) {
          if (!Array.isArray(frontmatter.sources)) {
            errors.push(`${path}: sources must be a YAML list`);
          } else {
            for (const source of frontmatter.sources) {
              const resource = source && typeof source === "object"
                ? (source as Record<string, unknown>).resource
                : undefined;
              if (
                typeof resource !== "string"
                || !resource.trim()
              ) {
                errors.push(`${path}: every source entry requires a resource`);
              }
            }
          }
        }
      }
      const body = content.replace(/^---\n[\s\S]*?\n---\n?/, "");
      if (!/^# /m.test(body)) {
        errors.push(`${path}: concept body requires a top-level heading`);
      }
    }
    await validateLinks(path, content, "docs");
  }
  return paths.length;
}

async function validateSkill(): Promise<void> {
  const path = "skills/pkgshift/SKILL.md";
  const content = await Bun.file(path).text();
  const frontmatter = parseFrontmatter(path, content);
  if (frontmatter) {
    const keys = Object.keys(frontmatter);
    if (keys.some((key) => !["name", "description"].includes(key))) {
      errors.push(`${path}: portable frontmatter may contain only name and description`);
    }
    const name = frontmatter.name;
    if (
      typeof name !== "string"
      || !/^[a-z0-9-]+$/.test(name)
      || name.startsWith("-")
      || name.endsWith("-")
      || name.includes("--")
      || name.length > 64
    ) {
      errors.push(`${path}: invalid skill name`);
    }
    const description = frontmatter.description;
    if (
      typeof description !== "string"
      || !description.trim()
      || description.length > 1024
      || /[<>]/.test(description)
    ) {
      errors.push(`${path}: invalid skill description`);
    }
  }
  if (content.split("\n").length > 500) {
    errors.push(`${path}: SKILL.md must remain below 500 lines`);
  }
  await validateLinks(path, content, null);

  const openAiPath = "skills/pkgshift/agents/openai.yaml";
  const metadata = Bun.YAML.parse(await Bun.file(openAiPath).text()) as {
    interface?: {
      short_description?: string;
      default_prompt?: string;
    };
  };
  const shortDescription = metadata.interface?.short_description ?? "";
  if (shortDescription.length < 25 || shortDescription.length > 64) {
    errors.push(`${openAiPath}: short_description must contain 25 to 64 characters`);
  }
  if (!metadata.interface?.default_prompt?.includes("$pkgshift")) {
    errors.push(`${openAiPath}: default_prompt must mention $pkgshift`);
  }
}

async function validateChangesets(): Promise<number> {
  const allowedPackages = new Set([
    "pkgshift",
    "pkgshift-core",
    "@bahadirarda/pkgshift-typescript",
  ]);
  const paths = (await filesMatching(".changeset/*.md"))
    .filter((path) => !path.endsWith("/README.md"));
  for (const path of paths) {
    const content = await Bun.file(path).text();
    const frontmatter = parseFrontmatter(path, content);
    if (frontmatter) {
      const packages = Object.keys(frontmatter);
      if (packages.length === 0) {
        errors.push(`${path}: Changeset must select at least one release package`);
      }
      for (const packageName of packages) {
        if (!allowedPackages.has(packageName)) {
          errors.push(`${path}: unsupported release package ${packageName}`);
        }
        if (!new Set(["major", "minor", "patch"]).has(String(frontmatter[packageName]))) {
          errors.push(`${path}: ${packageName} requires a major, minor, or patch bump`);
        }
      }
    }
    const summary = content.replace(/^---\n[\s\S]*?\n---\n?/, "").trim();
    if (!summary) {
      errors.push(`${path}: Changeset summary must not be empty`);
    }
  }
  return paths.length;
}

async function validateReleaseMetadata(): Promise<void> {
  const rootCargo = Bun.TOML.parse(
    await Bun.file("Cargo.toml").text(),
  ) as CargoManifest;
  const coreCargo = Bun.TOML.parse(
    await Bun.file("implementations/rust/pkgshift-core/Cargo.toml").text(),
  ) as CargoManifest;
  const cliCargo = Bun.TOML.parse(
    await Bun.file("implementations/rust/pkgshift-cli/Cargo.toml").text(),
  ) as CargoManifest;
  const rootPackage = await Bun.file("package.json").json() as PackageMetadata;
  const typescriptPackage = await Bun.file(
    "implementations/typescript/package.json",
  ).json() as PackageMetadata;
  const cliProxy = await Bun.file(
    "implementations/rust/pkgshift-cli/package.json",
  ).json() as PackageMetadata;
  const coreProxy = await Bun.file(
    "implementations/rust/pkgshift-core/package.json",
  ).json() as PackageMetadata;

  const version = rootCargo.workspace?.package?.version;
  const semver = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-(?:0|[1-9]\d*|\d*[a-zA-Z-][0-9a-zA-Z-]*)(?:\.(?:0|[1-9]\d*|\d*[a-zA-Z-][0-9a-zA-Z-]*))*)?(?:\+[0-9a-zA-Z-]+(?:\.[0-9a-zA-Z-]+)*)?$/;
  if (!version || !semver.test(version)) {
    errors.push("Cargo.toml: workspace.package.version must be valid SemVer");
    return;
  }
  const legacyTransition = version === "0.2.0"
    && await targetExists(".changeset/calendar-versioning.md");
  if (!isCalendarVersion(version) && !legacyTransition) {
    errors.push(
      "Cargo.toml: workspace.package.version must use 0.YYYYMMDD.REVISION calendar SemVer",
    );
  }

  const synchronizedVersions = [
    ["package.json", rootPackage.version],
    ["implementations/typescript/package.json", typescriptPackage.version],
    ["implementations/rust/pkgshift-cli/package.json", cliProxy.version],
    ["implementations/rust/pkgshift-core/package.json", coreProxy.version],
    [
      "implementations/rust/pkgshift-cli/Cargo.toml pkgshift-core dependency",
      cliCargo.dependencies?.["pkgshift-core"]?.version,
    ],
  ] as const;
  for (const [path, candidate] of synchronizedVersions) {
    if (candidate !== version) {
      errors.push(`${path}: expected version ${version}, found ${candidate ?? "missing"}`);
    }
  }

  const bunLock = Bun.JSONC.parse(await Bun.file("bun.lock").text()) as BunLock;
  for (const path of [
    "implementations/rust/pkgshift-cli",
    "implementations/rust/pkgshift-core",
    "implementations/typescript",
  ]) {
    const candidate = bunLock.workspaces?.[path]?.version;
    if (candidate !== version) {
      errors.push(`bun.lock ${path}: expected version ${version}, found ${candidate ?? "missing"}`);
    }
  }

  if (rootPackage.name !== "pkgshift-workspace" || rootPackage.private !== true) {
    errors.push("package.json: root package must remain the private pkgshift-workspace");
  }
  if (
    typescriptPackage.name !== "@bahadirarda/pkgshift-typescript"
    || typescriptPackage.private !== true
  ) {
    errors.push(
      "implementations/typescript/package.json: reference package must remain private and explicitly named",
    );
  }
  if (coreCargo.package?.name !== "pkgshift-core") {
    errors.push("implementations/rust/pkgshift-core/Cargo.toml: public crate must be named pkgshift-core");
  }
  if (cliCargo.package?.name !== "pkgshift") {
    errors.push("implementations/rust/pkgshift-cli/Cargo.toml: public CLI crate must be named pkgshift");
  }
  if (cliProxy.name !== "pkgshift" || cliProxy.private !== true) {
    errors.push("implementations/rust/pkgshift-cli/package.json: invalid private Changesets proxy");
  }
  if (coreProxy.name !== "pkgshift-core" || coreProxy.private !== true) {
    errors.push("implementations/rust/pkgshift-core/package.json: invalid private Changesets proxy");
  }

  const changesetsConfig = await Bun.file(
    ".changeset/config.json",
  ).json() as ChangesetsConfig;
  const fixedGroup = changesetsConfig.fixed?.find((group) => group.includes("pkgshift"));
  const expectedFixedGroup = [
    "@bahadirarda/pkgshift-typescript",
    "pkgshift",
    "pkgshift-core",
  ];
  if (
    !fixedGroup
    || [...fixedGroup].sort().join("\n") !== expectedFixedGroup.join("\n")
  ) {
    errors.push(".changeset/config.json: implementations must remain one fixed release group");
  }
  if (
    changesetsConfig.privatePackages?.version !== true
    || changesetsConfig.privatePackages.tag !== false
  ) {
    errors.push(".changeset/config.json: private proxies must be versioned but never tagged");
  }

  const changelog = await Bun.file("CHANGELOG.md").text();
  const unreleasedBody = changelog.match(
    /^## \[Unreleased\]\n([\s\S]*?)(?=^## \[)/m,
  )?.[1]?.trim();
  if (unreleasedBody) {
    errors.push("CHANGELOG.md: release notes must be declared through pending Changesets");
  }
  const escapedVersion = version.replaceAll(".", "\\.");
  if (!new RegExp(`^## \\[${escapedVersion}\\] - \\d{4}-\\d{2}-\\d{2}$`, "m").test(changelog)) {
    errors.push(`CHANGELOG.md: missing dated ${version} release section`);
  }

  const releaseTag = Bun.env.RELEASE_TAG;
  if (releaseTag && releaseTag !== `v${version}`) {
    errors.push(`release tag ${releaseTag} does not match canonical version v${version}`);
  }

  const rootLicense = await Bun.file("LICENSE").text();
  for (const path of [
    "implementations/rust/pkgshift-cli/LICENSE",
    "implementations/rust/pkgshift-core/LICENSE",
  ]) {
    if (await Bun.file(path).text() !== rootLicense) {
      errors.push(`${path}: packaged license must match the repository license`);
    }
  }
}

async function validateWebsite(): Promise<void> {
  const requiredPaths = [
    "site/index.html",
    "site/404.html",
    "site/favicon.svg",
    "site/install.sh",
    "site/llms.txt",
    "site/robots.txt",
    "site/script.js",
    "site/site.webmanifest",
    "site/sitemap.xml",
    "site/styles.css",
  ];
  for (const path of requiredPaths) {
    if (!(await targetExists(path))) {
      errors.push(`${path}: required website file is missing`);
    }
  }
  if (errors.some((error) => error.includes("required website file is missing"))) {
    return;
  }

  const canonicalUrl = "https://bahadirarda.github.io/pkgshift/";
  const index = await Bun.file("site/index.html").text();
  const requiredMarkup = [
    ['<html lang="en">', "English document language"],
    ["<title>pkgshift — Transactional JavaScript migrations</title>", "descriptive title"],
    ['name="description"', "meta description"],
    [`rel="canonical" href="${canonicalUrl}"`, "canonical URL"],
    ['property="og:image"', "Open Graph image"],
    ['name="twitter:card" content="summary_large_image"', "large social card"],
    ['type="application/ld+json"', "structured software data"],
    ["<h1>", "top-level product heading"],
  ] as const;
  for (const [markup, description] of requiredMarkup) {
    if (!index.includes(markup)) {
      errors.push(`site/index.html: missing ${description}`);
    }
  }

  const structuredData = index.match(
    /<script type="application\/ld\+json">\s*([\s\S]*?)\s*<\/script>/,
  )?.[1];
  if (!structuredData) {
    errors.push("site/index.html: structured data block is empty");
  } else {
    try {
      const parsed = JSON.parse(structuredData) as { "@graph"?: unknown[] };
      if (!Array.isArray(parsed["@graph"]) || parsed["@graph"].length < 2) {
        errors.push("site/index.html: structured data must describe the website and software");
      }
    } catch (error) {
      const message = error instanceof Error ? error.message : "unknown JSON error";
      errors.push(`site/index.html: invalid structured data: ${message}`);
    }
  }

  for (const htmlPath of ["site/index.html", "site/404.html"]) {
    const html = await Bun.file(htmlPath).text();
    for (const match of html.matchAll(/(?:href|src)="([^"]+)"/g)) {
      const rawTarget = match[1]?.split("#")[0];
      if (
        !rawTarget
        || rawTarget.startsWith("https://")
        || rawTarget.startsWith("http://")
        || rawTarget.startsWith("mailto:")
        || rawTarget.startsWith("/")
      ) {
        continue;
      }
      const target = resolve(dirname(htmlPath), rawTarget);
      if (!(await targetExists(target))) {
        errors.push(`${htmlPath}: broken website asset ${rawTarget}`);
      }
    }
  }

  try {
    const manifest = await Bun.file("site/site.webmanifest").json() as {
      start_url?: string;
      scope?: string;
      icons?: unknown[];
    };
    if (
      manifest.start_url !== "/pkgshift/"
      || manifest.scope !== "/pkgshift/"
      || !Array.isArray(manifest.icons)
      || manifest.icons.length === 0
    ) {
      errors.push("site/site.webmanifest: invalid GitHub Pages application boundary");
    }
  } catch (error) {
    const message = error instanceof Error ? error.message : "unknown JSON error";
    errors.push(`site/site.webmanifest: ${message}`);
  }

  const robots = await Bun.file("site/robots.txt").text();
  if (!robots.includes(`${canonicalUrl}sitemap.xml`)) {
    errors.push("site/robots.txt: canonical sitemap URL is missing");
  }
  const sitemap = await Bun.file("site/sitemap.xml").text();
  if (!sitemap.includes(`<loc>${canonicalUrl}</loc>`)) {
    errors.push("site/sitemap.xml: canonical product URL is missing");
  }

  const agentIndex = await Bun.file("site/llms.txt").text();
  if (
    !agentIndex.startsWith("# pkgshift\n\n> ")
    || !agentIndex.includes("## Agent integration")
    || !agentIndex.includes("skills/pkgshift/SKILL.md")
  ) {
    errors.push("site/llms.txt: invalid agent discovery index");
  }

  const installerPath = "site/install.sh";
  const installer = await Bun.file(installerPath).text();
  const installerMode = (await stat(installerPath)).mode;
  if ((installerMode & 0o111) === 0) {
    errors.push(`${installerPath}: installer must be executable`);
  }
  for (const required of ["SHA256SUMS", "sha256sum", "shasum", "--proto '=https'", "--tlsv1.2", "PKGSHIFT_DATA_DIR", "skills/pkgshift"]) {
    if (!installer.includes(required)) {
      errors.push(`${installerPath}: missing installer safety contract ${required}`);
    }
  }
  if (/\b(?:sudo|eval)\b/.test(installer)) {
    errors.push(`${installerPath}: installer must not elevate privileges or evaluate downloaded code`);
  }
}

async function validateEnglishOnly(): Promise<void> {
  const roots = ["AGENTS.md", "CHANGELOG.md", "CONTRIBUTING.md", "LICENSE", "README.md"];
  const patterns = [
    ".changeset/*.{json,md}",
    "implementations/rust/**/*.rs",
    "implementations/rust/**/*.md",
    "implementations/rust/**/LICENSE",
    "docs/**/*.md",
    "implementations/typescript/scripts/**/*.ts",
    "skills/**/*.{md,yaml,yml}",
    "implementations/typescript/src/**/*.ts",
    "implementations/typescript/tests/**/*.ts",
    ".github/workflows/*.{yaml,yml}",
    "site/**/*.{css,html,js,json,sh,svg,txt,xml}",
    "*.json",
    "*.toml",
  ];
  for (const pattern of patterns) {
    roots.push(...await filesMatching(pattern));
  }
  for (const path of [...new Set(roots)]) {
    const content = await Bun.file(path).text();
    if (/[\u00e7\u011f\u0131\u00f6\u015f\u00fc\u00c7\u011e\u0130\u00d6\u015e\u00dc]/.test(content)) {
      errors.push(`${path}: Turkish-specific characters violate the English-only rule`);
    }
  }
}

async function validateRustModuleBoundaries(): Promise<number> {
  const maximumProductionLines = 1000;
  const paths = await filesMatching("implementations/rust/**/src/**/*.rs");
  const productionPaths = paths.filter((path) =>
    !/(?:^|\/)tests(?:\/|\.rs$)/.test(path)
  );
  for (const path of productionPaths) {
    const lineCount = (await Bun.file(path).text()).split("\n").length;
    if (lineCount > maximumProductionLines) {
      errors.push(
        `${path}: production Rust modules must remain at or below ${maximumProductionLines} lines; found ${lineCount}`,
      );
    }
  }
  return productionPaths.length;
}

const conceptCount = await validateOkf();
await validateSkill();
const changesetCount = await validateChangesets();
await validateReleaseMetadata();
await validateWebsite();
await validateLinks("README.md", await Bun.file("README.md").text(), null);
await validateEnglishOnly();
const rustModuleCount = await validateRustModuleBoundaries();

if (errors.length > 0) {
  console.error(errors.join("\n"));
  process.exit(1);
}

console.log(`Validated ${conceptCount} OKF Markdown files, ${changesetCount} pending Changeset(s), ${rustModuleCount} bounded production Rust modules, release metadata, the product website, the portable Agent Skill, internal links, and English-only content.`);
