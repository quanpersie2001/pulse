# Plan: Ba tầng hướng dẫn và tám skill (Decision 0019)

> Kế hoạch thực hiện cho [Decision 0019](../decisions/0019-guidance-layers-and-single-node-owner.md).
> ADR chốt **cái gì và vì sao**; file này nói **làm thế nào và theo thứ tự nào**.
> Mâu thuẫn thì ADR thắng.
>
> Trạng thái: chưa bắt đầu.

## Nguyên tắc xếp thứ tự

1. **Cơ chế trước nội dung.** Tầng 2 cần `guidance.rs` quản được hai file trước
   khi có gì để viết vào.
2. **Guard trước prose nó gác.** Guard chủ sở hữu node phải tồn tại trước khi
   viết tám skill, bằng không luật chỉ là lời dặn.
3. **Skill nào có phụ thuộc cứng thì đi sau phụ thuộc đó.** `ratchet` cần
   `knowledge retire` (0018 G1) — guard parse lệnh sẽ **từ chối**
   `pulse knowledge retire` vì subcommand chưa tồn tại.

---

## Giai đoạn 1 — Cơ chế tầng 2

**Kích thước:** nhỏ. **Phụ thuộc:** không.

### 1.1 `guidance.rs` quản N file

Hiện hard-code hai đường: khối `AGENTS.md` (marker, refresh, drift) và seed
`PULSE.md` (ghi một lần, không bao giờ ghi lại). Cần một đường thứ ba:
**file Pulse sở hữu hoàn toàn, có marker, refresh được** — giống khối AGENTS
nhưng chiếm cả file.

Chốt khi implement: dùng lại `write_agents_block` tổng quát hoá, hay tách một
`ManagedFile { path, template, marker_style }`. Nghiêng phương án hai vì hai loại
sở hữu khác nhau (một khối trong file của repo, một file trọn của Pulse).

### 1.2 `init` ghi file thứ hai

`docs/pulse-workflow.md`. `--refresh` render lại; sửa tay trong marker → vào
`guidance_conflicts`, không ghi đè.

### 1.3 Guard parse mở rộng

`guidance_sources()` trong `tests/graph/architecture_guards.rs` thêm
`assets/pulse-workflow.md`.

### Ra khỏi giai đoạn 1 khi

`pulse init` trên fixture ghi hai file; `--refresh` render lại cả hai; sửa tay
một file thì đúng file đó vào `guidance_conflicts`; guard parse phủ cả hai.

---

## Giai đoạn 2 — Guard chủ sở hữu node

**Kích thước:** nhỏ. **Phụ thuộc:** không. **Phải xong trước giai đoạn 4.**

Guard: `skills/**` không chứa `pulse work create` hay `pulse graph edge add`
ngoài `skills/pulse-planning/`.

**Kiểm bằng cách phá:** thêm `pulse work create` vào một skill khác, khẳng định
guard đỏ, rồi bỏ ra. Guard chưa từng đỏ thì chưa chứng minh được gì.

Cân nhắc khi implement: `work transition` **được phép** ở skill khác (ADR phân
biệt tạo node với transition), nên guard chỉ cấm `create` và `edge add`.

### Ra khỏi giai đoạn 2 khi

Guard bắt được vi phạm ở một file skill thật, và cho `pulse-planning` đi qua.

---

## Giai đoạn 3 — Nội dung tầng 1 và tầng 2

**Kích thước:** trung bình. **Phụ thuộc:** giai đoạn 1.

### 3.1 Co `assets/agents-block.md` xuống ~20 dòng

Giữ: Pulse là gì, luật authority khi mâu thuẫn, bảng route, trỏ sang tầng 2.
Chuyển phần còn lại xuống tầng 2.

### 3.2 Viết `assets/pulse-workflow.md` (~70 dòng)

Map sáu plane; bốn câu hỏi chẩn đoán; luồng R0 **đầy đủ**; completion standard.
R1–R3 chỉ trỏ skill.

**Áp luật chi phối khi viết:** mỗi câu phải trả lời được "câu này nói gate sẽ hỏi
gì, hay chép lại luật gate đã ép?" Loại vế sau.

### Ra khỏi giai đoạn 3 khi

Một agent chỉ đọc hai file này làm được trọn một Ticket R0, không cần skill nào.

---

## Giai đoạn 4 — Tám skill

**Kích thước:** lớn nhất. **Phụ thuộc:** giai đoạn 2 và 3.

Khuôn chung: frontmatter (**có nửa "không dùng khi nào"**) → Establish Authority
→ bước đánh số → Report.

Thứ tự viết, theo phụ thuộc chứ không theo độ khó:

| | Skill | Ghi chú |
|---|---|---|
| 4.1 | `planning` | Viết **trước tiên**: nó là chủ sở hữu node, và kỷ luật cắt mà bảy skill kia trỏ tới |
| 4.2 | `grill` | Primitive dùng ở hai tầm — trong `wayfind` và đứng riêng |
| 4.3 | `research` | `spec` phụ thuộc nó; viết trước `spec` để không có con trỏ chết |
| 4.4 | `spec` | |
| 4.5 | `wayfind` | Gọi `grill` và `research`; ghi `docs/product/` |
| 4.6 | `onboard` | `disable-model-invocation: true` |
| 4.7 | `handoff` | Cùng hook mẫu `context-guard.sh` cho `docs/operations/` |
| 4.8 | `ratchet` | **Chặn bởi 0018 G1** — xem dưới |

### Ràng buộc cứng với `ratchet`

Bước 5 của nó là *"`misleading` hai lần thì retire và gỡ"*. Guard parse lệnh
(`484c935`) sẽ **từ chối** `pulse knowledge retire` vì subcommand đó chưa tồn
tại. Nên **0018 G1 phải landed trước 4.8**, hoặc `ratchet` viết thiếu bước cuối
và phải sửa lại sau — chọn cái thứ nhất.

0018 G1 nhỏ: hai nhánh trong `transition_status` + hai subcommand.

### Ra khỏi giai đoạn 4 khi

Tám skill tồn tại, guard parse xanh, guard chủ sở hữu node xanh, ba gate xanh.

---

## Giai đoạn 5 — Dọn văn bản

Làm **sau cùng**, khi hình dạng đã đúng, để không sửa hai lần.

- `0009`: Status ghi "Narrowed by 0019" — `wayfind` không tạo node, `tickets` →
  `planning`.
- `0013`: Status ghi "Amended by 0019"; §2 bước 1 bỏ `work create`, `edge add`.
- `PRODUCT.md` §5.4 (`docs/product/` có hợp đồng), §5.6 (ratchet đề xuất chứ
  không tạo), §5.8 (ba tầng, bảng tám skill, invocation).
- `ROADMAP.md`: Now cập nhật theo thứ tự thật.

---

## Thứ tự tổng, và điểm dừng an toàn

```text
G1 cơ chế ──► G3 nội dung tầng 1+2 ──► G4 tám skill ──► G5 dọn văn bản
G2 guard ────────────────────────────┘
                     0018 G1 ────────► 4.8 ratchet
```

**Điểm dừng an toàn:** sau G3 cho một sản phẩm nhất quán — hai tầng hướng dẫn
chạy, R0 làm được, skill chưa có thì R1–R3 vẫn làm tay như hôm nay.

G4 dừng giữa chừng cũng được: skill nào viết xong thì dùng được, chưa viết thì
route trỏ vào khoảng trống — chấp nhận được **nếu** khối AGENTS không quảng cáo
skill chưa tồn tại. Kiểm điều này khi viết 3.1.

---

## Chưa chốt, để lại cho lúc implement

1. Tổng quát hoá `guidance.rs` theo hướng nào (§1.1).
2. Guard chủ sở hữu node có cấm `pulse work supersede` không, hay chỉ `create` và
   `edge add` (§2).
3. Tầng 2 có nhắc tên tám skill không, hay chỉ trỏ "xem skill" — nhắc tên thì
   phải đồng bộ khi thêm/bớt skill (§3.2).

Cả ba là quyết định implementation, không đổi contract, nên không cần ADR mới —
nhưng ghi lựa chọn và lý do vào commit tương ứng.

## Quan hệ với 0018

0018 G1 (`knowledge retire`/`supersede`) là **điều kiện cần của 4.8**. Phần còn
lại của 0018 (G2–G5) độc lập, làm sau dogfood.
