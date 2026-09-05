# Handoff: Pulse — Bước 4, runner và communication

## Trạng thái bàn giao

- Repo: `/Users/quannv.dev/Workspace/Personal/pulse`
- Nhánh: `features/harness-experimental`
- HEAD: `6b6861b` (`docs: record Step 3 spine behavior`)
- Working tree: chỉ file handoff này đang được sửa để bàn giao.
- Bước 3.1–3.7 đã có code và test; `examples/todolist/` chưa được tạo.

## Thay đổi đã hoàn thành

Các commit của Bước 3, theo thứ tự:

1. `b99eb53` — `feat: enroll init actor with core grants`
2. `4ff3658` — `feat: make ticket markdown the contract source`
3. `7f65186` — `feat: gate shaping on ticket ambiguity`
4. `d7754f7` — `feat: close tickets across risk levels`
5. `42b558d` — `refactor: reduce work packet to real context`
6. `cc4eb8a` — `refactor: remove unrealized packet dispatch surfaces`
7. `70af2e7` — `feat: add docs tags and focused metadata`
8. `2540bd5` — `refactor: align knowledge applicability test`
9. `6fd826c` — `fix: scope git source paths to repository root`
10. `c36d89b` — `refactor: make generated ticket templates syncable`
11. `ed6604c` — `refactor: normalize empty QA owner in ticket briefs`
12. `6b6861b` — `docs: record Step 3 spine behavior`

Đã có:

- `init --actor kind:id`, fallback Git `user.name`, principal idempotent, default-deny và `CORE_GRANTS` không wildcard.
- Ticket tạo `works/<id>/ticket.md`, suy materialization từ risk, hỗ trợ parent/tag; parser pure ở `src/graph/model/brief.rs`; `work sync` bind hash và tăng `contract_revision`.
- `draft -> shaped` dùng ambiguity gate từ Markdown, không cần shaping receipt để vào `shaped -> ready`; các path blocked/rework được giữ.
- Close hỗ trợ medium/high/critical; high/critical bắt buộc actor `human`; có `work close <ticket>` resolve verification/lease hẹp.
- Packet đã bỏ dispatch/capability/scope-enforcement/assurance và capability schema; có ticket prose, parent/decision/blocker/docs/QA/source/tags/handoff cùng fingerprint/fence.
- Docs record đã rút về metadata mục tiêu, có `.pulse/docs/tags.json`, `docs tags add|list`, tag filter/validation và path/tag applicability.
- Git source snapshot tính `git rev-parse --show-prefix` và scope status/ls-files/diff đúng khi `--repo-root` là thư mục con.
- README, AGENTS, ARCHITECTURE, PRODUCT §8 đã phản ánh hiện trạng.

## Verification

Đã chạy thành công trên working tree hiện tại:

```text
cargo fmt --check
cargo clippy --all-targets --quiet -- -D warnings
cargo test --all-targets
```

Kết quả lần chạy cuối: 55 unit tests, 99 docs, 9 evidence, 224 graph, 22
knowledge, 14 process, 4 public API, 24 storage và 53 target-repository tests
đều pass (504 tests). Số test attributes trong source/tests: 558 trước Bước 3,
562 sau. Số dòng: `src/**/*.rs` 41,468 -> 42,284; `tests/**/*.rs` 25,807 ->
25,357.

Smoke đã xác nhận trên một Git repo tạm copy từ
`tests/fixtures/target-repos/minimal-service`: init, tạo Story/Ticket, sửa
`ticket.md`, sync, shaped và ready đều chạy được. Packet vẫn yêu cầu source
worktree sạch; cần commit các thay đổi canonical `.pulse/` trước khi gọi packet.

## Khoảng cách còn lại cần biết

- `ImplementationContract`, `ContractSetRequest`, `work contract set|show`,
  `qa-impact set`, `readiness-policy`, shaping receipt model/API/CLI và một số
  DTO trung gian legacy vẫn còn để giữ test/caller cũ. Chúng không còn là
  ceremony bắt buộc của lifecycle, nhưng chưa bị xoá hoàn toàn như PRODUCT §5.1
  và §5.2 mô tả.
- `work edit` hiện vẫn sửa title; không sửa ticket contract. `work sync` là
  đường cập nhật explicit. Hash stale không tự mutate read path.
- Packet source fence vẫn từ chối tracked/untracked dirty worktree thay vì
  phát packet với `dirty: true`; golden path thực tế cần commit graph/content
  trước packet. Đây là khác biệt cần quyết định lại trong Bước 4 nếu runner cần
  packet ngay sau mutation.
- Chưa có `runner/`, `pulse run`, lease TTL runner saga, `events tail`, `note`,
  knowledge capture/promote/applicable, hay `examples/todolist/` dogfood.
- PRODUCT.md chỉ được cập nhật cột “Hiện trạng” ở §8; target design trong các
  mục còn lại chưa được viết lại. Các khoảng cách trên là sai khác code-vs-target,
  không phải claim đã hoàn thành.

## PRODUCT.md có gì không khớp thực tế

1. §5.1/§5.2 yêu cầu xoá hoàn toàn JSON contract, shaping surfaces và DTO
   legacy; hiện chúng vẫn tồn tại ở compatibility layer dù không chặn golden
   lifecycle path.
2. Golden path §7 ghi packet ngay sau ready nhưng source fence hiện đòi Git
   clean, nên phải có bước commit/hoặc cần thay policy fence.
3. §5.2 mô tả QA cases nguyên văn từ Story `qa.md`; packet hiện giữ phần QA
   raw có thật nhưng chưa có runner để tạo/tiêm đầy đủ execution context.

## Quyết định đã chốt cho ba khoảng cách trên (2026-09-05, maintainer)

1. **Xoá compatibility layer, không giữ.** Nguyên tắc PRODUCT.md: không có hai
   đường song song. `ImplementationContract`, `ContractSetRequest`,
   `work contract set|show`, `qa-impact set`, `readiness-policy`, shaping
   receipt model/API/CLI và DTO trung gian legacy phải bị xoá; test/caller cũ
   viết lại qua `ticket.md` + `work sync`. Làm ở mục 4.0 dưới đây, trước runner.
2. **Packet chấp nhận worktree bẩn.** Fence không đòi Git clean. Packet ghi
   `source.commit`, `source.dirty: true|false`, `source.dirty_hash` (hash của
   `git diff` tracked + untracked manifest mà `source.rs` đã tính), và fingerprint
   packet bao gồm `dirty_hash`. Receipt handoff/verification bind cùng
   `dirty_hash`; close gate so `dirty_hash` hiện tại với receipt, khác thì
   stale. Chỉ giữ một check cứng: `.pulse/runtime/` và `.pulse/cache/` phải được
   gitignore (đã có `validate_packet_operational_path`). Lý do: golden path và
   runner gọi packet ngay sau khi mutate `.pulse/` và `works/`, và worker đang
   sửa code cũng phải gọi lại packet được. Sửa `kernel/packet.rs` quanh
   `packet_source_snapshot_from_packet` và `work_packet_source_unavailable`;
   cập nhật PRODUCT.md §5.2 câu "Fence theo commit" thành "Fence theo commit +
   dirty hash".
3. **QA cases trong packet** giữ raw từ `qa.md` như hiện tại; runner role `qa`
   sẽ tạo input riêng (PRODUCT.md §5.3), không cần thêm gì vào packet.

## Việc Bước 4

Đọc lại `PRODUCT.md` §5.3, §5.7, §5.8 và `design/archive/daemon-assignment-saga.rs`.
Không khôi phục daemon. Xây các seam mỏng sau:

0. Xoá compatibility layer theo quyết định 1 ở trên, và sửa packet fence theo
   quyết định 2. Hai commit `refactor:` riêng, test xanh, trước khi chạm runner.
1. Tách contract chạy command từ `src/qa/executor.rs` thành `src/runner/`;
   runner chỉ parse config/argv, timeout, bounded stdout/stderr, process-group
   cancellation và JSON output contract — không sở hữu graph truth.
2. Thêm `.pulse/config/runners.json` bootstrap và `pulse run <role>
   --ticket <id>`: kiểm tra ready/verifying, lease TTL, packet commit vào
   `.pulse/runtime/run/<ticket>/`, spawn command, map exit/timeout/malformed
   output thành receipt `inconclusive`, hash artifacts và ghi event.
3. Implement isolation rule: checkout mặc định; worktree chỉ khi Ticket khác
   đang active hoặc `--isolation worktree`; dọn worktree do Pulse tạo khi
   terminal, không xoá worktree ngoài ownership.
4. Thêm recovery/resume/release lease rõ ràng; không blind retry. Giữ
   reservation packet fingerprint và contract-drift acknowledgment.
5. Thêm `events tail` và `note` theo event-log append-only; note phải xuất hiện
   trong packet, không tạo broker/daemon.
6. Sau đó chạy dogfood Bước 5 bằng `examples/todolist/`, không chạy Pulse với
   `--repo-root .` ở repo phát triển.

Mỗi mục là một commit `feat:`/`refactor:` ngắn, kết bằng:

```text
Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>
```

Mỗi commit phải xanh:

```bash
cargo fmt --check
cargo clippy --all-targets --quiet -- -D warnings
cargo test --all-targets
```
