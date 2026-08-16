import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

const repositoryRoot = resolve(import.meta.dir, "../../..");
process.chdir(repositoryRoot);

const releasePackages = [
  "pkgshift",
  "pkgshift-core",
  "@bahadirarda/pkgshift-typescript",
] as const;

interface ReleasePlan {
  changesets: Array<{
    id: string;
    summary: string;
  }>;
  releases: Array<{
    name: string;
    oldVersion: string;
    newVersion: string;
    type: "major" | "minor" | "patch";
  }>;
}

interface PackageMetadata {
  name: string;
  version: string;
  [key: string]: unknown;
}

async function run(command: string[]): Promise<void> {
  const process = Bun.spawn(command, {
    cwd: repositoryRoot,
    env: Bun.env,
    stderr: "inherit",
    stdout: "inherit",
  });
  const exitCode = await process.exited;
  if (exitCode !== 0) {
    throw new Error(`${command.join(" ")} exited with code ${exitCode}`);
  }
}

async function output(command: string[]): Promise<string> {
  const process = Bun.spawn(command, {
    cwd: repositoryRoot,
    env: Bun.env,
    stderr: "inherit",
    stdout: "pipe",
  });
  const text = await new Response(process.stdout).text();
  const exitCode = await process.exited;
  if (exitCode !== 0) {
    throw new Error(`${command.join(" ")} exited with code ${exitCode}`);
  }
  return text.trim();
}

async function readJson(path: string): Promise<PackageMetadata> {
  return JSON.parse(await readFile(path, "utf8")) as PackageMetadata;
}

async function writeJson(path: string, value: PackageMetadata): Promise<void> {
  await writeFile(path, `${JSON.stringify(value, null, 2)}\n`);
}

function replaceExactly(
  path: string,
  content: string,
  pattern: RegExp,
  replacement: string,
): string {
  const matches = content.match(new RegExp(pattern.source, `${pattern.flags.replace("g", "")}g`));
  if (matches?.length !== 1) {
    throw new Error(`${path}: expected exactly one version field match`);
  }
  return content.replace(pattern, replacement);
}

function markdownBullet(summary: string): string {
  return `- ${summary.trim().replaceAll("\n", "\n  ")}`;
}

function updateChangelog(
  content: string,
  previousVersion: string,
  nextVersion: string,
  releaseDate: string,
  summaries: string[],
): string {
  const marker = "## [Unreleased]\n\n";
  if (!content.includes(marker)) {
    throw new Error("CHANGELOG.md: missing Unreleased section");
  }
  if (content.includes(`## [${nextVersion}]`)) {
    throw new Error(`CHANGELOG.md: release ${nextVersion} already exists`);
  }
  const section = [
    `## [${nextVersion}] - ${releaseDate}`,
    "",
    "### Changed",
    "",
    ...summaries.map(markdownBullet),
  ].join("\n");
  let updated = content.replace(marker, `${marker}${section}\n\n`);

  const unreleasedLink = /^\[Unreleased\]: .+$/m;
  if (!unreleasedLink.test(updated)) {
    throw new Error("CHANGELOG.md: missing Unreleased comparison link");
  }
  updated = updated.replace(
    unreleasedLink,
    `[Unreleased]: https://github.com/bahadirarda/pkgshift/compare/v${nextVersion}...HEAD\n[${nextVersion}]: https://github.com/bahadirarda/pkgshift/compare/v${previousVersion}...v${nextVersion}`,
  );
  return updated;
}

const rootCargoPath = "Cargo.toml";
const cliCargoPath = "implementations/rust/pkgshift-cli/Cargo.toml";
const rootPackagePath = "package.json";
const proxyPaths = [
  "implementations/rust/pkgshift-cli/package.json",
  "implementations/rust/pkgshift-core/package.json",
  "implementations/typescript/package.json",
] as const;

const rootCargoBefore = await readFile(rootCargoPath, "utf8");
const cargoMetadata = Bun.TOML.parse(rootCargoBefore) as {
  workspace?: { package?: { version?: string } };
};
const previousVersion = cargoMetadata.workspace?.package?.version;
if (!previousVersion) {
  throw new Error("Cargo.toml: missing canonical workspace version");
}

const planDirectory = await mkdtemp(join(tmpdir(), "pkgshift-release-plan-"));
const planPath = join(planDirectory, "plan.json");

try {
  await run(["bun", "x", "changeset", "status", "--output", planPath]);
  const plan = JSON.parse(await readFile(planPath, "utf8")) as ReleasePlan;
  if (plan.changesets.length === 0 || plan.releases.length === 0) {
    throw new Error("No pending Changesets are available for a version release");
  }

  const plannedVersions = new Map(
    plan.releases.map((release) => [release.name, release.newVersion]),
  );
  const nextVersion = plannedVersions.get("pkgshift");
  if (!nextVersion) {
    throw new Error("Release plan does not contain the pkgshift product package");
  }
  for (const packageName of releasePackages) {
    if (plannedVersions.get(packageName) !== nextVersion) {
      throw new Error(`Fixed release group does not agree on ${nextVersion}`);
    }
  }

  const releaseDate = Bun.env.RELEASE_DATE
    ?? await output(["git", "show", "-s", "--format=%cs", "HEAD"]);
  if (!/^\d{4}-\d{2}-\d{2}$/.test(releaseDate)) {
    throw new Error(`Invalid release date ${releaseDate}`);
  }
  const summaries = [...plan.changesets]
    .sort((left, right) => left.id.localeCompare(right.id))
    .map((changeset) => changeset.summary.trim());

  await run(["bun", "x", "changeset", "version"]);

  for (const path of proxyPaths) {
    const metadata = await readJson(path);
    if (metadata.version !== nextVersion) {
      throw new Error(`${path}: Changesets produced ${metadata.version}, expected ${nextVersion}`);
    }
  }

  const rootPackage = await readJson(rootPackagePath);
  rootPackage.version = nextVersion;
  await writeJson(rootPackagePath, rootPackage);

  const rootCargo = replaceExactly(
    rootCargoPath,
    rootCargoBefore,
    /(\[workspace\.package\][\s\S]*?\nversion = ")[^"]+("\n)/,
    `$1${nextVersion}$2`,
  );
  await writeFile(rootCargoPath, rootCargo);

  const cliCargoBefore = await readFile(cliCargoPath, "utf8");
  const cliCargo = replaceExactly(
    cliCargoPath,
    cliCargoBefore,
    /(pkgshift-core = \{ version = ")[^"]+("),/,
    `$1${nextVersion}$2,`,
  );
  await writeFile(cliCargoPath, cliCargo);

  const changelogPath = "CHANGELOG.md";
  const changelog = updateChangelog(
    await readFile(changelogPath, "utf8"),
    previousVersion,
    nextVersion,
    releaseDate,
    summaries,
  );
  await writeFile(changelogPath, changelog);

  await run(["bun", "install", "--lockfile-only"]);
  await run(["cargo", "check", "--workspace"]);
  await run(["bun", "run", "version:check"]);

  console.log(`Prepared pkgshift ${nextVersion} from ${plan.changesets.length} Changeset(s).`);
} finally {
  await rm(planDirectory, { force: true, recursive: true });
}
