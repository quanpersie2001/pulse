import { isDirectExecution } from "../cli_execution.mjs";
import { main as runStatusCommand } from "../pulse_status.mjs";

export * from "../pulse_status.mjs";

export function main(argv = process.argv.slice(2), context = {}) {
  return runStatusCommand(argv, context);
}

if (isDirectExecution(import.meta.url)) {
  process.exitCode = await main();
}
