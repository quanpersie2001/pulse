#!/usr/bin/env node

import path from "node:path";
import { fileURLToPath } from "node:url";

export * from "../../../../using-pulse/scripts/onboard_pulse.mjs";
import { main } from "../../../../using-pulse/scripts/onboard_pulse.mjs";

const SCRIPT_PATH = fileURLToPath(import.meta.url);

if (process.argv[1] && path.resolve(process.argv[1]) === SCRIPT_PATH) {
  process.exitCode = main(process.argv.slice(2));
}
