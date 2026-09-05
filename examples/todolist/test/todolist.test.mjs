import assert from "node:assert/strict";
import { test } from "node:test";
import {
  addTodo,
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
