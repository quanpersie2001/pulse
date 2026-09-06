import assert from "node:assert/strict";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import {
  CompleteOutcome,
  RenameOutcome,
  addTodo,
  completeTodo,
  completedTodos,
  createTodo,
  findTodo,
  pendingTodos,
  removeTodo,
  renameTodo,
} from "../src/todolist.mjs";

const cliPath = fileURLToPath(new URL("../src/cli.mjs", import.meta.url));

function runCli(cwd, ...args) {
  return spawnSync(process.execPath, [cliPath, ...args], {
    cwd,
    encoding: "utf8",
  });
}

// TK-004 AC-1, AC-2
test("every command fails clearly without replacing a corrupt state file", async (t) => {
  const cwd = await mkdtemp(path.join(tmpdir(), "todolist-corrupt-"));
  t.after(() => rm(cwd, { recursive: true, force: true }));
  const statePath = path.join(cwd, ".todolist.json");
  const corruptState = '{"id":"unfinished"';
  await writeFile(statePath, corruptState);

  const commands = [
    ["list"],
    ["count"],
    ["completed"],
    ["add", "t1", "one"],
    ["done", "t1"],
    ["rename", "t1", "renamed"],
    ["remove", "t1"],
  ];

  for (const args of commands) {
    const result = runCli(cwd, ...args);
    assert.equal(result.status, 1, args[0]);
    assert.equal(result.stdout, "", args[0]);
    assert.equal(
      result.stderr,
      "error: state file contains invalid JSON\n",
      args[0],
    );
    assert.equal(await readFile(statePath, "utf8"), corruptState, args[0]);
  }
});

// TK-004 AC-3
test("valid state files retain existing CLI behavior", async (t) => {
  const cwd = await mkdtemp(path.join(tmpdir(), "todolist-valid-"));
  t.after(() => rm(cwd, { recursive: true, force: true }));

  let result = runCli(cwd, "add", "t1", "one");
  assert.equal(result.status, 0);
  assert.equal(result.stdout, "");

  result = runCli(cwd, "list");
  assert.equal(result.status, 0);
  assert.equal(result.stdout, "t1\tone\n");

  result = runCli(cwd, "done", "t1");
  assert.equal(result.status, 0);
  assert.equal(result.stdout, "Completed\n");

  result = runCli(cwd, "completed");
  assert.equal(result.status, 0);
  assert.equal(result.stdout, "t1\tone\n");

  result = runCli(cwd, "remove", "t1");
  assert.equal(result.status, 0);
  assert.equal(result.stdout, "");
  assert.deepEqual(JSON.parse(await readFile(path.join(cwd, ".todolist.json"))), []);
});

test("createTodo builds an undone todo with a trimmed title", () => {
  assert.deepEqual(createTodo("t1", "  buy milk  "), {
    id: "t1",
    title: "buy milk",
    done: false,
  });
});

test("createTodo rejects empty ids and blank titles", () => {
  assert.throws(() => createTodo("", "x"), TypeError);
  assert.throws(() => createTodo("t1", "   "), TypeError);
});

test("addTodo appends without mutating the input list", () => {
  const first = createTodo("t1", "one");
  const todos = [first];
  const next = addTodo(todos, createTodo("t2", "two"));
  assert.equal(todos.length, 1);
  assert.equal(next.length, 2);
});

test("addTodo rejects duplicate ids", () => {
  const todos = [createTodo("t1", "one")];
  assert.throws(() => addTodo(todos, createTodo("t1", "again")), TypeError);
});

test("findTodo returns null for unknown ids", () => {
  assert.equal(findTodo([createTodo("t1", "one")], "nope"), null);
});

test("removeTodo drops only the targeted id", () => {
  const todos = [createTodo("t1", "one"), createTodo("t2", "two")];
  assert.deepEqual(removeTodo(todos, "t1").map((todo) => todo.id), ["t2"]);
});

test("pendingTodos filters done items", () => {
  const todos = [
    { id: "t1", title: "one", done: true },
    { id: "t2", title: "two", done: false },
  ];
  assert.deepEqual(pendingTodos(todos).map((todo) => todo.id), ["t2"]);
});

// TK-003 AC-1, AC-2, AC-3
test("count prints the pending count and excludes completed todos", async (t) => {
  const cwd = await mkdtemp(path.join(tmpdir(), "todolist-count-"));
  t.after(() => rm(cwd, { recursive: true, force: true }));

  let result = runCli(cwd, "count");
  assert.equal(result.status, 0);
  assert.equal(result.stdout, "0\n");

  assert.equal(runCli(cwd, "add", "t1", "one").status, 0);
  assert.equal(runCli(cwd, "add", "t2", "two").status, 0);
  result = runCli(cwd, "count");
  assert.equal(result.status, 0);
  assert.equal(result.stdout, "2\n");

  assert.equal(runCli(cwd, "done", "t1").status, 0);
  result = runCli(cwd, "count");
  assert.equal(result.status, 0);
  assert.equal(result.stdout, "1\n");
});

// TK-002 AC-1
test("completedTodos keeps only done items in insertion order without mutating input", () => {
  const todos = [
    { id: "t1", title: "one", done: true },
    { id: "t2", title: "two", done: false },
    { id: "t3", title: "three", done: true },
  ];
  const snapshot = structuredClone(todos);
  const completed = completedTodos(todos);
  assert.deepEqual(completed.map((todo) => todo.id), ["t1", "t3"]);
  assert.deepEqual(todos, snapshot);
  assert.notEqual(completed, todos);
});

test("completedTodos returns an empty list when nothing is done", () => {
  assert.deepEqual(completedTodos([createTodo("t1", "one")]), []);
});

// QA-001 / AC-1
test("completeTodo marks a known id done and returns Completed without mutating input", () => {
  const todos = [createTodo("t1", "one")];
  const snapshot = structuredClone(todos);
  const result = completeTodo(todos, "t1");
  assert.equal(result.outcome, "Completed");
  assert.equal(result.outcome, CompleteOutcome.Completed);
  assert.deepEqual(result.todos, [{ id: "t1", title: "one", done: true }]);
  assert.deepEqual(todos, snapshot);
  assert.notEqual(result.todos, todos);
});

// QA-002 / AC-2
test("completeTodo returns NotFound for an unknown id and changes nothing", () => {
  const todos = [{ id: "t1", title: "one", done: true }];
  const snapshot = structuredClone(todos);
  const result = completeTodo(todos, "missing-id");
  assert.equal(result.outcome, "NotFound");
  assert.equal(result.outcome, CompleteOutcome.NotFound);
  assert.deepEqual(result.todos, todos);
  assert.deepEqual(todos, snapshot);
});

test("completeTodo only touches the matched todo", () => {
  const todos = [createTodo("t1", "one"), createTodo("t2", "two")];
  const result = completeTodo(todos, "t2");
  assert.deepEqual(result.todos.map((todo) => [todo.id, todo.done]), [
    ["t1", false],
    ["t2", true],
  ]);
  assert.equal(todos[1].done, false);
});

// TK-007 AC-1, AC-3
test("renameTodo trims the title while preserving fields and input order", () => {
  const todos = [
    { id: "t1", title: "one", done: false, priority: "high" },
    { id: "t2", title: "two", done: true, priority: "low" },
  ];
  const snapshot = structuredClone(todos);
  const result = renameTodo(todos, "t2", "  renamed todo  ");

  assert.equal(result.outcome, "Renamed");
  assert.equal(result.outcome, RenameOutcome.Renamed);
  assert.deepEqual(result.todos, [
    { id: "t1", title: "one", done: false, priority: "high" },
    { id: "t2", title: "renamed todo", done: true, priority: "low" },
  ]);
  assert.deepEqual(todos, snapshot);
  assert.notEqual(result.todos, todos);
  assert.equal(result.todos[0], todos[0]);
});

// TK-007 AC-2, AC-3
test("renameTodo returns NotFound without changing the input", () => {
  const todos = [createTodo("t1", "one")];
  const snapshot = structuredClone(todos);
  const result = renameTodo(todos, "missing-id", "new title");

  assert.equal(result.outcome, "NotFound");
  assert.equal(result.outcome, RenameOutcome.NotFound);
  assert.equal(result.todos, todos);
  assert.deepEqual(todos, snapshot);
});

// TK-007 AC-3
test("renameTodo rejects a blank title without mutating the input", () => {
  const todos = [createTodo("t1", "one")];
  const snapshot = structuredClone(todos);

  assert.throws(() => renameTodo(todos, "t1", "   "), TypeError);
  assert.deepEqual(todos, snapshot);
});

// TK-007 AC-1
test("rename command persists a trimmed title and preserves done state and position", async (t) => {
  const cwd = await mkdtemp(path.join(tmpdir(), "todolist-rename-"));
  t.after(() => rm(cwd, { recursive: true, force: true }));
  const statePath = path.join(cwd, ".todolist.json");
  const todos = [
    { id: "t1", title: "one", done: false },
    { id: "t2", title: "two", done: true, priority: "high" },
  ];
  await writeFile(statePath, `${JSON.stringify(todos, null, 2)}\n`);

  const result = runCli(cwd, "rename", "t2", "  renamed todo  ");

  assert.equal(result.status, 0);
  assert.equal(result.stdout, "Renamed\n");
  assert.deepEqual(JSON.parse(await readFile(statePath, "utf8")), [
    { id: "t1", title: "one", done: false },
    { id: "t2", title: "renamed todo", done: true, priority: "high" },
  ]);
});

// TK-007 AC-2
test("rename command reports NotFound without changing the state file", async (t) => {
  const cwd = await mkdtemp(path.join(tmpdir(), "todolist-rename-missing-"));
  t.after(() => rm(cwd, { recursive: true, force: true }));
  const statePath = path.join(cwd, ".todolist.json");
  const state = `${JSON.stringify([createTodo("t1", "one")], null, 2)}\n`;
  await writeFile(statePath, state);

  const result = runCli(cwd, "rename", "missing-id", "new title");

  assert.equal(result.status, 1);
  assert.equal(result.stdout, "NotFound\n");
  assert.equal(await readFile(statePath, "utf8"), state);
});

// TK-005 AC-1 / QA-003
test("createTodo stores a valid due date verbatim without mutating its inputs", () => {
  const options = { due: "2026-12-01" };
  const optionsSnapshot = structuredClone(options);
  const dated = createTodo("t1", "  one  ", options);
  assert.deepEqual(dated, { id: "t1", title: "one", done: false, due: "2026-12-01" });
  assert.deepEqual(options, optionsSnapshot);

  const undated = createTodo("t0", "zero");
  const todos = [undated];
  const snapshot = structuredClone(todos);
  const next = addTodo(todos, dated);
  assert.deepEqual(todos, snapshot);
  assert.equal(todos.length, 1);
  assert.equal(next.length, 2);
  assert.equal(Object.hasOwn(next[0], "due"), false);
  assert.equal(next[1].due, "2026-12-01");
});

// TK-005 AC-1
test("createTodo accepts real calendar dates including leap days", () => {
  assert.equal(createTodo("t1", "one", { due: "2024-02-29" }).due, "2024-02-29");
  assert.equal(createTodo("t1", "one", { due: "2026-01-31" }).due, "2026-01-31");
  assert.equal(createTodo("t1", "one", { due: "2026-12-31" }).due, "2026-12-31");
});

// TK-005 AC-2
test("createTodo without a due leaves the field absent, not null", () => {
  const plain = createTodo("t1", "one");
  const emptyOptions = createTodo("t1", "one", {});
  const undefinedDue = createTodo("t1", "one", { due: undefined });
  for (const todo of [plain, emptyOptions, undefinedDue]) {
    assert.deepEqual(todo, { id: "t1", title: "one", done: false });
    assert.equal(Object.hasOwn(todo, "due"), false);
  }
});

// TK-005 AC-3 / QA-004
test("createTodo rejects wrong formats and nonexistent calendar dates with TypeError", () => {
  const invalid = [
    "12/01/2026", // wrong format
    "2026-02-30", // nonexistent day
    "2023-02-29", // not a leap year
    "2026-04-31", // April has 30 days
    "2026-13-01", // month out of range
    "2026-00-10", // month zero
    "2026-12-00", // day zero
    "2026-1-1", // unpadded
    "2026-12-01T00:00:00Z", // datetime, not a date
    "", // empty string
    null,
    20261201,
    new Date("2026-12-01"),
  ];
  for (const due of invalid) {
    assert.throws(() => createTodo("t1", "one", { due }), TypeError, String(due));
  }
  assert.throws(() => createTodo("t1", "one", "2026-12-01"), TypeError);
});

// TK-005 AC-1
test("add --due persists the date and list shows it as a third column", async (t) => {
  const cwd = await mkdtemp(path.join(tmpdir(), "todolist-due-"));
  t.after(() => rm(cwd, { recursive: true, force: true }));
  const statePath = path.join(cwd, ".todolist.json");

  let result = runCli(cwd, "add", "t1", "dated", "todo", "--due", "2026-12-01");
  assert.equal(result.status, 0);
  assert.equal(result.stdout, "");
  assert.equal(result.stderr, "");
  assert.deepEqual(JSON.parse(await readFile(statePath, "utf8")), [
    { id: "t1", title: "dated todo", done: false, due: "2026-12-01" },
  ]);

  result = runCli(cwd, "add", "t2", "--due=2027-01-15", "flag", "first");
  assert.equal(result.status, 0);
  result = runCli(cwd, "add", "t3", "undated");
  assert.equal(result.status, 0);

  result = runCli(cwd, "list");
  assert.equal(result.status, 0);
  assert.equal(
    result.stdout,
    "t1\tdated todo\t2026-12-01\nt2\tflag first\t2027-01-15\nt3\tundated\n",
  );

  // The date survives a further save/load roundtrip through another command.
  assert.equal(runCli(cwd, "done", "t3").status, 0);
  result = runCli(cwd, "list");
  assert.equal(result.stdout, "t1\tdated todo\t2026-12-01\nt2\tflag first\t2027-01-15\n");
  assert.equal(JSON.parse(await readFile(statePath, "utf8"))[0].due, "2026-12-01");
});

// TK-005 AC-2
test("undated add and pre-due state files behave byte-identically", async (t) => {
  const cwd = await mkdtemp(path.join(tmpdir(), "todolist-undated-"));
  t.after(() => rm(cwd, { recursive: true, force: true }));
  const statePath = path.join(cwd, ".todolist.json");
  const preDueState = [
    { id: "t1", title: "one", done: false },
    { id: "t2", title: "two", done: true },
  ];
  await writeFile(statePath, `${JSON.stringify(preDueState, null, 2)}\n`);

  let result = runCli(cwd, "list");
  assert.equal(result.status, 0);
  assert.equal(result.stdout, "t1\tone\n");

  result = runCli(cwd, "add", "t3", "three", "words", "here");
  assert.equal(result.status, 0);
  assert.equal(result.stdout, "");
  assert.equal(result.stderr, "");
  const expectedState = [...preDueState, { id: "t3", title: "three words here", done: false }];
  assert.equal(
    await readFile(statePath, "utf8"),
    `${JSON.stringify(expectedState, null, 2)}\n`,
  );
  assert.equal(await readFile(statePath, "utf8").then((text) => text.includes("due")), false);

  result = runCli(cwd, "list");
  assert.equal(result.stdout, "t1\tone\nt3\tthree words here\n");
  result = runCli(cwd, "completed");
  assert.equal(result.stdout, "t2\ttwo\n");
  result = runCli(cwd, "count");
  assert.equal(result.stdout, "2\n");
});

// TK-005 AC-3
test("add with an invalid --due prints one error line, exits 2 and writes nothing", async (t) => {
  const cwd = await mkdtemp(path.join(tmpdir(), "todolist-bad-due-"));
  t.after(() => rm(cwd, { recursive: true, force: true }));
  const statePath = path.join(cwd, ".todolist.json");

  for (const due of ["12/01/2026", "2026-02-30"]) {
    const result = runCli(cwd, "add", "t1", "one", "--due", due);
    assert.equal(result.status, 2, due);
    assert.equal(result.stdout, "", due);
    assert.match(result.stderr, /^error: [^\n]+\n$/, due);
    await assert.rejects(readFile(statePath), { code: "ENOENT" }, due);
  }

  const state = `${JSON.stringify([createTodo("t0", "zero")], null, 2)}\n`;
  await writeFile(statePath, state);
  for (const due of ["12/01/2026", "2026-02-30"]) {
    const result = runCli(cwd, "add", "t1", "one", "--due", due);
    assert.equal(result.status, 2, due);
    assert.equal(result.stdout, "", due);
    assert.match(result.stderr, /^error: [^\n]+\n$/, due);
    assert.equal(await readFile(statePath, "utf8"), state, due);
  }

  // A dangling --due is a usage error, also without any write.
  const result = runCli(cwd, "add", "t1", "one", "--due");
  assert.equal(result.status, 2);
  assert.equal(result.stdout, "");
  assert.match(result.stderr, /^usage: /);
  assert.equal(await readFile(statePath, "utf8"), state);
});

// TK-008 AC-1
test("help prints usage to stdout and exits 0", async (t) => {
  const cwd = await mkdtemp(path.join(tmpdir(), "todolist-help-"));
  t.after(() => rm(cwd, { recursive: true, force: true }));

  const result = runCli(cwd, "help");
  assert.equal(result.status, 0);
  assert.match(result.stdout, /^usage: node src\/cli\.mjs [^\n]*\bhelp\b[^\n]*\n$/);
  assert.equal(result.stderr, "");
  await assert.rejects(readFile(path.join(cwd, ".todolist.json")), { code: "ENOENT" });
});

// TK-008 AC-2
test("--help and -h anywhere print usage to stdout, exit 0 and run no command", async (t) => {
  const cwd = await mkdtemp(path.join(tmpdir(), "todolist-help-flag-"));
  t.after(() => rm(cwd, { recursive: true, force: true }));
  const statePath = path.join(cwd, ".todolist.json");
  const expected = runCli(cwd, "help").stdout;

  const invocations = [
    ["--help"],
    ["-h"],
    ["--help", "add", "t1", "one"],
    ["add", "t1", "one", "--help"],
    ["done", "t1", "-h"],
    ["remove", "-h", "t1"],
    ["nonsense", "--help"],
  ];
  for (const args of invocations) {
    const result = runCli(cwd, ...args);
    const label = args.join(" ");
    assert.equal(result.status, 0, label);
    assert.equal(result.stdout, expected, label);
    assert.equal(result.stderr, "", label);
    await assert.rejects(readFile(statePath), { code: "ENOENT" }, label);
  }

  // With existing state, a help flag still leaves the file untouched.
  const state = `${JSON.stringify([createTodo("t1", "one")], null, 2)}\n`;
  await writeFile(statePath, state);
  for (const args of [["done", "t1", "--help"], ["remove", "t1", "-h"]]) {
    const result = runCli(cwd, ...args);
    assert.equal(result.status, 0, args.join(" "));
    assert.equal(result.stdout, expected, args.join(" "));
    assert.equal(await readFile(statePath, "utf8"), state, args.join(" "));
  }
});

// TK-008 AC-3
test("unknown commands and missing arguments still print usage to stderr and exit 2", async (t) => {
  const cwd = await mkdtemp(path.join(tmpdir(), "todolist-misuse-"));
  t.after(() => rm(cwd, { recursive: true, force: true }));
  const helpText = runCli(cwd, "help").stdout;

  for (const args of [["nonsense"], [], ["done"], ["rename", "t1"]]) {
    const result = runCli(cwd, ...args);
    const label = args.join(" ") || "<no args>";
    assert.equal(result.status, 2, label);
    assert.equal(result.stdout, "", label);
    // Same single-source usage text as the help path, just on stderr.
    assert.equal(result.stderr, helpText, label);
  }
});
