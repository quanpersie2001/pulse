# 0022 — Dogfood ST-1: golden path v3 bằng agent thật (P2.2–P2.6)

Ngày chạy: 2026-09-16, một session operator (`human:quan`), branch
`features/harness-experimental` @ `db33d26`. Target:
`~/Workspace/Personal/todolist` (repo mới, scaffold commit `a63e636`).
Worker/reviewer là agent thật: Claude Code (`claude -p`, worker + kill/resume),
Codex (`codex exec`, review-correctness). Kết quả: **ST-5af4 done, TK-1378 done,
TK-82d7 done, close-story pass, learning candidate tạo và kiểm bằng TK-041f.**
Todolist HEAD cuối session: `476e1ad`.

## 1. Bảng friction

| id | Ticket | bước | mô tả | loại | sửa ở đâu | bằng chứng |
|---|---|---|---|---|---|---|
| F1 | — | B2 | `templates/qa/ui.mjs` dùng `page.accessibility.snapshot()` — API bị xoá khỏi playwright ≥1.45 → lane crash (`qa_ui_crashed`), và crash path không stop app → container lệch trạng thái | pulse-bug (template) | `templates/qa/ui.mjs`: dùng `locator.ariaSnapshot()`; bọc crash path bằng `finally` stop | selftest B2, lần chạy đầu |
| F2 | TK-82d7 | B5 | Detached start + poll `ready_url` không phân biệt container cũ/mới: `docker compose up -d --build` **recreate cả khi build cache đầy đủ** (~8s down, đo 20:13:27–20:13:34) → lane chấm app stale (shot là trang scaffold) hoặc app đang chết (`ERR_CONNECTION_REFUSED`); **verdict đổi ngẫu nhiên trên cùng một code** | pulse-bug (template) | `templates/qa/{ui,api}.mjs` + `scripts/qa/README.md`: chờ start child exit (bounded) rồi mới poll; yêu cầu 3×200 liên tiếp | evidence `ST-5af4/shots/QA-002-375x812.png` cũ, container recreated 12:56:41 sau khi lane xong |
| F3 | — | B3 | Plan §6 hứa mọi `--json` có `ok:bool`; thực tế `work update/new` echo record, không có `ok` | plan/code drift | plan §6 hoặc code | session log 12:08 |
| F4 | TK-1378 | B3 | Packet không chứa `story.qa_cases` (§9) — worker không thấy oracle của reviewer (QA-001 đòi POST 201); may là AC-1 đã pin 201 | target-harness/packet | `src/kernel/packet.rs`: thêm qa_cases (id/surface/expected/check.argv) vào packet.story | event `note.recorded` 12:12:49 |
| F5 | TK-1378 | B4 | Seed §8.2 `codex exec --sandbox read-only` **không viết được** file evidence của chính nó (§8.4 bắt buộc) → `lane_output_invalid`; hỏng im lặng vì lane không emit event (F8) | pulse-bug (plan seed) | plan §8.2 + `runners.json` seed: `workspace-write`; source protection = prompt + `lane_mutated_workspace` | `review2-run.log`, event thiếu |
| F6 | TK-1378 | B4 | Schema §8.4 đóng nhưng prompt không nói: codex thêm `commands_run[].cwd`/`note` + `environment.worktree_dirty` → cả một review pass (4.5 phút, tự chạy 22 test) bị từ chối seal | pulse-bug (prompt) | `templates/prompts/review-correctness.md`: pin closed schema; cân nhắc cho phép `cwd` trong `commands_run[]` | `review-correctness.json` lần 2 (bị từ chối) |
| F7 | TK-1378 | B4 | `pulse checkpoint` phát `event_type: run.completed` (`src/kernel/checkpoint.rs:150`) — operator đọc `events tail` tưởng run đã kết thúc trong khi attempt 0 vẫn chạy | pulse-bug | `src/kernel/checkpoint.rs`: event riêng `checkpoint.recorded` | event 12:15:20 vs `run.completed {attempt:0,handed_off}` 12:17:59 |
| F8 | TK-1378 | B4 | `run_lane` không emit event nào — lane chạy/thất bại vô hình với `events tail`, chỉ thấy trong log CLI | pulse-bug (observability) | `src/kernel/run.rs`: run.started/completed cho lane | `rg run. events` chỉ thấy worker |
| F9 | TK-1378 | B4 | Oracle `qa-001.sh` của operator: jq `//` coi `false` là falsy → `completed=false` đúng bị đọc là thiếu; thiếu `--arg` cho `$TITLE`. API hoàn toàn đúng, oracle báo sai | target-harness (human) | `scripts/qa/cases/qa-001.sh` (đã sửa) | `logs/QA-001.http.txt`: 201 + list đúng |
| F10 | TK-1378 | B4 | **Kẹt verifying**: qa script không bao giờ emit `findings[]` → fail của qa lane luôn bị seal hạ thành `inconclusive` (quy tắc §8.4) → không bao giờ bounce; cửa verifying→active duy nhất là review-lane fail; `run worker` từ chối verifying; handoff đòi active; transition không có lối verifying → **mọi sửa source sau handoff mà reviewer chấm pass = Ticket kẹt vĩnh viễn** (`close_source_stale` mãi mãi) | pulse-bug (design) | `src/kernel/{run,completion}.rs` + `templates/qa/*.mjs`: (a) qa script map case-fail có check vào findings[]; (b) cho worker resume từ verifying (re-verify → handoff mới); vòng khởi động lại phải chạy được | receipt `01M2N3NDAZAAF83R0ERGK4P7HK` (inconclusive, không bounce); `run_not_ready_or_active` 12:36 |
| F11 | TK-1378 | B4 | Hai không gian id trong cùng field: run event mang `run_01M2N…` còn checkpoint event mang receipt id `TK-1378-worker-1` | pulse-bug (nhỏ) | chuẩn hoá run_id | events 12:13–12:15 |
| F12 | TK-82d7 | B5 | CORSMiddleware thiếu — nằm ngoài `non_scope` của CẢ hai ticket (TK-1378: no web; TK-82d7: no api). Worker tự phát hiện và note friction đúng chuẩn (12:44:50), không tự sửa ngoài scope | target-harness (shaping gap) | operator sửa sau khi cả hai ticket done (commit `4e3f154`); bài học: cross-cutting concern cần một ticket sở hữu | `note.recorded` 12:44:50 + 12:48:48 |
| F13 | TK-82d7 | B5 | Van §7.3 risk=low không tháo được qa-ui vì seed §8.1 `ui-low = [review-correctness, qa-ui]` (khác api-low) → close vẫn đòi qa-ui pass | pulse-bug (seed/valve) | seed PULSE.md: `ui-low: [review-correctness]`, hoặc van theo mechanism khác | `close_lane_not_satisfied` 12:59 |
| F14 | TK-82d7 | B5 | PULSE.md nằm trong source fence → sửa harness config làm `close_source_stale`; đã tự loại qua `fence_ignore: ["PULSE.md"]` | pulse-bug (fence) | `src/source.rs`: fence luôn bỏ PULSE.md/AGENTS.md/runners.json như `.pulse/**` | close pass sau khi thêm fence_ignore |
| F15 | ST-5af4 | B6 | **close-story không có source fence**: tree bẩn (1 dòng thêm vào `api/app/main.py`) không bị từ chối vì bẩn — chỉ báo `close_story_qa_not_satisfied`. Nếu receipt qa đã có, story đóng được với tree bẩn | pulse-bug — cần ADR nhỏ | quyết định: có/không fence ở story level; chưa vá | lần close-story 13:05, HEAD `4e3f154` |
| F16 | ST-5af4 | B6 | Story-scope input chứa case của cả hai surface; `api.mjs` parse step `http://127.0.0.1:3000/` của QA-002 thành HTTP method → `qa_api_crashed` | pulse-bug (template) | `templates/qa/*.mjs`: filter case theo `surface` | `qa_api_crashed` "not a valid HTTP method" |
| F17 | ST-5af4 | B6 | Close-story đòi MỘT receipt pass mới nhất cover đủ mọi case high (`max_by(id)`), trong khi coverage hợp lệ chia theo surface → story đa surface không thể pass nếu không có lane gộp. Đã viết `scripts/qa/all.mjs` (role `qa-all`) | pulse-bug (design) | completion.rs: union coverage qua các receipt pass; hoặc gợi ý lane gộp trong seed | `close_story_qa_not_satisfied: QA-001 is not covered` |
| F18 | ST-5af4 | B6 | Story-scope `qa-ui` vẫn cần `--force`: profile suy từ surface/risk CỦA STORY (api-medium) — A1 chỉ hợp lệ hoá Story làm subject, không bỏ gate profile | plan/code drift | suy profile per-case-surface ở story scope | `lane_not_in_profile` |
| F19 | TK-82d7 | B5 | Oracle `qa-002.mjs` dùng playwright `check()` — React re-render khi PATCH resolve swap node giữa verify → "Clicking the checkbox did not change its state"; ui.mjs còn vứt stderr của check (không post-mortem được) | target-harness (human) + template | `qa-002.mjs`: click + poll; `ui.mjs`: giữ stderr | stderr lần đầu mất, sau patch thấy rõ lỗi |
| F20 | — | B6 | Steps của qa-api không có quy ước dọn dữ liệu → 9 task "buy milk" rác tích tụ trong dev DB | target-harness | `api.mjs`: xoá resource vừa tạo sau case | `GET /tasks` sau 3 lần lane |
| F21 | — | B2 | Playwright phải cài ở **repo root** chứ không phải `web/` (ESM resolve từ `scripts/qa/` đi lên); hướng dẫn ban đầu nói cài trong `web/` là không chạy | doc/briefing | scripts/qa/README.md nêu rõ vị trí node_modules | `qa_ui_playwright_missing` |
| F22 | — | B5 | `ui.mjs` screenshot ngay sau `goto`, không chờ settle → evidence chụp state "Loading tasks…", giảm giá trị chứng cứ | pulse-bug (template, nhỏ) | chờ network-idle / poll đáp ứng trước shot | a11y `Loading tasks…` |

Phân loại tổng: pulse-bug 14, target-harness/human 4, plan/code drift 3, doc 1.
Không có finding nào thuộc loại "agent làm sai vì hiểu sai" ngoài F6 (prompt
thiếu ràng buộc) — agent các lane tuân thủ prompt tốt.

## 2. Số đo §2 đo thật (một Ticket, TK-1378 → TK-82d7)

| Số đo | Đo được | Đích |
|---|---|---|
| Lệnh tay để đóng một Ticket (đường vàng) | **3** (`run worker`, `run review`, `close`) | ≤ 6 ✅ |
| Lệnh tay thực tế cả session cho TK-1378 | ~13 (gồm 2 review hỏng vì seal, 1 `run worker` bị từ chối, 1 qa rerun) | — |
| Flag bắt buộc trên đường vàng | **0** (id là đủ; ttl/continue-limit có default; `--json` tuỳ chọn) | ≤ 4 ✅ |
| `--force` buộc phải dùng | 2 lần (story-scope qa-ui, qa-all — F17/F18) | — |
| Vòng rework worker | **0** (code của worker đúng ở cả hai Ticket ngay vòng 1) | — |
| Vòng lane chạy lại vì hạ tầng/oracle | qa-api ×2, qa-ui ×1, qa-all ×4 trước khi pass sạch | — |
| Số lần mở file thay vì tin CLI | **~11** (events jsonl ×3, evidence ×4, source.rs/completion.rs/run.rs/checkpoint.rs ×4) — nguyên nhân: events tail không render payload, lane không emit event, checkpoint mạo danh run.completed, verdict file ≠ receipt | cần ≈ 0 |
| Wall-clock `run worker` | TK-1378: **4m40s** (attempt 0, 1 checkpoint giữa run); TK-82d7 run 1: ~4m (bị kill); resume: **55s** đến handoff | — |
| Wall-clock lane | review-correctness (codex): ~4m/lần; qa-api: ~1m30s; qa-ui: ~2–4m; qa-all: ~6m | — |
| Token/cost | **không đo được** — `claude -p`/`codex exec` không in usage ra stdout ở chế độ này | — |
| Friction/Ticket là lỗi Pulse | 14 pulse-bug / 2 Ticket ≈ **7** (gần nửa nằm ở templates qa + gate story) | < 1 ❌ |
| Repo chạy Pulse thật | **1** (todolist, UI + API, cả hai Ticket đi trọn golden path) | 1 ✅ |

## 3. Từng lane

- **review-correctness (Codex, workspace-write)**: TK-1378 **pass** — có chạy lại
  `verify[]` thật: `commands_run` ghi 2 lần `uv run pytest` (lần đầu exit 2 vì
  sandbox chặn `~/.cache/uv`, tự workaround `UV_CACHE_DIR=/tmp`, 22 passed);
  4/4 AC có `how` nêu lệnh quyết định. TK-82d7 **pass** (23 vitest, 4/4 AC).
  Artifact đúng loại (`review-correctness.json` sealed). Không false positive.
  Đánh giá: reviewer THẬT SỰ kiểm claim, không đọc story (không có handoff
  summary trong input — đúng §8.3).
- **qa-api**: pass (story-scope, sau khi sửa oracle F9). Artifacts
  `logs/QA-001.http.txt` (POST 201 + GET list) + `QA-001.server.txt` ✓, stop
  exit 0 ✓. Không false positive; hai lần "fail" đều là oracle/hạ tầng, không
  phải code.
- **qa-ui**: pass (story-scope qua qa-all). 2 artifact ảnh/case ✓, console sạch
  ✓, a11y có `textbox "New task"` ✓. Lần ticket-scope là inconclusive do app
  stale (F2) — shot "đúng loại nhưng sai app": **evidence đúng định dạng vẫn
  có thể nói dối về app nào được chấm**.
- **check-docs**: đã chạy thủ công (`pulse docs check` pass, exit 0) nhưng
  không chạy qua lane trong session này (không Ticket nào có profile docs).

## 4. Kill/resume

- Kill `claude -p` ngay sau checkpoint đầu của TK-82d7: runner báo
  `run_inconclusive` (exit 143), **lease giữ nguyên**, Ticket `active`, hint
  chỉ đúng 2 lệnh cứu (`pulse run worker` / `pulse release`) — đúng §10.1.
- Resume: packet có checkpoint; worker **không làm lại** — sau resume chỉ có
  checkpoint #2 (12:48:44) và handoff (12:48:48–57); không có file source mới
  nào được tạo so với snapshot trước resume; tổng thời gian resume→handoff
  55s. Bộ nhớ công việc nằm trọn trong cp.json (done_ac 4/4, decisions,
  gotchas).
- Detector 70% context: **không có điều kiện bắn** trong session (agent con
  không đầy context); marker `.pulse/runtime/context-threshold` không từng
  xuất hiện — cơ chế còn `unexercised` (doctor sẽ phải cảnh báo).

## 5. Prompt — chỗ agent làm sai vì prompt

1. `review-correctness.md`: không nói schema đầu ra là ĐÓNG (F6) — mất một
   review hoàn chỉnh. Đề xuất: thêm 3 dòng pin schema (đã làm ở bản copy).
2. `worker.md`: tốt — checkpoint sau mỗi AC, protocol, không chạm runners.json;
   worker tôn trọng `non_scope` đến mức KHÔNG sửa CORS dù UI chết (F12) — đây
   là prompt làm đúng, kết quả vẫn hỏng vì shaping thiếu ticket sở hữu.
   Đề xuất thêm cho worker: khi một friction nằm ngoài scope chặn AC, nêu rõ
   trong handoff `open_risks` (worker đã làm qua note).
3. Seed runners.json: sandbox `read-only` cho reviewer (F5) là chỗ prompt của
   lane ("You may only write under .pulse/evidence/") mâu thuẫn trực tiếp với
   sandbox của host.

## 6. Packet — dùng / thiếu

- Worker dùng thật: `issue` (objective/change/acceptance/verify/non_scope),
  `story.rules/exceptions`, `docs.applicable` (2 docs, có `lines` — worker đọc
  cả hai), `protocol`, `checkpoint` (resume), `source`.
- Thiếu: `story.qa_cases` (F4 — worker không thấy oracle).
- Thừa/rỗng: `decisions`, `blockers`, `notes` (rỗng đúng lúc đầu), `epic`
  (worker ít dùng nhưng rẻ). Không trường nào gây nhiễu nghiêm trọng.
- `last_verdicts` chỉ xuất hiện ở vòng rework — vòng rework không xảy ra với
  worker nên trường này `unexercised`.

## 7. Ba việc đầu cho Phase 3 + hai chỉ tiêu

Ba việc, theo bằng chứng (mỗi việc unblock phần còn lại):

1. **Sửa lifecycle + seal của qa lane templates** (F2/F10/F16/F19/F22): chờ
   start exit, filter surface, case-fail có check → `findings[]`, giữ stderr
   của check, settle trước shot. Không sửa thì mọi lane QA của mọi phase sau
   vẫn flaky và mọi rework loop qua qa vẫn chết.
2. **Cửa verifying→active** (F10/F13/F14): cho `pulse run worker` tiếp nhận
   Ticket `verifying` (resume → re-verify → handoff mới, snapshot mới) và đưa
   fence ra ngoài harness config (PULSE.md/AGENTS.md/runners.json). Đây là
   khác biệt giữa "close gate giữ tính trung thực" và "close gate kẹt кок".
3. **Event + observability** (F7/F8): `checkpoint.recorded` tách khỏi
   `run.completed`, lane emit run events, `events tail` render payload.
   Session này phải mở tay ~11 file chỉ vì ba khoảng trống này.

Chỉ tiêu §2:

- **src 13.126 dòng (đích < 10.000)**: Phase 3 còn thêm board/doctor/SPEC —
  nếu không kèm xoá thì sẽ phình. Đề xuất ràng buộc: mỗi mechanism mới ở
  Phase 3 phải kèm một xoá/gộp (vd. gộp `learn list`+`show`, bỏ `work dep rm`,
  gộp `events` vào một lệnh đọc) — với 27 leaf hiện tại, đích ≤ 22 đạt được
  bằng đúng 5 lần gộp/bỏ như vậy, không cần đụng năng lực nào.
- **Friction/Ticket = lỗi Pulse ≈ 7** so với đích < 1: gần nửa là template qa
  (chưa từng chạy thật trước P2) — mật độ lỗi này là lý do Phase 2 tồn tại;
  sau việc (1) ở trên, chạy lại dogfood ST-2 để đo lại chỉ tiêu này trên một
  Story "lạnh" là phép thử thật sự của v3.0.

---

## 8. Dogfood ST-2 — golden path qua `pulse-shape`/`pulse-plan` (2026-09-17)

Session ngay sau ST-1, trên cùng target, Story mới **ST-332b** ("Today/
Upcoming/Overdue views + due dates + priority") — lần đầu hai skill mới
(commit `977667f`) được dùng thật từ đầu đến cuối: shape → plan → golden
path ×2 → story-scope qa ×2 → close-story. Kết quả: **ST-332b done, cả hai
Ticket done, QA-003 (api) + QA-004 (ui) pass trên HEAD, friction/ticket =
0.5** (đích < 1; baseline ST-1 ≈ 7). Todolist HEAD cuối: `95e9d4b`.

### 8.1 Shape (pulse-shape)

Một cuộc phỏng vấn, 6 quyết định D-1..D-6, mỗi câu kèm recommended answer:
date-only due (D-1), phân hoạch 4 views uncompleted-only (D-2), priority
low/medium/high nullable (D-3), `?view=` server-side (D-4), tabs + form +
inline edit (D-5), và D-6 — **product timezone** — bắn sau khi đo được
host `09-17` vs container `09-16` UTC: "server local date" bị loại vì sẽ
sai view 7 tiếng mỗi đêm; chốt `TZ=Asia/Ho_Chi_Minh` cho service api.
Story `ready` ngay sau `pulse docs check` pass, không câu hỏi blocking.
Oracle authored trong shape: `qa-003.sh`, `qa-004.mjs`.

### 8.2 Plan (pulse-plan)

Cut 2 tracer bullets sau một lần duyệt grain/edges: TK-ea8f (api, low,
QA-003), TK-d1e3 (ui, low, QA-004, `blocked_by` TK-ea8f). 14 anchors đọc
từ disk. **Gate từ chối ready TK-d1e3** (`ready_blocked_by_open`) — đúng
§7.1-4 nhưng lệch plan §15 bước 3 ("cả hai ready"); ST-1 đã đi vòng bằng
cách ready ui sau khi api đóng. Drift plan/gate cần sửa text ở lần viết
SPEC (P3.4); gate giữ nguyên.

### 8.3 Golden path

| Bước | TK-ea8f (api) | TK-d1e3 (ui) |
|---|---|---|
| `run worker` | 3m44s, handoff 4/4 AC, checkpoint ×1 | 5m59s, handoff 4/4 AC, checkpoint ×1 |
| `run review` | pass, 85s, 0 findings | pass, 2m19s, 0 findings |
| `close` | done, sạch | done, sạch |

Vòng rework worker: **0**. Vòng lane chạy lại: qa-api ×1 (lỗi shaping,
xem 8.4). Lệnh tay mỗi ticket: 3 (`run worker`, `run review`, `close`),
0 flag bắt buộc. Worker TK-d1e3 tự chạy thử QA-004, phát hiện CORS
allow-list buộc UI chạy :3000, ghi note thay vì sửa ngoài scope — prompt
đang làm đúng.

### 8.4 Friction (8 note `--friction`, phân loại theo cách ST-1)

| id | Ticket | mô tả | loại |
|---|---|---|---|
| F25 | TK-ea8f | Worker break stdout contract (dòng cuối không phải JSON) → `run_inconclusive` dù handoff đã seal sạch 9s trước; nguyên nhân gốc không chứng minh được vì **runner vứt captured stdout/stderr sau khi classify** (`src/runner/mod.rs` drain_bounded → `run.rs`) | pulse-bug (observability) + trigger là agent |
| F26 | TK-ea8f | qa-api lane khởi động app từ `run.md` nhưng không bước nào chạy `alembic upgrade head` — db volume mới sẽ đứng ở 0002 mãi; worker phải tự migrate tay | target-harness (run.md start contract) |
| F27 | TK-ea8f | Worker ghi friction trùng nhau 2 lần (F26) + dán nhãn friction cho một gotcha triển khai (pydantic Strict) | agent (over-labeling) |
| F28 | ST-332b | Shaping viết step QA-003 dạng pseudo-JSON (`{title, due_date: today, …}`) — api.mjs THỰC THI mọi step surface-api như METHOD/path JSON-line nên parseStep crash cả lane (`qa_api_crashed`) trước khi kịp ghi artifact; qua `pulse run` crash output vô hình (cùng gap F25) | target-harness (human shaping) |
| F29 | TK-d1e3 | CORS allow-list chỉ phủ origin cấu hình; UI phải chạy :3000 cho QA — worker tự phát hiện, note đúng chuẩn | target-harness (docs gap) |

Không có bug mới ở gate/lifecycle/store/close — phần cơ chế đã vá sau ST-1
(cf16acd: verifying-door, fence, union coverage; templates: surface filter,
await_exit, settle) **chạy sạch lần đầu**. `--force` phải dùng: 0.

### 8.5 Số đo ST-2 vs đích

| Số đo | ST-1 | ST-2 | Đích |
|---|---|---|---|
| Friction/Ticket là lỗi Pulse | ≈ 7 (14/2) | **0.5 (1/2)** | < 1 ✅ |
| Lệnh tay đóng một Ticket | 3 | 3 | ≤ 6 ✅ |
| Flag bắt buộc | 0 | 0 | ≤ 4 ✅ |
| Vòng rework worker | 0 | 0 | — |
| `--force` | 2 | 0 | — |
| Mở file tay vì thiếu observability | ~11 | 2 (evidence qa-api.json + events tail) | ≈ 0 |
| Wall-clock worker | 4m40s / 55s resume | 3m44s / 5m59s | — |
| Friction tổng (mọi loại) | 22 | 8 note (5 vấn đề thật) | — |

### 8.6 Ba việc ứng viên cho Phase 3 (theo bằng chứng ST-2)

1. **Persist bounded run output** (F25/F28 — hai friction cùng loại, đủ
   ngưỡng ≥2): tail stdout/stderr vào `.pulse/evidence/<id>/` cho mọi run,
  không chỉ worker. Cần quyết định riêng + test, không vá trong session này.
2. **run.md start contract cho migration** (F26/F29): mẫu seed
   `docs/operations/run.md` nên tách `start` thành up+upgrade (hoặc thêm
   khóa `migrate:`) — sửa ở templates/seeds, một dòng kèm test fixture.
3. **Sửa plan §15 bước 3** theo gate thật (§8.2): Ticket bị chặn chỉ
   `ready` sau khi blocker done — text vào SPEC.md ở P3.4.

P3.5 (xoá skills v2 dưới `references/pulse-v2-skills/`) — điều kiện
"pulse-shape/pulse-plan chạy thật" đã đạt; xoá ở đầu Phase 3, giữ eval
fixture là records ST-332b.
