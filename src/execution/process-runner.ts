import { redactSensitiveText } from "../core/redaction.ts";

export interface ProcessExecutionRecord {
  argv: string[];
  exitCode: number | null;
  signal: string | null;
  timedOut: boolean;
  durationMs: number;
  stdout: string;
  stderr: string;
}

export interface ProcessRunner {
  run(argv: string[], cwd: string, timeoutMs?: number): Promise<ProcessExecutionRecord>;
}

function boundedOutput(value: string): string {
  let redacted = redactSensitiveText(value);
  for (const [name, secret] of Object.entries(process.env)) {
    if (
      secret
      && secret.length >= 6
      && /(?:TOKEN|KEY|SECRET|PASSWORD|PASS|AUTH|COOKIE|CREDENTIAL)/i.test(name)
    ) {
      redacted = redacted.replaceAll(secret, "<redacted>");
    }
  }
  return redacted.length > 8_000
    ? `${redacted.slice(0, 8_000)}\n[output truncated]`
    : redacted;
}

export class BunProcessRunner implements ProcessRunner {
  async run(
    argv: string[],
    cwd: string,
    timeoutMs = 300_000,
  ): Promise<ProcessExecutionRecord> {
    const started = performance.now();
    let process: ReturnType<typeof Bun.spawn>;
    try {
      process = Bun.spawn(argv, {
        cwd,
        env: processEnv(),
        stdin: "ignore",
        stdout: "pipe",
        stderr: "pipe",
      });
    } catch (error) {
      return {
        argv,
        exitCode: null,
        signal: null,
        timedOut: false,
        durationMs: Math.round(performance.now() - started),
        stdout: "",
        stderr: boundedOutput(error instanceof Error ? error.message : "Process launch failed."),
      };
    }
    let timedOut = false;
    let forceKillTimer: ReturnType<typeof setTimeout> | null = null;
    const timer = setTimeout(() => {
      timedOut = true;
      process.kill("SIGTERM");
      forceKillTimer = setTimeout(() => process.kill("SIGKILL"), 5_000);
    }, timeoutMs);
    const stdoutPromise = process.stdout instanceof ReadableStream
      ? new Response(process.stdout).text()
      : Promise.resolve("");
    const stderrPromise = process.stderr instanceof ReadableStream
      ? new Response(process.stderr).text()
      : Promise.resolve("");
    const exitCode = await process.exited;
    clearTimeout(timer);
    if (forceKillTimer) clearTimeout(forceKillTimer);
    const [stdout, stderr] = await Promise.all([stdoutPromise, stderrPromise]);
    return {
      argv,
      exitCode,
      signal: process.signalCode === null ? null : String(process.signalCode),
      timedOut,
      durationMs: Math.round(performance.now() - started),
      stdout: boundedOutput(stdout),
      stderr: boundedOutput(stderr),
    };
  }
}

function processEnv(): Record<string, string | undefined> {
  return { ...process.env };
}
