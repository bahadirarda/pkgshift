import { afterEach, describe, expect, test } from "bun:test";
import { writeFile } from "node:fs/promises";
import { join } from "node:path";
import { analyzeCapabilities } from "../src/capabilities/analyze-capabilities.ts";
import { inspectProject } from "../src/inspect/inspect-project.ts";
import { buildProjectIR } from "../src/ir/build-project-ir.ts";
import { planPackageManagerMigration } from "../src/plan/plan-package-manager.ts";
import { createProject, removeTemporaryProjects } from "./helpers/project.ts";

afterEach(removeTemporaryProjects);

describe("Project IR", () => {
  test("blocks workspace glob syntax outside the deterministic subset", async () => {
    const root = await createProject({
      "package.json": JSON.stringify({
        name: "root",
        private: true,
        packageManager: "npm@12.0.2",
        workspaces: ["{apps,packages}/*"],
      }),
      "package-lock.json": "{}",
      "apps/web/package.json": JSON.stringify({ name: "web", version: "1.0.0" }),
    });
    const inspection = await inspectProject(root);
    const projectIr = await buildProjectIR(inspection);

    expect(projectIr?.diagnostics).toContainEqual(expect.objectContaining({
      code: "WORKSPACE_PATTERN_UNSUPPORTED",
      blocking: true,
    }));
  });

  test("extracts workspace, dependency protocol, policy, and linker semantics", async () => {
    const root = await createProject({
      "package.json": JSON.stringify({
        name: "fixture-root",
        private: true,
        packageManager: "pnpm@11.0.0",
        workspaces: ["packages/*"],
      }),
      "packages/app/package.json": JSON.stringify({
        name: "@fixture/app",
        dependencies: {
          "@fixture/lib": "workspace:*",
          react: "catalog:",
        },
      }),
      "packages/lib/package.json": JSON.stringify({
        name: "@fixture/lib",
        version: "1.0.0",
      }),
      "pnpm-lock.yaml": "lockfileVersion: '9.0'\n",
      "pnpm-workspace.yaml": [
        "packages:",
        "  - packages/*",
        "catalog:",
        "  react: ^19.0.0",
        "overrides:",
        "  parent:",
        "    child: 1.0.0",
        "patchedDependencies:",
        "  example@1.0.0: patches/example.patch",
        "nodeLinker: isolated",
        "onlyBuiltDependencies:",
        "  - esbuild",
        "",
      ].join("\n"),
      ".npmrc": "//registry.npmjs.org/:_authToken=secret-value\n",
    });
    const inspection = await inspectProject(root);

    const projectIr = await buildProjectIR(inspection);

    expect(projectIr).not.toBeNull();
    expect(projectIr?.packages.map((entry) => entry.path)).toEqual([
      ".",
      "packages/app",
      "packages/lib",
    ]);
    expect(projectIr?.features.map((feature) => feature.id)).toEqual([
      "dependency.catalog-protocol",
      "dependency.workspace-protocol",
      "install.isolated-linker",
      "lifecycle.trusted-dependencies",
      "patch.patched-dependencies",
      "policy.catalogs",
      "registry.npmrc",
      "resolution.nested-overrides",
      "resolution.overrides",
      "workspace.manifest",
    ]);
    expect(JSON.stringify(projectIr)).not.toContain("secret-value");
    expect(projectIr?.projectIrId).toStartWith("ir_");
  });

  test("accepts UTF-8 BOM in workspace manifests", async () => {
    const root = await createProject({
      "package.json": `\uFEFF${JSON.stringify({
        name: "fixture-root",
        private: true,
        packageManager: "npm@12.0.2",
        workspaces: ["packages/*"],
      })}`,
      "packages/app/package.json": `\uFEFF${JSON.stringify({
        name: "@fixture/app",
        version: "1.0.0",
      })}`,
      "package-lock.json": "{}",
    });

    const inspection = await inspectProject(root);
    const projectIr = await buildProjectIR(inspection);

    expect(projectIr?.packages.map((entry) => entry.name)).toEqual([
      "fixture-root",
      "@fixture/app",
    ]);
  });

  test("blocks a target with a known unsupported capability", async () => {
    const root = await createProject({
      "package.json": JSON.stringify({
        name: "fixture",
        packageManager: "npm@11.0.0",
        overrides: {
          parent: {
            child: "1.0.0",
          },
        },
      }),
      "package-lock.json": "{}",
    });
    const inspection = await inspectProject(root);
    const projectIr = await buildProjectIR(inspection);
    const analysis = analyzeCapabilities(projectIr!, "bun");

    expect(analysis?.decisions.find((decision) =>
      decision.featureId === "resolution.nested-overrides"
    )?.classification).toBe("unsupported");
    expect(analysis?.diagnostics.map((diagnostic) => diagnostic.code)).toContain(
      "CAPABILITY_UNSUPPORTED",
    );

    const plan = await planPackageManagerMigration(inspection, projectIr!, analysis!, "bun");
    expect(plan?.diagnostics.some((diagnostic) => diagnostic.blocking)).toBeTrue();
  });

  test("classifies catalog expansion as a lossy but reviewable decision", async () => {
    const root = await createProject({
      "package.json": JSON.stringify({
        name: "fixture",
        private: true,
        packageManager: "pnpm@11.0.0",
        workspaces: ["packages/*"],
      }),
      "packages/app/package.json": JSON.stringify({
        name: "@fixture/app",
        dependencies: { react: "catalog:" },
      }),
      "pnpm-lock.yaml": "lockfileVersion: '9.0'\n",
      "pnpm-workspace.yaml": "packages:\n  - packages/*\ncatalog:\n  react: ^19.0.0\n",
    });
    const inspection = await inspectProject(root);
    const projectIr = await buildProjectIR(inspection);
    const analysis = analyzeCapabilities(projectIr!, "npm");

    expect(analysis?.summary.lossy).toBe(2);
    expect(analysis?.diagnostics.every((diagnostic) => !diagnostic.blocking)).toBeTrue();
    expect(analysis?.diagnostics.map((diagnostic) => diagnostic.code)).toEqual([
      "CAPABILITY_LOSSY",
      "CAPABILITY_LOSSY",
    ]);
  });

  test("redacts registry credentials before repository fingerprinting", async () => {
    const root = await createProject({
      "package.json": JSON.stringify({
        name: "fixture",
        packageManager: "npm@11.0.0",
      }),
      "package-lock.json": "{}",
      ".npmrc": "//registry.npmjs.org/:_authToken=first-secret\n",
    });
    const first = await inspectProject(root);

    await writeFile(
      join(root, ".npmrc"),
      "//registry.npmjs.org/:_authToken=second-secret\n",
      "utf8",
    );
    const second = await inspectProject(root);

    expect(first.fingerprint).toBe(second.fingerprint);
    expect(JSON.stringify(first)).not.toContain("first-secret");
    expect(JSON.stringify(second)).not.toContain("second-secret");
  });

  test("includes arbitrary project patch files in repository fingerprints", async () => {
    const root = await createProject({
      "package.json": JSON.stringify({
        name: "fixture",
        packageManager: "bun@1.3.14",
        patchedDependencies: { "left-pad@1.3.0": "patches/left-pad.patch" },
      }),
      "bun.lock": "{}\n",
      "patches/left-pad.patch": "diff --git a/index.js b/index.js\n--- a/index.js\n+++ b/index.js\n",
    });
    const first = await inspectProject(root);

    await writeFile(
      join(root, "patches/left-pad.patch"),
      "diff --git a/index.js b/index.js\n--- a/index.js\n+++ b/index.js\n+changed\n",
      "utf8",
    );
    const second = await inspectProject(root);

    expect(first.relevantFiles).toContain("patches/left-pad.patch");
    expect(first.fingerprint).not.toBe(second.fingerprint);
  });
});
