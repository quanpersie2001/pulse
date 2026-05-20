import fs from "node:fs";
import path from "node:path";

function normalizeSelector(selector) {
  return String(selector || "").trim().replaceAll("\\", "/");
}

export function summarizeProjectDocs(repoRoot, paths, readJsonIfExists) {
  const projectDocs = readJsonIfExists(paths.projectDocs);
  const rootContextPath = "CONTEXT.md";
  const contextMapPath = "CONTEXT-MAP.md";
  const adrDirPath = "docs/adr";
  const hasRootContext = fs.existsSync(path.join(repoRoot, rootContextPath));
  const hasContextMap = fs.existsSync(path.join(repoRoot, contextMapPath));
  const hasAdrDir = fs.existsSync(path.join(repoRoot, adrDirPath));

  const mappedEntries = Array.isArray(projectDocs?.context?.entries)
    ? projectDocs.context.entries
        .filter((entry) => entry && typeof entry.path === "string" && entry.path)
        .map((entry) => ({
          id: typeof entry.id === "string" ? entry.id : "",
          path: normalizeSelector(entry.path),
        }))
    : [];

  const mappedMode = typeof projectDocs?.mode === "string" ? projectDocs.mode : "";
  const status = projectDocs
    ? (typeof projectDocs.status === "string" && projectDocs.status ? projectDocs.status : "mapped")
    : ((hasRootContext || hasContextMap || hasAdrDir) ? "detected" : "missing");
  const mode = mappedMode || (hasContextMap ? "multi-context" : (hasRootContext ? "single-context" : ""));
  const contextRoot = typeof projectDocs?.context?.root === "string" && projectDocs.context.root
    ? normalizeSelector(projectDocs.context.root)
    : (hasRootContext ? rootContextPath : "");
  const contextMap = typeof projectDocs?.context?.map === "string" && projectDocs.context.map
    ? normalizeSelector(projectDocs.context.map)
    : (hasContextMap ? contextMapPath : "");
  const adrDir = typeof projectDocs?.adrs?.dir === "string" && projectDocs.adrs.dir
    ? normalizeSelector(projectDocs.adrs.dir)
    : (hasAdrDir ? adrDirPath : "");
  const notes = Array.isArray(projectDocs?.notes)
    ? projectDocs.notes.filter((item) => typeof item === "string" && item.trim() !== "")
    : [];
  const warnings = [];

  if (projectDocs && !mode) {
    warnings.push("project-docs.json exists but mode is missing.");
  }
  if (projectDocs && mode === "single-context" && !contextRoot) {
    warnings.push("project-docs.json says single-context but no root CONTEXT.md is mapped.");
  }
  if (projectDocs && mode === "multi-context" && !contextMap && mappedEntries.length === 0) {
    warnings.push("project-docs.json says multi-context but no CONTEXT-MAP.md or context entries are mapped.");
  }
  if (!projectDocs && (hasRootContext || hasContextMap || hasAdrDir)) {
    warnings.push("Repo-level project docs were detected but .pulse/project-docs.json is missing.");
  }

  return {
    exists: Boolean(projectDocs),
    status,
    mode,
    mapping_path: projectDocs ? ".pulse/project-docs.json" : "",
    context: {
      root: contextRoot,
      map: contextMap,
      entries: mappedEntries,
    },
    adrs: {
      enabled: typeof projectDocs?.adrs?.enabled === "boolean" ? projectDocs.adrs.enabled : hasAdrDir,
      dir: adrDir,
      exists: adrDir ? fs.existsSync(path.join(repoRoot, adrDir)) : false,
    },
    notes,
    warnings,
  };
}
