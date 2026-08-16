import { afterEach, describe, expect, test } from "bun:test";
import { inspectProject } from "../src/inspect/inspect-project.ts";
import { buildProjectIR } from "../src/ir/build-project-ir.ts";
import { analyzeCapabilities } from "../src/capabilities/analyze-capabilities.ts";
import { planPackageManagerMigration } from "../src/plan/plan-package-manager.ts";
import { createProject, removeTemporaryProjects } from "./helpers/project.ts";

afterEach(removeTemporaryProjects);

describe("package manager planning", () => {
  test("produces a deterministic, read-only pnpm to Bun plan", async () => {
    const root = await createProject({
      "package.json": JSON.stringify({
        name: "fixture",
        private: true,
        packageManager: "pnpm@10.0.0",
        workspaces: ["packages/*"],
      }),
      "pnpm-lock.yaml": "lockfileVersion: '9.0'\n",
      "pnpm-workspace.yaml": "packages:\n  - packages/*\n",
      "README.md": "Run `pnpm install` before development.\n",
    });
    const inspection = await inspectProject(root);
    const projectIr = await buildProjectIR(inspection);
    expect(projectIr).not.toBeNull();
    const analysis = analyzeCapabilities(projectIr!, "bun");
    expect(analysis).not.toBeNull();

    const first = await planPackageManagerMigration(inspection, projectIr!, analysis!, "bun");
    const second = await planPackageManagerMigration(inspection, projectIr!, analysis!, "bun");

    expect(first).not.toBeNull();
    expect(first?.planId).toBe(second?.planId);
    expect(first?.executable).toBeTrue();
    expect(first?.operations.map((operation) => operation.kind)).toEqual([
      "manifest.render-target",
      "integration.translate-commands",
      "dependency.import-and-install-target",
      "source.retire",
      "migration.verify",
    ]);
    expect(first?.operations.find((operation) =>
      operation.kind === "dependency.import-and-install-target"
    )?.command).toEqual([
      "bun",
      "install",
      "--ignore-scripts",
    ]);
    expect(first?.nativeImport).toEqual({
      id: "bun-pnpm-install-migration",
      source: "pnpm",
      target: "bun",
      mode: "install-integrated",
      command: ["bun", "install", "--ignore-scripts"],
      summary: "Use Bun's install-integrated pnpm lockfile migration path.",
    });
  });

  test("labels preview targets without blocking planning", async () => {
    const root = await createProject({
      "package.json": JSON.stringify({
        name: "fixture",
        packageManager: "npm@11.0.0",
      }),
      "package-lock.json": "{}",
    });
    const inspection = await inspectProject(root);
    const projectIr = await buildProjectIR(inspection);
    expect(projectIr).not.toBeNull();
    const analysis = analyzeCapabilities(projectIr!, "vlt");
    expect(analysis).not.toBeNull();

    const plan = await planPackageManagerMigration(inspection, projectIr!, analysis!, "vlt");

    expect(plan?.targetTier).toBe("preview-target");
    expect(plan?.diagnostics.map((diagnostic) => diagnostic.code)).toContain(
      "PM_TARGET_PREVIEW",
    );
  });
});
