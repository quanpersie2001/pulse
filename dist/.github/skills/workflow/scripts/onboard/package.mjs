import fs from "node:fs";
import path from "node:path";

import {
  getPluginRoot,
  getPulseEntrypointPath,
  getScriptDir,
} from "../pulse_package_paths.mjs";
import { resolveRepoRoot as resolveRepoRootFromPaths } from "../pulse_paths.mjs";

const SCRIPT_DIR = path.dirname(getScriptDir(import.meta.url));
const PLUGIN_ROOT = getPluginRoot(SCRIPT_DIR);
const PLUGIN_MANIFEST_PATH = path.join(PLUGIN_ROOT, ".codex-plugin", "plugin.json");
const PULSE_ENTRYPOINT_PATH = getPulseEntrypointPath(SCRIPT_DIR);

export const PULSE_COMMAND = `node ${JSON.stringify(PULSE_ENTRYPOINT_PATH)}`;

export function loadPluginVersion() {
  return JSON.parse(fs.readFileSync(PLUGIN_MANIFEST_PATH, "utf8")).version;
}

export function resolveRepoRoot(explicitRoot, env = process.env, cwd = process.cwd()) {
  return resolveRepoRootFromPaths({ explicitRoot, env, cwd });
}
