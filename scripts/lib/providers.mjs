import path from "node:path";

export const PROVIDERS = [
  { name: "claude-code", configDir: ".claude", skillsRoot: ".claude/skills", workflowSkillDir: "pulse:workflow" },
  { name: "cursor", configDir: ".cursor", skillsRoot: ".cursor/skills", workflowSkillDir: "pulse:workflow" },
  { name: "gemini", configDir: ".gemini", skillsRoot: ".gemini/skills", workflowSkillDir: "pulse:workflow" },
  { name: "codex", configDir: ".codex", skillsRoot: ".codex/skills", workflowSkillDir: "pulse:workflow" },
  { name: "agents", configDir: ".agents", skillsRoot: ".agents/skills", workflowSkillDir: "pulse:workflow" },
  { name: "github", configDir: ".github", skillsRoot: ".github/skills", workflowSkillDir: "pulse:workflow" },
  { name: "kiro", configDir: ".kiro", skillsRoot: ".kiro/skills", workflowSkillDir: "pulse:workflow" },
  { name: "opencode", configDir: ".opencode", skillsRoot: ".opencode/skills", workflowSkillDir: "pulse:workflow" },
  { name: "pi", configDir: ".pi", skillsRoot: ".pi/skills", workflowSkillDir: "pulse:workflow" },
  { name: "qoder", configDir: ".qoder", skillsRoot: ".qoder/skills", workflowSkillDir: "pulse:workflow" },
  { name: "trae-cn", configDir: ".trae-cn", skillsRoot: ".trae-cn/skills", workflowSkillDir: "pulse:workflow" },
  { name: "trae", configDir: ".trae", skillsRoot: ".trae/skills", workflowSkillDir: "pulse:workflow" },
  { name: "rovo-dev", configDir: ".rovodev", skillsRoot: ".rovodev/skills", workflowSkillDir: "pulse:workflow" },
];

export function getPulseCommand(provider) {
  return ["node", path.posix.join(provider.skillsRoot, provider.workflowSkillDir, "scripts", "pulse.mjs")].join(" ");
}
