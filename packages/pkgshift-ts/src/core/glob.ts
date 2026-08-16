function normalizePattern(pattern: string): string {
  let normalized = pattern.trim().replaceAll("\\", "/");
  while (normalized.startsWith("./")) {
    normalized = normalized.slice(2);
  }
  normalized = normalized.replace(/\/$/, "");
  if (normalized.endsWith("/package.json")) {
    normalized = normalized.slice(0, -"/package.json".length);
  }
  return normalized;
}

function escapeRegularExpression(character: string): string {
  return /[\\^$.[\]{}()+|]/.test(character) ? `\\${character}` : character;
}

export function globToRegularExpression(pattern: string): RegExp {
  const normalized = normalizePattern(pattern);
  let expression = "^";
  for (let index = 0; index < normalized.length; index += 1) {
    const character = normalized[index] ?? "";
    const next = normalized[index + 1];
    if (character === "*" && next === "*") {
      const following = normalized[index + 2];
      if (following === "/") {
        expression += "(?:.*/)?";
        index += 2;
      } else {
        expression += ".*";
        index += 1;
      }
    } else if (character === "*") {
      expression += "[^/]*";
    } else if (character === "?") {
      expression += "[^/]";
    } else {
      expression += escapeRegularExpression(character);
    }
  }
  expression += "$";
  return new RegExp(expression);
}

export function matchesWorkspacePatterns(
  directory: string,
  patterns: string[],
): boolean {
  const normalizedDirectory = directory.replaceAll("\\", "/").replace(/^\.\//, "");
  const positive = patterns.filter((pattern) => !pattern.trim().startsWith("!"));
  const negative = patterns
    .filter((pattern) => pattern.trim().startsWith("!"))
    .map((pattern) => pattern.trim().slice(1));
  if (positive.length === 0) {
    return false;
  }
  const included = positive.some((pattern) =>
    globToRegularExpression(pattern).test(normalizedDirectory),
  );
  if (!included) {
    return false;
  }
  return !negative.some((pattern) =>
    globToRegularExpression(pattern).test(normalizedDirectory),
  );
}

