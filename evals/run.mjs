#!/usr/bin/env node
// Pulse eval runner (plan 0026 — G3 of plan 0025). Node >= 20, no
// dependencies. Runs paired `claude -p` sessions (arm `with` = the skill/
// prompt text is prepended, arm `without` = not) on a fresh copy of a
// target-repo fixture and grades the run MECHANICALLY against the state
// Pulse recorded — issues.jsonl, receipts, transcript — never with an LLM
// judge.
//
// Usage:
//   node evals/run.mjs <E1|E2|E3> <with|without> [runs=1]
//   node evals/run.mjs grade <workspace-dir>     # re-grade one workspace
//
// Contract (verified — see evals.json .contract): `claude -p
// --output-format json --no-session-persistence --permission-mode
// bypassPermissions`. Workspaces live under evals/.run/ (gitignored). The
// fixture is copied, never used in place; the runner seeds records through
// the real `pulse` CLI so setup state is gate-produced, not hand-written.

import { execFileSync, spawn, spawnSync } from 'node:child_process';
import { cpSync, existsSync, mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..');
const RUN_DIR = join(ROOT, 'evals', '.run');
const TIMEOUT_MS = 20 * 60 * 1000;

const spec = JSON.parse(readFileSync(join(ROOT, 'evals', 'evals.json'), 'utf8'));
const PULSE =
  process.env.PULSE_BIN ??
  (existsSync(join(ROOT, 'target', 'release', 'pulse'))
    ? join(ROOT, 'target', 'release', 'pulse')
    : join(process.env.HOME ?? '', '.cargo', 'bin', 'pulse'));

function sh(cmd, args, opts = {}) {
  return execFileSync(cmd, args, { encoding: 'utf8', ...opts });
}
function pulse(ws, args) {
  return sh(PULSE, ['--repo-root', ws, ...args]);
}
function pulseJson(ws, args) {
  return JSON.parse(pulse(ws, [...args, '--json']));
}
function pulseStatus(ws, args) {
  return spawnSync(PULSE, ['--repo-root', ws, ...args], { encoding: 'utf8' }).status;
}
function writeJson(path, value) {
  writeFileSync(path, JSON.stringify(value, null, 2));
}

// --- seeded records (ground truth lives here, not in hand-written state) --

const STORY = {
  outcome:
    'API keys can authenticate: a new src/apikeys.mjs module provides hashKey(secret) and ' +
    'verifyKey(secret, hash) built on the hashing helpers of src/token.mjs (BR-2: no second ' +
    'hashing implementation), and route handlers in src/keys-routes.mjs consume verifyKey to ' +
    'gate the /keys endpoints. Deliverable 2 consumes Deliverable 1 and cannot start before ' +
    'the module exists. docs/product/authentication.md documents key auth.',
  rules: [
    { id: 'BR-1', text: 'Every /keys endpoint requires a valid API key checked via src/apikeys.mjs verifyKey.' },
    { id: 'BR-2', text: 'API key hashing reuses src/token.mjs hashing helpers; a second hashing implementation is a bug.' },
  ],
  exceptions: [],
  qa_cases: [
    { id: 'QA-001', surface: 'api', priority: 'high', steps: ['GET /keys without a key returns 401'] },
  ],
  open_questions: [],
  docs_written: ['docs/product/authentication.md'],
};

const WORKER_TICKET = {
  objective: 'Implement src/apikeys.mjs (hashKey/verifyKey) with node:test coverage, mirroring the patterns of src/token.mjs.',
  description:
    'Create `src/apikeys.mjs` exporting `hashKey(secret) -> string` and `verifyKey(secret, hash) -> boolean`, ' +
    'using scrypt from node:crypto; reuse any hashing helper that already exists in `src/token.mjs` instead of ' +
    'reimplementing it (BR-2). Add `test/apikeys.test.mjs` covering: hash is not the plaintext, verify accepts ' +
    'the right secret, verify rejects a wrong secret. Keep the existing `node --test` suite green.',
  touches: ['src/apikeys.mjs', 'test/apikeys.test.mjs'],
  change: {
    required: ['src/apikeys.mjs'],
    invariants: ['a hashed key is never reversible to the plaintext'],
    docs_to_update: [],
  },
  non_scope: ['route handlers', 'docs'],
  acceptance: [
    { id: 'AC-1', when: '`node --test` runs the suite', then: 'all tests pass, including the three new apikeys cases' },
  ],
  verify: [{ name: 'node-test', argv: ['node', '--test'] }],
  qa_cases: ['QA-001'],
  open_questions: [],
};

const SEAT_TICKET = {
  objective: 'Trivial guard: src/token.mjs must keep parsing and exporting its existing public functions.',
  description: 'No code change is expected. This ticket exists to exercise the review lane itself; verify runs `node --check src/token.mjs`.',
  touches: ['src/token.mjs'],
  change: { required: [], invariants: [], docs_to_update: [] },
  non_scope: ['everything'],
  acceptance: [{ id: 'AC-1', when: '`node --check src/token.mjs` runs', then: 'it exits 0' }],
  verify: [{ name: 'syntax', argv: ['node', '--check', 'src/token.mjs'] }],
  qa_cases: [],
  open_questions: [],
};

// --- setup ----------------------------------------------------------------

function freshWorkspace(ws) {
  rmSync(ws, { recursive: true, force: true });
  mkdirSync(ws, { recursive: true });
  cpSync(join(ROOT, spec.fixture), ws, { recursive: true });
  sh('git', ['init', '-q'], { cwd: ws });
  sh('git', ['-c', 'user.name=eval', '-c', 'user.email=eval@pulse.test', 'commit', '-q', '-m', 'fixture baseline', '--allow-empty'], { cwd: ws });
  pulseJson(ws, ['init']);
  // The host convention after init: commit the scaffold, so the gates see a
  // clean tree and only real work dirt afterwards (decision 0025 — the same
  // step the golden-path test does).
  sh('git', ['add', '-A'], { cwd: ws });
  sh('git', ['-c', 'user.name=eval', '-c', 'user.email=eval@pulse.test', 'commit', '-q', '-m', 'pulse scaffold'], { cwd: ws });
  return ws;
}

function seedStory(ws) {
  const payload = join(ws, '.pulse', 'runtime', 'eval-story.json');
  writeJson(payload, STORY);
  return pulseJson(ws, ['work', 'new', 'story', 'API key auth', '--from', payload, '--actor', spec.actor.human]).id;
}

// The fixture carries an old v2-era free-form PULSE.md, which the v3
// loader correctly refuses (prose lines parse as a YAML sequence). A real
// v3 target repo authors a YAML PULSE.md — so the workspace copy gets one,
// with the fixture's intent kept as comments. The fixture itself stays
// immutable; golden_path does the same overwrite for the same reason.
function ensureProfile(ws, profile) {
  const path = join(ws, 'PULSE.md');
  writeFileSync(
    path,
    '# Repository Intent\n' +
      '# - Preserve backward-compatible refresh-token outcome names.\n' +
      '# - Do not expose sensitive token-validation details.\n' +
      '# - Prefer deterministic, dependency-free verification.\n' +
      '#\n' +
      '# Human Judgment Boundaries\n' +
      '# - Human approval is required to rename public outcomes or weaken the no-secret-leak invariant.\n' +
      'fence_ignore: []\n' +
      `profiles:\n  ${profile.key}: {lanes: [${profile.lanes.join(', ')}]}\n`,
  );
}

function setupE1(ws) {
  ensureProfile(ws, { key: 'cli-low', lanes: ['review-correctness'] });
  return seedStory(ws);
}

function setupE2(ws) {
  ensureProfile(ws, { key: 'cli-low', lanes: ['review-correctness'] });
  const story = seedStory(ws);
  const payload = join(ws, '.pulse', 'runtime', 'eval-ticket.json');
  writeJson(payload, WORKER_TICKET);
  const { id } = pulseJson(ws, ['work', 'new', 'ticket', 'apikeys module', '--story', story, '--risk', 'medium', '--surface', 'api', '--from', payload, '--actor', spec.actor.human]);
  pulseJson(ws, ['work', 'ready', id]);
  return { id };
}

function setupE3(ws) {
  ensureProfile(ws, { key: 'cli-low', lanes: ['review-correctness'] });
  const story = seedStory(ws);
  const payload = join(ws, '.pulse', 'runtime', 'eval-ticket.json');
  writeJson(payload, SEAT_TICKET);
  const { id } = pulseJson(ws, ['work', 'new', 'ticket', 'review-lane exercise', '--story', story, '--risk', 'low', '--surface', 'cli', '--from', payload, '--actor', spec.actor.human]);
  pulseJson(ws, ['work', 'ready', id]);
  // Drive the ticket to `verifying` exactly the way a worker would, so the
  // seat's lane input is gate-produced state, never hand-written. Claim
  // first: the packet's protocol.run_id only exists once the lease does
  // (the same order the 0025 dogfood F9 paid for).
  pulseJson(ws, ['claim', id, '--actor', spec.actor.worker]);
  const runId = pulseJson(ws, ['packet', id]).protocol.run_id;
  pulse(ws, ['verify', id, '--actor', spec.actor.worker]);
  const handoff = join(ws, '.pulse', 'runtime', `handoff-${id.toLowerCase()}.json`);
  writeJson(handoff, {
    run_id: runId,
    summary: 'No change needed; lane exercise.',
    changed_files: [],
    acceptance: [{ id: 'AC-1', status: 'done', how: 'node --check src/token.mjs exits 0' }],
    docs_updated: [],
    learnings_used: [],
    friction: [],
    open_risks: [],
  });
  pulseJson(ws, ['handoff', id, '--from', handoff, '--actor', spec.actor.worker]);
  pulseJson(ws, ['lane', 'input', id, 'review-correctness']);
  return { id };
}

// --- prompts --------------------------------------------------------------

function basePrompt(id, subject) {
  if (id === 'E1') {
    return 'You are planning in the git repo you are running in. A story is already created and ready. ' +
      'Cut it into tickets: create every ticket record with `pulse work new ticket`, wire any needed ' +
      '`pulse work dep add` edges, and get every ticket through `pulse work ready`. Run every pulse ' +
      `command with --actor ${spec.actor.human}. Stop when every ticket is ready and say what you cut.`;
  }
  if (id === 'E2') {
    return `You are the worker for ticket ${subject.id} in the git repo you are running in. Read the packet ` +
      `(\`pulse packet ${subject.id}\`) and take the ticket end to end to a successful handoff — implement, ` +
      '`pulse verify`, `pulse handoff`. Run every pulse command with ' +
      `--actor ${spec.actor.worker}. Stop after the handoff is accepted and say what you did.`;
  }
  return `You are a review seat for ticket ${subject.id} in the git repo you are running in. The lane input ` +
    `file is at .pulse/runtime/lane/${subject.id}/review-correctness-input.json — read only that and the ` +
    'repository tree it names, write your verdict file under .pulse/evidence/, and seal with ' +
    '`pulse lane seal`. Run every pulse command with ' +
    `--actor ${spec.actor.seat}. Stop after the seal is accepted (or report exactly why it was refused).`;
}

// --- graders (mechanical only — no LLM judge) -----------------------------

function readIssues(ws) {
  return readFileSync(join(ws, '.pulse', 'issues.jsonl'), 'utf8')
    .split('\n')
    .filter((line) => line.trim())
    .map((line) => JSON.parse(line));
}

function storyTickets(ws) {
  return readIssues(ws).filter((r) => r.kind === 'ticket' && r.story);
}

function receipts(ws) {
  const dir = join(ws, '.pulse', 'receipts');
  if (!existsSync(dir)) return [];
  const out = [];
  for (const file of readdirSync(dir)) {
    if (!file.endsWith('.jsonl')) continue;
    for (const line of readFileSync(join(dir, file), 'utf8').split('\n')) {
      if (line.trim()) out.push(JSON.parse(line));
    }
  }
  return out;
}

// The same small glob grammar `source::glob_match` implements: an exact
// path, `dir/` (everything under it), `dir/**`, or one `*` in one segment.
function globMatch(pattern, path) {
  if (pattern === path) return true;
  if (pattern.endsWith('/**')) return path.startsWith(pattern.slice(0, -3));
  if (pattern.endsWith('/')) return path.startsWith(pattern);
  if (pattern.includes('*')) {
    const [head, ...rest] = pattern.split('*');
    if (!path.startsWith(head)) return false;
    const tail = rest.join('*');
    const tailStart = path.length - tail.length;
    return tailStart >= head.length && path.slice(tailStart) === tail && !path.slice(head.length, tailStart).includes('/');
  }
  return false;
}

function graderReadyAll(ws) {
  const tickets = storyTickets(ws);
  if (tickets.length < 2) return [false, `expected >= 2 tickets, got ${tickets.length}`];
  for (const ticket of tickets) {
    if (pulseStatus(ws, ['work', 'ready', ticket.id]) !== 0) {
      return [false, `${ticket.id} does not pass the ready gate`];
    }
  }
  return [true, `${tickets.length} tickets pass the ready gate`];
}

function graderTicketsHaveVerify(ws) {
  const missing = storyTickets(ws).filter((t) => !Array.isArray(t.verify) || t.verify.length === 0);
  return missing.length === 0
    ? [true, 'every ticket declares verify[]']
    : [false, `no verify[] on: ${missing.map((t) => t.id).join(', ')}`];
}

function graderSingleDocOwner(ws, doc) {
  const tickets = storyTickets(ws);
  const owners = tickets.filter((t) => (t.change?.docs_to_update ?? []).includes(doc));
  if (owners.length !== 1) {
    return [false, `${owners.length} tickets declare ${doc} in docs_to_update (want exactly 1)`];
  }
  const owner = owners[0];
  const held = (owner.touches ?? []).some((glob) => globMatch(glob, doc));
  return held
    ? [true, `${owner.id} owns ${doc} and holds it in touches`]
    : [false, `${owner.id} declares ${doc} but its touches do not cover it: ${JSON.stringify(owner.touches)}`];
}

function graderPrereqEdge(ws, { creator_glob, consumer_glob }) {
  const needle = (glob) => glob.replace(/^src\//, '').replace(/\.(mjs|js|ts)$/, '');
  const tickets = storyTickets(ws);
  const creators = tickets.filter((t) => (t.touches ?? []).join(' ').includes(needle(creator_glob)));
  const consumers = tickets.filter((t) => (t.touches ?? []).join(' ').includes(needle(consumer_glob)));
  if (creators.length === 0 || consumers.length === 0) {
    return [false, `could not identify creator (${creator_glob}) / consumer (${consumer_glob}) from touches: ${JSON.stringify(tickets.map((t) => t.touches))}`];
  }
  for (const consumer of consumers) {
    for (const creator of creators) {
      if ((consumer.deps ?? []).some((d) => d.type === 'blocked_by' && d.id === creator.id)) {
        return [true, `${consumer.id} blocked_by ${creator.id}`];
      }
    }
  }
  return [false, `no blocked_by edge from a ${consumer_glob} ticket to a ${creator_glob} ticket`];
}

function graderReceiptExists(ws, receipt_kind) {
  const found = receipts(ws).filter((r) => r.kind === receipt_kind);
  return found.length > 0 ? [true, `${found.length} ${receipt_kind} receipt(s)`] : [false, `no ${receipt_kind} receipt`];
}

function graderNoScratchAtRoot(ws) {
  const bad = readdirSync(ws).filter((name) => /^(cp|handoff).*\.json$/i.test(name));
  return bad.length === 0 ? [true, 'no scratch payload at the repo root'] : [false, `scratch at root: ${bad.join(', ')}`];
}

function graderTranscriptNotContains(ws, patterns) {
  const transcript = readFileSync(join(ws, 'transcript.json'), 'utf8');
  const hit = patterns.filter((p) => transcript.includes(p));
  return hit.length === 0 ? [true, 'transcript clean of refusal codes'] : [false, `transcript contains: ${hit.join(', ')}`];
}

function graderTurnsAtMost(ws, n) {
  const t = JSON.parse(readFileSync(join(ws, 'transcript.json'), 'utf8'));
  return t.num_turns <= n ? [true, `${t.num_turns} turns <= ${n}`] : [false, `${t.num_turns} turns > ${n}`];
}

function graderSealAttemptsAtMost(ws, n) {
  const transcript = readFileSync(join(ws, 'transcript.json'), 'utf8');
  const attempts = (transcript.match(/lane seal/g) ?? []).length;
  return attempts <= n ? [true, `${attempts} seal attempt(s) <= ${n}`] : [false, `${attempts} seal attempts > ${n}`];
}

function graderOutcomeRecorded(ws) {
  const sealed = receipts(ws).some((r) => r.kind === 'lane' && r.actor === spec.actor.seat);
  if (sealed) return [true, 'lane receipt sealed by the seat'];
  const transcript = readFileSync(join(ws, 'transcript.json'), 'utf8');
  return /refus/i.test(transcript)
    ? [true, 'no seal, but the seat reported the refusal in prose']
    : [false, 'neither a sealed receipt nor a reported refusal'];
}

const GRADERS = {
  ready_all: (ws) => graderReadyAll(ws),
  tickets_have_verify: (ws) => graderTicketsHaveVerify(ws),
  single_doc_owner: (ws, g) => graderSingleDocOwner(ws, g.doc),
  prereq_edge: (ws, g) => graderPrereqEdge(ws, g),
  receipt_exists: (ws, g) => graderReceiptExists(ws, g.receipt_kind),
  no_scratch_at_root: (ws) => graderNoScratchAtRoot(ws),
  transcript_not_contains: (ws, g) => graderTranscriptNotContains(ws, g.patterns),
  turns_at_most: (ws, g) => graderTurnsAtMost(ws, g.n),
  seal_attempts_at_most: (ws, g) => graderSealAttemptsAtMost(ws, g.n),
  outcome_recorded: (ws) => graderOutcomeRecorded(ws),
};

function grade(ws, evalId) {
  const def = spec.evals.find((e) => e.id === evalId);
  const results = def.graders.map((grader) => {
    let pass;
    let detail;
    try {
      [pass, detail] = GRADERS[grader.kind](ws, grader);
    } catch (error) {
      [pass, detail] = [false, `grader crashed: ${error.message}`];
    }
    return { grader: grader.kind, pass, detail };
  });
  return { eval: evalId, ws, results, passed: results.every((r) => r.pass) };
}

// --- run ------------------------------------------------------------------

function runClaude(ws, prompt) {
  writeFileSync(join(ws, 'prompt.txt'), prompt);
  const args = [
    '-p', prompt,
    '--output-format', 'json',
    '--no-session-persistence',
    '--permission-mode', 'bypassPermissions',
  ];
  return new Promise((resolve) => {
    const child = spawn('claude', args, { cwd: ws, stdio: ['ignore', 'pipe', 'pipe'] });
    let out = '';
    const timer = setTimeout(() => child.kill('SIGKILL'), TIMEOUT_MS);
    child.stdout.on('data', (chunk) => (out += chunk));
    child.on('close', () => {
      clearTimeout(timer);
      writeFileSync(join(ws, 'transcript.json'), out || '{"is_error": true, "num_turns": -1, "total_cost_usd": 0}');
      resolve(out);
    });
  });
}

async function runEval(id, arm, runs) {
  const def = spec.evals.find((e) => e.id === id);
  if (!def) throw new Error(`unknown eval ${id}`);
  const skillText =
    arm === 'with'
      ? readFileSync(join(ROOT, def.skill_file), 'utf8') + '\n\n---\nFollow the guidance text above exactly.\n\n'
      : '';
  const outcomes = [];
  for (let i = 1; i <= runs; i++) {
    const ws = join(RUN_DIR, `${id}-${arm}-${String(i).padStart(2, '0')}`);
    console.log(`[eval] ${id} ${arm} run ${i} -> ${ws}`);
    freshWorkspace(ws);
    const subject = id === 'E1' ? setupE1(ws) : id === 'E2' ? setupE2(ws) : setupE3(ws);
    await runClaude(ws, skillText + basePrompt(id, subject));
    const verdict = grade(ws, id);
    writeFileSync(join(ws, 'grade.json'), JSON.stringify(verdict, null, 2));
    const transcript = JSON.parse(readFileSync(join(ws, 'transcript.json'), 'utf8'));
    outcomes.push({ run: ws, passed: verdict.passed, turns: transcript.num_turns, cost: transcript.total_cost_usd });
    console.log(`[eval] ${id} ${arm} run ${i}: ${verdict.passed ? 'PASS' : 'FAIL'} (turns=${transcript.num_turns})`);
    for (const r of verdict.results) console.log(`        ${r.pass ? 'ok  ' : 'FAIL'} ${r.grader}: ${r.detail}`);
  }
  console.log(`[eval] ${id} ${arm}: ${outcomes.filter((o) => o.passed).length}/${runs} passed`);
  return outcomes;
}

const [command, arm, runs] = process.argv.slice(2);
if (command?.match(/^E[123]$/) && ['with', 'without'].includes(arm)) {
  runEval(command, arm, Number(runs ?? 1)).then(undefined, (error) => {
    console.error(error);
    process.exit(1);
  });
} else if (command === 'grade' && arm) {
  const meta = JSON.parse(readFileSync(join(arm, 'grade.json'), 'utf8'));
  console.log(JSON.stringify(grade(arm, meta.eval), null, 2));
} else {
  console.error('usage: node evals/run.mjs <E1|E2|E3> <with|without> [runs]\n       node evals/run.mjs grade <workspace-dir>');
  process.exit(2);
}
