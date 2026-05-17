#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

import { readPulseStatus } from "../.pulse/scripts/pulse_state.mjs";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const repoRoot = path.resolve(__dirname, "..");
const DEFAULT_WORKSPACE = path.join(repoRoot, "pulse-eval-workspace");
const DEFAULT_CONFIG = path.join(repoRoot, ".plugin-eval", "benchmark.json");
const PHASE_ORDER = ["static", "runtime", "scout", "benchmark", "scenarios"];
const DEFAULT_EVIDENCE_FILE = "benchmark-evidence.json";
const DEFAULT_PACKET_FILE = "benchmark-packet.md";
const CHAT_RUN_NOTE = "chat-run benchmark: scenario execution and scoring were captured in the interactive session, not spawned headlessly by the evaluator.";
const DISCRIMINATION_NOTE = "non-discriminating signal unavailable in chat-run v2: no normalized baseline is collected by the evaluator.";

function parseArgs(argv) {
  const subcommands = new Set(["run", "analyze", "scout", "benchmark"]);
  const args = {
    command: "run",
    workspace: DEFAULT_WORKSPACE,
    config: DEFAULT_CONFIG,
    runCmd: null,
    iteration: null,
    evalIds: null,
    evidence: null,
    runtime: null,
    skipStatic: false,
    skipRuntime: false,
    skipScenarios: false,
    strict: false,
    json: false,
    help: false,
  };

  let index = 0;
  if (argv[0] && subcommands.has(argv[0])) {
    args.command = argv[0];
    index = 1;
  }

  for (; index < argv.length; index += 1) {
    const token = argv[index];
    if (token === "--workspace") args.workspace = path.resolve(argv[++index]);
    else if (token === "--config") args.config = path.resolve(argv[++index]);
    else if (token === "--run-cmd") args.runCmd = argv[++index];
    else if (token === "--iteration") args.iteration = argv[++index];
    else if (token === "--eval-ids") args.evalIds = argv[++index];
    else if (token === "--evidence") args.evidence = path.resolve(argv[++index]);
    else if (token === "--runtime") args.runtime = argv[++index];
    else if (token === "--skip-static") args.skipStatic = true;
    else if (token === "--skip-runtime") args.skipRuntime = true;
    else if (token === "--skip-scenarios") args.skipScenarios = true;
    else if (token === "--strict") args.strict = true;
    else if (token === "--json") args.json = true;
    else if (token === "--help" || token === "-h") args.help = true;
    else throw new Error(`Unknown option: ${token}`);
  }

  return args;
}

function printHelp() {
  console.log(`Pulse plugin evaluation runner

Usage:
  node scripts/pulse-plugin-eval.mjs [options]
  node scripts/pulse-plugin-eval.mjs run [options]
  node scripts/pulse-plugin-eval.mjs analyze [options]
  node scripts/pulse-plugin-eval.mjs scout [options]
  node scripts/pulse-plugin-eval.mjs benchmark [options]

Commands:
  run                     Run all evaluator stages (default)
  analyze                 Run static and runtime checks only
  scout                   Run Pulse readiness scout only
  benchmark               Prepare chat benchmark artifacts or finalize from evidence

Options:
  --workspace <path>      Eval workspace (default: pulse-eval-workspace)
  --config <path>         Benchmark config (default: .plugin-eval/benchmark.json)
  --run-cmd <command>     Optional command to execute during runtime checks
  --iteration <name|num>  Explicit iteration (e.g. iteration-6 or 6)
  --eval-ids <list>       Comma-separated eval IDs to select (e.g. 7,8,19)
  --evidence <path>       Filled benchmark evidence JSON to finalize into benchmark artifacts
  --runtime <value>       Deprecated in chat-run mode; accepted but ignored
  --skip-static           Skip static checks (run only)
  --skip-runtime          Skip runtime/scout checks (run only)
  --skip-scenarios        Skip benchmark/scenario checks (run only)
  --strict                Exit non-zero when any WARN occurs
  --json                  Print JSON summary only
  --help                  Show this help
`);
}

function exists(targetPath) {
  return fs.existsSync(targetPath);
}

function readJson(targetPath) {
  return JSON.parse(fs.readFileSync(targetPath, "utf8"));
}

function writeJson(targetPath, value) {
  fs.writeFileSync(targetPath, `${JSON.stringify(value, null, 2)}\n`, "utf8");
}

function addResult(results, phase, name, status, detail) {
  results.push({ phase, name, status, detail });
}

function commandOnPath(command) {
  const out = spawnSync("sh", ["-c", `command -v ${command}`], { encoding: "utf8" });
  return out.status === 0 ? out.stdout.trim().split("\n")[0] || "" : "";
}

function runCommand(command, args, options = {}) {
  return spawnSync(command, args, {
    encoding: "utf8",
    cwd: options.cwd ?? repoRoot,
    env: options.env ?? process.env,
    maxBuffer: 20 * 1024 * 1024,
  });
}

function normalizeIterationName(iterationArg) {
  if (!iterationArg) return null;
  if (iterationArg.startsWith("iteration-")) return iterationArg;
  if (/^\d+$/.test(iterationArg)) return `iteration-${iterationArg}`;
  return iterationArg;
}

function listIterations(workspacePath) {
  if (!exists(workspacePath)) return [];
  return fs
    .readdirSync(workspacePath, { withFileTypes: true })
    .filter((entry) => entry.isDirectory() && /^iteration-\d+$/.test(entry.name))
    .map((entry) => entry.name)
    .sort((a, b) => Number(a.split("-")[1]) - Number(b.split("-")[1]));
}

function findLatestIteration(workspacePath) {
  const iterations = listIterations(workspacePath);
  return iterations[iterations.length - 1] ?? null;
}

function nextIterationName(workspacePath) {
  const iterations = listIterations(workspacePath);
  const last = iterations[iterations.length - 1];
  if (!last) return "iteration-1";
  return `iteration-${Number(last.split("-")[1]) + 1}`;
}

function parseEvalIds(csv) {
  if (!csv) return null;
  return csv
    .split(",")
    .map((value) => Number(value.trim()))
    .filter((value) => Number.isInteger(value) && value > 0);
}

function relativeToRepo(targetPath) {
  return path.relative(repoRoot, targetPath) || ".";
}

function buildOutputSnippet(text) {
  const normalized = String(text || "").trim();
  if (normalized.length <= 600) return normalized;
  return `${normalized.slice(0, 600)}…`;
}

function loadBenchmarkConfig(configPath) {
  if (!exists(configPath)) {
    throw new Error(`Benchmark config not found: ${relativeToRepo(configPath)}`);
  }
  return readJson(configPath);
}

function loadScenarioLibrary(config, workspacePath) {
  const sourcePath = config.scenarioSource
    ? path.resolve(repoRoot, config.scenarioSource)
    : path.join(workspacePath, "evals.json");
  if (!exists(sourcePath)) {
    throw new Error(`Scenario source not found: ${relativeToRepo(sourcePath)}`);
  }
  const payload = readJson(sourcePath);
  return { sourcePath, scenarios: Array.isArray(payload.evals) ? payload.evals : [] };
}

function selectScenarios(scenarios, requestedIds, defaultIds) {
  const selectedIds = requestedIds && requestedIds.length > 0 ? requestedIds : defaultIds;
  if (!selectedIds || selectedIds.length === 0) {
    return scenarios;
  }
  const byId = new Map(scenarios.map((scenario) => [scenario.id, scenario]));
  return selectedIds.map((id) => byId.get(id)).filter(Boolean);
}

function runVerifierCommands(commands, cwd) {
  return (commands || []).map((command) => {
    const out = runCommand("sh", ["-c", command], { cwd });
    return {
      command,
      status: out.status === 0 ? "PASS" : "FAIL",
      exit_status: out.status ?? 1,
      stdout: buildOutputSnippet(out.stdout),
      stderr: buildOutputSnippet(out.stderr),
    };
  });
}

function buildEvidencePath(args, iterationPath) {
  return args.evidence || path.join(iterationPath, DEFAULT_EVIDENCE_FILE);
}

function buildPacketMarkdown({ iterationName, evidencePath, selectedScenarios }) {
  const lines = [
    `# Pulse Benchmark Packet ${iterationName}`,
    "",
    "Run each scenario in the current interactive chat.",
    "",
    "After each answer:",
    "1. paste the assistant response into the matching entry in `benchmark-evidence.json`",
    "2. fill the `judge` block in the same file from the current chat session",
    "3. rerun `node scripts/pulse-plugin-eval.mjs benchmark --evidence <path>` to compile final artifacts",
    "",
    `Evidence template: ${relativeToRepo(evidencePath)}`,
    "",
  ];

  for (const scenario of selectedScenarios) {
    lines.push(`## Scenario #${scenario.id} — ${scenario.skill}`, "");
    lines.push("### User prompt", "", scenario.prompt, "");
    lines.push("### Expected output", "", scenario.expected_output || "(none)", "");
    lines.push("### Checklist", "");
    for (const expectation of scenario.expectations || []) {
      lines.push(`- ${expectation}`);
    }
    lines.push("");
  }

  return `${lines.join("\n")}\n`;
}

function buildEvidenceTemplate({ configPath, sourcePath, iterationName, selectedScenarios }) {
  return {
    kind: "pulse-chat-benchmark-evidence",
    schemaVersion: 1,
    created_at: new Date().toISOString(),
    iteration: iterationName,
    config_path: relativeToRepo(configPath),
    scenario_source: relativeToRepo(sourcePath),
    selected_scenario_ids: selectedScenarios.map((scenario) => scenario.id),
    runner: {
      mode: "chat",
      label: "current interactive chat",
    },
    notes: [],
    scenarios: selectedScenarios.map((scenario) => ({
      id: scenario.id,
      skill: scenario.skill,
      skill_path: scenario.skill_path,
      prompt: scenario.prompt,
      expected_output: scenario.expected_output,
      expectations: scenario.expectations,
      response: "",
      judge: {
        passed: null,
        score: null,
        summary: "",
        satisfied: [],
        missed: [],
        warnings: [],
      },
      notes: [],
    })),
  };
}

function sanitizeStringList(value) {
  if (!Array.isArray(value)) return [];
  return value.map((entry) => String(entry)).filter(Boolean);
}

function sanitizeJudge(rawJudge) {
  if (!rawJudge || typeof rawJudge !== "object") return null;
  if (typeof rawJudge.passed !== "boolean") return null;
  const summary = typeof rawJudge.summary === "string" ? rawJudge.summary.trim() : "";
  if (!summary) return null;
  return {
    passed: rawJudge.passed,
    score: typeof rawJudge.score === "number" ? rawJudge.score : null,
    summary,
    satisfied: sanitizeStringList(rawJudge.satisfied),
    missed: sanitizeStringList(rawJudge.missed),
    warnings: sanitizeStringList(rawJudge.warnings),
  };
}

function buildScenarioEntries(selectedScenarios, evidence) {
  const evidenceEntries = Array.isArray(evidence?.scenarios) ? evidence.scenarios : [];
  const evidenceById = new Map(evidenceEntries.map((entry) => [entry.id, entry]));

  return selectedScenarios.map((scenario) => {
    const evidenceEntry = evidenceById.get(scenario.id) || {};
    const response = typeof evidenceEntry.response === "string" ? evidenceEntry.response.trim() : "";
    const judge = sanitizeJudge(evidenceEntry.judge);
    const notes = sanitizeStringList(evidenceEntry.notes);
    let status = "pending";
    if (response && judge) status = "completed";
    else if (response) status = "partial";

    return {
      id: scenario.id,
      skill: scenario.skill,
      skill_path: scenario.skill_path,
      prompt: scenario.prompt,
      expected_output: scenario.expected_output,
      expectations: scenario.expectations,
      status,
      response,
      response_snippet: buildOutputSnippet(response),
      judge,
      notes,
    };
  });
}

function buildScenarioSummary(entries) {
  const count = entries.length;
  const completedEntries = entries.filter((entry) => entry.status === "completed");
  const passed = completedEntries.filter((entry) => entry.judge?.passed).length;
  const failed = completedEntries.filter((entry) => entry.judge && !entry.judge.passed).length;
  const partial = entries.filter((entry) => entry.status === "partial").length;
  const pending = entries.filter((entry) => entry.status === "pending").length;
  const completed = completedEntries.length;
  const incomplete = count - completed;
  return {
    count,
    completed,
    passed,
    failed,
    partial,
    pending,
    incomplete,
    pass_rate: completed > 0 ? Number((passed / completed).toFixed(4)) : null,
  };
}

function buildCoverage(selectedScenarios, evidence) {
  const selectedIds = new Set(selectedScenarios.map((scenario) => scenario.id));
  const evidenceIds = new Set(Array.isArray(evidence?.scenarios) ? evidence.scenarios.map((entry) => entry.id) : []);
  return {
    missing: [...selectedIds].filter((id) => !evidenceIds.has(id)),
    extra: [...evidenceIds].filter((id) => !selectedIds.has(id)),
  };
}

function buildBenchmarkMarkdown(benchmark) {
  const summary = benchmark.run_summary || {};
  const lines = [
    `# Pulse Benchmark ${benchmark.iteration}`,
    "",
    `Generated: ${benchmark.created_at}`,
    `Scenario source: ${benchmark.scenario_source}`,
    `Evidence: ${benchmark.evidence_path || "(none)"}`,
    `Runner mode: ${benchmark.runner?.mode || "unknown"}`,
    `Selected scenario IDs: ${benchmark.selected_scenario_ids.join(", ") || "(all)"}`,
    "",
    "## Summary",
    "",
    "| Count | Completed | Passed | Failed | Partial | Pending | Pass rate |",
    "|---:|---:|---:|---:|---:|---:|---:|",
    `| ${summary.count ?? 0} | ${summary.completed ?? 0} | ${summary.passed ?? 0} | ${summary.failed ?? 0} | ${summary.partial ?? 0} | ${summary.pending ?? 0} | ${summary.pass_rate ?? "n/a"} |`,
    "",
    "## Scenario Results",
    "",
  ];

  for (const scenario of benchmark.scenarios || []) {
    const marker = scenario.status === "completed"
      ? scenario.judge?.passed ? "PASS" : "FAIL"
      : scenario.status === "partial"
        ? "PARTIAL"
        : "PENDING";
    lines.push(`- [${marker}] #${scenario.id} ${scenario.skill} — ${scenario.judge?.summary || scenario.status}`);
    if (scenario.judge?.missed?.length) {
      lines.push(`  - Missed: ${scenario.judge.missed.join("; ")}`);
    }
    if (scenario.judge?.warnings?.length) {
      lines.push(`  - Warnings: ${scenario.judge.warnings.join("; ")}`);
    }
    if (scenario.notes?.length) {
      lines.push(`  - Notes: ${scenario.notes.join("; ")}`);
    }
  }

  lines.push("", "## Notes", "");
  for (const note of benchmark.notes || []) {
    lines.push(`- ${note}`);
  }

  return `${lines.join("\n")}\n`;
}

function buildBenchmarkHtml(benchmark) {
  const summary = benchmark.run_summary || {};
  const scenarioRows = (benchmark.scenarios || [])
    .map((scenario) => {
      const status = scenario.status === "completed"
        ? scenario.judge?.passed ? "PASS" : "FAIL"
        : scenario.status.toUpperCase();
      return `<tr><td>#${scenario.id}</td><td>${escapeHtml(scenario.skill)}</td><td>${escapeHtml(status)}</td><td>${escapeHtml(scenario.judge?.summary || scenario.status)}</td></tr>`;
    })
    .join("");
  const notes = (benchmark.notes || []).map((note) => `<li>${escapeHtml(note)}</li>`).join("");

  return `<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8" />
  <title>Pulse Eval Review</title>
  <style>
    body { font-family: ui-sans-serif, system-ui, sans-serif; margin: 2rem; }
    table { border-collapse: collapse; width: 100%; margin-bottom: 1.5rem; }
    th, td { border: 1px solid #d4d4d8; padding: 0.5rem; text-align: left; vertical-align: top; }
    th { background: #f4f4f5; }
    code { background: #f4f4f5; padding: 0.1rem 0.3rem; }
  </style>
</head>
<body>
  <h1>Pulse Eval Review</h1>
  <p><strong>Iteration:</strong> ${escapeHtml(benchmark.iteration)}</p>
  <p><strong>Generated:</strong> ${escapeHtml(benchmark.created_at)}</p>
  <p><strong>Scenario source:</strong> <code>${escapeHtml(benchmark.scenario_source)}</code></p>
  <p><strong>Evidence:</strong> <code>${escapeHtml(benchmark.evidence_path || "(none)")}</code></p>
  <p><strong>Runner mode:</strong> ${escapeHtml(benchmark.runner?.mode || "unknown")}</p>
  <h2>Summary</h2>
  <table>
    <thead>
      <tr><th>Count</th><th>Completed</th><th>Passed</th><th>Failed</th><th>Partial</th><th>Pending</th><th>Pass rate</th></tr>
    </thead>
    <tbody>
      <tr>
        <td>${summary.count ?? 0}</td>
        <td>${summary.completed ?? 0}</td>
        <td>${summary.passed ?? 0}</td>
        <td>${summary.failed ?? 0}</td>
        <td>${summary.partial ?? 0}</td>
        <td>${summary.pending ?? 0}</td>
        <td>${summary.pass_rate ?? "n/a"}</td>
      </tr>
    </tbody>
  </table>
  <h2>Scenario Results</h2>
  <table>
    <thead>
      <tr><th>Scenario</th><th>Skill</th><th>Status</th><th>Summary</th></tr>
    </thead>
    <tbody>${scenarioRows}</tbody>
  </table>
  <h2>Notes</h2>
  <ul>${notes}</ul>
</body>
</html>
`;
}

function escapeHtml(text) {
  return String(text)
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#39;");
}

function prepareBenchmark(results, args, config, sourcePath, selectedScenarios, iterationName, iterationPath) {
  const evidencePath = buildEvidencePath(args, iterationPath);
  const packetPath = path.join(iterationPath, DEFAULT_PACKET_FILE);
  fs.mkdirSync(iterationPath, { recursive: true });

  const packet = buildPacketMarkdown({ iterationName, evidencePath, selectedScenarios });
  fs.writeFileSync(packetPath, packet, "utf8");
  addResult(results, "benchmark", "artifact:benchmark-packet", "PASS", relativeToRepo(packetPath));

  if (!exists(evidencePath)) {
    const template = buildEvidenceTemplate({
      configPath: args.config,
      sourcePath,
      iterationName,
      selectedScenarios,
    });
    writeJson(evidencePath, template);
    addResult(results, "benchmark", "artifact:benchmark-evidence-template", "PASS", relativeToRepo(evidencePath));
  } else {
    addResult(results, "benchmark", "artifact:benchmark-evidence-template", "WARN", `${relativeToRepo(evidencePath)} already exists; left unchanged`);
  }

  addResult(
    results,
    "benchmark",
    "benchmark-ready",
    "WARN",
    `Run scenarios in chat, fill ${relativeToRepo(evidencePath)}, then rerun with --evidence ${relativeToRepo(evidencePath)}`,
  );

  return {
    mode: "prepared",
    iterationName,
    iterationPath,
    packetPath,
    evidencePath,
    benchmarkJsonPath: null,
  };
}

function finalizeBenchmark(results, args, config, sourcePath, selectedScenarios, iterationName, iterationPath, evidencePath) {
  if (!exists(evidencePath)) {
    throw new Error(`Evidence file not found: ${relativeToRepo(evidencePath)}. Run benchmark without --evidence first to generate a template.`);
  }

  const evidence = readJson(evidencePath);
  const coverage = buildCoverage(selectedScenarios, evidence);
  const scenarioEntries = buildScenarioEntries(selectedScenarios, evidence);
  const summary = buildScenarioSummary(scenarioEntries);
  const packetPath = path.join(iterationPath, DEFAULT_PACKET_FILE);

  addResult(results, "benchmark", "artifact:benchmark-evidence", "PASS", relativeToRepo(evidencePath));
  addResult(
    results,
    "benchmark",
    "evidence-selected-ids",
    coverage.missing.length === 0 ? "PASS" : "WARN",
    coverage.missing.length === 0 ? "all selected scenarios have evidence entries" : `missing entries for: ${coverage.missing.join(",")}`,
  );
  if (coverage.extra.length > 0) {
    addResult(results, "benchmark", "evidence-extra-ids", "WARN", `extra evidence entries: ${coverage.extra.join(",")}`);
  }

  const benchmark = {
    kind: "pulse-benchmark-result",
    schemaVersion: 2,
    created_at: new Date().toISOString(),
    iteration: iterationName,
    config_path: relativeToRepo(args.config),
    scenario_source: relativeToRepo(sourcePath),
    evidence_path: relativeToRepo(evidencePath),
    selected_scenario_ids: selectedScenarios.map((scenario) => scenario.id),
    runner: {
      mode: "chat",
      label: typeof evidence?.runner?.label === "string" && evidence.runner.label.trim()
        ? evidence.runner.label.trim()
        : "current interactive chat",
    },
    scenarios: scenarioEntries,
    verifiers: runVerifierCommands(config.shared?.verifiers || [], repoRoot),
    run_summary: summary,
    notes: [
      ...(config.notes || []),
      ...sanitizeStringList(evidence.notes),
      CHAT_RUN_NOTE,
      DISCRIMINATION_NOTE,
    ],
  };

  for (const verifier of benchmark.verifiers) {
    addResult(results, "benchmark", `verifier:${verifier.command}`, verifier.status, verifier.stderr || verifier.stdout || "");
  }

  const benchmarkJsonPath = path.join(iterationPath, "benchmark.json");
  const benchmarkMdPath = path.join(iterationPath, "benchmark.md");
  const reviewHtmlPath = path.join(args.workspace, "pulse-eval-review.html");
  writeJson(benchmarkJsonPath, benchmark);
  fs.writeFileSync(benchmarkMdPath, buildBenchmarkMarkdown(benchmark), "utf8");
  fs.writeFileSync(reviewHtmlPath, buildBenchmarkHtml(benchmark), "utf8");

  if (exists(packetPath)) {
    addResult(results, "benchmark", "artifact:benchmark-packet", "PASS", relativeToRepo(packetPath));
  }
  addResult(results, "benchmark", "artifact:benchmark-json", "PASS", relativeToRepo(benchmarkJsonPath));
  addResult(results, "benchmark", "artifact:benchmark-md", "PASS", relativeToRepo(benchmarkMdPath));
  addResult(results, "benchmark", "artifact:review-html", "PASS", relativeToRepo(reviewHtmlPath));
  addResult(
    results,
    "benchmark",
    "benchmark-summary",
    summary.failed > 0 || summary.incomplete > 0 ? "WARN" : "PASS",
    `completed=${summary.completed}, passed=${summary.passed}, failed=${summary.failed}, incomplete=${summary.incomplete}`,
  );

  return {
    mode: "finalized",
    iterationName,
    iterationPath,
    evidencePath,
    packetPath: exists(packetPath) ? packetPath : null,
    benchmarkJsonPath,
  };
}

function runBenchmark(results, args) {
  const config = loadBenchmarkConfig(args.config);
  const { sourcePath, scenarios } = loadScenarioLibrary(config, args.workspace);
  const requestedIds = parseEvalIds(args.evalIds);
  const selectedScenarios = selectScenarios(scenarios, requestedIds, config.pilotScenarioIds);

  if (selectedScenarios.length === 0) {
    throw new Error("No scenarios selected for benchmark run.");
  }

  addResult(results, "benchmark", "benchmark-config", exists(args.config) ? "PASS" : "FAIL", relativeToRepo(args.config));
  addResult(results, "benchmark", "scenario-source", "PASS", relativeToRepo(sourcePath));
  addResult(results, "benchmark", "scenario-count", "PASS", `count=${selectedScenarios.length}`);
  addResult(results, "benchmark", "benchmark-mode", "PASS", "chat evidence workflow");
  if (args.runtime) {
    addResult(results, "benchmark", "deprecated-runtime-option", "WARN", `ignored in chat-run mode: ${args.runtime}`);
  }

  const iterationName = normalizeIterationName(args.iteration) || nextIterationName(args.workspace);
  const iterationPath = path.join(args.workspace, iterationName);
  fs.mkdirSync(iterationPath, { recursive: true });

  if (!args.evidence) {
    return prepareBenchmark(results, args, config, sourcePath, selectedScenarios, iterationName, iterationPath);
  }

  return finalizeBenchmark(results, args, config, sourcePath, selectedScenarios, iterationName, iterationPath, args.evidence);
}

function findSkillCountClaims(raw) {
  const claims = [];
  const lines = raw.split(/\r?\n/);
  const claimPattern = /\b(\d{1,4})\b(?:\s+[A-Za-z][\w-]*){0,2}\s+skills?\b/gi;

  lines.forEach((line, index) => {
    let match;
    while ((match = claimPattern.exec(line)) !== null) {
      claims.push({
        count: Number(match[1]),
        line: index + 1,
        snippet: line.trim(),
      });
    }
  });

  return claims;
}

function runStaticChecks(results) {
  const pluginManifestPath = path.join(repoRoot, ".codex-plugin/plugin.json");
  const claudePluginPath = path.join(repoRoot, ".claude-plugin/plugin.json");
  const claudeMarketplacePath = path.join(repoRoot, ".claude-plugin/marketplace.json");
  const claudeHooksPath = path.join(repoRoot, "hooks", "hooks.json");
  const codexHooksPath = path.join(repoRoot, "hooks", "codex-hooks.json");
  const claudeSessionStartHookPath = path.join(repoRoot, "hooks", "session-start.mjs");
  const marketplacePath = path.join(repoRoot, ".agents/plugins/marketplace.json");
  const mcpPath = path.join(repoRoot, ".mcp.json");
  const skillsRoot = path.join(repoRoot, "skills");

  const required = [
    pluginManifestPath,
    claudePluginPath,
    claudeMarketplacePath,
    claudeHooksPath,
    codexHooksPath,
    claudeSessionStartHookPath,
    marketplacePath,
    mcpPath,
    skillsRoot,
  ];
  for (const targetPath of required) {
    addResult(results, "static", `exists:${relativeToRepo(targetPath)}`, exists(targetPath) ? "PASS" : "FAIL", "");
  }

  if (!required.every(exists)) return;

  const manifest = readJson(pluginManifestPath);
  const claudePlugin = readJson(claudePluginPath);
  const claudeMarketplace = readJson(claudeMarketplacePath);
  const claudeHooks = readJson(claudeHooksPath);
  const codexHooks = readJson(codexHooksPath);
  const marketplace = readJson(marketplacePath);
  const mcp = readJson(mcpPath);

  const manifestVersion = manifest?.plugins?.[0]?.version ?? manifest?.version;
  const pluginVersion = claudePlugin?.version;
  const marketplaceVersion = marketplace?.plugins?.find((entry) => entry.name === "pulse")?.version;
  const claudeMarketplaceSource = claudeMarketplace?.plugins?.find((entry) => entry.name === "pulse")?.source;
  const marketplaceSourceRaw = marketplace?.plugins?.find((entry) => entry.name === "pulse")?.source;
  const marketplaceSource =
    typeof marketplaceSourceRaw === "string"
      ? marketplaceSourceRaw
      : marketplaceSourceRaw?.source === "url"
        ? marketplaceSourceRaw?.url
        : marketplaceSourceRaw?.path;

  addResult(
    results,
    "static",
    "version-sync",
    manifestVersion && pluginVersion && marketplaceVersion && manifestVersion === pluginVersion && pluginVersion === marketplaceVersion ? "PASS" : "WARN",
    `manifest=${manifestVersion ?? "n/a"}, plugin=${pluginVersion ?? "n/a"}, marketplace=${marketplaceVersion ?? "n/a"}`,
  );
  addResult(
    results,
    "static",
    "codex-plugin-hook-declaration",
    manifest?.hooks === "./hooks/codex-hooks.json" ? "PASS" : "WARN",
    `hooks=${manifest?.hooks ?? "n/a"}`,
  );
  const codexSessionStartEntries = Array.isArray(codexHooks?.hooks?.SessionStart)
    ? codexHooks.hooks.SessionStart
    : [];
  const codexPreToolUseEntries = Array.isArray(codexHooks?.hooks?.PreToolUse)
    ? codexHooks.hooks.PreToolUse
    : [];
  const codexStopEntries = Array.isArray(codexHooks?.hooks?.Stop)
    ? codexHooks.hooks.Stop
    : [];
  const codexSessionStartHook = codexSessionStartEntries[0]?.hooks?.[0] || null;
  const codexPreToolUseHook = codexPreToolUseEntries[0]?.hooks?.[0] || null;
  const codexStopHook = codexStopEntries[0]?.hooks?.[0] || null;
  addResult(
    results,
    "static",
    "codex-hook-registration",
    codexSessionStartEntries[0]?.matcher === "startup|resume"
      && codexPreToolUseEntries[0]?.matcher === "Bash"
      && codexStopEntries.length > 0
      ? "PASS"
      : "WARN",
    `SessionStart=${codexSessionStartEntries[0]?.matcher ?? "n/a"}, PreToolUse=${codexPreToolUseEntries[0]?.matcher ?? "n/a"}, Stop=${codexStopEntries.length > 0 ? "present" : "missing"}`,
  );
  addResult(
    results,
    "static",
    "codex-hook-command-contract",
    codexSessionStartHook?.command
        === 'node "$(git rev-parse --show-toplevel 2>/dev/null || pwd)/hooks/session-start.mjs"'
      && codexPreToolUseHook?.command
        === 'node "$(git rev-parse --show-toplevel 2>/dev/null || pwd)/hooks/pre-tool-use.mjs"'
      && codexStopHook?.command
        === 'node "$(git rev-parse --show-toplevel 2>/dev/null || pwd)/hooks/stop.mjs"'
      ? "PASS"
      : "WARN",
    `SessionStart=${codexSessionStartHook?.command ?? "n/a"}, PreToolUse=${codexPreToolUseHook?.command ?? "n/a"}, Stop=${codexStopHook?.command ?? "n/a"}`,
  );
  const expectedPackagedSkills = [
    "./skills/workflow",
    "./skills/architecture-rescue",
    "./skills/systematic-debug",
    "./skills/dev-note",
    "./skills/dev-note-distil",
    "./skills/prompt-leverage",
    "./skills/gitnexus",
  ];
  const declaredSkills = Array.isArray(claudePlugin?.skills) ? claudePlugin.skills : [];
  const packagedSkillsMatch =
    declaredSkills.length === expectedPackagedSkills.length
    && expectedPackagedSkills.every((skillPath) => declaredSkills.includes(skillPath));

  addResult(
    results,
    "static",
    "claude-plugin-path-contract",
    packagedSkillsMatch && claudePlugin?.mcpServers === "./.mcp.json" ? "PASS" : "WARN",
    `skills=${JSON.stringify(claudePlugin?.skills ?? "n/a")}, mcpServers=${claudePlugin?.mcpServers ?? "n/a"}`,
  );
  const claudeSessionStartEntries = Array.isArray(claudeHooks?.hooks?.SessionStart)
    ? claudeHooks.hooks.SessionStart
    : [];
  const claudeSessionStartHook = claudeSessionStartEntries[0]?.hooks?.[0] || null;
  addResult(
    results,
    "static",
    "claude-hook-registration",
    claudeSessionStartEntries.length > 0 && claudeSessionStartEntries[0]?.matcher === "startup|clear|compact"
      ? "PASS"
      : "WARN",
    `matcher=${claudeSessionStartEntries[0]?.matcher ?? "n/a"}`,
  );
  addResult(
    results,
    "static",
    "claude-hook-command-contract",
    claudeSessionStartHook?.command === 'node "${CLAUDE_PLUGIN_ROOT}/hooks/session-start.mjs"' && claudeSessionStartHook?.async === false
      ? "PASS"
      : "WARN",
    `command=${claudeSessionStartHook?.command ?? "n/a"}, async=${claudeSessionStartHook?.async ?? "n/a"}`,
  );
  addResult(
    results,
    "static",
    "claude-marketplace-source",
    claudeMarketplaceSource === "." || claudeMarketplaceSource === "./" ? "PASS" : "WARN",
    `source=${claudeMarketplaceSource ?? "n/a"}`,
  );
  addResult(
    results,
    "static",
    "codex-marketplace-source",
    marketplaceSource === "https://github.com/quanpersie2001/pulse.git" ? "PASS" : "WARN",
    `source=${marketplaceSource ?? "n/a"}`,
  );

  const skillDirs = fs.readdirSync(skillsRoot, { withFileTypes: true }).filter((entry) => entry.isDirectory());
  const missingSkillFiles = skillDirs
    .map((entry) => path.join(skillsRoot, entry.name, "SKILL.md"))
    .filter((targetPath) => !exists(targetPath));

  const actualSkillCount = skillDirs.length;
  const skillCountClaimFiles = [
    path.join(repoRoot, "README.md"),
    path.join(repoRoot, "CONTRIBUTING.md"),
    path.join(repoRoot, "CLAUDE.md"),
    marketplacePath,
  ];
  const driftDetails = [];

  for (const filePath of skillCountClaimFiles) {
    if (!exists(filePath)) {
      driftDetails.push(`${relativeToRepo(filePath)} (missing)`);
      continue;
    }

    const raw = fs.readFileSync(filePath, "utf8");
    const claims = findSkillCountClaims(raw);
    for (const claim of claims) {
      if (claim.count !== actualSkillCount) {
        driftDetails.push(`${relativeToRepo(filePath)}:${claim.line} claims ${claim.count} skills (actual=${actualSkillCount})`);
      }
    }
  }

  addResult(results, "static", "skills-discovered", skillDirs.length > 0 ? "PASS" : "FAIL", `count=${skillDirs.length}`);
  addResult(
    results,
    "static",
    "skill-count-claims-sync",
    driftDetails.length === 0 ? "PASS" : "WARN",
    driftDetails.length === 0 ? `all checked claims match actual=${actualSkillCount}` : driftDetails.join("; "),
  );
  addResult(
    results,
    "static",
    "skill-markdown-contract",
    missingSkillFiles.length === 0 ? "PASS" : "FAIL",
    missingSkillFiles.length === 0 ? "all skill dirs include SKILL.md" : missingSkillFiles.map(relativeToRepo).join(", "),
  );
  const mcpServers = Object.keys(mcp || {});
  addResult(results, "static", "mcp-servers-parse", mcpServers.length > 0 ? "PASS" : "WARN", `servers=${mcpServers.join(", ") || "none"}`);
}

function runRuntimeChecks(results, runCmd) {
  for (const command of ["git", "pulse-work"]) {
    addResult(results, "runtime", `command:${command}`, commandOnPath(command) ? "PASS" : "WARN", "");
  }

  if (runCmd) {
    const out = runCommand("sh", ["-c", runCmd]);
    const status = out.status === 0 ? "PASS" : "FAIL";
    const detail = [out.stdout, out.stderr].filter(Boolean).join("\n").trim().slice(0, 2000);
    addResult(results, "runtime", "scenario-run-command", status, detail || "(no output)");
  }
}

async function runScoutChecks(results) {
  const status = await readPulseStatus(repoRoot);
  addResult(
    results,
    "scout",
    "onboarding-status",
    status.onboarding.status === "complete" ? "PASS" : "WARN",
    `status=${status.onboarding.status || "missing"}, plugin_version=${status.onboarding.plugin_version || "n/a"}`,
  );
  addResult(
    results,
    "scout",
    "tooling-status",
    status.tooling_status.status && status.tooling_status.status.toLowerCase() !== "fail" ? "PASS" : "WARN",
    `status=${status.tooling_status.status || "missing"}, recommended_mode=${status.tooling_status.recommended_mode || "n/a"}`,
  );
  addResult(
    results,
    "scout",
    "gitnexus-readiness",
    status.gitnexus_readiness.configured ? "PASS" : "WARN",
    status.gitnexus_readiness.recommended_action || "n/a",
  );
  return status;
}

function runScenarioChecks(results, workspacePath, iterationArg, evalIdsArg) {
  const evalsPath = path.join(workspacePath, "evals.json");
  const reviewPath = path.join(workspacePath, "pulse-eval-review.html");

  addResult(results, "scenarios", "evals-json", exists(evalsPath) ? "PASS" : "FAIL", relativeToRepo(evalsPath));
  addResult(results, "scenarios", "review-ui", exists(reviewPath) ? "PASS" : "WARN", relativeToRepo(reviewPath));
  if (!exists(evalsPath)) return;

  const evals = readJson(evalsPath);
  const evalIds = new Set((evals.evals || []).map((entry) => entry.id));
  addResult(results, "scenarios", "scenario-count", evalIds.size > 0 ? "PASS" : "FAIL", `count=${evalIds.size}`);

  const requestedIds = parseEvalIds(evalIdsArg);
  if (requestedIds && requestedIds.length > 0) {
    const missing = requestedIds.filter((id) => !evalIds.has(id));
    addResult(
      results,
      "scenarios",
      "requested-eval-ids",
      missing.length === 0 ? "PASS" : "FAIL",
      missing.length === 0 ? `all present: ${requestedIds.join(",")}` : `missing: ${missing.join(",")}`,
    );
  }

  const pickedIteration = normalizeIterationName(iterationArg) || findLatestIteration(workspacePath);
  if (!pickedIteration) {
    addResult(results, "scenarios", "iteration-detected", "WARN", "no iteration-* directories found");
    return;
  }

  const iterationPath = path.join(workspacePath, pickedIteration);
  const benchmarkJsonPath = path.join(iterationPath, "benchmark.json");
  const benchmarkMdPath = path.join(iterationPath, "benchmark.md");
  const benchmarkPacketPath = path.join(iterationPath, DEFAULT_PACKET_FILE);
  const benchmarkEvidencePath = path.join(iterationPath, DEFAULT_EVIDENCE_FILE);

  addResult(results, "scenarios", "iteration-detected", exists(iterationPath) ? "PASS" : "FAIL", pickedIteration);
  addResult(results, "scenarios", "benchmark-packet", exists(benchmarkPacketPath) ? "PASS" : "WARN", relativeToRepo(benchmarkPacketPath));
  addResult(results, "scenarios", "benchmark-evidence", exists(benchmarkEvidencePath) ? "PASS" : "WARN", relativeToRepo(benchmarkEvidencePath));
  addResult(results, "scenarios", "benchmark-json", exists(benchmarkJsonPath) ? "PASS" : "WARN", relativeToRepo(benchmarkJsonPath));
  addResult(results, "scenarios", "benchmark-md", exists(benchmarkMdPath) ? "PASS" : "WARN", relativeToRepo(benchmarkMdPath));

  if (!exists(benchmarkJsonPath)) return;

  const benchmark = readJson(benchmarkJsonPath);
  if (benchmark.runner?.mode) {
    addResult(results, "scenarios", "benchmark-runner-mode", benchmark.runner.mode === "chat" ? "PASS" : "WARN", benchmark.runner.mode);
  }
  if (benchmark.evidence_path) {
    addResult(results, "scenarios", "benchmark-evidence-path", "PASS", benchmark.evidence_path);
  }

  const runSummary = benchmark.run_summary || {};
  if (typeof runSummary.count === "number") {
    addResult(
      results,
      "scenarios",
      "benchmark-summary",
      "PASS",
      `pass_rate=${runSummary.pass_rate ?? "n/a"}, passed=${runSummary.passed ?? 0}, failed=${runSummary.failed ?? 0}, incomplete=${runSummary.incomplete ?? 0}`,
    );
  } else if (runSummary.combined) {
    const combined = runSummary.combined;
    addResult(
      results,
      "scenarios",
      "benchmark-summary",
      "PASS",
      `combined_pass_rate=${combined.pass_rate ?? "n/a"}, passed=${combined.passed ?? "n/a"}, failed=${combined.failed ?? "n/a"}`,
    );
  } else {
    const withSkillPassRate = runSummary.with_skill?.pass_rate?.mean;
    const withoutSkillPassRate = runSummary.without_skill?.pass_rate?.mean;
    const deltaTime = runSummary.delta?.time_seconds;
    addResult(
      results,
      "scenarios",
      "benchmark-summary",
      "PASS",
      `with_skill_pass_rate=${withSkillPassRate ?? "n/a"}, without_skill_pass_rate=${withoutSkillPassRate ?? "n/a"}, delta_time=${deltaTime ?? "n/a"}`,
    );
  }

  const discriminationNote = Array.isArray(benchmark.notes)
    ? benchmark.notes.find((note) => /non-discriminating|discriminating/i.test(note))
    : null;
  addResult(
    results,
    "scenarios",
    "discrimination-signal",
    discriminationNote ? "PASS" : "WARN",
    discriminationNote || "no explicit discrimination note found",
  );
}

function finalize(results, strict) {
  const counts = {
    PASS: results.filter((entry) => entry.status === "PASS").length,
    WARN: results.filter((entry) => entry.status === "WARN").length,
    FAIL: results.filter((entry) => entry.status === "FAIL").length,
  };
  return {
    counts,
    shouldFail: counts.FAIL > 0 || (strict && counts.WARN > 0),
  };
}

function printText(results, counts) {
  const phases = PHASE_ORDER.filter((phase) => results.some((entry) => entry.phase === phase));
  for (const phase of phases) {
    const items = results.filter((entry) => entry.phase === phase);
    console.log(`\n[${phase}]`);
    for (const item of items) {
      const suffix = item.detail ? ` — ${item.detail}` : "";
      console.log(`${item.status.padEnd(4)} ${item.name}${suffix}`);
    }
  }
  console.log(`\nSummary: PASS=${counts.PASS} WARN=${counts.WARN} FAIL=${counts.FAIL}`);
}

async function main() {
  let args;
  try {
    args = parseArgs(process.argv.slice(2));
  } catch (error) {
    console.error(String(error.message || error));
    process.exit(2);
  }

  if (args.help) {
    printHelp();
    return;
  }

  const results = [];
  let scoutStatus = null;
  let benchmarkInfo = null;

  try {
    if (args.command === "analyze") {
      runStaticChecks(results);
      runRuntimeChecks(results, args.runCmd);
    } else if (args.command === "scout") {
      scoutStatus = await runScoutChecks(results);
    } else if (args.command === "benchmark") {
      benchmarkInfo = runBenchmark(results, args);
      if (benchmarkInfo?.benchmarkJsonPath) {
        runScenarioChecks(results, args.workspace, benchmarkInfo.iterationName, args.evalIds);
      }
    } else {
      if (!args.skipStatic) runStaticChecks(results);
      if (!args.skipRuntime) {
        runRuntimeChecks(results, args.runCmd);
        scoutStatus = await runScoutChecks(results);
      }
      if (!args.skipScenarios) {
        benchmarkInfo = runBenchmark(results, args);
        if (benchmarkInfo?.benchmarkJsonPath) {
          runScenarioChecks(results, args.workspace, benchmarkInfo.iterationName, args.evalIds);
        }
      }
    }
  } catch (error) {
    addResult(results, "benchmark", "exception", "FAIL", String(error.message || error));
  }

  const { counts, shouldFail } = finalize(results, args.strict);
  const output = {
    workspace: args.workspace,
    config: args.config,
    command: args.command,
    strict: args.strict,
    counts,
    status: shouldFail ? "FAIL" : "PASS",
    scout: scoutStatus,
    benchmark: benchmarkInfo,
    checks: results,
  };

  if (args.json) {
    console.log(JSON.stringify(output, null, 2));
  } else {
    printText(results, counts);
    console.log(`Overall: ${output.status}`);
  }

  process.exit(shouldFail ? 1 : 0);
}

await main();
