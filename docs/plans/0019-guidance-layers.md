# Plan: Hai tầng hướng dẫn và tám skill (Decisions 0019, 0020)

> Kế hoạch thực hiện cho
> [Decision 0019](../decisions/0019-guidance-layers-and-single-node-owner.md),
> được thu hẹp bởi
> [Decision 0020](../decisions/0020-collapse-guidance-into-agents.md).
> ADR chốt **cái gì và vì sao**; file này nói **làm thế nào và theo thứ tự nào**.
> Mâu thuẫn thì ADR mới hơn thắng.
>
> Trạng thái: đang thực hiện. Command-parsing guard, planning-only guard,
> `DOC-GLOSSARY` và draft `pulse-wayfind` đã có trong working tree ngày
> 2026-09-15.

## Nguyên tắc xếp thứ tự

1. **Guard trước prose nó gác.** Single-owner phải được kiểm bằng máy trước khi
   có nhiều skill.
2. **Skill upstream trước downstream, theo thứ tự chuỗi thật.** Decision 0021
   đặt chuỗi là `wayfind → grill → spec → planning`, nên `grill` và `spec` đi
   trước `planning`: chúng định nghĩa hợp đồng `works/_drafts/<slug>/` mà
   planning nhận nuôi.
3. **Instruction flow sau skill.** Chỉ quảng cáo skill trong block AGENTS khi
   skill đó tồn tại; tránh route tới path chết trong giai đoạn làm dở.
4. **Dependency cứng đi trước skill.** `ratchet` cần `knowledge retire` và
   `knowledge supersede` của 0018 G1.

---

## Giai đoạn 1 — Guards và bootstrap destinations

**Trạng thái:** phần chính đã landed hoặc có trong working tree.

- Guard parse literal `pulse …` bằng clap thật trong `assets/agents-block.md` và
  `skills/**`.
- Guard cấm `pulse work create` và `pulse graph edge add` ngoài
  `skills/pulse-planning/`.
- Negative proof: tạm thêm command tạo node vào skill khác, test phải đỏ, rồi
  bỏ probe.
- `pulse init` seed và đăng ký `DOC-GLOSSARY` mà không overwrite nội dung có
  sẵn.

### Ra khỏi giai đoạn 1 khi

Hai guard xanh, negative proof đã chạy, init glossary idempotent và không có
`docs/pulse-workflow.md` trong source hoặc output init.

---

## Giai đoạn 2 — Tám skill

Khuôn chung: frontmatter có cả “dùng khi nào” và “không dùng khi nào” →
`Establish Authority` → các bước đánh số → `Report`.

| Thứ tự | Skill | Contract chính |
|---|---|---|
| 2.1 | `wayfind` | `docs/product/` và decision frontier; không tạo node |
| 2.2 | `grill` | nghĩa đã chốt, `works/_drafts/<slug>/story.md`, glossary |
| 2.3 | `research` | primary-source file thuộc Ticket chủ, hoặc dưới draft khi chưa có node |
| 2.4 | `spec` | `approach.md`, `qa.md` cùng draft; không phỏng vấn lại |
| 2.5 | `planning` | chủ sở hữu duy nhất của node/edge; cắt Ticket, nhận nuôi draft, ready |
| 2.6 | `onboard` | human-only; read-only pass trước mutation |
| 2.7 | `handoff` | host/human invoked; flush, live-thread doc, note, stop |
| 2.8 | `ratchet` | ba lane, one intervention, expected signal, fresh rerun |

`wayfind` và `planning` đã draft và eval trong working tree. `planning` được
viết theo hai-mode của 0019 nên phải cắt lại theo 0021 sau khi `grill` và `spec`
chốt hợp đồng draft — kèm chạy lại eval.

### `planning` vào một lần (Decision 0021)

Đầu vào là một draft đã có `story.md`, `approach.md` và `qa.md`. Đầu ra là Epic,
Story và implementation Ticket dựng một lượt, prose được nhận nuôi vào
`works/<id>/`, Ticket đủ gate thì `ready`.

Vẫn giữ từ 0019: propose breakdown cho human trước mutation, reuse node hiện có,
tạo blocker trước và wire edge ở pass thứ hai, fog không thành node, R0 không tự
động gọi planning.

Nhận nuôi theo thứ tự copy → `work sync` → `qa baseline` → xoá draft, để chạy
lại sau khi vỡ là idempotent. Không thêm lệnh CLI cho việc này.

### Ràng buộc cứng với `ratchet`

Trước 2.8, implement 0018 G1:

- `pulse knowledge retire <id> --reason <text>`;
- `pulse knowledge supersede <id> --by <id>`;
- lifecycle/relation transaction và test tương ứng.

Guard parse sẽ từ chối command chưa tồn tại, nên không viết ratchet thiếu bước
rồi hứa sửa sau.

### Ra khỏi giai đoạn 2 khi

Tám skill tồn tại; mỗi skill có eval source; command guard và planning-only
guard xanh; `quick_validate.py` xanh cho từng skill.

---

## Giai đoạn 3 — Khối AGENTS hoàn chỉnh

**Phụ thuộc:** skill được block quảng cáo đã tồn tại.

Sửa `assets/agents-block.md` thành khoảng 35–45 dòng. Giữ đúng năm phần:

1. Pulse là truth layer; prose route, CLI quyết định.
2. Authority khi accepted Decision, approved product docs, code/test và receipt
   mâu thuẫn.
3. Bốn câu hỏi chẩn đoán trước mutation.
4. Luồng R0 đầy đủ ở mức command và artifact.
5. Completion standard cùng route các flow đặc biệt sang skill đã cài.

Không tạo `assets/pulse-workflow.md`, không seed `docs/pulse-workflow.md`, không
mở rộng `kernel/guidance.rs` thành N managed files.

**Luật review từng câu:** câu này nói gate sẽ hỏi gì/cách cung cấp đầu vào, hay
chép lại luật gate đã ép? Xoá vế thứ hai.

### Ra khỏi giai đoạn 3 khi

- Một agent chỉ đọc AGENTS block làm được trọn Ticket R0.
- Read-only request dừng mà không mutation.
- R1–R3 route tới skill có thật.
- `pulse init --refresh` vẫn giữ nội dung ngoài marker và báo
  `guidance_conflicts` khi block bị sửa tay.
- Init không tạo hoặc trỏ `docs/pulse-workflow.md`.

---

## Giai đoạn 4 — Hook handoff

Cùng `pulse-handoff`, thêm template `context-guard.sh` trong
`docs/operations/` khi user yêu cầu qua option init đã chốt ở Decision 0013.
Mặc định init không tự gắn hook host.

Kiểm Stop hook chống loop bằng `stop_hook_active`, threshold bytes cấu hình được
và reason bắt agent chạy handoff rồi dừng.

---

## Giai đoạn 5 — Dọn văn bản và dogfood

- `0009`: ghi narrowed by 0019; bỏ mô tả wayfind tạo Epic và tên `tickets` cũ.
- `0013`: ghi amended by 0019; handoff không tạo node/edge.
- `0019`: đã ghi guidance layers narrowed by 0020.
- `PRODUCT.md`: giữ hai tầng, product contract, single-node-owner và invocation.
- `ROADMAP.md`: cập nhật theo thứ tự thật.
- Dựng target mới chỉ sau khi guidance surface hoàn chỉnh; chạy golden path từ
  intent tới ready bằng agent tương tác, không gõ lệnh hộ.

---

## Thứ tự tổng và điểm dừng an toàn

```text
G1 guards/bootstrap
  -> G2 wayfind -> planning -> grill -> research -> spec -> onboard -> handoff
                                      0018 G1 -----------------------> ratchet
  -> G3 AGENTS block
  -> G4 hook
  -> G5 docs cleanup + new dogfood target
```

Dừng giữa G2 được nếu AGENTS block hiện tại chưa quảng cáo skill chưa tồn tại.
Không có file tầng giữa hoặc route chết cần giữ để biểu diễn tiến độ.
