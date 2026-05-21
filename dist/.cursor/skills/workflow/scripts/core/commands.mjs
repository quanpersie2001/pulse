import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { firstNonEmptyString } from "./strings.mjs";

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const METADATA_PATH = path.resolve(SCRIPT_DIR, "..", "metadata", "command-metadata.json");
let cachedMetadata = null;

export function loadWorkflowCommandMetadata() {
  if (!cachedMetadata) {
    cachedMetadata = JSON.parse(fs.readFileSync(METADATA_PATH, "utf8"));
  }
  return cachedMetadata;
}

export function validWorkflowCommands() {
  return new Set(
    (loadWorkflowCommandMetadata().commands || [])
      .map((command) => command?.name)
      .filter((name) => typeof name === "string" && name.trim()),
  );
}

export function normalizeWorkflowCommand(value) {
  const normalized = firstNonEmptyString(value);
  if (!normalized) {
    return "";
  }

  const validCommands = validWorkflowCommands();
  if (normalized.startsWith("pulse:workflow ")) {
    const command = normalized.slice("pulse:workflow ".length).trim();
    return validCommands.has(command) ? normalized : "";
  }
  if (normalized.startsWith("pulse:")) {
    return "";
  }
  return validCommands.has(normalized) ? `pulse:workflow ${normalized}` : "";
}

export function assertWorkflowCommand(value) {
  const normalized = normalizeWorkflowCommand(value);
  if (!normalized) {
    throw new Error(`Invalid workflow command: ${value}`);
  }
  return normalized;
}
