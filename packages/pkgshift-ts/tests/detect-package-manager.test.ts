import { afterEach, describe, expect, test } from "bun:test";
import { readJsonObject } from "../src/core/files.ts";
import { detectPackageManager } from "../src/inspect/detect-package-manager.ts";
import { createProject, removeTemporaryProjects } from "./helpers/project.ts";
import { join } from "node:path";

afterEach(removeTemporaryProjects);

describe("package manager detection", () => {
  test("prefers an explicit pnpm packageManager declaration", async () => {
    const root = await createProject({
      "package.json": JSON.stringify({
        name: "fixture",
        packageManager: "pnpm@10.0.0",
      }),
      "package-lock.json": "{}",
    });
    const manifest = await readJsonObject(join(root, "package.json"));

    const detection = await detectPackageManager(root, manifest);

    expect(detection.selected).toBe("pnpm");
    expect(detection.candidates[0]?.confidence).toBe("high");
    expect(detection.diagnostics.map((item) => item.code)).toContain(
      "PM_CONFLICTING_EVIDENCE",
    );
  });

  test("blocks equally strong npm and pnpm lockfile evidence", async () => {
    const root = await createProject({
      "package.json": JSON.stringify({ name: "fixture" }),
      "package-lock.json": "{}",
      "pnpm-lock.yaml": "lockfileVersion: '9.0'\n",
    });
    const manifest = await readJsonObject(join(root, "package.json"));

    const detection = await detectPackageManager(root, manifest);

    expect(detection.selected).toBeNull();
    expect(detection.diagnostics.map((item) => item.code)).toContain(
      "PM_SOURCE_AMBIGUOUS",
    );
  });

  test("distinguishes Yarn Modern through its configuration", async () => {
    const root = await createProject({
      "package.json": JSON.stringify({ name: "fixture" }),
      "yarn.lock": "# yarn lockfile\n",
      ".yarnrc.yml": "nodeLinker: node-modules\n",
    });
    const manifest = await readJsonObject(join(root, "package.json"));

    const detection = await detectPackageManager(root, manifest);

    expect(detection.selected).toBe("yarn-modern");
    expect(detection.diagnostics).toHaveLength(0);
  });
});

