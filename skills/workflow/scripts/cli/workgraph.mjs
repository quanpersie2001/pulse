import { isDirectExecution } from "../cli_execution.mjs";
import { main } from "../pulse_work.mjs";

export * from "../pulse_work.mjs";

if (isDirectExecution(import.meta.url)) {
  process.exitCode = await main();
}
