import fs from "node:fs";
import os from "node:os";
import path from "node:path";

function parseTomlMcpServerNames(filePath) {
  if (!fs.existsSync(filePath)) {
    return [];
  }

  const source = fs.readFileSync(filePath, "utf8");
  const names = new Set();
  for (const pattern of [/^\s*\[mcp_servers\.([^\]]+)\]\s*$/gm, /^\s*\[mcp\.servers\.([^\]]+)\]\s*$/gm]) {
    for (const match of source.matchAll(pattern)) {
      names.add(match[1].trim().replace(/^['"]|['"]$/g, ""));
    }
  }
  return [...names];
}

function parseJsonMcpServerNames(filePath) {
  if (!fs.existsSync(filePath)) {
    return [];
  }

  try {
    const payload = JSON.parse(fs.readFileSync(filePath, "utf8"));
    return payload && typeof payload === "object" && !Array.isArray(payload) ? Object.keys(payload) : [];
  } catch {
    return [];
  }
}

function readGitNexusMcpSources(repoRoot) {
  const sources = [
    {
      key: "repo_codex_config",
      server_names: parseTomlMcpServerNames(path.join(repoRoot, ".codex", "config.toml")),
    },
    {
      key: "global_codex_config",
      server_names: parseTomlMcpServerNames(path.join(os.homedir(), ".codex", "config.toml")),
    },
    {
      key: "plugin_mcp_manifest",
      server_names: parseJsonMcpServerNames(path.join(repoRoot, ".mcp.json")),
    },
  ];

  return sources
    .filter((source) => source.server_names.includes("gitnexus"))
    .map((source) => source.key)
    .sort((left, right) => left.localeCompare(right));
}

function buildGitNexusRecommendedAction(configured, matchedSources) {
  if (configured) {
    return `GitNexus is configured in ${matchedSources.join(", ")} — use graph-backed discovery as supporting context, then confirm results with direct file reads.`;
  }

  return "GitNexus is not configured in known MCP sources — use grep/file inspection fallback, or add the gitnexus MCP server before graph-backed discovery.";
}

export async function readGitNexusReadiness(repoRoot) {
  const matchedSources = readGitNexusMcpSources(repoRoot);
  const configured = matchedSources.length > 0;

  return {
    configured,
    matched_sources: matchedSources,
    recommended_action: buildGitNexusRecommendedAction(configured, matchedSources),
  };
}
