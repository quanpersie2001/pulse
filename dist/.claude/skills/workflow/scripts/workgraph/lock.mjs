import {
  acquireJsonFileLock,
  inspectJsonFileLock,
  releaseJsonFileLock,
  removeStaleJsonFileLock,
} from "../core/lock.mjs";

export function inspectLock(lockPath) {
  return inspectJsonFileLock(lockPath);
}

export function acquireWriteLock(lockPath, command) {
  return acquireJsonFileLock(lockPath, {
    command,
    removeStale: false,
    staleMessage: `Workgraph lock exists but is stale at ${lockPath}. Run pulse.mjs workgraph doctor --fix to clear it.`,
    timeoutMessage: (details) => `Workgraph lock is active at ${lockPath}. Details: ${JSON.stringify(details.metadata)}`,
  });
}

export function releaseWriteLock(lock) {
  return releaseJsonFileLock(lock);
}

export function removeStaleLock(lockPath) {
  return removeStaleJsonFileLock(lockPath);
}
