import fs from "node:fs";
import path from "node:path";

import {
  getPluginRoot,
  getScriptDir,
} from "../pulse_package_paths.mjs";

const SCRIPT_DIR = path.dirname(getScriptDir(import.meta.url));
const PLUGIN_ROOT = getPluginRoot(SCRIPT_DIR);
const AGENTS_TEMPLATE_PATH = path.join(PLUGIN_ROOT, "AGENTS.template.md");

export function managedAgentsPresent(text) {
  return text.includes("<!-- PULSE:START -->") && text.includes("<!-- PULSE:END -->");
}

export function mergeAgentsContent(existing, template) {
  const stripped = existing.trim();
  if (!stripped) {
    return { text: template, status: "created_from_template" };
  }

  if (managedAgentsPresent(existing)) {
    const updated = existing.replace(
      /<!-- PULSE:START -->[\s\S]*?<!-- PULSE:END -->\n?/,
      template,
    );
    return { text: `${updated.replace(/\s*$/, "")}\n`, status: "updated_managed_block" };
  }

  const glue = existing.endsWith("\n\n") ? "" : "\n\n";
  return {
    text: `${existing.replace(/\s*$/, "")}${glue}${template}`,
    status: "appended_managed_block",
  };
}

export function readTemplate() {
  return `${fs.readFileSync(AGENTS_TEMPLATE_PATH, "utf8").replace(/\s*$/, "")}\n`;
}
