import path from "node:path";
import { fileURLToPath } from "node:url";

export function getScriptDir(metaUrl) {
  return path.dirname(fileURLToPath(metaUrl));
}

export function getWorkflowSkillDir(scriptDir) {
  return path.resolve(scriptDir, "..");
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
  return path.join(scriptDir, "pulse.mjs");
}

export function getPluginRoot(scriptDir) {
  return path.resolve(getWorkflowSkillDir(scriptDir), "..", "..");
}
