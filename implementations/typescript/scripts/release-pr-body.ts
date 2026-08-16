import { writeFile } from "node:fs/promises";
import { resolve } from "node:path";
import { isCalendarVersion } from "./release/calendar-version.ts";

const repositoryRoot = resolve(import.meta.dir, "../../..");
process.chdir(repositoryRoot);

interface CargoMetadata {
  workspace?: { package?: { version?: string } };
}

function option(name: string): string {
  const index = Bun.argv.indexOf(name);
  const value = index >= 0 ? Bun.argv[index + 1] : undefined;
  if (!value) throw new Error(`${name} requires a value`);
  return value;
}

async function gitFile(ref: string, path: string): Promise<string> {
  const process = Bun.spawn(["git", "show", `${ref}:${path}`], {
    stderr: "inherit",
    stdout: "pipe",
  });
  const content = await new Response(process.stdout).text();
  const exitCode = await process.exited;
  if (exitCode !== 0) throw new Error(`Cannot read ${path} from ${ref}`);
  return content;
}

const ref = option("--ref");
const output = option("--output");
if (!/^[A-Za-z0-9._/-]+$/.test(ref)) {
  throw new Error(`Invalid Git reference ${ref}`);
}

const cargo = Bun.TOML.parse(await gitFile(ref, "Cargo.toml")) as CargoMetadata;
const version = cargo.workspace?.package?.version;
if (!version) throw new Error(`${ref} does not contain a canonical package version`);
if (!isCalendarVersion(version)) {
  throw new Error(`${ref} does not contain a pkgshift calendar version: ${version}`);
}

const changelog = await gitFile(ref, "CHANGELOG.md");
const escapedVersion = version.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
const notes = changelog.match(
  new RegExp(`^## \\[${escapedVersion}\\] - \\d{4}-\\d{2}-\\d{2}\\n([\\s\\S]*?)(?=^## \\[)`, "m"),
)?.[1]?.trim();
if (!notes) throw new Error(`CHANGELOG.md does not contain release notes for ${version}`);

const body = `# pkgshift ${version}

This automated release pull request consumes reviewed Changesets and synchronizes the fixed package group around the repository-calculated calendar version.

## Release notes

${notes}

## Release identity

- Canonical version: \`${version}\`
- Annotated tag after merge: \`v${version}\`
- Fixed group: \`pkgshift\`, \`pkgshift-core\`, and \`@bahadirarda/pkgshift-typescript\`
- Required gates: TypeScript, Rust, package, documentation, and release metadata validation
`;

await writeFile(output, body);
console.log(version);
