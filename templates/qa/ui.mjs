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
async function waitForStartExit(pid, awaitExit) {
  if (!awaitExit) return;
  const exitDeadline = Date.now() + 120000;
  while (Date.now() < exitDeadline && isRunning(pid)) {
    await new Promise((resolve) => setTimeout(resolve, 1000));
  }
}

// `ready_url` must answer 200 several times in a row so a recreate blip
// cannot count as ready.
async function pollReady(url) {
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

// The block's optional `migrate: [argv]` key runs after `start` has settled
// and before `ready_url` is polled (dogfood ST-2, F26/F29: a fresh db
// volume stays on its old schema forever when no lane step ever runs the
// migrations). Absent or empty means the app migrates itself — skipped. A
// failed or timed-out migrate never reaches the QA cases; the caller turns
// the returned error into an inconclusive finding, and the command's tail
// rides along in logs/migrate.txt (a crash whose output is thrown away is
// exactly what made F28 undiagnosable).
async function runMigrate(config, repoRoot, evidenceDir) {
  const argv = config.migrate;
  if (!Array.isArray(argv) || argv.length === 0) return { exit: null, error: null };
  const logPath = path.join(evidenceDir, 'logs', 'migrate.txt');
  try {
    const { stdout, stderr } = await execFileAsync(argv[0], argv.slice(1), {
      cwd: repoRoot,
      timeout: 120000,
    });
    await writeFile(logPath, [stdout, stderr].filter(Boolean).join('\n'));
    return { exit: 0, error: null };
  } catch (error) {
    const output = [error.stdout, error.stderr].filter(Boolean).join('\n').trim();
    await writeFile(logPath, output || String(error));
    if (error.killed) return { exit: 1, error: 'timed out after 120s' };
    const lastLine = output.split('\n').pop() || '';
    return {
      exit: typeof error.code === 'number' ? error.code : 1,
      error: `exit ${typeof error.code === 'number' ? error.code : '?'}: ${lastLine}`,
    };
  }
}

async function runProcess(argv, cwd) {
  return execFileAsync(argv[0], argv.slice(1), { cwd });
}

// `start` is spawned detached into its own process group so a hanging
// command cannot take the whole lane down; waitForStartExit then waits for
// it in a bounded way, runMigrate runs the optional `migrate` argv, and
// only then does pollReady trust `ready_url` (dogfood ST-1, F2).
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
  let migrateExit = null;
  let browser;
  let report;
  try {
    await waitForStartExit(pid, config.await_exit !== 'false');
    const migrate = await runMigrate(config, repoRoot, evidenceDir);
    migrateExit = migrate.exit;
    if (migrate.error) {
      report = inconclusiveReport(
        commit,
        `migrate (${config.migrate.join(' ')}) ${migrate.error}`,
        'run.md migrate'
      );
    } else if (!(await pollReady(config.ready_url))) {
      report = inconclusiveReport(commit, `${config.ready_url} never returned 200`, config.ready_url);
    } else {
      browser = await playwright.chromium.launch();
      const viewports = input.viewports && input.viewports.length > 0 ? input.viewports : ['1280x800'];
      const cases = [];
      // A story-scope input carries the Story's whole qa_cases[]; this lane
      // only runs its own surface's cases (dogfood ST-1, F16).
      const uiCases = (input.qa_cases || []).filter((c) => !c.surface || c.surface === 'ui');
      for (const qaCase of uiCases) {
        const [url, ...rest] = qaCase.steps || [];
        const consoleLines = [];
        for (const viewport of viewports) {
          const [width, height] = viewport.split('x').map(Number);
          const page = await browser.newPage({ viewport: { width, height } });
          page.on('console', (message) => consoleLines.push(`[${viewport}] ${message.type()}: ${message.text()}`));
          if (url) await page.goto(url);
          // Let client-side fetches settle so evidence shows the rendered
          // page rather than a loading state (dogfood ST-1, F22).
          await page.waitForLoadState('networkidle', { timeout: 5000 }).catch(() => {});
          await page.screenshot({ path: path.join(evidenceDir, 'shots', `${qaCase.id}-${viewport}.png`) });
          if (viewport === viewports[0]) {
            // playwright >= 1.45 removed page.accessibility.snapshot(); the
            // supported replacement is locator.ariaSnapshot() — a YAML
            // string, written as-is (dogfood ST-1, F1).
            let a11y;
            try {
              a11y = await page.locator('body').ariaSnapshot();
            } catch (a11yError) {
              a11y = `(ariaSnapshot unavailable: ${a11yError.message})`;
            }
            await writeFile(path.join(evidenceDir, 'logs', `${qaCase.id}.a11y.txt`), String(a11y));
          }
          await page.close();
        }
        await writeFile(path.join(evidenceDir, 'logs', `${qaCase.id}.console.txt`), consoleLines.join('\n'));

        const artifacts = viewports
          .map((viewport) => `shots/${qaCase.id}-${viewport}.png`)
          .concat([`logs/${qaCase.id}.console.txt`, `logs/${qaCase.id}.a11y.txt`]);

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
          ? `${rest.join(' | ')} || check: ${checkDetail}`
          : rest.join(' | ');
        cases.push({ id: qaCase.id, status, observation, artifacts });
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
        ...(migrateExit !== null ? [{ argv: config.migrate, exit: migrateExit }] : []),
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
