import { isDirectExecution } from "../cli_execution.mjs";
import { main as runReservationCommand } from "../pulse_reservations.mjs";

export * from "../pulse_reservations.mjs";

export function main(argv = process.argv.slice(2), context = {}) {
  return runReservationCommand(argv, context);
}

if (isDirectExecution(import.meta.url)) {
  process.exitCode = main();
}
