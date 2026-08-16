import { afterEach, describe, expect, test } from "bun:test";
import { analyzeCapabilities } from "../src/capabilities/analyze-capabilities.ts";
import type { PackageManagerId, PlannedFileMutation } from "../src/domain/models.ts";
import { inspectProject } from "../src/inspect/inspect-project.ts";
import { buildProjectIR } from "../src/ir/build-project-ir.ts";
import { planPackageManagerMigration } from "../src/plan/plan-package-manager.ts";
import { createProject, removeTemporaryProjects } from "./helpers/project.ts";

afterEach(removeTemporaryProjects);

function mutations(plan: NonNullable<Awaited<ReturnType<typeof planPackageManagerMigration>>>): PlannedFileMutation[] {
  return plan.operations.flatMap((operation) => operation.mutations ?? []);
}

async function planFixture(
  files: Record<string, string>,
  target: PackageManagerId,
  acceptedLossy = false,
) {
  const root = await createProject(files);
  const inspection = await inspectProject(root);
  const projectIr = await buildProjectIR(inspection);
  const analysis = analyzeCapabilities(projectIr!, target);
  const plan = await planPackageManagerMigration(
    inspection,
    projectIr!,
    analysis!,
    target,
    { acceptedLossy },
  );
  return { root, plan: plan!, analysis: analysis! };
}

describe("target transformations", () => {
  test("plans every basic production source-to-target direction", async () => {
    const sources: Array<[PackageManagerId, Record<string, string>]> = [
      ["npm", {
        "package.json": JSON.stringify({ name: "root", packageManager: "npm@12.0.2" }),
        "package-lock.json": "{}",
      }],
      ["pnpm", {
        "package.json": JSON.stringify({ name: "root", packageManager: "pnpm@11.21.0" }),
        "pnpm-lock.yaml": "lockfileVersion: '9.0'\n",
      }],
      ["yarn-classic", {
        "package.json": JSON.stringify({ name: "root", packageManager: "yarn@1.22.22" }),
        "yarn.lock": "# fixture\n",
      }],
      ["yarn-modern", {
        "package.json": JSON.stringify({ name: "root", packageManager: "yarn@4.18.0" }),
        ".yarnrc.yml": "nodeLinker: node-modules\n",
        "yarn.lock": "# fixture\n",
      }],
      ["bun", {
        "package.json": JSON.stringify({ name: "root", packageManager: "bun@1.3.14" }),
        "bun.lock": "{}\n",
      }],
    ];
    const targets = sources.map(([source]) => source);
    let plannedDirections = 0;
    for (const [source, files] of sources) {
      for (const target of targets) {
        if (target === source) continue;
        const { plan } = await planFixture(files, target);
        expect(plan.executable, `${source} -> ${target}`).toBeTrue();
        expect(plan.operations.some((entry) => entry.kind === "dependency.install-target")).toBeTrue();
        plannedDirections += 1;
      }
    }
    expect(plannedDirections).toBe(20);
  });

  test("expands pnpm workspace and catalog protocols for npm", async () => {
    const { plan } = await planFixture({
      "package.json": JSON.stringify({
        name: "root",
        private: true,
        packageManager: "pnpm@10.0.0",
        workspaces: ["packages/*"],
      }),
      "packages/app/package.json": JSON.stringify({
        name: "app",
        dependencies: { lib: "workspace:^", react: "catalog:" },
      }),
      "packages/lib/package.json": JSON.stringify({ name: "lib", version: "2.3.4" }),
      "pnpm-workspace.yaml": "packages:\n  - packages/*\ncatalog:\n  react: ^19.0.0\n",
      "pnpm-lock.yaml": "lockfileVersion: '9.0'\n",
    }, "npm", true);

    expect(plan.executable).toBeTrue();
    expect(plan.acceptedLossy).toBeTrue();
    const appMutation = mutations(plan).find((entry) => entry.path === "packages/app/package.json");
    const app = JSON.parse(appMutation?.content ?? "{}") as {
      dependencies: Record<string, string>;
    };
    expect(app.dependencies).toEqual({ lib: "^2.3.4", react: "^19.0.0" });
    const rootMutation = mutations(plan).find((entry) => entry.path === "package.json");
    const root = JSON.parse(rootMutation?.content ?? "{}") as {
      packageManager: string;
      workspaces: string[];
    };
    expect(root.packageManager).toBe("npm@12.0.2");
    expect(root.workspaces).toEqual(["packages/*"]);
  });

  test("requires explicit acceptance before a lossy plan becomes executable", async () => {
    const { plan } = await planFixture({
      "package.json": JSON.stringify({
        name: "root",
        private: true,
        packageManager: "pnpm@10.0.0",
        workspaces: ["packages/*"],
      }),
      "packages/app/package.json": JSON.stringify({
        name: "app",
        dependencies: { react: "catalog:" },
      }),
      "pnpm-workspace.yaml": "packages:\n  - packages/*\ncatalog:\n  react: ^19.0.0\n",
      "pnpm-lock.yaml": "lockfileVersion: '9.0'\n",
    }, "npm");

    expect(plan.executable).toBeFalse();
    expect(plan.diagnostics.map((entry) => entry.code)).toContain("LOSSY_ACCEPTANCE_REQUIRED");
  });

  test("renders pnpm selectors from nested npm overrides", async () => {
    const { plan } = await planFixture({
      "package.json": JSON.stringify({
        name: "root",
        packageManager: "npm@11.0.0",
        overrides: { parent: { child: "1.2.3" } },
      }),
      "package-lock.json": "{}",
    }, "pnpm");

    expect(plan.executable).toBeTrue();
    const configuration = mutations(plan).find((entry) => entry.path === "pnpm-workspace.yaml");
    const parsed = Bun.YAML.parse(configuration?.content ?? "") as {
      overrides: Record<string, string>;
    };
    expect(parsed.overrides).toEqual({ "parent>child": "1.2.3" });
  });

  test("renders executable baseline plans for both Yarn families and Bun", async () => {
    for (const [target, pin] of [
      ["yarn-classic", "yarn@1.22.22"],
      ["yarn-modern", "yarn@4.18.0"],
      ["bun", "bun@1.3.14"],
    ] as const) {
      const { plan } = await planFixture({
        "package.json": JSON.stringify({ name: "root", packageManager: "npm@11.0.0" }),
        "package-lock.json": "{}",
      }, target);
      expect(plan.executable).toBeTrue();
      const manifest = mutations(plan).find((entry) => entry.path === "package.json");
      expect(JSON.parse(manifest?.content ?? "{}").packageManager).toBe(pin);
      if (target === "yarn-modern") {
        const yarnConfiguration = mutations(plan).find((entry) => entry.path === ".yarnrc.yml");
        const parsed = Bun.YAML.parse(yarnConfiguration?.content ?? "") as Record<string, unknown>;
        expect(parsed.nodeLinker).toBe("node-modules");
        expect(plan.operations.find((entry) => entry.kind === "dependency.install-target")?.command).toEqual([
          "yarn",
          "install",
          "--mode=skip-build",
        ]);
      }
    }
  });

  test("blocks literal registry credentials from entering a Yarn plan", async () => {
    const { plan } = await planFixture({
      "package.json": JSON.stringify({ name: "root", packageManager: "npm@11.0.0" }),
      "package-lock.json": "{}",
      ".npmrc": "//registry.npmjs.org/:_authToken=literal-secret\n",
    }, "yarn-modern");

    expect(plan.executable).toBeFalse();
    expect(plan.diagnostics.map((entry) => entry.code)).toContain(
      "REGISTRY_SECRET_REQUIRES_ENVIRONMENT_REFERENCE",
    );
    expect(JSON.stringify(plan)).not.toContain("literal-secret");
  });

  test("preserves environment-backed registry references for Yarn Modern", async () => {
    const { plan } = await planFixture({
      "package.json": JSON.stringify({ name: "root", packageManager: "npm@11.0.0" }),
      "package-lock.json": "{}",
      ".npmrc": "registry=https://registry.npmjs.org\n//registry.npmjs.org/:_authToken=${NPM_TOKEN}\n",
    }, "yarn-modern");

    expect(plan.executable).toBeTrue();
    const configuration = mutations(plan).find((entry) => entry.path === ".yarnrc.yml");
    expect(configuration?.content).toContain("${NPM_TOKEN}");
    expect(mutations(plan)).toContainEqual(expect.objectContaining({
      path: ".npmrc",
      action: "delete",
    }));
  });

  test("blocks unsupported npmrc settings and workspace specifier variants", async () => {
    const npmrc = await planFixture({
      "package.json": JSON.stringify({ name: "root", packageManager: "npm@12.0.2" }),
      "package-lock.json": "{}",
      ".npmrc": "strict-ssl=false\n",
    }, "yarn-modern");
    expect(npmrc.plan.executable).toBeFalse();
    expect(npmrc.plan.diagnostics.map((entry) => entry.code)).toContain("NPMRC_SETTING_UNSUPPORTED");

    const workspace = await planFixture({
      "package.json": JSON.stringify({
        name: "root",
        private: true,
        packageManager: "pnpm@11.21.0",
        workspaces: ["packages/*"],
      }),
      "packages/app/package.json": JSON.stringify({
        name: "app",
        dependencies: { lib: "workspace:../lib" },
      }),
      "packages/lib/package.json": JSON.stringify({ name: "lib", version: "1.0.0" }),
      "pnpm-workspace.yaml": "packages:\n  - packages/*\n",
      "pnpm-lock.yaml": "lockfileVersion: '9.0'\n",
    }, "npm");
    expect(workspace.plan.executable).toBeFalse();
    expect(workspace.plan.diagnostics.map((entry) => entry.code)).toContain(
      "WORKSPACE_SPECIFIER_UNSUPPORTED",
    );
  });

  test("keeps literal manifest credentials out of plan artifacts", async () => {
    const secret = "never-persist-this-value";
    const { plan } = await planFixture({
      "package.json": JSON.stringify({
        name: "root",
        packageManager: "npm@12.0.2",
        deploymentToken: secret,
      }),
      "package-lock.json": "{}",
    }, "bun");

    expect(plan.executable).toBeFalse();
    expect(plan.diagnostics.map((entry) => entry.code)).toContain("SECRET_REDACTION_FAILED");
    expect(JSON.stringify(plan)).not.toContain(secret);
  });
});
