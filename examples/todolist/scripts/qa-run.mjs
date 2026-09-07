// QA runner role script (role `qa` in .pulse/config/runners.json).
//
// Contract: argv[2] is the path to the run input JSON written by
// `pulse run qa`. The script executes the `pulse-check` block each affected
// baseline case declares and prints exactly one final JSON line on stdout:
//   {"cases":[{"id","status","observation"}],"artifacts":[...],"findings":[...]}
//
// Decision 0010: the script never reads `qa.md`. Everything it needs — argv,
// env, assertions, `$VAR` substitutions — arrives in `qa-input.json`. A case
// without a `check` block is reported `inconclusive` with a reason; the script
// does not interpret `Steps`/`Expected` prose.
//
// It never records receipts, reads no evidence manifest and needs no
// `evidence.record` grant: `pulse run qa` builds and records the
// `qa_checkpoint` receipt itself from the input contract, this output and
// the ingested artifacts (Decision 0014).
//
// Exit code stays 0 when the run produced evidence; product failures are
// reported through case status, not through the exit code.

import { spawnSync } from "node:child_process";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import path from "node:path";
import process from "node:process";

// `$NAME` and `${NAME}` resolve from the input's `variables` map. An unknown
// name is left verbatim: guessing a path is worse than a visibly failing check.
function substitute(value, variables) {
  if (typeof value !== "string") return value;
  return value.replace(/\$\{?([A-Z_][A-Z0-9_]*)\}?/g, (match, name) =>
    Object.hasOwn(variables, name) ? variables[name] : match,
  );
}

function hashFile(filePath) {
  try {
    return createHash("sha256").update(readFileSync(filePath)).digest("hex");
  } catch (error) {
    return error.code === "ENOENT" ? "absent" : `unreadable:${error.code}`;
  }
}

// Evaluates one assertion against the finished process. Returns null when the
// assertion holds, or a one-line description of the discrepancy.
function evaluate(assertion, result, variables) {
  const [[kind, operand]] = Object.entries(assertion);
  const stdout = result.stdout ?? "";
  const stderr = result.stderr ?? "";
  switch (kind) {
    case "exit_code":
      return result.status === operand
        ? null
        : `exit_code expected ${operand}, got ${result.status}`;
    case "stdout_line": {
      const wanted = substitute(operand, variables);
      return stdout.split("\n").some((line) => line.replace(/\r$/, "") === wanted)
        ? null
        : `stdout has no line exactly equal to ${JSON.stringify(wanted)}`;
    }
    case "stdout_contains": {
      const wanted = substitute(operand, variables);
      return stdout.includes(wanted) ? null : `stdout does not contain ${JSON.stringify(wanted)}`;
    }
    case "stderr_contains": {
      const wanted = substitute(operand, variables);
      return stderr.includes(wanted) ? null : `stderr does not contain ${JSON.stringify(wanted)}`;
    }
    case "stdout_json_path": {
      const wanted = operand.equals;
      let value;
      try {
        value = JSON.parse(stdout);
      } catch {
        return "stdout is not JSON";
      }
      for (const key of operand.path.replace(/^\$\.?/, "").split(".").filter(Boolean)) {
        value = value?.[key];
      }
      return JSON.stringify(value) === JSON.stringify(wanted)
        ? null
        : `${operand.path} is ${JSON.stringify(value)}, expected ${JSON.stringify(wanted)}`;
    }
    case "file_unchanged": {
      const target = substitute(operand, variables);
      return hashFile(target) === result.snapshots[target]
        ? null
        : `${target} changed during the check`;
    }
    case "file_contains": {
      const target = substitute(operand.path, variables);
      let content;
      try {
        content = readFileSync(target, "utf8");
      } catch (error) {
        return `${target} is unreadable: ${error.code}`;
      }
      return content.includes(operand.text)
        ? null
        : `${target} does not contain ${JSON.stringify(operand.text)}`;
    }
    default:
      return `unsupported assertion ${kind}`;
  }
}

function runCheck(check, variables) {
  const [command, ...args] = check.run.map((argument) => substitute(argument, variables));
  const env = { ...process.env };
  for (const [key, value] of Object.entries(check.env ?? {})) {
    env[key] = substitute(value, variables);
  }
  // `file_unchanged` compares before and after, so the before-hashes have to
  // be taken while the check has not run yet.
  const snapshots = {};
  for (const assertion of check.assert) {
    if (Object.hasOwn(assertion, "file_unchanged")) {
      const target = substitute(assertion.file_unchanged, variables);
      snapshots[target] = hashFile(target);
    }
  }
  const result = spawnSync(command, args, {
    cwd: check.cwd ? substitute(check.cwd, variables) : process.cwd(),
    env,
    input: check.stdin ? substitute(check.stdin, variables) : undefined,
    encoding: "utf8",
    timeout: (check.timeout_seconds ?? 60) * 1000,
  });
  result.snapshots = snapshots;
  if (result.error) {
    return { status: "failed", observation: `check could not run: ${result.error.message}` };
  }
  for (const assertion of check.assert) {
    const failure = evaluate(assertion, result, variables);
    if (failure) {
      return { status: "failed", observation: failure };
    }
  }
  return { status: "passed", observation: "all assertions passed" };
}

function main() {
  const inputPath = process.argv[2];
  if (!inputPath) {
    console.error("usage: node scripts/qa-run.mjs <input.json>");
    process.exitCode = 2;
    return;
  }
  run(inputPath)
    .then((report) => {
      console.log(JSON.stringify(report));
    })
    .catch((error) => {
      // Harness/infrastructure failure: still honor the final-JSON contract.
      console.log(
        JSON.stringify({
          cases: [],
          artifacts: [],
          findings: [
            { case_id: null, summary: `qa-run crashed: ${error.message}`, owner: "scripts/qa-run.mjs", severity: "high" },
          ],
        }),
      );
      process.exitCode = 1;
    });
}

async function run(inputPath) {
  const input = JSON.parse(await readFile(inputPath, "utf8"));
  const variables = input.variables ?? {};
  const artifactDir = path.resolve(
    path.dirname(path.resolve(inputPath)),
    input.artifact_dir ?? "artifacts",
  );
  await mkdir(artifactDir, { recursive: true });

  const cases = [];
  const findings = [];
  const artifacts = [];

  for (const requested of input.cases ?? []) {
    if (!requested.check) {
      cases.push({
        id: requested.id,
        status: "inconclusive",
        observation: "no executable check is defined for this case",
      });
      continue;
    }
    const outcome = runCheck(requested.check, variables);
    cases.push({ id: requested.id, ...outcome });
  }

  const logPath = path.join(artifactDir, "qa-observations.json");
  await writeFile(
    logPath,
    `${JSON.stringify({ cases, findings }, null, 2)}\n`,
  );
  artifacts.push({ path: logPath, role: "log", case_id: null });

  return { cases, artifacts, findings };
}

main();
