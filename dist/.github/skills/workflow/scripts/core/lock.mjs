import fs from "node:fs";
import os from "node:os";

import { ensureParent } from "./fs.mjs";

const DEFAULT_RETRY_MS = 50;

function utcNow() {
  return new Date().toISOString();
}

function sleepMs(ms) {
  Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, ms);
}

function isProcessAlive(pid) {
  if (!Number.isInteger(pid) || pid <= 0) {
    return false;
  }

  try {
    process.kill(pid, 0);
    return true;
  } catch {
    return false;
  }
}

function readLockMetadata(lockPath) {
  try {
    return JSON.parse(fs.readFileSync(lockPath, "utf8"));
  } catch {
    return { malformed: true };
  }
}

export function inspectJsonFileLock(lockPath, options = {}) {
  let stats = null;
  try {
    stats = fs.statSync(lockPath);
  } catch (error) {
    if (error?.code === "ENOENT") {
      return {
        exists: false,
        stale: false,
        metadata: null,
      };
    }
    throw error;
  }

  const metadata = readLockMetadata(lockPath);
  if (metadata.malformed) {
    return {
      exists: true,
      stale: true,
      metadata,
      reason: "malformed",
    };
  }

  const sameHost = metadata.hostname === os.hostname();
  const alive = sameHost ? isProcessAlive(metadata.pid) : true;
  const staleByAge = Number.isFinite(options.staleMs) && Date.now() - stats.mtimeMs > options.staleMs;
  const stale = !alive || staleByAge;

  return {
    exists: true,
    stale,
    metadata,
    reason: staleByAge ? "stale_timeout" : sameHost ? (alive ? "active" : "dead_process") : "foreign_host",
  };
}

export function removeStaleJsonFileLock(lockPath, options = {}) {
  const details = inspectJsonFileLock(lockPath, options);
  if (!details.exists) {
    return false;
  }
  if (!details.stale) {
    throw new Error(`Lock at ${lockPath} is not stale and cannot be removed automatically.`);
  }
  fs.rmSync(lockPath, { force: true });
  return true;
}

export function acquireJsonFileLock(lockPath, options = {}) {
  ensureParent(lockPath);
  const retryMs = Number.isFinite(options.retryMs) ? options.retryMs : DEFAULT_RETRY_MS;
  const timeoutMs = Number.isFinite(options.timeoutMs) ? options.timeoutMs : 0;
  const deadline = Date.now() + timeoutMs;
  const metadata = {
    pid: process.pid,
    hostname: os.hostname(),
    started_at: utcNow(),
    command: typeof options.command === "string" ? options.command : "",
    owner: typeof options.owner === "string" ? options.owner : "",
  };

  while (true) {
    try {
      const fd = fs.openSync(lockPath, "wx");
      try {
        fs.writeFileSync(fd, `${JSON.stringify(metadata, null, 2)}\n`, "utf8");
      } finally {
        fs.closeSync(fd);
      }
      return {
        path: lockPath,
        metadata,
      };
    } catch (error) {
      if (error?.code !== "EEXIST") {
        throw error;
      }

      const details = inspectJsonFileLock(lockPath, { staleMs: options.staleMs });
      if (details.stale) {
        if (options.removeStale === false) {
          throw new Error(options.staleMessage || `Lock at ${lockPath} is stale.`);
        }
        fs.rmSync(lockPath, { force: true });
        continue;
      }

      if (Date.now() >= deadline) {
        const timeoutMessage =
          typeof options.timeoutMessage === "function" ? options.timeoutMessage(details) : options.timeoutMessage;
        throw new Error(timeoutMessage || `Timed out waiting for lock at ${lockPath}.`);
      }
      sleepMs(retryMs);
    }
  }
}

export function releaseJsonFileLock(lock) {
  if (!lock?.path) {
    return false;
  }
  try {
    fs.rmSync(lock.path, { force: true });
    return true;
  } catch (error) {
    if (error?.code === "ENOENT") {
      return false;
    }
    throw error;
  }
}
