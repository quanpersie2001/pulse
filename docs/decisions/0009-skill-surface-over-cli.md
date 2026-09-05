# Decision 0009: Workflow trong repo đích, skill chỉ cho việc cần manual

## Status

Proposed, 2026-09-06. Thu hẹp Decision 0007: trả lại lớp hướng dẫn, giữ
nguyên nguyên tắc "không prose nào sở hữu lifecycle".

## Context

Sau golden path (TK-001, TK-002 đóng thật), lớp hướng dẫn hiện tại chỉ phủ hai
bước cuối qua bootstrap prompt của `pulse run`. Từ intent đến Ticket `ready`
không có gì dẫn agent; developer làm tay. Nguyên tắc 8 và 12 của PRODUCT.md
(khử mơ hồ trước khi chạy, failure nuôi ratchet) không có bề mặt thực thi.

Hai nguồn đã đọc:

**repository-harness** (lấy làm khung chính):

- Quy trình sống trong repo đích: khối `AGENTS.md` có marker
  `<!-- HARNESS:BEGIN/END -->` do installer quản lý và merge ba chiều khi
  update, cộng `docs/WORKFLOW.md`. Không có skill "using".
- Route theo hình dạng yêu cầu: read-only → bounded change → durable planned
  change → material ambiguity dừng trước mutation. Không chuỗi cố định.
- Authority gate: mọi claim phân loại `Authoritative / Observed / Derived /
  Decision required / Unknown`; convention, test, default cấu hình không phải
  authority.
- Proof theo hành vi; checklist và message không phải proof.
- Skill chỉ cho việc explicit và hiếm: `onboard-repository` (read-only pass
  trước, propose rồi mới sửa), `improve-harness` (baseline → earliest gap →
  một intervention → fresh rerun → keep/revise/remove), `encode-invariant`
  (authority → guard nhỏ nhất ở validation owner sẵn có → positive và negative
  proof → báo bốn mức enforcement riêng). Ba đến năm skill, không hơn.
- Trong việc thường, agent **báo cáo** ma sát; chỉ **sửa** harness khi được
  gọi rõ, và chỉ được claim cải thiện sau fresh rerun.
- Không control-plane song song: không state.json, không HANDOFF.json.

**Matt Pocock skills** (`references/mattpocock`, lấy chuỗi và primitive):

- Chuỗi chính `grill-with-docs → to-spec → to-tickets → implement →
  code-review`, mỗi skill kết thúc ở một artifact, skill sau không phỏng vấn
  lại.
- `wayfinder` là on-ramp cho việc lớn còn mù mờ: đích đến trước, map là index
  của decision ticket (grilling, prototype, research, task), một ticket một
  phiên, fog of war, bàn giao sang to-spec chứ không tự build.
- `grilling` là primitive: một câu một lần, fact tự tra, decision hỏi người,
  không hành động cho đến khi người xác nhận. Glossary ghi ngay; ADR chỉ khi
  khó đảo ngược, khó hiểu nếu thiếu context, có trade-off thật.
- `research` là subagent nền, nguồn sơ cấp, để lại file có trích dẫn.
- Không lấy: issue tracker ngoài, `CONTEXT.md` ở root, `.scratch/`.

**Khuym** (lấy vài kỹ thuật, không lấy khung):

- Hỏi một câu một lần, kèm recommended answer, khoá quyết định có ID ổn định.
- Validating là gate cứng: giả định chưa có bằng chứng phải có spike trước
  khi chạy.
- Gate human gắn vào artifact, không gắn vào bước.
- SKILL.md ngắn, chi tiết trong `references/` nạp khi cần.
- Không lấy: `CONTEXT.md`, `state.json`, `HANDOFF.json`, go mode, beads.

Pulse đã có sẵn thứ repository-harness thiếu: graph có lifecycle, receipt, close
gate, knowledge store. Vì vậy phần "durable plan" của repository-harness ánh xạ
thẳng vào Story/Ticket, phần "report friction" ánh xạ vào knowledge candidate,
phần "encode invariant" ánh xạ vào `runners.json` role `check`.

## Decision

1. **Quy trình sống trong repo đích, do `pulse init` ghi và cập nhật.**
   `pulse init` ghi khối `<!-- PULSE:BEGIN --> … <!-- PULSE:END -->` vào
   `AGENTS.md` và tạo `PULSE.md`. Khối này route theo hình dạng yêu cầu và
   trỏ lệnh `pulse` cụ thể. `pulse init --refresh` render lại khối theo
   version CLI, giữ nguyên nội dung ngoài marker; lệch thì báo, không ghi đè.
2. **Skill theo artifact, mỗi skill kết thúc ở một trạng thái graph.** Chuỗi
   chính `pulse-grill → pulse-spec → pulse-tickets`, on-ramp `pulse-wayfind`
   cho việc lớn còn mù mờ, primitive `pulse-research`, và hai skill explicit
   `pulse-ratchet`, `pulse-onboard`. Không có `using`, không router: khối
   AGENTS.md route theo hình dạng yêu cầu.
3. **Executing và reviewing không phải skill.** Bootstrap prompt trong
   `src/kernel/run.rs` là contract máy, đã đủ. Khối `AGENTS.md` mô tả cùng
   contract cho agent tương tác.
4. **Capture ma sát là tự động, sửa harness là có bằng chứng.** Worker và
   reviewer bắt buộc ghi ma sát qua `pulse note --kind friction` hoặc
   `--friction` khi handoff. Close gate tự chuyển note friction thành learning
   `candidate` scope `harness`. `pulse-ratchet` được phép sửa `AGENTS.md`,
   `PULSE.md`, `runners.json` role `check` ngay khi có candidate, không hỏi,
   nhưng learning chỉ lên `validated` khi handoff của Ticket sau ghi
   `knowledge_usage: helpful` (fresh rerun). Không rerun thì vẫn `candidate`.
5. **Skill là hướng dẫn, CLI là authority.** Mọi mutation trong skill là lệnh
   `pulse` nguyên văn. Guard test: mọi lệnh `pulse …` trong `skills/**` và
   trong template khối AGENTS phải parse được bằng clap của crate.

## Flow theo hình dạng yêu cầu

Khối `AGENTS.md` trong repo đích nói đúng những dòng sau, không hơn:

```text
Yêu cầu chỉ đọc (giải thích, review, chẩn đoán, trạng thái)
  -> pulse work list/show/packet, pulse docs search/get, pulse events tail
  -> trả lời có dẫn chứng, không mutation

Thay đổi nhỏ, hướng rõ (R0)
  -> pulse work create --kind ticket --risk low, điền Objective/Acceptance/
     Anchors/Verify, pulse work ready
  -> pulse run worker, pulse run reviewer, pulse work close

Thay đổi nhiều phiên, nhiều Ticket, hoặc risk >= medium (R1–R3)
  -> pulse-grill (Story shaped) -> pulse-spec (approach.md, qa.md)
     -> pulse-tickets (Ticket ready, blocked_by)
  -> pulse run worker | reviewer | qa, docs validate --record, work close,
     work close-story

Việc lớn hơn một phiên, đường đi chưa thấy
  -> pulse-wayfind: Epic + decision_work Ticket + Decision, rồi mới grill

Mơ hồ về sản phẩm còn mở (objective, acceptance, invariant, public contract)
  -> dừng trước mutation; ghi câu hỏi vào ## Open questions với (blocking)
     hoặc tạo Decision; hỏi human một câu, kèm recommended answer

Quy tắc kiến trúc / bảo mật / chất lượng cần chặn tái phạm
  -> chỉ khi có authority trong docs approved hoặc Decision accepted
  -> guard nhỏ nhất trong validation owner sẵn có, thêm role check vào
     runners.json, positive + negative proof, báo mức enforcement

Ma sát với harness trong lúc làm
  -> pulse note --kind friction (luôn), không tự sửa harness trong Ticket
  -> pulse-ratchet sửa sau khi Ticket đóng, chờ rerun để validated

Hoàn thành
  -> chỉ receipt: handoff, verification, qa_checkpoint, docs_validation, close
```

Authority khi mâu thuẫn: Decision accepted và `docs/product` approved là
intent; code và test là implementation; receipt là observation; docs khác là
explanation. Mâu thuẫn ảnh hưởng acceptance thì gate fail với `docs_conflict`,
human quyết.

## Skill theo artifact

Lấy khung của Matt Pocock (`references/mattpocock`): mỗi skill kết thúc ở một
artifact và một trạng thái graph, không skill nào làm việc của skill kế tiếp.
Chuỗi chính và on-ramp:

```text
pulse-wayfind (on-ramp, việc lớn còn mù mờ)  ──► Epic + decision_work + Decision
        │
        ▼
pulse-grill  ──► Story shaped, glossary, Decision khi khó đảo ngược
pulse-spec   ──► story.md, approach.md, qa.md   (không phỏng vấn lại)
pulse-tickets──► Ticket ready, blocked_by
pulse run worker | reviewer | qa  (không phải skill)
pulse-ratchet (sau close)
pulse-onboard (brownfield, explicit)
pulse-research (primitive, model-invoked, subagent nền)
```

Ánh xạ khái niệm của Matt sang vật liệu Pulse đã có:

| Matt Pocock | Pulse |
|---|---|
| `wayfinder:map` issue, Decisions so far | Epic node, `works/EP-*/brief.md` là index |
| decision ticket grilling / prototype / task | Ticket `role: decision_work` dưới Epic |
| decision ticket research | `decision_work` + `works/<id>/research/<topic>.md` |
| answer khi đóng ticket | Decision node + receipt `decision_acceptance` |
| frontier, native blocking | `blocked_by` edge + `pulse work ready` |
| Not yet specified, Out of scope | hai mục trong `brief.md` |
| glossary `CONTEXT.md` | doc kind `domain`: `docs/domain/glossary.md` trong registry |
| ADR | Decision node |
| spec: user stories, seams, testing decisions | `story.md`, `approach.md`, `qa.md` |
| tracer-bullet ticket + blocked_by | Ticket `implementation`, `ticket.md`, edge |
| implement, code-review | `pulse run worker`, `pulse run reviewer` |

### `pulse-wayfind` (user-invoked)

Chỉ cho việc lớn hơn một phiên và đường đi chưa thấy. Lập kế hoạch, không
làm.

1. Đặt tên đích đến bằng một vòng grill ngắn. `pulse work create --kind epic`,
   `brief.md` có `## Destination`, `## Notes`, `## Decisions so far`,
   `## Not yet specified`, `## Out of scope`.
2. Grill theo chiều rộng để tìm quyết định còn mở. Không có sương mù thì dừng,
   chuyển sang `pulse-grill` với một Story.
3. Mỗi câu hỏi nêu được chính xác thành một Ticket `--role decision_work`
   dưới Epic, `## Question` và loại: grilling, prototype, research, task.
   Wire `blocked_by` ở lượt hai. Phần chưa nêu được ở lại `Not yet specified`.
4. Research ticket chạy ngay bằng `pulse-research` subagent, ghi
   `works/<id>/research/<topic>.md`, không chờ.
5. Mỗi phiên sau giải một ticket: `pulse work ready` cho frontier, resolve
   bằng grill hoặc prototype, câu trả lời thành Decision node
   (`work create --kind decision`, receipt `decision_acceptance` do human),
   đóng ticket, thêm một dòng vào `Decisions so far`, graduate fog thành
   ticket mới, ruled-out thì đóng và ghi `Out of scope`.
6. Map xong khi không còn `decision_work` mở. Bàn giao sang `pulse-grill`
   hoặc thẳng `pulse-spec` nếu Story đã rõ. Không build từ map.

### `pulse-grill` (user-invoked, có thể implicit khi mơ hồ)

Primitive grilling của Matt giữ nguyên: một câu một lần, kèm câu trả lời gợi
ý, fact tự tra bằng `pulse docs search/get` và code, decision hỏi người, không
hành động cho đến khi người xác nhận. Thêm giấy tờ:

- Thuật ngữ chốt xong ghi ngay vào `docs/domain/glossary.md` (đăng ký
  `DOC-GLOSSARY` kind domain nếu chưa có). Glossary chỉ là từ vựng.
- Quyết định đủ ba điều kiện (khó đảo ngược, khó hiểu nếu thiếu context, có
  trade-off thật) thành Decision node. Còn lại ghi `(resolved)` trong
  `## Open questions` của `story.md`.
- Kết thúc: `pulse work create --kind story` nếu chưa có, `story.md` có
  Outcome, Success signals, Scope boundary, Open questions;
  `pulse work transition --to shaped`. Đây là gate human thứ nhất.

### `pulse-spec` (user-invoked)

Không phỏng vấn lại. Tổng hợp từ cuộc trò chuyện, glossary, Decision, research:

- `approach.md`: solution, implementation decisions, seam để test (ưu tiên seam
  có sẵn, cao nhất có thể, càng ít càng tốt), testing decisions, out of scope.
  Hỏi người đúng một lần về seam.
- `qa.md`: user stories thành QA case trong block `pulse-qa`; `pulse qa
  baseline` validate.
- `pulse docs impact` để biết doc nào phải đổi; ghi vào `approach.md`.
- Nếu giữa chừng thiếu fact ngoài repo, gọi `pulse-research`.

### `pulse-tickets` (user-invoked)

Cắt `approach.md` thành Ticket tracer bullet: lát dọc xuyên mọi tầng, demo
được độc lập, vừa một context window, prefactoring trước. Wide refactor dùng
expand–contract. Trình bày breakdown (title, blocked by, delivers) và hỏi
người về độ mịn và edge trước khi tạo. Rồi theo thứ tự blocker trước:

```text
pulse work create --kind ticket --risk <r> --parent <ST>
pulse graph edge add --type blocked_by --from <TK> --to <TK>
<điền ticket.md: Acceptance có ID, Code anchors, Verify, Open questions
 disposition, Documentation impact, QA impact với case ID từ qa.md>
pulse work sync <TK>
pulse work ready <TK>
```

Ready gate là gate human thứ hai. R0 không cần ba skill trên: khối AGENTS.md
dẫn thẳng đến `work create` và `work ready`.

### `pulse-research` (model-invoked, primitive)

Subagent nền, chỉ nguồn sơ cấp, một file `works/<id>/research/<topic>.md`
có trích dẫn và ngày. Packet liệt kê file này của Story hoặc Epic cha dạng ref.
Được `pulse-wayfind`, `pulse-grill`, `pulse-spec` gọi; không tự tạo node.

### `pulse-ratchet` (explicit, sau close)

Gộp compounding của Khuym với improve-harness và encode-invariant của
repository-harness, vì cả ba đều là "failure → owner → intervention → proof":

1. Đọc chuỗi bằng chứng của Ticket vừa đóng: handoff, verification, findings,
   note friction, `events tail --ticket`.
2. `pulse knowledge capture --from <ticket>` cho mỗi bài học; scope
   `repository` (về codebase) hoặc `harness` (về cách dùng Pulse). Không
   reusable thì `non_durable`, không tạo record.
3. Tìm earliest gap theo phân loại của repository-harness: context,
   capability, ownership, authority, proof, environment.
4. Một intervention, tại owner đúng, không hỏi: `knowledge promote
   --document`, `--agents-md`, `--decision`, hoặc role `check` mới trong
   `runners.json` khi bài học là invariant cơ học có authority. Ghi giả thuyết
   trước khi sửa.
5. Fresh rerun là Ticket kế tiếp chạm cùng path. Handoff ghi
   `knowledge_usage`. `helpful` thì validated; `misleading` hai lần thì retire
   và gỡ. Không rerun thì giữ `candidate`, không claim cải thiện.

### `pulse-onboard` (explicit, brownfield)

Hai pass của repository-harness: pass một read-only, baseline Git và ignored
state, phân loại authority từng claim, so docs với check hiện có, đề xuất theo
thứ tự sửa instruction sai trước rồi mới thêm; pass hai sau approve chạy
`pulse init`, `docs register`, `docs tags add`, backup docs vào
`.pulse/migrations/docs-backups/` trước khi restructure.

### Gate human

Gắn vào trạng thái, không gắn vào skill: Story `shaped` (sau grill), Decision
`accepted` (wayfind hoặc grill), seam trong `approach.md` (spec hỏi một lần),
breakdown trước khi tạo Ticket (tickets), Ticket `ready`, close receipt. Giữ
grill → spec → tickets trong một context window; đầy thì `pulse note` để bàn
giao, không compact giữa chừng.

## Thay đổi cần làm

CLI và kernel:

- `pulse init` render khối `AGENTS.md` có marker và `PULSE.md`; `--refresh`
  re-render, phát hiện sửa tay trong marker và từ chối ghi đè.
- `pulse note --kind friction`; `work handoff --friction "<text>"`; close gate
  chuyển friction note thành learning `candidate` scope `harness` (cần scope
  learning từ HANDOFF Bước 6.7).
- `work handoff --learning-used <id>=helpful|not_needed|misleading` và
  `knowledge validate` tự chạy khi đủ `helpful` (Bước 6.7 đã lên kế hoạch).
- Packet liệt kê `works/<ST>/research/*.md` của Story cha dưới `parents[]`
  dạng ref có hash.
- Template `decision_work` thêm `## Output` trỏ `research/<topic>.md` hoặc
  `DEC-*`.
- Bootstrap prompt worker và reviewer thêm bước "ghi friction" bắt buộc.

Repo Pulse:

- `skills/pulse-{wayfind,grill,spec,tickets,research,ratchet,onboard}/`,
  mỗi cái SKILL.md ngắn và `references/`; `pulse-research` model-invoked,
  `pulse-grill` model-invoked khi phát hiện mơ hồ, còn lại user-invoked.
- `docs/domain/glossary.md` là đích glossary; `pulse init` đăng ký
  `DOC-GLOSSARY` kind domain rỗng để grill có chỗ ghi.
- Template `brief.md` của Epic có năm mục của map.
- `assets/agents-block.md` là template khối AGENTS, cùng file dùng cho
  `pulse init` và test.
- `.claude-plugin/plugin.json` và `.codex-plugin/` khai ba skill.
- `tests/graph/architecture_guards.rs`: bỏ guard cấm `skills/`; thêm guard
  parse lệnh `pulse` trong `skills/**` và `assets/agents-block.md`; thêm guard
  `skills/**` không chứa đường dẫn `.pulse/` ngoài `runtime/run`.
- `examples/todolist/AGENTS.md` nhận khối mới qua `pulse init --refresh`.
- PRODUCT.md §5.8 thêm "Bề mặt hướng dẫn"; §14 ghi hai nguồn; Decision 0007
  ghi "Narrowed by 0009".

Thứ tự: Bước 6 HANDOFF.md trước (scope learning, promote, reviewer
classification). Rồi khối `AGENTS.md`, `pulse-grill`, `pulse-spec`,
`pulse-tickets`, dogfood một Story hai Ticket trên todolist bằng agent tương
tác không gõ lệnh tay. Rồi `pulse-ratchet` với friction tự động, kiểm chứng
bằng Ticket kế tiếp. `pulse-wayfind` và `pulse-research` khi có một Epic thật.
`pulse-onboard` sau cùng, thử trên một repo thật ngoài todolist.

## Consequences

- Agent tương tác có đường đi từ intent đến `ready` mà không cần developer
  thuộc CLI; R0 vẫn rẻ vì không qua skill.
- Harness tự ghi ma sát mỗi lần chạy; sửa harness không cần hỏi nhưng chỉ được
  tính là cải thiện khi có rerun, nên không phình ceremony.
- Ba skill thay vì router: trigger rõ, không nạp bảng lệnh cho mọi việc.
- Khối AGENTS có marker tạo thêm một thứ để giữ đồng bộ với CLI; guard parse
  lệnh chặn phần cú pháp, dogfood chặn phần ngữ nghĩa.
