# Gray Area Probes by Domain Type

Use these probes in Phase 2 of `/pulse explore` to generate decision-grade gray areas.
Select only 2–4 probes per active domain type.

Do not use all probes; pick the ones that are genuinely undecided for the active story.

---

## SEE — Something users look at
*(UI, dashboards, visualizations, layouts, forms)*

**Layout & Density**
- What is the primary layout container — list, card grid, table, timeline, or canvas?
- How dense should this be? Information-rich (power user) or spacious (casual user)?
- What happens at mobile/small viewports — same layout, stacked, hidden, or a different view?
- Is there a fixed header/footer/sidebar, or does everything scroll?

**Visual States**
- What does the empty state look like when there is no data yet?
- What does the loading state look like — skeleton, spinner, or optimistic render?
- How are errors surfaced — inline, toast, banner, or modal?
- Are there hover, focus, or selection states to define?

**Interactions**
- Is this read-only, or can users interact (click, drag, edit inline)?
- If interactive: are changes immediate (optimistic) or do they require explicit save?
- Are there destructive actions? What is the confirmation pattern?
- What triggers navigation — explicit button, row click, or both?

**Content Presentation**
- How much text is shown before truncation? Is there expand/collapse?
- How are long lists paginated — page numbers, load-more, or infinite scroll?
- Are images, avatars, or icons included? What is the fallback?
- How are sorting and filtering exposed — dropdowns, tabs, or search?

---

## CALL — Something callers invoke
*(REST APIs, GraphQL, CLIs, webhooks, SDKs, internal service interfaces)*

**Interface Contract**
- What is the primary input shape — URL params, request body, flags, or event payload?
- What does a successful response contain — the created/updated resource, an ID, or just a status?
- Is this synchronous, or does it start an async job?
- What is the versioning strategy — path version, header, or query param?

**Authentication & Authorization**
- Who is the expected caller — internal service, authenticated user, or anonymous client?
- What authentication mechanism is expected — API key, JWT, OAuth token, or session cookie?
- Are there permission tiers? Can some callers do more than others?

**Error Handling**
- What status/error codes map to which failure modes?
- What should error payloads include — code/message only or structured details?
- How should callers handle throttling — 429 + Retry-After, or a different signal?
- Are there partial-success responses?

**Behavior Modes**
- Is idempotency required?
- What timeout/retry expectations should callers follow?
- Does this operation have side effects and should they ever be suppressible?
- Is a dry-run or preview mode required?

---

## RUN — Something that executes
*(Background jobs, cron tasks, scripts, CLI tools, services, pipelines)*

**Invocation**
- How is this triggered — schedule, event/message, explicit command, or webhook?
- Is it single-instance or can multiple instances run in parallel?
- Expected runtime duration — seconds, minutes, or hours?

**Output & Reporting**
- Where does output go — stdout, log file, database, or notification?
- What verbosity levels are needed — silent, normal, verbose?
- How does progress report for long-running work — percentage, count, or none?
- What must the final summary contain?

**Error Recovery**
- What happens on partial failure — abort everything, continue with failures logged, or retry?
- Are failures retryable? What backoff/retry policy applies?
- Who gets notified on failure?
- Is there a dead-letter queue or error archive?

**Behavior Modes**
- Is there a dry-run mode with no side effects?
- Is there a force/override mode for normally-blocked cases?
- What concurrency model applies — one-at-a-time, bounded pool, or unlimited?
- Are there resource limits (CPU/memory/API quotas) to respect?

---

## READ — Something users read
*(Documentation, emails, reports, notifications, changelogs, READMEs)*

**Structure & Navigation**
- What top-level structure is required — step-by-step, reference table, narrative, or mixed?
- Should this be one page, multi-page, or collapsible sections?
- Is a table of contents needed?
- How should related docs be linked?

**Tone & Depth**
- Primary audience — beginner, intermediate, or expert?
- Tone — formal technical, conversational, or neutral reference?
- How much background context should be included?
- Are examples/snippets required, and in what format?

**Content Shape**
- Which sections are mandatory?
- Are warnings/notes/callouts required?
- Is content versioned by release?
- What update cadence is expected?

---

## ORGANIZE — Something being structured
*(Data models, file layouts, taxonomies, naming conventions, config schemas)*

**Grouping Criteria**
- What is the primary grouping dimension — type, domain, date, owner, status?
- Are there sub-groupings? How deep?
- What determines group membership — hard rule, convention, or manual tagging?

**Naming Conventions**
- Required naming pattern — camelCase, snake_case, kebab-case, PascalCase?
- Required prefixes/suffixes?
- How are name conflicts resolved?
- Are names stable IDs or freely renamable labels?

**Edge Cases & Exceptions**
- How are items that do not fit the primary grouping handled?
- What happens to orphaned items?
- Can one item belong to multiple groups?
- How is order in a group determined?

**Migration & Evolution**
- Does existing data need restructuring?
- Can structure evolve over time?
- How are new groups/categories introduced?

---

## Cross-Cutting Probes
*(Apply to any domain type when relevant)*

**Scope Boundary**
- What is explicitly out of scope?
- Which adjacent features are touched but not owned?

**Prior Decisions**
- Is there an established repo pattern to follow or intentionally diverge from?
- Were these choices already decided in a prior explore pass?

**Downstream Consumer**
- Who consumes this output — users, services, internal tools?
- Must output be machine-readable, human-readable, or both?
