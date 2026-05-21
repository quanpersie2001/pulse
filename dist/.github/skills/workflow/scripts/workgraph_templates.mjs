import fs from "node:fs";
import path from "node:path";

import { getScriptDir, getWorkflowWorksTemplateDir } from "./pulse_package_paths.mjs";
import { moveItemDirectory, toFilesystemPath } from "./workgraph_paths.mjs";

const TEMPLATE_DIR = getWorkflowWorksTemplateDir(getScriptDir(import.meta.url));

function resolveTemplateDir() {
  if (!fs.existsSync(TEMPLATE_DIR)) {
    throw new Error(`Unable to locate work templates at ${TEMPLATE_DIR}.`);
  }
  return TEMPLATE_DIR;
}

function readTemplateFile(name) {
  return fs.readFileSync(path.join(resolveTemplateDir(), name), "utf8");
}

function renderTemplate(template, item) {
  return template
    .replaceAll("{{id}}", item.id)
    .replaceAll("{{title}}", item.title)
    .replaceAll("{{slug}}", item.slug);
}

function readmeTemplateForKind(kind) {
  switch (kind) {
    case "EPIC":
      return readTemplateFile("epic-README.md");
    case "STORY":
      return readTemplateFile("story-README.md");
    case "TASK":
    case "BUG":
      return readTemplateFile("task-README.md");
    default:
      throw new Error(`Unsupported item kind: ${kind}`);
  }
}

function writeFileIfMissing(filePath, content) {
  if (fs.existsSync(filePath)) {
    return false;
  }
  fs.mkdirSync(path.dirname(filePath), { recursive: true });
  fs.writeFileSync(filePath, content, "utf8");
  return true;
}

export function scaffoldItemFiles(repoRoot, item) {
  const pendingWrites = [
    {
      absolutePath: toFilesystemPath(repoRoot, item.content_path),
      relativePath: item.content_path,
      content: renderTemplate(readmeTemplateForKind(item.kind), item),
    },
  ];

  if (item.kind === "TASK" || item.kind === "BUG") {
    pendingWrites.push({
      absolutePath: toFilesystemPath(repoRoot, item.verification_path),
      relativePath: item.verification_path,
      content: renderTemplate(readTemplateFile("verification.md"), item),
    });
  }

  for (const file of pendingWrites) {
    if (fs.existsSync(file.absolutePath)) {
      const basename = path.basename(file.absolutePath);
      throw new Error(`Refusing to overwrite existing ${basename} for ${item.id} at ${file.relativePath}`);
    }
  }

  const created = [];
  try {
    for (const file of pendingWrites) {
      fs.mkdirSync(path.dirname(file.absolutePath), { recursive: true });
      fs.writeFileSync(file.absolutePath, file.content, { encoding: "utf8", flag: "wx" });
      created.push(file.absolutePath);
    }
  } catch (error) {
    for (const filePath of created.reverse()) {
      try {
        fs.unlinkSync(filePath);
      } catch {
        // Best effort cleanup only.
      }
    }
    throw error;
  }

  return pendingWrites.map((file) => file.relativePath);
}

export function ensureItemFiles(repoRoot, item) {
  const written = [];
  if (writeFileIfMissing(toFilesystemPath(repoRoot, item.content_path), renderTemplate(readmeTemplateForKind(item.kind), item))) {
    written.push(item.content_path);
  }

  if (item.kind === "TASK" || item.kind === "BUG") {
    if (
      writeFileIfMissing(
        toFilesystemPath(repoRoot, item.verification_path),
        renderTemplate(readTemplateFile("verification.md"), item),
      )
    ) {
      written.push(item.verification_path);
    }
  }

  return written;
}

export function moveItemContent(repoRoot, previousItem, nextItem) {
  return moveItemDirectory(repoRoot, previousItem, nextItem);
}
