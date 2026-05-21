import path from "node:path";

export const WORKFLOW_SKILL_NAME = "workflow";

export const PROVIDERS = [
  { name: "claude-code", configDir: ".claude", skillsRoot: ".claude/skills" },
  { name: "cursor", configDir: ".cursor", skillsRoot: ".cursor/skills" },
  { name: "gemini", configDir: ".gemini", skillsRoot: ".gemini/skills" },
  { name: "codex", configDir: ".codex", skillsRoot: ".codex/skills" },
  { name: "agents", configDir: ".agents", skillsRoot: ".agents/skills" },
  { name: "github", configDir: ".github", skillsRoot: ".github/skills" },
  { name: "kiro", configDir: ".kiro", skillsRoot: ".kiro/skills" },
  { name: "opencode", configDir: ".opencode", skillsRoot: ".opencode/skills" },
  { name: "pi", configDir: ".pi", skillsRoot: ".pi/skills" },
  { name: "qoder", configDir: ".qoder", skillsRoot: ".qoder/skills" },
  { name: "trae-cn", configDir: ".trae-cn", skillsRoot: ".trae-cn/skills" },
  { name: "trae", configDir: ".trae", skillsRoot: ".trae/skills" },
  { name: "rovo-dev", configDir: ".rovodev", skillsRoot: ".rovodev/skills" },
];

export function getPulseCommand(provider) {
  return ["node", path.posix.join(provider.skillsRoot, WORKFLOW_SKILL_NAME, "scripts", "pulse.mjs")].join(" ");
}
