# PLAN — Tái kiến trúc Pulse v2 thành workflow router + standalone utility skills

## 1. Bối cảnh cập nhật

Plan này được cập nhật theo các quyết định mới đã chốt:

- Pulse **không còn đi theo mô hình single public skill** cho toàn bộ capability.
- Pulse sẽ có **một workflow router skill duy nhất là `pulse:workflow`**.
- Workflow router chỉ chứa các phase của happy-path pipeline:
  - `onboard`
  - `explore`
  - `brainstorm`
  - `plan`
  - `validate`
  - `execute`
  - `swarm`
  - `review`
  - `compound`
- Các standalone public skills sẽ **đứng ngoài workflow router**:
  - `pulse:architecture-rescue`
  - `pulse:systematic-debug-fix`
  - `pulse:dev-note`
  - `pulse:dev-note-distil`
  - `pulse:prompt-leverage`
- `pulse-work` là **runtime CLI** riêng cho workgraph/runtime state, không phải public conversational surface.
- `preflight` bị loại bỏ; bootstrap/readiness được hấp thụ vào `pulse:workflow onboard`.
- `dream` bị loại bỏ khỏi packaged public surface.
- `skill-catalog.json` bị loại bỏ; không giữ thêm một lớp catalog trung gian.
- `HARNESS.md` là canonical reference của workflow router, nên nằm trong `skills/workflow/references/`.
- `HARNESS_BACKLOG.md` là template/seed artifact, nên nằm trong `skills/workflow/templates/` và được materialize vào `.pulse/harness/`.
- Plugin repo vẫn có thể self-host / dogfood Pulse v2 qua `.pulse/`, và runtime source canonical phải nằm trong workflow skill tree ở `skills/workflow/scripts/runtime/`.

Điểm thay đổi lớn nhất so với bản trước:

- Không còn tư duy “mọi capability phải đi qua một router duy nhất”.
- Thay vào đó, đây là một đợt **thu gọn workflow surface** về `pulse:workflow`, trong khi vẫn giữ các utility skills độc lập cho những tác vụ không phải workflow phase.

---

## 2. Quyết định kiến trúc đã chốt

### 2.1 Một workflow router public duy nhất: `pulse:workflow`

Workflow chính của Pulse đi qua đúng một entrypoint:

- `pulse:workflow onboard`
- `pulse:workflow explore`
- `pulse:workflow brainstorm`
- `pulse:workflow plan`
- `pulse:workflow validate`
- `pulse:workflow execute`
- `pulse:workflow swarm`
- `pulse:workflow review`
- `pulse:workflow compound`

Có thể bổ sung workflow commands sau này, nhưng **không quay lại mô hình mỗi workflow phase = một skill public riêng**.

### 2.2 Standalone public skills ở ngoài workflow router

Các skill sau vẫn là public surface, nhưng **không thuộc workflow pipeline**:

- `pulse:architecture-rescue`
- `pulse:systematic-debug-fix`
- `pulse:dev-note`
- `pulse:dev-note-distil`
- `pulse:prompt-leverage`

Lý do:

- chúng không phải các bước bắt buộc của happy-path flow
- chúng phục vụ mental model khác với workflow pipeline
- nhét chúng vào router sẽ làm router bị hiểu thành “menu mọi thứ” thay vì “pipeline chuẩn”

### 2.3 Tách rõ ba lớp: workflow router, standalone skills, runtime CLI

Cần giữ ranh giới rõ giữa:

- **`pulse:workflow ...`** = user-facing workflow router
- **`pulse:<standalone-skill>`** = user-facing utility skill độc lập
- **`pulse-work ...`** = runtime CLI thao tác workgraph/state

Ví dụ:

- user dùng `pulse:workflow plan` để vào planning flow
- user dùng `pulse:architecture-rescue` khi cần cứu hộ kiến trúc ngoài happy-path flow
- agent/harness dùng `pulse-work create`, `pulse-work ready`, `pulse-work close` để thao tác canonical workgraph

### 2.4 `HARNESS.md` là reference, không phải template

`HARNESS.md` là tài liệu mô tả hợp đồng vận hành của workflow router, nên canonical source phải nằm ở:

- `skills/workflow/references/HARNESS.md`

không phải dưới runtime plane.

### 2.5 `HARNESS_BACKLOG.md` là template/seed artifact

`HARNESS_BACKLOG.md` là file backlog vận hành của harness trong mỗi repo dùng Pulse, nên source canonical nên nằm ở:

- `skills/workflow/templates/HARNESS_BACKLOG.md`

và runtime materialization nằm ở:

- `.pulse/harness/HARNESS_BACKLOG.md`

### 2.6 Không giữ `skill-catalog.json`

Với kiến trúc mới:

- workflow router metadata sống trong `skills/workflow/SKILL.md`
- workflow command metadata sống trong `skills/workflow/scripts/command-metadata.json`
- standalone skills tự mang contract của chúng trong `skills/<skill>/SKILL.md`

`skill-catalog.json` trở thành một lớp metadata dư thừa và dễ drift.

### 2.7 Legacy concepts chỉ còn là migration language

Các khái niệm sau không còn là active runtime contract:

- `br`
- `bv`
- `.beads/`
- `history/<feature>/...`
- `pulse:preflight`
- `pulse:dream`

Nếu còn nhắc tới, chúng chỉ nên xuất hiện trong:

- migration docs
- compatibility readers có chủ đích
- audit notes

---

## 3. Mục tiêu cuối cùng

Sau migration, repo này cần đạt các trạng thái sau:

1. workflow pipeline đi qua đúng một router là `pulse:workflow`
2. các utility/rescue/note capabilities và các standalone support skills cần giữ vẫn tồn tại như standalone public skills, không bị ép thành workflow phases
3. Pulse v2 chạy bằng `pulse-work` thay cho `br` / `bv` / `.beads/`
4. runtime state canonical của repo tự host nằm ở `.pulse/runtime/`
5. metadata workgraph canonical của repo tự host nằm ở `.pulse/workgraph/items.jsonl`
6. runtime source canonical nằm ở `skills/workflow/scripts/runtime/`
7. `HARNESS.md` được giữ như reference source trong `skills/workflow/references/`
8. `HARNESS_BACKLOG.md` được giữ như template source trong `skills/workflow/templates/`
9. `preflight`, `dream`, `skill-catalog.json`, `refresh-project-docs`, và `writing-pulse-skills` không còn tồn tại trong target architecture
10. public skill inventory được phân loại rõ giữa workflow router, standalone public utilities, và removed legacy surfaces
11. repo này tự dogfood được contract v2 mới mà không làm lẫn lộn structure của plugin repo với downstream repo

---

## 4. ASCII project structure

### 4.1 Structure hiện tại của plugin repo

```text
pulse/
|-- .agents/
|   `-- plugins/
|-- .claude-plugin/
|-- .codex-plugin/
|-- .codex/
|   `-- hooks/
|-- .plugin-eval/
|-- .pulse/
|   |-- scripts/
|   |-- handoffs/
|   |-- checkpoints/
|   |-- memory/
|   |-- verification/
|   |-- state.json
|   |-- STATE.md
|   |-- tooling-status.json
|   |-- current-feature.json
|   |-- runtime-snapshot.json
|   `-- reservations.json
|-- assets/
|-- docs/
|   |-- evaluation/
|   `-- examples/
|-- hooks/
|-- pulse-eval-workspace/
|-- references/
|-- scripts/
|-- skills/
|   |-- architecture-rescue/
|   |-- brainstorming/
|   |-- compounding/
|   |-- dev-note/
|   |-- dev-note-distil/
|   |-- dream/
|   |-- executing/
|   |-- exploring/
|   |-- planning/
|   |-- preflight/
|   |-- prompt-leverage/
|   |-- refresh-project-docs/
|   |-- reviewing/
|   |-- swarming/
|   |-- systematic-debug-fix/
|   |-- using-pulse/
|   |-- validating/
|   `-- writing-pulse-skills/
|-- AGENTS.md
|-- CLAUDE.md
|-- CONTRIBUTING.md
|-- README.md
|-- SPEC.md
|-- PLAN.md
`-- skill-catalog.json
```

Vấn đề của trạng thái hiện tại:

- workflow surface bị phân mảnh thành quá nhiều skill
- router/public UX chưa tách rõ giữa workflow phases và utility skills
- bootstrap bị chia đôi giữa `preflight` và `using-pulse`
- runtime state của repo tự host bị mirror ở quá nhiều file top-level trong `.pulse/`
- runtime source hiện neo vào skill tree cũ thay vì có một runtime root rõ ràng
- `skill-catalog.json` tạo thêm một lớp routing metadata dễ drift
- inventory skill hiện tại vẫn trộn lẫn standalone public skills cần giữ với legacy utilities cần loại bỏ, nên target surface phải được phân loại tường minh

### 4.2 Target structure của plugin repo sau migration

```text
pulse/
|-- .agents/
|   `-- plugins/
|       `-- marketplace.json
|-- .claude-plugin/
|   |-- plugin.json
|   `-- marketplace.json
|-- .codex-plugin/
|   `-- plugin.json
|-- .codex/
|   `-- hooks/
|-- .plugin-eval/
|   `-- benchmark.json
|-- .pulse/
|   |-- workgraph/
|   |   |-- items.jsonl
|   |   |-- schema.json
|   |   |-- write.lock
|   |   `-- views/
|   |       |-- active.json
|   |       |-- closed.json
|   |       |-- ready.json
|   |       `-- graph.json
|   |-- runtime/
|   |   |-- tooling-status.json
|   |   |-- state.json
|   |   |-- STATE.md
|   |   |-- reservations.json
|   |   |-- handoffs/
|   |   |   `-- manifest.json
|   |   `-- checkpoints/
|   |-- harness/
|   |   `-- HARNESS_BACKLOG.md
|   |-- memory/
|   `-- scripts/
|       |-- pulse-work
|       |-- pulse_work.mjs
|       |-- pulse_state.mjs
|       |-- pulse_status.mjs
|       |-- pulse_session_context.mjs
|       `-- pulse_reservations.mjs
|-- assets/
|-- docs/
|   |-- ARCHITECTURE.md
|   |-- evaluation/
|   `-- examples/
|-- hooks/
|-- pulse-eval-workspace/
|-- references/
|   `-- impeccable/
|-- scripts/
|   |-- pulse-plugin-eval.mjs
|   `-- sync-skills.sh
|-- skills/
|   |-- pulse/
|   |   |-- SKILL.md
|   |   |-- references/
|   |   |   |-- HARNESS.md
|   |   |   `-- shared/
|   |   |       |-- workflow-contract.md
|   |   |       |-- planes-and-artifacts.md
|   |   |       |-- workgraph-model.md
|   |   |       |-- approval-gates.md
|   |   |       |-- verification-contract.md
|   |   |       |-- swarm-execution-rules.md
|   |   |       `-- handoff-and-resume.md
|   |   |-- commands/
|   |   |   |-- onboard/
|   |   |   |   |-- command.md
|   |   |   |   |-- references/
|   |   |   |   |   |-- readiness.md
|   |   |   |   |   `-- migration-warnings.md
|   |   |   |   `-- scripts/
|   |   |   |       `-- onboard_pulse.mjs
|   |   |   |-- explore/
|   |   |   |   `-- command.md
|   |   |   |-- brainstorm/
|   |   |   |   |-- command.md
|   |   |   |   `-- references/
|   |   |   |       `-- spec-reviewer-prompt.md
|   |   |   |-- plan/
|   |   |   |   `-- command.md
|   |   |   |-- validate/
|   |   |   |   `-- command.md
|   |   |   |-- swarm/
|   |   |   |   `-- command.md
|   |   |   |-- execute/
|   |   |   |   `-- command.md
|   |   |   |-- review/
|   |   |   |   `-- command.md
|   |   |   `-- compound/
|   |   |       `-- command.md
|   |   |-- templates/
|   |   |   |-- HARNESS_BACKLOG.md
|   |   |   `-- works/
|   |   |       |-- epic-README.md
|   |   |       |-- story-README.md
|   |   |       |-- story-SPEC.md
|   |   |       |-- task-README.md
|   |   |       `-- verification.md
|   |   `-- scripts/
|   |       |-- command-metadata.json
|   |       |-- runtime/
|   |       |   |-- pulse_work.mjs
|   |       |   |-- workgraph_store.mjs
|   |       |   |-- workgraph_validate.mjs
|   |       |   |-- workgraph_ids.mjs
|   |       |   |-- workgraph_paths.mjs
|   |       |   |-- workgraph_views.mjs
|   |       |   |-- workgraph_lock.mjs
|   |       |   |-- workgraph_templates.mjs
|   |       |   |-- pulse_state.mjs
|   |       |   |-- pulse_status.mjs
|   |       |   |-- pulse_session_context.mjs
|   |       |   `-- pulse_reservations.mjs
|   |       `-- lib/
|   |           |-- resolve-command.mjs
|   |           |-- render-help.mjs
|   |           `-- paths.mjs
|   |-- architecture-rescue/
|   |-- dev-note/
|   |-- dev-note-distil/
|   |-- prompt-leverage/
|   `-- systematic-debug-fix/
|-- tests/
|   |-- pulse/
|   |-- runtime/
|   `-- integration/
|-- AGENTS.md
|-- CLAUDE.md
|-- CONTRIBUTING.md
|-- README.md
|-- SPEC.md
`-- PLAN.md
```

Điểm cốt lõi của target structure:

- workflow router có source tree riêng ở `skills/workflow/`
- workflow commands đi theo command modules ở `skills/workflow/references/<command>/`
- runtime source canonical nằm trong workflow skill tree ở `skills/workflow/scripts/runtime/`
- `.pulse/scripts/` chỉ là installed mirror cho runtime-facing scripts
- standalone public skills vẫn tồn tại ở `skills/architecture-rescue/`, `skills/systematic-debug-fix/`, `skills/dev-note/`, `skills/dev-note-distil/`, `skills/prompt-leverage/`
- `refresh-project-docs` và `writing-pulse-skills` bị loại khỏi target packaged surface thay vì được giữ như một lớp utility riêng
- `HARNESS.md` là reference source, còn `HARNESS_BACKLOG.md` là template source
- brainstorm là story-scoped; output của nó là `SPEC.md` dưới `works/`, còn story `README.md` vẫn là description artifact riêng

### 4.3 Những gì phải biến mất khỏi target state

```text
REMOVE
|-- skills/preflight/
|-- skills/using-pulse/
|-- skills/exploring/
|-- skills/planning/
|-- skills/validating/
|-- skills/swarming/
|-- skills/executing/
|-- skills/reviewing/
|-- skills/compounding/
|-- skills/brainstorming/
|-- skills/dream/
|-- skill-catalog.json
|-- .pulse/current-feature.json
|-- .pulse/runtime-snapshot.json
`-- top-level .pulse/reservations.json
```

Ở mức contract/docs cũng phải bỏ vai trò active-source của:

```text
STOP TREATING AS ACTIVE RUNTIME CONTRACT
|-- history/<feature>/...
`-- .beads/
```

### 4.4 Classification của public skills và removed legacy utilities

Public workflow surface:

- `pulse:workflow`

Public standalone utility surface:

- `pulse:architecture-rescue`
- `pulse:systematic-debug-fix`
- `pulse:dev-note`
- `pulse:dev-note-distil`
- `pulse:prompt-leverage`

Removed legacy utility surface:

- `bootstrap-project-context`
- `refresh-project-docs`
- `writing-pulse-skills`

### 4.5 Scope của `.gitignore`

Ở chính plugin repo này, `.gitignore` **không thay đổi** trong đợt migration này. `.pulse/` ở repo này vẫn là local dogfood/runtime state.

Nếu cần mô tả policy track/ignore cho downstream repo, điều đó phải được viết như contract của repo cài Pulse, không phải như thay đổi bắt buộc với plugin repo này.

---

## 5. Mapping từ surface cũ sang surface mới

Workflow mapping:

- `using-pulse` + `preflight` → `pulse:workflow onboard`
- `exploring` → `pulse:workflow explore`
- `brainstorming` → `pulse:workflow brainstorm`
- `planning` → `pulse:workflow plan`
- `validating` → `pulse:workflow validate`
- `swarming` → `pulse:workflow swarm`
- `executing` → `pulse:workflow execute`
- `reviewing` → `pulse:workflow review`
- `compounding` → `pulse:workflow compound`

Standalone utility mapping:

- `architecture-rescue` → `pulse:architecture-rescue`
- `systematic-debug-fix` → `pulse:systematic-debug-fix`
- `dev-note` → `pulse:dev-note`
- `dev-note-distil` → `pulse:dev-note-distil`
- `bootstrap-project-context` → remove
- `prompt-leverage` → `pulse:prompt-leverage`

Removed:

- `dream` → remove
- `refresh-project-docs` → remove
- `writing-pulse-skills` → remove

---

## 6. Phát hiện chính sau khi explore repo

### 6.1 Runtime/onboarding hiện đang neo mạnh vào `skills/using-pulse/scripts/`

Các file lõi hiện tại tập trung ở:

- `skills/using-pulse/scripts/pulse_state.mjs`
- `skills/using-pulse/scripts/pulse_status.mjs`
- `skills/using-pulse/scripts/pulse_session_context.mjs`
- `skills/using-pulse/scripts/pulse_reservations.mjs`
- `skills/using-pulse/scripts/onboard_pulse.mjs`
- `.pulse/scripts/pulse_state.mjs`

Kết luận:

- runtime brain hiện tại đã tồn tại, nhưng neo vào source tree cũ
- migration đúng không phải viết lại từ số 0, mà là **dời và tái tổ chức** chúng về `skills/workflow/scripts/runtime/` và `skills/workflow/scripts/onboard/`

### 6.2 `preflight` là behavioral dependency, không chỉ là một folder

`preflight` đang bị encode trong docs, tests, hooks, evals, và onboarding language.

Kết luận:

- bỏ `preflight` là **repo-wide contract rewrite**
- `pulse:workflow onboard` phải thay được vai trò cũ trước khi xóa sạch references

### 6.3 `dream` không nên được migrate 1:1

`dream` có blast radius trong docs/eval/tests, nhưng không phải một workflow contract cần giữ.

Kết luận:

- hướng đúng là **delete**, không phải rename thành surface mới
- nếu có hành vi nào của `dream` đáng giữ thì phải được hấp thụ có chủ đích, không giữ skill public riêng

### 6.4 `skill-catalog.json` là di sản của multi-skill era

Với cấu trúc mới:

- workflow menu đến từ `skills/workflow/SKILL.md`
- workflow command behavior đi qua `skills/workflow/references/<command>/command.md`
- workflow command metadata đến từ `skills/workflow/scripts/command-metadata.json`
- standalone skills tự mang contract của chúng trong từng thư mục skill

### 6.5 Utility skills không nên bị ép vào workflow router

`architecture-rescue`, `systematic-debug-fix`, `dev-note`, `dev-note-distil` có mental model khác happy-path pipeline.

Kết luận:

- chúng nên được giữ thành **standalone public skills**
- workflow router phải chỉ đại diện cho pipeline chuẩn
- naming, docs, manifests, và tests phải phản ánh ranh giới này

### 6.6 Residual skill inventory phải được tách rõ giữa keep và remove

Các skill ngoài workflow router không nên bị gom chung vào một nhóm mơ hồ.

Keep như standalone public skills:

- `prompt-leverage`

Remove khỏi target packaged surface:

- `refresh-project-docs`
- `writing-pulse-skills`

Kết luận:

- `prompt-leverage` phải ở lại dưới `skills/` như standalone public skills
- `refresh-project-docs` và `writing-pulse-skills` phải bị loại khỏi packaged surface

---

## 7. Chiến lược triển khai tổng thể

Tôi khuyến nghị chia thành **6 phase chính**:

1. chốt workflow-router architecture và sửa structure/docs nền
2. dựng workflow router `pulse:workflow`
3. dựng `pulse-work` engine v1
4. migrate runtime/onboarding về runtime root mới
5. collapse workflow skills cũ + chốt keep/remove cho residual standalone skills
6. rewrite docs/hooks/eval/tests và audit cuối

---

## Phase 0 — Chốt kiến trúc mới và dọn nền repo

### Mục tiêu

Khóa lại target architecture mới để các phase sau không drift.

### File chính

- `PLAN.md`
- `SPEC.md`
- `.claude-plugin/plugin.json`
- `.claude-plugin/marketplace.json`
- `.codex-plugin/plugin.json`

### Việc cần làm

1. cập nhật `SPEC.md` cho khớp mô hình `pulse:workflow` + standalone skills
2. làm rõ ranh giới giữa workflow router, standalone utilities, và runtime CLI
3. làm rõ ranh giới giữa plugin repo này và downstream/self-hosted target repo
4. xác nhận `skill-catalog.json` bị loại bỏ khỏi target design
5. đồng bộ manifest/docs mô tả plugin theo public surface mới
6. phân loại rõ public workflow skill, public standalone skills cần giữ, và removed legacy utilities

### Done khi

- `SPEC.md` không còn giả định mọi capability đều đi qua một router duy nhất
- plan/spec/docs không còn mâu thuẫn về vị trí của HARNESS/HARNESS_BACKLOG
- runtime source path được chốt là `skills/workflow/scripts/runtime/`
- residual skills không còn ở trạng thái “chưa rõ số phận” giữa keep và remove

---

## Phase 1 — Dựng workflow router `pulse:workflow`

### Mục tiêu

Tạo workflow surface mới trước khi dẹp workflow surface cũ.

### File chính

- `skills/workflow/SKILL.md`
- `skills/workflow/references/HARNESS.md`
- `skills/workflow/references/<command>/command.md`
- `skills/workflow/references/<command>/*`
- `skills/workflow/scripts/<command>/*`
- `skills/workflow/references/shared/*`
- `skills/workflow/templates/HARNESS_BACKLOG.md`
- `skills/workflow/scripts/command-metadata.json`

### Việc cần làm

1. tạo `skills/workflow/SKILL.md` làm workflow router duy nhất
2. định nghĩa command table cho:
   - onboard
   - explore
   - brainstorm
   - plan
   - validate
   - execute
   - swarm
   - review
   - compound
3. tạo command modules riêng cho các command có assets nặng như `onboard` và `brainstorm`
4. chuyển `HARNESS.md` thành canonical reference ở `skills/workflow/references/HARNESS.md`
5. tạo `HARNESS_BACKLOG.md` seed template ở `skills/workflow/templates/`
6. tạo shared references dùng chung cho nhiều workflow command
7. thêm `story-SPEC.md` template và chốt rule rằng `pulse:workflow brainstorm` chỉ viết story-level `SPEC.md`, không ghi đè story `README.md`
8. tạo `command-metadata.json` làm metadata source duy nhất cho workflow subcommands
9. bỏ dependency kiến trúc vào `skill-catalog.json`

### Done khi

- `pulse:workflow` có thể đóng vai trò workflow router public duy nhất
- workflow command surface mới được mô tả đầy đủ trong một nơi duy nhất
- brainstorm được chốt là story-scoped và ghi output vào `works/**/SPEC.md`
- `HARNESS.md` và `HARNESS_BACKLOG.md` đã được đặt đúng lớp kiến trúc

---

## Phase 2 — Dựng `pulse-work` engine v1

### Mục tiêu

Có workgraph engine thật để workflow router và runtime bám vào.

### File chính

- `skills/workflow/scripts/runtime/pulse_work.mjs`
- `skills/workflow/scripts/runtime/workgraph_store.mjs`
- `skills/workflow/scripts/runtime/workgraph_validate.mjs`
- `skills/workflow/scripts/runtime/workgraph_ids.mjs`
- `skills/workflow/scripts/runtime/workgraph_paths.mjs`
- `skills/workflow/scripts/runtime/workgraph_views.mjs`
- `skills/workflow/scripts/runtime/workgraph_lock.mjs`
- `skills/workflow/scripts/runtime/workgraph_templates.mjs`

### Việc cần làm

1. parse/save `.pulse/workgraph/items.jsonl`
2. validate schema strict theo `SPEC.md`
3. generate IDs theo `<KIND>-<TIMESECOND>[-<SEQ>]`
4. build workgraph and works-surface scaffolding
5. implement write lock + atomic writes
6. build generated views
7. implement command tối thiểu:
   - `create`
   - `show`
   - `list`
   - `ready`
   - `update`
   - `close`
   - `reopen`
   - `dep add`
   - `dep rm`
   - `children`
   - `graph`
   - `doctor`

### Done khi

- `pulse-work` có thể thao tác canonical workgraph
- runtime source tree có vị trí rõ ràng trong workflow skill tree
- `.pulse/scripts/*` có thể được materialize từ `skills/workflow/scripts/runtime/`

---

## Phase 3 — Migrate runtime/onboarding về kiến trúc mới

### Mục tiêu

Bỏ mô hình `preflight -> using-pulse`, thay bằng `pulse:workflow onboard` + runtime mới.

### File chính

- `skills/workflow/scripts/onboard/onboard_pulse.mjs`
- `skills/workflow/scripts/runtime/pulse_state.mjs`
- `skills/workflow/scripts/runtime/pulse_status.mjs`
- `skills/workflow/scripts/runtime/pulse_session_context.mjs`
- `skills/workflow/scripts/runtime/pulse_reservations.mjs`
- `.pulse/scripts/*`

### Việc cần làm

1. port logic từ `skills/using-pulse/scripts/*` sang `skills/workflow/scripts/runtime/*` và `skills/workflow/scripts/onboard/*`
2. chuyển canonical runtime paths sang `.pulse/runtime/*`
3. bỏ persistence của:
   - `.pulse/current-feature.json`
   - `.pulse/runtime-snapshot.json`
4. dời reservations sang `.pulse/runtime/reservations.json`
5. materialize `.pulse/harness/HARNESS_BACKLOG.md` từ template source
6. giữ `pulse_status` như scout tool bám vào runtime mới
7. để `pulse:workflow onboard` thay authority cũ của `preflight` + `using-pulse`

### Done khi

- repo có thể bootstrap từ `pulse:workflow onboard`
- runtime state canonical nằm dưới `.pulse/runtime/`
- `.pulse/harness/HARNESS_BACKLOG.md` được tạo đúng từ template source

---

## Phase 4 — Collapse workflow skills cũ và phân loại lại public surface

### Mục tiêu

Thu gọn workflow surface về `pulse:workflow` trong khi giữ các standalone utilities ở ngoài router.

### File chính

- `skills/preflight/**`
- `skills/using-pulse/**`
- `skills/exploring/**`
- `skills/planning/**`
- `skills/validating/**`
- `skills/swarming/**`
- `skills/executing/**`
- `skills/reviewing/**`
- `skills/compounding/**`
- `skills/brainstorming/**`
- `skills/dream/**`
- `skills/architecture-rescue/**`
- `skills/dev-note/**`
- `skills/dev-note-distil/**`
- `skills/prompt-leverage/**`
- `skills/systematic-debug-fix/**`
- `skills/refresh-project-docs/**`
- `skills/writing-pulse-skills/**`

### Việc cần làm

1. migrate nội dung workflow skills cũ sang `skills/workflow/references/`
2. giữ `architecture-rescue`, `systematic-debug-fix`, `dev-note`, `dev-note-distil`, `prompt-leverage` thành standalone public skills
3. update các standalone skills để docs của chúng không giả định chúng là workflow phases
4. xóa các workflow skill public cũ sau khi router mới đã usable
5. xóa hẳn `dream`
6. xóa hẳn `skill-catalog.json`
7. xóa `refresh-project-docs` và `writing-pulse-skills` khỏi packaged public surface

### Done khi

- workflow public surface chỉ còn `pulse:workflow`
- rescue/debug/note và các standalone support skills đã chốt tồn tại riêng như standalone public skills
- `refresh-project-docs` và `writing-pulse-skills` không còn bị ship như public skills
- `dream` và `skill-catalog.json` không còn tồn tại

---

## Phase 5 — Sửa docs, hooks, eval, benchmark, và tests

### Mục tiêu

Đồng bộ toàn repo theo public surface mới.

### File chính

- `README.md`
- `AGENTS.md`
- `AGENTS.template.md`
- `CLAUDE.md`
- `docs/ARCHITECTURE.md`
- `docs/examples/golden-path.md`
- `CONTRIBUTING.md`
- `hooks/*`
- `.codex/hooks/*`
- `scripts/pulse-plugin-eval.mjs`
- `.plugin-eval/benchmark.json`
- `pulse-eval-workspace/evals.json`
- `tests/**`

### Việc cần làm

1. rewrite docs từ “một router cho mọi capability” sang “workflow router + standalone utilities + runtime CLI”
2. update session-start/bootstrap language sang `pulse:workflow onboard`
3. update eval corpus sang `pulse:workflow <command>` cho workflow path
4. update docs/tests cho standalone skills:
   - `pulse:architecture-rescue`
   - `pulse:systematic-debug-fix`
   - `pulse:dev-note`
   - `pulse:dev-note-distil`
   - `pulse:prompt-leverage`
5. bỏ `bv`-specific hook guard
6. update tests cho:
   - workflow router
   - runtime layout `.pulse/runtime/*`
   - workgraph layout `.pulse/workgraph/*`
   - harness backlog materialization
7. bỏ mọi assumption về `preflight`, `dream`, `skill-catalog.json`, `br`, `bv`

### Done khi

- docs, hooks, tests, và eval cùng phản ánh public surface mới
- repo không còn rule nào kéo user quay về legacy workflow skills

---

## Phase 6 — Migration docs, cleanup, và audit cuối

### Mục tiêu

Khóa lại migration bằng tài liệu rõ ràng và cleanup repo-wide.

### Nội dung bắt buộc

1. mô tả rõ `pulse:workflow` là workflow router duy nhất
2. mô tả rõ các standalone public skills là utilities ngoài workflow pipeline
3. mô tả rõ `pulse-work` là runtime CLI riêng
4. mô tả rõ vị trí của:
   - `skills/workflow/references/HARNESS.md`
   - `skills/workflow/templates/HARNESS_BACKLOG.md`
   - `.pulse/harness/HARNESS_BACKLOG.md`
   - `skills/workflow/scripts/runtime/`
5. map legacy concepts sang contract mới
6. backup brownfield docs nếu cần trước các đợt restructure lớn

### Audit cuối cần grep lại

- `pulse:preflight`
- `pulse:using-pulse`
- `pulse:dream`
- `skill-catalog.json`
- `br`
- `bv`
- `.beads`
- `history/`
- `.pulse/current-feature.json`
- `.pulse/runtime-snapshot.json`
- top-level `.pulse/reservations.json`

### Done khi

- legacy references chỉ còn ở migration notes hoặc compatibility readers có chủ đích
- repo contract v2 nhất quán end-to-end

---

## 8. File inventory ưu tiên cao

### P0 — phải chạm sớm

- `SPEC.md`
- `skills/workflow/SKILL.md`
- `skills/workflow/references/HARNESS.md`
- `skills/workflow/templates/HARNESS_BACKLOG.md`
- `skills/workflow/templates/works/story-SPEC.md`
- `skills/workflow/scripts/command-metadata.json`
- `skills/workflow/scripts/runtime/pulse_work.mjs`
- `skills/workflow/scripts/onboard/onboard_pulse.mjs`
- `skills/workflow/scripts/runtime/pulse_state.mjs`
- `skills/workflow/scripts/runtime/pulse_status.mjs`
- `skills/workflow/scripts/runtime/pulse_session_context.mjs`
- `skills/workflow/scripts/runtime/pulse_reservations.mjs`

### P1 — migrate ngay sau khi P0 ổn

- `skills/using-pulse/**`
- `skills/exploring/**`
- `skills/planning/**`
- `skills/validating/**`
- `skills/swarming/**`
- `skills/executing/**`
- `skills/reviewing/**`
- `skills/compounding/**`
- `skills/brainstorming/**`
- `skills/preflight/**`
- `skills/dream/**`
- `skills/architecture-rescue/**`
- `skills/systematic-debug-fix/**`
- `skills/dev-note/**`
- `skills/dev-note-distil/**`
- `README.md`
- `AGENTS.md`
- `AGENTS.template.md`
- `CLAUDE.md`
- `docs/ARCHITECTURE.md`
- `docs/examples/golden-path.md`
- `CONTRIBUTING.md`

### P2 — khóa release cuối

- `hooks/pre-tool-use.mjs`
- `hooks/session-start.mjs`
- `scripts/pulse-plugin-eval.mjs`
- `.plugin-eval/benchmark.json`
- `pulse-eval-workspace/evals.json`
- `docs/evaluation/pulse-plugin-eval.md`
- migration blueprint docs

---

## 9. Kiểm thử và verification plan

### 9.1 Workflow router tests

Cần test cho:

- `pulse:workflow` không args → render command menu đúng
- `pulse:workflow onboard` → load đúng command reference
- `pulse:workflow explore` → route đúng
- `pulse:workflow brainstorm` → route đúng
- `pulse:workflow plan` → route đúng
- `pulse:workflow validate` → route đúng
- `pulse:workflow execute` → route đúng
- `pulse:workflow swarm` → route đúng
- `pulse:workflow review` → route đúng
- `pulse:workflow compound` → route đúng
- first word không match command → fallback behavior hợp lệ

### 9.2 Standalone skill checks

Cần bảo đảm ít nhất:

- `pulse:architecture-rescue` vẫn là standalone public skill hợp lệ
- `pulse:systematic-debug-fix` vẫn là standalone public skill hợp lệ
- `pulse:dev-note` vẫn là standalone public skill hợp lệ
- `pulse:dev-note-distil` vẫn là standalone public skill hợp lệ
- `pulse:prompt-leverage` vẫn là standalone public skill hợp lệ
- docs/help/manifests không mô tả các skill này như workflow phases

### 9.3 Runtime / workgraph tests

Cần test cho:

- ID generation + collision suffix
- slug sanitize
- path derivation
- hierarchy validation
- dependency cycle detection
- close/reopen rules
- doctor checks
- stale lock detection
- unique prefix resolution
- workgraph view generation

### 9.4 Integration / smoke tests

1. bootstrap repo bằng `pulse:workflow onboard`
2. onboarding tạo đúng layout `.pulse/runtime/*`, `.pulse/workgraph/*`, `.pulse/harness/HARNESS_BACKLOG.md`
3. `pulse-work create` cho epic/story/task/bug, với task/bug chỉ được tạo dưới story
4. `pulse:workflow brainstorm` ghi output vào `works/**/SPEC.md` thay vì story `README.md`
5. `pulse-work dep add` / `dep rm`
6. `pulse-work ready --json`
7. `pulse-work close` fail nếu thiếu verification
8. `pulse-work reopen`
9. `pulse-work doctor --json`
10. `node .pulse/scripts/pulse_status.mjs --json`
11. verify repo không còn expose `pulse:preflight`, `pulse:using-pulse`, `pulse:dream`

### 9.5 Repo audit checks

Sau khi gần xong, chạy repo-wide audit để bảo đảm:

- không còn hard requirement `br` / `bv`
- workflow surface public chỉ còn `pulse:workflow`
- standalone rescue/debug/note/support skills không bị mô tả sai như workflow phases
- `refresh-project-docs` và `writing-pulse-skills` không còn ở packaged public surface
- không còn `skill-catalog.json`
- không còn `history/` là active work source trong runtime/contracts
- không còn top-level runtime state files cũ là canonical surfaces

---

## 10. Thứ tự thực hiện khuyến nghị theo changeset

### Changeset A — Kiến trúc nền

- cập nhật `PLAN.md`
- cập nhật `SPEC.md`
- chốt `pulse:workflow` là workflow router
- chốt standalone public skills
- chốt runtime source ở `skills/workflow/scripts/runtime/`
- chốt các standalone public skills được giữ lại và các legacy utilities bị loại bỏ

### Changeset B — Workflow router

- tạo `skills/workflow/SKILL.md`
- tạo command modules dưới `skills/workflow/references/`
- tạo `references/HARNESS.md`
- tạo `templates/HARNESS_BACKLOG.md`
- tạo `command-metadata.json`

### Changeset C — `pulse-work` engine

- dựng workgraph modules dưới `skills/workflow/scripts/runtime/`
- schema
- doctor
- views
- runtime CLI surface

### Changeset D — Runtime / onboard migration

- port scripts từ `skills/using-pulse/scripts/` sang `skills/workflow/scripts/runtime/` và `skills/workflow/scripts/onboard/`
- runtime paths sang `.pulse/runtime/*`
- materialize `.pulse/harness/HARNESS_BACKLOG.md`
- thay bootstrap bằng `pulse:workflow onboard`

### Changeset E — Collapse workflow skills + classify surfaces

- migrate nội dung workflow skills cũ vào `skills/workflow/references/`
- giữ rescue/debug/note/support skills đã chốt là standalone public surface
- xóa `preflight`
- xóa `dream`
- xóa `skill-catalog.json`
- xóa `refresh-project-docs` và `writing-pulse-skills`

### Changeset F — Docs / hooks / eval / audit cuối

- rewrite README / AGENTS / CLAUDE / docs
- update hooks
- update tests
- update benchmark/eval corpus
- cleanup grep pass

---

## 11. Định nghĩa hoàn thành

Plan này được coi là đạt mục tiêu khi repo có thể chứng minh đồng thời:

- user có đúng một workflow entrypoint public là `pulse:workflow`
- workflow command behavior được tổ chức rõ theo `skills/workflow/references/<command>/`
- `pulse-work` tồn tại như runtime CLI riêng, có vị trí rõ ràng ở `skills/workflow/scripts/runtime/`
- bootstrap bằng `pulse:workflow onboard`, không cần `pulse:preflight` hay `pulse:using-pulse`
- `architecture-rescue`, `systematic-debug-fix`, `dev-note`, `dev-note-distil`, `prompt-leverage` vẫn tồn tại như standalone public skills
- `dream`, `refresh-project-docs`, và `writing-pulse-skills` không còn tồn tại như packaged public skills
- `skill-catalog.json` không còn tồn tại
- `HARNESS.md` nằm đúng ở `skills/workflow/references/HARNESS.md`
- `HARNESS_BACKLOG.md` nằm đúng ở `skills/workflow/templates/HARNESS_BACKLOG.md` và materialize vào `.pulse/harness/HARNESS_BACKLOG.md`
- brainstorm output của story nằm ở `works/**/SPEC.md`, còn story `README.md` vẫn là description artifact riêng
- workgraph metadata, runtime state, workflow router, và standalone surfaces cùng phản ánh một contract v2 thống nhất

---

## 12. Kết luận ngắn

Hướng mới của Pulse không còn là “một router cho mọi capability”, mà là:

- **một workflow router duy nhất là `pulse:workflow`**
- **một nhóm standalone public skills cho rescue/debug/note/support utilities**
- **runtime thật tách ra thành `pulse-work` + `.pulse/workgraph` + `.pulse/runtime`**
- **loại bỏ các lớp dư thừa như `preflight`, `dream`, `skill-catalog.json`**
- **giữ `prompt-leverage` như standalone public skills và loại `bootstrap-project-context`, `refresh-project-docs`, `writing-pulse-skills` khỏi packaged surface**

Đây là hướng sạch hơn về mental model: workflow là workflow, utility là utility, runtime là runtime.