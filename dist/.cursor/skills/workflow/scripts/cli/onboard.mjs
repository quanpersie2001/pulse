import { isDirectExecution } from "../cli_execution.mjs";
import { main } from "../onboard_pulse.mjs";

export * from "../onboard_pulse.mjs";

if (isDirectExecution(import.meta.url)) {
  process.exitCode = main();
}
