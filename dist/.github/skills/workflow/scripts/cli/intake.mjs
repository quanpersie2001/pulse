import path from "node:path";
import { fileURLToPath } from "node:url";

import { assertBareBooleanOptions, assertKnownOptions, parseCliArgs as parseArgv } from "./args.mjs";
import { normalizeIo, writePayload } from "./io.mjs";
import { readTextIfExists } from "../core/fs.mjs";

const CONTRACT_PATH = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  "..",
  "..",
  "references",
  "intake",
  "command.md",
);

function buildPayload(argv = []) {
  const parsed = parseArgv(argv);
  assertKnownOptions(parsed, ["json"]);
  assertBareBooleanOptions(parsed, ["json"]);

  return {
    asJson: parsed.options.has("json"),
    payload: {
      command: "intake",
      implemented: false,
      request: parsed.positionals.join(" ").trim() || null,
      contract_path: CONTRACT_PATH,
      contract_markdown: readTextIfExists(CONTRACT_PATH).trim(),
    },
  };
}

export function main(argv = process.argv.slice(2), context = {}) {
  const io = normalizeIo(context.io);

  if (argv.includes("--help") || argv.includes("-h")) {
    io.stdout.write(
      `${[
        "Usage: pulse.mjs intake [user input] [--json]",
        "",
        "Displays the current pulse:workflow intake contract from references/intake/command.md.",
      ].join("\n")}\n`,
    );
    return 0;
  }

  const { asJson, payload } = buildPayload(argv);
  writePayload(payload, {
    json: asJson,
    render: (value) => value.contract_markdown,
    output: io.stdout,
  });
  return 0;
}
