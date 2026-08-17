#!/usr/bin/env bun

import { mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";

const repositoryRoot = resolve(import.meta.dir, "..");
const manifestPath = join(repositoryRoot, "validation/real-world-corpus.json");
const binary = resolve(
  repositoryRoot,
  process.env.PKGSHIFT_BIN ?? "target/release/pkgshift",
);
const outputPath = process.env.PKGSHIFT_CORPUS_OUTPUT
  ? resolve(repositoryRoot, process.env.PKGSHIFT_CORPUS_OUTPUT)
  : undefined;

function run(argv, options = {}) {
  const processResult = Bun.spawnSync(argv, {
    cwd: options.cwd ?? repositoryRoot,
    env: process.env,
    stdout: "pipe",
    stderr: "pipe",
  });
  return {
    exitCode: processResult.exitCode,
    stdout: processResult.stdout.toString(),
    stderr: processResult.stderr.toString(),
  };
}

function requireEqual(errors, label, actual, expected) {
  if (actual !== expected) {
    errors.push(`${label}: expected ${JSON.stringify(expected)}, received ${JSON.stringify(actual)}`);
  }
}

function diagnosticCounts(diagnostics) {
  const counts = {};
  for (const diagnostic of diagnostics ?? []) {
    counts[diagnostic.code] = (counts[diagnostic.code] ?? 0) + 1;
  }
  return Object.fromEntries(Object.entries(counts).sort(([left], [right]) => left.localeCompare(right)));
}

async function checkoutRepository(workspace, repository) {
  const destination = join(workspace, repository.id);
  const initialized = run(["git", "init", "--quiet", destination]);
  if (initialized.exitCode !== 0) {
    throw new Error(`git init failed for ${repository.id}: ${initialized.stderr.trim()}`);
  }
  const remote = run(["git", "remote", "add", "origin", repository.url], { cwd: destination });
  if (remote.exitCode !== 0) {
    throw new Error(`git remote failed for ${repository.id}: ${remote.stderr.trim()}`);
  }
  const fetched = run(
    ["git", "fetch", "--quiet", "--depth=1", "origin", repository.revision],
    { cwd: destination },
  );
  if (fetched.exitCode !== 0) {
    throw new Error(`git fetch failed for ${repository.id}: ${fetched.stderr.trim()}`);
  }
  const checkedOut = run(["git", "checkout", "--quiet", "--detach", "FETCH_HEAD"], {
    cwd: destination,
  });
  if (checkedOut.exitCode !== 0) {
    throw new Error(`git checkout failed for ${repository.id}: ${checkedOut.stderr.trim()}`);
  }
  return destination;
}

function executeCase(testCase, repository) {
  const argv = [
    binary,
    "plan",
    "package-manager",
    "--to",
    testCase.target,
    "--cwd",
    repository,
    "--json",
    "--no-color",
    "--non-interactive",
  ];
  if (testCase.acceptLossy) {
    argv.push("--accept-lossy");
  }
  const execution = run(argv);
  let result;
  try {
    result = JSON.parse(execution.stdout);
  } catch (error) {
    return {
      id: testCase.id,
      passed: false,
      errors: [`pkgshift did not emit JSON: ${error.message}`, execution.stderr.trim()].filter(Boolean),
    };
  }

  const plan = result.artifacts?.find((artifact) => artifact.type === "package-manager-plan")?.content;
  const errors = [];
  requireEqual(errors, "exit code", execution.exitCode, testCase.expect.status === "blocked" ? 3 : 0);
  requireEqual(errors, "status", result.status, testCase.expect.status);
  requireEqual(errors, "source", result.summary?.source, testCase.expect.source);
  requireEqual(errors, "target", result.summary?.target, testCase.target);
  requireEqual(errors, "executable", plan?.executable, testCase.expect.executable);
  requireEqual(errors, "operations", plan?.operations?.length, testCase.expect.operations);
  for (const [classification, expected] of Object.entries(testCase.expect.capabilities)) {
    requireEqual(
      errors,
      `capabilities.${classification}`,
      plan?.capabilitySummary?.[classification],
      expected,
    );
  }
  const counts = diagnosticCounts(result.diagnostics);
  for (const code of testCase.expect.diagnosticCodes) {
    if (!counts[code]) {
      errors.push(`diagnostic code ${code} was not reported`);
    }
  }

  const workingTree = run(["git", "status", "--porcelain", "--untracked-files=all"], {
    cwd: repository,
  });
  if (workingTree.exitCode !== 0) {
    errors.push(`git status failed: ${workingTree.stderr.trim()}`);
  } else if (workingTree.stdout.trim()) {
    errors.push(`read-only planning changed the checkout: ${workingTree.stdout.trim()}`);
  }

  return {
    id: testCase.id,
    repository: testCase.repository,
    target: testCase.target,
    revision: testCase.revision,
    passed: errors.length === 0,
    status: result.status,
    executable: plan?.executable,
    operations: plan?.operations?.length,
    capabilities: plan?.capabilitySummary,
    diagnosticCounts: counts,
    errors,
  };
}

const manifest = JSON.parse(await readFile(manifestPath, "utf8"));
if (manifest.schemaVersion !== "1.0") {
  throw new Error(`unsupported corpus schema ${manifest.schemaVersion}`);
}
const binaryVersion = run([binary, "--version"]);
if (binaryVersion.exitCode !== 0) {
  throw new Error(`pkgshift binary is unavailable at ${binary}: ${binaryVersion.stderr.trim()}`);
}

const workspace = await mkdtemp(join(tmpdir(), "pkgshift-corpus-"));
const checkouts = new Map();
const outcomes = [];
try {
  for (const repository of manifest.repositories) {
    process.stderr.write(`Checking out ${repository.id}@${repository.revision}\n`);
    checkouts.set(repository.id, await checkoutRepository(workspace, repository));
  }
  for (const testCase of manifest.cases) {
    const repositoryDefinition = manifest.repositories.find(
      (repository) => repository.id === testCase.repository,
    );
    const checkout = checkouts.get(testCase.repository);
    if (!repositoryDefinition || !checkout) {
      outcomes.push({
        id: testCase.id,
        passed: false,
        errors: [`unknown repository ${testCase.repository}`],
      });
      continue;
    }
    process.stderr.write(`Planning ${testCase.id}\n`);
    outcomes.push(
      executeCase(
        { ...testCase, revision: repositoryDefinition.revision },
        checkout,
      ),
    );
  }
} finally {
  if (process.env.PKGSHIFT_CORPUS_KEEP === "1") {
    process.stderr.write(`Corpus workspace preserved at ${workspace}\n`);
  } else {
    await rm(workspace, { recursive: true, force: true });
  }
}

const summary = {
  schemaVersion: manifest.schemaVersion,
  pkgshiftVersion: binaryVersion.stdout.trim(),
  passed: outcomes.every((outcome) => outcome.passed),
  cases: outcomes,
};
const serialized = `${JSON.stringify(summary, null, 2)}\n`;
if (outputPath) {
  await mkdir(dirname(outputPath), { recursive: true });
  await writeFile(outputPath, serialized);
}
process.stdout.write(serialized);
if (!summary.passed) {
  process.exitCode = 1;
}
