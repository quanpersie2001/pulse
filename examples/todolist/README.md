# todolist

Dogfood target repository for Pulse (see `PRODUCT.md` §7 in the parent repo).
A tiny todo-list CLI with zero dependencies: the domain logic is a pure
module in `src/todolist.mjs`, wrapped by a stateless CLI in `src/cli.mjs`.

```bash
node src/cli.mjs add t1 "Write handoff summary"
node src/cli.mjs list
node scripts/verify.mjs
```
