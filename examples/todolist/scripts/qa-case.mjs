// Executable body of one QA baseline case (Decision 0010).
//
// Contract: argv[2] is a case id from works/<STORY>/qa.md. The script exercises
// the domain surface in process and prints two stdout lines: a verdict line
// `<CASE-ID> ok` or `<CASE-ID> failed`, then the observation. It exits 0 when
// the case holds and 1 when it does not. A case id with no body here exits 3,
// which is what `no executable check` looks like from the outside.
//
// `qa-run.mjs` never reads this file: it only runs the `pulse-check` argv each
// case declares and evaluates the declared assertions.

import process from "node:process";

import * as todolist from "../src/todolist.mjs";

const cases = {
  "QA-001": () => {
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
        : `completeTodo returned ${JSON.stringify(result)} for input ${JSON.stringify(todos)}`,
    };
  },
  "QA-002": () => {
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
  },
  "QA-003": () => {
    const dated = todolist.createTodo("qa-3", "sample", { due: "2026-12-01" });
    const undated = todolist.createTodo("qa-3-undated", "sample");
    const input = [undated];
    const inputSnapshot = JSON.stringify(input);
    const next = todolist.addTodo(input, dated);
    const stored = next.find((todo) => todo.id === "qa-3");
    const ok =
      dated.due === "2026-12-01" &&
      dated.done === false &&
      stored?.due === "2026-12-01" &&
      !Object.hasOwn(undated, "due") &&
      !Object.hasOwn(next[0], "due") &&
      input.length === 1 &&
      JSON.stringify(input) === inputSnapshot;
    return {
      ok,
      observation: ok
        ? "createTodo stored due 2026-12-01 verbatim, the undated sibling has no due field and the input list was not mutated"
        : `createTodo/addTodo produced dated=${JSON.stringify(dated)} list=${JSON.stringify(next)} input=${JSON.stringify(input)}`,
    };
  },
  "QA-004": () => {
    const problems = [];
    for (const [id, due] of [["qa-4", "12/01/2026"], ["qa-4b", "2026-02-30"]]) {
      try {
        const value = todolist.createTodo(id, "sample", { due });
        problems.push(`due ${due} returned ${JSON.stringify(value)} instead of throwing`);
      } catch (error) {
        if (!(error instanceof TypeError)) {
          problems.push(`due ${due} threw ${error.name} instead of TypeError`);
        }
      }
    }
    const ok = problems.length === 0;
    return {
      ok,
      observation: ok
        ? "createTodo threw TypeError for 12/01/2026 and 2026-02-30 without returning a todo"
        : problems.join("; "),
    };
  },
};

const caseId = process.argv[2];
const body = cases[caseId];
if (!body) {
  console.error(`no executable check for ${caseId ?? "<missing case id>"}`);
  process.exit(3);
}

try {
  const { ok, observation } = body();
  console.log(ok ? `${caseId} ok` : `${caseId} failed`);
  console.log(observation);
  if (!ok) {
    process.exit(1);
  }
} catch (error) {
  console.log(`${caseId} failed`);
  console.log(error.message);
  process.exit(1);
}
