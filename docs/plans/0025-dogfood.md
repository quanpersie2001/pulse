# 0025 — Dogfood ST-33d3: song song theo graph, verify tự quan sát, panel 3/2, vòng học (2026-09-19)

Ngày chạy: 2026-09-19, một session orchestrator (`human:quan`), branch
`dogfood/0025` của target `~/Workspace/Personal/todolist`, Pulse build từ
`features/harness-experimental` sau khi A–F, G1, G2 đã thực thi. Worker,
reviewer, seat panel và voter là agent thật (subagent của host); qa-api/qa-ui
chạy script thật qua compose. Kết quả: **ST-33d3 done** — TK-9470 (api,
medium) + TK-4872 (ui, medium) chạy **song song** trong cùng checkout rồi
close sạch; TK-3005 (api, high) bị reservation tự xếp hàng sau TK-9470 rồi đi
qua **panel 3/2** (`lane reconcile` pass) + adversarial + qa; close-story bị
chặn đúng 2 lỗi (`close_story_friction_unclassified` 28 mục,
`close_story_source_dirty`), hết vòng học (LRN-2b26 candidate, KHÔNG
activate) thì done. Todolist HEAD cuối: `7744e28` (branch `dogfood/0025`,
chưa push). Binary pulse: `~/.cargo/bin/pulse` (bản cũ giữ ở `pulse.bak`).

## 0. init --refresh (G2) — trạng thái từng file

| File | Trạng thái lần đầu | Xử lý | Ghi chú |
|---|---|---|---|
| AGENTS.md (block) | `kept (no base)` | `--take-new agents-block` | local = template cũ, không có chỉnh riêng |
| `.pulse/prompts/worker.md` | `kept (no base)` | `--take-new` | chỉnh ST-1 đã được upstream vào template mới |
| `.pulse/prompts/review-correctness.md` | `kept (no base)` | `--take-new` | cwd hint + schema pin (F5/F6 0022) đã có sẵn trong bản mới (dòng 57, 77) |
| `.pulse/prompts/review-adversarial.md` | `kept (no base)` | `--take-new` | như trên |
| `.pulse/prompts/reconcile.md` | `created` | — | file mới của C3 |
| `.pulse/base/**` | `created` | — | state mới, commit |
| PULSE.md | không nằm trong báo cáo refresh | — | human-owned; panel api-high thêm tay |

Không gặp `merged`/`conflict` (không có base nào) — **đường merge 3-way của
G2 chưa được tập luyện trong session này**, chỉ có đường kept→resolve.
Quy trình resolve khó hiểu ở 2 chỗ (F1, F2). `pulse doctor` ngay sau refresh:
clean — bản mới đọc state cũ không gãy.

## 1. Bảng friction

28 friction note (`pulse note --friction`) được ghi trong session, gom theo
nguyên nhân gốc thành 14 dòng. Loại ∈ pulse-bug · pulse-design ·
prompt/template · skill · target-harness · agent · docs. Cột **Xác minh**
là kết quả mở code đối chiếu từng dòng ngày 2026-09-19 (session sửa sau
dogfood — dogfood do agent viết, có thể chẩn đoán sai nguyên nhân); khi có
tái phân loại thì ghi rõ.

| F# | Ticket | Pha | Chuyện gì (lệnh + output rút gọn) | Loại | Sửa đề xuất (repo pulse) | Bằng chứng | Xác minh |
|---|---|---|---|---|---|---|---|
| F1 | ST-33d3 | 0 | `pulse init --refresh` phải gọi `--take-new <file>` **từng file một**; mỗi lần gọi chạy lại toàn bộ refresh và in lại mọi file chưa giải quyết là `kept — no base`, đọc như bị thui lại; chấp nhận cả display name (`agents-block`) lẫn path (`prompts/worker.md`) không nói cái nào là chuẩn | pulse-design | `src/kernel/init.rs`: nhận nhiều file/lần, in bảng trạng thái ổn định, chốt một cú pháp tham số | note `ST-33d3#evt_01M2WK5TGVR…`; log refresh 4 lần gọi liên tiếp | Đúng code: `initialize_repository_ext` nhận đúng một cặp (file, action)/lần gọi; mỗi lần render lại mọi unit và in lại toàn bộ `kept`; chỉ nhận unit label — tên `AGENTS.md` thật bị từ chối `init_refresh_unknown_file`. Giữ pulse-design → **quyết định** |
| F2 | ST-33d3 | 0 | `--take-new agents-block` in "repository initialized: **created AGENTS.md**" trong khi AGENTS.md đã tồn tại, chỉ block bên trong được thay (markers dòng 1/133 còn nguyên) | pulse-design | `src/kernel/init.rs` (`RefreshReport.created`): thêm nhãn `updated-block` | note `ST-33d3#evt_01M2WK5THW4…`; `git diff AGENTS.md` | Đúng: `created.push("AGENTS.md")` bám `block.wrote_live` kể cả khi file có sẵn. **Tái phân loại pulse-bug** (thông báo sai sự thật, không đổi luật nào) → **đã sửa** `c018b89` + test hồi quy |
| F3 | ST-33d3 | 1 | `pulse frontier ST-33d3` kỳ vọng T1+T2 runnable / T3 waiting; thực tế **TK-3005 (high) + TK-4872 runnable, TK-9470 waiting** — packer tham lam duyệt theo **id** nên cặp giao `api/app/models/tag.py` được chia theo thứ tự chữ, biến ticket high-risk có content-prerequisite (Tag model của T1) thành "runnable" | pulse-design | `src/kernel/frontier.rs`: tiebreak theo story-order/risk/created_at, hoặc gộp cặp giao thành một nhóm "chọn một" | note `ST-33d3#evt_01M2WKSKPR…`; output frontier 17:4x | Đúng: greedy duyệt theo id (`candidates.sort_by_key(id)`, vé nhỏ thắng — test `overlapping_tickets_serialize_and_the_smaller_id_runs_first`); frontier là phép toán trên file, không thấy content-prerequisite. Giữ pulse-design → **quyết định** (nửa skill đã sửa `299cd10`) |
| F4 | TK-9470 | 2 | worker.md chỉ định scratch tên bare `cp.json`/`handoff.json` ở repo root → worker-2 **đè handoff.json của worker-1** giữa chừng → `pulse handoff TK-9470` bị `handoff_lease_mismatch` (run_id của ticket kia!) + `handoff_acceptance_missing` + `handoff_documentation_missing` — 3 violation không cái nào nêu nguyên nhân thật | pulse-bug (template) | `templates/prompts/worker.md`: bắt buộc tên scratch có suffix ticket; sửa thông báo mismatch để gợi ý "payload file có thể đã bị ghi đè" | note `TK-9470#evt_01M2WN0JSSQ…`; handoff receipt `01M2WMW2E08A…` | Đúng cả hai nửa: worker.md chỉ định `cp.json`/`handoff.json` trần; hai variant message `handoff_lease_mismatch` không nêu nguyên nhân đè file. pulse-bug → **đã sửa**: template `5395724` + message `c0c7eb7`, guard test + test message |
| F5 | TK-9470 | 2–close | Hệ quả dây chuyền của F4/F6: worker-2 ghi đè `cp.json` (nằm trong touches vì F6) **sau** handoff của T1 → `close` bị `close_source_stale` + 2× `close_lane_not_satisfied` ("receipt sealed on a different tree state") → phải chạy cả vòng sửa fence: release → claim lại → sửa run_id → verify → handoff → chạy lại 2 lane → close. Một file scratch = mất ~15 phút + 2 lane chạy lại | pulse-bug (fence/scratch) | fence nên bỏ qua scratch không-track ở root (mở rộng `fence_ignore` mặc định cho `cp*.json`/`handoff*.json`), hoặc receipt lưu per-path tombstone thay vì hash cả scope | close output 18:3x; receipts trước/sau `6ea677b4…` vs mới | Đúng chuỗi cơ chế; nhưng cả hai phương án đề xuất đều là **đổi luật fence** (danh sách mẫu cứng thứ ba cạnh `.pulse/**` — có đường lọt dirt qua mặt bằng đổi tên). Tách: nửa template (giao thức scratch `.pulse/runtime/` per-ticket) **đã sửa** `5395724`; nửa luật fence → **quyết định** (gộp với F6) |
| F6 | TK-9470 | 2 | `handoff_unreserved_changes` **tính cả scratch do chính prompt yêu cầu**: "changed but reserved by no ticket: cp.json, handoff.json" → worker phải `pulse reserve` file rác của mình → kéo scratch vào fence → mở đường cho F5 | pulse-design | `src/kernel/completion.rs`: bỏ qua untracked scratch khớp mẫu đã biết; hoặc worker.md cấp sẵn tên trong `.pulse/runtime/` (đã bị fence bỏ) | handoff refusal lần 1 của worker-1 | Gate làm đúng hợp đồng (dirty sau `fence_ignore` phải thuộc `touches` của ai đó; `.pulse/**` luôn được fence sẵn — không phải lỗi gate). Nửa phương án 2 (prompt chỉ định `.pulse/runtime/`) **đã sửa** `5395724`; nửa đổi luật (bỏ qua mẫu scratch) → **quyết định** |
| F7 | TK-9470 | 2 | Seat review-correctness (lần 1) **loop 9.091 tool calls / 233k tokens / 50 phút**, viết xong evidence (pass) nhưng KHÔNG seal; thủ phạm dây chuyền: seal bị `lane_mutated_workspace` (cp.json churn → F5) và seat không có điều kiện dừng | agent + prompt | `templates/prompts/review-*.md`: thêm mục "Budget & termination" (~20 calls, 1 verify, ≤3 lần seal, hết quyền thì dừng); dispatch của host cũng phải nhét budget | note `TK-9470#evt_01M2WR1221…`; 3 file lạ trong `.pulse/evidence/TK-9470/` | Đúng: cả hai prompt review không có mục budget/dừng; seat không có quy tắc chấm dứt khi seal bị từ chối. prompt → **đã sửa** `5395724` (Budget & stopping, trần 3 lần seal) + guard test; nửa "dispatch của host" là hợp đồng host, đã nhắc trong chính mục Budget |
| F8 | TK-3005 | 3 | `pulse packet TK-3005` **chỉ in dòng header 19 byte**; packet 25.7KB chỉ ra với `--json`. Hai worker độc lập (TK-4872-resume, TK-3005) cùng dính và phải tự dựng lại packet bằng `work show --json` + `learn applicable` — mất chính cơ chế "một input duy nhất" | pulse-bug | `src/kernel/packet.rs` + `src/cli/`: mặc định in packet (JSON), `--json` thành no-op; hoặc render text đầy đủ | note `TK-3005#evt_01M2WT0Z57…`; `wc -c` 19 vs 25744 | Đúng: `handle_packet` render `format!("packet for {id}")` khi không `--json` — đúng 19 byte. pulse-bug → **đã sửa** `c0c7eb7`: packet luôn in chính nó, `--json` no-op được chấp nhận; test end-to-end không `--json` |
| F9 | TK-3005 | 3 | `pulse handoff` từ chối 4 lần: `learnings_used` phải là mảng **struct** `{id, usage}` (2× `from_file_invalid`, worker phải dò shape từ chuỗi lỗi serde và tự đoán usage=`"not_needed"`); `docs_updated` đòi path trần; viết payload sau verify → `handoff_verify_stale` | prompt/template | `templates/prompts/worker.md`: ví dụ handoff.json hiện rõ `learnings_used: [{"id":"LRN-…","usage":"…"}]` + liệt kê giá trị usage + thứ tự "reserve scratch → viết payload → verify → handoff" | note `TK-3005#evt_01M2WT0Z5PW…`; 4 refusal trong report worker-3 | Đúng: worker.md không có ví dụ handoff.json; `learnings_used` là `Vec<HandoffLearning>` serde — chuỗi trần bị `from_file_invalid`; thứ tự verify-trước-payload không được nêu. prompt → **đã sửa** `5395724`: ví dụ đầy đủ + thứ tự, guard test parse JSON theo đúng gate |
| F10 | TK-4872 | 2 | Cả 3 ticket cùng khai `docs_to_update: docs/product/todolist.md` nhưng không ticket nào có nó trong touches; reservation giữ file **qua cả review** → T2 bị chặn handoff (`handoff_documentation_missing` + 2× `claim_files_reserved: "TK-9470 (verifying, in review) holds docs/product/todolist.md"`) tới khi T1 done+commit. Đúng hợp đồng nhưng story đa ticket chạm một doc sẽ tuần tự hoá ở doc | pulse-design + target-harness | `src/kernel/ready.rs`: gate cảnh báo khi ≥2 ticket cùng story khai cùng `docs_to_update`; skill pulse-plan: chỉ một ticket sở hữu doc | note `TK-4872#evt_01M2WMNS1T…`; 2 refusal verbatim trong report worker-2 | Đúng hợp đồng (held set gồm `verifying`; refusal nêu đúng người giữ + file). Nửa skill (một doc một owner, docs_to_update nằm trong touches) **đã sửa** `299cd10`; nửa gate cảnh báo là thêm luật chéo ticket vào ready gate → **quyết định** |
| F11 | TK-9470/4872/3005 | 2–3 | Planner (operator) thiếu touches: `docs/product/todolist.md` (cả 3 ticket) và `web/test/views-page.test.tsx` (T2) → 3 lần `pulse reserve` giữa chừng (được chấp nhận hết — **cơ chế reserve hoạt động đúng**, thiếu là ở người cắt) | target-harness | skill pulse-plan: nhắc "docs_to_update phải nằm trong touches của ticket sở hữu" | notes `TK-9470#evt_01M2WMW2FR8…`, report worker-2/3 | Không phải lỗi Pulse — reserve đúng cả 3 lần. Sửa phía skill → **đã sửa** `299cd10` |
| F12 | TK-9470 | 2 | Oracle QA-005 của operator hỏng 3 kiểu: jq `[.tags] == $want` so mảng-lồng (`FAIL` dù echo byte-identical), step `PATCH /tasks/<id>` bị api.mjs chạy NGUYÊN VĂN (`422 uuid_parsing` — F28 tái phát), và case đòi `GET /tags` (endpoint của T3) tại lane scope của T1 → `verdict: fail` sai oan cho product; sửa: split QA-007 cho T3 + PATCH chuyển vào check script | target-harness (human) | templates/qa `api.mjs`: từ chối step chứa placeholder thay vì gửi lên wire; `check-qa-steps.mjs` bắt `<…>` placeholder | note `TK-9470#evt_01M2WN4V2Z…`; `.pulse/evidence/TK-9470/logs/QA-005.http.txt` | Đúng nửa template: `parseStep` gửi `urlPath` nguyên văn — placeholder lên wire thật. Phần jq + case đòi endpoint của T3 là lỗi oracle phía target (human), không phải Pulse. api.mjs → **đã sửa** `6940826` (refuse placeholder, inconclusive đặt tên step, không khởi động app khi mọi case hỏng) + test; `check-qa-steps.mjs` là script của target repo, không thuộc repo này |
| F13 | TK-4872 | 2 | QA-006 `steps[0] = "open http://127.0.0.1:3000/"` → ui.mjs navigate chuỗi nguyên văn → `qa_ui_crashed: Cannot navigate to invalid URL`; **crash path không viết evidence file** (chỉ stdout) — đúng họ F1/F2 0022 chưa vá hết; sửa steps[0] = URL trần | target-harness (human) + template | `templates/qa/ui.mjs`: bọc mọi crash sau prepare bằng evidence `inconclusive`; README nhấn "steps[0] là URL TRỰC" | note `TK-4872#evt_01M2WS223M…`; stdout crash 2m36s | Đúng: `page.goto` ném, `main().catch` chỉ in stderr — report không được viết. ui.mjs → **đã sửa** `6940826`: steps[0] phải URL trần (kiểm trước readConfig/import/app), crash giữa case → case `inconclusive` + report vẫn được viết; README nhấn rõ; test node hermetic |
| F14 | ST-33d3 | 4 | close-story đòi story-scope qa receipt **trên HEAD hiện tại** — vừa commit xong docs + check script là 2 receipt vừa seal thành stale, phải chạy lại qa-api+qa-ui (10s+25s, rẻ vì container ấm, nhưng là bẫy thứ tự: "commit sau cùng trước, qa story sau cùng nhất") | pulse-design (chấp nhận, đo) | giữ nguyên; docs/operations hoặc skill pulse-learn nên ghi thứ tự đúng | close-story refusal lần 2; seal pass 18:5x | Đúng code + test sẵn (`close_story_qa_receipts_from_an_older_head_do_not_cover`): receipt qa cũ HEAD không đủ. Giữ pulse-design → **quyết định** (khuyến nghị P1 = giữ nguyên, ghi thứ tự vào tài liệu vận hành) |

Tổng sau xác minh: đã sửa trọn vẹn 8 (F2, F4, F7, F8, F9, F11, F12, F13);
sửa nửa khả-dĩ + nửa còn lại là đổi luật chuyển thành quyết định 4 (F3,
F5, F6, F10); chờ quyết định trọn vẹn 2 (F1, F14); không tái hiện được 0;
không phải lỗi Pulse (mà là lỗi planner/target) 3 nửa (F10-half,
F11, F12-half) — mọi chẩn đoán của dogfood đều khớp code, không có mục
nào bị chữa sai bệnh. *(Kết cục cùng ngày: D1–D5 được chấp nhận theo
khuyến nghị — F1, F14 khép lại thành đã sửa; F3/F5/F6/F10 khép ở
"không đổi luật". Xem Kết cục ở §1.1.)*

### Kết quả xử lý

| F# | Kết quả | Commit |
|---|---|---|
| F1 | đã chốt D1-P1 — đã sửa (collapse output refresh) | `bfcecf1` |
| F2 | đã sửa (tái phân loại pulse-bug — thông báo) | `c018b89` |
| F3 | chờ quyết định; nửa skill đã sửa | `299cd10` |
| F4 | đã sửa (template + message) | `5395724`, `c0c7eb7` |
| F5 | đã sửa nửa template; nửa luật fence chờ quyết định | `5395724` |
| F6 | đã sửa nửa template; nửa luật gate chờ quyết định | `5395724` |
| F7 | đã sửa (prompt budget) | `5395724` |
| F8 | đã sửa (packet mặc định in JSON) | `c0c7eb7` |
| F9 | đã sửa (ví dụ handoff.json + thứ tự) | `5395724` |
| F10 | nửa skill đã sửa; nửa gate chờ quyết định | `299cd10` |
| F11 | đã sửa (skill); cơ chế Pulse đúng | `299cd10` |
| F12 | đã sửa (api.mjs); nửa oracle là lỗi human phía target | `6940826` |
| F13 | đã sửa (ui.mjs + README) | `6940826` |
| F14 | đã chốt D5-P1 — cơ chế giữ nguyên, thứ tự qa ghi vào docs | `b6008ea` |

## 1.1 Quyết định chờ chủ repo

Mỗi đoạn: vấn đề → hai phương án → khuyến nghị của session sửa + cái giá.
Kèm trả lời thẳng ba câu số liệu ở §2.

> **Kết cục:** chủ repo chấp nhận khuyến nghị P1 cho cả D1–D5 (cùng ngày
> 2026-09-19). D1 đã thực thi (`bfcecf1` — collapse output refresh), D5 đã
> thực thi (`b6008ea` — thứ tự qa story-scope ghi vào qa README, seed
> run.md và AGENTS block); D2/D3/D4 khép lại ở "không đổi luật" — nửa
> skill/docs đã sửa từ trước (`299cd10`, `5395724`). Không còn quyết định
> nào chờ.

**D1 (F1) — quy trình resolve `init --refresh`.** Vấn đề: `--take-new`
nhận một file/lần, mỗi lần in lại toàn bộ trạng thái, `kept — no base`
đọc như bị từ chối, và cả `agents-block` lẫn `prompts/worker.md` đều được
nhận mà không nói cái nào là chuẩn.
*P1* — giữ một-file-một-lần, chỉ sửa bề mặt: mọi note của refresh in đúng
cú pháp lệnh copy-paste được (`pulse init --refresh --take-new agents-block`),
các file khác trạng thái gom thành một dòng đếm (`…và 3 file kept khác`).
*P2* — nhận nhiều file mỗi lần (`--take-new a b` hoặc lặp flag) + bảng
trạng thái cuối cùng; đổi CLI và schema report.
**Khuyến nghị P1** — resolve là việc hiếm (một lần mỗi lần nâng template),
giá của P2 là một CLI/report schema đổi shape vì tiện một buổi. Giá của
P1: với nhiều file kept vẫn phải gọi N lần.

**D2 (F3) — tiebreak của `pulse frontier`.** Vấn đề: packer tham lam duyệt
theo id nên cặp ticket giao file được chia theo thứ tự chữ, đẩy ticket
high-risk có content-prerequisite thành "runnable" trước ticket viết nền
cho nó.
*P1* — giữ id-order (tất định, đã test, tái lập được); bù bằng planner:
skill pulse-plan đã thêm "content-prerequisite wire `blocked_by` rõ ràng".
*P2* — tiebreak theo risk rồi created_at (high trước): ưu tiên rủi ro cao
nhưng phá "deterministic theo id", và high-first còn giữ file sớm hơn.
**Khuyến nghị P1** — frontier là phép toán file; thứ tự khởi động là việc
của host, host đã có `waiting.reason: frontier` + `blocked_on` để tự điều.
Giá: story có content-prerequisite mà planner quên wire edge thì hai worker
vẫn khởi động ngược — phát hiện được ở `description` (đã viết nền trong
packet) chứ không ở frontier.

**D3 (F5+F6) — scratch và fence.** Vấn đề: scratch untracked ở root bị
`handoff_unreserved_changes` ép `pulse reserve` → vào `touches` → vào scope
fence → churn scratch làm stale close + chạy lại lane (F5: ~15 phút + 2
lane). Nửa khả-dĩ đã sửa: prompt cấp sẵn `.pulse/runtime/<scratch>-tk-<id>`
(per-ticket, đã bị fence). Còn lại: có nên đổi luật cho scratch không?
*P1* — không đổi luật; repo muốn tha thứ thêm thì tự khai `fence_ignore`
trong `PULSE.md` (cơ chế declarative sẵn có). *P2* — thêm danh sách mẫu
scratch cứng vào mặc định fence/`handoff_unreserved_changes`
(`cp*.json`, `handoff*.json`). **Khuyến nghị P1** — giá của P2 là luật
fence có danh sách tên đoán được thứ ba cạnh `.pulse/**`, và mọi mẫu đoán
được là con đường lọt dirt thật (đổi tên file ngoài scope thành
`handoff-x.json` là qua mặt gate). Giá của P1: worker đi lạc khỏi prompt
vẫn dính như dogfood — nhưng prompt giờ được guard-test và scratch nằm ở
vùng đã fence.

**D4 (F10) — cảnh báo ready gate cho `docs_to_update` trùng.** Vấn đề:
3 ticket cùng story khai cùng một doc không ai sở hữu → story tuần tự hoá
ở doc; refusal của reserve nêu đúng người giữ nên có tín hiệu sớm, nhưng
muốn ngăn từ khâu plan.
*P1* — không thêm gate; dựa vào skill (một doc một owner + docs_to_update
nằm trong touches — đã viết).
*P2* — thêm điều kiện ready-gate: ticket khai doc mà ≥1 ticket khác cùng
story đã khai doc đó → violation (hoặc cảnh báo trong report).
**Khuyến nghị P1** — đây là luật chéo ticket đầu tiên lọt vào một gate vốn
per-record; planner hợp lệ vẫn có thể chủ ý cho hai ticket tuần tự ở một
doc. Giá của P1: planner lách skill thì story lại tuần tự hoá — đo lại ở
dogfood kế qua số lần `claim_files_reserved` vì doc.

**D5 (F14) — qa story-scope đóng trên HEAD hiện tại.** Vấn đề: commit sau
khi qa seal làm stale receipt, phải chạy lại qa (ở đây: 10s+25s, rẻ — nhưng
là bẫy thứ tự).
*P1* — giữ nguyên (milestone = cây sạch đã chốt, HEAD-pinned); ghi thứ tự
đúng vào seed `docs/operations/run.md`: "qa story-scope là bước cuối, sau
commit cuối cùng" (đề xuất gốc của dogfood).
*P2* — nới close-story nhận qa receipt theo fence-cây (so `dirty_hash`)
thay vì HEAD: commit docs không stale qa nữa, nhưng cần luật so fence mới
cho receipt qa và bỏ mất tính "milestone chốt đúng cây đã qa".
**Khuyến nghị P1** — chi phí lỗi thứ tự là một lần chạy lại qa rẻ và tự
phát hiện; giá của P2 là thêm một phép so fence thứ ba chỉ để tiết kiệm
25 giây.

**Trả lời thẳng ba câu (§2):**

**(a) Build gãy chéo 0/story — có chuyển worktree-mỗi-ticket không?**
KHÔNG. 0 < ngưỡng 1 lần/story của decision 0025 → giữ single-checkout.
Va chạm liên-worker có xảy ra nhưng ở **tầng scratch-file** (F4, F5),
không ở build — và đã xử lý bằng giao thức scratch per-ticket (commit
`5395724`), không phải bằng cô lập checkout. Xét lại worktree chỉ khi một
story sau có worker thật sự đọc nhánh build của nhau (pytest/vitest đọc
toàn cây) và gãy ≥1 lần.

**(b) `docs_maybe_stale` 0 đúng / 3 oan — đủ chuyển cảnh báo thành chặn
chưa?** CHƯA. 0/3 là tỉ lệ báo sai 100% — chuyển thành chặn bây giờ là
chặn oan mọi ticket api/web. Giữ advisory; nửa docs đã sửa (seed
`docs/README.md`: `applies_to` nên là globs mức-file, không mức-thư-mục —
commit `299cd10`). Chuẩn đề xuất để xét chuyển: ≥80% cảnh báo đúng trên
≥10 cảnh báo của ≥2 story; chưa đạt thì vẫn advisory vĩnh viễn cũng chấp
nhận được (giá của một cảnh báo oan thấp hơn giá của một doc stale bị lọt).

**(c) Panel 3/2 có tìm ra thứ một lane đơn bỏ sót không — giữ, thu hẹp,
hay bỏ?** Trên ticket **xanh**: KHÔNG — 3 seat đồng thanh pass, 0 finding,
0 check; ~2.4× wall, ~3× token; giá trị phân hoá thật nằm ở adversarial
đơn (5 góc tấn công, có probe). Nhưng thế lùi "quorum-không-check →
inconclusive" chưa từng được tập (0 finding để rơi vào), và cơ chế
reconcile chạy trọn vẹn đúng lần đầu → **BỎ là quá sớm; khuyến nghị THU
HẸP**: giữ cơ chế + giữ `panels` trong profile làm công cụ *có điều kiện* —
chỉ bật panel khi có tín hiệu nghi ngờ (`lane_verdict_corrected` > 0 cho
ticket đó, rework > 0, hoặc finding `high` trong adversarial), thay vì bật
mặc định cho mọi ticket `*-high`. Giá: ticket high xanh lần đầu không còn
3 seat chéo — bù bằng adversarial đơn vốn đã chứng minh giá trị trong
dogfood này; đổi lại mỗi lần panel chạy là một lần nó có việc thật để làm.

## 2. BA CON SỐ

| Số | Giá trị | Đích / quyết định |
|---|---|---|
| (a) Build gãy chéo giữa hai worker / story | **0** (pytest và vitest không bao giờ đọc nhánh của nhau; api/web tách biệt hoàn toàn) | ngưỡng 0025: >1 → xét lại worktree ⇒ **GIỮ single-checkout**. Lưu ý: giao thoa liên-worker xảy ra ở **tầng scratch-file** (2 sự cố, F4/F5) — thay vì build, chính là bằng chứng cho việc chuẩn hoá scratch |
| (b) `docs_maybe_stale`: cảnh báo / ĐÚNG | **3 cảnh báo / 0 ĐÚNG — 3 OAN** (cùng một doc `docs/architecture/overview.md`, `applies_to: ["api/**","web/**"]` quá rộng; doc đúng cần sửa đã được worker sửa) | tỉ lệ này **không đủ để chuyển cảnh báo thành chặn** (F3); sửa trước ở phía docs: siết `applies_to` |
| (c) `pulse verify`: min / median / max | **1.4s / 1.8s / 11.2s** (n=21; 20 pass / 1 fail — fail là test flaky thật của worker, catch đúng) | timeout 900s: max chiếm 1.2% ⇒ default ổn, không cần `--timeout` (khớp dự đoán build nguội 1m45s < 10 phút) |

## 3. Số phụ

- **Reserve giữa chừng**: 10 lần thành công / 3 lần bị từ chối
  (`claim_files_reserved` ×2 khi T2 xin doc T1 đang giữ; 1 xin trước khi có
  `--actor`). Trong đó: 3 lần do touches thiếu của planner (F11), 7 lần là
  scratch-file theo tên mới (suffixed).
- **Description gap** (worker phải tự đoán điều description không nói): **8**
  — T1: 3 (cần `sa.Table` trong metadata cho compare_metadata; sketch
  `cascade="all, delete"` sai hướng với relationship secondary; PATCH chỉ-tags
  phải commit riêng), T2: 4 (guard tag-rỗng trên form; container của chip;
  layout dòng aria-live; cách viết doc), T3: 1 (fixture sibling khác mô tả).
  Cộng 2 gap ở **template** (F4 tên scratch, F9 shape learnings_used) không
  tính vào description.
- **Va chạm liên-worker**: 2 sự cố tầng scratch (F4, F5), 0 ở source.
- **Panel 3/2 (TK-3005)**: seat 1/2/3 = 103s/83s/126s (~30k tokens mỗi seat),
  chạy song song ~2 phút wall; adversarial 258s; reconcile --prepare + 3
  voters (~22s, ~5k tokens mỗi) + seal ~1 phút. **Tổng panel ≈ 2.4× wall của
  một lane đơn, ≈3× token.** Ba seat nói y hệt nhau: pass, 0 findings,
  cùng cách xác minh (chạy verify + đọc code) → **panel không mang lại
  finding nào mà một lane đơn thiếu** trên ticket đã xanh; giá trị phân hoá
  nằm ở adversarial (5 góc tấn công, có probe thật). open/unconfirmed/
  resolved = **0/0/0**. Không có finding nào rơi vào thế "đủ quorum nhưng
  không check → inconclusive → kẹt verifying" (không có finding nào để rơi
  vào — thế lùi này vẫn chưa được tập luyện thật).
- **Check.argv trong finding**: 0 seat gắn check cho finding (vì không có
  finding); adversarial tự chạy 5 probe curl thực nhưng để ở `commands_run`,
  không phải `check` — prompt adversarial đòi check-per-finding chỉ có răng
  khi có finding.
- **Hook**: 4 phép thử đúng hợp đồng — file của ticket khác `exit 2` kèm tên
  người giữ; file của mình `exit 0` im lặng; file ngoài mọi touches `exit 2`
  kèm gợi ý reserve; absolute path resolve đúng. Hook chỉ cài vào
  `.claude/settings.json` của repo (gộp, không ghi đè PostToolUse có sẵn),
  chưa có hiệu lực trong session này — gọi tay tại các điểm [HOOK].
- **verify runs**: 21 receipt (20 pass / 1 fail); lane verdicts session này:
  26 pass / 0 fail / 8 inconclusive (inconclusive gồm seat bị kill và qa-ui
  crash — đúng hành vi "không tự chấm").
- **Cơ chế đúng ngay lần đầu, không friction**: `claim_files_reserved` cho
  T3 (message nêu người giữ + file + hint); fence theo scope (lane chạy giữa
  lúc T2 dirty không bị `lane_mutated_workspace` oan); `lane seat` +
  `lane_seat_actor_reused` không xảy ra (mỗi seat một actor); `pulse close`
  0.2s sau khi fence khớp; friction gate liệt kê 28 mục kèm id sự kiện; học
  `--cite` tự hash, `doctor` báo `stale_cites: 0`.

## 4. `pulse metrics` / `pulse doctor` (nguyên văn)

```
tickets_done                       8
friction_per_ticket_done           6.62
friction_unclassified              0
rework_rate                        0.00
lane_verdicts pass/fail/inconclusive 26/0/8
lane_verdict_corrected             1
verify_runs passed/failed          20/1
panel_reconciles                   1
panel_findings open/unconfirmed/resolved 0/0/0
median_claim_to_done_minutes       18.6
learnings candidate/active/retired 4/1/0
learnings suspect                  0
learnings stale_cites              0
learnings enforced                 0
usage helpful/not_needed/misleading 1/1/0
not_derivable: claim_conflicts — a refused claim writes no event, so conflicts leave no trace; counting them needs a failure event, which this session deliberately does not add
not_derivable: rust_lines_src — counts the development repo's own source tree (plan 0022's `find src | wc -l`); no target-repo record encodes it
not_derivable: error_codes_distinct — a static property of the Pulse binary (grep over source), not log data
not_derivable: cli_leaf_commands — a static property of the Pulse binary (recurse --help), not log data
not_derivable: hand_typed_commands_to_close_ticket — plan 0022 measures this qualitatively against a golden path; the log has no event for 'an operator typed a command'
not_derivable: required_flags_on_close_path — plan 0022 measures this qualitatively against a golden path; not log data
not_derivable: repos_running_pulse — the registry (~/.pulse/projects.json) is cross-project and outside any single repo's event log
```

```
store
  clean (every line parses and validates)
receipts
  all readable
leases
  no active ticket holds an expired lease
evidence
  every directory is named by a receipt
lane preparations
  every prepared lane was sealed
awaiting commit
  every dirty path belongs to a ticket still working
learnings
  no suspect learning, no stale citation

doctor: clean
```

## 5. Verdict từng cơ chế (GIỮ / SỬA / BỎ)

- **Description tự do** — GIỮ: 8 gap/3 ticket nhưng không gap nào làm lệch
  hợp đồng; worker tự bù được và note lại; gate `ready_description_missing`
  đủ sàng lọc.
- **touches + reservation** — GIỮ: khoá song song hoạt động chính xác cả hai
  chiều (chặn đúng T3, thả đúng khi T1 done); mọi sự cố là lỗi cắt của
  planner (F10/F11), và reserve là lối thoát đúng.
- **frontier** — SỬA: tiebreak theo raw id khiến ticket high-risk được ưu tiên
  trước content-prerequisite của nó (F3); cần tiebreak theo story-order/risk
  hoặc hiển thị nhóm "chọn một trong hai".
- **Fence theo scope** — GIỮ cơ chế, SỬA vùng phủ scratch: scope lọc đúng cả
  khi repo dirty chéo ticket; nhưng scratch-file trôi vào fence biến một file
  rác thành cả vòng sửa fence (F5/F6) — cần fence_ignore mặc định cho
  scratch đã biết.
- **`pulse verify`** — GIỮ: receipt tự quan sát là nguồn sự thật của handoff
  (bắt được 1 test flaky thật + 2 lần stale do sửa cây sau verify); nhanh hơn
  timeout 600 lần.
- **Luật lane-phải-tự-verify** — GIỮ: mọi seat/lane đều tự chạy và receipt
  của từng actor được seal; không lane nào tin lời khai nữa.
- **Panel + reconcile** — SỬA: cơ chế khép kín (seat mù → prepare → vote →
  arbiter, máy thắng phiếu khi có check) chạy trọn; nhưng trên ticket xanh
  panel = 3× chi phí cho 0 marginal finding, và thế "quorum-không-check →
  inconclusive" vẫn chưa từng được tập. Cần: khuyến nghị panel chỉ khi lane
  đơn từng fail/hạ verdict (dùng `lane_verdict_corrected`/rework làm tín
  hiệu), hoặc seat thứ 2/3 có lens khác biệt bắt buộc (vd. seat 2 = adversarial
  lite).
- **Friction gate ở close-story** — GIỮ: `close_story_friction_unclassified`
  liệt kê 28 mục kèm event id buộc phân loại hết — không story nào nữa kết
  thúc với friction bỏ quên.
- **Learning check_argv** — GIỮ (chưa phán cuối): LRN-2b26 tạo candidate với
  `--friction` + `--cite` + `--check-argv` suôn sẻ, hash cite được doctor xác
  nhận tươi; enforcement trong `pulse verify` chưa được bật (việc human) —
  phán sau lần đầu activate.
- **docs_maybe_stale** — SỬA: 0/3 đúng trên repo này; gate theo
  `applies_to` thư mục-level quá thô (một doc tổng quan "api/**" bị báo oan
  bởi mọi ticket api). Cần `applies_to` theo path cụ thể hơn, hoặc học từ
  docs_written nào thực sự tương ứng.
- **docs_written** — GIỮ: kiểm cơ học (path tồn tại + chứa BR/E id) rẻ, rõ,
  ép rules sống ngoài issues.jsonl; gate pass với doc viết đúng lúc.
- **Hook** — GIỮ: 4/4 ngữ nghĩa đúng (deny-held / allow-own / deny-outside /
  absolute path); chưa tập qua host hook thật vì hook chỉ hiệu lực session
  sau — cần 1 session chạy với hook sống để đo tỉ lệ chặn oan.
- **Refresh 3-way** — SỬA thông báo, GIỮ cơ chế: đường kept→resolve dùng
  được nhưng một-lần-một-file và message "created AGENTS.md" gây hoảng (F1,
  F2); đường merge/conflict của G2 **vẫn chưa được tập luyện** (không file
  nào có base) — cần một repo đã init bằng bản mới để test đàng merged.

## 6. Đường vòng đã dùng để lượt chạy đi tiếp

1. **Tên scratch có suffix ticket** (`handoff-tk-<id>.json`,
   `cp-tk-<id>.json`) — protocol của host cho mọi subagent từ worker-2-resume
   trở đi, sau F4; gitignore hoá ở story close.
2. **Vòng sửa fence cho T1** (F5): `pulse release` (human) → `pulse claim`
   lại từ verifying (cửa resume mở cho claim trực tiếp) → sửa `run_id` trong
   handoff payload → `pulse verify` → `pulse handoff` → chạy lại qa-api +
   review-correctness → `pulse close`.
3. **Commit harness của operator giữa chừng** (oracle qa-005/qa-007, check
   script) để thoát `handoff_unreserved_changes` / `close_story_source_dirty`
   — đúng chủ sở hữu nên không phải reserve.
4. **Split QA-005 → QA-007** + sửa steps qua `pulse work update` sau khi
   story đã ready (qa_cases là mảng ghi đè nguyên vẹn) — phép "sửa oracle về
   phía harness" khi verdict fail là lỗi oracle, không bounce ticket.
5. **Re-dispatch seat review với budget cứng** trong prompt dispatch (15–30
   calls, tối đa 3 lần seal) — seal lần đầu bị `lane_not_prepared` do snapshot
   của seat cũ, agent tự re-prepare theo hint rồi seal.
6. **Workers tự dựng packet** khi `pulse packet` chỉ in header (F8):
   `work show --json` + `pulse learn applicable` — không sửa pulse repo.

## 7. Ghi chú kết thúc

- ST-2e5f (Google OAuth) không bị đụng. Không repo nào được push. Repo pulse
  chỉ nhận đúng file báo cáo này, không commit.
- Commits todolist trên `dogfood/0025`: `835515b` chore(harness) refresh →
  `eaed491` plan(st-tags) → `effed31`/`…` chore(qa) oracle + scratch check →
  `5d0550c` feat(TK-9470) → `2941cd3` feat(TK-4872) → `21988c5`
  feat(TK-3005) → `7744e28` dogfood(0025) cuối.
- Ba con số cho decision 0025: build gãy chéo **0** (giữ single-checkout),
  docs_maybe_stale đúng **0/3** (chưa đủ chặn), verify max **11.2s**
  (timeout 900s dư).
