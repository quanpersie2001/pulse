#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const SCRIPT_PATH = fileURLToPath(import.meta.url);

function findRepoRoot(start) {
  let candidate = path.resolve(start || ".");
  while (true) {
    if (fs.existsSync(path.join(candidate, ".pulse", "onboarding.json"))) {
      return candidate;
    }
    if (fs.existsSync(path.join(candidate, ".git"))) {
      return candidate;
    }
    const parent = path.dirname(candidate);
    if (parent === candidate) {
      return candidate;
    }
    candidate = parent;
  }
}

async function readHookPayload(stream = process.stdin) {
  const chunks = [];
  for await (const chunk of stream) {
    chunks.push(chunk);
  }
  const raw = Buffer.concat(chunks).toString("utf8");
  return JSON.parse(raw || "{}");
}

async function loadSessionContext(repoRoot) {
  const modulePath = path.join(repoRoot, ".pulse", "scripts", "pulse_session_context.mjs");
  if (!fs.existsSync(modulePath)) {
    throw new Error("Pulse session context helper not found.");
  }

  return {
    includeBootstrapSkill: false,
    module: await import(pathToFileURL(modulePath).href),
  };
}

export async function main() {
  const payload = await readHookPayload();
  const repoRoot = findRepoRoot(payload.cwd || process.cwd());
  const { includeBootstrapSkill, module } = await loadSessionContext(repoRoot);
  const { buildPulseSessionStartContext } = module;
  const additionalContext = await buildPulseSessionStartContext(repoRoot, {
    includeBootstrapSkill,
    syncRuntimeArtifactsIfOnboarded: true,
  });

  process.stdout.write(
    JSON.stringify({
      hookSpecificOutput: {
        hookEventName: "SessionStart",
        additionalContext,
      },
    }),
  );
  return 0;
}

function isDirectExecution() {
  if (!process.argv[1]) {
    return false;
  }

  const argvPath = path.resolve(process.argv[1]);
  if (argvPath === SCRIPT_PATH) {
    return true;
  }

  try {
    return fs.realpathSync.native(argvPath) === fs.realpathSync.native(SCRIPT_PATH);
  } catch {
    return false;
  }
}

if (isDirectExecution()) {
  process.exitCode = await main();
}
