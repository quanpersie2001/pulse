/**
 * Purpose: Persist and enforce runtime reservation records and lock discipline.
 * Caller/flow: Used by pulse_reservations CLI and status aggregation.
 * Reads/Writes: Reads/writes .pulse/runtime/reservations.json and a sibling lock file.
 * CLI args: None (module API).
 * Ownership: Owns normalization, conflict detection, overlap checks, and atomic mutation behavior.
 * Repo root rule: Caller passes repo root; store path is resolved via pulse_paths.mjs.
 */

import fs from "node:fs";
import path from "node:path";

import { getPulsePaths } from "./pulse_paths.mjs";

export const RESERVATION_SCHEMA_VERSION = "1.0";
const LOCK_RETRY_MS = 50;
const LOCK_TIMEOUT_MS = 2_000;
const DEFAULT_LOCK_STALE_MS = 30_000;
const WILDCARD_PATTERN = "**";

function utcNow() {
  return new Date().toISOString();
}

function sleepMs(ms) {
  Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, ms);
}

function normalizeSlashPath(value) {
  return String(value || "").replace(/\\/g, "/");
}

function stripLeadingDotSlash(value) {
  return normalizeSlashPath(value).replace(/^\.\/+/, "");
}

function hasGlobMagic(value) {
  return /[*?[\]{}]/.test(String(value || ""));
}

export function normalizeReservationPattern(repoRoot, value) {
  if (typeof value !== "string" || !value.trim()) {
    return "";
  }

  const trimmed = stripLeadingDotSlash(value.trim());
  if (path.isAbsolute(trimmed)) {
    const relativePath = path.relative(repoRoot, trimmed);
    return normalizeSlashPath(relativePath || ".");
  }
  return normalizeSlashPath(trimmed);
}

function escapeRegex(text) {
  return text.replace(/[|\\{}()[\]^$+?.]/g, "\\$&");
}

function globToRegExp(pattern) {
  const normalized = normalizeSlashPath(pattern);
  let regex = "^";

  for (let index = 0; index < normalized.length; index += 1) {
    const char = normalized[index];
    const next = normalized[index + 1];

    if (char === "*" && next === "*") {
      const afterNext = normalized[index + 2];
      if (afterNext === "/") {
        regex += "(?:.+/)?";
        index += 2;
        continue;
      }
      regex += ".*";
      index += 1;
      continue;
    }

    if (char === "*") {
      regex += "[^/]*";
      continue;
    }

    if (char === "?") {
      regex += "[^/]";
      continue;
    }

    regex += escapeRegex(char);
  }

  regex += "$";
  return new RegExp(regex);
}

function staticPrefix(pattern) {
  const normalized = normalizeSlashPath(pattern);
  const match = normalized.match(/^[^*?[\]{}]*/);
  return (match?.[0] || "").replace(/\/+$/, "");
}

export function reservationPatternsOverlap(leftPattern, rightPattern) {
  const left = stripLeadingDotSlash(leftPattern);
  const right = stripLeadingDotSlash(rightPattern);
  if (!left || !right) {
    return false;
  }

  if (left === WILDCARD_PATTERN || right === WILDCARD_PATTERN) {
    return true;
  }

  if (left === right) {
    return true;
  }

  const leftHasMagic = hasGlobMagic(left);
  const rightHasMagic = hasGlobMagic(right);

  if (!leftHasMagic && !rightHasMagic) {
    return left === right;
  }

  if (!leftHasMagic && rightHasMagic && globToRegExp(right).test(left)) {
    return true;
  }

  if (leftHasMagic && !rightHasMagic && globToRegExp(left).test(right)) {
    return true;
  }

  const leftPrefix = staticPrefix(left);
  const rightPrefix = staticPrefix(right);
  if (!leftPrefix || !rightPrefix) {
    return true;
  }

  return leftPrefix.startsWith(rightPrefix) || rightPrefix.startsWith(leftPrefix);
}

function ensureParent(filePath) {
  fs.mkdirSync(path.dirname(filePath), { recursive: true });
}

function buildEmptyReservationStore() {
  return {
    schema_version: RESERVATION_SCHEMA_VERSION,
    updated_at: utcNow(),
    reservations: [],
  };
}

function normalizeReservation(entry) {
  const createdAt =
    typeof entry?.created_at === "string" && entry.created_at ? entry.created_at : utcNow();
  const ttlSeconds =
    Number.isFinite(entry?.ttl_seconds) && entry.ttl_seconds > 0
      ? Math.floor(entry.ttl_seconds)
      : null;
  const expiresAt =
    typeof entry?.expires_at === "string" && entry.expires_at
      ? entry.expires_at
      : ttlSeconds
        ? new Date(Date.parse(createdAt) + ttlSeconds * 1000).toISOString()
        : null;

  return {
    id:
      typeof entry?.id === "string" && entry.id
        ? entry.id
        : `resv-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`,
    agent:
      typeof entry?.agent === "string" && entry.agent.trim() ? entry.agent.trim() : "unknown",
    item_id: typeof entry?.item_id === "string" ? entry.item_id.trim() : "",
    paths: Array.isArray(entry?.paths)
      ? entry.paths.filter((item) => typeof item === "string" && item.trim())
      : [],
    created_at: createdAt,
    updated_at:
      typeof entry?.updated_at === "string" && entry.updated_at ? entry.updated_at : createdAt,
    ttl_seconds: ttlSeconds,
    expires_at: expiresAt,
    status:
      typeof entry?.status === "string" && entry.status
        ? entry.status
        : expiresAt && Date.now() > Date.parse(expiresAt)
          ? "expired"
          : "active",
    released_at: typeof entry?.released_at === "string" ? entry.released_at : null,
    note: typeof entry?.note === "string" ? entry.note : "",
  };
}

function normalizeReservationStore(store) {
  const normalized = buildEmptyReservationStore();
  if (!store || typeof store !== "object" || Array.isArray(store)) {
    return normalized;
  }

  normalized.updated_at =
    typeof store.updated_at === "string" && store.updated_at ? store.updated_at : utcNow();
  normalized.reservations = Array.isArray(store.reservations)
    ? store.reservations.map(normalizeReservation)
    : [];
  return normalized;
}

export function getReservationsPath(repoRoot) {
  return getPulsePaths(repoRoot).reservations;
}

export function readReservationStore(repoRoot) {
  const reservationsPath = getReservationsPath(repoRoot);
  if (!fs.existsSync(reservationsPath)) {
    return buildEmptyReservationStore();
  }

  try {
    return normalizeReservationStore(JSON.parse(fs.readFileSync(reservationsPath, "utf8")));
  } catch {
    return buildEmptyReservationStore();
  }
}

function writeReservationStore(repoRoot, store) {
  const reservationsPath = getReservationsPath(repoRoot);
  ensureParent(reservationsPath);
  fs.writeFileSync(
    reservationsPath,
    `${JSON.stringify(normalizeReservationStore(store), null, 2)}\n`,
    "utf8",
  );
}

export function ensureReservationStore(repoRoot) {
  const reservationsPath = getReservationsPath(repoRoot);
  if (!fs.existsSync(reservationsPath)) {
    writeReservationStore(repoRoot, buildEmptyReservationStore());
  }
  return reservationsPath;
}

function getLockPath(repoRoot) {
  return `${getReservationsPath(repoRoot)}.lock`;
}

function withReservationLock(repoRoot, mutator) {
  const lockPath = getLockPath(repoRoot);
  ensureParent(lockPath);
  const deadline = Date.now() + LOCK_TIMEOUT_MS;

  while (true) {
    try {
      const fd = fs.openSync(lockPath, "wx");
      fs.writeFileSync(fd, JSON.stringify({ pid: process.pid, created_at: utcNow() }), "utf8");
      fs.closeSync(fd);
      break;
    } catch (error) {
      if (error?.code !== "EEXIST") {
        throw error;
      }

      let stale = false;
      try {
        const stats = fs.statSync(lockPath);
        stale = Date.now() - stats.mtimeMs > DEFAULT_LOCK_STALE_MS;
      } catch {
        stale = true;
      }

      if (stale) {
        fs.rmSync(lockPath, { force: true });
        continue;
      }

      if (Date.now() >= deadline) {
        throw new Error("Timed out waiting for reservation lock.");
      }
      sleepMs(LOCK_RETRY_MS);
    }
  }

  try {
    return mutator();
  } finally {
    fs.rmSync(lockPath, { force: true });
  }
}

export function sweepExpiredReservations(repoRoot) {
  return withReservationLock(repoRoot, () => {
    const store = readReservationStore(repoRoot);
    const now = Date.now();
    const swept = [];

    for (const reservation of store.reservations) {
      if (reservation.status !== "active" || !reservation.expires_at) {
        continue;
      }
      if (Date.parse(reservation.expires_at) > now) {
        continue;
      }
      reservation.status = "expired";
      reservation.updated_at = utcNow();
      swept.push(reservation.id);
    }

    if (swept.length > 0) {
      store.updated_at = utcNow();
      writeReservationStore(repoRoot, store);
    }

    return {
      swept_count: swept.length,
      swept_ids: swept,
      store,
    };
  });
}

function reservationMatchesPathFilter(reservation, pathPatterns) {
  if (!Array.isArray(pathPatterns) || pathPatterns.length === 0) {
    return true;
  }

  return reservation.paths.some((reservationPath) =>
    pathPatterns.some((requestedPath) =>
      reservationPatternsOverlap(reservationPath, requestedPath),
    ),
  );
}

export function listReservations(repoRoot, options = {}) {
  const store = readReservationStore(repoRoot);
  const now = Date.now();

  const reservations = store.reservations
    .map((reservation) => {
      if (
        reservation.status === "active" &&
        reservation.expires_at &&
        Date.parse(reservation.expires_at) <= now
      ) {
        return {
          ...reservation,
          status: "expired",
        };
      }
      return reservation;
    })
    .filter((reservation) => {
      if (options.status && reservation.status !== options.status) {
        return false;
      }
      if (options.activeOnly && reservation.status !== "active") {
        return false;
      }
      if (options.agent && reservation.agent !== options.agent) {
        return false;
      }
      if (options.itemId && reservation.item_id !== options.itemId) {
        return false;
      }
      return reservationMatchesPathFilter(reservation, options.paths);
    });

  return {
    schema_version: RESERVATION_SCHEMA_VERSION,
    updated_at: store.updated_at,
    reservations,
  };
}

export function findReservationConflicts(repoRoot, options = {}) {
  const requestedPaths = Array.isArray(options.paths)
    ? options.paths
        .map((item) => normalizeReservationPattern(repoRoot, item))
        .filter(Boolean)
    : [];
  if (requestedPaths.length === 0) {
    return [];
  }

  const activeReservations = listReservations(repoRoot, { activeOnly: true }).reservations;
  return activeReservations
    .filter((reservation) => reservation.agent !== options.agent)
    .filter((reservation) =>
      reservation.paths.some((heldPath) =>
        requestedPaths.some((requestedPath) =>
          reservationPatternsOverlap(heldPath, requestedPath),
        ),
      ),
    );
}

export function reservePaths(repoRoot, options = {}) {
  const agent = typeof options.agent === "string" ? options.agent.trim() : "";
  if (!agent) {
    throw new Error("reserve requires --agent.");
  }

  const normalizedPaths = Array.isArray(options.paths)
    ? options.paths.map((item) => normalizeReservationPattern(repoRoot, item)).filter(Boolean)
    : [];
  if (normalizedPaths.length === 0) {
    throw new Error("reserve requires at least one path or glob.");
  }

  return withReservationLock(repoRoot, () => {
    const store = readReservationStore(repoRoot);
    const now = utcNow();

    for (const reservation of store.reservations) {
      if (
        reservation.status === "active" &&
        reservation.expires_at &&
        Date.parse(reservation.expires_at) <= Date.now()
      ) {
        reservation.status = "expired";
        reservation.updated_at = now;
      }
    }

    const conflicts = store.reservations.filter(
      (reservation) =>
        reservation.status === "active" &&
        reservation.agent !== agent &&
        reservation.paths.some((heldPath) =>
          normalizedPaths.some((requestedPath) =>
            reservationPatternsOverlap(heldPath, requestedPath),
          ),
        ),
    );

    if (conflicts.length > 0) {
      return {
        ok: false,
        conflicts,
      };
    }

    const ttlSeconds =
      Number.isFinite(options.ttlSeconds) && options.ttlSeconds > 0
        ? Math.floor(options.ttlSeconds)
        : null;
    const reservation = normalizeReservation({
      agent,
      item_id: typeof options.itemId === "string" ? options.itemId.trim() : "",
      paths: normalizedPaths,
      created_at: now,
      updated_at: now,
      ttl_seconds: ttlSeconds,
      expires_at: ttlSeconds
        ? new Date(Date.parse(now) + ttlSeconds * 1000).toISOString()
        : null,
      status: "active",
      note: typeof options.note === "string" ? options.note : "",
    });

    store.reservations.push(reservation);
    store.updated_at = now;
    writeReservationStore(repoRoot, store);

    return {
      ok: true,
      reservation,
      conflicts: [],
    };
  });
}

function shouldReleaseReservation(reservation, options = {}) {
  if (reservation.status !== "active") {
    return false;
  }
  if (options.agent && reservation.agent !== options.agent) {
    return false;
  }
  if (options.ids?.length && !options.ids.includes(reservation.id)) {
    return false;
  }
  if (options.itemId && reservation.item_id !== options.itemId) {
    return false;
  }
  if (!reservationMatchesPathFilter(reservation, options.paths)) {
    return false;
  }
  return true;
}

export function releaseReservations(repoRoot, options = {}) {
  return withReservationLock(repoRoot, () => {
    const store = readReservationStore(repoRoot);
    const released = [];
    const releasedAt = utcNow();

    for (const reservation of store.reservations) {
      if (!shouldReleaseReservation(reservation, options)) {
        continue;
      }
      reservation.status = "released";
      reservation.updated_at = releasedAt;
      reservation.released_at = releasedAt;
      released.push(reservation.id);
    }

    if (released.length > 0) {
      store.updated_at = releasedAt;
      writeReservationStore(repoRoot, store);
    }

    return {
      released_count: released.length,
      released_ids: released,
    };
  });
}

export function summarizeReservationStatus(repoRoot) {
  const reservations = listReservations(repoRoot).reservations;
  const active = reservations.filter((item) => item.status === "active");
  const expired = reservations.filter((item) => item.status === "expired");
  const released = reservations.filter((item) => item.status === "released");

  return {
    store_path: getReservationsPath(repoRoot),
    total: reservations.length,
    active_count: active.length,
    expired_count: expired.length,
    released_count: released.length,
    active_agents: [...new Set(active.map((item) => item.agent).filter(Boolean))].sort(),
    active_reservations: active,
  };
}

export function summarizeReservationStatusForState(repoRoot) {
  const summary = summarizeReservationStatus(repoRoot);
  const reservationStore = readReservationStore(repoRoot);

  return {
    exists: true,
    schema_version:
      typeof reservationStore?.schema_version === "string"
        ? reservationStore.schema_version
        : RESERVATION_SCHEMA_VERSION,
    updated_at: typeof reservationStore?.updated_at === "string" ? reservationStore.updated_at : "",
    ...summary,
  };
}
