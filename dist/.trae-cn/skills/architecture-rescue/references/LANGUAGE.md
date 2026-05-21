# LANGUAGE

Use this vocabulary consistently in architecture-rescue reports.

- **Module**: any unit with an interface and implementation (function, class, package, slice).
- **Interface**: everything callers must know (types, invariants, ordering, failure modes, config).
- **Implementation**: internal behavior hidden behind the interface.
- **Depth**: how much useful behavior a module hides behind a small interface.
- **Shallow module**: interface complexity is close to implementation complexity.
- **Deep module**: high leverage to callers from a relatively small interface.
- **Seam**: where behavior can be substituted or redirected without in-place edits.
- **Adapter**: concrete implementation attached at a seam.
- **Leverage**: caller-side benefit from depth and stability.
- **Locality**: maintainer-side benefit when changes stay concentrated.
- **Ownership drift**: one responsibility spread across multiple owners/modules.

## Principles

1. **The interface is the test surface.** If tests require implementation knowledge, the seam is weak.
2. **Deletion test.** Imagine deleting a module:
   - If complexity disappears, it was likely pass-through.
   - If complexity reappears across many callers, it likely provided leverage.
3. **Two adapters make a seam real.** One adapter can be speculative; two indicate true variation pressure.
4. **Name domain concepts, not mechanism trivia.** Prefer stable problem-domain names.
