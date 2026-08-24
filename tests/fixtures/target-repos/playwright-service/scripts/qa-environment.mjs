#!/usr/bin/env node

import { createHash } from "node:crypto";
import { execFileSync, spawn } from "node:child_process";
import { existsSync } from "node:fs";
import { mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { resolve } from "node:path";

const [phase, inputPath] = process.argv.slice(2);
if (!phase || !inputPath) {
  throw new Error("qa-environment requires phase and Pulse input path");
}

const fixtureRevision = "playwright-service-1";
const baseURL = "http://127.0.0.1:4173";
const runtime = resolve(".pulse/runtime/real-browser");
const statePath = resolve(runtime, "deployment.json");
const htmlPath = resolve(runtime, "index.html");
const cleanupLog = resolve(runtime, "cleanup.log");
const mismatchMarker = resolve(".pulse/runtime/force-build-mismatch");
const input = JSON.parse(await readFile(inputPath, "utf8"));

function emit(identity, observation) {
  process.stdout.write(
    JSON.stringify({
      schema_version: 1,
      environment_instance_id: identity.environment_instance_id,
      source_commit: identity.source_commit,
      fixture_revision: fixtureRevision,
      deployment: identity.deployment,
      observations: [observation],
    }),
  );
}

async function readState() {
  return JSON.parse(await readFile(statePath, "utf8"));
}

async function terminate(pid) {
  if (!Number.isInteger(pid) || pid <= 1) return;
  try {
    process.kill(-pid, "SIGTERM");
  } catch {
    try {
      process.kill(pid, "SIGTERM");
    } catch {
      return;
    }
  }
  for (let attempt = 0; attempt < 50; attempt += 1) {
    await new Promise((resolveWait) => setTimeout(resolveWait, 20));
    try {
      process.kill(pid, 0);
    } catch {
      return;
    }
  }
  throw new Error(`deployment process ${pid} did not stop`);
}

async function fetchJson(path, options) {
  let lastError;
  for (let attempt = 0; attempt < 50; attempt += 1) {
    try {
      const response = await fetch(`${baseURL}${path}`, options);
      if (!response.ok) throw new Error(`HTTP ${response.status}`);
      return await response.json();
    } catch (error) {
      lastError = error;
      await new Promise((resolveWait) => setTimeout(resolveWait, 40));
    }
  }
  throw lastError;
}

if (phase === "start") {
  await mkdir(runtime, { recursive: true });
  if (existsSync(statePath)) {
    await terminate((await readState()).pid);
    await rm(statePath, { force: true });
  }
  const sourceCommit = execFileSync("git", ["rev-parse", "HEAD"], {
    encoding: "utf8",
  }).trim();
  const template = await readFile(resolve("app/index.template.html"), "utf8");
  const buildId = `sha256:${createHash("sha256")
    .update(sourceCommit)
    .update("\0")
    .update(template)
    .digest("hex")}`;
  const deploymentId = `local-${buildId.slice(7, 23)}`;
  const actualIdentity = {
    environment_instance_id: `env-${deploymentId}`,
    source_commit: sourceCommit,
    deployment: {
      build_id: buildId,
      deployment_id: deploymentId,
      base_url: baseURL,
    },
  };
  const reportedIdentity = structuredClone(actualIdentity);
  if (existsSync(mismatchMarker)) {
    reportedIdentity.deployment.build_id = `sha256:${"0".repeat(64)}`;
  }
  const html = template
    .replaceAll("__SOURCE_COMMIT__", sourceCommit)
    .replaceAll("__BUILD_ID__", buildId)
    .replaceAll("__DEPLOYMENT_ID__", deploymentId);
  await writeFile(htmlPath, html);
  await writeFile(
    statePath,
    `${JSON.stringify({
      pid: null,
      reservation_count: 1,
      actual_identity: actualIdentity,
      reported_identity: reportedIdentity,
    })}\n`,
  );
  const child = spawn(process.execPath, [resolve("scripts/qa-server.mjs"), statePath, htmlPath], {
    detached: true,
    stdio: "ignore",
  });
  child.unref();
  const current = await readState();
  current.pid = child.pid;
  await writeFile(statePath, `${JSON.stringify(current)}\n`);
  emit(reportedIdentity, "candidate source built and deployment started");
} else if (phase === "healthcheck") {
  const current = await readState();
  const observed = await fetchJson("/health");
  if (
    observed.source_commit !== input.source_commit ||
    observed.deployment.build_id !== current.reported_identity.deployment.build_id ||
    observed.deployment.deployment_id !== current.reported_identity.deployment.deployment_id
  ) {
    throw new Error("source/build/deployment mismatch during healthcheck");
  }
  emit(current.reported_identity, "exact deployment healthcheck passed");
} else if (phase === "reset") {
  const current = await readState();
  const observed = await fetchJson("/reset", { method: "POST" });
  if (observed.reservation_count !== 1) {
    throw new Error("fixture reset did not restore one reservation");
  }
  emit(current.reported_identity, "deployment fixture reset passed");
} else if (phase === "cleanup") {
  const current = await readState();
  await terminate(current.pid);
  await writeFile(
    cleanupLog,
    `${current.reported_identity.deployment.deployment_id}\n`,
    { flag: "a" },
  );
  emit(current.reported_identity, "exact deployment cleanup passed");
} else {
  throw new Error(`unsupported lifecycle phase ${phase}`);
}
