import { isDirectExecution } from "../cli_execution.mjs";
import { main } from "../pulse_session_load.mjs";

export * from "../pulse_session_load.mjs";

if (isDirectExecution(import.meta.url)) {
  process.exitCode = main();
}
