# Decisions

Decision records explain why important product, architecture, validation, source-of-truth, or workflow choices were made.

Current decisions:

- [0001 — Lowercase plan artifact, mandatory docs impact, and workgraph materialization](0001-lowercase-plan-docs-impact-workgraph.md)
- [0002 — Rust workgraph storage boundaries](0002-rust-workgraph-storage-boundaries.md)
- [0003 — Pre-release contract baselines](0003-pre-release-contract-baselines.md)
- [0004 — CLI-mediated Agent context and workflow bootstrap](0004-cli-mediated-agent-context.md)
- [0005 — Rust daemon runtime control plane](0005-rust-daemon-runtime-control-plane.md)
- [0006 — Peer Worker, Reviewer, and QA task topology](0006-peer-agent-assurance-topology.md)

Add or update a decision when:

- a locked technical or architecture choice changes
- a product rule changes meaningfully
- a validation requirement is added, removed, or weakened
- a high-risk feature chooses one design over another
- the source-of-truth hierarchy changes
- workflow gates or canonical artifact ownership change

Use numbered filenames such as `0001-decision-title.md`.
