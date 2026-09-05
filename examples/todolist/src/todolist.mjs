// Pure todo-list domain logic. No I/O; callers own persistence.
//
// Invariants:
// - functions never mutate their `todos` argument; they return new values;
// - ids are unique non-empty strings; titles are non-empty trimmed strings.

export function createTodo(id, title) {
  if (typeof id !== "string" || id.length === 0) {
    throw new TypeError("id must be a non-empty string");
  }
  if (typeof title !== "string" || title.trim().length === 0) {
    throw new TypeError("title must be a non-empty string");
  }
  return { id, title: title.trim(), done: false };
}

export function addTodo(todos, todo) {
  if (todos.some((existing) => existing.id === todo.id)) {
    throw new TypeError(`duplicate todo id: ${todo.id}`);
  }
  return [...todos, todo];
}

export function findTodo(todos, id) {
  return todos.find((todo) => todo.id === id) ?? null;
}

export function removeTodo(todos, id) {
  return todos.filter((todo) => todo.id !== id);
}

export function pendingTodos(todos) {
  return todos.filter((todo) => !todo.done);
}

// Stable public outcome names for completeTodo. Renaming is human-gated.
export const CompleteOutcome = Object.freeze({
  Completed: "Completed",
  NotFound: "NotFound",
});

// Returns `{ outcome, todos }`. On Completed the returned list is a new
// array with the matched todo marked done; on NotFound the input list is
// returned untouched. Never throws for unknown ids.
export function completeTodo(todos, id) {
  const index = todos.findIndex((todo) => todo.id === id);
  if (index === -1) {
    return { outcome: CompleteOutcome.NotFound, todos };
  }
  const next = todos.slice();
  next[index] = { ...todos[index], done: true };
  return { outcome: CompleteOutcome.Completed, todos: next };
}
