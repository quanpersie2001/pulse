# Decision 0016: Worker sở hữu docs receipt, gate chặn tại handoff

## Status

Accepted, 2026-09-07. Sửa contract của `work handoff` (§5.3) và reviewer
input (§5.3); close gate không đổi. Đệ trình từ ma sát đo được của Track B
vòng 1, ghi trong `examples/todolist/works/friction.md`.

## Context

Ba trên sáu Ticket của vòng Track B vòng 1 (TK-004, TK-005, TK-008) bị rework
vì **cùng một khoảng trống**: docs receipt không được tham chiếu trong chuỗi
proof. `HANDOFF.md` ghi nhận cả ba là một gap duy nhất; `friction.md` gọi nó
là "Whose job is the docs receipt? … Both defensible — the contract is
ambiguous."

Đo được trong `examples/todolist` sau vòng đó:

```
16  documentation_validation receipt  ← cho 8 Ticket
 9  ghi bởi runner:worker
 4  ghi bởi runner:reviewer
 3  ghi bởi human:quanpersie2001
```

Gấp đôi số Ticket, ba actor cùng ghi một loại bằng chứng. Đó là hình dạng của
một trách nhiệm không có chủ.

Khoảng trống không nằm ở một chỗ. Nó có ba tầng chồng lên nhau:

### Tầng 1 — prompt worker im lặng

Bootstrap prompt worker (`src/kernel/run.rs`) có bảy bước; bước 2 bảo **đọc**
docs required. Không bước nào bảo **ghi** `docs validate --record`. Prompt
reviewer thì nói rõ: "A required-docs Ticket needs a documentation_validation
receipt referenced in the proofs, or close will refuse."

Worker không biết đó là việc của mình. Reviewer biết có việc đó nhưng không
biết của ai.

### Tầng 2 — `--evidence-receipt` của handoff là ngõ cụt

`work handoff --evidence-receipt` tồn tại, Pulse verify receipt trước fence
(`completion.rs`), lưu vào `HandoffReceipt.evidence_receipt_ids`. Nhưng
`validate_documentation_close` chỉ đọc từ receipt của reviewer:

```rust
for receipt_id in verification.acceptance_proofs
    .iter().flat_map(|proof| proof.evidence_receipt_ids.iter())
```

Trường của handoff không tới bất kỳ gate nào. Worker làm đúng hết — ghi
receipt, tham chiếu trong handoff — vẫn không thoả close được; reviewer vẫn
phải tự tham chiếu lại. Nên "dạy worker làm đúng" không bao giờ đóng được
khoảng trống này.

### Tầng 3 — reviewer được đưa một danh sách luôn rỗng

`reviewer-input.json` có `proof_receipts.documentation_validation` để reviewer
biết receipt nào đã tồn tại mà dùng lại. Nó lọc theo `subject.id == ticket_id`.
Nhưng docs receipt có `subject.kind = "documentation_registry"` và
`subject.id = repository_id`, `bindings.work` rỗng — **đúng theo thiết kế**,
vì nó chứng minh "docs required của repo current tại commit X", không phải
"docs của Ticket này".

Bộ lọc đó vì vậy không bao giờ khớp:

```
pulse evidence receipt list --subject TK-005 --kind documentation_validation
  → []            (cái reviewer nhìn thấy)
pulse evidence receipt list --kind documentation_validation
  → 16 receipts   (thực tế)
```

Reviewer được bảo đi tìm một thứ, được đưa một danh sách rỗng cứng, trong khi
thứ đó tồn tại. Cả hai lựa chọn nó có — tự ghi (TK-003) hay rework worker
(TK-004/005/008) — đều đúng theo prompt của chính nó.

### Vì sao chi phí lặp lại

`close` xảy ra **sau** khi reviewer đã làm xong việc. Mỗi lần thiếu docs
receipt, chi phí là trọn một vòng review, không phải một lệnh. Ba lần.

## Decision

1. **Worker sở hữu việc ghi docs receipt.** Khi `## Documentation impact`
   của Ticket có posture `required`, worker chạy `pulse docs validate
   --record --actor agent:runner:worker` trước khi handoff và truyền
   `--evidence-receipt <id>`. Bootstrap prompt worker nói rõ bước này.
2. **`work handoff` từ chối khi thiếu.** Posture `required` mà không receipt
   nào được tham chiếu là `documentation_validation` thì handoff fail với
   `handoff_documentation_receipt_missing`, kèm đúng lệnh cần chạy. Lỗi lộ
   tại bước của người sửa được nó, không phải sau một vòng review.
3. **Reviewer input liệt kê theo source commit, không theo ticket id.**
   `proof_receipts.documentation_validation` liệt kê docs receipt `passed`
   bound tới commit đang review. Bộ lọc theo ticket id bị bỏ vì nó không thể
   khớp với subject của loại receipt này.
4. **Reviewer tham chiếu lại, không tự ghi thay worker.** Prompt reviewer nói
   rõ receipt đã có trong input và việc của reviewer là verify rồi đưa vào
   `--proof`. Reviewer **vẫn được phép** ghi một receipt mới khi docs thật sự
   đổi dưới tay nó — cấm hẳn sẽ chặn ca hợp lệ đó — nhưng đó là ngoại lệ có
   lý do, không phải đường mặc định.
5. **Close gate không đổi.** Nó vẫn chỉ đọc proof của reviewer. Việc reviewer
   phải tự verify và tự tham chiếu là bước độc lập, và nguyên tắc 3 của
   `PRODUCT.md` giữ nguyên: người verify khác người làm.

## Alternatives Considered

1. **Reviewer sở hữu việc ghi.** Loại: reviewer sẽ sản xuất chính bằng chứng
   mà nó dựa vào để phán, đi ngược nguyên tắc 3. Nó cũng phát hiện docs hỏng
   sau khi worker đã rời phiên, nên vòng sửa vẫn tốn một dispatch.
2. **Chỉ sửa prompt, giữ gate ở close.** Loại: đó đúng là tầng 1 một mình.
   Tầng 2 khiến worker làm đúng vẫn không đủ, và chi phí mỗi lần trượt vẫn là
   một vòng review — thứ đã trả ba lần.
3. **Cho close đọc `evidence_receipt_ids` của handoff.** Loại: sẽ để bằng
   chứng của chính người làm thoả gate, mất tính độc lập. Trường đó vẫn giữ
   nguyên vai trò là claim của worker và là đầu vào của gate handoff mới.
4. **Gắn `bindings.work` của docs receipt vào Ticket.** Loại trong đợt này:
   docs receipt là bằng chứng về registry của repo tại một commit, không phải
   về một Ticket; gắn work binding sẽ làm sai nghĩa để tiện cho một bộ lọc.
   Lọc theo source commit đạt cùng mục đích mà không nói dối.

## Kiểm chứng

- Integration (`tests/runner/`): Ticket posture `required` mà handoff không
  tham chiếu docs receipt thì fail với `handoff_documentation_receipt_missing`
  và Ticket ở nguyên `active`; tham chiếu đủ thì handoff pass và Ticket sang
  `verifying`; posture `none` không bị ảnh hưởng.
- Integration: reviewer input của một Ticket có docs receipt tại commit đang
  review liệt kê đúng receipt đó (regression cho danh sách rỗng cứng).
- Unit: receipt được tham chiếu nhưng sai kind (ví dụ `qa_checkpoint`) không
  thoả gate handoff.

## Thay đổi

- `src/kernel/completion.rs`: gate posture `required` trong `record_handoff`.
  `load_receipt` không lấy lock nên kiểm tra chạy an toàn trong fence — đúng
  ràng buộc mà e27b5e4 đã dạy (`verify_receipt` mới là cái lấy lock, và nó
  vẫn chạy trước fence).
- `src/kernel/run.rs`: bước docs receipt trong prompt worker; lọc
  `proof_receipts.documentation_validation` theo source commit; prompt
  reviewer nói rõ "dùng lại receipt trong input".
- `PRODUCT.md` §5.3, §13.
- Đóng mục friction "Whose job is the docs receipt?".

## Consequences

- Vòng rework vì docs receipt biến mất: worker không thể handoff mà thiếu nó.
- Worker tốn thêm một lệnh trước handoff. Đó là chi phí đúng chỗ: rẻ hơn một
  vòng review đúng một bậc độ lớn.
- Reviewer hết phải đoán. Danh sách trong input là thật, nên đường mặc định
  là dùng lại chứ không phải tự ghi.
- `HandoffReceipt.evidence_receipt_ids` hết là trường chết: nó là đầu vào của
  gate handoff. Nó vẫn **không** thoả close gate, và đó là chủ ý.
- Ticket posture `deferred` hay `unknown` không bị gate này chạm; chúng đã bị
  `validate_close_postures` chặn ở close.
