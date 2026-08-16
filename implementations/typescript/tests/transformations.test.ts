import { afterEach, describe, expect, test } from "bun:test";
import { symlink } from "node:fs/promises";
import { join } from "node:path";
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

const TEXT_PATCH = [
  "diff --git a/index.js b/index.js",
  "--- a/index.js",
  "+++ b/index.js",
  "@@ -1 +1 @@",
  "-old",
  "+new",
  "",
].join("\n");

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
        expect(plan.operations.some((entry) =>
          entry.kind === "dependency.install-target"
          || entry.kind === "dependency.import-and-install-target"
        )).toBeTrue();
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

  test("renders current pnpm linker and lifecycle policy", async () => {
    const { plan } = await planFixture({
      "package.json": JSON.stringify({
        name: "root",
        private: true,
        packageManager: "bun@1.3.14",
        trustedDependencies: ["esbuild", "sharp"],
      }),
      "bun.lock": "{}\n",
      "bunfig.toml": "[install]\nlinker = \"isolated\"\n",
    }, "pnpm");

    expect(plan.executable).toBeTrue();
    const configuration = mutations(plan).find((entry) => entry.path === "pnpm-workspace.yaml");
    const parsed = Bun.YAML.parse(configuration?.content ?? "") as {
      nodeLinker: string;
      allowBuilds: Record<string, boolean>;
    };
    expect(parsed.nodeLinker).toBe("isolated");
    expect(parsed.allowBuilds).toEqual({ esbuild: true, sharp: true });
    expect(configuration?.content).not.toContain("onlyBuiltDependencies");
  });

  test("renders a Yarn lifecycle allow-list with dependency scripts disabled", async () => {
    const { plan } = await planFixture({
      "package.json": JSON.stringify({
        name: "root",
        private: true,
        packageManager: "pnpm@11.21.0",
      }),
      "pnpm-lock.yaml": "lockfileVersion: '9.0'\n",
      "pnpm-workspace.yaml": [
        "nodeLinker: isolated",
        "allowBuilds:",
        "  esbuild: true",
        "  blocked-package: false",
        "",
      ].join("\n"),
    }, "yarn-modern");

    expect(plan.executable).toBeTrue();
    const configuration = mutations(plan).find((entry) => entry.path === ".yarnrc.yml");
    const parsed = Bun.YAML.parse(configuration?.content ?? "") as Record<string, unknown>;
    expect(parsed.nodeLinker).toBe("pnpm");
    expect(parsed.enableScripts).toBeFalse();
    const manifest = mutations(plan).find((entry) => entry.path === "package.json");
    const root = JSON.parse(manifest?.content ?? "{}") as {
      dependenciesMeta: Record<string, { built: boolean }>;
    };
    expect(root.dependenciesMeta).toEqual({ esbuild: { built: true } });
  });

  test("reads a Yarn lifecycle allow-list when migrating to Bun", async () => {
    const { plan } = await planFixture({
      "package.json": JSON.stringify({
        name: "root",
        private: true,
        packageManager: "yarn@4.18.0",
        dependenciesMeta: {
          esbuild: { built: true },
          "blocked-package": { built: false },
        },
      }),
      "yarn.lock": "# fixture\n",
      ".yarnrc.yml": "nodeLinker: node-modules\nenableScripts: false\n",
    }, "bun");

    expect(plan.executable).toBeTrue();
    const manifest = mutations(plan).find((entry) => entry.path === "package.json");
    const root = JSON.parse(manifest?.content ?? "{}") as Record<string, unknown>;
    expect(root.trustedDependencies).toEqual(["esbuild"]);
    expect(root.dependenciesMeta).toBeUndefined();
  });

  test("blocks Yarn build denials outside allow-list mode", async () => {
    const { plan } = await planFixture({
      "package.json": JSON.stringify({
        name: "root",
        private: true,
        packageManager: "yarn@4.18.0",
        dependenciesMeta: { "native-addon": { built: false } },
      }),
      "yarn.lock": "# fixture\n",
      ".yarnrc.yml": "nodeLinker: node-modules\n",
    }, "bun");

    expect(plan.executable).toBeFalse();
    expect(plan.diagnostics.map((entry) => entry.code)).toContain(
      "YARN_BUILD_POLICY_UNSUPPORTED",
    );
  });

  test("preserves an empty Yarn lifecycle allow-list in pnpm", async () => {
    const { plan } = await planFixture({
      "package.json": JSON.stringify({
        name: "root",
        private: true,
        packageManager: "yarn@4.18.0",
      }),
      "yarn.lock": "# fixture\n",
      ".yarnrc.yml": "nodeLinker: node-modules\nenableScripts: false\n",
    }, "pnpm");

    expect(plan.executable).toBeTrue();
    const configuration = mutations(plan).find((entry) => entry.path === "pnpm-workspace.yaml");
    const parsed = Bun.YAML.parse(configuration?.content ?? "") as {
      allowBuilds: Record<string, boolean>;
    };
    expect(parsed.allowBuilds).toEqual({});
  });

  test("preserves a bare scoped Yarn resolution as an npm override", async () => {
    const { plan } = await planFixture({
      "package.json": JSON.stringify({
        name: "root",
        packageManager: "yarn@1.22.22",
        resolutions: { "@scope/package": "1.2.3" },
      }),
      "yarn.lock": "# fixture\n",
    }, "npm");

    expect(plan.executable).toBeTrue();
    const manifest = mutations(plan).find((entry) => entry.path === "package.json");
    expect(JSON.parse(manifest?.content ?? "{}").overrides).toEqual({
      "@scope/package": "1.2.3",
    });
  });

  test("renders npm package extensions in pnpm workspace configuration", async () => {
    const { plan } = await planFixture({
      "package.json": JSON.stringify({
        name: "root",
        private: true,
        packageManager: "npm@12.0.2",
        packageExtensions: {
          "bare-package": {
            dependencies: { "bare-runtime-dep": "1.0.0" },
          },
          "broken-package@^1": {
            dependencies: { "missing-runtime-dep": "^2.0.0" },
            peerDependencies: { react: "*" },
            peerDependenciesMeta: { react: { optional: true } },
          },
        },
      }),
      "package-lock.json": "{}",
    }, "pnpm");

    expect(plan.executable).toBeTrue();
    const manifest = mutations(plan).find((entry) => entry.path === "package.json");
    expect(JSON.parse(manifest?.content ?? "{}").packageExtensions).toBeUndefined();
    const configuration = mutations(plan).find((entry) => entry.path === "pnpm-workspace.yaml");
    const parsed = Bun.YAML.parse(configuration?.content ?? "") as {
      packageExtensions: Record<string, {
        dependencies: Record<string, string>;
        peerDependenciesMeta: Record<string, { optional: boolean }>;
      }>;
    };
    expect(parsed.packageExtensions["broken-package@^1"]!.dependencies).toEqual({
      "missing-runtime-dep": "^2.0.0",
    });
    expect(parsed.packageExtensions["bare-package"]!.dependencies).toEqual({
      "bare-runtime-dep": "1.0.0",
    });
    expect(parsed.packageExtensions["broken-package@^1"]!.peerDependenciesMeta).toEqual({
      react: { optional: true },
    });
  });

  test("renders pnpm package extensions in Yarn configuration", async () => {
    const { plan } = await planFixture({
      "package.json": JSON.stringify({
        name: "root",
        private: true,
        packageManager: "pnpm@11.21.0",
      }),
      "pnpm-lock.yaml": "lockfileVersion: '9.0'\n",
      "pnpm-workspace.yaml": [
        "packageExtensions:",
        "  'broken-package@1':",
        "    optionalDependencies:",
        "      optional-runtime: '^3.0.0'",
        "",
      ].join("\n"),
    }, "yarn-modern");

    expect(plan.executable).toBeTrue();
    const configuration = mutations(plan).find((entry) => entry.path === ".yarnrc.yml");
    const parsed = Bun.YAML.parse(configuration?.content ?? "") as {
      packageExtensions: Record<string, {
        optionalDependencies: Record<string, string>;
      }>;
    };
    expect(parsed.packageExtensions["broken-package@1"]!.optionalDependencies).toEqual({
      "optional-runtime": "^3.0.0",
    });
  });

  test("reads Yarn package extensions and renders them for npm", async () => {
    const { plan, analysis } = await planFixture({
      "package.json": JSON.stringify({
        name: "root",
        private: true,
        packageManager: "yarn@4.18.0",
      }),
      "yarn.lock": "# fixture\n",
      ".yarnrc.yml": [
        "nodeLinker: node-modules",
        "packageExtensions:",
        "  '@scope/broken@^2':",
        "    peerDependencies:",
        "      react: '>=18'",
        "",
      ].join("\n"),
    }, "npm");

    expect(analysis.decisions.map((entry) => entry.featureId)).toContain(
      "resolution.package-extensions",
    );
    expect(plan.executable).toBeTrue();
    const manifest = mutations(plan).find((entry) => entry.path === "package.json");
    expect(JSON.parse(manifest?.content ?? "{}").packageExtensions).toEqual({
      "@scope/broken@^2": { peerDependencies: { react: ">=18" } },
    });
  });

  test("blocks package extensions outside the shared schema", async () => {
    const { plan } = await planFixture({
      "package.json": JSON.stringify({
        name: "root",
        packageManager: "npm@12.0.2",
        packageExtensions: {
          "broken-package@1": { scripts: { postinstall: "node build.js" } },
        },
      }),
      "package-lock.json": "{}",
    }, "pnpm");

    expect(plan.executable).toBeFalse();
    expect(plan.diagnostics.map((entry) => entry.code)).toContain(
      "PACKAGE_EXTENSIONS_UNSUPPORTED",
    );
    expect(JSON.stringify(plan)).not.toContain("postinstall");
  });

  test("converts a Yarn patch protocol dependency to Bun policy", async () => {
    const { plan } = await planFixture({
      "package.json": JSON.stringify({
        name: "root",
        private: true,
        packageManager: "yarn@4.18.0",
        dependencies: {
          "left-pad": "patch:left-pad@npm%3A1.3.0#~/.yarn/patches/left-pad.patch",
        },
      }),
      "yarn.lock": "# fixture\n",
      ".yarnrc.yml": "nodeLinker: node-modules\n",
      ".yarn/patches/left-pad.patch": TEXT_PATCH,
    }, "bun");

    expect(plan.executable).toBeTrue();
    expect(plan.diagnostics.map((entry) => entry.code)).not.toContain(
      "TRANSFORMATION_UNIMPLEMENTED",
    );
    const manifest = mutations(plan).find((entry) => entry.path === "package.json");
    const parsed = JSON.parse(manifest?.content ?? "{}") as {
      dependencies: Record<string, string>;
      patchedDependencies: Record<string, string>;
    };
    expect(parsed.dependencies["left-pad"]).toBe("1.3.0");
    expect(parsed.patchedDependencies).toEqual({
      "left-pad@1.3.0": ".yarn/patches/left-pad.patch",
    });
  });

  test("converts a scoped Yarn patch protocol dependency to pnpm policy", async () => {
    const { plan } = await planFixture({
      "package.json": JSON.stringify({
        name: "root",
        private: true,
        packageManager: "yarn@4.18.0",
        devDependencies: {
          "@scope/tool": "patch:@scope/tool@npm%3A2.1.0#~/.yarn/patches/tool.patch",
        },
      }),
      "yarn.lock": "# fixture\n",
      ".yarnrc.yml": "nodeLinker: node-modules\n",
      ".yarn/patches/tool.patch": TEXT_PATCH,
    }, "pnpm");

    expect(plan.executable).toBeTrue();
    const manifest = mutations(plan).find((entry) => entry.path === "package.json");
    const parsedManifest = JSON.parse(manifest?.content ?? "{}") as {
      devDependencies: Record<string, string>;
    };
    expect(parsedManifest.devDependencies["@scope/tool"]).toBe("2.1.0");
    const configuration = mutations(plan).find((entry) => entry.path === "pnpm-workspace.yaml");
    const parsed = Bun.YAML.parse(configuration?.content ?? "") as {
      patchedDependencies: Record<string, string>;
    };
    expect(parsed.patchedDependencies).toEqual({
      "@scope/tool@2.1.0": ".yarn/patches/tool.patch",
    });
  });

  test("converts a transitive Yarn patch resolution to Bun policy", async () => {
    const { plan, analysis } = await planFixture({
      "package.json": JSON.stringify({
        name: "root",
        private: true,
        packageManager: "yarn@4.18.0",
        dependencies: { parent: "1.0.0" },
        resolutions: {
          "left-pad@npm:1.3.0":
            "patch:left-pad@npm%3A1.3.0#~/.yarn/patches/left-pad.patch",
        },
      }),
      "yarn.lock": "# fixture\n",
      ".yarnrc.yml": "nodeLinker: node-modules\n",
      ".yarn/patches/left-pad.patch": TEXT_PATCH,
    }, "bun");

    expect(analysis.decisions.map((entry) => entry.featureId)).toContain(
      "dependency.patch-protocol",
    );
    expect(plan.executable).toBeTrue();
    const manifest = mutations(plan).find((entry) => entry.path === "package.json");
    const parsed = JSON.parse(manifest?.content ?? "{}") as {
      resolutions?: Record<string, string>;
      patchedDependencies: Record<string, string>;
    };
    expect(parsed.resolutions).toBeUndefined();
    expect(parsed.patchedDependencies).toEqual({
      "left-pad@1.3.0": ".yarn/patches/left-pad.patch",
    });
  });

  test("converts pnpm patched dependencies to Yarn resolutions", async () => {
    const { plan } = await planFixture({
      "package.json": JSON.stringify({
        name: "root",
        private: true,
        packageManager: "pnpm@11.21.0",
        dependencies: { "left-pad": "^1.3.0" },
      }),
      "pnpm-lock.yaml": "lockfileVersion: '9.0'\n",
      "pnpm-workspace.yaml": [
        "patchedDependencies:",
        "  'left-pad@1.3.0': 'patches/left-pad.patch'",
        "",
      ].join("\n"),
      "patches/left-pad.patch": TEXT_PATCH,
    }, "yarn-modern");

    expect(plan.executable).toBeTrue();
    const manifest = mutations(plan).find((entry) => entry.path === "package.json");
    const parsed = JSON.parse(manifest?.content ?? "{}") as {
      dependencies: Record<string, string>;
      resolutions: Record<string, string>;
    };
    expect(parsed.dependencies["left-pad"]).toBe("^1.3.0");
    expect(parsed.resolutions).toEqual({
      "left-pad@npm:1.3.0": "patch:left-pad@npm%3A1.3.0#~/patches/left-pad.patch",
    });
  });

  test("carries Bun patched dependencies into pnpm configuration", async () => {
    const { plan } = await planFixture({
      "package.json": JSON.stringify({
        name: "root",
        private: true,
        packageManager: "bun@1.3.14",
        patchedDependencies: { "left-pad@1.3.0": "patches/left-pad.patch" },
      }),
      "bun.lock": "{}\n",
      "patches/left-pad.patch": TEXT_PATCH,
    }, "pnpm");

    expect(plan.executable).toBeTrue();
    const manifest = mutations(plan).find((entry) => entry.path === "package.json");
    expect(JSON.parse(manifest?.content ?? "{}").patchedDependencies).toBeUndefined();
    const configuration = mutations(plan).find((entry) => entry.path === "pnpm-workspace.yaml");
    const parsed = Bun.YAML.parse(configuration?.content ?? "") as {
      patchedDependencies: Record<string, string>;
    };
    expect(parsed.patchedDependencies).toEqual({
      "left-pad@1.3.0": "patches/left-pad.patch",
    });
  });

  test("blocks patch ranges and missing patch files", async () => {
    const range = await planFixture({
      "package.json": JSON.stringify({
        name: "root",
        packageManager: "yarn@4.18.0",
        dependencies: {
          "left-pad": "patch:left-pad@npm%3A%5E1.3.0#~/.yarn/patches/left-pad.patch",
        },
      }),
      "yarn.lock": "# fixture\n",
      ".yarnrc.yml": "nodeLinker: node-modules\n",
    }, "bun");
    expect(range.plan.executable).toBeFalse();
    expect(range.plan.diagnostics.map((entry) => entry.code)).toContain(
      "PATCH_SELECTOR_UNSUPPORTED",
    );

    const missing = await planFixture({
      "package.json": JSON.stringify({
        name: "root",
        packageManager: "bun@1.3.14",
        patchedDependencies: { "left-pad@1.3.0": "patches/missing.patch" },
      }),
      "bun.lock": "{}\n",
    }, "pnpm");
    expect(missing.plan.executable).toBeFalse();
    expect(missing.plan.diagnostics.map((entry) => entry.code)).toContain(
      "PATCH_FILE_NOT_FOUND",
    );
  });

  test("blocks patch paths backed by symbolic links", async () => {
    if (process.platform === "win32") return;
    const root = await createProject({
      "package.json": JSON.stringify({
        name: "root",
        packageManager: "bun@1.3.14",
        patchedDependencies: { "left-pad@1.3.0": "patches/linked.patch" },
      }),
      "bun.lock": "{}\n",
      "patches/real.patch": TEXT_PATCH,
    });
    await symlink("real.patch", join(root, "patches/linked.patch"));
    const inspection = await inspectProject(root);
    const projectIr = await buildProjectIR(inspection);
    const analysis = analyzeCapabilities(projectIr!, "pnpm");
    const plan = await planPackageManagerMigration(
      inspection,
      projectIr!,
      analysis!,
      "pnpm",
    );

    expect(plan?.executable).toBeFalse();
    expect(plan?.diagnostics.map((entry) => entry.code)).toContain(
      "PATCH_PATH_UNSUPPORTED",
    );
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
