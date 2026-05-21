import fs from "node:fs";
import path from "node:path";

import {
  getScriptDir,
  getWorkflowSkillDir,
} from "../pulse_package_paths.mjs";
import {
  ensureWorkgraphFilesystem,
  getWorkgraphPaths,
  loadItems,
  writeViews,
} from "../workgraph_store.mjs";

const SCRIPT_DIR = path.dirname(getScriptDir(import.meta.url));
const WORKFLOW_SKILL_DIR = getWorkflowSkillDir(SCRIPT_DIR);
const HARNESS_BACKLOG_TEMPLATE_PATH = path.join(WORKFLOW_SKILL_DIR, "templates", "HARNESS_BACKLOG.md");

export function supportAssetsNeedUpdate(repoRoot) {
  const harnessBacklogTarget = path.join(repoRoot, ".pulse", "harness", "HARNESS_BACKLOG.md");
  const source = fs.readFileSync(HARNESS_BACKLOG_TEMPLATE_PATH, "utf8");
  return !fs.existsSync(harnessBacklogTarget) || fs.readFileSync(harnessBacklogTarget, "utf8") !== source;
}

export function writeSupportAssets(repoRoot) {
  const written = [];
  const harnessDir = path.join(repoRoot, ".pulse", "harness");
  fs.mkdirSync(harnessDir, { recursive: true });
  const harnessBacklogTarget = path.join(harnessDir, "HARNESS_BACKLOG.md");
  fs.copyFileSync(HARNESS_BACKLOG_TEMPLATE_PATH, harnessBacklogTarget);
  fs.chmodSync(harnessBacklogTarget, 0o644);
  written.push(path.relative(repoRoot, harnessBacklogTarget));

  return written;
}

export function initializeWorkgraphFilesystem(repoRoot) {
  ensureWorkgraphFilesystem(repoRoot, { syncSchema: true });
  writeViews(repoRoot, loadItems(repoRoot));
  return getWorkgraphPaths(repoRoot);
}
