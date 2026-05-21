import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { execFileSync } from "node:child_process";

export function mkTempRepo(prefix = "pulse-test-") {
  return fs.mkdtempSync(path.join(os.tmpdir(), prefix));
}

export function cleanupTempRepo(root) {
  fs.rmSync(root, { recursive: true, force: true });
}

export function initGitRepo(root) {
  execFileSync("git", ["init", "-q"], { cwd: root, stdio: "ignore" });
  return root;
}
