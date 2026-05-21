import { isDirectExecution } from "../cli_execution.mjs";
import { main as runWorkgraphCommand } from "../pulse_work.mjs";

export { main as workgraphMain } from "../pulse_work.mjs";

export function main(argv = process.argv.slice(2), context = {}) {
  return runWorkgraphCommand(argv, context);
}

if (isDirectExecution(import.meta.url)) {
  process.exitCode = await main();
}
