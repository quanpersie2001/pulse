# DEEPENING

Deepening is turning shallow, scattered behavior into concentrated modules with smaller, clearer interfaces.

## Signals a module should be deepened

- Callers repeat policy checks or sequencing rules before calling.
- Many call sites handle the same edge/error conditions.
- One change requires touching many files to keep behavior aligned.
- Helper layers mostly forward calls and rename parameters.
- Ownership of one behavior is split across unrelated modules.

## Common rescue moves

1. **Concentrate policy**
   - Pull repeated branching/validation from callers into one module.
2. **Collapse pass-through layers**
   - Remove wrappers that add no leverage.
3. **Create a true seam**
   - Introduce adapter-backed seam where variation already exists.
4. **Re-center ownership**
   - Move one responsibility to one primary module owner.
5. **Shrink caller knowledge**
   - Replace ordered call choreography with one high-level operation.

## Guardrails

- Do not deepen by adding abstraction without observed variation.
- Keep interface changes proportional to the rescue value.
- Prefer one bounded rescue step over a sweeping rewrite.
- Avoid blending architecture rescue with unrelated style cleanups.

## Report framing

For each candidate, describe:

- Current friction
- Deepening move
- Expected leverage/locality gain
- Cost/risk and migration constraints
