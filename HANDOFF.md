# Handoff: Pulse — nợ ADR đã trả hết, tiếp theo là skill surface

## Trạng thái bàn giao

- Repo: `/Users/quannv.dev/Workspace/Personal/pulse`
- Nhánh: `features/harness-experimental` (chưa push)
- HEAD: `21f1756` docs: close the two ADR debts
- Tag: `v0.1.0` ở `16a0ef3`; `dogfood/track-b-final` ở `7fb1dd7`
- Working tree: **sạch**
- Ba gate xanh tại HEAD: `cargo fmt --check`, `cargo clippy --all-targets
  --quiet -- -D warnings`, `cargo test --all-targets` — **616 test** (trước là
  596), default threading.

Bốn commit của phiên này:

```
f903b6e feat(init): write the AGENTS.md block and PULSE.md (0009 part A)
0776335 feat(events): one JSONL file per day (0011)
03bd3c4 feat(knowledge): expected_signal gates a ratchet learning (0012)
21f1756 docs: close the two ADR debts (0004 narrowed, 0017 written)
```

Thứ tự làm việc do người dùng chốt: **hoàn thành hết ADR đã accepted → dựng
skill → instruction flow → rồi mới tạo example**. Ngược với handoff trước (định
lấy dữ liệu dogfood rồi mới chốt phạm vi part B), nhưng nhất quán hơn với chính
lý do `PRODUCT.md` §13.1 gỡ target cũ: chạy trên harness dở dang thì friction
lẫn "thiếu một tầng" với "thiết kế sai".

---

# ĐÃ LÀM

## Audit ADR: 16 → 17 record, nợ code còn đúng một mục

Kết quả audit từng ADR đối chiếu code (đừng audit lại, trừ khi nghi ngờ):

| ADR | Trạng thái |
|---|---|
| 0010, 0014, 0015, 0016 | Landed từ trước |
| 0011 | **Landed phiên này** |
| 0012 | Landed, `expected_signal` **bổ sung phiên này** |
| 0013 | `note --work`, `context_exhausted` có; `.pulse/runtime/handoff/<node>.md` và skill `pulse-handoff` thuộc part B |
| 0009 | A ✅ (`f903b6e`), C ✅ (phiên trước), **B (skill) chưa** |
| 0004 | Đánh dấu narrowed by 0008 |
| 0017 | **Mới viết**, ghi lại thay đổi đã landed ở `7fb1dd7` |

Nợ ADR còn lại = **đúng skill surface**, tức việc kế tiếp theo kế hoạch.

## 0011 — event log JSONL theo ngày

`.pulse/events/<date>.jsonl`, một event một dòng canonical compact, append
fsync qua `storage::append_line_fsync`.

**Điều ADR không nói và tốn nhiều thời gian nhất:** danh sách "Thay đổi" của
0011 bỏ sót `src/storage/transaction.rs`, nơi **phần lớn event thật sự được
ghi**. Prepared transaction trước đây trả lời "event của tôi đã ghi chưa" bằng
*sự tồn tại của file tại `event_path`*. Với day file dùng chung, câu đó phải
hỏi về **một dòng**: `observed_event` tìm theo `event_id` (substring
`"id":"evt_…"` để khỏi parse mọi dòng) rồi đối chiếu `event_hash`.

Kéo theo ràng buộc mới, check ngay tại `prepared()`: `event_payload.id` phải
bằng `event_id` của intent. Trước 0011 lệch nhau vô hại vì path mang danh tính;
giờ lệch = recovery đọc thành "chưa ghi" và **append lần hai** — duplicate im
lặng trong append-only log.

Điểm lệch có chủ ý so với ADR mục 5: `events_torn_tail` báo ra **stderr** dạng
JSON, không chèn vào payload `tail`. One-shot `--json` của `tail` là một mảng
event mà caller đã parse như vậy. Đã ghi vào ADR.

## 0012 — `expected_signal`

`Learning.expected_signal: Option<String>`, bắt buộc cho kind `ratchet`.

Gate `validate` **tách theo ai phán đoán được**:

- Nửa máy (Pulse kiểm): `--evidence` phải là **handoff receipt**
  (`.pulse/evidence/execution/handoffs/`, **không** phải `evidence/receipts/`)
  và mang `knowledge_usage` với đúng learning id + outcome `helpful`.
- Nửa ngữ nghĩa (Pulse không kiểm): signal là prose, receipt là prose. Actor
  khẳng định bằng `--signal-observed`; khẳng định được ghi ở
  `validation.signal_observed_at` kèm actor. Nguyên tắc 5.

`transition_status` vượt ngưỡng arg của clippy → refactor thành
`TransitionEvidence` thay vì `#[allow]`. Argument list vốn là union nhu cầu của
ba transition khác nhau, nên đó là fix đúng chứ không phải né lint.

**Giữ nguyên** ràng buộc `required_checks` không rỗng cho kind `ratchet` — ADR
0012 không nói gì về nó. Hệ quả: friction-derived candidate vẫn dùng
`ProcessInsight`.

---

# VIỆC CỦA PHIÊN SAU

## 0009 part B — skill surface (bảy skill + `pulse-handoff`)

Điều kiện tiên quyết: gỡ guard `legacy_skill_surfaces_are_absent`
(`tests/graph/architecture_guards.rs:104`) đang cấm `skills`, `dist`,
`.codex-plugin`, `.claude-plugin`. Thay bằng **guard parse lệnh**: mọi lệnh
`pulse …` trong `skills/**` và template khối AGENTS phải parse được bằng clap
của crate (`PRODUCT.md` §5.8).

Tám skill theo bảng `PRODUCT.md` §5.8: `wayfind`, `grill`, `spec`, `tickets`,
`research`, `ratchet`, `onboard` (0009) + `handoff` (0013). Mỗi skill kết thúc
ở một artifact và một trạng thái graph; skill là hướng dẫn, CLI là authority —
không state riêng, không gate riêng.

`pulse-handoff` mang theo phần 0013 còn thiếu:
`.pulse/runtime/handoff/<node>.md` và hook mẫu cho Claude Code.

Sau đó: instruction flow → dựng dogfood target mới → chạy thật.

## Hai thứ chỉ kiểm chứng được khi chạy thật

1. **Hai Ticket song song không va nhau** — tiêu chí phụ §7 duy nhất còn
   `chưa đạt`. TK-006/TK-007 đã va; 0015 sửa nguyên nhân, có
   `tests/runner/worktree_dispatch.rs`, **chưa chạy lại thật lần nào**.
2. **Khối `AGENTS.md` có dẫn được agent từ intent tới Ticket `ready` không.**
   Track B chỉ chạy TK-003..TK-008 — toàn Ticket đã shaped tay. Nhật ký friction
   **im lặng** về giai đoạn trước `ready`, không phải **phản đối** nó.

## Bug đã biết, phải xử trước khi dựng target

`.gitignore` neo ở gốc repo: pattern `.pulse/runtime/` **không** khớp thư mục
con. Target cũ đã vô tình track runtime state vì lỗi này. Xem `PRODUCT.md`
§13.1.

## Hàng đợi quyết định còn mở

| | Quyết định | Trạng thái |
|---|---|---|
| 0009 | Phạm vi part B | Người dùng chốt: làm đủ trước khi dogfood |
| mới | Repo downstream phát hiện bug Pulse thì ghi vào đâu | Thiết kế không có cửa nào |
| PRODUCT §13 | Bốn mục "còn mở" cuối file | Vẫn theo mặc định |

## Bẫy đã học, đừng dẫm lại

- **Lock không reentrant.** `WriteGuard` là flock; lấy lần hai trong cùng
  process là `LockTimeout`. `show_node`, `KnowledgeStore::create`,
  `KnowledgeStore::list`, `verify_receipt` đều lấy nó. Trong fence chỉ dùng
  biến thể `_unlocked` / `_under_lock`, hoặc làm trước khi lấy guard.
  `load_receipt` **không** lấy lock nên an toàn trong fence.
- **Vòng lặp duyệt event bị copy bảy lần** — ba trong `src/`, bốn trong test.
  Hai bản trong `src/` là bug chờ sẵn: `has_recording_event` chỉ descend vào
  *thư mục* ngày, nên sau 0011 mọi receipt đọc ra `integrity: invalid`. Giờ tất
  cả đi qua `event::read_event_log` và `tests/common/events.rs`. **Đừng viết
  bản thứ tám.**
- **`to_canonical_bytes` là pretty-print có newline cuối.** Dùng
  `to_canonical_line_bytes` cho bất cứ thứ gì một-bản-ghi-một-dòng.
- **`list_receipts`** trả `unreadable[]` thay vì fail cả listing. Đừng bọc
  `unwrap_or_default()` quanh nó ở callsite mới — Decision 0017.
- **Continuation `\` trong string literal Rust** nuốt cả indent dòng sau. Test
  data cho parser thụt lề phải dùng raw string.
- **`cargo test --all-targets` sau `cargo clippy --all-targets`** phải build
  lại từ đầu (khác profile): tính **8–14 phút**, đừng đặt timeout 120s.
- **`cmd | tail` nuốt exit code của cmd.** Một lần trong phiên này `clippy` fail
  mà chuỗi `&& echo "CLIPPY OK"` vẫn in OK. Kiểm `PIPESTATUS` hoặc chạy riêng.
- **`AGENTS.md` liệt kê 8 test crate, thực tế 10** — thiếu `tests/runner.rs` và
  `tests/communication.rs`. Chưa sửa.

## Quy tắc (không đổi)

Đọc `AGENTS.md`, `PRODUCT.md`, `ARCHITECTURE.md` trước khi sửa. Mỗi mục một
commit, có test, ba gate xanh. Không chạy Pulse với `--repo-root .` ở gốc;
hiện **không có target nào** để chạy thật. Sửa core chỉ khi ma sát bắt buộc;
mỗi fix có test hồi quy. Không đổi `PRODUCT.md` khi chưa có ADR.
