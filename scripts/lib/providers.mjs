export const WORKFLOW_SKILL_NAME = "workflow";

export const PROVIDERS = [
  {
    name: "claude-code",
    displayName: "Claude Code",
    providerTags: ["claude-code", "claude"],
    configDir: ".claude",
  },
  {
    name: "cursor",
    displayName: "Cursor",
    providerTags: ["cursor"],
    configDir: ".cursor",
  },
  {
    name: "gemini",
    displayName: "Gemini",
    providerTags: ["gemini"],
    configDir: ".gemini",
  },
  {
    name: "codex",
    displayName: "Codex",
    providerTags: ["codex"],
    configDir: ".codex",
  },
  {
    name: "agents",
    displayName: "Codex Repo Skills",
    providerTags: ["agents", "codex"],
    configDir: ".agents",
  },
  {
    name: "github",
    displayName: "GitHub Copilot",
    providerTags: ["github"],
    configDir: ".github",
  },
  {
    name: "kiro",
    displayName: "Kiro",
    providerTags: ["kiro"],
    configDir: ".kiro",
  },
  {
    name: "opencode",
    displayName: "OpenCode",
    providerTags: ["opencode"],
    configDir: ".opencode",
  },
  {
    name: "pi",
    displayName: "Pi",
    providerTags: ["pi"],
    configDir: ".pi",
  },
  {
    name: "qoder",
    displayName: "Qoder",
    providerTags: ["qoder"],
    configDir: ".qoder",
  },
  {
    name: "trae-cn",
    displayName: "Trae China",
    providerTags: ["trae-cn", "trae"],
    configDir: ".trae-cn",
  },
  {
    name: "trae",
    displayName: "Trae",
    providerTags: ["trae"],
    configDir: ".trae",
  },
  {
    name: "rovo-dev",
    displayName: "Rovo Dev",
    providerTags: ["rovo-dev"],
    configDir: ".rovodev",
  },
];

export function getPulseCommand(provider) {
  // The installed plugin is guidance only. Mutable workgraph and runtime
  // authority lives in the Rust executable and daemon available on PATH.
  return "pulse";
}
