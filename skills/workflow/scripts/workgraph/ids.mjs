import { ITEM_KIND_VALUES, KIND_PREFIX_MAP } from "./model.mjs";

const CROCKFORD_BASE32 = "0123456789ABCDEFGHJKMNPQRSTVWXYZ";

export function kindToPrefix(kind) {
  const prefix = KIND_PREFIX_MAP[kind];
  if (!prefix) {
    throw new Error(`Unsupported item kind: ${kind}`);
  }
  return prefix;
}

export function encodeUnixSeconds(seconds) {
  let remaining = Number.parseInt(String(seconds), 10);
  if (!Number.isInteger(remaining) || remaining < 0) {
    throw new Error(`Unix seconds must be a non-negative integer. Received: ${seconds}`);
  }

  if (remaining === 0) {
    return "0";
  }

  let encoded = "";
  while (remaining > 0) {
    encoded = `${CROCKFORD_BASE32[remaining % 32]}${encoded}`;
    remaining = Math.floor(remaining / 32);
  }

  return encoded;
}

export function generateItemId(kind, existingIds = [], date = new Date()) {
  if (!ITEM_KIND_VALUES.includes(kind)) {
    throw new Error(`Unsupported item kind: ${kind}`);
  }

  const prefix = kindToPrefix(kind);
  const seconds = Math.floor(new Date(date).getTime() / 1000);
  const encodedSeconds = encodeUnixSeconds(seconds);
  const baseId = `${prefix}-${encodedSeconds}`;
  const existingUpper = new Set((existingIds || []).map((value) => String(value).toUpperCase()));

  if (!existingUpper.has(baseId)) {
    return baseId;
  }

  let sequence = 1;
  while (existingUpper.has(`${baseId}-${sequence}`)) {
    sequence += 1;
  }

  return `${baseId}-${sequence}`;
}

export function resolveItemId(records, lookup) {
  const input = String(lookup || "").trim().toUpperCase();
  if (!input) {
    throw new Error("Item lookup must not be empty.");
  }

  const exact = (records || []).find((record) => String(record.id).toUpperCase() === input);
  if (exact) {
    return exact.id;
  }

  const matches = (records || []).filter((record) => String(record.id).toUpperCase().startsWith(input));
  if (matches.length === 1) {
    return matches[0].id;
  }

  if (matches.length === 0) {
    throw new Error(`No item matches lookup: ${lookup}`);
  }

  throw new Error(
    `Ambiguous item lookup: ${lookup}. Candidates: ${matches.map((record) => record.id).join(", ")}`,
  );
}
