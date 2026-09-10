# Decision 0017: Receipt không đọc được phải được báo, không được xoá khỏi danh sách

## Status

Accepted, 2026-09-11. Ghi lại sau khi code đã landed ở `7fb1dd7` (2026-09-07):
commit đó sửa `PRODUCT.md` §5.3 để thêm `proof_receipts.unreadable` vào
contract của reviewer input mà không có ADR nào chống lưng, trái quy tắc
"không đổi `PRODUCT.md` khi chưa có ADR". ADR này đóng khoảng trống đó và giữ
nguyên hành vi đang chạy; nó không đề xuất thay đổi mới.

## Context

`list_receipts` parse mọi file trong receipt store và trả về lỗi decode đầu
tiên gặp phải. Một receipt còn sót lại từ một lần đổi shape payload là đủ để
hạ mọi caller.

Đó chính là chuyện đã xảy ra: bảy `qa_checkpoint` receipt viết trước Decision
0010 làm `pulse evidence receipt list` fail cho **toàn bộ** dogfood repository
— không receipt loại nào đọc được qua CLI nữa. Chúng được phát hiện tình cờ
trong lúc migrate, không phải bởi một gate nào.

Nửa nguy hiểm thì im lặng hơn. Cả hai callsite của `list_receipts` trong
`kernel/run.rs` bọc lời gọi trong `unwrap_or_default()`, nên cùng điều kiện đó
đưa cho reviewer một `proof_receipts` **rỗng** thay vì một lỗi. Reviewer đọc
"không có `qa_checkpoint` nào tồn tại" rồi rework một worker đã ghi receipt
hoàn toàn đúng.

Đó đúng là hình dạng lỗi mà [Decision 0016](0016-docs-receipt-ownership-at-handoff.md)
vừa đóng ở tầng 3, nơi một danh sách docs luôn rỗng đã lấy mất ba trên sáu
Ticket của Track B một vòng review. Cùng một lớp lỗi, một plane khác.

Điểm chung của cả hai: **"không tồn tại" và "không nhìn được" là hai kết luận
ngược nhau, nhưng cùng được biểu diễn bằng một danh sách rỗng.**

## Decision

1. **`list_receipts` liệt kê tiếp và gom cái hỏng.** File không decode được đi
   vào `unreadable[] {id, path, reason}`; phần còn lại vẫn được liệt kê. Chỉ
   một *thư mục* receipt không đọc được mới còn là lỗi.
2. **Filter không bao giờ giấu `unreadable`.** Kind và subject chính là thứ
   không đọc được, nên lọc chúng ra theo kind hay subject sẽ dựng lại đúng sự
   im lặng vừa gỡ.
3. **`ReceiptList` luôn serialize `unreadable`, kể cả khi rỗng.** Caller phải
   phân biệt được "không có" với "không nhìn được"; một trường vắng mặt khi
   rỗng buộc caller phải đoán.
4. **Reviewer input mang cùng danh sách đó** tại `proof_receipts.unreadable`,
   **và prompt reviewer giải thích ý nghĩa**: một proof list rỗng chỉ có nghĩa
   "không tồn tại" khi `unreadable` cũng rỗng; ngược lại reviewer phải báo
   finding thuộc về evidence store, không rework worker. Một trường mà prompt
   không bao giờ nhắc tới thì mới chỉ là nửa cái sửa.
5. **Lỗi thật vẫn propagate** từ cả hai callsite thay vì thành danh sách rỗng.

## Alternatives Considered

1. **Fail cả listing khi có một receipt hỏng** (hành vi cũ). Loại: một receipt
   sót lại từ lần đổi shape làm mù toàn bộ CLI, kể cả những đường không liên
   quan gì tới nó. Blast radius không tương xứng với lỗi.
2. **Bỏ qua im lặng file không decode được.** Loại: đây đúng là cái
   `unwrap_or_default()` đã làm, và nó là nguồn của vòng rework oan. Bỏ qua im
   lặng biến mất mát bằng chứng thành lời khẳng định "không có bằng chứng".
3. **Migrate receipt cũ rồi giữ nguyên contract.** Loại: migrate sửa được lần
   này, không sửa được lần sau. Bất kỳ thay đổi payload nào trong tương lai
   cũng tái tạo lại đúng tình huống này; contract phải chịu được nó.
4. **Chỉ serialize `unreadable` khi không rỗng.** Loại: caller khi đó không thể
   phân biệt "đã kiểm, không có gì hỏng" với "phiên bản cũ không có trường
   này". Luôn có mặt là thứ làm cho im lặng đọc được.

## Kiểm chứng

`tests/runner/reviewer.rs::unreadable_receipt_reaches_the_reviewer_instead_of_emptying_its_proof_list`
đặt một receipt không decode được rồi khẳng định: listing vẫn trả lời và gọi
tên nó, reviewer input báo cáo nó, proof list `qa_checkpoint` vẫn rỗng chứ
không mượn nó, và prompt có giải thích trường đó.

## Thay đổi

- `src/evidence/receipt/store.rs`: `unreadable[]` trong `ReceiptList`, filter
  không chạm tới nó.
- `src/kernel/run.rs`: `proof_receipts.unreadable`, bỏ `unwrap_or_default()`
  ở cả hai callsite, prompt reviewer giải thích trường.
- `PRODUCT.md` §5.3 (đã sửa ở `7fb1dd7`; ADR này là cơ sở còn thiếu của nó).

## Consequences

- Một receipt hỏng làm mất đúng receipt đó, không làm mù cả plane.
- Reviewer không bao giờ đọc mất mát bằng chứng thành thiếu bằng chứng, nên
  không rework worker vì lỗi của evidence store.
- Danh sách rỗng trở thành một khẳng định đọc được: rỗng cộng `unreadable`
  rỗng nghĩa là "đã kiểm, không có".
- Vẫn còn một họ receipt thứ hai (`evidence/execution/*`) không đi qua đường
  này. Gộp hai họ là mục Later trong `PRODUCT.md` §11, chưa làm ở đây.
