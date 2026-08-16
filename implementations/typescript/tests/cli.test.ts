import { afterEach, describe, expect, test } from "bun:test";
import { runCli } from "../src/cli/main.ts";
import { parseArguments } from "../src/cli/parse-arguments.ts";
import { createProject, removeTemporaryProjects } from "./helpers/project.ts";

afterEach(removeTemporaryProjects);

describe("CLI", () => {
  test("supports the conventional help flag", async () => {
    let stdout = "";

    const exitCode = await runCli(
      ["--help", "--json"],
      {
        stdout: { write: (content) => { stdout += content; } },
        stderr: { write: () => undefined },
      },
      "/fixture",
    );
    const result = JSON.parse(stdout) as { command: string; status: string };

    expect(exitCode).toBe(0);
    expect(result.command).toBe("help");
    expect(result.status).toBe("completed");
  });

  test("maps the pm shortcut to the canonical plan command", () => {
    const parsed = parseArguments(
      ["pm", "to", "bun", "--json", "--non-interactive"],
      "/fixture",
    );

    expect(parsed.positional).toEqual(["plan", "package-manager"]);
    expect(parsed.options.target).toBe("bun");
    expect(parsed.options.json).toBeTrue();
    expect(parsed.options.nonInteractive).toBeTrue();
  });

  test("parses the guided migration command from the current directory", () => {
    const parsed = parseArguments(
      ["to", "pnpm", "--dry-run"],
      "/fixture",
    );

    expect(parsed.positional).toEqual(["to"]);
    expect(parsed.options.target).toBe("pnpm");
    expect(parsed.options.cwd).toBe("/fixture");
    expect(parsed.options.dryRun).toBeTrue();
  });

  test("plans guided non-interactive migrations without changing the repository", async () => {
    const root = await createProject({
      "package.json": JSON.stringify({
        name: "fixture",
        packageManager: "npm@11.0.0",
      }),
      "package-lock.json": "{}",
    });
    let stdout = "";

    const exitCode = await runCli(
      ["to", "bun", "--json", "--non-interactive"],
      {
        stdout: { write: (content) => { stdout += content; } },
        stderr: { write: () => undefined },
      },
      root,
    );
    const result = JSON.parse(stdout) as {
      command: string;
      status: string;
      planId: string;
      summary: { repositoryChanged: boolean };
      nextActions: Array<{
        argv: string[];
        requiresApproval: boolean;
        sideEffect: string;
      }>;
    };

    expect(exitCode).toBe(7);
    expect(result.command).toBe("to bun");
    expect(result.status).toBe("planned");
    expect(result.summary.repositoryChanged).toBeFalse();
    expect(result.nextActions).toEqual([{
      argv: [
        "pkgshift", "to", "bun",
        "--approve", result.planId,
        "--json", "--no-color", "--non-interactive",
      ],
      requiresApproval: true,
      sideEffect: "repository-write",
    }]);
    expect((await Bun.file(`${root}/package.json`).json()).packageManager).toBe("npm@11.0.0");
    expect(await Bun.file(`${root}/.pkgshift/state/plans/${result.planId}.json`).exists()).toBeFalse();
  });

  test("leaves an interactively declined guided migration read-only", async () => {
    const root = await createProject({
      "package.json": JSON.stringify({
        name: "fixture",
        packageManager: "npm@11.0.0",
      }),
      "package-lock.json": "{}",
    });
    let stdout = "";
    let requestedPlan = "";

    const exitCode = await runCli(
      ["to", "bun"],
      {
        stdout: { write: (content) => { stdout += content; } },
        stderr: { write: () => undefined },
        requestApproval: async (request) => {
          requestedPlan = request.planId;
          return false;
        },
      },
      root,
    );

    expect(exitCode).toBe(0);
    expect(requestedPlan).toStartWith("plan_");
    expect(stdout).toContain("Status: planned");
    expect(stdout).toContain("approval: declined");
    expect((await Bun.file(`${root}/package.json`).json()).packageManager).toBe("npm@11.0.0");
    expect(await Bun.file(`${root}/.pkgshift/state/plans/${requestedPlan}.json`).exists()).toBeFalse();
  });

  test("resolves state storage relative to the selected repository root", () => {
    const parsed = parseArguments([
      "plan", "package-manager", "--to", "bun",
      "--state-dir", ".pkgshift/state",
      "--cwd", "/fixture",
    ], "/other");

    expect(parsed.options.stateDirectory).toBe("/fixture/.pkgshift/state");
  });

  test("returns a structured plan for the pm shortcut", async () => {
    const root = await createProject({
      "package.json": JSON.stringify({
        name: "fixture",
        packageManager: "npm@11.0.0",
      }),
      "package-lock.json": "{}",
    });
    let stdout = "";
    let stderr = "";

    const exitCode = await runCli(
      ["pm", "to", "bun", "--json", "--non-interactive"],
      {
        stdout: { write: (content) => { stdout += content; } },
        stderr: { write: (content) => { stderr += content; } },
      },
      root,
    );
    const result = JSON.parse(stdout) as {
      command: string;
      status: string;
      planId: string | null;
      summary: { executionAvailable: boolean };
    };

    expect(exitCode).toBe(0);
    expect(stderr).toBe("");
    expect(result.command).toBe("plan package-manager");
    expect(result.status).toBe("planned");
    expect(result.planId).toStartWith("plan_");
    expect(result.summary.executionAvailable).toBeTrue();
  });

  test("persists a plan only when an explicit state directory is provided", async () => {
    const root = await createProject({
      "package.json": JSON.stringify({
        name: "fixture",
        packageManager: "npm@11.0.0",
      }),
      "package-lock.json": "{}",
    });
    const stateDirectory = `${root}/state`;
    let stdout = "";

    const exitCode = await runCli(
      [
        "plan",
        "package-manager",
        "--to",
        "bun",
        "--state-dir",
        stateDirectory,
        "--json",
      ],
      {
        stdout: { write: (content) => { stdout += content; } },
        stderr: { write: () => undefined },
      },
      root,
    );
    const result = JSON.parse(stdout) as {
      summary: { artifactStored: boolean };
      artifacts: Array<{
        type: string;
        content: { relativePath?: string };
      }>;
    };
    const reference = result.artifacts.find((artifact) =>
      artifact.type === "stored-artifact-reference"
    );

    expect(exitCode).toBe(0);
    expect(result.summary.artifactStored).toBeTrue();
    expect(reference?.content.relativePath).toEndWith(".json");
    expect(await Bun.file(`${stateDirectory}/${reference?.content.relativePath}`).exists()).toBeTrue();
  });

  test("requires exact approval before apply", async () => {
    const root = await createProject({
      "package.json": JSON.stringify({
        name: "fixture",
        packageManager: "npm@11.0.0",
      }),
      "package-lock.json": "{}",
    });
    const stateDirectory = `${root}/state`;
    let planOutput = "";
    await runCli(
      ["plan", "package-manager", "--to", "bun", "--state-dir", stateDirectory, "--json"],
      {
        stdout: { write: (content) => { planOutput += content; } },
        stderr: { write: () => undefined },
      },
      root,
    );
    const planId = (JSON.parse(planOutput) as { planId: string }).planId;
    let stdout = "";

    const exitCode = await runCli(
      ["apply", planId, "--state-dir", stateDirectory, "--json", "--non-interactive"],
      {
        stdout: { write: (content) => { stdout += content; } },
        stderr: { write: () => undefined },
      },
      root,
    );
    const result = JSON.parse(stdout) as {
      status: string;
      diagnostics: Array<{ code: string }>;
    };

    expect(exitCode).toBe(7);
    expect(result.status).toBe("failed");
    expect(result.diagnostics[0]?.code).toBe("APPROVAL_REQUIRED");
  });

  test("returns a structured diagnostic for a missing repository root", async () => {
    const parent = await createProject({});
    let stdout = "";

    const exitCode = await runCli(
      ["inspect", "package-manager", "--cwd", `${parent}/missing`, "--json"],
      {
        stdout: { write: (content) => { stdout += content; } },
        stderr: { write: () => undefined },
      },
      parent,
    );
    const result = JSON.parse(stdout) as {
      status: string;
      diagnostics: Array<{ code: string }>;
    };

    expect(exitCode).toBe(3);
    expect(result.status).toBe("blocked");
    expect(result.diagnostics[0]?.code).toBe("REPOSITORY_ROOT_NOT_FOUND");
  });

  test("installs and removes the project Agent Skill through explicit approval", async () => {
    const root = await createProject({});
    const approval = "skill:pkgshift:project:claude";
    let installOutput = "";
    const installExit = await runCli(
      [
        "skill", "install",
        "--scope", "project",
        "--client", "claude",
        "--approve", approval,
        "--json",
      ],
      {
        stdout: { write: (content) => { installOutput += content; } },
        stderr: { write: () => undefined },
      },
      root,
    );
    const installed = JSON.parse(installOutput) as {
      status: string;
      summary: { healthy: boolean; targetPath: string; mutationPerformed: boolean };
    };
    expect(installExit).toBe(0);
    expect(installed.summary.healthy).toBeTrue();
    expect(installed.summary.targetPath).toContain("/.claude/skills/");
    expect(installed.summary.mutationPerformed).toBeTrue();

    let repeatedOutput = "";
    const repeatedExit = await runCli(
      [
        "skill", "install",
        "--scope", "project",
        "--client", "claude",
        "--approve", approval,
        "--json",
      ],
      {
        stdout: { write: (content) => { repeatedOutput += content; } },
        stderr: { write: () => undefined },
      },
      root,
    );
    const repeated = JSON.parse(repeatedOutput) as {
      summary: { mutationPerformed: boolean };
    };
    expect(repeatedExit).toBe(0);
    expect(repeated.summary.mutationPerformed).toBeFalse();

    let uninstallOutput = "";
    const uninstallExit = await runCli(
      [
        "skill", "uninstall",
        "--scope", "project",
        "--client", "claude",
        "--approve", approval,
        "--json",
      ],
      {
        stdout: { write: (content) => { uninstallOutput += content; } },
        stderr: { write: () => undefined },
      },
      root,
    );
    const uninstalled = JSON.parse(uninstallOutput) as {
      summary: { installed: boolean; mutationPerformed: boolean };
    };
    expect(uninstallExit).toBe(0);
    expect(uninstalled.summary.installed).toBeFalse();
    expect(uninstalled.summary.mutationPerformed).toBeTrue();
  });
});
