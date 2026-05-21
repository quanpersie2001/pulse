import fs from "node:fs";
import os from "node:os";

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

export function readLockMetadata(lockPath) {
  if (!fs.existsSync(lockPath)) {
    return null;
  }

  try {
    return JSON.parse(fs.readFileSync(lockPath, "utf8"));
  } catch {
    return { malformed: true };
  }
}

export function inspectLock(lockPath) {
  const metadata = readLockMetadata(lockPath);
  if (!metadata) {
    return {
      exists: false,
      stale: false,
      metadata: null,
    };
  }

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
  return {
    exists: true,
    stale: !alive,
    metadata,
    reason: sameHost ? (alive ? "active" : "dead_process") : "foreign_host",
  };
}

export function acquireWriteLock(lockPath, command) {
  fs.mkdirSync(requireParent(lockPath), { recursive: true });
  const payload = {
    pid: process.pid,
    hostname: os.hostname(),
    started_at: new Date().toISOString(),
    command,
  };

  try {
    const fd = fs.openSync(lockPath, "wx");
    fs.writeFileSync(fd, `${JSON.stringify(payload, null, 2)}\n`, "utf8");
    fs.closeSync(fd);
    return {
      path: lockPath,
      metadata: payload,
    };
  } catch (error) {
    if (error?.code !== "EEXIST") {
      throw error;
    }

    const details = inspectLock(lockPath);
    if (details.stale) {
      throw new Error(
        `Workgraph lock exists but is stale at ${lockPath}. Run pulse.mjs workgraph doctor --fix to clear it.`,
      );
    }

    throw new Error(`Workgraph lock is active at ${lockPath}. Details: ${JSON.stringify(details.metadata)}`);
  }
}

export function releaseWriteLock(lock) {
  if (!lock?.path) {
    return false;
  }
  if (!fs.existsSync(lock.path)) {
    return false;
  }
  fs.unlinkSync(lock.path);
  return true;
}

export function removeStaleLock(lockPath) {
  const details = inspectLock(lockPath);
  if (!details.exists) {
    return false;
  }
  if (!details.stale) {
    throw new Error(`Lock at ${lockPath} is not stale and cannot be removed automatically.`);
  }
  fs.unlinkSync(lockPath);
  return true;
}

function requireParent(filePath) {
  return filePath.slice(0, Math.max(filePath.lastIndexOf("/"), filePath.lastIndexOf("\\")));
}
