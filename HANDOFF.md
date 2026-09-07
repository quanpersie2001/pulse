# Handoff: Pulse — 0010 đóng, dogfood target đã gỡ, vào giai đoạn chốt quyết định

## Trạng thái bàn giao

- Repo: `/Users/quannv.dev/Workspace/Personal/pulse`
- Nhánh: `features/harness-experimental` (chưa push)
- Tag: `v0.1.0` ở `16a0ef3`; **`dogfood/track-b-final` ở `7fb1dd7`** — toàn bộ
  dogfood target trước khi gỡ.
- Working tree: sạch (trừ file này).
- Ba gate xanh: `cargo fmt --check`, `cargo clippy --all-targets --quiet --
  -D warnings`, `cargo test --all-targets` — **589 test**, default threading.

## Vòng này làm gì

### Decision 0010 — qa.md là markdown heading (3 commit)

`src/qa/baseline.rs` viết lại quanh heading contract, biết fence; dòng `Key:`
lạ bị từ chối kèm nguyên văn dòng. Block `pulse-check` → `QaCheck` với argv
tách quote, từ chối toán tử shell. `case_hash` (sha256 section chuẩn hoá) thay
`revision` ở receipt payload, snapshot readiness và cả hai close gate.
`baseline_content_hash` **cố ý** giữ hash byte nguyên văn vì content binding
băm lại file trên đĩa. Bốn khác biệt so với bản ADR ghi ở §Status của
`docs/decisions/0010-*.md`.

### Hai fix từ friction (2 commit)

- **Wrapped bullet trong ticket.md** (`f770587`): dòng xuống hàng dưới
  `## Open questions` giờ nối vào câu hỏi trên nó thay vì bị từ chối (6/6
  Ticket từng fail sync đầu vì cái này). Mở rộng có chủ ý sang
  `list_section`/`parse_acceptance`/`parse_key_values` vì cả ba **im lặng** cắt
  mất nửa sau của bullet xuống hàng.
- **`list_receipts` nuốt lỗi** (`7fb1dd7`): một receipt không decode được từng
  giết cả listing, và hai callsite trong `kernel/run.rs` bọc
  `unwrap_or_default()` nên biến nó thành proof list **rỗng im lặng** cho
  reviewer. Giờ có `unreadable[] {id, path, reason}`, reviewer input mang nó,
  prompt reviewer giải thích nó nghĩa gì.

### Gỡ dogfood target

`examples/todolist/` (337 file tracked) đã gỡ. Lý do: **Decision 0009 chưa
implement**, nên mọi lần chạy đều thiếu lớp skill mà thiết kế Pulse đòi —
friction thu được lẫn "thiếu một tầng" với "thiết kế sai".

Cứu trước khi gỡ:
- `git mv` friction log → `docs/dogfood-friction-track-b.md`, kèm header giải
  thích nó **không phải** cơ chế `friction` mà Pulse thiết kế và nó trộn hai
  loại nội dung.
- `PRODUCT.md` §7 ghi thẳng sự kiện đã đạt (ngày, HEAD, số receipt) thay vì trỏ
  vào thư mục sắp biến mất.
- Tag `dogfood/track-b-final`. Lấy lại:
  `git checkout dogfood/track-b-final -- examples/todolist`.

ADR (8 file) **để nguyên** trích dẫn đường dẫn cũ — chúng là bản ghi lịch sử.

## Hiểu đúng về "friction" (đã trace từ code, đừng suy lại)

`friction` trong Pulse là **cơ chế của repo đích, không phải của Pulse**:
`pulse note --kind friction` → `.pulse/events/` của repo đó → close gate biến
thành learning scope `harness` → promote vào `AGENTS.md`/`PULSE.md`/
`runners.json` **của repo đó**. Không mắt xích nào chảy ngược về Pulse.
`PRODUCT.md` không có khái niệm báo bug lên upstream.

Hệ quả: `docs/dogfood-friction-track-b.md` **không phải** friction theo nghĩa
đó. Phần lớn nội dung là defect của Pulse core (sửa bằng code), thiểu số mới
là harness friction thật. `--kind friction` cũng chưa implement.

## Trạng thái §7 golden path

| Tiêu chí | Thực tế |
|---|---|
| Mục 1–7 | Đạt 2026-09-05, HEAD `845ff01` |
| `close-story` trên baseline thật | **Đạt** — ST-001 (09-05), ST-002 (09-06) |
| Hai Ticket song song không va nhau | **Chưa** — TK-006/TK-007 đã va; 0015 sửa, có test, **chưa chạy lại thật** |

Không còn dogfood target ⇒ mục cuối **không có cách kiểm chứng**, ⇒ cổng "chưa
đạt thì không thêm feature" ở đầu §7 hiện vô định. Cần quyết trước khi thêm
feature.

## Việc tiếp theo: chốt quyết định, chưa code

| | Quyết định | Trạng thái |
|---|---|---|
| mới | §7 lấy gì làm cổng khi không còn dogfood target | **Chặn mọi feature** |
| 0009 | Bề mặt skill | Accepted, chưa implement, phạm vi cần xem lại. Dữ liệu duy nhất là `docs/dogfood-friction-track-b.md` |
| mới | Repo downstream phát hiện bug Pulse thì ghi vào đâu | Thiết kế không có cửa nào |
| treo | `unreadable` trong `PRODUCT.md` §5.3 | Sửa contract mà chưa có ADR — viết ADR hay revert |
| 0011 | Event log `.jsonl` + compact | Accepted, chưa động dòng nào |
| 0004 | Packet lease-bound | Nên đánh "Superseded in part by 0008" |

## Đính chính đã phát hiện

- `AGENTS.md` từng khai `runtime/`/`cache/` của target là ignored — **sai**.
  Pattern `.pulse/runtime/` trong `.gitignore` neo ở gốc repo nên không khớp
  thư mục con; 152 file đó chưa bao giờ được ignore, chỉ chưa từng commit.
- `AGENTS.md` liệt kê 8 test crate, thực tế 10 — thiếu `tests/runner.rs` và
  `tests/communication.rs`.
- Giữa phiên, `design/` (20 file tracked) biến mất khỏi working tree mà không
  commit nào của phiên này đụng tới. Đã `git checkout -- design/`, khớp HEAD.
  Không rõ nguyên nhân; nếu tái diễn thì đáng nghi có tiến trình khác ghi vào
  repo.

## Quy tắc (không đổi)

Đọc `AGENTS.md`, `PRODUCT.md`, `ARCHITECTURE.md` trước khi sửa. Mỗi mục một
commit, có test, ba gate xanh. Không chạy Pulse với `--repo-root .` ở gốc;
hiện **không có target nào** để chạy thật. Sửa core chỉ khi ma sát bắt buộc;
mỗi fix có test hồi quy. Không đổi `PRODUCT.md` khi chưa có ADR.
