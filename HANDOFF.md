# Handoff: Pulse — 0009 phần C đã xong, tiếp theo là phần A

## Trạng thái bàn giao

- Repo: `/Users/quannv.dev/Workspace/Personal/pulse`
- Nhánh: `features/harness-experimental` (chưa push)
- HEAD: `d2db8b5` feat(close): derive harness learning candidates from friction
- Tag: `v0.1.0` ở `16a0ef3`; `dogfood/track-b-final` ở `7fb1dd7` — toàn bộ
  dogfood target trước khi gỡ.
- Working tree: **sạch** (trừ file này).
- Ba gate xanh tại HEAD: `cargo fmt --check`, `cargo clippy --all-targets
  --quiet -- -D warnings`, `cargo test --all-targets` — **596 test** (trước là
  589; thêm 7), default threading.

Hai commit của phiên này:

```
1f2bf28 feat(note): record a note kind and read friction back (0009 part C)
d2db8b5 feat(close): derive harness learning candidates from friction (0009 part C)
```

Handoff trước xếp phần C thành bốn mục và yêu cầu mỗi mục một commit. Mục 1
tách được thật nên đứng riêng. Mục 2 và 3 là một chuỗi phụ thuộc cứng — mục 3
không compile nếu thiếu trường `frictions` của mục 2 — nên nằm chung một commit
thay vì dựng ranh giới giả. Mục 4 (test) nằm trong chính hai commit đó.

---

# ĐÃ LÀM: đường ống friction (0009 phần C)

Nửa **sản xuất** của vòng harness learning giờ đã chạy. Nửa tiêu thụ đã có sẵn
từ trước (`run.rs` render `## Harness learnings`, `packet.rs` lọc khỏi
injection).

## Hai quyết định thiết kế, đã chốt với người dùng

**1. `--friction` khi handoff ghi vào `HandoffReceipt`, không ghi note.**
`HandoffReceipt` thêm `frictions: Vec<String>` với `#[serde(default,
skip_serializing_if = "Vec::is_empty")]` — đúng khuôn `checks` /
`acceptance_proofs` / `knowledge_usage` đã dùng, nên receipt cũ giữ nguyên
fingerprint.

Lý do (đã ghi trong commit): gọi `record_note` ở đây sẽ chạm `show_node`, mà
`show_node` lấy chính write guard handoff đang giữ (`nodes.rs:267` vs
`completion.rs:88`) — đúng lớp bug `e27b5e4`. Ghi trước khi lấy guard thì an
toàn về lock nhưng handoff là idempotent: retry cùng idempotency key sẽ append
note trùng. Receipt vừa atomic vừa idempotent.

Đánh đổi đã chấp nhận: friction từ handoff **không** hiện trong `events tail`
và không vào packet.

**2. Sinh candidate là recoverable, không phải best-effort.**
`KnowledgeStore::create` tự lấy write guard nên close gate không gọi được.
Thêm `create_unlocked` và `learnings_derived_from_unlocked`, theo tiền lệ
`release_reservation_under_lock`.

Derivation chạy **sau** transaction close, nên lỗi ở đó không làm hỏng close.
Vì close idempotent và retry thoát sớm ở replay path, derivation **cũng chạy
trên replay path** — nếu không thì một lần lỗi là mất luôn, không có đường
quay lại. Dedup seed từ learning đã lưu rồi mở rộng trong vòng lặp, nên replay
là no-op và một report đến từ cả note lẫn receipt vẫn chỉ sinh một candidate.

## Ba ràng buộc của knowledge model — do test bắt được, không phải do đọc model

Đây là phần dễ mất thời gian nhất nếu phải tìm lại:

- **`LearningKind::Ratchet` dùng không được.** `validate.rs:300` bắt buộc
  `guidance.required_checks` không rỗng; derivation tự động không có check nào
  để nêu. Dùng `ProcessInsight`.
- **`Applicability` rỗng bị từ chối.** `validate_applicability`
  (`validate.rs:378`) đòi **cả** một chiều positive **và** một chiều concrete
  (`model.rs:465`, `481`). `signals: ["harness_friction"]` là chiều concrete
  duy nhất gate này điền được một cách trung thực — nó load-bearing, đừng gỡ.
- **`Guidance` rỗng cũng bị từ chối** (`validate.rs:343`). Đặt một câu nói đúng
  trạng thái thật của candidate: nguyên liệu thô chờ `pulse-ratchet` phân loại
  (hằng `FRICTION_GUIDANCE` trong `completion.rs`). Câu đó không bao giờ tới
  prompt worker vì learning `candidate` không tự inject (`PRODUCT.md` dòng
  1031).

`expected_signal` mà `PRODUCT.md` dòng 1089 mô tả cho kind `ratchet` **không
tồn tại trong code**. Không có gì phụ thuộc vào nó.

## Bề mặt mới

```text
pulse note --work <id> --message "<text>" [--kind note|friction]
pulse work handoff ... [--friction "<text>"]   # lặp lại được
```

`--kind` mặc định `note`, nên mọi caller cũ giữ nguyên hành vi và cả output
người đọc. Kind lưu ở key `kind` của payload `note.recorded`; payload là JSON
không typed nên note ghi trước thay đổi này đọc lại thành `note`, không cần
migration.

`list_friction_for_ticket` (`communication.rs`) là reader của close gate. Khác
`list_notes_for_ticket`, nó **không** truncate và **không** cap ở 8: packet là
ngân sách context có giới hạn, còn ở đây bỏ rơi một report là mất bằng chứng.

## Test đã thêm

- `tests/communication.rs`: kind mặc định là `note`; `--kind friction` ghi đúng
  payload; cả hai kind vẫn vào packet; kind sai bị clap từ chối (không im lặng
  rơi về `note`).
- `tests/graph/reservation.rs`: fixture `close_with_friction` chạy hết chuỗi
  reserve → activate → handoff → verify → close. Phủ: note friction sinh
  candidate scope `harness` status `candidate`; friction từ handoff cũng sinh;
  không có friction thì không sinh gì; replay không nhân đôi; note và receipt
  trùng nội dung chỉ sinh một.

---

# VIỆC CỦA PHIÊN SAU

## Phần A — khối `AGENTS.md` + `PULSE.md` (0009 Quyết định 1)

Hiện **chưa có gì**: không `PULSE:BEGIN` ở đâu trong `src/`, không
`assets/agents-block.md`. Đây là điều kiện cần để nối lại dogfood, và
`0009 §Thứ tự` xếp nó ngay sau phần C.

- `pulse init` ghi khối `<!-- PULSE:BEGIN --> … <!-- PULSE:END -->` vào
  `AGENTS.md` và tạo `PULSE.md`.
- `pulse init --refresh` render lại theo version CLI, giữ nguyên nội dung
  ngoài marker; phát hiện sửa tay trong marker thì **báo, không ghi đè**.
- `assets/agents-block.md` là template dùng chung cho `init` và test.
- Nội dung khối: đúng bảng route trong `0009 §Flow theo hình dạng yêu cầu`,
  không hơn. Dòng friction giờ đã có lệnh thật đứng sau nó.

Sau đó: **dựng dogfood target mới** rồi chạy thật bằng agent tương tác. Rồi mới
quyết phạm vi phần B (bảy skill) bằng dữ liệu — cần gỡ guard
`legacy_skill_surfaces_are_absent` (`tests/graph/architecture_guards.rs:104`)
đang cấm `skills`, `dist`, `.codex-plugin`, `.claude-plugin`.

## Đính chính về 0009 (giữ nguyên, đừng suy lại)

Handoff các phiên trước lặp lại: *"toàn bộ friction là ergonomics CLI, không
mục nào là 'agent không biết làm gì tiếp' ⇒ 0009 có thể thừa"*. **Sai.**

`0009 §Context`: *"Từ intent đến Ticket `ready` không có gì dẫn agent;
developer làm tay."* Track B chỉ chạy TK-003..TK-008 — toàn Ticket đã shaped
sẵn bằng tay. Nhật ký friction chỉ phủ giai đoạn **sau `ready`**. Giai đoạn
trước `ready` chưa bao giờ giao cho agent nên không thể sinh mục friction nào.
Nhật ký **im lặng** về nửa đó, không phải **phản đối** nó.

## Hiểu đúng "friction" (đã trace từ code, đừng suy lại)

`friction` là cơ chế **của repo đích, không phải của Pulse**: `note --kind
friction` → `.pulse/events/` của repo đó → close gate → learning scope
`harness` → promote vào `AGENTS.md`/`PULSE.md`/`runners.json` **của repo đó**.
Không mắt xích nào chảy ngược về Pulse.

Hệ quả: `docs/dogfood-friction-track-b.md` **không phải** friction theo nghĩa
đó. Phần lớn là defect của Pulse core (sửa bằng code). Header file ghi rõ rồi.

## Trạng thái 0009 trong code

| Phần | Có gì |
|---|---|
| Khối `AGENTS.md` + `PULSE.md` | Chưa gì cả |
| `note --kind friction` + close gate → candidate | **Xong** (phiên này) |
| Bảy skill | Chưa; guard còn cấm `skills/` |
| Đích harness learning | Đã chạy — scope, prompt injection, packet filtering |
| Learning lên `validated` qua rerun | Chưa; cố ý ngoài phạm vi phiên này |

## Trạng thái §7 golden path

| Tiêu chí | Thực tế |
|---|---|
| Mục 1–7 | Đạt 2026-09-05, HEAD `845ff01` |
| `close-story` trên baseline thật | Đạt — ST-001 (09-05), ST-002 (09-06) |
| Hai Ticket song song không va nhau | **Chưa** — TK-006/TK-007 đã va; 0015 sửa, có test, chưa chạy lại thật |

Không còn dogfood target ⇒ mục cuối chưa kiểm chứng được. Đường thoả mãn là
A → dựng target mới → chạy thật.

## Hàng đợi quyết định còn mở

| | Quyết định | Trạng thái |
|---|---|---|
| 0009 | Phạm vi phần B (bảy skill) | Chờ dữ liệu từ dogfood mới |
| mới | Repo downstream phát hiện bug Pulse thì ghi vào đâu | Thiết kế không có cửa nào |
| treo | `unreadable` trong `PRODUCT.md` §5.3 | Sửa contract mà chưa có ADR — viết ADR ngắn hay revert hunk |
| 0011 | Event log `.jsonl` + compact | Accepted, chưa động dòng nào |
| 0004 | Packet lease-bound | Nên đánh "Superseded in part by 0008" |
| mới | `PRODUCT.md` dòng 1089 nói learning `ratchet` có `expected_signal` | Field không tồn tại; hoặc bỏ khỏi PRODUCT hoặc implement |

## Bẫy đã học, đừng dẫm lại

- **Lock không reentrant.** `WriteGuard` là flock; lấy lần hai trong cùng
  process là `LockTimeout`. `show_node`, `KnowledgeStore::create`,
  `KnowledgeStore::list`, `verify_receipt` đều lấy nó. Trong fence chỉ dùng
  biến thể `_unlocked` / `_under_lock`, hoặc làm việc đó trước khi lấy guard.
- **`list_receipts`** trả `unreadable[]` thay vì fail cả listing. Đừng bọc
  `unwrap_or_default()` quanh nó ở callsite mới — bug đã sửa ở `7fb1dd7`.
- **Continuation `\` trong string literal Rust** nuốt cả indent dòng sau. Test
  data cho parser thụt lề phải dùng raw string.
- **`cargo test --all-targets` sau `cargo clippy --all-targets`** phải build
  lại từ đầu (khác profile): tính **8–14 phút**, đừng đặt timeout 120s.
- **`.gitignore` neo ở gốc:** `.pulse/runtime/` **không** khớp thư mục con.
  Đã đính chính ở `PRODUCT.md` §13.1.
- **`AGENTS.md` liệt kê 8 test crate, thực tế 10** — thiếu `tests/runner.rs` và
  `tests/communication.rs`. Chưa sửa.
- **Bất thường chưa giải thích:** một phiên trước, `design/` (20 file tracked)
  biến mất khỏi working tree mà không commit nào đụng tới. Đã
  `git checkout -- design/`, khớp HEAD. Tái diễn thì nghi có tiến trình khác
  ghi vào repo. Phiên này không thấy lại.

## Quy tắc (không đổi)

Đọc `AGENTS.md`, `PRODUCT.md`, `ARCHITECTURE.md` trước khi sửa. Mỗi mục một
commit, có test, ba gate xanh. Không chạy Pulse với `--repo-root .` ở gốc;
hiện **không có target nào** để chạy thật. Sửa core chỉ khi ma sát bắt buộc;
mỗi fix có test hồi quy. Không đổi `PRODUCT.md` khi chưa có ADR.
