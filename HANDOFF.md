# Handoff: Pulse — vào đường ống friction (0009 phần C)

## Trạng thái bàn giao

- Repo: `/Users/quannv.dev/Workspace/Personal/pulse`
- Nhánh: `features/harness-experimental` (chưa push)
- HEAD: `61766e4` chore: remove the Track B dogfood target, keep its evidence
- Tag: `v0.1.0` ở `16a0ef3`; **`dogfood/track-b-final` ở `7fb1dd7`** — toàn bộ
  dogfood target trước khi gỡ.
- Working tree: **sạch** (trừ file này).
- Ba gate xanh tại HEAD: `cargo fmt --check`, `cargo clippy --all-targets
  --quiet -- -D warnings`, `cargo test --all-targets` — **589 test**, default
  threading.

Sáu commit của phiên trước:

```
ce97810 feat(qa): parse qa.md headings and hash cases (0010)
578feaf dogfood(0010): heading baselines and a case-agnostic pulse-check runner
76ac229 docs: record how 0010 landed and correct the status rows
f770587 fix(graph): fold wrapped bullets in the ticket.md contract
7fb1dd7 fix(evidence): report unreadable receipts instead of erasing the proof list
61766e4 chore: remove the Track B dogfood target, keep its evidence
```

---

# VIỆC CỦA PHIÊN NÀY

## Làm gì: đường ống friction — Decision 0009 phần C

Bốn thay đổi. Nửa **tiêu thụ** của vòng harness learning đã chạy sẵn; chỉ
thiếu nửa **sản xuất**.

### 1. `pulse note --kind friction`

- `src/cli/args.rs`: thêm `--kind` vào `Command::Note` (mặc định `note`,
  giá trị hợp lệ tối thiểu `note | friction`).
- `src/cli/mod.rs:56` truyền xuống `events::handle_note`.
- `src/cli/events.rs:12` `handle_note(store, work, message, from, json)` —
  thêm tham số kind.
- `src/kernel/communication.rs:26` `record_note(work_id, message, actor)` —
  thêm kind, ghi vào payload event `note.recorded`
  (`src/kernel/communication.rs:62`). `NoteRecorded` (dòng 84) thêm trường.
- `list_notes_for_ticket` (dòng 97) lọc theo `event_type == "note.recorded"`
  — cân nhắc cho packet phân biệt note thường với friction.

### 2. `work handoff --friction "<text>"`

- `src/cli/work.rs:271` `WorkCommand::Handoff` — thêm `#[arg(long = "friction")]
  frictions: Vec<String>` cạnh `--check` / `--evidence-receipt`.
- Dispatch ở `src/cli/work.rs:719`.
- Ghi thành note kind `friction` gắn Ticket, hoặc vào `HandoffReceipt`; chọn
  một và ghi lý do. **Cẩn thận:** đừng gọi gì lấy repository write lock trong
  lúc handoff đang giữ fence — đó là bug `e27b5e4` (`verify_receipt` load docs
  registry → deadlock). `load_receipt` chỉ đọc file, an toàn.

### 3. Close gate: friction note → learning `candidate` scope `harness`

- `src/kernel/completion.rs` — sau khi close thành công, gom note kind
  `friction` của Ticket thành `candidate`.
- Store learning: `src/knowledge/store.rs`. Scope đã có:
  `LearningScope::Harness` (`src/knowledge/model.rs:203`).
- 0009 §4: ratchet được sửa harness ngay khi có candidate, nhưng learning chỉ
  lên `validated` khi Ticket sau ghi `knowledge_usage: helpful`. Phiên này
  **chỉ làm tới candidate**, đừng làm luôn phần validated.

### 4. Test hồi quy

Crate `communication` (`tests/communication.rs`) cho note kind; crate `graph`
cho close gate sinh candidate. Ít nhất: note friction ghi đúng kind vào event;
close sinh đúng một candidate scope harness cho mỗi friction note; Ticket
không có friction note thì không sinh gì.

## Vì sao là việc này

- **Bằng chứng mạnh nhất trong toàn bộ friction log:** cả một file phải duy
  trì bằng tay *vì* cơ chế này không tồn tại (`docs/dogfood-friction-track-b.md`).
- Vị trí file viết tay đó — `works/` của target, tức source plane — nằm trong
  dirty fence và **đã từng chặn một `close`** với `close_source_stale`.
  `.pulse/events/` được loại khỏi fence (`src/source.rs::is_pulse_metadata_path`),
  nên cơ chế đúng sẽ không bao giờ gây ra chuyện đó.
- Nửa tiêu thụ đã chạy thật: `LearningScope::Harness` được `run.rs:2071` render
  thành `## Harness learnings` trong prompt worker, và `packet.rs:554` lọc nó
  khỏi injection theo path.
- `0009 §Thứ tự` cũng xếp phần này trước.

## Sau đó (không làm trong phiên này)

1. **Phần A — khối `AGENTS.md` + `PULSE.md`** (`0009` Quyết định 1). Hiện
   **chưa có gì**: không `PULSE:BEGIN` ở đâu trong `src/`, không
   `assets/agents-block.md`. Đây là điều kiện cần để nối lại dogfood.
2. **Dựng dogfood target mới** rồi chạy thật bằng agent tương tác.
3. **Phần B — bảy skill** (`0009` Quyết định 2): quyết phạm vi **sau khi** có
   dữ liệu từ bước 2. Cần gỡ guard `legacy_skill_surfaces_are_absent`
   (`tests/graph/architecture_guards.rs:104`) đang cấm `skills`, `dist`,
   `.codex-plugin`, `.claude-plugin`.

---

# BỐI CẢNH CẦN BIẾT

## Đính chính quan trọng về 0009

Handoff các phiên trước lặp lại: *"toàn bộ friction là ergonomics CLI, không
mục nào là 'agent không biết làm gì tiếp' ⇒ 0009 có thể thừa"*. **Sai.**

`0009 §Context`: *"Từ intent đến Ticket `ready` không có gì dẫn agent;
developer làm tay."* Track B chỉ chạy TK-003..TK-008 — toàn Ticket đã shaped
sẵn bằng tay. Nhật ký friction chỉ phủ giai đoạn **sau `ready`**. Giai đoạn
trước `ready` — đúng phần bảy skill phụ trách — chưa bao giờ giao cho agent,
nên không thể sinh mục friction nào.

Nhật ký **im lặng** về nửa đó, không phải **phản đối** nó. Đừng thu hẹp 0009
bằng lập luận đó nữa. Thu hẹp bằng **thứ tự**: làm C → A → dựng target → chạy
thật, rồi mới quyết B bằng dữ liệu.

## Trạng thái 0009 trong code

| Phần | Có gì |
|---|---|
| Khối `AGENTS.md` + `PULSE.md` | Chưa gì cả |
| `note --kind friction` + close gate → candidate | Chưa gì cả (`friction` không xuất hiện một lần trong `src/`) |
| Bảy skill | Chưa; guard còn cấm `skills/` |
| Đích harness learning | **Đã chạy** — scope, prompt injection, packet filtering |

## Hiểu đúng "friction" (đã trace từ code, đừng suy lại)

`friction` là cơ chế **của repo đích, không phải của Pulse**: `note --kind
friction` → `.pulse/events/` của repo đó → close gate → learning scope
`harness` → promote vào `AGENTS.md`/`PULSE.md`/`runners.json` **của repo đó**.
Không mắt xích nào chảy ngược về Pulse. `PRODUCT.md` không có khái niệm báo bug
lên upstream.

Hệ quả: `docs/dogfood-friction-track-b.md` **không phải** friction theo nghĩa
đó. Phần lớn nội dung là defect của Pulse core (sửa bằng code), thiểu số mới là
harness friction thật. Header file đã ghi rõ điều này.

## Trạng thái §7 golden path

| Tiêu chí | Thực tế |
|---|---|
| Mục 1–7 | Đạt 2026-09-05, HEAD `845ff01` |
| `close-story` trên baseline thật | **Đạt** — ST-001 (09-05), ST-002 (09-06) |
| Hai Ticket song song không va nhau | **Chưa** — TK-006/TK-007 đã va; 0015 sửa, có test, chưa chạy lại thật |

Không còn dogfood target ⇒ mục cuối chưa kiểm chứng được ⇒ cổng "chưa đạt thì
không thêm feature" hiện chưa thoả. **Không cần ADR treo nó**: C + A + target
mới chính là đường thoả mãn.

## Hàng đợi quyết định còn mở

| | Quyết định | Trạng thái |
|---|---|---|
| 0009 | Phạm vi phần B (bảy skill) | Chờ dữ liệu từ dogfood mới |
| mới | Repo downstream phát hiện bug Pulse thì ghi vào đâu | Thiết kế không có cửa nào |
| treo | `unreadable` trong `PRODUCT.md` §5.3 | Sửa contract mà chưa có ADR — viết ADR ngắn hay revert hunk |
| 0011 | Event log `.jsonl` + compact | Accepted, chưa động dòng nào |
| 0004 | Packet lease-bound | Nên đánh "Superseded in part by 0008" |

## Bẫy đã học, đừng dẫm lại

- **Lock:** `verify_receipt` lấy repository write lock (nó load docs registry).
  Không gọi khi đang giữ fence — bug `e27b5e4`. `load_receipt` an toàn.
- **`list_receipts`** giờ trả `unreadable[]` thay vì fail cả listing. Đừng bọc
  `unwrap_or_default()` quanh nó ở callsite mới — đó chính là bug vừa sửa ở
  `7fb1dd7`.
- **Continuation `\` trong string literal Rust** nuốt cả indent dòng sau. Test
  data cho parser thụt lề phải dùng raw string.
- **`cargo test --all-targets` sau `cargo clippy --all-targets`** phải build
  lại từ đầu (khác profile): tính **8–14 phút**, đừng đặt timeout 120s.
- **`.gitignore` neo ở gốc:** `.pulse/runtime/` **không** khớp thư mục con.
  `AGENTS.md` từng khai sai điều này; đã đính chính ở `PRODUCT.md` §13.1.
- **`AGENTS.md` liệt kê 8 test crate, thực tế 10** — thiếu `tests/runner.rs` và
  `tests/communication.rs`. Chưa sửa.
- **Bất thường chưa giải thích:** giữa phiên trước, `design/` (20 file tracked)
  biến mất khỏi working tree mà không commit nào đụng tới. Đã
  `git checkout -- design/`, khớp HEAD. Nếu tái diễn thì nghi có tiến trình
  khác ghi vào repo.

## Quy tắc (không đổi)

Đọc `AGENTS.md`, `PRODUCT.md`, `ARCHITECTURE.md` trước khi sửa. Mỗi mục một
commit, có test, ba gate xanh. Không chạy Pulse với `--repo-root .` ở gốc;
hiện **không có target nào** để chạy thật. Sửa core chỉ khi ma sát bắt buộc;
mỗi fix có test hồi quy. Không đổi `PRODUCT.md` khi chưa có ADR.
