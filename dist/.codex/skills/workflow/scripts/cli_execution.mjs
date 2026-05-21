import fs from "node:fs";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

export function isDirectExecution(metaUrl, argvPath = process.argv[1]) {
  if (!argvPath) {
    return false;
  }

  try {
    if (metaUrl === pathToFileURL(argvPath).href) {
      return true;
    }
  } catch {
  }

  const selfPath = fileURLToPath(metaUrl);
  const resolvedArgvPath = path.resolve(argvPath);
  if (resolvedArgvPath === selfPath) {
    return true;
  }

  try {
    return fs.realpathSync.native(resolvedArgvPath) === fs.realpathSync.native(selfPath);
  } catch {
    return false;
  }
}
