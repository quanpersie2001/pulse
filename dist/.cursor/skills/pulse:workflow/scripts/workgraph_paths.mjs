import fs from "node:fs";
import path from "node:path";

import { cloneItemRecord } from "./workgraph_model.mjs";

const POSIX = path.posix;

export function sanitizeSlug(input, fallback = "item") {
  const normalized = String(input || "")
    .normalize("NFKD")
    .replace(/[̀-ͯ]/g, "")
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .replace(/-{2,}/g, "-");

  return normalized || fallback;
}

export function assertSafeRelativeWorkPath(relativePath) {
  const candidate = String(relativePath || "").replace(/\\/g, "/");
  if (!candidate) {
    throw new Error("Path must not be empty.");
  }
  if (candidate.startsWith("/")) {
    throw new Error(`Absolute paths are not allowed: ${relativePath}`);
  }
  if (!candidate.startsWith("works/")) {
    throw new Error(`Workgraph paths must stay under works/: ${relativePath}`);
  }
  if (candidate.includes("..") || candidate.includes("%2e%2e") || candidate.includes("%2E%2E")) {
    throw new Error(`Path traversal is not allowed: ${relativePath}`);
  }

  const normalized = POSIX.normalize(candidate);
  if (normalized !== candidate) {
    throw new Error(`Path must already be normalized: ${relativePath}`);
  }

  return normalized;
}

export function toFilesystemPath(repoRoot, relativePath) {
  const safe = assertSafeRelativeWorkPath(relativePath);
  const absolute = path.resolve(repoRoot, ...safe.split("/"));
  const worksRoot = path.resolve(repoRoot, "works");

  if (absolute !== worksRoot && !absolute.startsWith(`${worksRoot}${path.sep}`)) {
    throw new Error(`Resolved path escapes works/: ${relativePath}`);
  }

  return absolute;
}

export function getItemDirectory(item) {
  return POSIX.dirname(assertSafeRelativeWorkPath(item.content_path));
}

function deriveItemDirectory(item, recordsById, memo) {
  if (memo.has(item.id)) {
    return memo.get(item.id);
  }

  let directory = "";
  if (item.kind === "EPIC") {
    directory = POSIX.join("works", "epics", `${item.id}-${item.slug}`);
  } else if (item.kind === "STORY") {
    const epic = recordsById.get(item.epic_id);
    if (!epic) {
      throw new Error(`Story ${item.id} references missing epic ${item.epic_id}.`);
    }
    directory = POSIX.join(deriveItemDirectory(epic, recordsById, memo), `${item.id}-${item.slug}`);
  } else if (item.kind === "TASK" || item.kind === "BUG") {
    const parent = recordsById.get(item.parent_id);
    if (!parent) {
      throw new Error(`Item ${item.id} references missing parent ${item.parent_id}.`);
    }

    if (parent.kind === "STORY") {
      directory = POSIX.join(
        deriveItemDirectory(parent, recordsById, memo),
        "tasks",
        `${item.id}-${item.slug}`,
      );
    } else {
      const epic = recordsById.get(item.epic_id);
      if (!epic) {
        throw new Error(`Item ${item.id} references missing epic ${item.epic_id}.`);
      }
      directory = POSIX.join(
        deriveItemDirectory(epic, recordsById, memo),
        "tasks",
        `${item.id}-${item.slug}`,
      );
    }
  } else {
    throw new Error(`Unsupported item kind: ${item.kind}`);
  }

  memo.set(item.id, directory);
  return directory;
}

export function deriveCanonicalPaths(item, recordsById) {
  const memo = new Map();
  const directory = deriveItemDirectory(item, recordsById, memo);
  const contentPath = POSIX.join(directory, "README.md");
  const verificationPath = item.kind === "TASK" || item.kind === "BUG"
    ? POSIX.join(directory, "verification.md")
    : null;

  return {
    content_path: assertSafeRelativeWorkPath(contentPath),
    verification_path: verificationPath ? assertSafeRelativeWorkPath(verificationPath) : null,
  };
}

export function applyCanonicalPaths(items) {
  const clones = (items || []).map((item) => cloneItemRecord(item));
  const recordsById = new Map(clones.map((item) => [item.id, item]));
  const memo = new Map();

  for (const item of clones) {
    const directory = deriveItemDirectory(item, recordsById, memo);
    item.content_path = assertSafeRelativeWorkPath(POSIX.join(directory, "README.md"));
    item.verification_path = item.kind === "TASK" || item.kind === "BUG"
      ? assertSafeRelativeWorkPath(POSIX.join(directory, "verification.md"))
      : null;
  }

  return clones;
}

export function moveItemDirectory(repoRoot, previousItem, nextItem) {
  const previousDir = getItemDirectory(previousItem);
  const nextDir = getItemDirectory(nextItem);
  if (previousDir === nextDir) {
    return false;
  }

  const previousAbsolute = toFilesystemPath(repoRoot, previousDir);
  const nextAbsolute = toFilesystemPath(repoRoot, nextDir);
  if (!fs.existsSync(previousAbsolute)) {
    return false;
  }
  if (fs.existsSync(nextAbsolute)) {
    throw new Error(`Refusing to overwrite existing work directory: ${nextDir}`);
  }

  fs.mkdirSync(path.dirname(nextAbsolute), { recursive: true });
  fs.renameSync(previousAbsolute, nextAbsolute);
  return true;
}
