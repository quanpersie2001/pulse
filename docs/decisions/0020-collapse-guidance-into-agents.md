# Decision 0020: Gộp workflow thường ngày vào khối AGENTS

## Status

Accepted, 2026-09-15.

**Thu hẹp [Decision 0019](0019-guidance-layers-and-single-node-owner.md):**
Pulse dùng hai tầng hướng dẫn thay vì ba. Bỏ `docs/pulse-workflow.md`; khối
Pulse quản lý trong `AGENTS.md` giữ luôn bốn câu hỏi chẩn đoán, luồng R0 và
completion standard. Skill vẫn là tầng nạp theo nhu cầu cho R1–R3.

Giữ nguyên mọi quyết định khác của 0019: hợp đồng `docs/product/`,
`pulse-planning` là chủ sở hữu duy nhất của graph shape, tám skill và invocation
policy.

## Context

0019 tách guidance thành:

```text
AGENTS.md (~20 dòng) -> docs/pulse-workflow.md (~70 dòng) -> skills
```

Lý do là progressive disclosure: `AGENTS.md` được host nạp ở mọi session, còn
workflow chỉ cần trước mutation. Nhưng với session có mutation — đường chính của
Pulse — agent vẫn phải đọc cả hai file. Tách file không giảm context trên đường
đó, đồng thời thêm:

- một lần đọc và một con trỏ agent có thể không theo;
- một Pulse-owned file phải version, hash, refresh và drift-detect;
- một path mới mà docs validation phải bảo đảm tồn tại;
- một điểm conflict và một failure mode bootstrap mới.

Ownership cũng không bắt buộc file riêng. `pulse init` đã sở hữu đúng một block
có marker trong `AGENTS.md`, giữ nguyên mọi nội dung repository viết ngoài
marker và từ chối ghi đè khi body bị sửa tay.

Pulse không cần copy `docs/WORKFLOW.md` của repository-harness theo cấu trúc.
Repository-harness phải dùng prose để ép nhiều luật mà Pulse đã có gate cơ học.
Workflow còn lại của Pulse đủ mỏng để sống trong entrypoint mà không thành một
encyclopedia.

## Decision

### Hai tầng guidance

1. **Khối Pulse trong `AGENTS.md` (~35–45 dòng, luôn được nạp).** Bao gồm:
   - Pulse là truth layer nào;
   - authority khi nguồn mâu thuẫn;
   - bốn câu hỏi chẩn đoán trước mutation;
   - luồng R0 đầy đủ ở mức command/artifact;
   - completion standard;
   - route R1–R3 và các ca hiếm sang skill.
2. **`skills/**` (nạp theo nhu cầu).** Chứa nghi thức chuyên biệt cho wayfind,
   grill, planning, spec, research, ratchet, onboard và handoff.

Không tạo hay seed `docs/pulse-workflow.md`.

### Luật chi phối prose giữ nguyên

> Prose nói gate sẽ hỏi gì và làm sao cung cấp artifact/evidence. Prose không
> chép lại luật mà CLI đã ép.

Do đó gộp file không có nghĩa chuyển lifecycle authority sang `AGENTS.md`.
Block chỉ route và chỉ cách đi; clap, policy và gate vẫn quyết định.

### R0 ở entrypoint

R0 không gọi skill. Khối AGENTS phải đủ để một agent hoàn thành bounded change
rõ hướng mà không mở thêm hướng dẫn Pulse. Đây là lý do phần R0 nằm trực tiếp
trong block thay vì ở một file trung gian.

### Ownership

- Pulse chỉ sửa nội dung giữa `<!-- PULSE:BEGIN -->` và
  `<!-- PULSE:END -->`.
- Repository giữ nội dung ngoài marker và toàn bộ `PULSE.md` sau khi seed.
- Skill giữ chi tiết R1–R3.
- CLI giữ lifecycle, authority và evidence gates.

## Alternatives Considered

1. **Giữ ba tầng của 0019.** Loại: chỉ tiết kiệm context cho session read-only,
   nhưng làm mutation path dài hơn và thêm một managed surface.
2. **Đưa workflow vào `PULSE.md`.** Loại: `PULSE.md` là authority của repo đích,
   không phải Pulse-owned operating guidance.
3. **Đẩy cả R0 vào một skill.** Loại: R0 là đường thường, gọi skill tạo ceremony
   và để entrypoint không đủ dùng.
4. **Khôi phục block 65 dòng nguyên trạng của 0009.** Loại: block cũ vừa route
   vừa mô tả nhiều flow chuyên biệt. Block mới chỉ giữ phần chung và R0; chi tiết
   R1–R3 vẫn ở skill.

## Consequences

- Mutation session mất một lần đọc và một failure mode do dead link.
- `pulse init` tiếp tục quản đúng một guidance surface có drift detection.
- Read-only session nạp thêm khoảng 15–25 dòng so với thiết kế ba tầng.
- R0 có golden path ngay trong entrypoint.
- Khối AGENTS phải giữ kỷ luật kích thước; chi tiết của skill không được trôi
  ngược vào block.

## Verification

- `assets/agents-block.md` chứa authority, bốn câu hỏi, R0 flow, completion và
  route sang skill trong khoảng mục tiêu 35–45 dòng.
- Guard parse mọi literal `pulse …` command trong template và `skills/**` bằng
  clap thật.
- `pulse init` và `--refresh` chỉ quản block AGENTS cùng `PULSE.md`; không tạo
  `docs/pulse-workflow.md`.
- Test init khẳng định block đủ route R0 và không chứa path
  `docs/pulse-workflow.md`.
- Một agent chỉ đọc `AGENTS.md` làm được trọn một Ticket R0.

## Required changes

- Sửa `PRODUCT.md` §5.8 thành guidance hai tầng.
- Sửa plan 0019: bỏ giai đoạn managed workflow file; viết lại giai đoạn nội dung
  thành một AGENTS block 35–45 dòng.
- Sửa `pulse-wayfind` và các skill sau không trỏ `docs/pulse-workflow.md`.
- Khi triển khai guidance content, co/thay `assets/agents-block.md` theo contract
  trên; không thêm `assets/pulse-workflow.md` hay code quản file thứ ba.
