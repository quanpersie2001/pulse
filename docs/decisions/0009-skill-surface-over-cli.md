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
2. **Ba skill, explicit-only trừ một.** `pulse-shape` (từ intent đến Ticket
   `ready`, có thể được gọi ngầm khi yêu cầu có mơ hồ), `pulse-ratchet`
   (compound và improve-harness, explicit), `pulse-onboard` (brownfield,
   explicit, read-only pass trước). Không có `using`, không router.
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
  -> nạp skill pulse-shape: Story + qa.md, approach.md khi R2,
     Decision khi R3, Ticket ready qua ambiguity gate
  -> pulse run worker | reviewer | qa, docs validate --record, work close,
     work close-story

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

## Ba skill

### `pulse-shape`

Câu hỏi: từ intent này đến Ticket `ready` cần gì? SKILL.md dưới 120 dòng, mỗi
move là một file trong `references/`, nạp theo materialization:

| Move | Khi nào | Đọc | Ghi | Lệnh |
|---|---|---|---|---|
| wayfind | luôn | intent, `docs tree`, `work rollup` | `story.md` (đích, đã biết, chưa biết, quyết định phải ra trước) | `work create --kind story`, `edge add parent` |
| research | có câu hỏi mà repo không trả lời được | web, docs ngoài | `works/<ST>/research/<topic>.md` có nguồn và ngày | `work create --role decision_work` nếu cần track |
| brainstorm | R2+ hoặc có lựa chọn khó đảo ngược | `story.md`, `research/`, `docs search` | `approach.md`; `decision.md` | `work create --kind decision`; receipt `decision_acceptance` do human |
| grill | R1+ | `docs applicable`, `docs get`, code anchors | `## Open questions` với disposition | `work sync`, `work transition shaped` |
| plan | luôn | `approach.md`, `qa.md`, `docs impact` | `ticket.md` đầy đủ, `qa.md` baseline | `work create --kind ticket --risk`, `edge add blocked_by`, `qa baseline`, `work ready` |
| validate | R2+ hoặc có giả định chưa chứng minh | `work executability`, `work packet` | `plan.md`; spike là `decision_work` Ticket | `work packet --json` |

Gate human gắn vào trạng thái: Story `shaped` (đích đúng chưa), Decision
`accepted` (lựa chọn khó đảo ngược), Ticket `ready` (plan đúng chưa). R0 bỏ
qua skill này hoàn toàn.

Quy tắc trong skill: đọc repo và docs trước khi hỏi; một câu một lần với
recommended answer; mọi claim gắn nhãn authority; không tạo artifact ngoài
bảng trên; không tạo Ticket cho việc chưa được duyệt.

### `pulse-ratchet`

Câu hỏi: lần chạy này để lại gì cho lần sau, và harness sửa ở đâu? Gộp
compounding của Khuym với improve-harness và encode-invariant của
repository-harness, vì cả ba đều là "failure → owner → intervention → proof":

1. Đọc chuỗi bằng chứng của Ticket vừa đóng: handoff, verification, findings,
   note friction, `events tail --ticket`.
2. `pulse knowledge capture --from <ticket>` cho mỗi bài học; scope
   `repository` (về codebase) hoặc `harness` (về cách dùng Pulse). Không
   reusable thì `non_durable`, không tạo record.
3. Tìm earliest gap theo phân loại của repository-harness: context,
   capability, ownership, authority, proof, environment. Ghi vào
   `guidance.required_checks` hoặc `guidance.avoid`.
4. Một intervention, tại owner đúng, không hỏi: `knowledge promote
   --document` (docs), `--agents-md` (harness learning), `--decision`, hoặc
   role `check` mới trong `runners.json` khi bài học là invariant cơ học có
   authority. Ghi giả thuyết trước khi sửa: "nếu thêm X tại owner Y thì agent
   sau sẽ Z vì W; bằng chứng làm yếu: …; điều kiện gỡ: …".
5. Fresh rerun là Ticket kế tiếp chạm cùng path. Handoff của nó ghi
   `knowledge_usage`. `helpful` → `validate`; `misleading` hai lần → `retire`
   và gỡ intervention. Không có rerun thì giữ `candidate`; không claim cải
   thiện.

Explicit-only. Khối `AGENTS.md` nói agent gọi nó sau `work close`, hoặc
developer gọi định kỳ.

### `pulse-onboard`

Repo brownfield chưa có `.pulse/`. Lấy nguyên hai pass của repository-harness:
pass một read-only, ghi baseline Git và ignored state trước khi inspect, phân
loại authority từng claim, so docs với check hiện có, đề xuất; pass hai sau
approve chạy `pulse init`, `docs register`, `docs tags add`, backup docs vào
`.pulse/migrations/docs-backups/` trước khi restructure. Bỏ phần evidence
capsule và audit script, giữ bảng authority và thứ tự đề xuất (sửa instruction
sai trước, link tới guidance có sẵn, rồi mới thêm mới).

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

- `skills/pulse-shape/`, `skills/pulse-ratchet/`, `skills/pulse-onboard/`,
  mỗi cái SKILL.md ngắn và `references/`.
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
classification). Rồi khối `AGENTS.md` và `pulse-shape`, dogfood một Ticket R1
trên todolist bằng agent tương tác không gõ lệnh tay. Rồi `pulse-ratchet` với
friction tự động, kiểm chứng bằng Ticket kế tiếp thấy intervention trong packet
hoặc prompt. `pulse-onboard` sau cùng, thử trên một repo thật ngoài todolist.

## Consequences

- Agent tương tác có đường đi từ intent đến `ready` mà không cần developer
  thuộc CLI; R0 vẫn rẻ vì không qua skill.
- Harness tự ghi ma sát mỗi lần chạy; sửa harness không cần hỏi nhưng chỉ được
  tính là cải thiện khi có rerun, nên không phình ceremony.
- Ba skill thay vì router: trigger rõ, không nạp bảng lệnh cho mọi việc.
- Khối AGENTS có marker tạo thêm một thứ để giữ đồng bộ với CLI; guard parse
  lệnh chặn phần cú pháp, dogfood chặn phần ngữ nghĩa.
