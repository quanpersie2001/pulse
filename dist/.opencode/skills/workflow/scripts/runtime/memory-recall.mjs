import fs from "node:fs";
import path from "node:path";

import { readTextIfExists as fileTextIfExists } from "../core/fs.mjs";
import { firstNonEmptyString } from "../core/strings.mjs";

function listDirectoryFiles(dirPath) {
  if (!fs.existsSync(dirPath)) {
    return [];
  }

  try {
    return fs.readdirSync(dirPath, { withFileTypes: true })
      .filter((entry) => entry.isFile())
      .map((entry) => entry.name)
      .sort((a, b) => a.localeCompare(b));
  } catch {
    return [];
  }
}

function tokenizeRecallValue(value) {
  return String(value || "")
    .toLowerCase()
    .split(/[^a-z0-9]+/)
    .filter(Boolean);
}

function stripDatedMemoryPrefix(fileName) {
  return fileName.toLowerCase().replace(/^\d{8}-/, "");
}

function parseInlineMetadataArray(value) {
  const normalized = String(value || "").trim();
  if (!normalized) {
    return [];
  }
  if (normalized.startsWith("[") && normalized.endsWith("]")) {
    return normalized
      .slice(1, -1)
      .split(",")
      .map((item) => item.trim().replace(/^['"]|['"]$/g, ""))
      .filter(Boolean);
  }
  return [normalized.replace(/^['"]|['"]$/g, "")].filter(Boolean);
}

function parseFrontmatterScalar(value) {
  const normalized = String(value || "").trim();
  if (!normalized) {
    return "";
  }
  if (normalized.startsWith("[") && normalized.endsWith("]")) {
    return parseInlineMetadataArray(normalized);
  }
  if (normalized === "true") {
    return true;
  }
  if (normalized === "false") {
    return false;
  }
  return normalized.replace(/^['"]|['"]$/g, "");
}

function parseMetadataFrontmatter(text) {
  if (!text.startsWith("---\n")) {
    return {};
  }

  const lines = text.split("\n");
  let endIndex = -1;
  for (let index = 1; index < lines.length; index += 1) {
    if (lines[index].trim() === "---") {
      endIndex = index;
      break;
    }
  }
  if (endIndex === -1) {
    return {};
  }

  const parsed = {};
  let activeArrayKey = "";
  for (const line of lines.slice(1, endIndex)) {
    const listMatch = line.match(/^\s*-\s*(.+)$/);
    if (activeArrayKey && listMatch) {
      parsed[activeArrayKey].push(parseFrontmatterScalar(listMatch[1]));
      continue;
    }

    const match = line.match(/^([A-Za-z0-9_-]+):\s*(.*)$/);
    if (!match) {
      activeArrayKey = "";
      continue;
    }

    const key = match[1].toLowerCase();
    const rawValue = match[2].trim();
    if (!rawValue) {
      parsed[key] = [];
      activeArrayKey = key;
      continue;
    }

    parsed[key] = parseFrontmatterScalar(rawValue);
    activeArrayKey = Array.isArray(parsed[key]) ? key : "";
  }
  return parsed;
}

function extractApplicableWhen(text) {
  const exactMatch = text.match(/^\*\*Applicable-when:\*\*\s*(.+)$/im);
  if (exactMatch) {
    return exactMatch[1].trim();
  }
  const fallbackMatch = text.match(/^applicable-when:\s*(.+)$/im);
  return fallbackMatch ? fallbackMatch[1].trim() : "";
}

function toMetadataArray(value) {
  if (Array.isArray(value)) {
    return value.map((item) => String(item || "").trim()).filter(Boolean);
  }
  return parseInlineMetadataArray(value || "");
}

function loadRecallEntryMetadata(repoRoot, relativePath) {
  const absolutePath = path.join(repoRoot, relativePath);
  const text = fileTextIfExists(absolutePath);
  const frontmatter = parseMetadataFrontmatter(text);
  const appliesWhen = firstNonEmptyString(
    frontmatter.applies_when,
    frontmatter["applicable-when"],
    extractApplicableWhen(text),
  );
  const scope = toMetadataArray(frontmatter.scope || frontmatter.files || []);
  const signals = toMetadataArray(frontmatter.signals || frontmatter.triggers || []);
  const tags = toMetadataArray(frontmatter.tags || []);
  const feature = firstNonEmptyString(frontmatter.feature);
  const severity = firstNonEmptyString(frontmatter.severity);
  const missingFields = [];

  if (!feature) {
    missingFields.push("feature");
  }
  if (tags.length === 0) {
    missingFields.push("tags");
  }
  if (!severity) {
    missingFields.push("severity");
  }
  if (!appliesWhen) {
    missingFields.push("applies_when");
  }
  if (scope.length === 0) {
    missingFields.push("scope");
  }
  if (signals.length === 0) {
    missingFields.push("signals");
  }

  return {
    feature,
    tags,
    severity,
    applies_when: appliesWhen,
    scope,
    signals,
    missing_fields: missingFields,
    has_metadata: text.startsWith("---\n"),
  };
}

function buildRecallSelectionContext(feature, status) {
  return {
    feature_tokens: [...new Set(tokenizeRecallValue(feature))],
    blocker_tokens: [...new Set(
      (Array.isArray(status.tooling_status?.blockers) ? status.tooling_status.blockers : [])
        .flatMap((item) => tokenizeRecallValue(item)),
    )],
    phase_tokens: [...new Set(tokenizeRecallValue(status.current_feature?.phase || status.state_json?.phase || ""))],
  };
}

function scoreMetadataTokens(tokens, haystacks, reasonPrefix, reasons, pointsPerMatch) {
  let score = 0;
  for (const token of tokens || []) {
    if (!token) {
      continue;
    }
    if (haystacks.some((value) => value.includes(token))) {
      reasons.push(`${reasonPrefix}:${token}`);
      score += pointsPerMatch;
    }
  }
  return score;
}

function scoreExactFieldMatch(tokens, values, reasonPrefix, reasons, pointsPerMatch) {
  let score = 0;
  const normalizedValues = (values || []).map((value) => tokenizeRecallValue(value).join(" ")).filter(Boolean);
  for (const token of tokens || []) {
    if (!token) {
      continue;
    }
    if (normalizedValues.some((value) => value === token || value.split(" ").includes(token))) {
      reasons.push(`${reasonPrefix}:${token}`);
      score += pointsPerMatch;
    }
  }
  return score;
}

function inferRecallSchemaStrength(metadata) {
  const requiredFields = ["feature", "tags", "severity", "applies_when", "scope", "signals"];
  const presentCount = requiredFields.filter((field) => {
    const value = metadata?.[field];
    return Array.isArray(value) ? value.length > 0 : Boolean(value);
  }).length;
  return {
    required_fields: requiredFields,
    present_fields: presentCount,
    is_strong: presentCount === requiredFields.length,
  };
}

function classifyRecallEntry(relativePath, selectionContext, repoRoot) {
  const fileName = stripDatedMemoryPrefix(path.basename(relativePath, path.extname(relativePath)));
  const metadata = loadRecallEntryMetadata(repoRoot, relativePath);
  const metadataHaystacks = [
    metadata.feature,
    ...metadata.tags,
    metadata.applies_when,
    ...metadata.scope,
    ...metadata.signals,
  ].flatMap((value) => tokenizeRecallValue(value));
  const fileNameTokens = tokenizeRecallValue(fileName);
  const reasons = [];
  let score = 0;

  score += scoreExactFieldMatch(selectionContext.feature_tokens, [metadata.feature], "feature", reasons, 8);
  score += scoreMetadataTokens(selectionContext.feature_tokens, metadataHaystacks, "feature", reasons, 6);
  score += scoreMetadataTokens(selectionContext.phase_tokens, metadata.tags, "phase-tag", reasons, 6);
  score += scoreMetadataTokens(selectionContext.phase_tokens, [metadata.applies_when], "phase", reasons, 5);
  score += scoreMetadataTokens(selectionContext.phase_tokens, metadata.scope, "scope", reasons, 4);
  score += scoreMetadataTokens(selectionContext.blocker_tokens, metadata.signals, "signal", reasons, 7);
  score += scoreMetadataTokens(selectionContext.blocker_tokens, [metadata.applies_when], "blocker", reasons, 5);
  score += scoreMetadataTokens(selectionContext.blocker_tokens, metadata.scope, "scope", reasons, 4);

  if (reasons.length === 0) {
    score += scoreMetadataTokens(selectionContext.feature_tokens, fileNameTokens, "feature", reasons, 2);
    score += scoreMetadataTokens(selectionContext.phase_tokens, fileNameTokens, "phase", reasons, 1);
    score += scoreMetadataTokens(selectionContext.blocker_tokens, fileNameTokens, "blocker", reasons, 1);
  }

  if (metadata.severity === "critical") {
    score += 2;
    reasons.push("severity:critical");
  }

  const schemaStrength = inferRecallSchemaStrength(metadata);
  if (schemaStrength.is_strong) {
    score += 2;
  }

  return {
    path: relativePath,
    reasons: [...new Set(reasons)],
    score,
    metadata: {
      ...metadata,
      schema_strength: schemaStrength,
    },
  };
}

function pickRelevantRecallEntries(pathsList, selectionContext, repoRoot) {
  const matched = [];
  const fallback = [];

  for (const relativePath of pathsList) {
    const entry = classifyRecallEntry(relativePath, selectionContext, repoRoot);
    if (entry.reasons.length > 0) {
      matched.push(entry);
    } else {
      fallback.push(entry);
    }
  }

  const sortEntries = (entries) => entries.sort((left, right) => {
    if (right.score !== left.score) {
      return right.score - left.score;
    }
    return left.path.localeCompare(right.path);
  });

  return matched.length > 0 ? sortEntries(matched).slice(0, 3) : sortEntries(fallback).slice(0, 3);
}

function getFileSizeSafe(filePath) {
  try {
    return fs.statSync(filePath).size;
  } catch {
    return 0;
  }
}

function getFileAgeDaysSafe(filePath) {
  try {
    const modifiedAt = fs.statSync(filePath).mtimeMs;
    const ageMs = Date.now() - modifiedAt;
    return Math.floor(ageMs / (24 * 60 * 60 * 1000));
  } catch {
    return null;
  }
}

function collectDuplicateMemorySlugs(relativePaths) {
  const counts = new Map();
  for (const relativePath of relativePaths) {
    const slug = stripDatedMemoryPrefix(path.basename(relativePath, path.extname(relativePath)));
    counts.set(slug, (counts.get(slug) || 0) + 1);
  }
  return [...counts.entries()]
    .filter(([, count]) => count > 1)
    .map(([slug]) => slug)
    .sort((left, right) => left.localeCompare(right));
}

function buildMemoryHygiene(paths, selectedRecall, allRecallPaths) {
  const warnings = [];
  const criticalPatternsBytes = fs.existsSync(paths.criticalPatterns) ? getFileSizeSafe(paths.criticalPatterns) : 0;

  if (criticalPatternsBytes > 24 * 1024) {
    warnings.push("critical-patterns.md is getting large; review for compact, globally useful guidance only.");
  }

  const duplicateLearnings = collectDuplicateMemorySlugs(allRecallPaths.learnings);
  if (duplicateLearnings.length > 0) {
    warnings.push(`Possible duplicate learnings: ${duplicateLearnings.join(", ")}.`);
  }

  const duplicateCorrections = collectDuplicateMemorySlugs(allRecallPaths.corrections);
  if (duplicateCorrections.length > 0) {
    warnings.push(`Possible duplicate corrections: ${duplicateCorrections.join(", ")}.`);
  }

  const missingMetadataWarnings = [
    ...selectedRecall.learnings,
    ...selectedRecall.corrections,
    ...selectedRecall.ratchet,
  ].flatMap((entry) => {
    const missingFields = Array.isArray(entry.metadata?.missing_fields) ? entry.metadata.missing_fields : [];
    return missingFields.length > 0
      ? [`${entry.path} missing metadata: ${missingFields.join(", ")}`]
      : [];
  });
  if (missingMetadataWarnings.length > 0) {
    warnings.push(`Selected memory entries need stronger metadata: ${missingMetadataWarnings.join("; ")}.`);
  }

  const staleEntries = [
    ...selectedRecall.learnings,
    ...selectedRecall.corrections,
    ...selectedRecall.ratchet,
  ].flatMap((entry) => {
    const absolutePath = path.join(path.dirname(paths.agents), entry.path);
    const ageDays = getFileAgeDaysSafe(absolutePath);
    return ageDays !== null && ageDays > 180 ? [`${entry.path} (${ageDays}d old)`] : [];
  });
  if (staleEntries.length > 0) {
    warnings.push(`Selected memory entries may be stale: ${staleEntries.join(", ")}.`);
  }

  return {
    warnings,
    stats: {
      critical_patterns_bytes: criticalPatternsBytes,
      learnings_count: allRecallPaths.learnings.length,
      corrections_count: allRecallPaths.corrections.length,
      ratchet_count: allRecallPaths.ratchet.length,
    },
  };
}

function summarizeRecallReason(entry, fallbackReason) {
  if (!entry || !Array.isArray(entry.reasons) || entry.reasons.length === 0) {
    return fallbackReason;
  }

  return `matched ${entry.reasons.join(", ")}`;
}

function buildRecallPack(criticalPatternsPath, selectedRecall) {
  const pack = [];

  if (criticalPatternsPath) {
    pack.push({
      kind: "critical-patterns",
      path: criticalPatternsPath,
      reason: "global planning baseline",
    });
  }

  for (const entry of selectedRecall.corrections) {
    pack.push({
      kind: "correction",
      path: entry.path,
      reason: summarizeRecallReason(entry, "targeted tactical guardrail"),
    });
  }
  for (const entry of selectedRecall.ratchet) {
    pack.push({
      kind: "ratchet",
      path: entry.path,
      reason: summarizeRecallReason(entry, "targeted non-regression rule"),
    });
  }
  for (const entry of selectedRecall.learnings) {
    pack.push({
      kind: "learning",
      path: entry.path,
      reason: summarizeRecallReason(entry, "targeted prior lesson"),
    });
  }

  return pack;
}

export function summarizeMemoryRecall(paths, feature, status) {
  const memoryRootExists = fs.existsSync(paths.memoryRoot);
  const criticalPatternsExists = fs.existsSync(paths.criticalPatterns);
  const repoRoot = path.dirname(paths.agents);
  const learnings = listDirectoryFiles(paths.memoryLearnings).map((fileName) => path.join(".pulse", "memory", "learnings", fileName));
  const corrections = listDirectoryFiles(paths.memoryCorrections).map((fileName) => path.join(".pulse", "memory", "corrections", fileName));
  const ratchet = listDirectoryFiles(paths.memoryRatchet).map((fileName) => path.join(".pulse", "memory", "ratchet", fileName));
  const selectionContext = buildRecallSelectionContext(feature, status);
  const selectedRecall = {
    learnings: pickRelevantRecallEntries(learnings, selectionContext, repoRoot),
    corrections: pickRelevantRecallEntries(corrections, selectionContext, repoRoot),
    ratchet: pickRelevantRecallEntries(ratchet, selectionContext, repoRoot),
  };
  const criticalPatternsPath = criticalPatternsExists ? ".pulse/memory/critical-patterns.md" : "";

  const selectedEntries = [
    ...selectedRecall.learnings,
    ...selectedRecall.corrections,
    ...selectedRecall.ratchet,
  ];
  const strongSchemaCount = selectedEntries.filter((entry) => entry.metadata?.schema_strength?.is_strong).length;

  return {
    root_exists: memoryRootExists,
    critical_patterns: criticalPatternsPath,
    learnings: selectedRecall.learnings.map((entry) => entry.path),
    corrections: selectedRecall.corrections.map((entry) => entry.path),
    ratchet: selectedRecall.ratchet.map((entry) => entry.path),
    selection_context: selectionContext,
    recall_pack: buildRecallPack(criticalPatternsPath, selectedRecall),
    schema_summary: {
      selected_entries: selectedEntries.length,
      strong_schema_entries: strongSchemaCount,
      metadata_first_ranking: true,
      fallback_to_filename_tokens: selectedEntries.some((entry) => entry.reasons.length === 0),
    },
    hygiene: buildMemoryHygiene(paths, selectedRecall, { learnings, corrections, ratchet }),
  };
}
