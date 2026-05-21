---
name: prompt-leverage
description: Use when the user asks to improve, structure, or templatize a prompt before execution by Codex or another AI agent.
metadata:
  version: '1.1'
  dependencies: []
---

# Prompt Leverage

Turn the user's current prompt into a stronger working prompt without changing the underlying intent. Preserve the task, fill in missing execution structure, and add only enough scaffolding to improve reliability.

## Workflow

1. Read the raw prompt and identify the real job to be done.
2. Infer the task type: coding, research, writing, analysis, planning, review, or packaged Pulse domain workflow.
3. If the prompt targets a packaged Pulse domain workflow, load that domain skill's workflow rules and templates as source material before rewriting.
4. Rebuild the prompt with the framework blocks in `references/framework.md`, grounded in the domain workflow when applicable.
5. Keep the result proportional: do not over-specify a simple task.
6. Return both the improved prompt and a short explanation of what changed when useful.

## Transformation Rules

- Preserve the user's objective, constraints, and tone unless they conflict.
- Prefer adding missing structure over rewriting everything stylistically.
- For prompts targeting packaged Pulse domain workflows, derive structure from that domain skill's workflow contract and references instead of inventing a parallel framework.
- Add context requirements only when they improve correctness.
- Add tool rules only when tool use materially affects correctness.
- Add verification and completion criteria for non-trivial tasks.
- Keep prompts compact enough to be practical in repeated use.

## Framework Blocks

Use these blocks selectively.

- `Objective`: state the task and what success looks like.
- `Context`: list sources, files, constraints, and unknowns.
- `Work Style`: set depth, breadth, care, and first-principles expectations.
- `Tool Rules`: state when tools, browsing, or file inspection are required.
- `Output Contract`: define structure, formatting, and level of detail.
- `Verification`: require checks for correctness, edge cases, and better alternatives.
- `Done Criteria`: define when the agent should stop.

## Output Modes

Choose one mode based on the user request.

- `Inline upgrade`: provide the upgraded prompt only.
- `Upgrade + rationale`: provide the prompt plus a brief list of improvements.
- `Template extraction`: convert the prompt into a reusable fill-in-the-blank template.
- `Hook spec`: explain how to apply the framework automatically before execution.

## Hook Pattern

When the user asks for a hook, model it as a pre-processing layer:

1. Accept the current prompt.
2. Classify the task and risk level.
3. Expand the prompt using the framework blocks.
4. Return the upgraded prompt for execution.
5. Optionally keep a diff or summary of injected structure.

Use `scripts/augment_prompt.py` when a deterministic first-pass rewrite is helpful.

## Quality Bar

Before finalizing, check the upgraded prompt:

- still matches the original intent
- does not add unnecessary ceremony
- includes the right verification level for the task
- gives the agent a clear definition of done

If the prompt is already strong, say so and make only minimal edits.

## Quick Reference: Deficiency → Framework Block

| Prompt Deficiency | Recommended Block |
|-------------------|-------------------|
| No success criteria | Objective + Done Criteria |
| Vague scope | Objective + Context |
| Missing context | Context |
| No error handling guidance | Verification |
| Unclear output format | Output Contract |
| No tool usage rules | Tool Rules |

## Integration with Pulse

Owns prompt improvement and templating across Pulse skills.

When the prompt is for a packaged domain workflow (for example bootstrap, docs refresh, exploring, planning, validating, swarming, executing, reviewing, compounding), reuse that domain skill's workflow rules and templates as the source of truth.

Use this skill first for prompt upgrades, then route execution to the target domain skill.
