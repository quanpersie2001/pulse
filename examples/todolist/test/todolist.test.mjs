import assert from "node:assert/strict";
import { test } from "node:test";
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
