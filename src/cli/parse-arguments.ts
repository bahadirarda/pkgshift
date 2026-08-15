import { resolve } from "node:path";

export interface CliOptions {
  acceptLossy: boolean;
  approval: string | null;
  cwd: string;
  client: string;
  dryRun: boolean;
  help: boolean;
  json: boolean;
  installMode: string;
  noColor: boolean;
  nonInteractive: boolean;
  quiet: boolean;
  scope: string;
  stateDirectory: string | null;
  target: string | null;
  version: boolean;
}

export interface ParsedArguments {
  options: CliOptions;
  positional: string[];
  errors: string[];
}

export function parseArguments(
  argv: string[],
  defaultCwd: string,
): ParsedArguments {
  const options: CliOptions = {
    acceptLossy: false,
    approval: null,
    cwd: defaultCwd,
    client: "codex",
    dryRun: false,
    help: false,
    json: false,
    installMode: "copy",
    noColor: false,
    nonInteractive: false,
    quiet: false,
    scope: "project",
    stateDirectory: null,
    target: null,
    version: false,
  };
  const positional: string[] = [];
  const errors: string[] = [];

  for (let index = 0; index < argv.length; index += 1) {
    const token = argv[index];
    if (!token) {
      continue;
    }
    if (token === "--help" || token === "-h") {
      options.help = true;
    } else if (token === "--version" || token === "-V") {
      options.version = true;
    } else if (token === "--json") {
      options.json = true;
    } else if (token === "--no-color") {
      options.noColor = true;
    } else if (token === "--non-interactive") {
      options.nonInteractive = true;
    } else if (token === "--quiet") {
      options.quiet = true;
    } else if (token === "--accept-lossy") {
      options.acceptLossy = true;
    } else if (token === "--dry-run") {
      options.dryRun = true;
    } else if (
      token === "--cwd"
      || token === "--to"
      || token === "--state-dir"
      || token === "--approve"
      || token === "--scope"
      || token === "--mode"
      || token === "--client"
    ) {
      const value = argv[index + 1];
      if (!value || value.startsWith("--")) {
        errors.push(`${token} requires a value.`);
      } else {
        if (token === "--cwd") {
          options.cwd = value;
        } else if (token === "--state-dir") {
          options.stateDirectory = value;
        } else if (token === "--approve") {
          options.approval = value;
        } else if (token === "--scope") {
          options.scope = value;
        } else if (token === "--mode") {
          options.installMode = value;
        } else if (token === "--client") {
          options.client = value;
        } else {
          options.target = value;
        }
        index += 1;
      }
    } else if (token.startsWith("--")) {
      errors.push(`Unknown option: ${token}`);
    } else {
      positional.push(token);
    }
  }

  if (positional[0] === "pm" && positional[1] === "to" && positional[2]) {
    options.target = positional[2];
    positional.splice(0, 3, "plan", "package-manager");
  }

  if (positional[0] === "to" && positional[1]) {
    options.target = positional[1];
    positional.splice(0, 2, "to");
  }

  if (options.stateDirectory) {
    options.stateDirectory = resolve(options.cwd, options.stateDirectory);
  }

  return { options, positional, errors };
}
