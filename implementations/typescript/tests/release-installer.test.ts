import { afterEach, describe, expect, test } from "bun:test";
import {
  chmod,
  mkdir,
  mkdtemp,
  rm,
  writeFile,
} from "node:fs/promises";
import { tmpdir } from "node:os";
import { basename, join, resolve } from "node:path";

const temporaryDirectories: string[] = [];
const repositoryRoot = resolve(import.meta.dir, "../../..");

afterEach(async () => {
  await Promise.all(
    temporaryDirectories.splice(0).map((path) => rm(path, { recursive: true, force: true })),
  );
});

async function temporaryDirectory(): Promise<string> {
  const path = await mkdtemp(join(tmpdir(), "pkgshift-release-installer-"));
  temporaryDirectories.push(path);
  return path;
}

async function run(command: string[], options: { cwd?: string; env?: Record<string, string> } = {}) {
  const process = Bun.spawn(command, {
    ...(options.cwd ? { cwd: options.cwd } : {}),
    env: { ...Bun.env, ...options.env },
    stdout: "pipe",
    stderr: "pipe",
  });
  const [exitCode, stdout, stderr] = await Promise.all([
    process.exited,
    new Response(process.stdout).text(),
    new Response(process.stderr).text(),
  ]);
  return { exitCode, stdout, stderr };
}

describe("verified release installer", () => {
  test("installs and atomically refreshes the portable Agent Skill data", async () => {
    const root = await temporaryDirectory();
    const version = "v0.20260817.5";
    const target = "x86_64-unknown-linux-gnu";
    const stagingName = `pkgshift-${version}-${target}`;
    const releaseDirectory = join(root, "release");
    const staging = join(root, stagingName);
    const mockBin = join(root, "mock-bin");
    const installDirectory = join(root, "install-bin");
    const dataDirectory = join(root, "data");
    await mkdir(join(staging, "skills/pkgshift"), { recursive: true });
    await mkdir(releaseDirectory, { recursive: true });
    await mkdir(mockBin, { recursive: true });
    await writeFile(
      join(staging, "pkgshift"),
      "#!/bin/sh\ncase \"${1:-}\" in --version|skill) exit 0 ;; *) exit 2 ;; esac\n",
    );
    await chmod(join(staging, "pkgshift"), 0o755);
    await writeFile(
      join(staging, "skills/pkgshift/SKILL.md"),
      "---\nname: pkgshift\ndescription: Safe migrations.\n---\n",
    );
    const archive = join(releaseDirectory, `${stagingName}.tar.gz`);
    const archived = await run(["tar", "-czf", archive, "-C", root, stagingName]);
    expect(archived.exitCode).toBe(0);
    const hash = new Bun.CryptoHasher("sha256")
      .update(await Bun.file(archive).arrayBuffer())
      .digest("hex");
    await writeFile(
      join(releaseDirectory, "SHA256SUMS"),
      `${hash}  ${basename(archive)}\n`,
    );
    await writeFile(
      join(mockBin, "curl"),
      "#!/bin/sh\noutput=\"\"\nurl=\"\"\nwhile [ \"$#\" -gt 0 ]; do\n  case \"$1\" in\n    --output) output=\"$2\"; shift 2 ;;\n    --*) shift ;;\n    *) url=\"$1\"; shift ;;\n  esac\ndone\ncp \"$MOCK_RELEASE_DIR/${url##*/}\" \"$output\"\n",
    );
    await writeFile(
      join(mockBin, "uname"),
      "#!/bin/sh\ncase \"$1\" in -s) printf '%s\\n' Linux ;; -m) printf '%s\\n' x86_64 ;; *) exit 2 ;; esac\n",
    );
    await chmod(join(mockBin, "curl"), 0o755);
    await chmod(join(mockBin, "uname"), 0o755);

    const environment = {
      MOCK_RELEASE_DIR: releaseDirectory,
      PATH: `${mockBin}:${Bun.env.PATH ?? ""}`,
      PKGSHIFT_DATA_DIR: dataDirectory,
    };
    const command = [
      "sh",
      join(repositoryRoot, "site/install.sh"),
      "--version",
      version,
      "--to",
      installDirectory,
    ];
    const first = await run(command, { cwd: repositoryRoot, env: environment });
    expect(first.exitCode).toBe(0);
    expect(first.stderr).toBe("");
    expect(await Bun.file(join(installDirectory, "pkgshift")).exists()).toBeTrue();
    expect(await Bun.file(join(dataDirectory, "skills/pkgshift/SKILL.md")).exists()).toBeTrue();

    await writeFile(join(dataDirectory, "skills/pkgshift/stale.md"), "stale\n");
    const refreshed = await run(command, { cwd: repositoryRoot, env: environment });
    expect(refreshed.exitCode).toBe(0);
    expect(await Bun.file(join(dataDirectory, "skills/pkgshift/stale.md")).exists()).toBeFalse();
    expect(refreshed.stdout).toContain("installed portable Agent Skill data");
  });
});
