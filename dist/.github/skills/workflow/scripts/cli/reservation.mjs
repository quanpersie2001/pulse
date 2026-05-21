import { isDirectExecution } from "../cli_execution.mjs";
import { main } from "../pulse_reservations.mjs";

export * from "../pulse_reservations.mjs";

if (isDirectExecution(import.meta.url)) {
  process.exitCode = main();
}
