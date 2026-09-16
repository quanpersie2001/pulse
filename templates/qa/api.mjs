#!/usr/bin/env node
// Pulse QA lane: qa-api (plan 0022 §8.6). Node >= 20, no bundled dependency.
//
// Usage: node scripts/qa/api.mjs <input.json>
// Input: the qa-api lane input (plan §8.3): qa_cases[], evidence_dir,
// handoff_commit. Output: <evidence_dir>/qa-api.json (plan §8.4), then a
// final stdout line {"status":"done"}. See scripts/qa/README.md for the
// docs/operations/run.md `pulse-run` block format this reads.

import { readFile, writeFile, mkdir } from 'node:fs/promises';
import { mkdirSync, openSync } from 'node:fs';
import { execFile, spawn } from 'node:child_process';
import { promisify } from 'node:util';
import path from 'node:path';

const execFileAsync = promisify(execFile);

function parsePulseRunBlocks(text) {
  const blocks = [];
  const pattern = /```pulse-run\n([\s\S]*?)\n```/g;
  let match;
  while ((match = pattern.exec(text)) !== null) {
    const fields = {};
    for (const line of match[1].split('\n')) {
      const colon = line.indexOf(':');
      if (colon === -1) continue;
      const key = line.slice(0, colon).trim();
      let value = line.slice(colon + 1).trim();
      fields[key] = value.startsWith('[') ? JSON.parse(value) : value.replace(/^"(.*)"$/, '$1');
    }
    blocks.push(fields);
  }
  return blocks;
}

async function readRunConfig(repoRoot, id) {
  let text;
  try {
    text = await readFile(path.join(repoRoot, 'docs/operations/run.md'), 'utf8');
  } catch {
    return { error: 'docs/operations/run.md is missing' };
  }
  const block = parsePulseRunBlocks(text).find((candidate) => candidate.id === id);
  if (!block) {
    return { error: `docs/operations/run.md has no \`\`\`pulse-run block with id: ${id}` };
  }
  for (const field of ['start', 'ready_url', 'stop', 'log']) {
    if (!(field in block)) {
      return { error: `docs/operations/run.md's id: ${id} pulse-run block is missing ${field}` };
    }
  }
  return { config: block };
}

function isRunning(pid) {
  try {
    process.kill(pid, 0);
    return true;
  } catch {
    return false;
  }
}

// The detached `start` may keep working after something already answers:
// `docker compose up -d --build` recreates the container even on a cached
// build, and the old instance goes down for several seconds while the new
// one comes up — so a 200 from `ready_url` can come from an instance that
// is about to be replaced (dogfood ST-1, F2). With `await_exit: true` (the
// block default) the script therefore waits for the start command to exit
// first (bounded), then requires `ready_url` to answer 200 several times
// in a row so a recreate blip cannot count as ready. A start command that
// never exits (a long-running dev server) must set `await_exit: false` —
// the wait is capped and cannot tell a dev server from a pending compose.
async function waitForStartThenReady(pid, url, awaitExit) {
  if (awaitExit) {
    const exitDeadline = Date.now() + 120000;
    while (Date.now() < exitDeadline && isRunning(pid)) {
      await new Promise((resolve) => setTimeout(resolve, 1000));
    }
  }
  let consecutive = 0;
  const readyDeadline = Date.now() + 60000;
  while (Date.now() < readyDeadline) {
    try {
      const response = await fetch(url);
      consecutive = response.status === 200 ? consecutive + 1 : 0;
      if (consecutive >= 3) return true;
    } catch {
      consecutive = 0;
    }
    await new Promise((resolve) => setTimeout(resolve, 1000));
  }
  return false;
}

async function runProcess(argv, cwd) {
  return execFileAsync(argv[0], argv.slice(1), { cwd });
}

// `start` is spawned detached into its own process group so a hanging
// command cannot take the whole lane down; waitForStartThenReady then waits
// for it in a bounded way before trusting `ready_url` (dogfood ST-1, F2).
function startApp(argv, cwd, logPath) {
  const fullLogPath = path.join(cwd, logPath);
  mkdirSync(path.dirname(fullLogPath), { recursive: true });
  const out = openSync(fullLogPath, 'a');
  const child = spawn(argv[0], argv.slice(1), {
    cwd,
    detached: true,
    stdio: ['ignore', out, out],
  });
  // Without a listener, a failed spawn (e.g. command not found) would throw
  // as an uncaught 'error' event; let ready_url's own timeout report it
  // instead.
  child.on('error', () => {});
  child.unref();
  return child.pid;
}

// `stop` is expected to actually terminate the app; if it fails, fall back
// to killing the process group `start` was detached into (best-effort).
async function stopApp(argv, cwd, pid) {
  try {
    await runProcess(argv, cwd);
    return 0;
  } catch (stopError) {
    if (pid) {
      try {
        process.kill(-pid, 'SIGTERM');
      } catch {
        // already gone
      }
    }
    return typeof stopError.code === 'number' ? stopError.code : 1;
  }
}

function matchesAssert(assertions, exitCode) {
  if (!assertions || assertions.length === 0) return exitCode === 0;
  return assertions.every((assertion) =>
    typeof assertion.exit_code === 'number' ? assertion.exit_code === exitCode : true
  );
}

function inconclusiveReport(commit, message, owner) {
  return {
    verdict: 'inconclusive',
    acceptance: [],
    cases: [],
    findings: [
      { id: 'F-1', ref: '-', summary: message, owner, check: null, severity: 'medium', status: 'open' },
    ],
    commands_run: [],
    environment: { commit, server: null, tool: 'fetch' },
  };
}

async function currentCommit(repoRoot) {
  try {
    return (await runProcess(['git', 'rev-parse', 'HEAD'], repoRoot)).stdout.trim();
  } catch {
    return '';
  }
}

// Parse one `METHOD /path [json-body]` step line.
function parseStep(step) {
  const firstSpace = step.indexOf(' ');
  const secondSpace = step.indexOf(' ', firstSpace + 1);
  const method = step.slice(0, firstSpace);
  const urlPath = secondSpace === -1 ? step.slice(firstSpace + 1) : step.slice(firstSpace + 1, secondSpace);
  const bodyText = secondSpace === -1 ? undefined : step.slice(secondSpace + 1).trim();
  return { method, urlPath, body: bodyText ? JSON.parse(bodyText) : undefined };
}

async function tailLines(filePath, count) {
  try {
    const text = await readFile(filePath, 'utf8');
    return text.split('\n').slice(-count).join('\n');
  } catch {
    return `(no log at ${filePath})`;
  }
}

async function main() {
  const inputPath = process.argv[2];
  if (!inputPath) {
    console.error('usage: node api.mjs <input.json>');
    process.exit(2);
  }
  const input = JSON.parse(await readFile(inputPath, 'utf8'));
  const repoRoot = process.cwd();
  const evidenceDir = path.join(repoRoot, input.evidence_dir);
  await mkdir(path.join(evidenceDir, 'logs'), { recursive: true });
  const outputPath = path.join(evidenceDir, 'qa-api.json');
  const commit = await currentCommit(repoRoot);

  const { config, error } = await readRunConfig(repoRoot, 'api');
  if (error) {
    await writeFile(outputPath, JSON.stringify(inconclusiveReport(commit, error, 'docs/operations/run.md'), null, 2));
    console.log(JSON.stringify({ status: 'done' }));
    return;
  }

  const pid = startApp(config.start, repoRoot, config.log);
  let report;
  try {
    if (!(await waitForStartThenReady(pid, config.ready_url, config.await_exit !== 'false'))) {
      report = inconclusiveReport(commit, `${config.ready_url} never returned 200`, config.ready_url);
    } else {
      const cases = [];
      // A story-scope input carries the Story's whole qa_cases[]; this lane
      // only runs its own surface's cases — the other surface's steps are
      // not `METHOD /path` lines and would crash parseStep (dogfood ST-1,
      // F16).
      const apiCases = (input.qa_cases || []).filter((c) => !c.surface || c.surface === 'api');
      for (const qaCase of apiCases) {
        const httpLines = [];
        const created = [];
        for (const step of qaCase.steps || []) {
          const { method, urlPath, body } = parseStep(step);
          const url = new URL(urlPath, config.ready_url).toString();
          const response = await fetch(url, {
            method,
            headers: body === undefined ? {} : { 'content-type': 'application/json' },
            body: body === undefined ? undefined : JSON.stringify(body),
          });
          const responseText = await response.text();
          httpLines.push(`> ${step}`, `< ${response.status}`, responseText, '');
          // Remember what a POST created so the case can delete it after
          // itself (dogfood ST-1, F20 — lane steps used to leave residue
          // rows in the dev database).
          if (method === 'POST' && response.ok) {
            try {
              const created_id = JSON.parse(responseText).id;
              if (created_id) created.push({ urlPath, id: created_id });
            } catch {
              // non-JSON body — nothing to track
            }
          }
        }
        // Cleanup before the transcript is written, so the artifact shows
        // the steps' effects AND their cleanup (dogfood ST-1, F20).
        for (const { urlPath, id } of created) {
          const cleanupUrl = new URL(`${urlPath}/${id}`, config.ready_url).toString();
          try {
            const cleanup = await fetch(cleanupUrl, { method: 'DELETE' });
            httpLines.push(`> DELETE ${urlPath}/${id} (cleanup)`, `< ${cleanup.status}`, '');
          } catch (cleanupError) {
            httpLines.push(`> DELETE ${urlPath}/${id} (cleanup)`, `< ${cleanupError.message}`, '');
          }
        }
        await writeFile(path.join(evidenceDir, 'logs', `${qaCase.id}.http.txt`), httpLines.join('\n'));
        const logPath = path.join(repoRoot, config.log);
        await writeFile(path.join(evidenceDir, 'logs', `${qaCase.id}.server.txt`), await tailLines(logPath, 200));

        let status = 'inconclusive';
        let checkDetail = '';
        if (qaCase.check) {
          let exitCode = 0;
          try {
            await runProcess(qaCase.check.argv, repoRoot);
          } catch (checkError) {
            exitCode = typeof checkError.code === 'number' ? checkError.code : 1;
            const output = [checkError.stdout, checkError.stderr].filter(Boolean).join('\n').trim();
            checkDetail = output.split('\n').pop() ?? '';
          }
          status = matchesAssert(qaCase.check.assert, exitCode) ? 'pass' : 'fail';
        }
        // A failed check's own last output line rides along in `observation`
        // so the receipt alone is diagnosable (dogfood ST-1, F19).
        const observation = checkDetail
          ? `${(qaCase.steps || []).join(' | ')} || check: ${checkDetail}`
          : (qaCase.steps || []).join(' | ');
        cases.push({
          id: qaCase.id,
          status,
          observation,
          artifacts: [`logs/${qaCase.id}.http.txt`, `logs/${qaCase.id}.server.txt`],
        });
      }

      const verdict = cases.some((c) => c.status === 'fail')
        ? 'fail'
        : cases.some((c) => c.status === 'inconclusive')
          ? 'inconclusive'
          : 'pass';

      report = {
        verdict,
        acceptance: [],
        cases,
        findings: [],
        commands_run: [],
        environment: { commit, server: config.ready_url, tool: 'fetch' },
      };
    }
  } finally {
    const stopExit = await stopApp(config.stop, repoRoot, pid);
    if (report) {
      report.commands_run = [
        { argv: config.start, exit: null, detached: true },
        { argv: config.stop, exit: stopExit },
      ];
    }
  }
  await writeFile(outputPath, JSON.stringify(report, null, 2));
  console.log(JSON.stringify({ status: 'done' }));
}

main().catch((error) => {
  console.error(JSON.stringify({ schema_version: 1, code: 'qa_api_crashed', message: String(error) }));
  process.exit(1);
});
