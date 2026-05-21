export function firstNonEmptyString(...values) {
  for (const value of values) {
    if (typeof value === "string" && value.trim()) {
      return value.trim();
    }
  }
  return "";
}

export function uniqueStrings(values) {
  return [...new Set((values || []).filter((value) => typeof value === "string" && value.trim()).map((value) => value.trim()))];
}

export function normalizeSlashPath(value) {
  return String(value || "").replace(/\\/g, "/");
}

export function stripLeadingDotSlash(value) {
  return normalizeSlashPath(value).replace(/^\.\/+/, "");
}
