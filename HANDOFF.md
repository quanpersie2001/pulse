# Handoff: Pulse — Bước 5, dogfood golden path trên examples/todolist

## Trạng thái bàn giao

- Repo: `/Users/quannv.dev/Workspace/Personal/pulse`
- Nhánh: `features/harness-experimental`
- HEAD: `e9881d0` (`feat: add pulse note and events tail over the append-only log`)
- Working tree: sạch (chỉ file handoff này được cập nhật để bàn giao).
- Bước 4 (runner + communication) đã hoàn thành đủ 6 mục, mỗi mục một commit,
  mọi commit đều xanh cả ba gate.

## Commit của Bước 4

1. `7110002` — `refactor: remove legacy contract and shaping compatibility layer`
2. `fa2dcef` — `refactor: fence packet source on commit plus dirty hash`
3. `671c6f7` — `feat: add runner command-execution contract`
4. `71a6d5e` — `feat: add pulse run with runners.json bootstrap and work handoff CLI`
5. `fbe6815` — `feat: add isolation rule with pulse-owned worktrees`
6. `d8fc9ae` — `feat: add lease recovery, verified resume and work release`
7. `e9881d0` — `feat: add pulse note and events tail over the append-only log`

## Đã có sau Bước 4

- **Ticket contract**: chỉ còn một nguồn sự thật `works/<id>/ticket.md`
  (`brief_hash` + metadata docs/QA suy từ markdown). Legacy JSON contract,
  shaping receipt, decision frontier, `work contract|qa-impact|shaping|readiness-policy`
  đã bị xoá; các grant `shape.*`/`qa.none.*` removed khỏi CORE_GRANTS.
- **Packet fence**: worktree bẩn là base hợp lệ; packet ghi
  `source.dirty`/`source.dirty_hash` (content-hashed manifest loại trừ metadata
  Pulse) và fingerprint bao gồm dirty hash. Handoff/verification/close bind cùng
  dirty identity — sửa source sau handoff làm proof stale.
- **`src/runner/`**: parse command spec từ JSON, split argv không qua shell,
  placeholder `{input}/{ticket}/{repo}/{artifact_dir}`, timeout, bounded
  stdout/stderr, process-group kill, JSON output contract
  (`runner_output_malformed` cho output xấu).
- **`pulse run <role> --ticket <id>`**: `pulse init` bootstrap
  `.pulse/config/runners.json` (worker/reviewer/qa). Worker gate `ready`,
  reviewer/qa gate `verifying`; lease TTL; input/env/prompt commit vào
  `.pulse/runtime/run/<ticket>/`; classify `handed_off|blocked|inconclusive`
  (timeout/cancelled/exit_nonzero/malformed/unproven_claim); run record + event
  `run.completed`; provision tự động principal `runner:<role>` với grant hẹp.
- **Isolation**: checkout mặc định; worktree Pulse-owned tại
  `.pulse/runtime/worktrees/<ticket>/` khi Ticket khác đang active hoặc
  `--isolation worktree`; `auto_isolation: false` trong runners.json từ chối;
  dọn worktree khi terminal/release (chỉ worktree có marker + registered).
- **Recovery**: run gián đoạn giữ lease; re-run không drift → resume cùng lease;
  drift (contract_revision/source commit) → `run_resume_drift`, cần
  `--acknowledge-drift` (release stale + Active→Ready + fresh lease);
  `pulse work release` đưa Ticket active về ready.
- **Giao tiếp**: `pulse note` (grant `note`, event `note.recorded`),
  `pulse events tail --since/--ticket/--follow`; note hiện trong packet
  (tối đa 8 note mới nhất, message cap 500 ký tự).
- CLI proof: `pulse work handoff --lease --session --source-commit --summary`,
  `pulse work verify --handoff --actor --check name=cmd=exit --proof AC=checks=receipts`.

## Verification

Đã chạy trên working tree tại `e9881d0`:

```text
cargo fmt --check          -> OK
cargo clippy --all-targets --quiet -- -D warnings  -> OK
cargo test --all-targets   -> 496 pass, 0 fail
```

Phân bổ: 55 unit, 99 docs, 9 evidence, 175 graph, 22 knowledge, 14 process,
35 runner, 6 communication, 4 public API, 24 storage, 53 target-repo.

## Khoảng cách còn lại (nhận thức, không phải việc đã xong)

- **Artifact ingest chưa có**: PRODUCT §5.3 bước 7 (hash artifact khai báo,
  copy vào `.pulse/evidence/artifacts/sha256/`) chưa được wire vào `pulse run`.
  Run record hiện chỉ có stderr_tail.
- **QA runner input** mới ở mức cases rút từ baseline; vòng life của
  `qa_checkpoint` receipt qua `pulse run qa` chưa khép (vẫn qua `pulse evidence
  receipt record` của repo).
- **Reviewer output contract** (`disposition/acceptance/findings`) chưa được
  parse từ final JSON; reviewer run chỉ classify `completed`.
- **`pulse run` reviewer/qa chưa dùng worktree** (chỉ worker isolate); tuần tự
  sau worker trên cùng checkout như PRODUCT mô tả.
- `work edit` vẫn chỉ sửa title; `ticket.md` + `work sync` là đường cập nhật.
- PRODUCT.md §8 đã cập nhật cột hiện trạng; các mục target khác giữ nguyên.

## Việc Bước 5: dogfood golden path

Đọc lại `PRODUCT.md` §7 (golden path v0.1). Tạo `examples/todolist/` trong repo
Pulse (cùng Git history, KHÔNG nested `.git`, KHÔNG chạy `pulse` với
`--repo-root .` tại gốc repo phát triển):

1. Tạo app todolist nhỏ + docs tối thiểu (`AGENTS.md`, docs map).
2. `pulse init --repo-root examples/todolist`; chỉnh `.gitignore` của thư mục
   đó để `.pulse/runtime/` + `.pulse/cache/` ignored; commit.
3. Tạo Story với `qa.md` (2 case) + Ticket R1 với `ticket.md` đầy đủ, risk
   medium, QA required, docs required; sync; `shaped`; `ready`.
4. `work packet` — kiểm tra developer đọc thấy đủ context.
5. `pulse run worker` với runner thật (Claude Code headless mặc định theo
   quyết định 13.2); agent handoff qua CLI; verify; `docs validate --record`;
   close medium risk.
6. Kill agent giữa chừng → `pulse run` lại → lease/packet đúng; sửa source sau
   handoff → receipt stale, close bị từ chối với lý do.
7. `knowledge capture` 1 learning, `promote`, ticket sau thấy trong packet.

Mỗi phát hiện sai lệch giữa PRODUCT và thực tế khi dogfood: sửa code hoặc ghi
Decision; không thêm feature ngoài golden path. Mỗi commit ngắn kết thúc bằng
`Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>` và phải xanh:

```bash
cargo fmt --check
cargo clippy --all-targets --quiet -- -D warnings
cargo test --all-targets
```
