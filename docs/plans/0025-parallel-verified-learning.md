# Plan 0025 — Song song theo graph, bằng chứng tự quan sát, review panel, vòng học khép kín

Trạng thái: **đã thực thi A–F, G1, G2; G3 hoãn** (xem "Trạng thái thực
thi" cuối file). Viết 2026-09-18 trên nhánh
`features/harness-experimental` sau một phiên đọc code (không tin docs) của
Pulse và ba repo tham chiếu (`references/repo-harness`,
`references/better-harness`, `references/repository-harness`).

Tài liệu này tự đủ: một session mới chỉ cần đọc `AGENTS.md` rồi file này là
làm được. Mọi `path:line` dưới đây đúng tại thời điểm viết — **xác minh lại
bằng cách mở file trước khi sửa**, vì working tree đang có thay đổi chưa
commit (xem Pha 0).

---

## 0. Tư tưởng sản phẩm (chủ repo chốt) và phán quyết so với code

| Trụ cột | Code hôm nay | Khoảng trống |
|---|---|---|
| 1. Chia để trị, chạy song song theo graph, va chạm file thì chờ nhau | `blocked_by` + cycle check + `work list --ready` | `acquire_lease` **cấm** song song (`src/kernel/reservation.rs:77-90`, `run_another_active`); source fence hash cả cây nên hai worker làm stale lẫn nhau; mọi worker cùng tên `agent:worker` |
| 2. Ticket đủ giàu để agent độc lập không lệch | `description` markdown tự do + gate `ready_description_missing` (**đã làm**, chưa commit) | — |
| 3. Documenting để không mất context | gate "doc khai đã sửa phải có diff"; `docs check` | chỉ là gate một chiều; `docs applicable` giá trị thấp |
| 4. Học lại, harness tự cải thiện | learning candidate→active, recall theo glob, đếm usage | friction→learning thủ công (help text nói dối: `src/cli/args.rs`, cờ `--friction`); `Check` chỉ là text; `misleading` không hậu quả |
| 5. Verify nhiều lane, có evidence | lane input bịt narrative, lane không được sửa cây, qa-ui thiếu ảnh → inconclusive, artifact hash vào receipt | handoff tin lời khai (`exit` không được đọc); lane receipt không so `dirty_hash`; packet trả `findings: []` cứng; không có nhiều reviewer cùng một vấn đề |

Nguyên tắc giữ nguyên: **Pulse không dispatch agent, không daemon.** Host
spawn; Pulse trả lời "cái gì chạy được", "ai giữ gì", "cái gì tính là xong".

Không làm (đã cân nhắc và loại): khối `plan` có cấu trúc trong schema;
~30 subcommand kiểu repo-harness; markdown-làm-database; DSL/compiler kiểu
better-harness; hook tư vấn luôn `exit 0` đặt cạnh gate thật; debate tự do
giữa reviewer.

---

## 1. Ba decision phải viết trước khi code pha tương ứng

Viết vào `docs/decisions/` theo format các file 0022–0024 hiện có, thêm vào
`docs/decisions/README.md`. Mặc định đề xuất bên dưới đủ để không bị chặn;
chủ repo có thể đổi.

### Decision 0025 — File reservation và fence theo scope (Pha B)
- **Chốt:** mảng `touches` riêng trên Ticket (không tái dùng
  `context.anchors`: anchors = chỗ đọc, touches = chỗ sửa). Song song trong
  **cùng một checkout**; fence của ticket có `touches` chỉ hash nội dung các
  file khớp `touches`, **không** so `HEAD`.
- **Ticket không có `touches` = độc quyền** (xung đột với mọi ticket khác) —
  giữ đúng hành vi hôm nay cho dữ liệu cũ và cho lối nhanh `risk: low`.
- **Rủi ro chấp nhận:** reserve theo file không chặn được worker A làm hỏng
  build của worker B qua file B chỉ *đọc*. Giảm nhẹ bằng `blocked_by` khi
  plan, và bằng `pulse verify` (Pha D) chạy tại handoff. Phương án bị loại
  lúc này: worktree-mỗi-ticket (đúng hơn về cô lập, nhưng đưa lại worktree
  mirroring mà plan 0022 §10.6 đã cắt; xét lại nếu friction build-gãy-chéo
  > 1 lần / story trong dogfood).
- **Rủi ro chấp nhận 2:** `lane_mutated_workspace` với ticket có `touches`
  chỉ còn phát hiện lane sửa file *trong scope của ticket đó*.

### Decision 0026 — Pulse được chạy argv đã khai (Pha D)
- **Chốt:** `pulse verify <id>` chạy `verify[].argv` của Ticket và ghi
  receipt. Lý do không vi phạm "Pulse không chạy test": (a) đã có tiền lệ —
  `src/docs/check.rs:144` chạy `generated_by.check_argv`; (b) argv đến từ
  record mà chỉ `human:` sửa được (`Action::MutateGraph`,
  `src/kernel/roles.rs`); (c) Pulse không *chọn* chạy gì, chỉ chạy thứ đã
  khai và ghi lại điều nó tự quan sát.
- Sửa câu trong `AGENTS.md`/`ARCHITECTURE.md`/block seed: "does not run
  tests" → "runs only the argv a record declares (`verify[]`,
  `check.argv`), and records what it observed".

### Decision 0027 — Review panel và đối chứng finding (Pha C)
- **Chốt:** mặc định `count: 1` (hành vi hôm nay). Panel là opt-in theo
  profile. Vòng 1 mù. Vòng 2 là **đối chứng từng finding**, không debate:
  finding có `check.argv` do Pulse chạy → kết quả máy thắng mọi phiếu;
  finding không có check cần ≥ `quorum` xác nhận; còn lại hạ severity.
- Seat receipt dùng `kind: "lane_seat"`; chỉ receipt reconcile mới là
  `kind: "lane"` → close gate gần như không đổi.

---

## 2. Thứ tự và phụ thuộc

```
Pha 0 (dọn cây) → A (vá gate) → B (song song) → D (verify tự quan sát)
                                              → C (panel; cần D để chạy check.argv)
                → E (learning) ─ độc lập sau A
                → F (docs)     ─ độc lập sau A
                → G (hook, 3-way merge, eval) ─ sau cùng
```

Mỗi pha = một hoặc vài commit, mỗi commit xanh cả ba lệnh:

```bash
cargo fmt --check
cargo clippy --all-targets --quiet -- -D warnings
cargo test --all-targets        # threading mặc định; không hạ --test-threads
```

Quy ước bắt buộc của repo (đọc `AGENTS.md`): mọi error code mới đi qua
`PulseError::kernel(code, message, hint)` — **code không có hint là bug**;
module có invariant mở đầu bằng `//!`; không `panic!` cho input/FS; không
chạy Pulse mutation với `--repo-root .` trong repo này; fixture bất biến.
Guard cần nhớ: `tests/architecture_guards.rs` —
`agents_block_only_names_commands_the_cli_has`,
`templates_only_name_commands_the_cli_has`,
`skills_only_name_commands_the_cli_has` (⇒ **thêm lệnh CLI trước, rồi mới
nhắc tên nó trong template/skill**), `daemon_runtime_tree_is_absent` (⇒
**không** tạo `src/run.rs`, `src/process.rs`, `pub mod run;` — module chạy
argv đặt tên `kernel::verify`), `kernel_does_not_depend_on_cli`,
`store_issues_does_not_depend_on_kernel_or_cli`.

---

## Pha 0 — Dọn working tree (15 phút, hỏi chủ repo nếu mơ hồ)

`git status` lúc viết plan: `SPEC.md` bị xoá, `design/archive/**` bị xoá,
`AGENTS.md`/`ARCHITECTURE.md`/4 skill/`src/cli/{args,docs,doctor}.rs` bị
sửa (đợt gỡ runner, do chủ repo làm), **cộng** thay đổi của phiên trước:

- `src/schema/issue.schema.json` — thêm `description`
- `src/kernel/ready.rs` — `check_description` + 2 test
- `src/kernel/lane.rs` — `description` vào lane input
- `templates/prompts/{worker,review-correctness}.md`, `skills/pulse-plan/SKILL.md`
- file plan này

Việc: chạy 3 lệnh validation; nếu xanh, đề nghị chủ repo commit thành hai
commit tách bạch (gỡ runner của họ; `feat(ticket): free-form description +
ready gate`). Không tự commit nếu chưa được bảo. `AGENTS.md` trỏ tới
`SPEC.md` đã bị xoá — ghi nhận, hỏi chủ repo có khôi phục không (Pha A–G
cần một nơi ghi spec; nếu `SPEC.md` không về thì ghi vào `ARCHITECTURE.md`
+ decision).

---

## Pha A — Vá gate verify (nhỏ, không đổi kiến trúc)

### A1. Handoff đọc lời khai thay vì chỉ đếm
File: `src/kernel/completion.rs`, `evaluate_handoff` (khoảng dòng 148–261).
- Trong vòng lặp AC: với AC có mặt mà `status != "done"` →
  violation `handoff_acceptance_not_done` ("acceptance {id} is {status},
  not done"). Từ vựng status của handoff chốt là đúng một giá trị `done`
  (worker chưa xong thì checkpoint, không handoff) — cập nhật
  `templates/prompts/worker.md` mục Finishing nói rõ.
- Trong vòng lặp verify: result có mặt mà `exit != 0` →
  `handoff_verify_failed` ("verify {name} exited {exit}").
- Hint nằm ở `fail()` chung (`gate_failed`); không cần hint riêng từng
  violation — giữ nguyên pattern hiện có.
- Test (cùng file, cạnh `handoff_verify_missing_is_reported`):
  `handoff_verify_failed_is_reported`,
  `handoff_acceptance_not_done_is_reported`.
- Pha D sẽ thay nguồn của `exit` bằng receipt `verify`; A1 vẫn đáng làm vì
  rẻ và có ngay.

### A2. Close so cả `dirty_hash` của lane receipt
File: `src/kernel/completion.rs`, khối `match lane_receipt` (~dòng 425–468).
- Sau nhánh so `commit`, thêm nhánh: `receipt.source.dirty_hash !=
  handoff_receipt.source.dirty_hash` → `close_lane_not_satisfied`
  ("{lane}: receipt was sealed on a different tree state than the
  handoff").
- Kiểm tra helper test `seal_passing_lane_receipt` (dùng
  `source::snapshot(repo,&[])`) vẫn khớp handoff trên cây sạch — đang khớp.
- Test mới: `close_lane_not_satisfied_when_lane_dirty_hash_differs`.
- Pha B sẽ tổng quát hoá phép so này thành "so fence của ticket".

### A3. Packet trả findings thật cho worker làm lại
File: `src/kernel/packet.rs`, `last_verdicts` (~dòng 78–95) + `//!` đầu file
(đoạn nói "no caller producing verdict: fail yet" đã lỗi thời — sửa).
- Đổi chữ ký thành `last_verdicts(repo_root, ticket) -> Result<Vec<Value>>`.
  Với mỗi `verdicts[role]`, lấy `receipt` id → `evidence::receipt::
  load_receipt`. Trả `{lane, verdict, commit, findings, failed_acceptance,
  failed_cases}` trong đó `findings` = payload.findings có `status !=
  "resolved"` (giữ nguyên `id/ref/summary/owner/check/severity`),
  `failed_acceptance` = payload.acceptance có `status == "fail"`,
  `failed_cases` = payload.cases có `status != "pass"`.
- Receipt không đọc được → không lỗi cả packet: trả mục đó với
  `"findings_unavailable": true`.
- Test: seal một lane `fail` có finding + check, build packet, assert
  finding xuất hiện.
- `templates/prompts/worker.md`: thêm một đoạn "Rework: đọc
  `last_verdicts[].findings` trước; mỗi finding có `check.argv` là lệnh
  phải pass trước khi handoff lại".

### A4. Gate nguyên tử + `release` kiểm người giữ
Vấn đề: `handoff`/`close`/`close_story`/`seal` làm read → evaluate →
`record_receipt` → `mutate` không chung lock. `WriteGuard` là `flock` trên
fd mới mỗi lần acquire ⇒ **không re-entrant**: gọi `issues::mutate` khi đang
giữ guard sẽ treo 10s rồi `LockTimeout`.
- `src/store/issues.rs`: tách `mutate` thành
  `pub fn mutate_locked<F>(_guard: &WriteGuard, repo_root, transform)`
  (thân hiện tại trừ dòng acquire) và `mutate` = acquire + `mutate_locked`.
- `src/kernel/issues.rs`: `apply_to_record` không đổi; thêm biến thể
  `set_status_locked` nếu cần.
- Trong `handoff`/`close`/`close_story` (`completion.rs`) và
  `validate_and_seal`+`seal` (`lane.rs`), `acquire_lease`/`release`
  (`reservation.rs`): mở một block
  `{ let guard = WriteGuard::acquire(repo_root)?; read_all; evaluate;
  record_receipt; mutate_locked(&guard, …) }` — **đóng block trước** khi gọi
  `append_note`, `learn::record_usage`, `emit_event` nào tự lấy lock (kiểm
  tra: `append_note` dùng `issues::mutate`; `learn::add` lấy `WriteGuard`;
  `record_receipt` và `emit_event` không lấy lock).
- `release` (`reservation.rs` ~dòng 120): từ chối
  `release_not_holder` khi lease còn sống, caller không phải người giữ và
  không phải `human:`. Hint: "wait for expiry (`pulse doctor` lists expired
  leases) or have a human release it".
- Test: hai thread cùng `handoff` một ticket → đúng một receipt handoff
  (mẫu: `concurrent_mutations_from_two_threads_never_lose_a_write` trong
  `src/store/issues.rs`). Test `release_not_holder`.

### A5. Help text nói dối
`src/cli/args.rs`, doc của cờ `friction` trên `Note`: bỏ "(the close gate
turns it into a learning candidate)" → "(surfaced as unclassified friction
at close until a learning or a dismissal cites it)". Chỉ đúng sau E1 — nếu
làm A trước E, tạm ghi "(review it with the pulse-learn skill)".

Commit gợi ý: `fix(gates): handoff reads declared results; close compares
lane tree state; packet carries findings; gates hold one lock`.

---

## Pha B — Song song theo graph + file reservation (Decision 0025)

### B1. Schema + ready gate
- `src/schema/issue.schema.json`: thêm
  `"touches": {"type":"array","items":{"type":"string"}}` — đường dẫn
  repo-relative hoặc glob theo đúng ngữ pháp `source::glob_match`
  (`src/source.rs:93`: exact, `dir/`, `dir/**`, một `*` trong một segment).
- `src/kernel/ready.rs`: `check_touches` — với ticket implementation
  `risk != low`: `touches` rỗng → `ready_touches_missing`; entry tuyệt đối
  hoặc chứa `..` → cùng code (dùng `storage::safe_repo_relative`). Không
  kiểm tra tồn tại (file có thể được tạo mới).
- `src/cli/work.rs`: `Update` đang từ chối field runtime (`lease`,
  `verdicts`, `checkpoints`) — `touches` **không** thuộc nhóm đó (planner
  sửa được).
- `skills/pulse-plan/SKILL.md`: thêm `touches` vào payload mẫu + một gạch
  đầu dòng: "liệt kê mọi file sẽ sửa/tạo; đây là khoá song song — thiếu thì
  worker phải `pulse reserve` giữa chừng, thừa thì chặn ticket khác vô cớ;
  hai ticket cùng `touches` phải có cạnh `blocked_by` hoặc chấp nhận chạy
  tuần tự".

### B2. Phép giao của hai tập pattern
File mới: `src/kernel/scope.rs` (`//!` nêu: thuần hàm, không I/O, phụ thuộc
chỉ `crate::source::glob_match`). Đăng ký trong `src/kernel/mod.rs`.
```rust
/// Whether two `touches` lists can name the same file. Conservative:
/// a false positive serializes two tickets; a false negative lets two
/// workers write one file.
pub fn overlaps(a: &[String], b: &[String]) -> Option<(String, String)>
pub fn covers(touches: &[String], path: &str) -> bool   // any glob_match
```
Quy tắc cặp `(p, q)`: bằng nhau; hoặc một bên không có `*` và
`glob_match(bên kia, nó)`; hoặc cả hai có prefix thư mục (`dir/`, `dir/**`)
và một prefix là tiền tố của prefix kia; hoặc cả hai có `*` và phần literal
trước `*` đầu tiên của một bên là tiền tố của bên kia. Danh sách rỗng ở
**bất kỳ** bên nào = giao (độc quyền). Test bảng: ≥ 12 ca gồm
`src/a.rs` vs `src/a.rs`, `src/**` vs `src/x/y.rs`, `docs/*.md` vs
`docs/a.md`, `src/a.rs` vs `src/b.rs` (không giao), rỗng vs bất kỳ.

### B3. Claim theo reservation
File: `src/kernel/reservation.rs`, `acquire_lease`.
- "Tập đang giữ" = mọi ticket khác có (`status == active` **và** lease còn
  sống) **hoặc** `status == verifying`. (File phải được giữ qua review:
  close so fence với handoff; nếu người khác sửa file đó trong lúc
  verifying thì close stale mãi.)
- Thay khối `run_another_active` bằng: với mỗi ticket đang giữ,
  `scope::overlaps(mine, theirs)` → lỗi
  `claim_files_reserved` — message: "{other} ({status}, {actor}) holds
  {pattern}"; hint: "wait for {other} to close, or add a blocked_by edge;
  `pulse frontier` lists what can run now". **Xoá** code
  `run_another_active` (grep: chỉ còn trong `reservation.rs` +
  `docs/plans/0022-*.md` lịch sử — không sửa plan cũ, ghi vào
  `docs/plans/0022-error-code-audit.md` một dòng "0025: thay bằng
  claim_files_reserved" nếu file đó là sổ sống).
- **Định danh worker:** hai worker song song không được cùng
  `agent:worker`. `acquire_lease` từ chối `claim_actor_busy` khi actor gọi
  đang giữ lease sống trên ticket *khác*. Hint: "give each parallel worker
  its own actor: --actor agent:worker-<n>". (`roles.rs::is_lane_role` đã
  coi mọi agent id không có tiền tố lane là worker — không phải sửa.)
- **Ràng run_id (thay cho "D2 session token" — xem ghi chú cuối pha D):**
  `HandoffInput` thêm `run_id: String` (bắt buộc); `evaluate_handoff` so
  với `lease.run_id` → `handoff_lease_mismatch`. `checkpoint` đã có
  `run_id` trong input — thêm phép so tương tự
  (`checkpoint_lease_mismatch`). Cập nhật `worker.md` (handoff.json có
  `run_id`), fixture `full_handoff()` và mọi test dựng handoff
  (`tests/golden_path.rs`, `tests/lane.rs`, `tests/target_repo/*`).

### B4. `pulse reserve`
- CLI: `src/cli/args.rs` thêm
  `Reserve { id, paths: Vec<String>, --actor, --json }`; handler trong
  `src/cli/lease.rs`; route trong `src/cli/mod.rs`.
- Kernel: `reservation::reserve(repo_root, actor, id, paths)` — yêu cầu
  caller giữ lease sống của `id` (`reserve_lease_mismatch`); path an toàn;
  giao với tập đang giữ của ticket khác → `claim_files_reserved` (cùng
  code, cùng hint + "checkpoint and stop; reclaim after {other} closes");
  ngược lại append vào `touches` (dedup), `bump`, event `lease.reserved`
  `{paths}`. `Action::CheckpointOrHandoff` là quyền phù hợp.
- "Chờ" trong hệ không daemon: worker checkpoint + dừng; host theo dõi
  `pulse events tail --follow --json` tìm `issue.transitioned {to: done}`
  / `{released: true}` của ticket chặn rồi spawn lại. Ghi đúng như vậy vào
  `worker.md` ("Blocked on a reserved file").

### B5. `pulse frontier`
- CLI: `Frontier { story: Option<String>, --json }` (top-level, cạnh
  `Claim`). Kernel: `src/kernel/frontier.rs`:
  1. ứng viên = ticket `status == ready` (đã qua ready gate ⇒ deps xong),
     lọc theo `story` nếu có; sắp theo `id` cho tất định;
  2. loại ứng viên giao với tập đang giữ (B3) → vào `waiting[]` kèm
     `{id, blocked_on, pattern}`;
  3. tham lam: duyệt theo thứ tự, nhận ứng viên không giao với các ứng viên
     đã nhận → `runnable[]`; còn lại vào `waiting[]` với `blocked_on` là
     ticket runnable nó giao.
  Output `{runnable:[{id,title,surface,risk,touches}], waiting:[…],
  held:[{id,status,actor,touches}]}`. Read-only, không lock.
- Test đơn vị trên records dựng tay; test tích hợp trong
  `tests/golden_path.rs` hoặc crate mới `tests/parallel.rs` (nhớ thêm vào
  danh sách crate trong `AGENTS.md` mục Test layout).

### B6. Fence theo scope
File: `src/source.rs`.
```rust
/// Content fence for one ticket: every file matching `touches`, tracked or
/// not, hashed by path + bytes (absent file hashes as a tombstone).
/// Independent of HEAD and of any path outside `touches`.
pub fn scoped_snapshot(repo_root: &Path, touches: &[String]) -> Result<Source>
```
- Liệt kê: `git ls-files` ∪ `git ls-files --others --exclude-standard`,
  lọc `glob_match`, sort, hash `path\0bytes\0`. `commit` = HEAD (chỉ để
  đọc, **không** dùng để so); `dirty_hash` = `"scope:sha256:…"` (tiền tố
  khác để không bao giờ bằng nhầm hash cả-cây); `dirty_paths` = các path
  trong scope đang dirty.
- Một điểm vào duy nhất cho mọi gate, đặt ở `src/kernel/profile.rs` hoặc
  `scope.rs`:
  `pub(crate) fn fence_for(repo_root, record) -> Result<Source>` —
  `touches` không rỗng → `scoped_snapshot`; rỗng (ticket cũ, Story) →
  `snapshot(repo_root, &fence_ignore)` như hôm nay.
- Thay mọi chỗ gọi `source::snapshot` trong gate bằng `fence_for`:
  `completion.rs` (`handoff`, `evaluate_close`, `close`), `lane.rs`
  (`prepare`, `validate_and_seal`), `checkpoint.rs`. `close_story` giữ
  snapshot cả cây (story milestone phải sạch).
- Phép so tại close: ticket có `touches` → chỉ so `dirty_hash` (bỏ so
  `commit` cho cả `close_source_stale` lẫn nhánh lane receipt của A2);
  ticket không `touches` → như cũ.
- `lane_commit_mismatch` (`lane.rs` ~dòng 413): giữ — lane khai commit nó
  chạy; nếu ticket khác commit giữa chừng thì seal lại sau khi chạy lại.
  Ghi friction nếu dogfood gặp nhiều.
- **Sửa ngoài scope không được lọt review:** `evaluate_handoff` thêm
  `handoff_unreserved_changes`: mọi dirty path (sau `fence_ignore`) không
  được `scope::covers` bởi `touches` của **bất kỳ** ticket `active |
  verifying | done` → liệt kê. Hint: "`pulse reserve {id} <path>` if it is
  yours, revert it if not". (`done` được tính vì file của ticket đã close
  có thể đang chờ commit.) Chỉ áp dụng khi ticket đang handoff có
  `touches`.
- `lane_input.changed_files` (`lane.rs::changed_files_since`): lọc thêm
  theo `covers(touches, path)` khi ticket có `touches`, để reviewer không
  chấm file của worker khác.
- Commit sau close: ghi vào block seed (`templates/seeds/agents-block.md`)
  quy ước host: ngay sau `pulse close <id>`, `git add -- <touches> && git
  commit`. `pulse doctor` thêm check "dirty path chỉ thuộc ticket `done`"
  → cảnh báo `awaiting_commit` (`src/kernel/doctor.rs`, theo mẫu
  `ExpiredLease`).

### B7. Bề mặt chữ
Sau khi lệnh tồn tại: `templates/seeds/agents-block.md` (bảng lệnh thêm
`frontier`, `reserve`; đoạn "Working a Ticket" thêm vòng song song: `pulse
frontier` → spawn mỗi ticket một worker actor riêng → close → commit),
`templates/prompts/worker.md`, `skills/pulse-plan/SKILL.md`,
`ARCHITECTURE.md` (module mới `kernel::scope`, `kernel::frontier`),
`README.md` nếu có bảng lệnh.

### B8. Dogfood (bắt buộc trước khi coi B là xong)
Trên `~/Workspace/Personal/todolist`: một Story ≥ 3 ticket, trong đó 2
ticket rời nhau (api vs web) và 1 ticket đụng file của ticket kia. Kỳ vọng:
`frontier` trả 2 runnable + 1 waiting; hai worker chạy song song, cả hai
close được **không** có `close_source_stale`; ticket thứ ba claim được sau
khi ticket chặn nó done. Mọi trục trặc → `pulse note <id> "…" --friction`
và một bảng F-số trong `docs/plans/0025-dogfood.md` theo mẫu
`0022-dogfood-st1.md`.

Commit gợi ý (4): schema+ready+scope · claim/reserve/frontier · scoped
fence + unreserved gate · templates/skills/docs.

---

## Pha D — Bằng chứng do Pulse tự quan sát (Decision 0026)

### D1. `pulse verify <id>`
- Module: `src/kernel/verify.rs` (**không** đặt tên `run`). `//!`: chạy
  đúng argv record khai, không shell, không chọn lệnh, ghi điều quan sát.
- CLI: `Verify { id, --timeout <secs, default 900>, --actor, --json }`.
- Luồng: đọc ticket; `verify[]` rỗng → `verify_nothing_declared`. Với mỗi
  entry theo thứ tự: `Command::new(argv[0]).args(..).current_dir(repo_root
  .join(cwd))`, `cwd` qua `safe_repo_relative`; stdout+stderr gộp, giữ
  **64 KiB cuối** (theo tinh thần Decision 0024 "bounded tails"), đưa qua
  `evidence::redaction` (đọc `src/evidence/redaction.rs` để dùng đúng API;
  nếu API chỉ nhận JSON thì bọc log vào `Value::String` rồi
  `clean_json_strings`), ghi `.pulse/evidence/<id>/verify/<name>.log`;
  timeout → kill, `exit: null, timed_out: true`. Không dừng ở lệnh fail
  đầu tiên (báo hết, như các gate).
- Receipt: `kind: "verify"`, `source = fence_for(ticket)` **chụp sau khi
  chạy xong**, payload `{results:[{name,argv,cwd,exit,timed_out,
  duration_ms,log}]}`, `artifact_paths` = các file log (được hash sẵn bởi
  `record_receipt`). Event `verify.recorded {passed: bool}`.
- Quyền: thêm `Action::Verify` trong `roles.rs` — mọi actor human/agent
  (worker chạy trước handoff; lane chạy lại độc lập).
- Exit code của CLI: 0 nếu mọi result `exit == 0`, ngược lại non-zero với
  `verify_failed` (để script/host gate được).

### D2. Handoff và close đọc receipt verify
- `evaluate_handoff`: nếu ticket có `verify[]` → cần receipt `verify` mới
  nhất của ticket với `source.dirty_hash == fence_for(ticket).dirty_hash`
  (`handoff_verify_stale`: "tree changed since the last `pulse verify`")
  và mọi result `exit == 0` (`handoff_verify_failed`). Khi đó
  `verify_results` trong handoff.json **không còn là nguồn sự thật**: giữ
  field cho tương thích (`deny_unknown_fields` vẫn bật), bỏ
  `handoff_verify_missing`, và cập nhật `worker.md`: "chạy `pulse verify
  <id>` thay vì tự khai exit code".
- Lane: `review-correctness.md` đổi "Re-run every verify[] yourself" →
  "chạy `pulse verify <id> --actor agent:<lane>`; receipt của chính bạn là
  bằng chứng". `apply_seal_corrections` (`lane.rs`): lane `review-*` báo
  `pass` mà không có receipt `verify` của **chính actor đó** trên fence
  hiện tại → hạ `inconclusive` (đối xứng với luật "qa-ui pass không ảnh").
  Chỉ áp dụng khi ticket có `verify[]`.
- Test: crate `tests/lane.rs` + unit trong `verify.rs` dùng argv `true` /
  `false` / `sh -c 'sleep 5'` với timeout 1s.

### Ghi chú về "danh tính" (D2 cũ trong danh sách chốt)
Luật TTY cho `human:` **bị loại**: các skill `pulse-plan`/`pulse-shape`
chạy lệnh với `--actor human:<name>` từ trong session agent, luật đó sẽ
phá chúng. Thay bằng ba thứ rẻ và thật: ràng `run_id` (B3),
`claim_actor_busy` (B3), seat actor phải khác nhau (C2). Ghi thẳng vào
Decision 0026 phần "Không giải quyết": một session cố tình đổi `--actor`
vẫn tự review được; chống gian lận có chủ đích cần hook host (Pha G1).

---

## Pha C — Review panel + đối chứng finding (Decision 0027)

### C1. Cấu hình
`src/kernel/profile.rs`: `Profile` thêm
`#[serde(default)] pub panels: BTreeMap<String, Panel>` với
`Panel { count: u32, quorum: u32 }`; validate khi load: `1 <= quorum <=
count`, role phải có trong `lanes` → `pulse_md_invalid`. Seed
`templates/seeds/PULSE.md`: **không** bật mặc định; thêm comment mẫu
`# api-high: {lanes: [...], panels: {review-correctness: {count: 3,
quorum: 2}}, human: required}`.

### C2. Seat
- CLI: `lane input`/`lane seal` thêm `--seat <n>` (1-based). Không có panel
  mà truyền `--seat` → `lane_seat_unexpected`; có panel mà thiếu →
  `lane_seat_required`.
- Đường dẫn: input `.pulse/runtime/lane/<id>/<role>.<n>-input.json`,
  snapshot `<role>.<n>.snapshot.json`, output
  `.pulse/evidence/<id>/<role>.<n>.json` (cùng schema `LaneOutput`).
- `seal` cho seat: receipt `kind: "lane_seat"`, payload thêm
  `{seat, handoff: <id receipt handoff mới nhất>}`; **không** ghi
  `verdicts`, **không** bounce ticket khi `fail`. Từ chối
  `lane_seat_actor_reused` nếu một seat khác cùng `handoff` đã seal bởi
  cùng actor. Quy ước actor: `agent:<role>-<n>` (qua `is_lane_role` vì
  cùng tiền tố).
- Input seat y hệt lane input thường (mù: không thấy seat khác). Test
  khẳng định input seat 2 không chứa gì từ output seat 1.

### C3. `pulse lane reconcile`
- CLI: `LaneCommand::Reconcile { id, role, --prepare, --actor, --json }`.
- `--prepare`: cần đủ `count` seat receipt cùng `handoff` hiện tại
  (`reconcile_seats_missing`, liệt kê seat thiếu). Ghi
  `.pulse/runtime/lane/<id>/<role>-reconcile-input.json` = lane input gốc +
  `findings: [{rid:"RF-1", ref, summary, owner, severity, check}]` gộp từ
  mọi seat, **bỏ** seat/actor, xáo theo thứ tự `rid` tất định (sort theo
  `ref` rồi `summary`) + `acceptance_split: [{id, pass: n, fail: m}]`.
  Pulse không tự khử trùng lặp ngữ nghĩa.
- Vòng 2: mỗi seat ghi `.pulse/evidence/<id>/<role>.reconcile.<n>.json`:
  `{votes:[{rid, vote:"confirmed"|"refuted"|"duplicate", of:"RF-x"?,
  how}]}` — schema đóng, `deny_unknown_fields`. Prompt mới
  `templates/prompts/reconcile.md` (đăng ký trong `kernel/init.rs`
  `ensure_prompts`): "tái hiện từng finding; `confirmed` cần `how` nêu lệnh
  hoặc `path:line`; không bỏ phiếu theo số đông".
- Seal (không `--prepare`), luật cho từng finding:
  1. có `check.argv` → Pulse chạy bằng máy móc của `kernel::verify`
     (log vào `.pulse/evidence/<id>/reconcile/<rid>.log`); exit thực `!=
     check.exit` ⇒ **đứng** (bất kể phiếu); `==` ⇒ `resolved`;
  2. không check: `1 (người nêu) + confirmed >= quorum` ⇒ đứng; `duplicate`
     gộp vào `of`; còn lại ⇒ `severity: low`, `status: "unconfirmed"`.
  AC: `pass` khi ≥ `quorum` seat báo pass, ngược lại `fail`.
  Verdict: `fail` nếu có AC fail hoặc finding `high` đứng; `pass` nếu
  không; sau đó vẫn đi qua `apply_seal_corrections` hiện có (giữ luật
  "fail không có check nào ⇒ inconclusive").
  Receipt `kind: "lane"`, payload như lane thường + `{reconciled: true,
  seats: [receipt ids], votes_summary}`; ghi `verdicts[role]`; `fail` ⇒
  bounce `verifying → active` như `seal` hôm nay (tái dùng code).
- Close gate (`completion.rs`): với role có panel, receipt `lane` phải có
  `payload.reconciled == true` → nếu không, `close_lane_not_satisfied`
  ("{lane}: panel of {count} requires `pulse lane reconcile`").
- `pulse doctor`: seat đã prepare chưa seal đã được bao bởi
  `stale_lane_preparations` nếu giữ đúng thư mục snapshot — kiểm tra glob
  tên file mới `<role>.<n>.snapshot.json` có được nhận.
- Skill `skills/pulse-review/SKILL.md`: thêm mục "Panel seat" và "Round 2".
  Block seed: mô tả vòng panel cho host (spawn `count` lane song song →
  `reconcile --prepare` → spawn vòng 2 → `reconcile`).
- Test: `tests/lane.rs` — (a) 3 seat, một finding có check fail thật, hai
  seat `refuted` ⇒ finding vẫn đứng, verdict `fail`; (b) finding không
  check, chỉ người nêu ⇒ hạ `unconfirmed`, verdict `pass`; (c) thiếu seat
  ⇒ `reconcile_seats_missing`; (d) cùng actor hai seat ⇒
  `lane_seat_actor_reused`; (e) close từ chối khi chỉ có seat receipt.

---

## Pha E — Khép vòng learning

Đọc trước: `src/learn/{mod,store,recall}.rs`, `skills/pulse-learn/SKILL.md`.

- **E1. Friction chưa phân loại lộ ra ở close.** Note kind friction nằm
  trong `notes[]` của record (`kernel::issues::append_note`, `NoteKind`).
  "Đã phân loại" = có learning mà `frontmatter.from` chứa id ticket **hoặc**
  note được đánh dấu bỏ qua. Thêm lệnh
  `pulse learn dismiss <id> <note-index> --reason "…"` (human-only) ghi
  event `friction.dismissed`; trạng thái tính từ event log + learnings, không
  thêm field vào note. `close`: **không chặn**, trả thêm
  `unclassified_friction: [...]` trong JSON và in một dòng cảnh báo.
  `close-story`: **chặn** với `close_story_friction_unclassified` liệt kê
  `ticket#index`. Sửa help text A5 cho khớp.
- **E2. `check` thành thứ được ép.** `Frontmatter` thêm
  `#[serde(default)] check_argv: Vec<String>` (+ `check_cwd`). `pulse learn
  add` thêm `--check-argv` (JSON array). `kernel::verify` gộp vào danh sách
  chạy mọi learning `active` khớp ticket (dùng `learn::recall::applicable`)
  có `check_argv`, tên `learning:<LRN-id>`; fail của nó chặn handoff như
  một verify thường. Packet `learning_view` thêm `check_argv`. Chỉ
  `active` (đã qua human) mới được ép — candidate thì không.
- **E3. `misleading` có hậu quả.** `recall::applicable`: loại learning có
  `usage.misleading > usage.helpful`; `pulse doctor` liệt kê chúng
  (`learning_suspect`) để human retire.
- **E4. Trích dẫn có hash.** `Frontmatter` thêm `cites: Vec<{path, lines,
  sha256}>`; `pulse learn add --cite path:from-to` tự tính sha256 của dải
  dòng tại thời điểm thêm. `pulse doctor` + packet đánh dấu `stale: true`
  khi hash không còn khớp (không tự retire).
- **E5 (sau).** `pulse learn mine` đọc transcript host — hoãn; cần khảo sát
  định dạng transcript từng host (tham khảo
  `references/better-harness/scripts/session-analysis/platforms/*.mjs`).
- **E6. `pulse metrics`.** `src/kernel/metrics.rs` + CLI `Metrics { --since,
  --json }`, thuần đọc `event::read_events` + receipts: friction/ticket,
  tỉ lệ rework (`issue.transitioned {reason: rework}` / ticket done),
  `lane_verdict_corrected`/lane, thời gian `run.started`→`done` trung vị,
  learning helpful vs misleading, (sau B) số lần `claim_files_reserved`.
  Thay cho việc viết tay `docs/plans/0022-metrics.md`.

---

## Pha F — Docs

- **F1.** Đóng băng `docs applicable`: giữ lệnh, bỏ khỏi block seed và
  `worker.md` như lối chính ("grep/glob trước; `docs.applicable` trong
  packet chỉ là gợi ý"). Không xoá code trong pha này.
- **F2.** Giữ nguyên gate "khai sửa doc thì phải có diff" và `docs check`.
- **F3. Gate ngược.** `evaluate_handoff`: với mỗi doc có frontmatter
  `applies_to`, nếu có changed file (trong scope ticket) khớp `applies_to`
  mà doc đó không nằm trong changed files → **cảnh báo**, không chặn:
  trả trong JSON handoff `docs_maybe_stale: [{doc, because}]` và đưa danh
  sách này vào lane input của `check-docs`. Chặn cứng chỉ khi dogfood cho
  thấy tỉ lệ báo đúng đủ cao.
- **F4.** `evaluate_close_story`: Story có `rules[]` không rỗng phải khai
  `docs_written: ["docs/…"]` (field mới trên Story) và mỗi path tồn tại +
  chứa mọi rule id (`BR-n`) dưới dạng chuỗi → `close_story_docs_missing`.
  Kiểm tra cơ học, không chấm nội dung.

---

## Pha G — Làm sau

- **G1. `pulse hook pre-edit --path <p> [--actor]`**: exit 2 khi path không
  được `covers` bởi `touches` của ticket mà actor đang giữ lease. `pulse
  init` **in** đoạn cấu hình hook cho host; không bao giờ tự ghi vào
  `~/.claude` hay `~/.codex`. Đây là chỗ duy nhất chống được agent bỏ qua
  Pulse.
- **G2. `init --refresh` 3-way merge**: lưu bản gốc đã render vào
  `.pulse/base/<path>`; refresh = `git merge-file` (base, local, mới);
  xung đột → để marker + báo, không ghi đè (học từ
  `references/repository-harness/crates/harness/src/application/service.rs`).
- **G3. Eval skill**: `evals/` chạy `claude -p` có/không skill trên fixture,
  chấm theo ground truth (học từ `references/repo-harness/evals/`).

---

## 3. Bảng error code mới (mỗi code phải có hint)

| Code | Pha | Nơi phát |
|---|---|---|
| `handoff_acceptance_not_done`, `handoff_verify_failed` | A1 | `completion.rs` (violation trong `gate_failed`) |
| `release_not_holder` | A4 | `reservation.rs` |
| `ready_touches_missing` | B1 | `ready.rs` |
| `claim_files_reserved` (thay `run_another_active`), `claim_actor_busy` | B3 | `reservation.rs` |
| `reserve_lease_mismatch` | B4 | `reservation.rs` |
| `handoff_unreserved_changes` | B6 | `completion.rs` |
| `verify_nothing_declared`, `verify_failed`, `handoff_verify_stale` | D | `verify.rs`, `completion.rs` |
| `lane_seat_required`, `lane_seat_unexpected`, `lane_seat_actor_reused`, `reconcile_seats_missing` | C | `lane.rs` |
| `close_story_friction_unclassified` | E1 | `completion.rs` |
| `close_story_docs_missing` | F4 | `completion.rs` |

Đã thêm ở phiên trước: `ready_description_missing`. Nếu
`docs/plans/0022-error-code-audit.md` là sổ sống về số lượng code, cập nhật
tổng ở cuối mỗi pha.

## 4. Định nghĩa "xong" cho cả plan

1. Ba lệnh validation xanh ở mỗi commit, threading mặc định.
2. Mỗi gate mới có test riêng nhắm đúng code của nó (pattern hiện có: một
   test / một violation).
3. Không lệnh nào được nhắc trong template/skill trước khi tồn tại trong
   CLI (guard sẽ bắt).
4. Dogfood B8 chạy trên `todolist` với hai worker song song thật, bảng
   friction được ghi; sau C, một ticket `api-high` đi qua panel 3/2.
5. `AGENTS.md`, `ARCHITECTURE.md`, block seed, 4 skill và (nếu còn)
   `SPEC.md` mô tả đúng hành vi mới; ba decision 0025–0027 nằm trong
   `docs/decisions/`.
6. Handoff cuối: nhánh, commit, test đã chạy, rủi ro còn lại, bước kế.

## 5. Rủi ro đã biết

- **Build gãy chéo** giữa hai worker cùng checkout (Decision 0025) — đo ở
  dogfood; ngưỡng xét lại worktree đã nêu.
- **`lane_commit_mismatch` khi ticket khác commit giữa lúc lane chạy** —
  chấp nhận, đo.
- **`handoff_unreserved_changes` báo oan** file sinh tự động chưa
  gitignore — cách thoát là `fence_ignore` trong `PULSE.md` (đã có).
- **Panel tốn token ×3** — vì thế opt-in và chỉ khuyến nghị cho `*-high`.
- **Danh tính vẫn là tự khai** cho tới G1 — ghi trung thực trong decision,
  không tuyên bố quá điều code chứng minh.

## Trạng thái thực thi

**Dogfood: đã chạy 2026-09-19** trên `~/Workspace/Personal/todolist`
(ST-33d3, hai worker song song + panel 3/2 + qa thật). 14 friction gốc
(gom 28 note): **10 đã sửa trọn vẹn, 4 sửa nửa khả-dĩ khép ở "không đổi
luật"** (D2/D3/D4 — chủ repo chấp nhận P1 cùng ngày); **0 còn chờ quyết
định**; 0 không tái hiện, 0 chữa sai bệnh. Ba con số:
build gãy chéo **0** (giữ single-checkout, dưới ngưỡng decision 0025),
`docs_maybe_stale` đúng **0/3** (giữ advisory), `pulse verify` max
**11.2s** (timeout 900s giữ nguyên). Các commit sửa: `c0c7eb7` (F8/F4),
`5395724` (F4/F5/F6-nửa-template/F7/F9), `6940826` (F12/F13), `299cd10`
(F10-nửa-skill/F11), `c018b89` (F2), `bfcecf1` (F1/D1), `b6008ea`
(F14/D5). Chi tiết xác minh từng F + kết cục D1–D5:
[`docs/plans/0025-dogfood.md`](0025-dogfood.md).

Đã xong (mỗi pha xanh cả ba lệnh validation, threading mặc định):
**A** (gate verify đọc lời khai + dirty_hash lane + findings trong packet +
lock nguyên tử), **B** (song song theo `touches`, `reserve`, `frontier`,
fence theo scope), **D** (`pulse verify` tự quan sát, decision 0026),
**C** (review panel + `lane reconcile`, decision 0027), **E** (vòng học:
friction lộ ở close, check_argv bị ép, misleading có hậu quả, cites, metrics),
**F** (F1 đóng băng `docs applicable` thành gợi ý; F2 giữ nguyên gate cũ;
F3 gate ngược `docs_maybe_stale` + `docs check --ticket` — tư vấn, không
chặn; F4 close-story đòi `docs_written` cho rules/exceptions).

Lệch plan đáng kể đã biết (tin code, giữ phương án ít đổi hành vi nhất):

- **`lane_seat_invalid` gộp** vào các code seat sẵn có (`lane_seat_unexpected`/
  `lane_seat_required`) thay vì thêm code riêng.
- **Friction khoá bằng event id** (`<subject>#evt_<ulid>`, key bất biến),
  không phải index note — note bị cắt ở 50 mục, event id thì không.
- **`docs check --ticket <id>` thay cho lane input của check-docs**: lane
  `check-docs` không có agent nào đọc input, nên việc "đưa `docs_maybe_stale`
  vào lane input" là vô ích; lệnh tự tính danh sách doc có thể cũ và ghi
  thành finding low trong report (`--write` vẫn chính là lane check-docs).
- Nghĩa của frontmatter `applies_to` chuyển từ "doc này nên đưa cho worker
  nào" sang "doc này mô tả phần code nào" (F3 sống nhờ nghĩa mới); gợi ý
  trong packet chỉ còn là công dụng phụ.

**G1 — `pulse hook` (đã thực thi):** `kernel::hook::pre_edit` quyết định
theo đường dẫn, không theo danh tính: fenced-out thì free (trừ lane chỉ
được viết dưới `.pulse/evidence/`), lane không sửa source, không ticket
active thì theo `hook.unclaimed` (mặc định `allow`), ticket không `touches`
là độc quyền, path phải rơi vào `touches` của ticket đang giữ, không được
đụng vùng `verifying`, còn lại deny kèm gợi ý `pulse reserve`. CLI
`pulse hook pre-edit` với hợp đồng exit 0/2/1 (0 = allow im lặng, 2 = deny
ra stderr cho host trả lại agent, 1 = lỗi nội bộ — store rách không được
phép khoá mọi chỉnh sửa); `--stdin-json` rút path từ các khoá host đã xác
minh trong `references/repo-harness` (`tool_input.file_path`,
`tool_input.path`, `tool_input.notebook_path`, dòng apply-patch
`*** Add/Update/Delete File:` của Codex, `cwd` để giải path tương đối);
`pulse hook snippet <host>` chỉ IN cấu hình đã xác minh (Claude Code
PreToolUse) — không bao giờ ghi file host nào; host khác →
`hook_host_unknown`. `pulse init` thêm một dòng gợi ý snippet.

Lệch G1 của session này (tin code/thực tế macOS):

- **Repo-root resolution cho hook**: dùng đúng `state_repo_root` như mọi
  lệnh khác (cwd hoặc `--repo-root`), KHÔNG đi lên tìm `.pulse` — một cơ
  chế mới sẽ là hành vi thứ hai cho cùng việc. Host chạy hook từ project
  root (Claude Code làm vậy).
- **Path normalization**: path tuyệt đối được so với cả repo_root lẫn bản
  canonicalized, và chỉ thư mục CHA của path được canonicalize (file có thể
  chưa tồn tại) — macOS trả `current_dir` dạng `/private/tmp/…` trong khi
  host gửi `/tmp/…`; bắt được live, có test.
- **`--actor` không rơi về `git config user.name`** như `resolve_actor` —
  hook mà tự nhận human thì luật actor vô nghĩa (đúng-phạn văn prompt).
- **Không hỗ trợ snippet cho Codex** dù payload apply-patch đã được parse:
  hình dạng cấu hình `~/.codex/hooks.json` đã xác minh qua references,
  NHƯNG hợp đồng deny của Codex (exit 2 có bị trả lại agent không, hay cần
  JSON decision) CHƯA xác minh được từ references — không bịa.

**G2 — `pulse init --refresh` 3-way merge (đã thực thi):** mỗi lần init/
refresh ghi một file do template sinh ra, lưu bản template vào
`.pulse/base/` (state bền, commit — không gitignore). Refresh:
local == new → unchanged; có base và local == base → updated; có base và
local != base → `git merge-file` trên file tạm dưới
`.pulse/runtime/refresh/` — merge sạch → ghi kết quả + cập nhật base,
xung đột → KHÔNG đụng local, file marker vào `.pulse/runtime/refresh/`,
base nguyên, báo conflict (exit vẫn 0); không có base và local != new →
giữ local, ghi `.new`, báo kept, base vắng cho tới khi người dùng giải
quyết bằng `--refresh --take-new <file>` hoặc `--keep-mine <file>`
(file lạ → `init_refresh_unknown_file`). Vùng block của AGENTS.md đi qua
cùng thuật toán như một "file" văn bản; nội dung ngoài block không bao giờ
bị đụng. Hai test cũ khẳng định "refresh ghi đè" được viết lại theo ngữ
nghĩa mới, không xoá trắng.

### Còn lại

- **G3 eval skill** — HOÃN: cần dữ liệu dogfood thật và ngân sách token
  cho `claude -p` có/không skill trên fixture (tham khảo
  `references/repo-harness/evals/`). Chỉ đáng làm khi B/D/C/E/F/G đã chạy
  đủ một vòng dogfood để biết skill nào cần đo.
- **E5 `learn mine`** — HOÃN: cần khảo sát định dạng transcript từng host
  (`references/better-harness/scripts/session-analysis/platforms/*.mjs`).
- **DOGFOOD — ĐÃ CHẠY 2026-09-19** (xem dòng đầu mục này và
  [`0025-dogfood.md`](0025-dogfood.md)). Ba con số đã đo: (1) build gãy
  chéo **0**/story → giữ single-checkout; (2) `docs_maybe_stale` đúng
  **0/3** → chưa đủ chuyển cảnh báo thành chặn (khuyến nghị + ngưỡng xét
  chuyển nằm trong §1.1 của file dogfood); (3) `pulse verify` max
  **11.2s** vs timeout 900s → default ổn. Bảng friction F1–F14 đã được
  xác minh lại từng dòng với code và sửa/quyết định như trên.
