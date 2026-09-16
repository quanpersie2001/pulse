#!/usr/bin/env node
// Pulse QA lane: qa-ui (plan 0022 §8.6). Node >= 20, no bundled dependency —
// playwright loads dynamically from this repo's own node_modules.
//
// Usage: node scripts/qa/ui.mjs <input.json>
// Input: the qa-ui lane input (plan §8.3): qa_cases[], evidence_dir,
// handoff_commit, viewports. Output: <evidence_dir>/qa-ui.json (plan §8.4),
// then a final stdout line {"status":"done"}. See scripts/qa/README.md for
// the docs/operations/run.md `pulse-run` block format this reads.

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

async function waitForReady(url, timeoutMs = 60000) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    try {
      const response = await fetch(url);
      if (response.status === 200) return true;
    } catch {
      // not up yet
    }
    await new Promise((resolve) => setTimeout(resolve, 500));
  }
  return false;
}

async function runProcess(argv, cwd) {
  return execFileAsync(argv[0], argv.slice(1), { cwd });
}

// `start` is often a long-running dev server that never exits on its own
// (plan §8.6's own `pnpm dev` example) — waiting for it the way runProcess
// does would hang until Pulse's lane timeout. Spawn it detached into its own
// process group and return immediately; `ready_url` polling is the actual
// readiness signal, not this call returning.
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
    environment: { commit, server: null, tool: 'playwright' },
  };
}

async function currentCommit(repoRoot) {
  try {
    return (await runProcess(['git', 'rev-parse', 'HEAD'], repoRoot)).stdout.trim();
  } catch {
    return '';
  }
}

async function main() {
  const inputPath = process.argv[2];
  if (!inputPath) {
    console.error('usage: node ui.mjs <input.json>');
    process.exit(2);
  }
  const input = JSON.parse(await readFile(inputPath, 'utf8'));
  const repoRoot = process.cwd();
  const evidenceDir = path.join(repoRoot, input.evidence_dir);
  await mkdir(path.join(evidenceDir, 'shots'), { recursive: true });
  await mkdir(path.join(evidenceDir, 'logs'), { recursive: true });
  const outputPath = path.join(evidenceDir, 'qa-ui.json');
  const commit = await currentCommit(repoRoot);

  const { config, error } = await readRunConfig(repoRoot, 'ui');
  if (error) {
    await writeFile(outputPath, JSON.stringify(inconclusiveReport(commit, error, 'docs/operations/run.md'), null, 2));
    console.log(JSON.stringify({ status: 'done' }));
    return;
  }

  let playwright;
  try {
    playwright = await import('playwright');
  } catch (importError) {
    console.error(
      JSON.stringify({
        schema_version: 1,
        code: 'qa_ui_playwright_missing',
        message: `playwright is not installed in this repo: ${importError.message}`,
        hint: 'install it in the target repo, e.g. `npm install -D playwright`',
      })
    );
    process.exit(1);
  }

  const pid = startApp(config.start, repoRoot, config.log);
  let browser;
  let report;
  try {
    if (!(await waitForReady(config.ready_url))) {
      report = inconclusiveReport(commit, `${config.ready_url} never returned 200`, config.ready_url);
    } else {
      browser = await playwright.chromium.launch();
      const viewports = input.viewports && input.viewports.length > 0 ? input.viewports : ['1280x800'];
      const cases = [];
      for (const qaCase of input.qa_cases || []) {
        const [url, ...rest] = qaCase.steps || [];
        const consoleLines = [];
        for (const viewport of viewports) {
          const [width, height] = viewport.split('x').map(Number);
          const page = await browser.newPage({ viewport: { width, height } });
          page.on('console', (message) => consoleLines.push(`[${viewport}] ${message.type()}: ${message.text()}`));
          if (url) await page.goto(url);
          await page.screenshot({ path: path.join(evidenceDir, 'shots', `${qaCase.id}-${viewport}.png`) });
          if (viewport === viewports[0]) {
            const snapshot = await page.accessibility.snapshot();
            await writeFile(path.join(evidenceDir, 'logs', `${qaCase.id}.a11y.txt`), JSON.stringify(snapshot, null, 2));
          }
          await page.close();
        }
        await writeFile(path.join(evidenceDir, 'logs', `${qaCase.id}.console.txt`), consoleLines.join('\n'));

        const artifacts = viewports
          .map((viewport) => `shots/${qaCase.id}-${viewport}.png`)
          .concat([`logs/${qaCase.id}.console.txt`, `logs/${qaCase.id}.a11y.txt`]);

        let status = 'inconclusive';
        if (qaCase.check) {
          let exitCode = 0;
          try {
            await runProcess(qaCase.check.argv, repoRoot);
          } catch (checkError) {
            exitCode = typeof checkError.code === 'number' ? checkError.code : 1;
          }
          status = matchesAssert(qaCase.check.assert, exitCode) ? 'pass' : 'fail';
        }
        cases.push({ id: qaCase.id, status, observation: rest.join(' | '), artifacts });
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
        environment: { commit, server: config.ready_url, tool: 'playwright' },
      };
    }
  } finally {
    if (browser) await browser.close();
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
  console.error(JSON.stringify({ schema_version: 1, code: 'qa_ui_crashed', message: String(error) }));
  process.exit(1);
});
