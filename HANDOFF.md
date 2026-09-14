# Handoff: viết `pulse-grill`

## Task

Viết skill `skills/pulse-grill/` theo Decision 0019 và **Decision 0021**. Đây là
skill kế tiếp trong chuỗi, và là skill **đầu tiên chịu hợp đồng draft** mà 0021
vừa chốt — nó định nghĩa `works/_drafts/<slug>/story.md` mà `pulse-spec` và
`pulse-planning` sẽ đọc.

Không implement feature Pulse ngoài guard hoặc fixture tối thiểu cần để skill có
contract kiểm được. Không chạy Pulse mutation với `--repo-root .` tại repo này.

## Context: chuỗi đã đổi

Decision 0021 (accepted 2026-09-15) sửa thứ tự của 0019:

```text
wayfind → grill → spec → planning → run
```

`planning` vào **một lần** ở cuối, không còn hai lần ở hai tầm. Lý do: `grill`
và `spec` ghi prose vào `works/_drafts/<slug>/` khi chưa có node, nên hai mốc
tạo node (trước đây bị `spec` chen vào giữa) gộp được làm một.

Hai nguồn tham chiếu đều đặt hiểu trước hình dạng: Matt là
`wayfinder → grill-with-docs → to-spec → to-tickets`; Khuym (`references/skills`)
là `exploring → planning → validating`, với `exploring` chạy trước và
`CONTEXT.md` ở địa chỉ theo slug.

## Current repository state

- Repo: `/Users/quannv.dev/Workspace/Personal/pulse`
- Branch: `features/harness-experimental`
- HEAD: `8967a0c` — **mọi thứ dưới đây chưa commit**
- Modified: `HANDOFF.md`, `PRODUCT.md`, `assets/agents-block.md`,
  `docs/decisions/0019-*`, `docs/decisions/README.md`, `docs/plans/0019-*`,
  `tests/graph/architecture_guards.rs`, `tests/graph/cli_lifecycle_contract.rs`
- Untracked: `docs/decisions/0020-*`, `docs/decisions/0021-*`, `skills/`
- Không có dogfood target. Không chạy Pulse against repo root hoặc immutable
  fixture tại chỗ.

## Work completed

### Decision 0021 và các văn bản theo sau

- `docs/decisions/0021-prose-before-graph-and-one-planning-entry.md` — accepted.
- `PRODUCT.md`: `works/_drafts/<slug>/` vào bảng plane §4 và vào layout, kèm lý
  do nó cùng plane work prose; §5.8 bảng skill đổi thứ tự và mô tả một-lần-gọi.
- `assets/agents-block.md`: route R1–R3 thành
  `pulse-grill → pulse-spec → pulse-planning`. (Nó đang ghi `pulse-tickets`, tên
  0019 đã bỏ — con trỏ chết đã sửa luôn.)
- `docs/decisions/0019`: Status ghi amended by 0021, và đoạn "planning gọi hai
  lần" có block sửa ngay tại chỗ để thân bài không mâu thuẫn với status.
- `docs/plans/0019`: giai đoạn 2 đổi thứ tự — `grill` (2.2), `research` (2.3),
  `spec` (2.4), `planning` (2.5).

### `pulse-planning` (xong, chưa commit)

`skills/pulse-planning/` gồm `SKILL.md`, `references/graph-breakdown.md`,
`references/tracer-bullets.md`, `evals/evals.json` và ba fixture. Một lần gọi,
hai input (draft giao hàng; frontier đã confirm — cái sau là transcription),
bước nhận nuôi copy → `work sync` → `qa baseline` → xoá draft sau cùng.

Eval iteration 5: **với skill 97.5%, baseline 48.0%, delta +0.49** trên năm case
(thiếu draft, draft→graph, từ chối hẹp, clear-R0 near miss, frontier đã confirm).
Iteration 6 chạy lại riêng eval 2 sau khi chuyển kiểm `qa.md` từ bước 4 lên bước
1 — **đọc kết quả đó trước khi coi planning là xong**.

### `pulse-wayfind` (xong, chưa commit)

Bàn giao sang `grill` thay vì `planning`; frontier vẫn sang `planning` vì đó là
chủ sở hữu node.

### Test mới (`tests/graph/cli_lifecycle_contract.rs`)

- `a_pre_graph_prose_draft_is_not_a_node_and_does_not_fail_validation`
- `qa_baseline_resolves_only_after_the_draft_qa_is_adopted`

Cái thứ hai ép thứ tự nhận nuôi: cùng một `qa.md`, ở draft thì fail có code, ở
node path thì resolve ra case.

## Required behavior of `pulse-grill`

### Đầu ra

1. `works/_drafts/<slug>/story.md` — Outcome, success signals, scope boundary,
   `## Open questions` với disposition theo bảng ambiguity gate `PRODUCT.md`
   §5.1 (`resolved`/`rejected`/`delegated`/`deferred`/`blocking`).
2. Thuật ngữ đã chốt vào `docs/domain/glossary.md` (`DOC-GLOSSARY` đã được seed
   và đăng ký ở commit `8775c35`).
3. Decision node **chỉ khi** đủ ba điều kiện: khó đảo ngược, khó hiểu nếu thiếu
   context, có trade-off thật. Không đủ thì ghi `(resolved)` trong
   `## Open questions`.

### Primitive giữ từ Matt và Khuym

Một câu hỏi mỗi lượt, kèm câu trả lời gợi ý. Fact tự tra bằng
`pulse docs search`/`get` và code; chỉ hỏi người về intent, preference,
authority, trade-off. Không hành động tới khi người xác nhận. Khuym `exploring`
gán ID ổn định cho mỗi quyết định (`D1`, `D2`…) và liệt kê anti-pattern: bundled
questions, deep implementation analysis, architecture proposals, tạo node, code.

### Hai ràng buộc cứng

1. **Không tạo node.** 0009 dòng 237 nói grill kết thúc bằng
   `pulse work create --kind story` — **0019 đã bãi**. Guard
   `only_planning_skill_can_name_node_creation_commands` sẽ làm test đỏ nếu
   `skills/pulse-grill/` chứa `pulse work create` hoặc `pulse graph edge add`.
   Grill ghi prose vào vùng draft; Story do `planning` tạo sau.
2. **Gate `shaped` hiện không kiểm gì với Story.** Profile `shaped` chỉ chạy một
   family `ticket_ambiguity` (`src/graph/read/readiness.rs:298`), và family đó
   mở đầu bằng `if role != Some(TicketRole::Implementation) { NotApplicable }`
   (dòng 451) — Story không có role. Nên kỷ luật shaping ở tầm Story **chỉ sống
   trong prose của grill**, khác `planning` vốn trỏ được vào ready gate. Cân
   nhắc ADR thêm family `story_ambiguity`; chưa quyết.

## Relevant files

- `docs/decisions/0021-*` — hợp đồng draft, thứ tự chuỗi, ba điểm để ngỏ.
- `docs/decisions/0019-*` — single-node-owner, khuôn skill, invocation policy.
- `docs/decisions/0009-*` §`pulse-grill` (dòng 226–239) — nguồn hành vi; đọc qua
  lăng kính 0019/0021, **không copy bước tạo Story**.
- `PRODUCT.md` §5.1 ambiguity gate; §4 layout; §5.8 bảng skill.
- `references/mattpocock/skills/skills/engineering/grill-with-docs/SKILL.md`
- `references/skills/plugins/khuym/skills/exploring/SKILL.md` — Socratic
  locking, `CONTEXT.md` theo slug, anti-pattern.
- `skills/pulse-planning/SKILL.md` — downstream consumer; hợp đồng draft phải
  khớp bước 1 và bước 4 của nó.
- `skills/pulse-wayfind/SKILL.md` — upstream, bàn giao sang grill.
- `src/graph/read/readiness.rs` — gate `shaped` thật sự kiểm gì.
- `src/qa/baseline.rs` — posture hợp lệ **không có** `required`.
- `/Users/quannv.dev/.pi/agent/skills/skill-creator/SKILL.md` — bắt buộc đọc.

## Validation

```bash
cargo fmt --check
cargo clippy --all-targets --quiet -- -D warnings
cargo test --all-targets          # 623 passed lần chạy gần nhất
cargo test --test graph -- architecture_guards
python3 -m scripts.quick_validate skills/pulse-grill
```

Guard chủ sở hữu node đã kiểm bằng cách phá: tạo skill vi phạm → đỏ với
`only pulse-planning may own graph shape` → xoá probe.

## Bài học từ việc dựng eval

- Fixture để trong cây skill thì baseline agent đọc được `SKILL.md` → baseline
  ảo cao. Copy fixture ra ngoài, cấm đọc `skills/` **và** cấm `semble`.
- Runner phải bị cấm spawn subagent: một fork đã ghi đè output của agent chính.
- Aggregate script cần layout `eval-*/<config>/run-*/grading.json`; viewer chỉ
  cần `<config>/outputs/`. Ghi grading.json cả hai chỗ.
- `python` không tồn tại; dùng `python3`.
- Assertion gộp hai sự thật vào một câu sẽ lật qua lật lại giữa các lần chạy.
- Fixture phải được kiểm bằng CLI thật: bản `qa.md` đầu tiên dùng
  `Posture: required`, không hợp lệ, và dạy sai cả skill lẫn eval.

## Next action

1. Đọc kết quả iteration 6 (eval 2) trước khi chốt `pulse-planning`.
2. Đọc 0021, 0019, `grill-with-docs`, `exploring`, skill-creator.
3. Viết `skills/pulse-grill/` — draft, guard, quick validate, rồi eval.
4. Sau grill: `research` → `spec` → `onboard` → `handoff` → `ratchet`
   (`ratchet` bị chặn bởi 0018 G1 vì guard parse từ chối
   `pulse knowledge retire` khi subcommand chưa tồn tại).
5. Commit chỉ khi user yêu cầu.
