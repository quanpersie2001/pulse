// Minimal CLI over the todo domain module. State lives in .todolist.json
// (cwd); each invocation reads, mutates through the module, and writes back.
import { readFile, writeFile } from "node:fs/promises";
import process from "node:process";
import {
  CompleteOutcome,
  addTodo,
  completeTodo,
  createTodo,
  pendingTodos,
  removeTodo,
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
    "usage: node src/cli.mjs add <id> <title> | list | done <id> | remove <id>",
  );
  process.exitCode = 2;
}

const [command, id, ...rest] = process.argv.slice(2);
const todos = await loadTodos();

switch (command) {
  case "add": {
    if (!id || rest.length === 0) {
      usage();
      break;
    }
    await saveTodos(addTodo(todos, createTodo(id, rest.join(" "))));
    break;
  }
  case "list": {
    for (const todo of pendingTodos(todos)) {
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
