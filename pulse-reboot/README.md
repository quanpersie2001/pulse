# Pulse Reboot: Bản đồ tài liệu

[Quay lại trang vào](../PULSE_REBOOT.md)

## Progressive disclosure

Tài liệu được chia thành ba tầng:

- **L0 - Direction:** [`PULSE_REBOOT.md`](../PULSE_REBOOT.md) trả lời Pulse là gì, không là gì và quyết định chính.
- **L1 - Navigation:** file này cho biết nên đọc gì và tài liệu nào sở hữu sự thật nào.
- **L2 - Design:** các file chủ đề chứa contract, schema, lifecycle, failure mode và acceptance scenario.

Nguyên tắc: L0 chỉ tóm tắt và liên kết. Mỗi quy tắc normative chỉ có một tài liệu L2 sở hữu; tài liệu khác tham chiếu thay vì sao chép.

## Đường đọc theo nhu cầu

### Tôi muốn hiểu direction

1. [`PULSE_REBOOT.md`](../PULSE_REBOOT.md)
2. [`01-foundations.md`](01-foundations.md)
3. [`09-decisions-and-dod.md`](09-decisions-and-dod.md)

### Tôi muốn hiểu quản lý công việc local-first

1. [`02-work-graph.md`](02-work-graph.md)
2. [`06-priority-reconciliation.md`](06-priority-reconciliation.md)
3. [`03-story-qa.md`](03-story-qa.md)

### Tôi muốn implement Pulse Core

1. [`02-work-graph.md`](02-work-graph.md)
2. [`10-documentation-system.md`](10-documentation-system.md)
3. [`11-documentation-retrieval.md`](11-documentation-retrieval.md)
4. [`12-knowledge-compounding.md`](12-knowledge-compounding.md)
5. [`07-verification-ratchet.md`](07-verification-ratchet.md)

### Tôi muốn implement Pulse Daemon

1. [`04-runtime-harness.md`](04-runtime-harness.md)
2. [`05-cross-agent-coordination.md`](05-cross-agent-coordination.md)
3. [`08-implementation-roadmap.md`](08-implementation-roadmap.md)
4. [`../proposals/phase2-rust-daemon-realignment-implementation-gap.md`](../proposals/phase2-rust-daemon-realignment-implementation-gap.md)

### Tôi muốn hiểu self-improvement và memory recall

1. [`12-knowledge-compounding.md`](12-knowledge-compounding.md)
2. [`07-verification-ratchet.md`](07-verification-ratchet.md)
3. [`10-documentation-system.md`](10-documentation-system.md)
4. [`11-documentation-retrieval.md`](11-documentation-retrieval.md)

### Tôi muốn hiểu documentation management

1. [`10-documentation-system.md`](10-documentation-system.md)
2. [`11-documentation-retrieval.md`](11-documentation-retrieval.md)
3. [`02-work-graph.md`](02-work-graph.md)
4. [`07-verification-ratchet.md`](07-verification-ratchet.md)

### Tôi muốn hiểu multi-agent

1. [`05-cross-agent-coordination.md`](05-cross-agent-coordination.md)
2. [`02-work-graph.md`](02-work-graph.md)
3. [`06-priority-reconciliation.md`](06-priority-reconciliation.md)

## Ownership map

| Tài liệu | Sở hữu |
|---|---|
| `01-foundations.md` | Product thesis, goals, non-goals, lessons từ OpenAI và references |
| `02-work-graph.md` | Work item model, storage, artifacts, lifecycle, dependency và external sync |
| `03-story-qa.md` | Developer verification boundary, Story test cases, Ticket checkpoint, surface-adaptive executors, receipts và close gate |
| `04-runtime-harness.md` | Core/Daemon boundary, Project/Workspace/Session/Provider lifecycle, process ownership, timeline, protocol và repository harness |
| `05-cross-agent-coordination.md` | Roles, ownership/communication graphs, assignment, mailbox, handoff, deliberation, parallel dispatch, authority và recovery |
| `06-priority-reconciliation.md` | Semantic priority, decision/execution frontier scheduling, supersession và reconcile decisions |
| `07-verification-ratchet.md` | Verification, review, doctor, eval, failure classification và harness backlog |
| `08-implementation-roadmap.md` | Technology, brownfield/product migration boundaries, phases, acceptance scenarios và risks |
| `09-decisions-and-dod.md` | Decision register và Definition of Done theo milestone |
| `10-documentation-system.md` | Documentation taxonomy, source hierarchy, ownership, context routing, lifecycle, validation, promotion và drift |
| `11-documentation-retrieval.md` | Generated indexes, section extraction, lexical search/get, cache, retrieval eval và optional semantic adapter |
| `12-knowledge-compounding.md` | Learning capture/schema/lifecycle, promotion, applicability-aware recall, prompt routing, feedback và retirement |

## Thứ tự nguồn sự thật

Khi có mâu thuẫn:

1. Code và test đang chạy cho biết **hiện trạng**.
2. Decision đã accepted cho biết **direction đã khóa**.
3. Tài liệu chủ đề sở hữu **thiết kế mục tiêu**.
4. Root document chỉ là summary.
5. Reference projects cung cấp bài học, không tự động trở thành contract của Pulse.

Trong target repository, accepted Decision/product contract diễn tả intent; code/tests diễn tả implementation và receipts diễn tả observation. Mâu thuẫn giữa chúng phải được reconcile, không áp dụng heuristic “code luôn thắng”; xem [`10-documentation-system.md`](10-documentation-system.md).

## Reference set

- [OpenAI Harness engineering](https://openai.com/index/harness-engineering/)
- [OpenAI Symphony announcement](https://openai.com/index/open-source-codex-orchestration-symphony/)
- [OpenAI Symphony repository](https://github.com/openai/symphony)
- [QMD repository](https://github.com/tobi/qmd) — hybrid docs retrieval reference, không phải Core dependency
- [MiniSearch repository](https://github.com/lucaong/minisearch) — pure-JavaScript BM25+ reference lesson (không còn là direction sau khi chốt Rust)
- [Tantivy repository](https://github.com/quickwit-oss/tantivy) — pure-Rust BM25+ full-text engine, implementation direction cho Pulse core
- [comrak repository](https://github.com/kivikakk/comrak) — Rust GFM Markdown parser với source positions, direction cho section extraction
- [Knowledge Base Builder](https://github.com/shivdeepak/knowledge-base-builder) — generated/indexed progressive-disclosure reference
- [`references/harness-experimental`](../references/harness-experimental)
- [`references/maestro`](../references/maestro)
- [`references/paseo`](../references/paseo) — primary runtime-daemon reference
  cho Project/Workspace/Session/Provider managers, lifecycle, timeline sync và
  transport-neutral tool catalog; Pulse implement shape này bằng Rust và giữ
  work/proof semantics trong Core
- [`references/mattpocock/skills`](../references/mattpocock/skills) — `grilling` reference cho one-question-at-a-time decision pressure-test; `wayfinder` reference cho destination, decision frontier, fog-of-war và progressive reconciliation. Pulse giữ local work graph/owner semantics thay vì copy skill chain, tracker canonicality hoặc artifact layout

## Quy tắc cập nhật

- Thay đổi một contract tại file sở hữu nó.
- Nếu direction thay đổi, cập nhật thêm summary ở root và decision register.
- Không biến `README.md` thành một bản thiết kế thứ hai.
- Ví dụ schema phải ghi rõ illustrative hay normative.
- Mọi phase phải trỏ về acceptance scenario, không chỉ liệt kê đầu việc.
- Thuật ngữ `Agent`, `Orchestration Agent`, `Worker Agent`, `Ticket`, `Story` phải giữ đúng nghĩa đã định nghĩa.
- Quy tắc documentation normative phải thuộc `10-documentation-system.md`; docs retrieval/index/search normative thuộc `11-documentation-retrieval.md`; learning/compound/applicable-recall normative thuộc `12-knowledge-compounding.md`; các file khác chỉ tích hợp chúng vào contract mà mình sở hữu.
