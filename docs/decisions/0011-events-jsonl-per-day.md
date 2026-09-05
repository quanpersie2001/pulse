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

- `src/event.rs`: `event_path` trả `<date>.jsonl`; `write_event` append với
  lock và fsync; `read_events` parse từng dòng, bỏ dòng cụt cuối.
- `src/cli/events.rs`: `tail --since` theo ngày của ULID; lệnh `compact`.
- `src/storage/`: helper append-with-fsync và kiểm tra byte cuối.
- Test: ghi song song dưới lock, dòng cụt, cursor qua ranh giới ngày, compact
  giữ đúng thứ tự và số event.
- PRODUCT.md §4 layout và §5.7.
