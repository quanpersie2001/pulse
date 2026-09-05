// QA runner role script (role `qa` in .pulse/config/runners.json).
//
// Contract: argv[2] is the path to the run input JSON written by
// `pulse run qa`. The script executes the affected baseline cases against
// the todolist module, records one `qa_checkpoint` receipt through the
// pulse CLI, and prints exactly one final JSON line on stdout:
//   {"cases":[{"id","status","observation"}],"artifacts":[...],"findings":[...]}
//
// Exit code stays 0 when the run produced evidence; product failures are
// reported through case status, not through the exit code.

import { createHash, randomBytes } from "node:crypto";
import { spawnSync } from "node:child_process";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import process from "node:process";

const PULSE_BIN = process.env.PULSE_BIN ?? "pulse";
const CROCKFORD = "0123456789ABCDEFGHJKMNPQRSTVWXYZ";

function newReceiptId() {
  const enc = (value, width) => {
    let out = "";
    for (let i = width - 1; i >= 0; i -= 1) {
      out = CROCKFORD[Number((value >> BigInt(i * 5)) & 31n)] + out;
    }
    return out;
  };
  let randomness = 0n;
  for (const byte of randomBytes(10)) {
    randomness = (randomness << 8n) | BigInt(byte);
  }
  return `rcpt_${enc(BigInt(Date.now()), 10)}${enc(randomness, 16)}`;
}

function sha256(bytes) {
  return `sha256:${createHash("sha256").update(bytes).digest("hex")}`;
}

function extractPulseQaBlock(markdown) {
  const marker = "```pulse-qa";
  const first = markdown.indexOf(marker);
  if (first < 0) {
    throw new Error("qa.md must contain exactly one pulse-qa block");
  }
  if (markdown.indexOf(marker, first + marker.length) >= 0) {
    throw new Error("qa.md must contain exactly one pulse-qa block");
  }
  const contentStart = first + marker.length;
  const rest = markdown.slice(contentStart).replace(/^[ \t]*\n/, "");
  const end = rest.indexOf("```");
  if (end < 0) {
    throw new Error("pulse-qa block is not closed");
  }
  return JSON.parse(rest.slice(0, end).trim());
}

// Executable checks per baseline case id. Each runner returns
// {ok, observation}; a thrown error is a product failure observation.
async function runCase(caseId, baseline) {
  const todolist = await import("../src/todolist.mjs");
  if (caseId === "QA-001") {
    const todos = todolist.addTodo([], todolist.createTodo("qa-1", "sample"));
    const result = todolist.completeTodo(todos, "qa-1");
    const ok =
      result?.outcome === "Completed" &&
      result?.todos?.[0]?.done === true &&
      todos[0].done === false;
    return {
      ok,
      observation: ok
        ? "completeTodo returned Completed and marked the todo done without mutating the input"
        : `completeTodo returned ${JSON.stringify(result)}`,
    };
  }
  if (caseId === "QA-002") {
    const todo = todolist.createTodo("qa-1", "sample");
    const todos = [{ ...todo, done: true }];
    const result = todolist.completeTodo(todos, "missing-id");
    const ok =
      result?.outcome === "NotFound" &&
      Array.isArray(result?.todos) &&
      result.todos.length === 1 &&
      result.todos[0].id === "qa-1" &&
      result.todos[0].done === true;
    return {
      ok,
      observation: ok
        ? "completeTodo returned NotFound and left the list unchanged"
        : `completeTodo returned ${JSON.stringify(result)}`,
    };
  }
  throw new Error(`no executable check is defined for case ${caseId}`);
}

function main() {
  const inputPath = process.argv[2];
  if (!inputPath) {
    console.error("usage: node scripts/qa-run.mjs <input.json>");
    process.exitCode = 2;
    return;
  }
  run(inputPath)
    .then((report) => {
      console.log(JSON.stringify(report));
    })
    .catch((error) => {
      // Harness/infrastructure failure: still honor the final-JSON contract.
      console.log(
        JSON.stringify({
          cases: [],
          artifacts: [],
          findings: [
            { case_id: null, summary: `qa-run crashed: ${error.message}`, severity: "high" },
          ],
        }),
      );
      process.exitCode = 1;
    });
}

async function run(inputPath) {
  const input = JSON.parse(await readFile(inputPath, "utf8"));
  const repoRoot = process.cwd();
  const artifactDir = path.resolve(
    path.dirname(path.resolve(inputPath)),
    input.artifact_dir ?? "artifacts",
  );
  await mkdir(artifactDir, { recursive: true });

  const baselinePath = path.join(repoRoot, "works", input.story_id, "qa.md");
  const baselineBytes = await readFile(baselinePath);
  const baselineHash = sha256(baselineBytes);
  const baseline = extractPulseQaBlock(baselineBytes.toString("utf8"));

  const cases = [];
  const findings = [];
  const artifacts = [];
  let baselineDrift = baselineHash !== input.baseline_content_hash;

  for (const requested of input.cases ?? []) {
    const definition = baseline.cases.find((c) => c.id === requested.id);
    if (baselineDrift) {
      cases.push({
        id: requested.id,
        status: "inconclusive",
        observation: "current qa.md hash does not match the run input baseline hash",
      });
      continue;
    }
    if (!definition) {
      cases.push({
        id: requested.id,
        status: "inconclusive",
        observation: `case ${requested.id} is absent from the baseline`,
      });
      continue;
    }
    try {
      const { ok, observation } = await runCase(requested.id, baseline);
      cases.push({
        id: requested.id,
        status: ok ? "passed" : "failed",
        observation,
      });
    } catch (error) {
      cases.push({
        id: requested.id,
        status: "failed",
        observation: error.message,
      });
    }
  }

  if (baselineDrift) {
    findings.push({
      case_id: null,
      summary: `baseline hash drift: input ${input.baseline_content_hash} vs current ${baselineHash}`,
      severity: "high",
    });
  }

  // Map output statuses onto receipt outcomes.
  const outcomeFor = (status) =>
    status === "passed" ? "passed" : "product_failure";
  const allPassed = cases.length > 0 && cases.every((c) => c.status === "passed");
  const hasFailure = cases.some((c) => c.status === "failed");
  const receiptResult = allPassed
    ? "passed"
    : hasFailure
      ? "failed"
      : "inconclusive";

  // Record the qa_checkpoint receipt through the CLI (evidence.record grant).
  // Scope: story_close qualifications are subject to the Story; ticket
  // checkpoints stay subject to the Ticket.
  const qaScope = input.qa_scope ?? "ticket_checkpoint";
  const subjectId = qaScope === "story_close" ? input.story_id : input.ticket_id;
  const manifest = JSON.parse(
    await readFile(path.join(repoRoot, ".pulse", "evidence", "manifest.json"), "utf8"),
  );
  const logPath = path.join(artifactDir, "qa-observations.json");
  await writeFile(
    logPath,
    `${JSON.stringify({ input, cases, findings }, null, 2)}\n`,
  );
  const logArtifact = { path: logPath, role: "log", case_id: null };
  artifacts.push(logArtifact);

  const receipt = {
    schema_version: 1,
    receipt_version: 2,
    id: newReceiptId(),
    kind: "qa_checkpoint",
    result: receiptResult,
    actor: { kind: "agent", id: "runner:qa" },
    recorded_at: new Date().toISOString(),
    subject: { kind: "work", id: subjectId },
    bindings: {
      source: {
        kind: "git_commit",
        commit: input.source_commit,
        repository_id: manifest.repository_id,
      },
      content: [{ path: `works/${input.story_id}/qa.md`, sha256: baselineHash }],
    },
    payload: {
      payload_version: 1,
      qa_scope: qaScope,
      story_id: input.story_id,
      ticket_id: input.ticket_id,
      baseline_revision: input.baseline_revision,
      baseline_content_hash: input.baseline_content_hash,
      cases: input.cases.map((requested, index) => ({
        case_id: requested.id,
        case_revision: requested.revision,
        outcome: outcomeFor(cases[index].status),
      })),
      executor: { name: "todolist-qa-runner", version: "1.0.0", capabilities: ["module"] },
      observations: cases.map((c) => `${c.id}: ${c.observation}`),
    },
  };
  const receiptPath = path.join(artifactDir, "qa-checkpoint-receipt.json");
  await writeFile(receiptPath, `${JSON.stringify(receipt, null, 2)}\n`);
  const recorded = spawnSync(
    PULSE_BIN,
    ["evidence", "receipt", "record", "--file", receiptPath, "--json"],
    { cwd: repoRoot, encoding: "utf8" },
  );
  if (recorded.status !== 0) {
    findings.push({
      case_id: null,
      summary: `recording qa_checkpoint receipt failed: ${recorded.stderr.trim() || recorded.stdout.trim()}`,
      severity: "high",
    });
  }

  return { cases, artifacts, findings };
}

main();
