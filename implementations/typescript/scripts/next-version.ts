import { readFile } from "node:fs/promises";
import { resolve } from "node:path";
import { nextCalendarVersion } from "./release/calendar-version.ts";

const repositoryRoot = resolve(import.meta.dir, "../../..");
process.chdir(repositoryRoot);

async function gitOutput(args: string[]): Promise<string> {
  const process = Bun.spawn(["git", ...args], {
    cwd: repositoryRoot,
    stderr: "inherit",
    stdout: "pipe",
  });
  const output = await new Response(process.stdout).text();
  const exitCode = await process.exited;
  if (exitCode !== 0) throw new Error(`git ${args.join(" ")} exited with code ${exitCode}`);
  return output.trim();
}

const cargo = Bun.TOML.parse(await readFile("Cargo.toml", "utf8")) as {
  workspace?: { package?: { version?: string } };
};
const previousVersion = cargo.workspace?.package?.version;
if (!previousVersion) throw new Error("Cargo.toml does not contain a canonical version");

const releaseDate = Bun.env.RELEASE_DATE
  ?? await gitOutput(["show", "-s", "--format=%cs", "HEAD"]);
console.log(nextCalendarVersion(previousVersion, releaseDate));
