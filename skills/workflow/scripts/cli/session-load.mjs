import { isDirectExecution } from "../cli_execution.mjs";
import { main as runSessionLoadCommand } from "../pulse_session_load.mjs";

export * from "../pulse_session_load.mjs";

export function main(argv = process.argv.slice(2), context = {}) {
  return runSessionLoadCommand(argv, context);
}

if (isDirectExecution(import.meta.url)) {
  process.exitCode = main();
}
