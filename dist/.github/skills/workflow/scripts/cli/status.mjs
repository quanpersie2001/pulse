import { main as runStatusCommand } from "../pulse_status.mjs";

export * from "../pulse_status.mjs";

export function main(argv = process.argv.slice(2), context = {}) {
  return runStatusCommand(argv, context);
}
