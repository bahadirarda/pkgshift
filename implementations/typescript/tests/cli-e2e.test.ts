import { afterEach, describe, expect, test } from "bun:test";
import { join } from "node:path";
import { runCli } from "../src/cli/main.ts";
import { createProject, removeTemporaryProjects } from "./helpers/project.ts";

afterEach(removeTemporaryProjects);

async function jsonCommand(argv: string[], root: string): Promise<{
  exitCode: number;
  result: Record<string, any>;
}> {
  let stdout = "";
  const flags = ["--json", "--no-color", "--non-interactive"]
    .filter((flag) => !argv.includes(flag));
  const exitCode = await runCli(
    [...argv, ...flags],
    {
      stdout: { write: (content) => { stdout += content; } },
      stderr: { write: () => undefined },
    },
    root,
  );
  return { exitCode, result: JSON.parse(stdout) as Record<string, any> };
}

async function createMigrationFixture(): Promise<string> {
  return createProject({
    "package.json": `${JSON.stringify({
      name: "fixture",
      version: "1.0.0",
      packageManager: "npm@11.0.0",
      dependencies: { local: "file:./vendor/local" },
    }, null, 2)}\n`,
    "vendor/local/package.json": `${JSON.stringify({
      name: "local",
      version: "1.0.0",
    }, null, 2)}\n`,
    "package-lock.json": `${JSON.stringify({
      name: "fixture",
      version: "1.0.0",
      lockfileVersion: 3,
      requires: true,
      packages: { "": { name: "fixture", version: "1.0.0" } },
    }, null, 2)}\n`,
  });
}

async function createPnpmWorkspaceFixture(): Promise<string> {
  return createProject({
    "package.json": `${JSON.stringify({
      name: "pnpm-workspace-fixture",
      version: "1.0.0",
      private: true,
      packageManager: "pnpm@11.21.0",
      devDependencies: { "local-tool": "file:./vendor/local-tool" },
    }, null, 2)}\n`,
    "pnpm-workspace.yaml": [
      "packages:",
      "  - apps/*",
      "  - packages/*",
      "nodeLinker: isolated",
      "catalog:",
      "  fixture-runtime: ^1.0.0",
      "catalogs:",
      "  testing:",
      "    fixture-test: ^2.0.0",
      "onlyBuiltDependencies:",
      "  - local-tool",
      "",
    ].join("\n"),
    "pnpm-lock.yaml": "lockfileVersion: '9.0'\nimporters:\n  .: {}\npackages: {}\nsnapshots: {}\n",
    "apps/web/package.json": `${JSON.stringify({
      name: "@fixture/web",
      version: "1.0.0",
      private: true,
      dependencies: { "@fixture/shared": "workspace:^" },
    }, null, 2)}\n`,
    "packages/shared/package.json": `${JSON.stringify({
      name: "@fixture/shared",
      version: "2.4.0",
      dependencies: { "local-tool": "file:../../vendor/local-tool" },
    }, null, 2)}\n`,
    "packages/worker/package.json": `${JSON.stringify({
      name: "@fixture/worker",
      version: "1.0.0",
      private: true,
      dependencies: { "@fixture/shared": "workspace:*" },
    }, null, 2)}\n`,
    "vendor/local-tool/package.json": `${JSON.stringify({
      name: "local-tool",
      version: "1.0.0",
    }, null, 2)}\n`,
    ".github/workflows/ci.yml": "steps:\n  - run: pnpm install\n  - run: pnpm run test\n",
    "Dockerfile": "FROM oven/bun:1\nRUN pnpm install\n",
    "README.md": "Install with `pnpm install` and run checks with `pnpm run test`.\n",
  });
}

async function createPnpmExcludedWorkspaceFixture(): Promise<string> {
  return createProject({
    "package.json": `${JSON.stringify({
      name: "pnpm-exclusion-fixture",
      version: "1.0.0",
      private: true,
      packageManager: "pnpm@11.21.0",
      dependencies: { "local-plugin": "file:./vendor/local-plugin" },
    }, null, 2)}\n`,
    "pnpm-workspace.yaml": [
      "packages:",
      "  - services/*",
      "  - packages/*",
      "  - '!packages/legacy'",
      "",
    ].join("\n"),
    "pnpm-lock.yaml": "lockfileVersion: '9.0'\nimporters:\n  .: {}\npackages: {}\nsnapshots: {}\n",
    ".npmrc": "registry=https://registry.npmjs.org\n",
    "services/api/package.json": `${JSON.stringify({
      name: "@fixture/api",
      version: "1.0.0",
      private: true,
      dependencies: {
        "@fixture/domain": "workspace:*",
        "local-plugin": "file:../../vendor/local-plugin",
      },
    }, null, 2)}\n`,
    "packages/domain/package.json": `${JSON.stringify({
      name: "@fixture/domain",
      version: "3.1.0",
    }, null, 2)}\n`,
    "packages/legacy/package.json": `${JSON.stringify({
      name: "@fixture/legacy",
      version: "0.1.0",
    }, null, 2)}\n`,
    "vendor/local-plugin/package.json": `${JSON.stringify({
      name: "local-plugin",
      version: "1.0.0",
    }, null, 2)}\n`,
    ".gitlab-ci.yml": "install:\n  script: pnpm install\n",
    "Containerfile": "FROM oven/bun:1\nRUN pnpm install\n",
  });
}

async function runApprovedGuidedMigration(root: string): Promise<{
  preview: Record<string, any>;
  completed: Record<string, any>;
}> {
  const preview = await jsonCommand(["to", "bun"], root);
  expect(preview.exitCode).toBe(7);
  expect(preview.result.status).toBe("planned");
  expect(preview.result.summary.source).toBe("pnpm");
  expect(preview.result.summary.repositoryChanged).toBeFalse();
  const nextAction = preview.result.nextActions[0] as { argv: string[] };
  const completed = await jsonCommand(nextAction.argv.slice(1), root);
  expect(completed.exitCode, JSON.stringify(completed.result, null, 2)).toBe(0);
  expect(completed.result.status).toBe("completed");
  expect(completed.result.summary.runStatus).toBe("succeeded");
  expect(completed.result.summary.failed).toBe(0);
  return { preview: preview.result, completed: completed.result };
}

async function rollBackGuidedMigration(
  root: string,
  runId: string,
): Promise<Record<string, any>> {
  const stateDirectory = join(root, ".pkgshift", "state");
  const rolledBack = await jsonCommand([
    "rollback", runId,
    "--state-dir", stateDirectory,
    "--approve", runId,
  ], root);
  expect(rolledBack.exitCode).toBe(0);
  expect(rolledBack.result.status).toBe("rolled-back");
  return rolledBack.result;
}

describe("CLI transaction", () => {
  test("completes the plan, apply, verify, and rollback workflow", async () => {
    const root = await createMigrationFixture();
    const stateDirectory = join(root, ".pkgshift", "state");
    const planned = await jsonCommand([
      "pm", "to", "bun", "--state-dir", stateDirectory,
    ], root);
    expect(planned.exitCode).toBe(0);
    const planId = planned.result.planId as string;

    const applied = await jsonCommand([
      "apply", planId,
      "--state-dir", stateDirectory,
      "--approve", planId,
    ], root);
    expect(applied.exitCode).toBe(0);
    expect(applied.result.status).toBe("completed");
    const runId = applied.result.runId as string;
    expect(await Bun.file(join(root, "bun.lock")).exists()).toBeTrue();

    const verified = await jsonCommand([
      "verify", runId, "--state-dir", stateDirectory,
    ], root);
    expect(verified.exitCode).toBe(0);
    expect(verified.result.summary.runStatus).toBe("succeeded");

    const rolledBack = await jsonCommand([
      "rollback", runId,
      "--state-dir", stateDirectory,
      "--approve", runId,
    ], root);
    expect(rolledBack.exitCode).toBe(0);
    expect(rolledBack.result.status).toBe("rolled-back");
    expect((await Bun.file(join(root, "package.json")).json()).packageManager).toBe("npm@11.0.0");
    expect(await Bun.file(join(root, "package-lock.json")).exists()).toBeTrue();
    expect(await Bun.file(join(root, "bun.lock")).exists()).toBeFalse();
  });

  test("completes a guided migration through exact non-interactive approval", async () => {
    const root = await createMigrationFixture();

    const preview = await jsonCommand(["to", "bun"], root);
    expect(preview.exitCode).toBe(7);
    expect(preview.result.status).toBe("planned");
    expect(preview.result.summary.repositoryChanged).toBeFalse();
    const nextAction = preview.result.nextActions[0] as { argv: string[] };

    const completed = await jsonCommand(nextAction.argv.slice(1), root);
    expect(completed.exitCode).toBe(0);
    expect(completed.result.command).toBe("to bun");
    expect(completed.result.status).toBe("completed");
    expect(completed.result.planId).toBe(preview.result.planId);
    expect(completed.result.summary.runStatus).toBe("succeeded");
    expect(await Bun.file(join(root, "bun.lock")).exists()).toBeTrue();
    expect((await Bun.file(join(root, "package.json")).json()).packageManager).toStartWith("bun@");

    const runId = completed.result.runId as string;
    const stateDirectory = join(root, ".pkgshift", "state");
    const rolledBack = await jsonCommand([
      "rollback", runId,
      "--state-dir", stateDirectory,
      "--approve", runId,
    ], root);
    expect(rolledBack.exitCode).toBe(0);
    expect((await Bun.file(join(root, "package.json")).json()).packageManager).toBe("npm@11.0.0");
  });

  test("completes an approved guided migration in one interactive command", async () => {
    const root = await createMigrationFixture();
    let stdout = "";
    let approvalRequests = 0;

    const exitCode = await runCli(
      ["to", "bun"],
      {
        stdout: { write: (content) => { stdout += content; } },
        stderr: { write: () => undefined },
        requestApproval: async (request) => {
          approvalRequests += 1;
          expect(request.source).toBe("npm");
          expect(request.target).toBe("bun");
          expect(request.planId).toStartWith("plan_");
          return true;
        },
      },
      root,
    );

    expect(exitCode).toBe(0);
    expect(approvalRequests).toBe(1);
    expect(stdout).toContain("pkgshift: to bun");
    expect(stdout).toContain("Status: completed");
    expect(await Bun.file(join(root, "bun.lock")).exists()).toBeTrue();
    expect((await Bun.file(join(root, "package.json")).json()).packageManager).toStartWith("bun@");
  });

  test("migrates a pnpm workspace with catalogs, isolated linking, and integrations to Bun", async () => {
    const root = await createPnpmWorkspaceFixture();
    const { preview, completed } = await runApprovedGuidedMigration(root);

    expect(preview.summary.files).toBe(7);
    expect(completed.planId).toBe(preview.planId);
    expect(completed.summary.skipped).toBe(1);
    expect(await Bun.file(join(root, "bun.lock")).exists()).toBeTrue();
    expect(await Bun.file(join(root, "pnpm-lock.yaml")).exists()).toBeFalse();
    expect(await Bun.file(join(root, "pnpm-workspace.yaml")).exists()).toBeFalse();
    const manifest = await Bun.file(join(root, "package.json")).json();
    expect(manifest.packageManager).toBe("bun@1.3.14");
    expect(manifest.workspaces).toEqual({
      packages: ["apps/*", "packages/*"],
      catalog: { "fixture-runtime": "^1.0.0" },
      catalogs: { testing: { "fixture-test": "^2.0.0" } },
    });
    expect(manifest.trustedDependencies).toEqual(["local-tool"]);
    expect(await Bun.file(join(root, "bunfig.toml")).text()).toContain('linker = "isolated"');
    expect(await Bun.file(join(root, ".github/workflows/ci.yml")).text()).toContain("bun install");
    expect(await Bun.file(join(root, "Dockerfile")).text()).toContain("bun install");
    expect(await Bun.file(join(root, "README.md")).text()).toContain("bun run test");

    await rollBackGuidedMigration(root, completed.runId as string);
    expect((await Bun.file(join(root, "package.json")).json()).packageManager).toBe("pnpm@11.21.0");
    expect(await Bun.file(join(root, "pnpm-workspace.yaml")).exists()).toBeTrue();
    expect(await Bun.file(join(root, "pnpm-lock.yaml")).exists()).toBeTrue();
    expect(await Bun.file(join(root, "bun.lock")).exists()).toBeFalse();
  });

  test("migrates a pnpm workspace with exclusions and local dependencies to Bun", async () => {
    const root = await createPnpmExcludedWorkspaceFixture();
    const { preview, completed } = await runApprovedGuidedMigration(root);

    expect(preview.summary.files).toBe(5);
    expect(completed.planId).toBe(preview.planId);
    expect(await Bun.file(join(root, "bun.lock")).exists()).toBeTrue();
    expect(await Bun.file(join(root, "pnpm-lock.yaml")).exists()).toBeFalse();
    const manifest = await Bun.file(join(root, "package.json")).json();
    expect(manifest.packageManager).toBe("bun@1.3.14");
  expect(manifest.workspaces).toEqual([
    "services/*",
    "packages/*",
    "!packages/legacy",
  ]);
    expect(await Bun.file(join(root, ".npmrc")).exists()).toBeTrue();
    expect(await Bun.file(join(root, ".gitlab-ci.yml")).text()).toContain("bun install");
    expect(await Bun.file(join(root, "Containerfile")).text()).toContain("bun install");

    await rollBackGuidedMigration(root, completed.runId as string);
    expect((await Bun.file(join(root, "package.json")).json()).packageManager).toBe("pnpm@11.21.0");
    expect(await Bun.file(join(root, "pnpm-lock.yaml")).exists()).toBeTrue();
    expect(await Bun.file(join(root, "bun.lock")).exists()).toBeFalse();
  });
});
