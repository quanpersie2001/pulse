#!/usr/bin/env node

import path from "node:path";
import { fileURLToPath } from "node:url";

import {
  checkpointDiff,
  checkpointList,
  checkpointResumeBrief,
  checkpointSave,
  checkpointShow,
  readPulseStatus,
  renderPulseStatus,
  resolveRepoRoot,
  syncPulseRuntimeArtifacts,
} from "./pulse_state.mjs";

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));

function parseCliArgs(argv) {
  const args = {
    repoRoot: undefined,
    json: false,
    command: "status",
    operation: "",
    feature: "",
    checkpoint_id: "",
    path: "",
    from: "",
    to: "",
    summary: "",
    next_action: "",
    sync: false,
  };

  let index = 0;
  if (argv[0] === "checkpoint") {
    args.command = "checkpoint";
    args.operation = argv[1] || "";
    index = 2;
  }

  for (; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === "--repo-root") {
      args.repoRoot = argv[index + 1];
      index += 1;
      continue;
    }
    if (arg.startsWith("--repo-root=")) {
      args.repoRoot = arg.slice("--repo-root=".length);
      continue;
    }
    if (arg === "--feature") {
      args.feature = argv[index + 1] || "";
      index += 1;
      continue;
    }
    if (arg.startsWith("--feature=")) {
      args.feature = arg.slice("--feature=".length);
      continue;
    }
    if (arg === "--checkpoint-id") {
      args.checkpoint_id = argv[index + 1] || "";
      index += 1;
      continue;
    }
    if (arg.startsWith("--checkpoint-id=")) {
      args.checkpoint_id = arg.slice("--checkpoint-id=".length);
      continue;
    }
    if (arg === "--path") {
      args.path = argv[index + 1] || "";
      index += 1;
      continue;
    }
    if (arg.startsWith("--path=")) {
      args.path = arg.slice("--path=".length);
      continue;
    }
    if (arg === "--from") {
      args.from = argv[index + 1] || "";
      index += 1;
      continue;
    }
    if (arg.startsWith("--from=")) {
      args.from = arg.slice("--from=".length);
      continue;
    }
    if (arg === "--to") {
      args.to = argv[index + 1] || "";
      index += 1;
      continue;
    }
    if (arg.startsWith("--to=")) {
      args.to = arg.slice("--to=".length);
      continue;
    }
    if (arg === "--summary") {
      args.summary = argv[index + 1] || "";
      index += 1;
      continue;
    }
    if (arg.startsWith("--summary=")) {
      args.summary = arg.slice("--summary=".length);
      continue;
    }
    if (arg === "--next-action") {
      args.next_action = argv[index + 1] || "";
      index += 1;
      continue;
    }
    if (arg.startsWith("--next-action=")) {
      args.next_action = arg.slice("--next-action=".length);
      continue;
    }
    if (arg === "--json") {
      args.json = true;
      continue;
    }
    if (arg === "--sync") {
      args.sync = true;
      continue;
    }
    if (arg === "--help" || arg === "-h") {
      process.stdout.write(
        [
          "Usage:",
          "  pulse_status.mjs [--repo-root <path>] [--json] [--sync]",
          "  pulse_status.mjs checkpoint <save|list|show|diff|resume-brief> [options] [--json]",
          "",
          "Shows a non-mutating Pulse status snapshot or checkpoint operator aid output.",
          "Use --sync to refresh persisted runtime artifacts before rendering status.",
        ].join("\n"),
      );
      process.exit(0);
    }
    throw new Error(`Unknown argument: ${arg}`);
  }

  return args;
}

function renderCheckpointResult(result) {
  if (result.operation === "save") {
    return [
      "Checkpoint saved",
      `Feature: ${result.feature}`,
      `Checkpoint: ${result.checkpoint?.checkpoint_id || "(unknown)"}`,
      `Path: ${result.checkpoint?.path || "(unknown)"}`,
      `Summary: ${result.checkpoint?.summary || "(none)"}`,
      `Next action: ${result.checkpoint?.next_action || "(none)"}`,
    ].join("\n");
  }

  if (result.operation === "list") {
    const warnings = Array.isArray(result.checkpoints?.warnings) ? result.checkpoints.warnings : [];
    return [
      `Checkpoint list for ${result.feature || "(no feature)"}`,
      `Count: ${result.checkpoints?.count || 0}`,
      ...(result.checkpoints?.entries || []).map((entry) => `- ${entry.operator_summary}`),
      ...(warnings.length > 0 ? ["Checkpoint warnings:", ...warnings.map((warning) => `- ${warning}`)] : []),
    ].join("\n");
  }

  if (result.operation === "show") {
    if (!result.ok || !result.checkpoint) {
      return `Checkpoint show failed\nError: ${result.error || "Checkpoint not found."}`;
    }
    return [
      `Checkpoint ${result.checkpoint.checkpoint_id}`,
      `Feature: ${result.feature}`,
      `Path: ${result.checkpoint.path}`,
      `Summary: ${result.checkpoint.summary || "(none)"}`,
      `Next action: ${result.checkpoint.next_action || "(none)"}`,
      `Phase: ${result.checkpoint.captured?.phase || "(none)"}`,
      `Gate: ${result.checkpoint.captured?.gate || "(none)"}`,
    ].join("\n");
  }

  if (result.operation === "diff") {
    if (!result.ok || !result.diff) {
      return `Checkpoint diff failed\nError: ${result.error || "Two checkpoints are required for diff."}`;
    }
    const changed = Object.entries(result.diff.fields)
      .filter(([, field]) => field.changed)
      .map(([key, field]) => `- ${key}: ${field.before} -> ${field.after}`);
    return [
      `Checkpoint diff for ${result.feature}`,
      `From: ${result.diff.from?.checkpoint_id || "(missing)"}`,
      `To: ${result.diff.to?.checkpoint_id || "(missing)"}`,
      ...(changed.length > 0 ? changed : ["- no changed fields"]),
    ].join("\n");
  }

  if (result.operation === "resume-brief") {
    if (!result.ok || !result.resume_brief) {
      return `Resume brief failed\nError: ${result.error || "Checkpoint not found."}`;
    }
    return [
      `Resume brief for ${result.feature}`,
      `Checkpoint: ${result.resume_brief.checkpoint?.checkpoint_id || "(missing)"}`,
      `Note: ${result.resume_brief.note}`,
      "Next reads:",
      ...result.resume_brief.next_reads.map((item) => `- ${item}`),
    ].join("\n");
  }

  return JSON.stringify(result, null, 2);
}

async function runCheckpointCommand(repoRoot, args) {
  const options = {
    feature: args.feature,
    checkpoint_id: args.checkpoint_id,
    path: args.path,
    from: args.from,
    to: args.to,
    summary: args.summary,
    next_action: args.next_action,
  };

  switch (args.operation) {
    case "save":
      return checkpointSave(repoRoot, options);
    case "list":
      return checkpointList(repoRoot, options);
    case "show":
      return checkpointShow(repoRoot, options);
    case "diff":
      return checkpointDiff(repoRoot, options);
    case "resume-brief":
      return checkpointResumeBrief(repoRoot, options);
    default:
      throw new Error(`Unknown checkpoint operation: ${args.operation || "(missing)"}`);
  }
}

export async function main(argv = process.argv.slice(2)) {
  const args = parseCliArgs(argv);
  const repoRoot = resolveRepoRoot(args.repoRoot, SCRIPT_DIR);

  if (args.command === "checkpoint") {
    const result = await runCheckpointCommand(repoRoot, args);
    process.stdout.write(args.json ? `${JSON.stringify(result, null, 2)}\n` : `${renderCheckpointResult(result)}\n`);
    return result.ok === false ? 1 : 0;
  }

  if (args.sync) {
    syncPulseRuntimeArtifacts(repoRoot);
  }

  const status = await readPulseStatus(repoRoot);
  process.stdout.write(
    args.json ? `${JSON.stringify(status, null, 2)}\n` : `${renderPulseStatus(status)}\n`,
  );
  return 0;
}

if (process.argv[1]) {
  process.exitCode = await main();
}
