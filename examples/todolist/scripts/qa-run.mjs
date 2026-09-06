// QA runner role script (role `qa` in .pulse/config/runners.json).
//
// Contract: argv[2] is the path to the run input JSON written by
// `pulse run qa`. The script executes the affected baseline cases against
// the todolist module and prints exactly one final JSON line on stdout:
//   {"cases":[{"id","status","observation"}],"artifacts":[...],"findings":[...]}
//
// It never records receipts, reads no evidence manifest and needs no
// `evidence.record` grant: `pulse run qa` builds and records the
// `qa_checkpoint` receipt itself from the input contract, this output and
// the ingested artifacts (Decision 0014).
//
// Exit code stays 0 when the run produced evidence; product failures are
// reported through case status, not through the exit code.

import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import process from "node:process";

// Executable checks per baseline case id. Each runner returns
// {ok, observation}; a thrown error is a product failure observation.
// Cases without a handler here have no executable check: they report
// `inconclusive` instead of guessing (Decision 0010).
async function runCase(caseId) {
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
  return null;
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
            { case_id: null, summary: `qa-run crashed: ${error.message}`, owner: "scripts/qa-run.mjs", severity: "high" },
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

  const cases = [];
  const findings = [];
  const artifacts = [];

  for (const requested of input.cases ?? []) {
    try {
      const check = await runCase(requested.id);
      if (!check) {
        cases.push({
          id: requested.id,
          status: "inconclusive",
          observation: "no executable check is defined for this case",
        });
        continue;
      }
      cases.push({
        id: requested.id,
        status: check.ok ? "passed" : "failed",
        observation: check.observation,
      });
    } catch (error) {
      cases.push({
        id: requested.id,
        status: "failed",
        observation: error.message,
      });
    }
  }

  const logPath = path.join(artifactDir, "qa-observations.json");
  await writeFile(
    logPath,
    `${JSON.stringify({ cases, findings }, null, 2)}\n`,
  );
  artifacts.push({ path: logPath, role: "log", case_id: null });

  return { cases, artifacts, findings };
}

main();
