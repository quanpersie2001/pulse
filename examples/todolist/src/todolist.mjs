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
