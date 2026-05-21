import { main as runWorkgraphCommand } from "./workgraph.mjs";

export function main(argv = process.argv.slice(2), context = {}) {
  return runWorkgraphCommand(["ready", ...argv], context);
}
