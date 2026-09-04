# Handoff: Pulse — Bước 2, cắt code theo PRODUCT.md

## Bối cảnh

Repo: `/Users/quannv.dev/Workspace/Personal/pulse`, nhánh `features/harness-experimental`,
HEAD `30c4964` ("docs: narrow scope to truth layer, add PRODUCT.md and ADR 0008").
Working tree sạch.

Ngày 2026-09-05 sản phẩm đã được chốt lại. Đọc theo thứ tự trước khi làm gì:

1. `AGENTS.md` — quy tắc vận hành, validation gate, test layout.
2. `PRODUCT.md` — nguồn sự thật về sản phẩm. Đặc biệt §5 (tính năng), §8 (so với
   code hiện tại), §9 (triage code: Spine / Supporting / Frozen / Remove), §13
   (quyết định).
3. `docs/decisions/0008-narrow-scope-to-truth-layer.md`.

Tóm tắt: Pulse là CLI local truth layer cho developer dùng coding agent trong một
repo (work graph, packet, runner, docs, evidence gate, ratchet, event log).
**Không daemon, không agent runtime, không test runtime, không broker.** `pulse-reboot/`
và `PULSE_REBOOT.md` đã xoá; `proposals/` đã chuyển vào `design/archive/proposals/`.

Trạng thái code: ~50.6k dòng Rust production, ~36.5k dòng test, 661 test pass.
Chưa từng chạy thật trên repo nào. Kế hoạch tổng: Bước 2 cắt code (session này) →
Bước 3 sửa ceremony trong spine → Bước 4 runner → Bước 5 `examples/todolist/` và
golden path.

## Quy tắc bắt buộc

- Không bao giờ chạy `pulse` với `--repo-root .` tại gốc repo Pulse. Smoke test
  thì copy `tests/fixtures/target-repos/minimal-service/` ra `mktemp -d`.
- Không thêm feature. Session này chỉ xoá và dọn.
- Mỗi mục dưới đây là một commit riêng. Sau mỗi commit phải xanh cả ba gate:

  ```bash
  cargo fmt --check
  cargo clippy --all-targets --quiet -- -D warnings
  cargo test --all-targets
  ```

  `cargo test --all-targets` chạy ở default threading; không hạ `--test-threads`.
- Commit message dạng `refactor: remove <thứ gì>` hoặc `chore: …`, kết bằng dòng
  `Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>`.
- Khi xoá một module, xoá luôn test, fixture, dependency và mục docs chỉ nó dùng.
  Không để lại `#[allow(dead_code)]` hay stub.
- Nếu một test đang dùng thứ bị xoá để kiểm tra hành vi Core còn giữ, viết lại
  test đó để gọi Core API trực tiếp, không xoá coverage của Core.
- Không sửa `PRODUCT.md` trừ khi phát hiện nó sai với thực tế code; nếu sửa, ghi
  rõ trong báo cáo cuối.

## Việc cần làm, theo thứ tự

### 2.1 Xoá daemon runtime

Trước khi xoá: `git mv src/daemon/application/assignment.rs design/archive/daemon-assignment-saga.rs`
và thêm 3 dòng đầu file ghi "archived 2026-09, reference for runner lease/crash
semantics, not compiled". Thêm một dòng vào `design/archive/README.md`.

Xoá:

- `src/daemon/` toàn bộ; `pub mod daemon;` trong `src/lib.rs`.
- `src/cli/daemon.rs`; các variant `Daemon`, `Project`, `Workspace`, `Session`
  trong `src/cli/args.rs` và nhánh dispatch trong `src/cli/mod.rs`. Option toàn
  cục `--idempotency-key` trong `args.rs`: giữ nếu Core command nào còn đọc nó
  (kiểm tra `cli/work.rs`), nếu không thì xoá.
- `tests/daemon.rs`, `tests/daemon/`.
- `tests/fixtures/fake_codex_*.mjs` (4 file), `tests/fixtures/target-repos/playwright-service/`
  và dòng exception của nó trong `.gitignore`. Cập nhật `tests/fixtures/target-repos/README.md`.
- `windows-sys` trong `Cargo.toml` (`[target.'cfg(windows)'.dependencies]`).
  `libc` **giữ** vì `src/storage/atomic.rs` còn dùng. Kiểm tra `rand`, `uuid`,
  `hex`, `base64` còn ai dùng không; xoá dep nào không còn dùng.
- Trong `src/execution.rs`, `src/reservation.rs`, `src/kernel/reservation.rs`,
  `src/kernel/completion.rs`: kiểm tra có import hay type nào chỉ daemon dùng
  không (ví dụ runtime binding, provider handle). Type nào Core còn cần cho
  lease/handoff/verification thì giữ.

Test phải sửa (không xoá coverage):

- `tests/public_api_contract.rs`: xoá `daemon_application_public_paths_compile`,
  `daemon_is_the_only_runtime_lifecycle_authority`, và phần cuối file (khoảng
  dòng 1574) dùng `pulse::daemon::*`.
- `tests/graph/architecture_guards.rs`: xoá assertion về daemon; giữ assertion
  "Core never imports daemon" dưới dạng "không tồn tại `src/daemon`".
- `tests/graph/reservation.rs`: hiện có ~95 dòng dùng `DaemonApplication` để
  drive reservation. Viết lại để gọi thẳng `pulse::kernel::reservation` (reserve,
  acknowledge, activate, release). Đây là phần tốn công nhất của 2.1.
- `tests/graph/assignment_fixture.rs`: giữ, là helper dựng Ticket ready; bỏ
  phần nào liên quan daemon.

Docs phải sửa sau khi xoá: `README.md` mục Status (bỏ câu "daemon exists in the
tree"), `AGENTS.md` (bỏ mục `src/daemon/: frozen` và rule 4), `CONTRIBUTING.md`
(bỏ dòng daemon). `docs/decisions/0005`, `0006` giữ nguyên làm lịch sử.

### 2.2 Cắt QA về contract chạy lệnh

Trong `src/qa/executor.rs` **giữ**: `QaExecutorManifest` (chỉ `id`, `version`,
`executable`, `args`, `timeout_seconds`, `max_output_bytes`, `capabilities`),
`QaRunnerInput` (bỏ `qualification`, `environment`), `QaRunnerOutput` (bỏ
`browser`, `cleanup_passed`), `QaCaseObservation`, `QaRunnerArtifact`. Đây là
mầm của `runner/` ở Bước 4, đừng xoá.

**Xoá**: `QaExecutorKind` (chỉ còn một kind thì bỏ enum), `QaBrowserManifest`,
`QaBrowserEngine`, `QaBrowserReport`, `QaBrowserAssertion`, `QaEnvironmentManifest`,
`QaEnvironmentCommand`, `QaEnvironmentStepOutput`, `QaRunnerQualification`, các
trường `environment_profile`, `fixture_revision`, `environment`, `browser` trong
manifest.

Trong `src/qa/receipt.rs`: xoá `QaRuntimeEnvironment`, `QaEnvironmentIdentity`,
`QaDeploymentIdentity`, `QaEnvironmentLifecycle`, `QaFlakyWaiver`; gộp payload
version về một version hiện hành (đọc được payload cũ không cần nữa vì chưa có
user). Receipt `qa_checkpoint` giữ: baseline hash, result, cases với status và
artifact hash, `qa_scope` (`ticket_checkpoint` | `story_close`).

Trong `src/kernel/story_completion.rs` và `src/kernel/completion.rs`: **giữ**
`close_story` (Story close gate là product, xem PRODUCT.md §5.5). **Xoá** matrix
entry per platform, retry lineage validator (`previous_attempt_receipt_id`,
`attempt`), flaky waiver và grant `qa.flaky.waive`, `qa.non_applicable.approve`.
Story close cần: Story `ready`, mọi child terminal, không hard blocker, một
receipt `qa_checkpoint` scope `story_close` passed trên HEAD hiện tại cover đủ
case required, closing actor khác QA actor. Rule flaky mới (pass sau fail trên
cùng source = flaky, chặn close cho đến khi human ghi lý do vào close receipt)
để Bước 3, session này chỉ xoá cơ chế waiver cũ và để flaky chặn close.

Trong `src/qa/baseline.rs`: xoá `matrix`, `non_applicable_approval`; case còn
`id`, `revision`, `intent`, `steps` (đổi tên từ `actions`), `expected` (từ
`expected_observations`), `surface`, `priority`, `risk_refs`, `applicability`
với `non_applicable_reason`. Nếu đổi tên field làm test fixture `qa.md` trong
`tests/` phải sửa nhiều, được phép giữ tên cũ session này và ghi vào báo cáo.

Xoá schema: `src/schema/evidence/qa-checkpoint-browser*.schema.json`,
`qa-checkpoint-lifecycle.schema.json`, `receipt-envelope-qa.schema.json` nếu
chỉ browser dùng.

Test: `tests/graph/story_completion.rs`, `tests/evidence/evidence_receipts.rs`,
`tests/graph/assignment_fixture.rs` sửa theo; xoá test về matrix/flaky/browser;
giữ test Story close cơ bản.

### 2.3 Dọn schema, shim, eval, bench

- JSON schema nhúng: `jsonschema` crate chỉ được gọi ở `src/work_packet.rs` và
  `src/reservation.rs`. Các schema `src/schema/docs/*`, `src/schema/evidence/*`,
  `src/schema/knowledge/*`, `src/schema/policy/*` được ghi ra `.pulse/**/schemas/`
  lúc init và pin hash trong manifest nhưng không validate gì. Xoá chúng, xoá
  việc ghi ra đĩa trong `graph/store/bootstrap.rs`, `evidence/manifest.rs`,
  `docs/manifest.rs`, `knowledge/manifest.rs`, xoá field hash tương ứng trong
  manifest. Giữ `node.schema.json`, `edge.schema.json` nếu bootstrap dùng để
  validate; giữ `work-packet.schema.json` và `capability-inventory.schema.json`
  vì đang được validate thật. Sửa test kỳ vọng danh sách file `init` tạo ra
  (`tests/target_repo/repository_init.rs`).
- 13 shim một dòng `src/graph/{contract,edge,executability,frontier,lifecycle,manifest,node,projection,readiness,rollup,shaping,traversal,validate}.rs`:
  xoá, sửa mọi `use pulse::graph::<name>::` thành đường layered
  (`graph::model::`, `graph::read::`, `graph::validation::`). Cập nhật
  `tests/public_api_contract.rs` cho đúng.
- `src/docs/eval.rs`, `tests/docs/docs_retrieval_eval.rs`, `benches/docs_retrieval.rs`,
  `[[bench]]` trong `Cargo.toml`, `src/schema/docs/retrieval-eval.schema.json`,
  file tracked `.pulse/docs/retrieval-evals/core.jsonl` ở gốc repo: xoá. Cập
  nhật `docs::mod` export và câu "Optional retrieval benchmark" nếu còn ở đâu.
- File legacy tracked ở gốc: `.pulse/workgraph/items.jsonl`, `.pulse/workgraph/schema.json`
  (thời Node). Xoá, và xoá câu nhắc về chúng trong `AGENTS.md`.
- `src/docs/cache.rs` giữ (supporting), chỉ xoá nếu không còn ai gọi sau khi
  bỏ eval.

### 2.4 ARCHITECTURE.md

Sau khi 2.1–2.3 xanh, viết `ARCHITECTURE.md` ở gốc, tiếng Việt hoặc Anh đều
được, 150–250 dòng: mô tả **đúng cái còn lại** theo module, dependency direction,
plane dữ liệu trên đĩa, test layout, và một bảng "chưa có, xem PRODUCT.md §5.x"
cho runner, ratchet commands, events tail/note, MCP. Không mô tả target design;
đó là việc của PRODUCT.md. Cập nhật mục Source architecture trong `AGENTS.md`
cho khớp và bảng "What works today" trong `README.md`.

## Kết quả mong đợi

- Khoảng 25–30k dòng production, 400–450 test, tất cả xanh.
- `cargo build` không còn `daemon`, `windows-sys`, `jsonschema` chỉ còn 2 chỗ.
- CLI `pulse --help` không còn `daemon`, `project`, `workspace`, `session`.
- Smoke: copy `minimal-service` ra temp, chạy `pulse init`, `work create --kind
  ticket …`, `graph validate`, `docs index`, `docs search` vẫn chạy.

## Báo cáo cuối session

Ghi rõ: danh sách commit (hash + message), số dòng và số test trước/sau, test
nào đã viết lại thay vì xoá, dependency đã gỡ, điều gì trong PRODUCT.md phát
hiện sai so với code, việc để lại cho Bước 3 (đặc biệt: default grants khi
init, `ticket.md` thành nguồn contract thay JSON 25 trường, close gate cho mọi
risk hiện chỉ có `Risk::Low` ở `kernel/completion.rs` khoảng dòng 658, section
`not_installed` trong `work_packet.rs`, docs metadata 17 → 8 trường + tags,
`source.rs` strip `git rev-parse --show-prefix` khi repo-root là thư mục con).
