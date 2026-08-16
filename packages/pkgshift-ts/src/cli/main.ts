import { createInterface } from "node:readline/promises";
import {
  executeCommand,
  type GuidedApprovalRequest,
} from "./commands.ts";
import { parseArguments } from "./parse-arguments.ts";
import { renderHuman } from "./report.ts";
import { SCHEMA_VERSION, type CommandExecution } from "../domain/models.ts";

export interface CliIo {
  stdout: { write(content: string): unknown };
  stderr: { write(content: string): unknown };
  requestApproval?: (request: GuidedApprovalRequest) => Promise<boolean>;
}

async function requestTerminalApproval(
  request: GuidedApprovalRequest,
  io: CliIo,
): Promise<boolean> {
  io.stdout.write([
    `Migration: ${request.source} -> ${request.target}`,
    `Plan: ${request.planId}`,
    `Files: ${request.files}`,
    `Operations: ${request.operations}`,
    `Warnings: ${request.warnings}`,
    `Lossy decisions: ${request.lossyDecisions}`,
    "",
  ].join("\n"));
  const readline = createInterface({
    input: process.stdin,
    output: process.stdout,
  });
  try {
    const answer = await readline.question("Apply this migration? [y/N] ");
    return /^(?:y|yes)$/i.test(answer.trim());
  } finally {
    readline.close();
  }
}

export async function runCli(
  argv: string[],
  io: CliIo = process,
  defaultCwd = process.cwd(),
): Promise<number> {
  const parsed = parseArguments(argv, defaultCwd);
  let execution: CommandExecution;
  try {
    const interactive = !parsed.options.json && !parsed.options.nonInteractive;
    const requestApproval = interactive
      ? io.requestApproval
        ?? (process.stdin.isTTY && process.stdout.isTTY
          ? (request: GuidedApprovalRequest) => requestTerminalApproval(request, io)
          : undefined)
      : undefined;
    execution = await executeCommand(parsed, {
      ...(requestApproval ? { requestApproval } : {}),
    });
  } catch {
    execution = {
      exitCode: 8,
      result: {
        schemaVersion: SCHEMA_VERSION,
        command: parsed.positional.join(" ") || "unknown",
        status: "failed",
        planId: null,
        runId: null,
        summary: { trustworthyResult: false },
        artifacts: [],
        diagnostics: [{
          code: "PKGSHIFT_INTERNAL_ERROR",
          severity: "error",
          summary: "An internal error prevented a trustworthy result.",
          blocking: true,
          remediation: ["Stop before mutation and report the failure for investigation."],
        }],
        nextActions: [],
      },
    };
  }
  const output = parsed.options.json
    ? `${JSON.stringify(execution.result, null, 2)}\n`
    : renderHuman(execution.result);
  io.stdout.write(output);
  return execution.exitCode;
}
