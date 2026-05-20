import fs from "node:fs";
import path from "node:path";

function listHistoryFeatureFiles(repoRoot, feature) {
  const historyDir = path.join(repoRoot, "history", feature);
  if (!fs.existsSync(historyDir)) {
    return [];
  }

  const queue = [historyDir];
  const files = [];
  while (queue.length > 0) {
    const currentDir = queue.shift();
    let entries = [];
    try {
      entries = fs.readdirSync(currentDir, { withFileTypes: true });
    } catch {
      continue;
    }

    for (const entry of entries) {
      const absolutePath = path.join(currentDir, entry.name);
      if (entry.isDirectory()) {
        queue.push(absolutePath);
        continue;
      }
      const relativePath = path.relative(repoRoot, absolutePath).split(path.sep).join("/");
      files.push(relativePath);
    }
  }

  return files.sort((left, right) => left.localeCompare(right));
}

export function summarizeHistoryLifecycle(repoRoot, feature) {
  const summary = {
    feature,
    exists: false,
    lifecycle_summary: "",
    approved_artifacts: [],
    verification: [],
    memory_promotions: [],
    lifecycle_signals: [],
    next_reads: [],
    self_sufficient: false,
  };

  if (!feature) {
    return summary;
  }

  const historyFiles = listHistoryFeatureFiles(repoRoot, feature);
  if (historyFiles.length === 0) {
    return summary;
  }

  summary.exists = true;
  const lifecycleSummaryPath = `history/${feature}/lifecycle-summary.md`;
  if (historyFiles.includes(lifecycleSummaryPath)) {
    summary.lifecycle_summary = lifecycleSummaryPath;
  }

  const requiredArtifacts = [
    `history/${feature}/CONTEXT.md`,
    `history/${feature}/approach.md`,
  ].filter((item) => historyFiles.includes(item));
  const shapeArtifacts = [
    `history/${feature}/phase-plan.md`,
    `history/${feature}/epic-map.md`,
    `history/${feature}/work-shape.md`,
    `history/${feature}/current-story-pack.md`,
  ].filter((item) => historyFiles.includes(item));
  const approvedArtifacts = [...requiredArtifacts, ...shapeArtifacts];
  summary.approved_artifacts = approvedArtifacts;

  const lifecycleSignals = historyFiles.filter((item) => (
    /phase-\d+-(contract|story-map)\.md$/u.test(item)
    || /\/(epic-map|work-shape|current-story-pack)\.md$/u.test(item)
  ));
  summary.lifecycle_signals = lifecycleSignals;

  summary.verification = historyFiles.filter((item) => item.startsWith(`history/${feature}/verification/`));
  summary.memory_promotions = [
    ...historyFiles.filter((item) => item.startsWith(`history/${feature}/memory/`)),
    ...historyFiles.filter((item) => item.endsWith("lifecycle-summary.md") && item !== lifecycleSummaryPath),
  ];

  summary.self_sufficient = Boolean(
    summary.lifecycle_summary
    && requiredArtifacts.length >= 2
    && shapeArtifacts.length > 0
    && lifecycleSignals.length > 0
    && summary.verification.length > 0,
  );

  summary.next_reads = [...new Set([
    summary.lifecycle_summary,
    ...approvedArtifacts,
    ...lifecycleSignals.slice(0, 4),
    ...summary.verification.slice(0, 4),
  ].filter(Boolean))];

  return summary;
}
