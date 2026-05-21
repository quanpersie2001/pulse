import path from "node:path";
import { spawnSync } from "node:child_process";

import { REPO_ROOT } from "./fixtures.mjs";

export const PULSE_PATH = path.join(REPO_ROOT, "skills", "workflow", "scripts", "pulse.mjs");

export function spawnPulse(args = [], options = {}) {
  return spawnSync(process.execPath, [PULSE_PATH, ...args], {
    cwd: options.cwd ?? REPO_ROOT,
    env: { ...process.env, ...(options.env ?? {}) },
    encoding: "utf8",
  });
}

export function parseJsonOutput(result) {
  return JSON.parse(result.stdout);
}
