import fs from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { pathToFileURL } from "node:url";

export function writeImporter(root, targetPath, name = "module") {
  const safeName = String(name).replace(/[^A-Za-z0-9_.-]+/g, "-");
  const importerPath = path.join(root, `import-${safeName}.mjs`);
  fs.writeFileSync(importerPath, `import ${JSON.stringify(pathToFileURL(targetPath).href)};\n`, "utf8");
  return importerPath;
}

export function importModuleInNode(targetPath, options = {}) {
  const importerPath = writeImporter(options.root, targetPath, options.name);
  return spawnSync(process.execPath, [importerPath], {
    cwd: options.cwd,
    encoding: "utf8",
  });
}
