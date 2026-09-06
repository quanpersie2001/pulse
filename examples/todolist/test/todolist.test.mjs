import assert from "node:assert/strict";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import {
  CompleteOutcome,
  addTodo,
  completeTodo,
  completedTodos,
  createTodo,
  findTodo,
  pendingTodos,
  removeTodo,
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
