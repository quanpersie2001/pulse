#!/usr/bin/env node

import path from "node:path";
import { fileURLToPath } from "node:url";

export * from "../../pulse/scripts/runtime/pulse_reservations.mjs";
import { main } from "../../pulse/scripts/runtime/pulse_reservations.mjs";

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  process.exitCode = main(process.argv.slice(2));
}
