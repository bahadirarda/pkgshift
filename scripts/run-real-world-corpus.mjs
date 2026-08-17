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

function executeJson(argv, cwd) {
  const execution = run(argv, { cwd });
  try {
    return { execution, result: JSON.parse(execution.stdout), errors: [] };
  } catch (error) {
    return {
      execution,
      result: undefined,
      errors: [`pkgshift did not emit JSON: ${error.message}`, execution.stderr.trim()].filter(Boolean),
    };
  }
}

function doctorCase(testCase, repository) {
  const argv = [
    binary,
    "doctor",
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
  const { execution, result, errors } = executeJson(argv, repository);
  if (!result) {
    return { errors };
  }
  const report = result.artifacts?.find(
    (artifact) => artifact.type === "migration-readiness",
  )?.content;
  requireEqual(errors, "doctor exit code", execution.exitCode, testCase.expect.executable ? 0 : 3);
  requireEqual(
    errors,
    "doctor status",
    result.status,
    testCase.expect.executable ? "completed" : "blocked",
  );
  requireEqual(errors, "doctor source", report?.source, testCase.expect.source);
  requireEqual(errors, "doctor target", report?.target, testCase.target);
  requireEqual(errors, "doctor verdict", report?.verdict, testCase.expect.doctorVerdict);
  requireEqual(errors, "doctor read-only", report?.readOnly, true);
  requireEqual(errors, "doctor migration available", report?.migrationAvailable, testCase.expect.executable);
  requireEqual(errors, "doctor plan identifier", result.planId, null);
  requireEqual(errors, "doctor run identifier", result.runId, null);
  if (result.artifacts?.some((artifact) => artifact.type === "package-manager-plan")) {
    errors.push("doctor exposed a package-manager plan artifact");
  }
  for (const [classification, expected] of Object.entries(testCase.expect.capabilities)) {
    requireEqual(
      errors,
      `doctor capabilities.${classification}`,
      report?.capabilities?.[classification],
      expected,
    );
  }
  const counts = diagnosticCounts(result.diagnostics);
  for (const code of testCase.expect.diagnosticCodes) {
    if (!counts[code]) {
      errors.push(`doctor diagnostic code ${code} was not reported`);
    }
  }
  return { report, counts, errors };
}

const matrixCache = new Map();

function doctorMatrix(testCase, repository) {
  const cacheKey = `${repository}:${testCase.acceptLossy === true}`;
  if (matrixCache.has(cacheKey)) {
    const cached = matrixCache.get(cacheKey);
    return { ...cached, errors: [...cached.errors] };
  }
  const argv = [
    binary,
    "doctor",
    "--cwd",
    repository,
    "--json",
    "--no-color",
    "--non-interactive",
  ];
  if (testCase.acceptLossy) {
    argv.push("--accept-lossy");
  }
  const { execution, result, errors } = executeJson(argv, repository);
  if (!result) {
    const failed = { errors };
    matrixCache.set(cacheKey, failed);
    return failed;
  }
  const matrix = result.artifacts?.find(
    (artifact) => artifact.type === "migration-readiness-matrix",
  )?.content;
  requireEqual(errors, "matrix doctor exit code", execution.exitCode, 0);
  requireEqual(errors, "matrix doctor status", result.status, "completed");
  requireEqual(errors, "matrix doctor source", matrix?.source, testCase.expect.source);
  requireEqual(errors, "matrix doctor read-only", matrix?.readOnly, true);
  requireEqual(errors, "matrix doctor targets", matrix?.summary?.targets, 7);
  requireEqual(errors, "matrix doctor plan identifier", result.planId, null);
  requireEqual(errors, "matrix doctor run identifier", result.runId, null);
  if (result.artifacts?.some((artifact) => artifact.type === "package-manager-plan")) {
    errors.push("matrix doctor exposed a package-manager plan artifact");
  }
  const value = { matrix, errors };
  matrixCache.set(cacheKey, value);
  return { ...value, errors: [...errors] };
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
  const doctor = doctorCase(testCase, repository);
  const matrix = doctorMatrix(testCase, repository);
  const { execution, result, errors } = executeJson(argv, repository);
  errors.push(...doctor.errors);
  errors.push(...matrix.errors);
  if (!result) {
    return {
      id: testCase.id,
      passed: false,
      errors,
    };
  }

  const plan = result.artifacts?.find((artifact) => artifact.type === "package-manager-plan")?.content;
  const matrixReport = matrix.matrix?.reports?.find((report) => report.target === testCase.target);
  requireEqual(errors, "exit code", execution.exitCode, testCase.expect.status === "blocked" ? 3 : 0);
  requireEqual(errors, "status", result.status, testCase.expect.status);
  requireEqual(errors, "source", result.summary?.source, testCase.expect.source);
  requireEqual(errors, "target", result.summary?.target, testCase.target);
  requireEqual(errors, "executable", plan?.executable, testCase.expect.executable);
  requireEqual(errors, "matrix target verdict", matrixReport?.verdict, testCase.expect.doctorVerdict);
  requireEqual(errors, "matrix target report", matrixReport?.reportId, doctor.report?.reportId);
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
    doctorVerdict: doctor.report?.verdict,
    doctorReportId: doctor.report?.reportId,
    doctorDiagnosticCounts: doctor.counts,
    doctorMatrixId: matrix.matrix?.matrixId,
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
    process.stderr.write(`Assessing and planning ${testCase.id}\n`);
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
