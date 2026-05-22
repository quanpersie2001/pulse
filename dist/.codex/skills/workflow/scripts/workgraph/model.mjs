export const ITEM_KIND_VALUES = ["EPIC", "STORY", "TASK", "BUG"];
export const STATUS_VALUES = ["OPEN", "IN_PROGRESS", "BLOCKED", "CLOSED"];
export const RISK_FLAG_VALUES = [
  "AUTH",
  "DATA",
  "SECURITY",
  "MIGRATION",
  "EXISTING_BEHAVIOR",
  "EXTERNAL_API",
  "PERFORMANCE",
  "UX",
  "CI",
  "UNKNOWN",
];
export const KIND_PREFIX_MAP = {
  EPIC: "E",
  STORY: "S",
  TASK: "T",
  BUG: "B",
};
export const PREFIX_KIND_MAP = Object.fromEntries(
  Object.entries(KIND_PREFIX_MAP).map(([kind, prefix]) => [prefix, kind]),
);
export const ITEM_FIELD_ORDER = [
  "id",
  "kind",
  "title",
  "slug",
  "status",
  "parent_id",
  "epic_id",
  "depends_on",
  "linked_items",
  "priority",
  "owner",
  "labels",
  "risk_flags",
  "blocked_reason",
  "content_path",
  "verification_path",
  "created_at",
  "updated_at",
  "closed_at",
];

const KIND_RANK = {
  EPIC: 0,
  STORY: 1,
  TASK: 2,
  BUG: 3,
};

export function utcNow(date = new Date()) {
  return new Date(date).toISOString();
}

export function normalizeNullableString(value) {
  if (value === undefined || value === null) {
    return null;
  }

  const trimmed = String(value).trim();
  return trimmed ? trimmed : null;
}

export function normalizeStringArray(values) {
  const next = new Set();

  for (const value of values || []) {
    const normalized = normalizeNullableString(value);
    if (normalized) {
      next.add(normalized);
    }
  }

  return [...next].sort((left, right) => left.localeCompare(right));
}

export function cloneItemRecord(item) {
  return {
    ...item,
    depends_on: [...(item.depends_on || [])],
    linked_items: [...(item.linked_items || [])],
    labels: [...(item.labels || [])],
    risk_flags: [...(item.risk_flags || [])],
  };
}

export function cloneItems(items) {
  return (items || []).map((item) => cloneItemRecord(item));
}

export function canonicalizeItemRecord(item) {
  const source = cloneItemRecord(item);
  source.depends_on = normalizeStringArray(source.depends_on);
  source.linked_items = normalizeStringArray(source.linked_items);
  source.labels = normalizeStringArray(source.labels);
  source.risk_flags = normalizeStringArray(source.risk_flags);

  const ordered = {};
  for (const field of ITEM_FIELD_ORDER) {
    ordered[field] = Object.prototype.hasOwnProperty.call(source, field) ? source[field] : null;
  }
  return ordered;
}

export function sortItemsDeterministically(items) {
  return [...(items || [])]
    .map((item) => canonicalizeItemRecord(item))
    .sort((left, right) => {
      const createdCompare = String(left.created_at || "").localeCompare(String(right.created_at || ""));
      if (createdCompare !== 0) {
        return createdCompare;
      }

      const kindCompare = (KIND_RANK[left.kind] ?? 99) - (KIND_RANK[right.kind] ?? 99);
      if (kindCompare !== 0) {
        return kindCompare;
      }

      return String(left.id || "").localeCompare(String(right.id || ""));
    });
}

export function isIsoUtcTimestamp(value) {
  if (typeof value !== "string" || !value.endsWith("Z")) {
    return false;
  }

  const parsed = Date.parse(value);
  return Number.isFinite(parsed) && new Date(parsed).toISOString() === value;
}

export function parsePriority(value, fallback = 2) {
  if (value === undefined || value === null || value === "") {
    return fallback;
  }

  if (typeof value === "number" && Number.isInteger(value)) {
    return value;
  }

  const parsed = Number.parseInt(String(value), 10);
  if (!Number.isInteger(parsed)) {
    throw new Error(`Priority must be an integer. Received: ${value}`);
  }
  return parsed;
}

export function normalizeOptionalBoolean(value) {
  if (value === undefined || value === null) {
    return false;
  }

  if (typeof value === "boolean") {
    return value;
  }

  const lowered = String(value).trim().toLowerCase();
  return lowered === "1" || lowered === "true" || lowered === "yes" || lowered === "on";
}
