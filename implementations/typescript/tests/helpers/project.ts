import { mkdtemp, rm, writeFile, mkdir } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";

const temporaryProjects: string[] = [];

export async function createProject(
  files: Record<string, string>,
): Promise<string> {
  const root = await mkdtemp(join(tmpdir(), "pkgshift-test-"));
  temporaryProjects.push(root);
  for (const [path, content] of Object.entries(files)) {
    const absolutePath = join(root, path);
    await mkdir(dirname(absolutePath), { recursive: true });
    await writeFile(absolutePath, content, "utf8");
  }
  return root;
}

export async function removeTemporaryProjects(): Promise<void> {
  const projects = temporaryProjects.splice(0);
  await Promise.all(projects.map((root) => rm(root, { recursive: true, force: true })));
}

