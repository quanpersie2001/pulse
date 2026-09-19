# Decision 0027: Review panel và đối chứng finding

## Status

Accepted — owner delegated the call (plan 0025), 2026-09-18. Triển khai Pha C
của [plan 0025](../plans/0025-parallel-verified-learning.md): C1 (cấu hình),
C2 (seat), C3 (`pulse lane reconcile`). Pha E, F, G không thuộc decision này.

**Trạng thái thực thi:** C1–C3 thi hành trong phiên này. C3 gọi lại bộ chạy
`kernel::verify::run_argv` của decision 0026 để phân xử finding có
`check.argv` — không có bộ chạy thứ hai.

## Context

"Nhiều lane" hôm nay là nhiều **góc nhìn** khác nhau, không phải nhiều
reviewer cho cùng một vấn đề: mỗi `surface-risk` profile liệt kê các lane
`review-correctness`, `review-adversarial`, `qa-api`, `qa-ui`, `check-docs`,
mỗi lane một reviewer, mỗi lane một câu hỏi riêng. Profile `api-high` chạy
ba reviewer nhưng ba người **khác việc** — không ai là người thứ hai kiểm
lại kết luận của người thứ nhất.

Hệ quả nằm ở close gate: receipt lane được tra theo `kind == "lane"` +
`payload.role`, lấy cái **mới nhất** theo `role`. Reviewer thứ hai cùng
role ghi đè receipt của reviewer thứ nhất — không phải hai phiếu, mà là một
phiếu bị che. Muốn hai–ba reviewer **độc lập cho cùng một vấn đề** thì phải
có một tầng dữ liệu mới: mỗi reviewer một "ghế" (seat), và một bước hợp
nhất các kết luận thành đúng **một** receipt mà close gate đọc.

Điều không được làm: debate tự do giữa các reviewer. Ba lý do đã cân nhắc ở
plan 0025 §0 và §C:

- **Hội tụ giả**: hai agent nói chuyện với nhau sẽ đồng ý với nhau, không
  phải vì đúng, mà vì lịch sự và vì câu chữ.
- **Bên nói dài thắng**: trong một cuộc tranh luận không có thẩm phán, lượt
  trả lời dài và tự tin hơn thắng lượt ngắn và đúng hơn.
- **Giá trị của N reviewer là cái sai KHÁC nhau**: nếu họ đọc kết luận của
  nhau trước khi tự kiểm, cái sai thứ hai bị cái sai thứ nhất hút vào, và
  ta trả tiền cho N bản sao của một người.

Vì vậy vòng 1 **mù** (mỗi seat chỉ thấy input của lane, không thấy output
của seat khác) và vòng 2 là **đối chứng từng finding**, không phải tự do
tranh luận: mỗi seat nhận danh sách finding đã ẩn danh và phải **tái hiện**
từng cái, chứ không được thuyết phục bởi cái khác.

Finding nào kiểm được bằng máy thì máy phân xử. Đây là chỗ decision 0026
được trả nợ: `check.argv` do Pulse chạy, exit thật so với `check.exit`, và
kết quả đó **thắng mọi phiếu** — một finding có check không thể bị hai
phiếu `refuted` lật, và một finding có check không thể đứng bằng hai phiếu
`confirmed` nếu lệnh thực sự pass.

## Decision

1. **Panel là opt-in theo profile.** `PULSE.md` thêm
   `panels: {<role>: {count, quorum}}` trong một profile. Không khai
   `panels` = **không panel** = đúng hành vi hôm nay (một lane, một
   receipt, một verdict). Chỉ panel khai `count >= 2` mới hợp lệ; `count: 1`
   bị từ chối vì nó là một cách nói khác của "không panel" và hai cách nói
   cùng một điều là hai đường để lệch nhau.
2. **Seat là đơn vị độc lập.** Lane có panel phải chạy `count` lần, mỗi lần
   một `--seat <n>` (1-based) và một actor riêng (`agent:<role>-<n>`).
   Một actor không được giữ hai ghế trong cùng một vòng
   (`lane_seat_actor_reused`). Seat nào cũng đi qua đúng
   `validate_and_seal` của lane thường — cùng mutation check, cùng schema,
   cùng `apply_seal_corrections` (kể cả luật D2: seat `review-*` báo `pass`
   phải có receipt `verify` của **chính actor đó**).
3. **Vòng 1 mù.** Input của một seat **byte-for-byte** giống input của lane
   thường; nó không chứa output của seat khác, không chứa actor của seat
   khác, không chứa danh sách ghế. Bằng chứng: seat chỉ ghi được
   `.pulse/evidence/<id>/<role>.<n>.json` và fence của lane chỉ cho phép
   thay đổi dưới `.pulse/evidence/<id>/`.
4. **Vòng 2 là đối chứng, không debate.** `pulse lane reconcile <id>
   <role> --prepare` gom finding của mọi seat vào **một** file ẩn danh
   (`findings: [{rid, ref, summary, owner, severity, check}]`, bỏ seat,
   actor và id gốc), sắp thứ tự tất định rồi đánh số `RF-n`. Mỗi seat đọc
   file đó và ghi một file phiếu riêng, tái hiện từng finding.
   `confirmed` cần `how` nêu lệnh đã chạy hoặc `path:line`;
   `cannot_reproduce` là câu trả lời trung thực khi không tái hiện được
   (không phải `refuted`); `refuted` cần bằng chứng ngược lại; `duplicate`
   trỏ về finding gốc.
5. **Luật phân xử** (áp cho từng finding sau khi gộp duplicate):
   - finding có `check.argv` → Pulse chạy lệnh (decision 0026). Exit thực
     `== check.exit` → `resolved`; khác (kể cả timeout, spawn fail) →
     **đứng**, giữ severity gốc. **Phiếu không lật được kết quả này.**
   - finding không có check → đứng khi số ghế ủng hộ (người nêu ∪ những
     seat bỏ phiếu `confirmed`) `>= quorum`; ngược lại hạ
     `severity: "low"` + `status: "unconfirmed"`.
   - acceptance: `pass` khi số seat báo pass `>= quorum`; `fail` khi số seat
     báo fail `>= count - quorum + 1`; còn lại `not_checked`.
   - verdict thô: `fail` nếu có AC `fail` hoặc còn finding `high` đứng;
     ngược lại `pass`; sau đó đi qua đúng `apply_seal_corrections` hiện có.
6. **Hình dạng dữ liệu để close gate gần như không đổi.** Mỗi seat ghi
   receipt `kind: "lane_seat"` — close gate không bao giờ đọc nó. Chỉ
   receipt của bước hợp nhất là `kind: "lane"` (như hôm nay) với
   `payload.reconciled: true`, `payload.seats: [receipt ids]`,
   `payload.handoff: <khoá vòng>` và `payload.votes_summary`. Với một role
   có panel, close gate chỉ tính receipt `lane` có `reconciled == true` và
   `handoff` khớp receipt handoff hiện tại.
7. **Khoá vòng là handoff.** `handoff` trong receipt seat/reconcile là id
   receipt handoff mới nhất của subject; một subject không có handoff
   (story-scope qa) dùng `head:<commit>`. Handoff lại = vòng mới: seat của
   vòng cũ không được tính, `--prepare` báo thiếu ghế, và close từ chối
   receipt reconcile của vòng cũ.
8. **Một ghế chết không chặn phân xử.** File phiếu thiếu hoặc sai schema
   cho một seat → seat đó coi như không có phiếu (ghi vào
   `votes_summary.missing`), lệnh vẫn chạy. Rid lạ hoặc `of` trỏ rid không
   tồn tại → bỏ phiếu đó, đếm vào `votes_summary.invalid`. Lý do: finding
   không đủ quorum sẽ tự hạ, nên một reviewer chết làm kết quả **yếu hơn**,
   không làm lệnh **hỏng**.
9. **Seat `fail` không bounce ticket.** Chỉ bước hợp nhất mới đổi trạng
   thái (`verifying -> active` khi verdict `fail`), để `count` seat không
   tranh nhau đưa ticket về `active` trước khi vòng 2 chạy.

### Không giải quyết (ghi trung thực)

- **Các ghế chỉ độc lập nếu host thật sự spawn session riêng.** Pulse ép
  được actor khác nhau (`lane_seat_actor_reused`, và luật cũ
  `lane_actor_not_independent`), nhưng actor vẫn là **chuỗi tự khai** —
  cùng giới hạn decision 0026 đã ghi ở mục "Không giải quyết". Một session
  cố tình đổi `--actor` vẫn tự bỏ hai phiếu. Chống gian lận có chủ đích cần
  hook host (plan 0025 Pha G1), không phải một luật trong repo.
- **Pulse không khử trùng lặp ngữ nghĩa.** Hai seat có thể nêu cùng một lỗi
  bằng hai câu chữ khác nhau; Pulse không làm được một cách trung thực
  (không có oracle ngữ nghĩa), nên nó giữ cả hai và để bước gộp `duplicate`
  của vòng 2 xử lý — do con người/agent quyết, không do suy đoán chuỗi.
- **Chi phí token ×count (+ vòng 2).** Một panel 3 ghế tốn ít nhất 3 lần
  công review cộng một vòng đối chứng. Vì vậy panel là opt-in và chỉ nên
  bật cho profile `*-high`.
- **Panel trên lane `qa-*` không hợp nhất `cases`.** Receipt reconcile mang
  `acceptance` và `findings` đã phân xử, nhưng **không** có `cases`: mỗi seat
  có artifacts riêng và Pulse không có cách trung thực để gộp chúng.
  `evaluate_close` chỉ đọc verdict nên panel qa ở scope Ticket vẫn chạy
  đúng; nhưng `close_story` gom coverage theo `cases` của receipt `lane`,
  nên panel trên một lane `qa-*` story-scope sẽ **không** thoả
  `close_story_qa_not_satisfied` — một lỗi to, tự báo, không im lặng. Panel
  được thiết kế cho lane review; gộp `cases` là việc của sau này.

## Consequences

- `PULSE.md` thêm khoá `panels` (tuỳ chọn, mặc định rỗng). Profile cũ parse
  y như trước; seed **không** bật panel nào, chỉ có comment mẫu.
- Code mới: `lane_seat_required`, `lane_seat_invalid`,
  `lane_seat_actor_reused`, `reconcile_seats_missing`,
  `reconcile_not_prepared` (mỗi code có hint). Plan 0025 gọi
  `lane_seat_unexpected` cho trường hợp "không có panel mà truyền `--seat`";
  ở đây gộp vào `lane_seat_invalid` cùng với "seat ngoài 1..=count" và
  "`--force` + `--seat`" — một code, message nói rõ trường hợp.
- `kernel::lane` thêm seat và `reconcile`; `kernel::verify::write_observed_log`
  là helper `pub(crate)` dùng chung cho log của `verify` và log phân xử
  finding (một cách ghi log, không hai).
- `templates/prompts/reconcile.md` là prompt mới, đăng ký trong
  `kernel::init::ensure_prompts`. Block seed và `skills/pulse-review` mô tả
  vòng panel; `ARCHITECTURE.md` thêm panel/seat/reconcile.
- Doctor: `stale_lane_preparations` vốn quét mọi `<stem>.snapshot.json`,
  nên `<role>.<n>.snapshot.json` (seat) và `<role>.reconcile.snapshot.json`
  (bước hợp nhất) đều tự lọt vào báo cáo — thêm test khẳng định thay vì
  đổi mẫu quét.
- Packet: `last_verdicts[].findings` chỉ còn finding `status == "open"`.
  Finding `unconfirmed` là thứ vòng 2 **đã hạ**, không phải việc phải làm
  lại; đưa nó cho worker là biến một nghi ngờ đã bị bác thành một yêu cầu.

## Rủi ro

1. **Panel bị lạm dụng cho ticket nhỏ** → token ×3 cho một thay đổi một
   dòng. Giảm nhẹ: opt-in theo profile, seed không bật, chỉ khuyến nghị
   `*-high`; đo ở dogfood trước khi bật mặc định ở đâu.
2. **Một finding `check.argv` phá cây** → log/exit ghi nhận, nhưng fence
   của receipt reconcile lấy **trước** khi chạy check, nên một check tự sửa
   file không làm receipt tự mô tả sai cây mà seat đã review; cái bắt nó
   vẫn là `handoff_unreserved_changes` (decision 0025 B6). Cùng tinh thần
   decision 0026 rủi ro 3.
3. **`how` của phiếu là lời khai** — schema ép nó không rỗng cho
   `confirmed`, không kiểm được nó có thật sự tái hiện. Giá trị thật của
   vòng 2 vẫn dựa vào việc host spawn session riêng, như mục "Không giải
   quyết".
