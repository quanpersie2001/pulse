# Decision 0011: Event log là JSONL theo ngày

## Status

Accepted, 2026-09-06.

## Context

`.pulse/events/` ghi một file JSON cho mỗi event: `<date>/evt_<ulid>.json`.
Dogfood hai Ticket trên `examples/todolist/` tạo 75 file trong một ngày, mỗi
file khoảng 300 byte. Event là append-only, không bao giờ sửa, không tra cứu
ngẫu nhiên theo id, chỉ đọc bằng cursor (`events tail --since`). Với đặc tính
đó, một file một event chỉ làm phình cây Git và chậm `read_events` mà không
mua được gì.

Các store khác không có vấn đề này: node và edge cần CAS theo `revision` và
diff riêng từng item; receipt bất biến, content-hash, được tham chiếu theo id
từ node và verification, số lượng thấp; learning có `revision` và sửa được.

## Decision

1. `.pulse/events/<YYYY-MM-DD>.jsonl`, một `EventEnvelope` canonical JSON mỗi
   dòng, kết thúc `\n`, không pretty-print. Schema envelope không đổi.
2. Ngày lấy từ `occurred_at` UTC. Một event không bao giờ nằm sai file.
3. Ghi bằng append dưới repository lock hiện có, `fsync` sau mỗi dòng. Không
   dùng temp file và rename cho event.
4. Thứ tự dòng là thứ tự ghi. `id` ULID vẫn là cursor. `events tail --since
   <ulid>` mở các file có ngày lớn hơn hoặc bằng ngày của ULID, bỏ dòng có id
   nhỏ hơn hoặc bằng cursor.
5. Crash giữa chừng để lại tối đa một dòng cuối cụt. Reader bỏ dòng cuối
   không parse được và báo `events_torn_tail` trong output JSON. Writer kế tiếp
   kiểm tra byte cuối là `\n`; nếu không, cắt dòng cụt trước khi append. Event
   đã fsync không bao giờ mất.
6. Worktree không ghi canonical; CLI ghi vào repo chính dưới lock, nên không
   có hai nhánh cùng append. Nếu sau này có, conflict ở cuối file resolve bằng
   giữ cả hai dòng.
7. `pulse events compact` chuyển đổi một lần: đọc mọi `<date>/evt_*.json`,
   ghi `<date>.jsonl` theo thứ tự ULID, xoá thư mục cũ. Chạy trong
   `examples/todolist/` rồi commit.
8. Node, edge, receipt, learning giữ một file một bản ghi.

Còn mở, mặc định giữ nguyên: gộp `evidence/execution/{handoffs,verifications,
closes}` vào `evidence/receipts/` với `kind` tương ứng. Cùng bản chất receipt,
đang tách vì lịch sử code; gộp không đổi số file.

## Consequences

- Cây Git của repo đích không phình theo số mutation.
- `read_events` đọc tuần tự vài file thay vì hàng trăm file nhỏ.
- Transaction primitive có thêm một đường ghi append bên cạnh atomic rename;
  test recovery trong `tests/process` phải cover dòng cụt.
- Cursor không đổi nên `events tail`, packet notes và ratchet không đổi contract.

## Thay đổi

- `src/event.rs`: `day_file_path` là chủ sở hữu duy nhất của luật đặt tên;
  `write_event` append canonical một dòng; `read_event_log` parse từng dòng và
  trả `torn_tails`; `compact_events` chuyển đổi legacy.
- `src/cli/events.rs`: lệnh `compact`; `events_torn_tail` báo ra stderr.
- `src/storage/append.rs`: `append_line_fsync` — cắt dòng cụt rồi append, fsync.
- Test: dòng cụt, cursor qua ranh giới ngày, compact giữ đúng thứ tự và số
  event, một day file một ngày.
- PRODUCT.md §4 layout và §5.7.

**Bổ sung khi implement (2026-09-11):** danh sách trên bỏ sót
`src/storage/transaction.rs`, nơi phần lớn event thật sự được ghi. Prepared
transaction trước đây trả lời "event của tôi đã ghi chưa" bằng *sự tồn tại của
file tại `event_path`*; với day file dùng chung, câu hỏi đó phải hỏi về **một
dòng**, nên `observed_event` tìm theo `event_id` rồi đối chiếu `event_hash`.
Kéo theo một ràng buộc mới: `event_payload.id` phải bằng `event_id` của intent,
kiểm tra ngay lúc `prepared()`. Trước 0011 hai giá trị này lệch nhau vô hại vì
path mang danh tính; giờ lệch nghĩa là recovery đọc nhầm là "chưa ghi" và
append lần hai. Bảy bản sao của vòng lặp duyệt `.pulse/events/<date>/` (ba
trong `src/`, còn lại trong test) đều phải sửa; chúng giờ đi qua
`read_event_log` và `tests/common/events.rs`.

Một điểm lệch có chủ ý so với mục 5: `events_torn_tail` được báo ra **stderr**
dạng JSON, không chèn vào payload của `tail`. One-shot `--json` của `tail` là
một mảng event mà caller đã parse như vậy, và một mảnh vỡ do crash không phải
là một event.
