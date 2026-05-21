import path from "node:path";
import { execFileSync } from "node:child_process";

export function resolveRepoRoot({
  explicitRoot,
  env = process.env,
  cwd = process.cwd(),
} = {}) {
  if (explicitRoot) {
    return path.resolve(explicitRoot);
  }

  if (env && typeof env.PULSE_REPO_ROOT === "string" && env.PULSE_REPO_ROOT.trim()) {
    return path.resolve(env.PULSE_REPO_ROOT.trim());
  }

  const resolvedCwd = path.resolve(cwd);
  try {
    const stdout = execFileSync("git", ["rev-parse", "--show-toplevel"], {
      cwd: resolvedCwd,
      encoding: "utf8",
      stdio: ["ignore", "pipe", "ignore"],
    });
    return path.resolve(stdout.trim());
  } catch {
    return resolvedCwd;
  }
}

export function getPulsePaths(repoRoot) {
  return {
    onboarding: path.join(repoRoot, ".pulse", "runtime", "onboarding.json"),
    toolingStatus: path.join(repoRoot, ".pulse", "runtime", "tooling-status.json"),
    stateJson: path.join(repoRoot, ".pulse", "runtime", "state.json"),
    stateMarkdown: path.join(repoRoot, ".pulse", "runtime", "STATE.md"),
    reservations: path.join(repoRoot, ".pulse", "runtime", "reservations.json"),
    handoffManifest: path.join(repoRoot, ".pulse", "runtime", "handoffs", "manifest.json"),
    memoryRoot: path.join(repoRoot, ".pulse", "memory"),
    memoryLearnings: path.join(repoRoot, ".pulse", "memory", "learnings"),
    memoryCorrections: path.join(repoRoot, ".pulse", "memory", "corrections"),
    memoryRatchet: path.join(repoRoot, ".pulse", "memory", "ratchet"),
    agents: path.join(repoRoot, "AGENTS.md"),
    criticalPatterns: path.join(repoRoot, ".pulse", "memory", "critical-patterns.md"),
  };
}

export function relativePosix(repoRoot, filePath) {
  return path.relative(repoRoot, filePath).split(path.sep).join(path.posix.sep);
}

export function isSafeSessionRelativePath(relativePath) {
  const candidate = String(relativePath || "").replace(/\\/g, "/").trim();
  if (!candidate || candidate.startsWith("/") || candidate.includes("..") || candidate.includes("%2e%2e") || candidate.includes("%2E%2E")) {
    return false;
  }
  const normalized = path.posix.normalize(candidate);
  if (normalized !== candidate) {
    return false;
  }
  return (
    normalized === "AGENTS.md" ||
    normalized.startsWith(".pulse/runtime/handoffs/") ||
    normalized.startsWith(".pulse/memory/") ||
    normalized.startsWith("works/") ||
    normalized.startsWith("docs/")
  );
}

export function resolveSafeRepoRelativePath(repoRoot, relativePath) {
  const candidate = String(relativePath || "").replace(/\\/g, "/").trim();
  if (!isSafeSessionRelativePath(candidate)) {
    return null;
  }
  const absolute = path.resolve(repoRoot, ...candidate.split("/"));
  const root = path.resolve(repoRoot);
  if (absolute !== root && !absolute.startsWith(`${root}${path.sep}`)) {
    return null;
  }
  return { relative: candidate, absolute };
}
