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

// Single source of the usage line: the successful `help` path and the
// failing misuse path print the same text, only on different streams.
const USAGE =
  "usage: node src/cli.mjs help | add <id> <title> [--due <YYYY-MM-DD>] | list | count | completed | done <id> | rename <id> <title> | remove <id>";

// Genuine misuse (unknown command, missing arguments): usage on stderr, exit 2.
function usage() {
  console.error(USAGE);
  process.exitCode = 2;
}

// Explicit help request: usage on stdout, exit 0.
function printHelp() {
  console.log(USAGE);
  process.exitCode = 0;
}

function isHelpFlag(arg) {
  return arg === "--help" || arg === "-h";
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

const argv = process.argv.slice(2);
const [command, id, ...rest] = argv;

// `--help` / `-h` anywhere on the command line wins before any state is
// read or any command runs, so it never touches .todolist.json.
if (argv.some(isHelpFlag)) {
  printHelp();
  process.exit();
}

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
    case "help": {
      printHelp();
      break;
    }
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
