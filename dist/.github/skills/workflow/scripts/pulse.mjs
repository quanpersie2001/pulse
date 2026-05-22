#!/usr/bin/env node

import fs from "node:fs";
import { fileURLToPath } from "node:url";

import { main } from "./cli/router.mjs";

function isDirectExecution(metaUrl, entryPath = process.argv[1]) {
  if (!entryPath) {
    return false;
  }
  try {
    return fs.realpathSync(fileURLToPath(metaUrl)) === fs.realpathSync(entryPath);
  } catch {
    return false;
  }
}

if (isDirectExecution(import.meta.url)) {
  process.exitCode = await main();
}
