# Decision 0026: Pulse chạy argv đã khai (`pulse verify`)

## Status

Accepted, 2026-09-18 (owner: quan). Triển khai Pha D của
[plan 0025](../plans/0025-parallel-verified-learning.md).

**Trạng thái thực thi:** phiên này thi hành D1 (`pulse verify`) và D2
(handoff/lane đọc receipt verify). Pha C (panel chạy `finding.check.argv`)
và Pha E (learning check) sẽ gọi lại đúng bộ chạy của D1, không viết bộ
chạy thứ hai. Pha E, F, G không thuộc session này.

## Context

Hai chỗ bằng chứng quan trọng nhất của hệ hôm nay vẫn là **lời khai**, không
phải quan sát:

- `pulse handoff` đọc `verify_results` trong `handoff.json` — chính worker
  tự khai `exit` của từng lệnh (`src/kernel/completion.rs`, violation
  `handoff_verify_failed`). Gate chỉ so tên và con số mà worker vừa tự viết
  ra.
- `pulse lane seal` đọc `commands_run` trong output của lane: reviewer khai
  nó đã chạy gì, chạy ra sao. Không có gì buộc lời khai đó đúng.

Ngoài `git status`/`HEAD` (source fence), Pulse chưa từng tự chạy một lệnh
nào của repo đích rồi ghi lại điều nó thấy. Nghĩa là "verify" trong hệ hiện
tại là một nghi thức chữ, không phải một phép đo — đúng khoảng trống mà
plan 0025 §0 gọi là "handoff tin lời khai (`exit` không được đọc)".

Tiền lệ đã có sẵn: `pulse docs check` chạy `generated_by.check_argv` của
từng doc (`src/docs/check.rs::check_generated_by`) và ghi `CommandRun` với
exit code thật. Pulse đã chạy argv do record khai từ lâu; điều còn thiếu là
một cơ chế dùng chung có tên, có receipt, có fence.

## Decision

1. **`pulse verify <id>` chạy đúng `verify[].argv` của record**, không hơn:
   không shell (`sh -c`), không tự chọn lệnh, không suy diễn lệnh từ
   `surface`/`risk`/`title`. Mỗi entry là `{name, argv[], cwd?}` theo đúng
   schema Ticket đang có (`src/schema/issue.schema.json`), `argv` không rỗng.
2. **Ghi một receipt `kind: "verify"`** với `source = fence_for(ticket)` và
   payload `{results: [{name, argv, cwd, exit, timed_out, duration_ms, log}],
   passed}` — `log` là **đường dẫn** file log, `artifact_paths` là chính các
   file đó nên `record_receipt` hash sẵn `sha256`. Fence chụp **sau** khi
   chạy xong (xem rủi ro 3).
3. **Log là đuôi có chặn, đã redaction**: giữ **32 KiB cuối** mỗi lệnh —
   đúng con số của [Decision 0024](0024-persist-run-output.md) §3 ("the
   interesting end of a broken run is its last lines"), không dùng 64 KiB
   mà plan 0025 D1 phác — cắt ở ranh giới UTF-8 hợp lệ, ghi
   `.pulse/evidence/<id>/verify/<name>.log`. Nếu
   `evidence::redaction::clean_text` **từ chối** log (bí mật), log được thay
   bằng một câu nói rõ nó bị giữ lại — **exit code vẫn được ghi**. Redaction
   không bao giờ nuốt kết quả của lệnh.
4. **Không dừng ở lệnh fail đầu tiên**: chạy hết mọi entry theo thứ tự (mẫu
   "báo hết" của các gate) rồi mới kết luận `passed`. Một lệnh **không
   spawn được** (ENOENT…) không phải lỗi của cả lượt chạy: nó vào kết quả
   với `exit: null` và một log nói rõ, để một lệnh hỏng không che kết quả
   các lệnh khác.
5. **Quyền**: thêm `Action::Verify` vào bảng `kernel::roles` — human và
   **mọi** agent (worker chạy trước handoff; lane chạy lại độc lập); system
   thì không. Ticket phải `active` hoặc `verifying` → ngược lại
   `verify_not_runnable`; **không** đòi giữ lease (lane không giữ lease).
   `verify[]` rỗng → `verify_nothing_declared`.
6. **Handoff đọc receipt, không đọc lời khai.** `evaluate_handoff` với
   ticket có `verify[]` không rỗng: thiếu receipt → `handoff_verify_missing`
   (tái dùng mã cũ, đổi message); fence receipt lệch fence hiện tại
   (`profile::same_fence`, không tự so tay) → `handoff_verify_stale`; có
   result `exit != 0` hoặc thiếu một `name` đang khai → `handoff_verify_failed`
   (liệt kê tên). `verify_results` trong `handoff.json` **giữ field** cho
   tương thích (`deny_unknown_fields` vẫn bật, handoff cũ không vỡ) nhưng từ
   đây chỉ là thông tin.
7. **Lane `review-*` phải tự chạy**: báo `pass` cho ticket có `verify[]` mà
   không có receipt `verify` **do chính actor đang seal** ghi, trên fence
   hiện tại, mọi exit 0 → hạ `inconclusive` (đối xứng luật "qa-ui pass không
   ảnh", `kernel::lane::apply_seal_corrections`). Lane `qa-*`/`check-*`
   không bị luật này; Story làm subject thì không áp dụng.
8. **Ranh giới cứng**: Pulse không chạy agent, không chạy nền, không daemon,
   không retry, không song song hoá lệnh trong một lượt. Một lần gọi = chạy
   tuần tự rồi thoát. Timeout (mặc định 900s) là hết.

### Vì sao điều này không phá "không chạy test, không dispatch"

(a) **Đã có tiền lệ** — `docs::check` chạy `generated_by.check_argv` và ghi
`CommandRun` từ trước decision này; `pulse verify` là cùng một hành vi, được
gọi tên và gắn receipt.

(b) **argv nằm trong record mà chỉ `human:` sửa được** — `verify[]` là field
của Ticket, sửa nó là `Action::MutateGraph`, mà bảng `roles` không cho agent
quyền đó. Agent không tự nhét lệnh vào được; nó chỉ chạy thứ người đã khai
trong record.

(c) **Pulse không điều phối** — nó không chọn chạy gì, không quyết định khi
nào, không spawn agent, không đọc kết quả để rẽ nhánh. Nó chạy đúng argv đã
khai và ghi lại điều nó quan sát: **quan sát, không điều khiển**.

### Không giải quyết (ghi trung thực)

- **Danh tính vẫn là chuỗi tự khai.** Một session cố tình đổi `--actor` vẫn
  tự review được. Luật TTY cho `human:` **bị loại** vì skill `pulse-plan`
  và `pulse-shape` chạy `--actor human:<name>` từ trong session agent — luật
  đó sẽ phá chúng, không phải bảo vệ chúng. Chống gian lận **có chủ đích**
  cần hook ở host (plan 0025 Pha G1), không phải một luật trong repo.
- **Đã có thay thế, nhưng ở mức ràng buộc chứ không phải chứng minh:** ràng
  `run_id` giữa handoff/checkpoint và lease (decision 0025 B3);
  `claim_actor_busy` cấm một actor giữ hai lease sống (B3); và ở Pha C, seat
  actor phải khác nhau. Chúng làm gian lận *vô tình* khó xảy ra và gian lận
  *có chủ đích* phải cố ý hơn — chúng không làm nó bất khả.

## Consequences

- Câu "does not run tests" đổi nghĩa ở mọi nơi nó xuất hiện — `AGENTS.md`,
  `ARCHITECTURE.md`, `README.md`, block seed
  `templates/seeds/agents-block.md` → **"runs only the argv a record
  declares (`verify[]`), and records what it observed"**, giữ nguyên "does
  not run agents, has no daemon".
- `ARCHITECTURE.md` thêm `kernel::verify` vào bản đồ tầng; prompt
  `worker.md`, hai prompt review, `skills/pulse-plan`, `skills/pulse-review`
  nói đúng việc mới: chạy `pulse verify <id>` thay vì tự khai exit code.
  `protocol` trong packet thêm `"verify": "pulse verify <id>"`.
- Exit code tiến trình của `pulse verify`: 0 khi mọi result `exit == 0`,
  ngược lại `verify_failed` (để script/host gate được). **Receipt luôn được
  ghi TRƯỚC khi trả lỗi** — một verify fail là bằng chứng, không phải thứ bị
  nuốt.
- Code mới: `verify_nothing_declared`, `verify_not_runnable`,
  `verify_argv_invalid`, `verify_failed`; violation `handoff_verify_stale`.
  Event `verify.recorded {passed, receipt}` — `infer_subject_kind` **không**
  cần biết tiền tố `verify.` mới: `receipt.recorded` hôm nay đã rơi vào
  `subject_kind: "resource"`, và payload mang receipt id.
- `kernel::verify::run_argv` là bộ chạy dùng chung cho Pha C và Pha E; module
  tên `verify` (không phải `run`/`process`) theo guard
  `daemon_runtime_tree_is_absent`.
- Ghi đè log cùng tên ở lần chạy sau là **chủ ý**: receipt cũ giữ `sha256`
  của log cũ nên nó vẫn tự mô tả đúng điều nó đã thấy; file trên đĩa là bản
  mới nhất.
- Doctor không thêm check: receipt `verify` của một ticket `verifying` mà
  `passed == false` không thể đi qua gate, nên không có trạng thái lạ để bắt.

## Rủi ro

1. **Lệnh treo** → timeout (mặc định 900s), rồi `kill()` + `wait()`. Biết và
   chấp nhận: tiến trình **cháu** có thể sống sót sau `kill` của cha (session
   này không dựng process group); xử lý ở host/`ps`, không phải trong verify.
2. **Log lộ bí mật** → redaction. Đây là ranh giới cơ học với danh sách
   pattern cố định (Decision 0012 §4), **không** phải bộ lọc thông minh: bí
   mật không khớp pattern vẫn lọt vào log đã commit. Prompt worker phải nhắc
   đừng in secret ra.
   **Đo được khi thi hành:** vì `clean_text` coi một giá trị *bắt đầu* bằng
   `/` là **cả một đường dẫn**, và một log có newline cuối không bao giờ
   canonicalize được như một đường dẫn, nên **mọi log bắt đầu bằng đường dẫn
   tuyệt đối đều bị giữ lại** — một `pwd` là đủ. Exit code luôn còn (đó là
   thứ gate đọc); log là thứ mất. Tương tự, một `argv` chứa đường dẫn tuyệt
   đối ngoài repo (hay chuỗi giống bí mật) làm `record_receipt` từ chối cả
   payload: lệnh đã chạy, log đã ghi, nhưng **không có receipt** — ranh giới
   tracked-plane thắng.
3. **Lệnh phá cây** → fence chụp **sau** khi chạy, nên một verify tự sửa file
   sẽ làm chính receipt của nó khớp cây **đã sửa**. Đây là hành vi **chủ ý**,
   không phải lỗ hổng: receipt nói "**trên cây NÀY**, các lệnh cho kết quả
   NÀY". Hệ quả cần biết: `handoff_verify_stale` **không** bắt được một verify
   ghi vào cây — cái bắt nó là `handoff_unreserved_changes` (decision 0025 B6)
   khi file đó nằm ngoài `touches`.
4. **Verify là lời khai về môi trường, không phải về hành vi**: một `verify[]`
   chứa `true` vẫn "pass". Decision này làm exit code **đo được** thay vì
   **khai**; nó không bảo đảm lệnh được khai là lệnh có ý nghĩa.
