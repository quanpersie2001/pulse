# Decision 0019: Ba tầng hướng dẫn, và đúng một skill sở hữu việc tạo node

## Status

Accepted, 2026-09-13. Guidance layers narrowed by
[Decision 0020](0020-collapse-guidance-into-agents.md): the AGENTS block now
owns the common/R0 workflow and `docs/pulse-workflow.md` is removed. Skill order
amended by [Decision 0021](0021-prose-before-graph-and-one-planning-entry.md):
the chain is `wayfind → grill → spec → planning`, `pulse-planning` is entered
once per delivery chain instead of twice at two altitudes, and `grill`/`spec`
write prose to `works/_drafts/<slug>/` before any node exists. The
single-node-owner and product-contract decisions below remain current.

**Thu hẹp [Decision 0009](0009-skill-surface-over-cli.md)**: khối `AGENTS.md`
không còn vừa route vừa dạy; `pulse-wayfind` không còn tạo node; `pulse-tickets`
đổi tên và mở rộng thành `pulse-planning`. **Sửa
[Decision 0013](0013-session-handoff-host-threshold.md)** §2: bước flush của
`pulse-handoff` không còn `work create` và `edge add`. **Sửa `PRODUCT.md` §5.6**:
intervention loại implementation work là *đề xuất* Ticket, không phải tạo.

Bổ sung `PRODUCT.md` §5.4 (`docs/product/` có hợp đồng), §5.8 (ba tầng, bảng
skill).

## Context

### Khối AGENTS đang làm hai việc

`pulse init` ghi 65 dòng vào `AGENTS.md`, vừa là bảng route vừa là toàn bộ
hướng dẫn. Không có tầng nào ở giữa "bản đồ" và "skill".

`references/repository-harness` chia ba tầng và nói thẳng lý do (`docs/HARNESS.md`
nguyên tắc 2): **"`AGENTS.md` is an entrypoint, not an encyclopedia."**

| | `AGENTS.md` | `docs/WORKFLOW.md` | `.agents/skills/` |
|---|---|---|---|
| dòng | 31 | 128 | 67–506 |
| việc | route + authority | từng bước | nghi lễ gọi tường minh |

Điểm quan trọng: **luồng thường ngày của nó không nằm trong skill.** Agent làm
việc bounded đi theo `WORKFLOW.md` mà không gọi skill nào. Skill là ngoại lệ.

Pulse thiếu đúng tầng giữa đó. Hệ quả: agent làm việc R0 — phần lớn công việc —
chỉ có một bảng route và không có gì khác.

### Nhưng Pulse có gate cơ học, nên tầng giữa phải mỏng hơn

`WORKFLOW.md` của repository-harness phải dặn bằng lời: *"claim completion only
with executable evidence"*, *"stop when authority is ambiguous"*, *"identify
authority before editing"*.

Pulse **ép** cả ba: close gate đòi receipt, ready gate chặn open question
`blocking`, `authority.json` default-deny. Chép lại chúng vào prose là tạo nguồn
sự thật thứ hai cho luật mà CLI đã sở hữu — dấu hiệu §12 *"Hai hệ thống cùng sửa
status mà không có field ownership"*.

### Bắt đầu một dự án mới không có đường đi

`wayfind` của 0009 kết thúc ở *"Epic với `brief.md` là map"* rồi bàn giao sang
`grill` với **một Story**. Đúng cho một sáng kiến trong sản phẩm đã có; sai cho
một sản phẩm mới, nơi kết quả điều tra phải là **nhiều Epic**.

Và `docs/product/` có trong taxonomy §5.4 nhưng **không skill nào ghi vào, không
ai nói nó phải chứa gì**.

`references/better-harness/references/bootstrap/spec-structure.md` cho hợp đồng
đó: bảy mục, ba hệ ID (`BR-*` business rule, `E-*` exception, `AC-*` acceptance),
và luật truy vết *"Every `AC-*` references the `BR-*` or `E-*` ids it exercises.
Any rule or exception with no referencing criterion is unverified and blocks the
gate."*

Ánh xạ sang Pulse gần như một-một, và **ba tầng dưới đã chạy thật**:

```
BR-*  business rule    ←→  docs/product/                    CHƯA CÓ
E-*   exception        ←→  docs/product/                    CHƯA CÓ
AC-*  acceptance       ←→  ## Acceptance trong ticket.md    đã có
AC → BR/E truy vết     ←→  acceptance_proofs                đã có
coverage gate          ←→  close gate                       đã có
decision gate          ←→  ready gate, Open questions       đã có
```

Sợi chỉ đứt đúng ở khúc trên: `AC-1` hôm nay là câu tự do, không trích dẫn gì.

### Ba skill cùng tạo node

0009 cho `tickets` tạo Ticket. 0013 §2 cho `handoff` gọi `work create` và
`edge add` trong bước flush. `PRODUCT.md` §5.6 cho ratchet biến intervention
thành Ticket.

Ba chỗ cùng tạo node nghĩa là **kỷ luật cắt phải dạy ở ba chỗ**, và sẽ trôi khỏi
nhau. Với `handoff` còn tệ hơn: context sắp đầy là lúc agent **ít khả năng nhất**
để ra một quyết định cắt, mà đó lại đúng là lúc 0013 bảo nó tạo node.

## Decision

### Ba tầng

1. **Khối `AGENTS.md` (~20 dòng, Pulse sở hữu).** Pulse là gì, luật authority khi
   mâu thuẫn, bảng route theo hình dạng yêu cầu, và "trước khi mutation, đọc
   `docs/pulse-workflow.md`". Không hơn.

2. **`docs/pulse-workflow.md` (~70 dòng, Pulse sở hữu, marker + drift detect).**
   Dùng lại nguyên cơ chế `kernel/guidance.rs` đã có cho khối AGENTS: marker,
   version, hash của thân, sửa tay thì báo `guidance_conflicts` chứ không ghi đè.
   **Không đăng ký vào docs registry** — registry giữ truth của repo đích,
   workflow là truth của harness; cùng lý do [0018](0018-knowledge-plane-retrieval-and-lifecycle-exits.md)
   §3 từ chối trộn learning với docs.

   Bốn phần: map sáu plane; **bốn câu hỏi chẩn đoán**; luồng R0 đầy đủ;
   completion standard. R1–R3 chỉ trỏ sang skill.

3. **Skill.** Khuôn chung: frontmatter có **nửa "không dùng khi nào"** → Establish
   Authority → các bước đánh số → Report.

### Route bằng câu hỏi, không bằng bảng tra

Bảng tra buộc agent nhận đúng ô trước khi dùng được; câu hỏi buộc nó trả lời về
việc trước mắt, nên chịu được ca lạ. Bốn câu:

1. Có mutation không? Không → đọc, trả lời có dẫn chứng, dừng.
2. Hình dạng nào? R0 thẳng; R1–R3 sang skill; chưa thấy đường thì `wayfind`.
3. Gate sẽ đòi bằng chứng gì? Đọc từ risk và posture, **không đoán**.
4. Còn mơ hồ gì đổi được acceptance/invariant/public contract? → dừng trước
   mutation.

### Luật chi phối prose

> **Prose nói gate sẽ hỏi gì và làm sao thoả. Prose không bao giờ chép lại luật
> mà gate đã ép.**

### `docs/product/` có hợp đồng

`wayfind` ghi và `pulse docs register` đăng ký. Bốn mục:

```markdown
## Requirement Overview    bối cảnh, in/out of scope, glossary
## Business Rules          BR-01… mỗi rule kiểm được, resolve về ĐÚNG MỘT chỗ
## Exception Scenarios     E-01… trigger + message người dùng thấy + error code
## Open Questions          còn mở, có owner; và decision record (đã chốt, đã loại)
```

ID **ổn định và append-only**. Rule chết thì đánh dấu withdrawn và **giữ số** —
commit, test, review comment đã trích số cũ.

`## Acceptance` trong `ticket.md` **trích ngược** `BR-*`/`E-*`, đúng khuôn
`## QA impact` đã trích case ID từ `qa.md`. Cơ chế y hệt, plane khác.

**Gắn vào materialization, không gắn toàn cục:** R0/R1 không đòi trích; R2/R3
mới đòi. Sửa lỗi chính tả không cần business rule. Nguyên tắc 7.

### Đúng một skill tạo node

> **Quyết định "cần có node nào, hình dạng ra sao" thuộc về `pulse-planning`.
> Skill khác chỉ `transition` trạng thái mà nó sở hữu gate.**

`transition` không phải tạo node: `grill` đưa Story sang `shaped`, `planning` đưa
Ticket sang `ready` — mỗi skill chuyển đúng cái gate nó gác.

| Skill | Tạo node | Transition | Ra gì |
|---|---|---|---|
| `wayfind` | ❌ | ❌ | `docs/product/` |
| `grill` | ❌ | Story → `shaped` | `story.md`, glossary, Decision |
| **`planning`** | ✅ **độc quyền** | Ticket → `ready` | Epic, Story, Ticket, `decision_work`, `blocked_by` |
| `spec` | ❌ | ❌ | `approach.md`, `qa.md` |
| `research` | ❌ | ❌ | `works/<id>/research/<topic>.md` |
| `ratchet` | ❌ đề xuất | ❌ | learning, intervention |
| `onboard` | ❌ | ❌ | `init`, `docs register`, backup |
| `handoff` | ❌ | ❌ | live thread doc, note con trỏ |

`planning` được gọi **hai lần ở hai tầm** — sau `wayfind` ra Epic/Story, sau
`spec` ra Ticket. Cùng một kỷ luật cắt, viết **một lần**.

> **Sửa bởi [0021](0021-prose-before-graph-and-one-planning-entry.md):** không
> còn hai lần. `grill` và `spec` ghi prose vào `works/_drafts/<slug>/` trước khi
> có node, nên `planning` vào một lần ở cuối chuỗi và dựng Epic, Story, Ticket
> một lượt. Bảng trên cũng đổi theo: `grill` ra
> `works/_drafts/<slug>/story.md`, `spec` ra `approach.md`/`qa.md` cùng chỗ, và
> `planning` thêm việc nhận nuôi prose vào `works/<id>/`.

### `wayfind` không tạo Epic

Epic là *"outcome lớn và ranh giới đầu tư"* (§5.1). Một cuộc điều tra không phải
outcome của sản phẩm. Bản nháp trước của quyết định này có "Epic khám phá" được
tạo rồi đóng lại trước khi tạo Epic xây — nghĩa là `Epic` mang **hai nghĩa** tuỳ
lúc, trái nguyên tắc 4.

Nó cũng không cần thiết: §5.1 nói *"Hierarchy là tuỳ chọn. Ticket độc lập hợp
lệ"*, nên `decision_work` Ticket không cần cha.

Sương mù ở hai tầng: câu hỏi **nêu được chính xác** → `decision_work` Ticket do
`planning` tạo, và tập ticket đang mở **chính là** danh sách sương mù
(`pulse work list --role decision_work --status ready`); sương mù **chưa nêu
thành câu hỏi được** → `## Open Questions` của `docs/product/`.

**Epic chỉ có một nghĩa: một lát của sản phẩm đã chốt.**

Mất `pulse work rollup` cho cuộc điều tra (rollup cần node cha). Đổi bằng
`work list --role decision_work`. Điều tra lớn tới mức thật sự cần rollup thì tạo
Epic — quyết định theo tình huống, không phải bước mặc định.

### Invocation theo tiêu chí, không theo bảng copy

Tiêu chí: **skill này có làm việc mà người dùng bắt buộc phải yêu cầu trước
không?**

- **Model gọi được:** `wayfind`, `grill`, `planning`, `spec`, `research`,
  `ratchet`. Chúng là luồng chính, không phải ngoại lệ.
- **Hook của host:** `handoff` — `disable-model-invocation: true` **và** Stop hook
  ép (0013 §3). Model tự chọn thời điểm sẽ gọi sai lúc, hoặc gọi để né việc khó.
- **Chỉ người:** `onboard`. Lý do của 0026 repository-harness: *"installation
  cannot determine whether a repository is ready for inspection, which
  operational path the user wants mapped."*

**Invocation và authority là hai trục độc lập.** Agent tự vào `grill`, nhưng
không tự vượt gate `shaped`. Agent chạy worker, nhưng không tự khai `done`.

Policy nằm trong frontmatter của `SKILL.md`, **không có manifest sidecar**.

## Alternatives Considered

1. **Giữ tất cả trong khối AGENTS, viết dài hơn.** Loại: vi phạm progressive
   disclosure (nguyên tắc 6). Khối được nạp mọi phiên; chi tiết chỉ cần khi sắp
   mutation.
2. **Không có tầng giữa, dồn vào skill.** Loại: việc R0 không gọi skill nào, nên
   agent làm R0 chỉ còn bảng route. Phần chung (authority, completion standard,
   khi nào dừng) cũng bị lặp tám lần rồi trôi.
3. **Đặt tầng giữa vào `PULSE.md`.** Loại: `PULSE.md` là authority của repo đích
   (§5.4). Nội dung Pulse sở hữu không sống được ở file repo sở hữu.
4. **Đăng ký `docs/pulse-workflow.md` vào docs registry.** Loại: registry giữ
   truth của repo đích. Cùng lý do 0018 §3.
5. **Tách `plan` và `tickets` thành hai skill.** Loại: cùng phương pháp (đề xuất
   breakdown → duyệt → tạo node → wire `blocked_by`), chỉ khác tầm. Hai file dạy
   cùng một kỷ luật là cách nó trôi khỏi nhau.
6. **Gộp bước chia Epic/Story vào `wayfind`.** Loại: phá luật một chủ sở hữu
   node, và `wayfind` thành ba việc lớn trong một skill.
7. **`handoff` vẫn tạo node khi flush (giữ 0013 §2).** Loại: context sắp đầy là
   lúc tệ nhất để phán đoán cắt. Ghi vào live thread doc, phiên sau `planning`
   quyết.
8. **Copy `allow_implicit_invocation: false` của repository-harness cho mọi
   skill.** Loại: đó là copy **giá trị** thay vì **tiêu chí**. Ở nó luồng chính
   nằm trong `WORKFLOW.md` và skill là ngoại lệ; ở Pulse skill **là** luồng
   chính.
9. **Mở luồng riêng "từ số không tới product spec".** Loại: `wayfind` +
   `docs/product/` đã phủ. Thêm skill thứ chín cho cùng một việc là nghi lễ.

## Consequences

- Agent làm việc R0 có hướng dẫn thật, không chỉ một bảng route.
- Khối `AGENTS.md` nạp mọi phiên co từ 65 xuống ~20 dòng.
- Sợi chỉ ID nối liền: `BR-*` → Story → `QA-*` → `AC-*` → check → receipt. Lần
  đầu acceptance của Ticket trích ngược lên một rule có số.
- Kỷ luật cắt sống ở đúng một file.
- Thêm một file Pulse ghi vào repo đích (`docs/pulse-workflow.md`) — thêm một bề
  mặt refresh/drift, nhưng dùng lại cơ chế đã có.
- **Ba văn bản đã accepted bị sửa** (0009, 0013, PRODUCT §5.6). Không phải diễn
  giải lại.
- R0/R1 không chịu thêm nghi lễ nào: hợp đồng ID gắn vào materialization.

## Kiểm chứng

- Guard parse lệnh (đã có) phủ `docs/pulse-workflow.md` ngoài `assets/agents-block.md`
  và `skills/**`.
- Guard mới: `skills/**` không chứa `pulse work create` hay `pulse graph edge add`
  ngoài `skills/pulse-planning/`.
- `pulse init` ghi cả hai file; `--refresh` render lại cả hai; sửa tay trong
  marker của file nào thì file đó vào `guidance_conflicts` và không bị ghi đè.
- `pulse init` seed và đăng ký `DOC-GLOSSARY` (đã có).

## Thay đổi

- `src/kernel/guidance.rs`: quản N file có marker thay vì hard-code hai.
- `assets/agents-block.md` co lại; `assets/pulse-workflow.md` mới.
- `src/kernel/init.rs`: ghi file thứ hai.
- `tests/graph/architecture_guards.rs`: mở rộng guard parse; thêm guard chủ sở
  hữu node.
- `skills/pulse-{wayfind,grill,planning,spec,research,ratchet,onboard,handoff}/`.
- `docs/decisions/0009`: ghi "Narrowed by 0019".
- `docs/decisions/0013`: ghi "Amended by 0019"; sửa §2 bước 1.
- `PRODUCT.md` §5.4, §5.6, §5.8.
- Plan: [`docs/plans/0019-guidance-layers.md`](../plans/0019-guidance-layers.md).
