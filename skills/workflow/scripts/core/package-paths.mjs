import path from "node:path";
import { fileURLToPath } from "node:url";

export function getScriptDir(metaUrl) {
  return path.dirname(fileURLToPath(metaUrl));
}

export function getWorkflowScriptsDir(scriptDir) {
  let current = path.resolve(scriptDir);
  while (path.basename(current) !== "scripts") {
    const parent = path.dirname(current);
    if (parent === current) {
      throw new Error(`Unable to locate workflow scripts directory from ${scriptDir}.`);
    }
    current = parent;
  }
  return current;
}

export function getWorkflowSkillDir(scriptDir) {
  return path.dirname(getWorkflowScriptsDir(scriptDir));
}

export function getWorkflowTemplateDir(scriptDir) {
  return path.join(getWorkflowSkillDir(scriptDir), "templates");
}

export function getWorkflowWorksTemplateDir(scriptDir) {
  return path.join(getWorkflowTemplateDir(scriptDir), "works");
}

export function getWorkflowSkillPath(scriptDir) {
  return path.join(getWorkflowSkillDir(scriptDir), "SKILL.md");
}

export function getPulseEntrypointPath(scriptDir) {
  return path.join(getWorkflowScriptsDir(scriptDir), "pulse.mjs");
}

export function getPluginRoot(scriptDir) {
  return path.resolve(getWorkflowSkillDir(scriptDir), "..", "..");
}
