# Fixture map

- `app/` owns the deterministic browser surface.
- `scripts/qa-environment.mjs` owns build/deployment lifecycle.
- `scripts/qa-playwright.mjs` owns the typed Playwright execution wrapper.
- `playwright.config.mjs` and `package.json` pin the browser toolchain contract.
