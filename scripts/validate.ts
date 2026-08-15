import { stat } from "node:fs/promises";
import { basename, dirname, join, resolve } from "node:path";

const errors: string[] = [];

async function filesMatching(pattern: string): Promise<string[]> {
  return (await Array.fromAsync(
    new Bun.Glob(pattern).scan({ cwd: ".", onlyFiles: true }),
  )).sort();
}

function parseFrontmatter(
  path: string,
  content: string,
): Record<string, unknown> | null {
  const match = content.match(/^---\n([\s\S]*?)\n---(?:\n|$)/);
  if (!match?.[1]) {
    errors.push(`${path}: missing or malformed YAML frontmatter`);
    return null;
  }
  try {
    const parsed: unknown = Bun.YAML.parse(match[1]);
    if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
      errors.push(`${path}: frontmatter must be a YAML mapping`);
      return null;
    }
    return parsed as Record<string, unknown>;
  } catch (error) {
    const message = error instanceof Error ? error.message : "unknown YAML error";
    errors.push(`${path}: ${message}`);
    return null;
  }
}

async function targetExists(path: string): Promise<boolean> {
  try {
    await stat(path);
    return true;
  } catch {
    return false;
  }
}

async function validateLinks(
  path: string,
  content: string,
  bundleRoot: string | null,
): Promise<void> {
  for (const match of content.matchAll(/\[[^\]]*\]\(([^)]+)\)/g)) {
    const rawTarget = match[1]?.split("#")[0];
    if (!rawTarget || /^(https?:|mailto:)/.test(rawTarget)) {
      continue;
    }
    const target = rawTarget.startsWith("/") && bundleRoot
      ? join(bundleRoot, rawTarget)
      : resolve(dirname(path), rawTarget);
    if (!(await targetExists(target))) {
      errors.push(`${path}: broken link ${rawTarget}`);
    }
  }
}

async function validateOkf(): Promise<number> {
  const paths = await filesMatching("docs/**/*.md");
  for (const path of paths) {
    const content = await Bun.file(path).text();
    const filename = basename(path);
    if (filename === "index.md") {
      if (path === "docs/index.md") {
        const frontmatter = parseFrontmatter(path, content);
        if (frontmatter) {
          const keys = Object.keys(frontmatter);
          if (frontmatter.okf_version !== "0.2" || keys.some((key) => key !== "okf_version")) {
            errors.push(`${path}: root index may contain only okf_version 0.2`);
          }
        }
      } else if (content.startsWith("---")) {
        errors.push(`${path}: nested index files must not contain frontmatter`);
      }
    } else if (filename === "log.md") {
      if (content.startsWith("---")) {
        errors.push(`${path}: log files must not contain frontmatter`);
      }
      const headings = [...content.matchAll(/^## (.+)$/gm)].map((match) => match[1]);
      if (headings.some((heading) => !/^\d{4}-\d{2}-\d{2}$/.test(heading ?? ""))) {
        errors.push(`${path}: log date headings must use YYYY-MM-DD`);
      }
    } else {
      const frontmatter = parseFrontmatter(path, content);
      if (frontmatter) {
        if (typeof frontmatter.type !== "string" || !frontmatter.type.trim()) {
          errors.push(`${path}: type must be a non-empty string`);
        }
        if (
          frontmatter.status !== undefined
          && !["draft", "stable", "deprecated"].includes(String(frontmatter.status))
        ) {
          errors.push(`${path}: unsupported lifecycle status`);
        }
        if (frontmatter.sources !== undefined) {
          if (!Array.isArray(frontmatter.sources)) {
            errors.push(`${path}: sources must be a YAML list`);
          } else {
            for (const source of frontmatter.sources) {
              if (
                !source
                || typeof source !== "object"
                || typeof (source as Record<string, unknown>).resource !== "string"
                || !(source as Record<string, string>).resource.trim()
              ) {
                errors.push(`${path}: every source entry requires a resource`);
              }
            }
          }
        }
      }
      const body = content.replace(/^---\n[\s\S]*?\n---\n?/, "");
      if (!/^# /m.test(body)) {
        errors.push(`${path}: concept body requires a top-level heading`);
      }
    }
    await validateLinks(path, content, "docs");
  }
  return paths.length;
}

async function validateSkill(): Promise<void> {
  const path = "skills/pkgshift/SKILL.md";
  const content = await Bun.file(path).text();
  const frontmatter = parseFrontmatter(path, content);
  if (frontmatter) {
    const keys = Object.keys(frontmatter);
    if (keys.some((key) => !["name", "description"].includes(key))) {
      errors.push(`${path}: portable frontmatter may contain only name and description`);
    }
    const name = frontmatter.name;
    if (
      typeof name !== "string"
      || !/^[a-z0-9-]+$/.test(name)
      || name.startsWith("-")
      || name.endsWith("-")
      || name.includes("--")
      || name.length > 64
    ) {
      errors.push(`${path}: invalid skill name`);
    }
    const description = frontmatter.description;
    if (
      typeof description !== "string"
      || !description.trim()
      || description.length > 1024
      || /[<>]/.test(description)
    ) {
      errors.push(`${path}: invalid skill description`);
    }
  }
  if (content.split("\n").length > 500) {
    errors.push(`${path}: SKILL.md must remain below 500 lines`);
  }
  await validateLinks(path, content, null);

  const openAiPath = "skills/pkgshift/agents/openai.yaml";
  const metadata = Bun.YAML.parse(await Bun.file(openAiPath).text()) as {
    interface?: {
      short_description?: string;
      default_prompt?: string;
    };
  };
  const shortDescription = metadata.interface?.short_description ?? "";
  if (shortDescription.length < 25 || shortDescription.length > 64) {
    errors.push(`${openAiPath}: short_description must contain 25 to 64 characters`);
  }
  if (!metadata.interface?.default_prompt?.includes("$pkgshift")) {
    errors.push(`${openAiPath}: default_prompt must mention $pkgshift`);
  }
}

async function validateEnglishOnly(): Promise<void> {
  const roots = ["AGENTS.md", "README.md"];
  const patterns = [
    "docs/**/*.md",
    "scripts/**/*.ts",
    "skills/**/*.{md,yaml,yml}",
    "src/**/*.ts",
    "tests/**/*.ts",
    "*.json",
  ];
  for (const pattern of patterns) {
    roots.push(...await filesMatching(pattern));
  }
  for (const path of [...new Set(roots)]) {
    const content = await Bun.file(path).text();
    if (/[\u00e7\u011f\u0131\u00f6\u015f\u00fc\u00c7\u011e\u0130\u00d6\u015e\u00dc]/.test(content)) {
      errors.push(`${path}: Turkish-specific characters violate the English-only rule`);
    }
  }
}

const conceptCount = await validateOkf();
await validateSkill();
await validateLinks("README.md", await Bun.file("README.md").text(), null);
await validateEnglishOnly();

if (errors.length > 0) {
  console.error(errors.join("\n"));
  process.exit(1);
}

console.log(`Validated ${conceptCount} OKF Markdown files, the portable Agent Skill, internal links, and English-only content.`);
