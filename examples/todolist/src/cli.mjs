// Minimal CLI over the todo domain module. State lives in .todolist.json
// (cwd); each invocation reads, mutates through the module, and writes back.
import { readFile, writeFile } from "node:fs/promises";
import process from "node:process";
import {
  CompleteOutcome,
  RenameOutcome,
  addTodo,
  completeTodo,
  completedTodos,
  createTodo,
  pendingTodos,
  removeTodo,
  renameTodo,
} from "./todolist.mjs";

const STATE_FILE = ".todolist.json";

async function loadTodos() {
  try {
    return JSON.parse(await readFile(STATE_FILE, "utf8"));
  } catch (error) {
    if (error.code === "ENOENT") return [];
    throw error;
  }
}

async function saveTodos(todos) {
  await writeFile(STATE_FILE, `${JSON.stringify(todos, null, 2)}\n`);
}

function usage() {
  console.error(
    "usage: node src/cli.mjs add <id> <title> [--due <YYYY-MM-DD>] | list | count | completed | done <id> | rename <id> <title> | remove <id>",
  );
  process.exitCode = 2;
}

// Splits the `add` arguments into title words and an optional `--due`
// value (`--due <date>` or `--due=<date>`, anywhere after the id). Without
// the flag every word is title, exactly as before due dates existed.
function parseAddArgs(args) {
  const titleWords = [];
  let due;
  for (let index = 0; index < args.length; index += 1) {
    const arg = args[index];
    if (arg === "--due") {
      if (index + 1 >= args.length) return { error: "missing --due value" };
      due = args[index + 1];
      index += 1;
    } else if (arg.startsWith("--due=")) {
      due = arg.slice("--due=".length);
    } else {
      titleWords.push(arg);
    }
  }
  return { titleWords, due };
}

const [command, id, ...rest] = process.argv.slice(2);

let todos;
try {
  todos = await loadTodos();
} catch (error) {
  if (error instanceof SyntaxError) {
    console.error("error: state file contains invalid JSON");
    process.exitCode = 1;
  } else {
    throw error;
  }
}

if (todos !== undefined) {
  switch (command) {
    case "add": {
      const { titleWords, due, error } = parseAddArgs(rest);
      if (!id || error || titleWords.length === 0) {
        usage();
        break;
      }
      let todo;
      try {
        todo = createTodo(id, titleWords.join(" "), due === undefined ? undefined : { due });
      } catch (createError) {
        // Only a bad --due is mapped to the error contract; undated adds keep
        // their pre-due behavior untouched.
        if (due !== undefined && createError instanceof TypeError) {
          console.error(`error: ${createError.message}`);
          process.exitCode = 2;
          break;
        }
        throw createError;
      }
      await saveTodos(addTodo(todos, todo));
      break;
    }
    case "list": {
      for (const todo of pendingTodos(todos)) {
        const line = `${todo.id}\t${todo.title}`;
        console.log(todo.due == null ? line : `${line}\t${todo.due}`);
      }
      break;
    }
    case "count": {
      console.log(pendingTodos(todos).length);
      break;
    }
    case "completed": {
      for (const todo of completedTodos(todos)) {
        console.log(`${todo.id}\t${todo.title}`);
      }
      break;
    }
    case "done": {
      if (!id) {
        usage();
        break;
      }
      const result = completeTodo(todos, id);
      if (result.outcome === CompleteOutcome.Completed) {
        await saveTodos(result.todos);
      } else {
        process.exitCode = 1;
      }
      console.log(result.outcome);
      break;
    }
    case "rename": {
      if (!id || rest.length === 0) {
        usage();
        break;
      }
      const result = renameTodo(todos, id, rest.join(" "));
      if (result.outcome === RenameOutcome.Renamed) {
        await saveTodos(result.todos);
      } else {
        process.exitCode = 1;
      }
      console.log(result.outcome);
      break;
    }
    case "remove": {
      if (!id) {
        usage();
        break;
      }
      await saveTodos(removeTodo(todos, id));
      break;
    }
    default:
      usage();
  }
}
