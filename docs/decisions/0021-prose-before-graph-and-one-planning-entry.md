# Decision 0021: Prose trước graph, và planning vào một lần

## Status

Accepted, 2026-09-15.

**Sửa [Decision 0019](0019-guidance-layers-and-single-node-owner.md):** thứ tự
chuỗi skill đổi thành `wayfind → grill → spec → planning`; `pulse-planning` vào
**một lần** cho mỗi chuỗi giao hàng thay vì hai lần ở hai tầm; hai mode A/B của
planning bị bỏ.

Giữ nguyên luật cốt lõi của 0019: `pulse-planning` là chủ sở hữu duy nhất của
graph shape, skill khác chỉ transition trạng thái mà nó gác. Giữ nguyên
[Decision 0020](0020-collapse-guidance-into-agents.md) về hai tầng guidance.

## Context

### Luật quyền ghi đã đẩy một bước vào giữa luồng nhận thức

0019 chốt đúng một skill được tạo node. Hệ quả không lường trước: `grill` ghi
`works/<story-id>/story.md`, nên Story phải tồn tại **trước** grill, nên
`planning` phải chạy trước grill để tạo nó. Chuỗi thành:

```text
wayfind → planning#1 (Epic, Story draft) → grill → spec → planning#2 (Ticket) → run
```

Đọc lên thành "vẽ ranh giới công việc trước khi biết ranh giới nằm ở đâu". Và
vì planning vào hai lần ở hai tầm, `SKILL.md` của nó phải dạy hai mode cho cùng
một kỷ luật cắt.

### Hai nguồn tham chiếu đều đặt hiểu trước hình dạng

`references/mattpocock/skills`: `wayfinder → grill-with-docs → to-spec →
to-tickets`. Không có bước tạo node nào trước grill.

`references/skills` (Khuym): `exploring → planning → validating`. `exploring`
là grill — Socratic locking, một câu một message, mỗi quyết định gán ID ổn định
`D1`/`D2`/`D3` — và nó **chạy trước** planning. Anti-pattern của nó liệt kê
thẳng `beads` trong danh sách những thứ không được làm ở bước đó.

Khuym cũng cho thấy việc **vào planning hai lần không phải do luật một chủ sở
hữu node**: nó không có luật đó, nhưng hard gate của `planning` vẫn là *"create
beads only after validation accepts feasibility"*, và `validating` bước 6 nói
*"return to planning to create current-story/work beads, then resume
validation"*. Tạo node thi công muộn là hình dạng hội tụ, không phải hệ quả của
0019.

Điều Khuym làm khác là **chỗ ghi**: `history/<feature-slug>/CONTEXT.md` — địa
chỉ theo slug, tracked, tồn tại trước khi work graph có gì.

### Nguyên nhân gốc là khoá của prose, không phải thứ tự skill

Hai mốc tạo node bị `spec` chen vào giữa:

- `spec` ghi `works/<story-id>/approach.md` → Story phải có trước spec;
- cắt Ticket cần `approach.md` → Ticket phải có sau spec.

Không gộp được khi prose bắt buộc sống dưới `works/<node-id>/`. Đổi khoá thành
slug thì hai mốc thành một.

### Điều kiện hiện tại của code

- `src/graph/validation/graph.rs:253` chỉ ràng buộc `content_dir` **của một
  node** phải đúng `works/<node-id>`; không quét `works/` để cấm thư mục khác.
  Không có đường nào liệt kê thư mục con của `works/` — node list đọc
  `.pulse/workgraph/nodes/`.
- `src/kernel/run.rs:1481` hardcode `works/<story-id>/qa.md`, nên `pulse qa
  baseline` chỉ chạy được **sau** khi Story tồn tại và prose đã vào chỗ.
- `src/docs/policy.rs:173` và `src/docs/validate.rs:212` cấm đăng ký document
  có path dưới `works/`. Draft không có mặt trong `pulse docs search` — nhưng
  `works/<id>/` sau nhận nuôi cũng vậy, nên đây không phải giá mới.
- `research/<topic>.md` chỉ là quy ước; chuỗi `research` không xuất hiện trong
  `src/` ngoài template decision_work. Chuyển nó sang vùng slug tốn 0 dòng code.

## Decision

### Vùng prose trước graph

```text
works/
  _drafts/<feature-slug>/      # pre-graph, tracked
    story.md                   # grill ghi
    approach.md                # spec ghi
    qa.md                      # spec ghi
    research/<topic>.md        # research ghi khi chưa có Ticket chủ
```

Vẫn là plane **work prose** — nó trả lời đúng câu hỏi *"thay đổi nào đang được
đề xuất?"* — nên thừa hưởng nguyên git ownership của `works/` (tracked) và
không thêm hàng nào vào bảng sáu plane. Tiền tố `_` không đụng id pattern, nên
`_drafts` không thể bị nhầm là một node.

### Thứ tự chuỗi, và planning vào một lần

```text
wayfind → grill → spec → planning → run
```

`planning` nhận hiểu biết đã chốt (`story.md`) và cách làm đã chốt
(`approach.md`, `qa.md`), rồi dựng Epic, Story và Ticket **một lượt**. Hai mode
A/B biến mất: còn một kỷ luật cắt, một bảng proposal, một human gate.

### Nhận nuôi: copy, sync, rồi mới xoá

1. Tạo node theo thứ tự phụ thuộc (blocker và parent trước).
2. **Copy** prose từ `works/_drafts/<slug>/` vào `works/<ST-id>/`; cắt
   `ticket.md` cho từng `works/<TK-id>/`.
3. `pulse work sync <id>` từng Ticket.
4. `pulse qa baseline <story-id>` validate `qa.md` — chỉ chạy được ở bước này.
5. **Chỉ sau khi sync thành công** mới xoá `works/_drafts/<slug>/`.

Thứ tự này khiến draft là nguồn sự thật cho tới lúc graph đã bind xong, nên
crash giữa đường thì chạy lại là idempotent. **Không cần lệnh CLI mới và không
cần transaction cho prose** — đây là lý do chọn copy-rồi-xoá thay vì `git mv`.

### Decision frontier vẫn gọi planning riêng

Sương mù đã nêu thành câu hỏi chính xác vẫn thành `decision_work` Ticket qua
`planning`, để `pulse work list --role decision_work` còn là danh sách sương mù
sống (0019 dòng 176–179). Đó là **planning cho một work item khác**, không phải
lần thứ hai trên cùng một Story — khác biệt này có thật và làm luồng đọc được.

## Alternatives Considered

1. **Giữ hai lần planning (nguyên trạng 0019).** Loại: thứ tự đọc ngược với
   thứ tự hiểu, và cùng một kỷ luật cắt phải viết thành hai mode. Giá của nó là
   vĩnh viễn; giá của vùng draft là một lần.
2. **Nhận nuôi bằng `git mv`.** Loại: crash giữa đường để lại prose ở hai chỗ
   với `brief_hash` trỏ vào template. Muốn an toàn thì phải thêm transaction cho
   prose hoặc một lệnh `pulse work adopt`. Copy-rồi-xoá đạt cùng mức an toàn với
   0 dòng code.
3. **Đặt draft ở `.pulse/runtime/`.** Loại: runtime gitignored. `story.md` và
   `approach.md` là prose người đã review, sống qua nhiều phiên; mất khi handoff
   là mất việc thật. Khuym cũng đặt `CONTEXT.md` ở vùng tracked.
4. **Thêm plane thứ bảy hoặc thư mục top-level mới.** Loại: nó không trả lời một
   câu hỏi mới, nên không phải plane mới. Đặt ngoài `works/` là thêm một dòng
   `.gitignore`, một hàng bảng §4 và một bề mặt validation, đổi lấy không gì.
5. **Nới luật một-chủ-sở-hữu-node để `grill` tự tạo Story.** Loại: lý lẽ gốc của
   0019 vẫn đúng — kỷ luật cắt dạy ở ba chỗ sẽ trôi khỏi nhau. Vấn đề ở đây là
   khoá của prose, không phải quyền ghi; sửa đúng chỗ thì không phải trả giá đó.
6. **Giữ thứ tự và chỉ làm rõ bằng chữ.** Loại: nó vá cảm giác chứ không vá
   nguyên nhân, và để lại hai mode trong skill dài nhất của bộ.

## Consequences

- `pulse-planning` ngắn lại: mất Mode A/Mode B, mất phần định mode từ artifact.
- `grill`, `spec`, `research` viết được trước khi work graph có gì — ba skill
  chưa viết đều hưởng, nên đây là lúc rẻ nhất để đổi.
- Thêm một quy ước đường dẫn (`works/_drafts/<slug>/`) và một bước nhận nuôi
  trong planning. Bước này là mã prose, không phải mã Rust.
- `pulse qa baseline` chuyển sang chạy **sau** nhận nuôi. `spec` phải biết nó
  không validate được baseline tại chỗ.
- Draft bị bỏ sẽ tích lại. Chưa có ai dọn; xem Chưa chốt.
- Chuỗi khớp cả `to-tickets` của Matt và `exploring → planning` của Khuym, nên
  hai reference còn dùng được làm nguồn thay vì phải dịch ngược thứ tự.

## Verification

- Guard: `works/_drafts/` không xuất hiện trong `content_dir` của bất kỳ node;
  `graph validate` vẫn xanh khi `works/_drafts/<slug>/` tồn tại.
- Test: tạo `works/_drafts/<slug>/` rồi chạy `pulse graph validate` và
  `pulse work list` — không lệnh nào coi nó là node hay báo lỗi.
- Test nhận nuôi: copy → `work sync` → xoá draft. Cắt ở giữa sau khi tạo node
  nhưng trước `work sync`, chạy lại phải thành công, không nhân đôi node.
- `pulse qa baseline <story>` fail rõ ràng khi `qa.md` còn ở vùng draft, và xanh
  sau nhận nuôi.
- Guard chủ sở hữu node vẫn đỏ khi skill khác `pulse-planning` gọi lệnh tạo node.
- `skills/pulse-planning/SKILL.md` không còn chữ Mode A / Mode B.

## Required changes

- `PRODUCT.md` §4: thêm `works/_drafts/<slug>/` vào layout, nói rõ nó cùng plane
  work prose. §5.8: sửa bảng skill theo thứ tự mới.
- `docs/decisions/0019`: Status ghi "Amended by 0021" — thứ tự chuỗi và planning
  một lần.
- `docs/plans/0019-guidance-layers.md`: giai đoạn 4 đổi thứ tự viết skill —
  `grill` trước `planning`, vì planning giờ phụ thuộc hợp đồng draft của grill.
- `skills/pulse-planning/`: bỏ Mode A/B, thêm bước nhận nuôi, viết lại
  Establish Authority theo một input duy nhất. Chạy lại eval.
- `skills/pulse-wayfind/`: bàn giao sang `grill` thay vì `planning`.
- Guard/test theo mục Verification.

## Chưa chốt, để lại cho lúc implement

1. Ai dọn `works/_drafts/<slug>/` bị bỏ, và nó có phải là "việc đang mở" hợp lệ
   để `pulse work list` biết đến hay không.
2. `<feature-slug>` sinh từ đâu — người đặt, hay suy từ tiêu đề Story đề xuất.
   Cần ổn định giữa grill và spec trong hai phiên khác nhau.
3. Một Story hay nhiều Story cho mỗi slug. Nếu planning cắt một draft thành hai
   Story thì prose phải chia, và luật chia chưa có.
