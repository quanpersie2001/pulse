import { isDirectExecution } from "../cli_execution.mjs";
import { main } from "../pulse_status.mjs";

export * from "../pulse_status.mjs";

if (isDirectExecution(import.meta.url)) {
  process.exitCode = await main();
}
