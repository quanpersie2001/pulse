# Decision 0015: Worktree dispatch — workspace trong worktree, state về repo chính

## Status

Accepted, 2026-09-07. Đệ trình từ ma sát thật của Track B vòng 1
(TK-006/TK-007, `examples/todolist/`), ghi trong
`examples/todolist/works/friction.md`.

Một điều chỉnh khi implement so với bản Proposed: việc nhận diện repo chính
**không** suy ra từ `git rev-parse --git-common-dir` một mình. `--git-common-dir`
trả về toplevel của Git repo, còn repo-root của Pulse có thể là một thư mục
con của repo đó (`examples/todolist` là ví dụ đang chạy), nên nó không khôi
phục được state root. Marker `.pulse-owned` vì vậy mang luôn `state_repo_root`
tuyệt đối, và `--git-common-dir` được dùng đúng vai trò ADR mô tả: đối chứng
để không nhận nhầm một worktree người dùng tự tạo hay một marker bị chép đi
nơi khác.

## Context

PRODUCT §5.3 hứa: khi một Ticket khác đang giữ lease, `pulse run` từ chối
với `run_isolation_required`; cờ `--isolation worktree` tạo worktree cho
Ticket mới. Đơn vị cô lập là worktree; nguyên tắc 10 nói "làm trực tiếp
trong checkout", worktree là opt-in.

Track B vòng 1 là lần đầu cơ chế này chạy thật, và nó lộ hai lỗi cùng một
gốc, ở hai lớp.

### Lớp 1 — run workspace không nằm trong worktree

`pulse run worker --ticket TK-006 --isolation worktree` tạo worktree và
spawn runner với cwd = worktree (đúng), nhưng run workspace chỉ được ghi
vào repo chính: `src/kernel/run.rs` ghi `run_dir = self.repo_root.join(RUN_DIR).join(ticket_id)`
với `self.repo_root` là repo gốc, bất kể workspace. Agent được bootstrap
prompt bảo "đọc `.pulse/runtime/run/{ticket}/worker-prompt.md` in this
repository" — file đó không tồn tại dưới cwd của nó. Runner (codex) lần
theo path tuyệt đối có sẵn trong packet/env về repo chính và làm việc ở đó.

Hậu quả thấy được: deliverable của TK-006 (`works/TK-006/decision.md`,
`works/TK-006/research/schema-evolution.md`) xuất hiện trong **main
checkout** lúc 01:01, giữa lúc reviewer của TK-007 đang verify (01:00–01:07).
Dirty identity của main checkout đổi sau handoff của TK-007 → verify của
reviewer fail (`verification_source_mismatch`), run bị `inconclusive`,
TK-007 phải đi recovery đầy đủ theo LRN-001: release → re-run worker →
re-review → mới close được. "Cô lập" ở đây chỉ đúng một cách tình cờ:
fence của TK-006 bind dirty identity của *worktree* — vốn sạch vì worker
không hề làm việc trong đó.

### Lớp 2 — nếu agent đứng yên trong worktree thì cũng chưa chạy được

State planes tách đôi giữa worktree và repo chính:

- Runtime (leases, reservations, run records, locks) nằm ở
  `<main>/.pulse/runtime/` — gitignored, không được checkout vào worktree.
- Worktree có bản sao tracked `.pulse/` riêng (workgraph, receipts, events,
  policy, config) từ commit lúc tạo worktree.
- CLI lấy repo-root = cwd (`src/cli/mod.rs`). Một `pulse work handoff` gọi
  từ worktree sẽ đọc/ghi graph và runtime **của worktree** — lease không có
  ở đó, và mutation ghi vào một graph phụ không phải canonical. Hai hệ
  thống cùng sửa status, trái nguyên tắc 4 (one writable source per truth).

Tức là: mirror workspace đơn thuần chưa đủ; phải chốt luôn câu hỏi CLI
trong worktree nói chuyện với canonical state bằng cách nào.

### Bằng chứng bổ sung

- `git worktree list` sau vòng 1: worktree TK-006 được tạo rồi tự dọn khi
  close — cơ chế sở hữu (`.pulse-owned` marker + đăng ký worktree) hoạt
  động đúng, không cần đổi.
- Fence hiện bind `worktree_dirty_identity(workspace)` theo workspace của
  run — đúng chuẩn, giữ nguyên.
- Reviewer/qa hiện luôn chạy ở main (`decide_workspace` chỉ áp cho role
  worker), nên dù worker đứng yên trong worktree, reviewer cũng không thấy
  thay đổi của worker — reviewer sẽ review một tree khác với tree đã được
  handoff.

## Decision

1. **Pulse-owned worktree là một workspace của repo chính, không phải một
   repo Pulse thứ hai.** Worktree do Pulse tạo (có marker `.pulse-owned` và
   được `git worktree list` đăng ký) được CLI nhận diện; mọi **mutation
   state** (workgraph, receipts, events, knowledge, policy, config,
   runtime/leases) route về repo chính, thực hiện dưới **cùng một lock**
   của repo chính — single-writer không đổi. Việc nhận diện dựa trên
   `git rev-parse --git-common-dir` từ cwd worktree để suy repo gốc, kết
   hợp marker để không tự ý nhận worktree do người dùng tự tạo. Worktree
   lạ (không marker) được coi như repo bình thường của người dùng, CLI
   không map.
2. **Source plane là thứ duy nhất worktree sở hữu.** Đọc diff, dirty
   identity, source commit, file nội dung của một run worktree luôn tính
   trên worktree đó. Fence handoff/verify/close bind dirty identity của
   workspace đã ghi trong runtime binding — không đổi so với hiện trạng,
   ADR này chỉ ghi rõ thành contract.
3. **Run workspace được ghi vào cả hai nơi khi có worktree:** bản record
   ở runtime repo chính (như hiện tại) và bản agent dùng tại
   `<worktree>/.pulse/runtime/run/<ticket>/` (worker-prompt.md,
   worker-input.json, worker-env). Prompt nhúng path tuyệt đối của workspace
   nên agent không phải đoán. Runtime worktree là bản copy dùng một lần,
   xóa cùng worktree, không phải truth.
4. **Reviewer và qa chạy trong cùng workspace với worker.** Ticket có run
   worktree đang sống thì reviewer/qa lấy cwd = worktree đó; reviewer-input
   trỏ artifact dir và changed paths của workspace. Không cho phép review
   một tree khác tree đã handoff.
5. **Tracked `.pulse/` trong worktree trở thành read-only mirror.** CLI
   không bao giờ ghi vào tracked planes của worktree (mutation đã route về
   main ở mục 1); nếu phát hiện lệch (worktree tạo từ commit cũ) thì cảnh
   báo `worktree_graph_stale` trong packet/run record, không tự rebase.
6. **Hai họ receipt giữ nguyên — không gộp trong đợt này.** Việc gộp
   `evidence/execution/*` vào envelope chung (PRODUCT §11) chạm loader và
   doctor, tách rời phần braces của quyết định này; làm khi `pulse doctor`
   cần đọc chung, để ADR này đủ nhỏ để review.

### Các phương án đã loại

- **State local đầy đủ mỗi worktree, đồng bộ ở biên (merge/sync khi
  close):** hai writer cùng sửa một graph, xung đột xử lý bằng merge — đi
  ngược nguyên tắc 4 và mô hình lock repo-scoped hiện có; chi phí reconcile
  cao hơn đúng mà nó mang lại.
- **Chỉ nhúng path tuyệt đối vào prompt, không mirror workspace:** phụ thuộc
  runner tôn trọng cwd và không "đi dạo"; TK-006 chứng minh runner thực tế
  không đáng tin ở mức đó. Mirror là bắt buộc, path nhúng là bổ trợ.
- **Cấm worktree, chỉ chạy serial:** loại bỏ tính năng PRODUCT §5.7 đã hứa
  (chạy song song) thay vì sửa nó; vòng Track B 2 cần song song thật.

## Kiểm chứng

- Integration (`tests/runner/`): runner giả (script shell) với
  `--isolation worktree` — assert (a) file agent tạo nằm trong worktree,
  main checkout không đổi dirty identity; (b) handoff receipt nằm ở planes
  của repo chính; (c) reviewer chạy tiếp thấy thay đổi trong worktree và
  verify pass; (d) close xong worktree được dọn.
- Unit: ánh xạ repo-root cho worktree có marker; worktree không marker thì
  không map; `worktree_graph_stale` được gắn khi graph worktree cũ hơn
  HEAD.
- Guard hiện có (`tests/graph/architecture_guards.rs`) không đổi.

## Thay đổi (đã làm 2026-09-07)

- `src/kernel/run.rs`: `mirror_run_workspace` chép input/prompt/env vào
  `<workspace>/.pulse/runtime/run/<ticket>/`; `RunPaths` tách "bản ghi ở repo
  chính" khỏi "bản agent thấy"; `live_ticket_worktree` cấp workspace của
  worker cho reviewer/qa; prompt worker và reviewer nhúng path tuyệt đối của
  workspace; run record thêm `workspace_id` và `worktree_graph_stale`;
  artifact tương đối resolve theo workspace.
- `src/source.rs`: `WorktreeMarker`, `write_worktree_marker`,
  `read_worktree_marker`, `state_repo_root`; `.pulse-owned` vào
  `PULSE_RUNTIME_EXCLUDE_PATHS` để marker không làm workspace bẩn từ lúc
  sinh ra.
- `src/cli/mod.rs`: áp mapping đúng một lần ở biên CLI.
- `src/storage/lock.rs`: không đổi — đúng như dự đoán, mapping khiến mọi
  writer hội tụ về một lock.
- Placeholder `{repo}` đổi nghĩa thành workspace mà role chạy trong đó, thêm
  `{state_repo}` cho repo chính. Không đổi nghĩa thì mục 4 không thực hiện
  được: lệnh reviewer mẫu dùng `-C {repo}` vẫn sẽ ép về main.
- Tests: `tests/runner/worktree_dispatch.rs` — 8 test, gồm cả bốn assert của
  phần Kiểm chứng. Fake agent cố tình chỉ biết cwd của mình và không truyền
  `--repo-root`.
- `PRODUCT.md` §5.3 (isolation rule, placeholder), §5.7 (chạy song song),
  §13 mục 11; `ARCHITECTURE.md` §2 và §5.
- Đóng hai mục friction.md tương ứng, cộng một mục thứ ba phát hiện khi
  viết ADR (reviewer review khác cây).

### Thu hẹp có chủ ý

Mục 5 nói cảnh báo `worktree_graph_stale` "trong packet/run record"; bản
implement chỉ ghi vào **run record**. Packet được dựng từ repo chính (mapping
đã đưa repo-root về đó), nên một cờ "worktree cũ" trong packet không có chủ
thể rõ ràng. Hệ quả còn lại: `work packet` gọi từ trong worktree đọc HEAD và
dirty state của repo chính, không phải của worktree. Điều đó nhất quán trong
mọi trường hợp trừ khi developer commit vào main giữa lúc run đang chạy — và
đúng trường hợp đó thì run record báo `worktree_graph_stale`. Nếu dogfood cho
thấy khoảng này đau thật thì tách source root khỏi state root trong `JsonGraphStore`,
việc đó cần ADR riêng vì chạm mọi call site.

## Consequences

- Song song thực sự an toàn: mutation luôn về main dưới một lock; worktree
  chỉ mang source và runtime copy dùng một lần. False isolation hiện tại
  biến mất.
- Reviewer review đúng tree đã handoff — hết lớp sai số "review cây khác
  cây worker đã làm".
- CLI phức tạp thêm một bước nhận diện repo-root; chi phí này nằm ở đường
  đọc, mutation giữ nguyên mô hình cũ.
- Worktree tracked `.pulse/` là dead weight hiển thị (stale mirror); chấp
  nhận vì tracked planes nhỏ, và cảnh báo `worktree_graph_stale` đủ để
  không ai nhầm.
- Worktree do người dùng tự tạo (không marker) không được map — họ tự quản
  lý state của mình; Pulse không nhận diện vũ trụ worktree lạ.
- Hai họ receipt vẫn tách rời; doctor sau này phải đọc hai nơi cho tới khi
  có ADR gộp riêng.
