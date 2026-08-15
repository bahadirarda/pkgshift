import type { CommandResult } from "../domain/models.ts";

export function renderHuman(result: CommandResult): string {
  const lines = [`pkgshift: ${result.command}`, `Status: ${result.status}`];
  if (result.planId) {
    lines.push(`Plan: ${result.planId}`);
  }
  for (const [key, value] of Object.entries(result.summary)) {
    if (Array.isArray(value)) {
      lines.push(`${key}:`);
      for (const item of value) {
        lines.push(`  ${String(item)}`);
      }
    } else {
      lines.push(`${key}: ${String(value)}`);
    }
  }
  for (const diagnostic of result.diagnostics) {
    lines.push(`${diagnostic.severity.toUpperCase()} ${diagnostic.code}: ${diagnostic.summary}`);
  }
  return `${lines.join("\n")}\n`;
}

