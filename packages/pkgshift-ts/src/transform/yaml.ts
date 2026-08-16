function scalar(value: unknown): string {
  if (value === null) return "null";
  if (typeof value === "boolean" || typeof value === "number") {
    return String(value);
  }
  return JSON.stringify(String(value));
}

function key(value: string): string {
  return /^[A-Za-z_][A-Za-z0-9_.-]*$/.test(value)
    ? value
    : JSON.stringify(value);
}

function render(value: unknown, depth: number): string[] {
  const prefix = "  ".repeat(depth);
  if (Array.isArray(value)) {
    if (value.length === 0) return [`${prefix}[]`];
    return value.flatMap((entry) => {
      if (entry && typeof entry === "object") {
        return [`${prefix}-`, ...render(entry, depth + 1)];
      }
      return [`${prefix}- ${scalar(entry)}`];
    });
  }
  if (value && typeof value === "object") {
    const entries = Object.entries(value as Record<string, unknown>);
    if (entries.length === 0) return [`${prefix}{}`];
    return entries.flatMap(([entryKey, entryValue]) => {
      if (entryValue && typeof entryValue === "object") {
        return [`${prefix}${key(entryKey)}:`, ...render(entryValue, depth + 1)];
      }
      return [`${prefix}${key(entryKey)}: ${scalar(entryValue)}`];
    });
  }
  return [`${prefix}${scalar(value)}`];
}

export function stringifyYaml(value: Record<string, unknown>): string {
  return `${render(value, 0).join("\n")}\n`;
}
