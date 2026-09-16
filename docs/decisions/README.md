# Decisions

Decision records explain why important product, architecture, validation, source-of-truth, or workflow choices were made.

Current decisions:

- [0001 — Lowercase plan artifact, mandatory docs impact, and workgraph materialization](0001-lowercase-plan-docs-impact-workgraph.md)
- [0002 — Rust workgraph storage boundaries](0002-rust-workgraph-storage-boundaries.md)
- [0003 — Pre-release contract baselines](0003-pre-release-contract-baselines.md)
- [0004 — CLI-mediated Agent context and workflow bootstrap](0004-cli-mediated-agent-context.md)
- [0005 — Rust daemon runtime control plane](0005-rust-daemon-runtime-control-plane.md) — **superseded by 0008**; Pulse has no daemon
- [0006 — Peer Worker, Reviewer, and QA task topology](0006-peer-agent-assurance-topology.md)
- [0007 — Remove the legacy agent skill surface](0007-remove-legacy-agent-skill-surface.md)
- [0008 — Narrow Pulse to a vendor-neutral truth layer](0008-narrow-scope-to-truth-layer.md)
- [0009 — Workflow trong repo đích, skill theo artifact trên CLI](0009-skill-surface-over-cli.md) — **superseded in scope by 0022**; lessons retained
- [0010 — QA baseline là markdown heading, JSON chỉ ở biên runner](0010-qa-baseline-markdown-contract.md) — **superseded in scope by 0022**; lessons retained
- [0011 — Event log là JSONL theo ngày](0011-events-jsonl-per-day.md)
- [0012 — Thang bằng chứng, reviewer là bằng chứng, lane độc lập và lead hoà giải](0012-evidence-ladder-independent-lanes.md) — **superseded in scope by 0022**; lessons retained
- [0013 — Bàn giao phiên theo ngưỡng của host](0013-session-handoff-host-threshold.md) — **superseded in scope by 0022**; lessons retained
- [0014 — Pulse ghi `qa_checkpoint`, runner chỉ in output](0014-pulse-records-qa-checkpoint.md) — **superseded in scope by 0022**; lessons retained
- [0015 — Worktree dispatch: workspace trong worktree, state về repo chính](0015-worktree-dispatch-workspace-state.md) — **superseded in scope by 0022**; lessons retained
- [0016 — Worker sở hữu docs receipt, gate chặn tại handoff](0016-docs-receipt-ownership-at-handoff.md) — **superseded in scope by 0022**; lessons retained
- [0017 — Receipt không đọc được phải được báo, không được xoá khỏi danh sách](0017-unreadable-receipts-are-reported-not-erased.md) — **superseded in scope by 0022**; lessons retained
- [0018 — Knowledge plane: đường ra của vòng đời và retrieval có corpus riêng](0018-knowledge-plane-retrieval-and-lifecycle-exits.md) — **superseded in scope by 0022**; lessons retained
- [0019 — Ba tầng hướng dẫn, và đúng một skill sở hữu việc tạo node](0019-guidance-layers-and-single-node-owner.md) — **guidance layers narrowed by 0020**; **skill order amended by 0021**; **superseded in scope by 0022**; lessons retained
- [0020 — Gộp workflow thường ngày vào khối AGENTS](0020-collapse-guidance-into-agents.md) — **superseded in scope by 0022**; lessons retained
- [0021 — Prose trước graph, và planning vào một lần](0021-prose-before-graph-and-one-planning-entry.md) — **superseded in scope by 0022**; lessons retained
- [0022 — Pulse v3: giữ hợp đồng, bỏ máy móc](0022-thin-harness.md) — **§13/§14 amended by 0023**
- [0023 — `pulse serve` multi-project thay board tĩnh](0023-board-serve-multi-project.md)

Add or update a decision when:

- a locked technical or architecture choice changes
- a product rule changes meaningfully
- a validation requirement is added, removed, or weakened
- a high-risk feature chooses one design over another
- the source-of-truth hierarchy changes
- workflow gates or canonical artifact ownership change

Use numbered filenames such as `0001-decision-title.md`.
