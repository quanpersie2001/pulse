# PLAN — Tái kiến trúc Pulse v2 thành single-skill workgraph plugin

## 1. Bối cảnh cập nhật

Plan này đã được cập nhật theo các quyết định mới đã chốt trong cuộc trao đổi:

- **Không giữ mô hình nhiều skill public** như hiện tại nữa.
- Pulse sẽ có **một user-facing skill duy nhất là `/pulse`**.
- Các phase/capability sẽ đi qua **subcommands** của `/pulse`, thay vì mỗi phase là một skill độc lập.
- **`pulse-work`** là **runtime CLI** riêng để thao tác workgraph, không phải command surface nói chuyện trực tiếp với user.
- **`preflight` bị loại bỏ**; bootstrap/readiness được hấp thụ vào `/pulse onboard`.
- **`dream` bị loại bỏ** khỏi packaged surface.
- **`skill-catalog.json` bị loại bỏ**; không giữ thêm một lớp catalog trung gian.
- **`HARNESS.md` là canonical reference của skill**, nên nằm trong `skills/pulse/references/`.
- **`HARNESS_BACKLOG.md` là template/seed artifact**, nên nằm trong `skills/pulse/templates/` và được materialize vào `.pulse/harness/`.
- Plugin repo vẫn có thể **self-host / dogfood** Pulse v2 qua `.pulse/`, nhưng project structure trong plan này phải mô tả **repo plugin**, không phải repo downstream sau khi cài Pulse.

Điểm thay đổi lớn nhất so với bản plan trước là:

- Không còn tư duy “migrate nhiều skill public sang workgraph”.
- Thay vào đó, đây là một đợt **collapse public skill surface** về một router `/pulse`, rồi mới migrate runtime phía dưới sang workgraph v2.

---

## 2. Quyết định kiến trúc đã chốt

### 2.1 Một skill public duy nhất: `/pulse`

Pulse nên có đúng một entrypoint public:

- `/pulse onboard`
- `/pulse explore`
- `/pulse brainstorm`
- `/pulse plan`
- `/pulse validate`
- `/pulse swarm`
- `/pulse execute`
- `/pulse review`
- `/pulse compound`
- `/pulse rescue`
- `/pulse systematic-debug`
- `/pulse note`
- `/pulse note-distill`

Có thể bổ sung command runtime-facing như `status` sau, nhưng **không quay lại mô hình mỗi capability = một skill public riêng**.

### 2.2 Tách rõ hai lớp: router skill và runtime CLI

Cần giữ ranh giới thật rõ:

- **`/pulse ...`** = user-facing workflow router
- **`pulse-work ...`** = runtime CLI thao tác workgraph/state

Ví dụ:

- user dùng `/pulse plan` để yêu cầu agent chạy planning flow
- agent dùng `pulse-work create`, `pulse-work ready`, `pulse-work close` để thao tác canonical workgraph

Hai lớp này phải được mô tả, test, và tổ chức file riêng rẽ.

### 2.3 `HARNESS.md` là reference, không phải template

`HARNESS.md` không phải file để seed runtime state. Nó là tài liệu canonical mô tả cách hoạt động của Pulse harness.

Vì vậy nó phải nằm ở:

- `skills/pulse/references/HARNESS.md`

không phải:

- `skills/pulse/templates/HARNESS.md`

### 2.4 `HARNESS_BACKLOG.md` là template/seed artifact

`HARNESS_BACKLOG.md` là file backlog vận hành của harness trong mỗi repo dùng Pulse, nên source canonical nên nằm ở:

- `skills/pulse/templates/HARNESS_BACKLOG.md`

và runtime materialization nằm ở:

- `.pulse/harness/HARNESS_BACKLOG.md`

### 2.5 Không giữ `skill-catalog.json`

Với single-skill architecture, `skill-catalog.json` trở thành một lớp metadata dư thừa và dễ drift.

Source of truth mới nên là:

- `skills/pulse/SKILL.md` — router + command table
- `skills/pulse/scripts/command-metadata.json` — description / hint / category metadata cho subcommands

### 2.6 Legacy concepts chỉ còn là migration language

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

1. Pulse v2 chạy bằng **`pulse-work`** thay cho `br` / `bv` / `.beads/`
2. Public command surface collapse về **một skill duy nhất là `/pulse`**
3. Runtime state canonical của repo tự host nằm ở `.pulse/runtime/`
4. Metadata workgraph canonical của repo tự host nằm ở `.pulse/workgraph/items.jsonl`
5. `HARNESS.md` được giữ như **reference source** trong `skills/pulse/references/`
6. `HARNESS_BACKLOG.md` được giữ như **template source** trong `skills/pulse/templates/`
7. `preflight`, `dream`, và `skill-catalog.json` không còn tồn tại trong target architecture
8. Các capability như brainstorm, rescue, systematic-debug, note vẫn được giữ lại, nhưng nằm dưới router `/pulse`
9. Repo này tự dogfood được contract v2 mới mà không làm lẫn lộn structure của plugin repo với downstream repo

---

## 4. ASCII project structure

## 4.1 Structure hiện tại của chính plugin repo

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
|   |-- bootstrap-project-context/
|   |-- brainstorming/
|   |-- compounding/
|   |-- dev-note/
|   |-- dev-note-distil/
|   |-- dream/
|   |-- executing/
|   |-- exploring/
|   |-- gitnexus/
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

- public surface bị phân mảnh thành quá nhiều skill
- bootstrap bị chia đôi giữa `preflight` và `using-pulse`
- runtime state của repo tự host bị mirror ở quá nhiều file top-level trong `.pulse/`
- `pulse-work` chưa có vị trí rõ ràng trong source tree architecture
- `skill-catalog.json` tạo thêm một lớp routing metadata dễ drift
- các capability cứu hộ/ghi chú mạnh nhưng đang bị phơi thành các skill rời, làm mental model nặng hơn cần thiết
- các utility như `bootstrap-project-context`, `prompt-leverage`, `refresh-project-docs`, `writing-pulse-skills`, và `gitnexus` vẫn đang nằm dưới `skills/`, nên nếu không được phân loại sớm chúng có thể tiếp tục bị ship như public surface ngoài ý muốn

## 4.2 Target structure của plugin repo sau migration

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
|   |   |-- pulse-work
|   |   |-- pulse_work.mjs
|   |   |-- pulse_state.mjs
|   |   |-- pulse_status.mjs
|   |   |-- pulse_session_context.mjs
|   |   `-- pulse_reservations.mjs
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
|   `-- pulse/
|       |-- SKILL.md
|       |-- references/
|       |   |-- HARNESS.md
|       |   `-- shared/
|       |       |-- workflow-contract.md
|       |       |-- planes-and-artifacts.md
|       |       |-- workgraph-model.md
|       |       |-- approval-gates.md
|       |       |-- verification-contract.md
|       |       |-- swarm-execution-rules.md
|       |       `-- handoff-and-resume.md
|       |-- commands/
|       |   |-- onboard/
|       |   |   |-- command.md
|       |   |   |-- references/
|       |   |   |   |-- readiness.md
|       |   |   |   `-- migration-warnings.md
|       |   |   `-- scripts/
|       |   |       `-- onboard_pulse.mjs
|       |   |-- explore/
|       |   |   `-- command.md
|       |   |-- brainstorm/
|       |   |   |-- command.md
|       |   |   |-- references/
|       |   |   |   |-- spec-reviewer-prompt.md
|       |   |   |   `-- visual-support-guidance.md
|       |   |   `-- scripts/
|       |   |       |-- start-visual-server.sh
|       |   |       |-- stop-visual-server.sh
|       |   |       |-- visual-frame-template.html
|       |   |       |-- visual-helper.js
|       |   |       `-- visual-server.cjs
|       |   |-- plan/
|       |   |   `-- command.md
|       |   |-- validate/
|       |   |   `-- command.md
|       |   |-- swarm/
|       |   |   `-- command.md
|       |   |-- execute/
|       |   |   `-- command.md
|       |   |-- review/
|       |   |   `-- command.md
|       |   |-- compound/
|       |   |   `-- command.md
|       |   |-- rescue/
|       |   |   `-- command.md
|       |   |-- systematic-debug/
|       |   |   `-- command.md
|       |   |-- note/
|       |   |   `-- command.md
|       |   `-- note-distill/
|       |       `-- command.md
|       |-- templates/
|       |   |-- HARNESS_BACKLOG.md
|       |   `-- works/
|       |       |-- epic-README.md
|       |       |-- story-README.md
|       |       |-- task-README.md
|       |       `-- verification.md
|       |-- scripts/
|       |   |-- command-metadata.json
|       |   |-- runtime/
|       |   |   |-- pulse_work.mjs
|       |   |   |-- workgraph_store.mjs
|       |   |   |-- workgraph_validate.mjs
|       |   |   |-- workgraph_ids.mjs
|       |   |   |-- workgraph_paths.mjs
|       |   |   |-- workgraph_views.mjs
|       |   |   |-- workgraph_lock.mjs
|       |   |   |-- workgraph_templates.mjs
|       |   |   |-- pulse_state.mjs
|       |   |   |-- pulse_status.mjs
|       |   |   |-- pulse_session_context.mjs
|       |   |   `-- pulse_reservations.mjs
|       |   `-- lib/
|       |       |-- resolve-command.mjs
|       |       |-- render-help.mjs
|       |       `-- paths.mjs
|       `-- tests/
|           |-- router/
|           |-- runtime/
|           |-- integration/
|           `-- fixtures/
|-- AGENTS.md
|-- CLAUDE.md
|-- CONTRIBUTING.md
|-- README.md
|-- SPEC.md
`-- PLAN.md
```

Điểm cốt lõi của target structure:

- chỉ còn **một skill source tree** ở `skills/pulse/`
- command behavior không bị flatten, mà đi theo **command modules** ở `skills/pulse/commands/<command>/`
- `pulse-work` có vị trí rõ ràng ở `skills/pulse/scripts/runtime/pulse_work.mjs`
- `.pulse/scripts/` chỉ là installed mirror cho runtime-facing scripts, không phải mirror bắt buộc của mọi command asset
- `HARNESS.md` là **reference source**
- `HARNESS_BACKLOG.md` là **template source**
- `.pulse/harness/` chỉ materialize runtime backlog, tránh duplicate `HARNESS.md`
- không còn `skill-catalog.json`
- các utility maintainer-only như `bootstrap-project-context`, `prompt-leverage`, `refresh-project-docs`, `writing-pulse-skills`, và `gitnexus` không thuộc public workflow contract; nếu còn được giữ lại thì phase 4 phải di dời hoặc loại chúng khỏi packaged public discovery

## 4.3 Những gì phải biến mất khỏi target state

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
|-- skills/architecture-rescue/
|-- skills/systematic-debug-fix/
|-- skills/dev-note/
|-- skills/dev-note-distil/
|-- skill-catalog.json
|-- .pulse/current-feature.json
|-- .pulse/runtime-snapshot.json
`-- top-level .pulse/reservations.json
```

Và ở mức contract/docs cũng phải bỏ vai trò active-source của các khái niệm cũ:

```text
STOP TREATING AS ACTIVE RUNTIME CONTRACT
|-- history/<feature>/...
`-- .beads/
```

---

## 5. Mapping từ skill cũ sang command mới

Mapping public surface nên được chốt như sau:

- `using-pulse` + `preflight` → `/pulse onboard`
- `exploring` → `/pulse explore`
- `brainstorming` → `/pulse brainstorm`
- `planning` → `/pulse plan`
- `validating` → `/pulse validate`
- `swarming` → `/pulse swarm`
- `executing` → `/pulse execute`
- `reviewing` → `/pulse review`
- `compounding` → `/pulse compound`
- `architecture-rescue` → `/pulse rescue`
- `systematic-debug-fix` → `/pulse systematic-debug`
- `dev-note` → `/pulse note`
- `dev-note-distil` → `/pulse note-distill`
- `dream` → remove

Điều này cho phép giữ lại toàn bộ capability quan trọng, nhưng thu gọn mental model về một router duy nhất.

---

## 6. Phát hiện chính sau khi explore repo

## 6.1 Runtime/onboarding hiện đang neo mạnh vào `skills/using-pulse/scripts/`

Các file lõi hiện tại tập trung ở:

- `skills/using-pulse/scripts/pulse_state.mjs`
- `skills/using-pulse/scripts/pulse_status.mjs`
- `skills/using-pulse/scripts/pulse_session_context.mjs`
- `skills/using-pulse/scripts/pulse_reservations.mjs`
- `skills/using-pulse/scripts/onboard_pulse.mjs`
- `.pulse/scripts/pulse_state.mjs`

Kết luận:

- runtime brain hiện tại đã tồn tại, nhưng neo vào source tree cũ
- migration đúng không phải viết lại từ số 0, mà là **dời và tái tổ chức** chúng về `skills/pulse/scripts/runtime/` và `skills/pulse/commands/onboard/scripts/`

## 6.2 `preflight` là behavioral dependency, không chỉ là một folder

`preflight` đang bị encode trong:

- `README.md`
- `AGENTS.md`
- `AGENTS.template.md`
- `CLAUDE.md`
- `docs/ARCHITECTURE.md`
- `docs/examples/golden-path.md`
- `skills/using-pulse/SKILL.md`
- `skills/planning/SKILL.md`
- `skills/validating/SKILL.md`
- `skills/executing/SKILL.md`
- `skills/swarming/SKILL.md`
- `skills/reviewing/SKILL.md`
- `skills/compounding/SKILL.md`
- `skills/exploring/SKILL.md`
- `skills/using-pulse/scripts/test_onboard_pulse.mjs`
- `pulse-eval-workspace/evals.json`
- `scripts/pulse-plugin-eval.mjs`
- `CONTRIBUTING.md`

Kết luận:

- bỏ `preflight` là **repo-wide contract rewrite**
- command `/pulse onboard` phải thay được vai trò cũ trước khi xóa sạch references

## 6.3 `dream` không nên được migrate 1:1

`dream` có blast radius trong docs/eval/tests, nhưng user không dùng nó.

Kết luận:

- hướng đúng là **delete**, không phải rename thành subcommand mới
- nếu có một capability nào đó từ `dream` thực sự cần giữ, nó chỉ nên được hấp thụ một phần vào `compound`, không giữ public route riêng

## 6.4 `skill-catalog.json` là di sản của multi-skill era

Khi repo đi theo một skill router duy nhất:

- `skill-catalog.json` không còn vai trò kiến trúc hợp lý
- command menu phải đến từ `skills/pulse/SKILL.md`
- command behavior nên đi qua `skills/pulse/commands/<command>/command.md`
- command metadata phải đến từ `skills/pulse/scripts/command-metadata.json`

## 6.5 Làm rõ scope của `.gitignore`

Ở chính plugin repo này, `.gitignore` **không thay đổi** trong đợt migration này. `.pulse/` ở repo này vẫn là local dogfood/runtime state.

Nếu cần mô tả policy track/ignore cho repo downstream hoặc self-hosted target repo, điều đó phải được viết như contract của repo cài Pulse, không phải như thay đổi bắt buộc đối với `.gitignore` của plugin repo này.

## 6.6 Classification của residual skills và packaging constraint

Ngoài workflow surface đang collapse vào `/pulse`, repo hiện còn các skill không nằm trong mapping public chính:

- `bootstrap-project-context`
- `prompt-leverage`
- `refresh-project-docs`
- `writing-pulse-skills`
- `gitnexus`

Quyết định phase 0 được khóa như sau:

- các skill trên là **maintainer/developer-only utilities**, không phải public Pulse workflow contract
- `dream` là **obsolete legacy skill** và bị xóa ở phase 4, không migrate thành subcommand mới
- `skill-catalog.json` từ thời điểm này chỉ còn là **legacy artifact** chờ xóa ở phase 4; nó không còn là source of truth cho docs, manifest, hay router design
- nếu auto-discovery vẫn khiến các utility này bị ship như packaged public skills, phase 4 hoặc một cleanup phụ phải di dời chúng ra khỏi `skills/` hoặc loại khỏi packaging surface

Constraint này là dependency bắt buộc của phase 4; không được để các utility tồn tại như public surface by accident.

---

## 7. Chiến lược triển khai tổng thể

Tôi khuyến nghị chia thành **6 phase chính**:

1. chốt single-skill architecture và sửa structure/docs nền
2. dựng router `/pulse`
3. dựng `pulse-work` engine v1
4. migrate runtime/onboarding về source tree mới
5. collapse skill cũ + rewrite docs/eval/tests
6. cleanup migration và audit cuối

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

1. cập nhật `SPEC.md` cho khớp single-skill architecture
2. làm rõ ranh giới giữa plugin repo này và downstream/self-hosted target repo
   - không mô tả việc sửa `.gitignore` của plugin repo này như một hạng mục migration
   - mọi guidance về track/ignore `.pulse` phải được gắn đúng vào contract của target repo nếu cần
3. xác nhận `skill-catalog.json` bị loại bỏ khỏi target design và chỉ còn là legacy artifact chờ cleanup
4. đồng bộ manifest/docs mô tả plugin theo single-skill model
5. phân loại rõ residual skills còn nằm dưới `skills/` và ghi chúng thành packaging constraint cho phase 4

### Done khi

- `SPEC.md` không còn giả định multi-skill public surface
- plan/spec/docs không còn gán nhầm việc sửa `.gitignore` của plugin repo này thành hạng mục bắt buộc
- plan/spec/docs không còn mâu thuẫn về vị trí của HARNESS/HARNESS_BACKLOG
- residual skills không còn ở trạng thái “chưa rõ số phận” trước khi phase 4 bắt đầu

---

## Phase 1 — Dựng router skill `/pulse`

### Mục tiêu

Tạo public surface mới trước khi dẹp surface cũ.

### File chính

- `skills/pulse/SKILL.md`
- `skills/pulse/references/HARNESS.md`
- `skills/pulse/commands/<command>/command.md`
- `skills/pulse/commands/<command>/references/*`
- `skills/pulse/commands/<command>/scripts/*`
- `skills/pulse/references/shared/workflow-contract.md`
- `skills/pulse/references/shared/planes-and-artifacts.md`
- `skills/pulse/references/shared/workgraph-model.md`
- `skills/pulse/references/shared/approval-gates.md`
- `skills/pulse/references/shared/verification-contract.md`
- `skills/pulse/references/shared/swarm-execution-rules.md`
- `skills/pulse/references/shared/handoff-and-resume.md`
- `skills/pulse/templates/HARNESS_BACKLOG.md`
- `skills/pulse/scripts/command-metadata.json`

### Việc cần làm

1. tạo `skills/pulse/SKILL.md` làm router duy nhất
2. định nghĩa command table cho:
   - onboard
   - explore
   - brainstorm
   - plan
   - validate
   - swarm
   - execute
   - review
   - compound
   - rescue
   - systematic-debug
   - note
   - note-distill
3. tạo command modules riêng cho các command có assets nặng như `onboard` và `brainstorm`
4. chuyển `HARNESS.md` thành canonical reference ở `skills/pulse/references/HARNESS.md`
5. tạo `HARNESS_BACKLOG.md` seed template ở `skills/pulse/templates/`
6. tạo nhóm shared references dùng chung cho nhiều command:
   - `workflow-contract.md`
   - `planes-and-artifacts.md`
   - `workgraph-model.md`
   - `approval-gates.md`
   - `verification-contract.md`
   - `swarm-execution-rules.md`
   - `handoff-and-resume.md`
7. tạo `command-metadata.json` làm metadata source duy nhất cho subcommands
8. bỏ dependency kiến trúc vào `skill-catalog.json`

### Done khi

- `/pulse` có thể đóng vai trò router public duy nhất
- command surface mới được mô tả đầy đủ trong một nơi duy nhất
- command-specific references/scripts có chỗ ở rõ ràng thay vì bị flatten
- `HARNESS.md` và `HARNESS_BACKLOG.md` đã được đặt đúng lớp kiến trúc

---

## Phase 2 — Dựng `pulse-work` engine v1

### Mục tiêu

Có workgraph engine thật để router và runtime bám vào.

### File chính

- `skills/pulse/scripts/runtime/pulse_work.mjs`
- `skills/pulse/scripts/runtime/workgraph_store.mjs`
- `skills/pulse/scripts/runtime/workgraph_validate.mjs`
- `skills/pulse/scripts/runtime/workgraph_ids.mjs`
- `skills/pulse/scripts/runtime/workgraph_paths.mjs`
- `skills/pulse/scripts/runtime/workgraph_views.mjs`
- `skills/pulse/scripts/runtime/workgraph_lock.mjs`
- `skills/pulse/scripts/runtime/workgraph_templates.mjs`

### Việc cần làm

1. parse/save `.pulse/workgraph/items.jsonl`
2. validate schema strict theo `SPEC.md`
3. generate IDs theo `<KIND>-<TIMESECOND>[-<SEQ>]`
4. build self-host workgraph and work-surface scaffolding nếu repo tiếp tục dogfood full flow
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
- source tree đã có vị trí rõ ràng cho runtime CLI
- `.pulse/scripts/pulse_work.mjs` có thể được materialize từ source scripts

---

## Phase 3 — Migrate runtime/onboarding về kiến trúc mới

### Mục tiêu

Bỏ mô hình `preflight -> using-pulse`, thay bằng `/pulse onboard` + runtime mới.

### File chính

- `skills/pulse/commands/onboard/scripts/onboard_pulse.mjs`
- `skills/pulse/scripts/runtime/pulse_state.mjs`
- `skills/pulse/scripts/runtime/pulse_status.mjs`
- `skills/pulse/scripts/runtime/pulse_session_context.mjs`
- `skills/pulse/scripts/runtime/pulse_reservations.mjs`
- `.pulse/scripts/*`

### Việc cần làm

1. port logic từ `skills/using-pulse/scripts/*` sang `skills/pulse/scripts/runtime/*` và `skills/pulse/commands/onboard/scripts/*`
2. chuyển canonical runtime paths sang `.pulse/runtime/*`
3. bỏ persistence của:
   - `.pulse/current-feature.json`
   - `.pulse/runtime-snapshot.json`
4. dời reservations sang `.pulse/runtime/reservations.json`
5. materialize `.pulse/harness/HARNESS_BACKLOG.md` từ template source
6. giữ `pulse_status` như scout tool bám vào runtime mới
7. để `/pulse onboard` thay authority cũ của `preflight` + `using-pulse`

### Done khi

- repo có thể bootstrap từ `/pulse onboard`
- runtime state canonical nằm dưới `.pulse/runtime/`
- `.pulse/harness/HARNESS_BACKLOG.md` được tạo đúng từ template source

---

## Phase 4 — Collapse skill cũ vào router mới và xóa surface dư

### Mục tiêu

Thay multi-skill public surface bằng router `/pulse` thật sự.

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
- `skills/architecture-rescue/**`
- `skills/systematic-debug-fix/**`
- `skills/dev-note/**`
- `skills/dev-note-distil/**`
- `skills/dream/**`
- `README.md`
- `AGENTS.md`
- `AGENTS.template.md`
- `CLAUDE.md`
- `docs/ARCHITECTURE.md`
- `docs/examples/golden-path.md`
- `CONTRIBUTING.md`

### Việc cần làm

1. migrate nội dung phase skills cũ sang `skills/pulse/commands/`
2. migrate brainstorming flow sang `skills/pulse/commands/brainstorm/`
3. migrate architecture rescue / debug / note flows sang:
   - `skills/pulse/commands/rescue/`
   - `skills/pulse/commands/systematic-debug/`
   - `skills/pulse/commands/note/`
   - `skills/pulse/commands/note-distill/`
4. xóa các thư mục skill public cũ sau khi router mới đã usable
5. xóa hẳn `dream`
6. xóa hẳn `skill-catalog.json`
7. đổi public docs từ “nhiều skill” sang “một `/pulse` với subcommands”

### Ghi chú quan trọng

Đây là phase có blast radius lớn nhất vì nó đụng tới:

- command surface
- docs product
- onboarding language
- tests/eval corpus
- plugin metadata

### Done khi

- public surface chỉ còn `/pulse`
- không còn packaged skill public cũ
- `dream` và `skill-catalog.json` không còn tồn tại

---

## Phase 5 — Sửa hooks, eval, benchmark, và tests

### Mục tiêu

Bỏ mọi enforcement và test corpus đang kéo repo quay lại contract cũ.

### File chính

- `hooks/pre-tool-use.mjs`
- `.codex/hooks/pulse_pre_tool_use.mjs`
- `hooks/session-start.mjs`
- `.codex/hooks/pulse_session_start.mjs`
- `scripts/pulse-plugin-eval.mjs`
- `.plugin-eval/benchmark.json`
- `pulse-eval-workspace/evals.json`
- `docs/evaluation/pulse-plugin-eval.md`
- test files cho onboarding/runtime/router

### Việc cần làm

1. bỏ `bv`-specific hook guard
2. update session-start bootstrap language sang `/pulse onboard`
3. update eval corpus sang `/pulse <command>`
4. update tests cho:
   - router `/pulse`
   - runtime layout `.pulse/runtime/*`
   - workgraph layout `.pulse/workgraph/*`
   - harness backlog materialization
5. bỏ mọi assumption về:
   - `preflight`
   - `dream`
   - `skill-catalog.json`
   - `br`
   - `bv`

### Done khi

- hooks, tests, và eval cùng phản ánh single-skill architecture mới
- repo không còn rule nào kéo user quay về surface cũ

---

## Phase 6 — Migration docs, cleanup, và audit cuối

### Mục tiêu

Khóa lại migration bằng tài liệu rõ ràng và cleanup repo-wide.

### File nên tạo/cập nhật

- `README.md`
- `docs/ARCHITECTURE.md`
- `docs/examples/golden-path.md`
- migration blueprint docs
- nếu cần, `SPEC.md`

### Nội dung bắt buộc

1. mô tả rõ `/pulse` là command router duy nhất
2. mô tả rõ `pulse-work` là runtime CLI riêng
3. mô tả rõ vị trí của:
   - `skills/pulse/references/HARNESS.md`
   - `skills/pulse/templates/HARNESS_BACKLOG.md`
   - `.pulse/harness/HARNESS_BACKLOG.md`
4. map legacy concepts sang contract mới
5. backup brownfield docs nếu cần trước các đợt restructure lớn

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

## P0 — phải chạm sớm

- `SPEC.md`
- `skills/pulse/SKILL.md`
- `skills/pulse/references/HARNESS.md`
- `skills/pulse/templates/HARNESS_BACKLOG.md`
- `skills/pulse/scripts/command-metadata.json`
- `skills/pulse/scripts/runtime/pulse_work.mjs`
- `skills/pulse/commands/onboard/scripts/onboard_pulse.mjs`
- `skills/pulse/scripts/runtime/pulse_state.mjs`
- `skills/pulse/scripts/runtime/pulse_status.mjs`
- `skills/pulse/scripts/runtime/pulse_session_context.mjs`
- `skills/pulse/scripts/runtime/pulse_reservations.mjs`

## P1 — rewrite / migrate ngay sau khi P0 ổn

- `skills/using-pulse/**`
- `skills/exploring/**`
- `skills/planning/**`
- `skills/validating/**`
- `skills/swarming/**`
- `skills/executing/**`
- `skills/reviewing/**`
- `skills/compounding/**`
- `skills/brainstorming/**`
- `skills/architecture-rescue/**`
- `skills/systematic-debug-fix/**`
- `skills/dev-note/**`
- `skills/dev-note-distil/**`
- `skills/preflight/**`
- `skills/dream/**`
- `README.md`
- `AGENTS.md`
- `AGENTS.template.md`
- `CLAUDE.md`
- `docs/ARCHITECTURE.md`
- `docs/examples/golden-path.md`
- `CONTRIBUTING.md`

## P2 — khóa release cuối

- `hooks/pre-tool-use.mjs`
- `hooks/session-start.mjs`
- `scripts/pulse-plugin-eval.mjs`
- `.plugin-eval/benchmark.json`
- `pulse-eval-workspace/evals.json`
- `docs/evaluation/pulse-plugin-eval.md`
- migration blueprint docs

---

## 9. Kiểm thử và verification plan

## 9.1 Router / command-surface tests

Cần test cho:

- `/pulse` không args → render command menu đúng
- `/pulse onboard` → load đúng command reference
- `/pulse brainstorm` → route đúng
- `/pulse plan` → route đúng
- `/pulse swarm` → route đúng
- `/pulse rescue` → route đúng
- `/pulse systematic-debug` → route đúng
- `/pulse note` → route đúng
- `/pulse note-distill` → route đúng
- first word không match command → fallback behavior hợp lệ

## 9.2 Runtime / workgraph tests

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

## 9.3 Integration / smoke tests

1. bootstrap repo bằng `/pulse onboard`
2. onboarding tạo đúng layout `.pulse/runtime/*`, `.pulse/workgraph/*`, `.pulse/harness/HARNESS_BACKLOG.md`
3. `pulse-work create` cho epic/story/task/bug
4. `pulse-work dep add` / `dep rm`
5. `pulse-work ready --json`
6. `pulse-work close` fail nếu thiếu verification
7. `pulse-work reopen`
8. `pulse-work doctor --json`
9. `node .pulse/scripts/pulse_status.mjs --json`
10. `node scripts/pulse-plugin-eval.mjs analyze`
11. verify repo không còn expose `pulse:preflight`, `pulse:using-pulse`, `pulse:dream`

## 9.4 Repo audit checks

Sau khi gần xong, chạy repo-wide audit để bảo đảm:

- không còn hard requirement `br` / `bv`
- không còn public route multi-skill cũ
- không còn `skill-catalog.json`
- không còn `history/` là active work source trong runtime/contracts
- không còn top-level runtime state files cũ là canonical surfaces

---

## 10. Thứ tự thực hiện khuyến nghị theo changeset

### Changeset A — Kiến trúc nền

- cập nhật `PLAN.md`
- cập nhật `SPEC.md`
- làm rõ ranh giới plugin repo vs downstream/self-hosted target repo
- đồng bộ plugin manifests nếu cần

### Changeset B — Router `/pulse`

- tạo `skills/pulse/SKILL.md`
- tạo command modules dưới `skills/pulse/commands/`
- tạo `references/HARNESS.md`
- tạo `templates/HARNESS_BACKLOG.md`
- tạo `command-metadata.json`

### Changeset C — `pulse-work` engine

- dựng workgraph modules dưới `skills/pulse/scripts/runtime/`
- schema
- doctor
- views
- runtime CLI surface

### Changeset D — Runtime / onboard migration

- port scripts từ `skills/using-pulse/scripts/` sang `skills/pulse/scripts/runtime/` và `skills/pulse/commands/onboard/scripts/`
- runtime paths sang `.pulse/runtime/*`
- materialize `.pulse/harness/HARNESS_BACKLOG.md`
- thay bootstrap bằng `/pulse onboard`

### Changeset E — Collapse skill cũ

- migrate nội dung skill cũ vào `skills/pulse/commands/` và shared refs
- xóa `preflight`
- xóa `dream`
- xóa `skill-catalog.json`
- xóa public skill directories cũ

### Changeset F — Docs / hooks / eval / audit cuối

- rewrite README / AGENTS / CLAUDE / docs
- update hooks
- update tests
- update benchmark/eval corpus
- cleanup grep pass

---

## 11. Định nghĩa hoàn thành

Plan này được coi là đạt mục tiêu khi repo có thể chứng minh đồng thời:

- user chỉ cần một entrypoint public là `/pulse`
- command behavior được tổ chức rõ theo `skills/pulse/commands/<command>/`
- `pulse-work` tồn tại như runtime CLI riêng, có vị trí rõ ràng trong source tree ở `skills/pulse/scripts/runtime/`
- bootstrap bằng `/pulse onboard`, không cần `pulse:preflight` hay `pulse:using-pulse`
- không còn packaged skill `dream`
- không còn `skill-catalog.json`
- `HARNESS.md` nằm đúng ở `skills/pulse/references/HARNESS.md`
- `HARNESS_BACKLOG.md` nằm đúng ở `skills/pulse/templates/HARNESS_BACKLOG.md` và materialize vào `.pulse/harness/HARNESS_BACKLOG.md`
- workgraph metadata, runtime state, và command router cùng phản ánh một contract v2 thống nhất

---

## 12. Kết luận ngắn

Hướng mới của Pulse không còn là “nâng cấp một bộ nhiều skill public”, mà là:

- **collapse về một skill router duy nhất là `/pulse`**
- **dời runtime thật sang `pulse-work` + `.pulse/workgraph` + `.pulse/runtime`**
- **giữ các capability mạnh như brainstorm, rescue, systematic-debug, note dưới router đó**
- **loại bỏ các lớp dư thừa như `preflight`, `dream`, `skill-catalog.json`**

Đây là hướng product hóa sạch hơn, dễ dạy hơn, và hợp bản chất workflow system của Pulse hơn so với mô hình hiện tại.