# Playwright Service Fixture

This target-repository fixture proves a repository-owned Playwright wrapper on
a real browser binary. The lifecycle builds the rendered page from the current
Git commit, starts one local deployment, healthchecks and resets that exact
identity, then cleans it up. The QA wrapper reads Pulse's typed input, launches
Chromium, records deterministic DOM/source/build/deployment assertions and
writes a real Playwright trace ZIP.

The fixture intentionally contains no dependencies, browser cache, generated
build or `.pulse` state. Acceptance installs and executes only in the external
`TestRepo` copy.
