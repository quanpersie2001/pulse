<div align="center">

<img src="assets/logo-combination.svg" alt="Pulse logo" width="420" />

# Pulse

<p><strong>A gated delivery router for Claude Code and Codex</strong></p>

<p>
  <a href=".codex-plugin/plugin.json">
    <img alt="Version" src="https://img.shields.io/badge/version-3.5.3-0F766E?style=flat-square" />
  </a>
  <img alt="License" src="https://img.shields.io/badge/license-MIT-blue?style=flat-square" />
  <a href="skills/workflow">
    <img alt="Router" src="https://img.shields.io/badge/router-pulse%3Aworkflow-8B5CF6?style=flat-square" />
  </a>
</p>

<p><em>Keep agents aligned to approved scope, verified execution, and auditable outcomes.</em></p>

</div>

---

## What is Pulse?

Pulse ships one public workflow router skill, **`pulse:workflow`**, plus packaged standalone utility skills outside that router.

Workflow subcommands are: onboard, explore, brainstorm, plan, validate, swarm, execute, review, and compound. Runtime mutations are handled by the separate CLI **`pulse-work`**, with canonical state in **`.pulse/runtime/`** and canonical metadata in **`.pulse/workgraph/items.jsonl`**.

Standalone utility skills remain packaged separately for focused non-router tasks: `architecture-rescue`, `systematic-debug-fix`, `dev-note`, `dev-note-distil`, `prompt-leverage`, and `gitnexus`.

## The Delivery Chain

1. `pulse:workflow onboard` prepares runtime and readiness.
2. `pulse:workflow explore` locks decisions in feature context artifacts.
3. `pulse:workflow plan` selects shape and execution contract.
4. `pulse:workflow validate` proves feasibility before implementation.
5. `pulse:workflow swarm` or `pulse:workflow execute` delivers approved work.
6. `pulse:workflow review` enforces merge quality gates.
7. `pulse:workflow compound` captures reusable learnings.

### The 4 Human Gates

| Gate | What it blocks |
| --- | --- |
| **Gate 1** | Planning before decisions are locked |
| **Gate 2** | Execution prep before shape approval |
| **Gate 3** | Execution before validated current work approval |
| **Gate 4** | Merge before review completion |

## Why use Pulse

| Problem | Pulse response |
| --- | --- |
| Requirements drift in chat | Lock decisions in context artifacts |
| Plans are plausible but brittle | Validate before execution |
| Parallel workers collide | Coordinate through `pulse-work` + reservations |
| Work is hard to audit later | Preserve artifacts, evidence, and review trail |

## Installation

### Claude Code

```bash
/plugin marketplace add quanpersie2001/pulse
/plugin install pulse@pulse
```

### Codex

```bash
codex plugin marketplace add quanpersie2001/pulse
```

Codex reads the marketplace name from [`.agents/plugins/marketplace.json`](.agents/plugins/marketplace.json), so the installed plugin key is `pulse@pulse-dev`.

### After Install

Start with **`pulse:workflow onboard`** in the target repo to initialize `.pulse/runtime`, `.pulse/workgraph`, and runtime helpers under `.pulse/scripts/`.

## Project Docs

| Read this when you want... | Link |
| --- | --- |
| The architecture and runtime model | [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) |
| A concrete walkthrough | [docs/examples/golden-path.md](docs/examples/golden-path.md) |
| The evaluation workflow | [docs/evaluation/pulse-plugin-eval.md](docs/evaluation/pulse-plugin-eval.md) |

## Maintainer Notes

When public docs or `pulse:workflow` router metadata change:

```bash
bash scripts/check-markdown-links.sh
bash scripts/sync-skills.sh --dry-run
```

Run evaluations through the canonical entrypoint:

```bash
node scripts/pulse-plugin-eval.mjs run
```

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for skill structure, versioning, and PR process.

<div align="center">

MIT License

</div>
