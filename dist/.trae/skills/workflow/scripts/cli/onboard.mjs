import { isDirectExecution } from "../cli_execution.mjs";
import { main as runOnboardCommand } from "../onboard_pulse.mjs";

export * from "../onboard_pulse.mjs";

export function main(argv = process.argv.slice(2), context = {}) {
  const [command, ...rest] = argv;
  if (command === "check") {
    return runOnboardCommand(rest, context);
  }
  if (command === "apply") {
    return runOnboardCommand(["--apply", ...rest], context);
  }
  return runOnboardCommand(argv, context);
}

if (isDirectExecution(import.meta.url)) {
  process.exitCode = main();
}
