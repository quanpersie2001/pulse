// Pure todo-list domain logic. No I/O; callers own persistence.
//
// Invariants:
// - functions never mutate their `todos` argument; they return new values;
// - ids are unique non-empty strings; titles are non-empty trimmed strings;
// - `due` is optional: when present it is a `YYYY-MM-DD` calendar-date
//   string stored verbatim; undated todos carry no `due` field at all.

const DUE_DATE_PATTERN = /^(\d{4})-(\d{2})-(\d{2})$/;

function daysInMonth(year, month) {
  if (month === 2) {
    const leap = (year % 4 === 0 && year % 100 !== 0) || year % 400 === 0;
    return leap ? 29 : 28;
  }
  return [4, 6, 9, 11].includes(month) ? 30 : 31;
}

// True when `value` is a `YYYY-MM-DD` string naming a real calendar date.
// Pure string arithmetic: no Date parsing, so no timezone interpretation.
function isCalendarDate(value) {
  const match = typeof value === "string" ? DUE_DATE_PATTERN.exec(value) : null;
  if (!match) return false;
  const [year, month, day] = match.slice(1).map(Number);
  return month >= 1 && month <= 12 && day >= 1 && day <= daysInMonth(year, month);
}

// `options.due`, when provided, must be a valid `YYYY-MM-DD` calendar date;
// the returned todo then carries it verbatim. Without `due` the todo has no
// `due` field (not `due: null`), so pre-due consumers see no change.
export function createTodo(id, title, options = {}) {
  if (typeof id !== "string" || id.length === 0) {
    throw new TypeError("id must be a non-empty string");
  }
  if (typeof title !== "string" || title.trim().length === 0) {
    throw new TypeError("title must be a non-empty string");
  }
  if (options === null || typeof options !== "object") {
    throw new TypeError("options must be an object");
  }
  const todo = { id, title: title.trim(), done: false };
  if (options.due === undefined) {
    return todo;
  }
  if (!isCalendarDate(options.due)) {
    throw new TypeError("due must be a YYYY-MM-DD calendar date");
  }
  return { ...todo, due: options.due };
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

// Done todos in insertion order; the input list is never mutated.
export function completedTodos(todos) {
  return todos.filter((todo) => todo.done);
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

// Stable public outcome names for renameTodo. Renaming is human-gated.
export const RenameOutcome = Object.freeze({
  Renamed: "Renamed",
  NotFound: "NotFound",
});

// Returns `{ outcome, todos }`. On Renamed the returned list is a new array
// with only the matched todo's title changed; on NotFound the input list is
// returned untouched.
export function renameTodo(todos, id, title) {
  if (typeof title !== "string" || title.trim().length === 0) {
    throw new TypeError("title must be a non-empty string");
  }

  const index = todos.findIndex((todo) => todo.id === id);
  if (index === -1) {
    return { outcome: RenameOutcome.NotFound, todos };
  }

  const next = todos.slice();
  next[index] = { ...todos[index], title: title.trim() };
  return { outcome: RenameOutcome.Renamed, todos: next };
}
