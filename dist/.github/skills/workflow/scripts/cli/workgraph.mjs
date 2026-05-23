/**
 * Purpose: CLI facade for Pulse workgraph item and relationship operations.
 * Caller/flow: Invoked by operators/workers through `pulse.mjs workgraph ...`; ready also delegates here.
 * Reads/Writes: Reads/writes .pulse/workgraph items, derived views, and related work content via workgraph/service.mjs.
 * CLI args: create|show|list|ready|update|close|reopen|dep|link|children|graph|doctor with --repo-root and command options.
 * Ownership: Argument parsing and human rendering only; model, validation, locking, and persistence live under workgraph/*.mjs.
 * Repo root rule: Uses shared resolver from core/paths.mjs.
 */

import { resolveRepoRoot as resolveSharedRepoRoot } from "../core/paths.mjs";
import { normalizeNullableString } from "../workgraph/model.mjs";
import { assertBareBooleanOptions, assertKnownOptions, parseCliArgs as parseArgv } from "./args.mjs";
import { normalizeIo, writePayload } from "./io.mjs";
import {
  WORKGRAPH_COMMANDS,
  childrenOf,
  closeItem,
  createItem,
  doctor,
  graph,
  listItems,
  mutateDependencies,
  mutateLinks,
  readyItems,
  reopenItem,
  showItem,
  updateItem,
} from "../workgraph/service.mjs";

function resolveRepoRoot(explicitRoot, env = process.env, cwd = process.cwd()) {
  return resolveSharedRepoRoot({ explicitRoot, env, cwd });
}

function takeOption(parsed, name, fallback = undefined) {
  return parsed.options.has(name) ? parsed.options.get(name) : fallback;
}

function takeListOption(parsed, name) {
  const value = takeOption(parsed, name);
  if (value === undefined) {
    return [];
  }
  return Array.isArray(value) ? value : [value];
}

function takeBooleanOption(parsed, name) {
  return Boolean(takeOption(parsed, name, false));
}

function requirePositional(parsed, index, label) {
  const value = parsed.positionals[index];
  if (!value) {
    throw new Error(`Missing required argument: ${label}`);
  }
  return value;
}

function assertPositionalCount(parsed, count) {
  if (parsed.positionals.length > count) {
    throw new Error(`Unknown argument: ${parsed.positionals[count]}`);
  }
}

function parseWorkgraphArgs(argv) {
  const parsed = parseArgv(argv);
  assertKnownOptions(parsed, [
    "repo-root",
    "json",
    "kind",
    "title",
    "parent",
    "owner",
    "priority",
    "label",
    "risk",
    "status",
    "epic",
    "slug",
    "clear-owner",
    "blocked-reason",
    "clear-blocked-reason",
    "add-label",
    "rm-label",
    "add-risk",
    "rm-risk",
    "fix",
  ]);
  assertBareBooleanOptions(parsed, ["json", "clear-owner", "clear-blocked-reason", "fix"]);
  return parsed;
}

function assertCommandOptions(parsed, allowed = []) {
  assertKnownOptions(parsed, ["repo-root", "json", ...allowed]);
}

function renderHumanSummary(result) {
  if (result.command === "create") {
    return [
      `Created ${result.item.kind} ${result.item.id}`,
      `Title: ${result.item.title}`,
      `Path: ${result.item.content_path}`,
    ].join("\n");
  }

  if (result.command === "show") {
    const item = result.item;
    return [
      `${item.kind} ${item.id}`,
      `Title: ${item.title}`,
      `Status: ${item.status}`,
      `Ready: ${item.ready ? "yes" : "no"}`,
      `Children: ${item.children.join(", ") || "(none)"}`,
      `Dependencies: ${item.depends_on.join(", ") || "(none)"}`,
      ...(Array.isArray(item.linked_items) ? [`Linked items: ${item.linked_items.join(", ") || "(none)"}`] : []),
      ...(Array.isArray(item.reverse_links) ? [`Reverse links: ${item.reverse_links.join(", ") || "(none)"}`] : []),
    ].join("\n");
  }

  if (result.command === "list" || result.command === "ready" || result.command === "children") {
    return result.items.length === 0
      ? "No matching items."
      : result.items.map((item) => `${item.id} [${item.status}] ${item.title}`).join("\n");
  }

  if (result.command === "update" || result.command === "close" || result.command === "reopen") {
    return [`Updated ${result.item.id}`, `Status: ${result.item.status}`, `Path: ${result.item.content_path}`].join("\n");
  }

  if (result.command === "dep_add") {
    return `Added dependency ${result.dependency_id} -> ${result.item.id}`;
  }

  if (result.command === "dep_rm") {
    return `Removed dependency ${result.dependency_id} from ${result.item.id}`;
  }

  if (result.command === "link_add") {
    return `Added link ${result.item.id} -> ${result.linked_item_id}`;
  }

  if (result.command === "link_rm") {
    return `Removed link ${result.linked_item_id} from ${result.item.id}`;
  }

  if (result.command === "graph") {
    const linkCount = Array.isArray(result.graph.edges.links) ? result.graph.edges.links.length : 0;
    return `Graph: ${result.graph.nodes.length} nodes, ${result.graph.edges.hierarchy.length} hierarchy edges, ${result.graph.edges.dependencies.length} dependency edges, ${linkCount} link edges`;
  }

  if (result.command === "doctor") {
    if (result.ok) {
      return result.fixed_actions.length > 0
        ? `Doctor fixed ${result.fixed_actions.length} issue(s).`
        : "Doctor found no issues.";
    }
    return [
      `Doctor found ${result.issues.length} issue(s).`,
      ...result.issues.map((issue) => `- [${issue.code}] ${issue.message}`),
    ].join("\n");
  }

  if (result.command === "help") {
    return renderWorkgraphHelp();
  }

  return JSON.stringify(result, null, 2);
}

function renderWorkgraphHelp() {
  return [
    "Usage: pulse.mjs workgraph <command> [options]",
    "",
    "Maintain canonical Pulse work items, hierarchy, dependencies, cross-links, and workgraph health.",
    "Common options: --repo-root <repo>, --json",
    "",
    "Commands:",
    "  create --kind <kind> --title <title> [--parent <id>] [--owner <owner>] [--priority <n>] [--label <label>] [--risk <flag>]",
    "      Create an epic/story/task/bug item and optional parent, ownership, priority, labels, and risk flags.",
    "  show <id>",
    "      Print one work item with metadata, hierarchy, dependencies, labels, risks, and content paths.",
    "  list [--kind <kind>] [--status <status>] [--epic <id>] [--parent <id>] [--owner <owner>] [--label <label>]",
    "      Filter work items for planning, review, or operational triage.",
    "  ready",
    "      Show unblocked open items whose dependencies are complete and that are eligible for execution.",
    "  update <id> [--title <title>] [--slug <slug>] [--status <status>] [--priority <n>] [--owner <owner>] [--clear-owner]",
    "              [--blocked-reason <text>] [--clear-blocked-reason] [--add-label <label>] [--rm-label <label>] [--add-risk <flag>] [--rm-risk <flag>]",
    "      Mutate item metadata without editing workgraph files by hand.",
    "  close <id>",
    "      Mark an item complete/closed after verification artifacts are in place.",
    "  reopen <id>",
    "      Return a closed item to active status when follow-up work is required.",
    "  dep add <id> <depends-on> | dep rm <id> <depends-on>",
    "      Add or remove dependency edges that control readiness.",
    "  link add <id> <linked-item> | link rm <id> <linked-item>",
    "      Add or remove non-blocking related-item links.",
    "  children <id>",
    "      List direct child items under an epic/story/task parent.",
    "  graph",
    "      Summarize graph nodes plus hierarchy, dependency, and link edges.",
    "  doctor [--fix]",
    "      Validate workgraph integrity and optionally repair supported issues.",
  ].join("\n");
}

function helpPayload() {
  return {
    command: "help",
    ok: true,
    commands: WORKGRAPH_COMMANDS,
  };
}

async function dispatchCommand(repoRoot, parsed) {
  const command = parsed.positionals[0] || "help";

  switch (command) {
    case "create":
      assertCommandOptions(parsed, ["kind", "title", "parent", "owner", "priority", "label", "risk"]);
      assertPositionalCount(parsed, 1);
      return createItem(repoRoot, {
        kind: takeOption(parsed, "kind", ""),
        title: takeOption(parsed, "title"),
        parent: takeOption(parsed, "parent"),
        owner: takeOption(parsed, "owner"),
        priority: takeOption(parsed, "priority"),
        labels: takeListOption(parsed, "label"),
        riskFlags: takeListOption(parsed, "risk"),
      });
    case "show":
      assertCommandOptions(parsed);
      assertPositionalCount(parsed, 2);
      return showItem(repoRoot, { id: requirePositional(parsed, 1, "id") });
    case "list":
      assertCommandOptions(parsed, ["kind", "status", "epic", "parent", "owner", "label"]);
      assertPositionalCount(parsed, 1);
      return listItems(repoRoot, {
        kind: takeOption(parsed, "kind"),
        status: takeOption(parsed, "status"),
        epic: takeOption(parsed, "epic"),
        parent: takeOption(parsed, "parent"),
        owner: takeOption(parsed, "owner"),
        ownerProvided: parsed.options.has("owner"),
        label: takeOption(parsed, "label"),
      });
    case "ready":
      assertCommandOptions(parsed);
      assertPositionalCount(parsed, 1);
      return readyItems(repoRoot);
    case "update":
      assertCommandOptions(parsed, [
        "title",
        "slug",
        "status",
        "owner",
        "clear-owner",
        "blocked-reason",
        "clear-blocked-reason",
        "add-label",
        "rm-label",
        "add-risk",
        "rm-risk",
        "priority",
      ]);
      assertPositionalCount(parsed, 2);
      return updateItem(repoRoot, {
        id: requirePositional(parsed, 1, "id"),
        title: takeOption(parsed, "title"),
        slug: takeOption(parsed, "slug"),
        status: takeOption(parsed, "status"),
        owner: takeOption(parsed, "owner"),
        ownerProvided: parsed.options.has("owner"),
        clearOwner: takeBooleanOption(parsed, "clear-owner"),
        blockedReason: takeOption(parsed, "blocked-reason"),
        blockedReasonProvided: parsed.options.has("blocked-reason"),
        clearBlockedReason: takeBooleanOption(parsed, "clear-blocked-reason"),
        addLabels: takeListOption(parsed, "add-label"),
        removeLabels: takeListOption(parsed, "rm-label"),
        addRisks: takeListOption(parsed, "add-risk"),
        removeRisks: takeListOption(parsed, "rm-risk"),
        priority: takeOption(parsed, "priority"),
        priorityProvided: parsed.options.has("priority"),
      });
    case "close":
      assertCommandOptions(parsed);
      assertPositionalCount(parsed, 2);
      return closeItem(repoRoot, { id: requirePositional(parsed, 1, "id") });
    case "reopen":
      assertCommandOptions(parsed);
      assertPositionalCount(parsed, 2);
      return reopenItem(repoRoot, { id: requirePositional(parsed, 1, "id") });
    case "dep": {
      assertCommandOptions(parsed);
      assertPositionalCount(parsed, 4);
      const mode = parsed.positionals[1];
      return mutateDependencies(repoRoot, {
        mode,
        id: requirePositional(parsed, 2, "id"),
        dependencyId: requirePositional(parsed, 3, "depends-on"),
      });
    }
    case "link": {
      assertCommandOptions(parsed);
      assertPositionalCount(parsed, 4);
      const mode = parsed.positionals[1];
      return mutateLinks(repoRoot, {
        mode,
        id: requirePositional(parsed, 2, "id"),
        linkedItemId: requirePositional(parsed, 3, "linked-item"),
      });
    }
    case "children":
      assertCommandOptions(parsed);
      assertPositionalCount(parsed, 2);
      return childrenOf(repoRoot, { id: requirePositional(parsed, 1, "id") });
    case "graph":
      assertCommandOptions(parsed);
      assertPositionalCount(parsed, 1);
      return graph(repoRoot);
    case "doctor":
      assertCommandOptions(parsed, ["fix"]);
      assertPositionalCount(parsed, 1);
      return doctor(repoRoot, { fix: takeBooleanOption(parsed, "fix") });
    case "help":
      assertCommandOptions(parsed);
      assertPositionalCount(parsed, 1);
      return helpPayload();
    default:
      throw new Error(`Unknown command: ${command}`);
  }
}

export async function main(argv = process.argv.slice(2), context = {}) {
  const io = normalizeIo(context.io);
  if (argv.includes("--help") || argv.includes("-h")) {
    io.stdout.write(`${renderWorkgraphHelp()}\n`);
    return 0;
  }

  let asJson = argv.includes("--json");
  try {
    const parsed = parseWorkgraphArgs(argv);
    asJson = takeBooleanOption(parsed, "json");
    const repoRoot = resolveRepoRoot(normalizeNullableString(takeOption(parsed, "repo-root")), context.env, context.cwd);
    const result = await dispatchCommand(repoRoot, parsed);
    writePayload(result, { json: asJson, render: renderHumanSummary, output: io.stdout });
    return result.ok === false ? 1 : 0;
  } catch (error) {
    const payload = {
      ok: false,
      error: error.message,
      issues: error.issues || [],
    };
    writePayload(payload, { json: asJson, render: () => `Error: ${error.message}`, output: io.stdout });
    return 1;
  }
}

export { main as workgraphMain };
