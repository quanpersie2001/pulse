# Plan 0026 — Eval skills (G3 của plan 0025): đo skill bằng `claude -p`, chấm cơ học

Trạng thái: **nháp chờ duyệt** (viết 2026-09-19 sau khi dogfood 0025 chạy và
14 friction được khép — xem [`0025-dogfood.md`](0025-dogfood.md)). G3 bị
hoãn trong 0025 với điều kiện "chỉ đáng làm khi B/D/C/E/F/G đã chạy đủ một
vòng dogfood để biết skill nào cần đo" — điều kiện đã thỏa, và chính bảng
friction là dữ liệu chọn gì để đo. File này tự đủ: đọc AGENTS.md rồi file
này là làm được.

## 0. Tư tưởng và câu hỏi eval trả lời

Eval không đo "agent giỏi không". Eval đo **một skill/prompt có đổi hành vi
theo hướng harness muốn không**, bằng chạy ghép cặp (paired): cùng prompt,
cùng fixture, arm A có nội dung skill, arm B không — rồi chấm **cơ học**
trên state mà Pulse ghi lại (issues.jsonl, receipts, transcript), không
LLM-judge. Ground truth là assertion, không là cảm nhận.

Dogfood 0025 chỉ đích danh ba bề mặt mà lỗi thật xảy ra — đó là ba eval:

| Eval | Bề mặt | Friction nguồn | Hành vi cần cải thiện |
|---|---|---|---|
| E1 | `skills/pulse-plan` | F3, F10, F11 (lỗi cắt: thiếu touches cho doc, thiếu `blocked_by` content-prerequisite) | cắt đúng: docs_to_update có owner + nằm trong touches; prerequisite có edge |
| E2 | `templates/prompts/worker.md` | F4, F9 (scratch đè nhau; `learnings_used` sai shape → 4 refusal) | worker chạy từ packet đến handoff sạch, không đụng file của worker khác |
| E3 | `templates/prompts/review-*.md` | F7 (seat loop 9.091 calls, không có điều kiện dừng) | seat seal trong trần budget hoặc dừng đúng quy tắc |

Các cơ chế verdict "GIỮ" của dogfood (reservation, fence, verify, friction
gate) **không** vào eval — chúng đã có test cơ học trong repo; eval chỉ đo
nơi hành vi nằm ở agent.

## 1. Thiết kế từng eval

Fixture gốc: `tests/fixtures/target-repos/<tên>` — luôn copy ra thư mục tạm
trước khi chạy (flow của `tests/common/fixture_repo.rs`), **không bao giờ**
chạy eval tại chỗ fixture. Fixture cần được bổ sung một Story đã shape sẵn
với ground truth cài sẵn ở mức fixture (ví dụ Story có rule đòi một doc, một
cặp ticket có content-prerequisite ngầm trong code) — bổ sung fixture là
một commit riêng, fixture bất biến.

**E1 — cắt story (pulse-plan).** Prompt: "cut ST-<id> into tickets" (arm A
kèm SKILL.md). Grader đọc `.pulse/issues.jsonl` sau khi chạy:

- mọi `change.docs_to_update` của Story nằm trong `touches` của **đúng một**
  ticket (F10/F11);
- cặp ticket có content-prerequisite (định danh sẵn trong ground truth của
  fixture) có cạnh `blocked_by` (F3);
- mọi ticket qua `pulse work ready` — chạy lệnh sau phiên, không tin lời
  agent;
- mỗi ticket có ≥1 `verify[]` argv hợp lệ (không shell string).

**E2 — worker chạy một ticket (worker.md).** Prompt: packet + "làm ticket
này đến handoff" (arm A kèm worker.md). Fixture dựng sẵn hai ticket song
song rời nhau, cho phép hai hướng chạy riêng biệt. Grader:

- mọi scratch file agent tạo nằm dưới `.pulse/runtime/` và có suffix ticket
  (F4/F5/F6);
- không có refusal `from_file_invalid` trong transcript (F9 — shape
  handoff.json đúng lần đầu);
- `pulse handoff` được chấp nhận (receipt `handoff` tồn tại, verdict sửa
  không cần thiết);
- không có violation đụng `touches` của ticket kia.

**E3 — review seat có budget (review-*.md).** Prompt: lane input đã
`pulse lane input --json` chuẩn bị sẵn + "đóng vai seat". Grader đọc
transcript + evidence:

- số tool calls ≤ 30 (trần trong mục Budget), ≤ 3 lần gọi seal (F7);
- phiên kết thúc bằng: seal thành công, hoặc dừng và báo refusal — không
  có vòng retry vô hạn;
- nếu seal `pass` trên ticket có `verify[]`: có receipt `verify` của actor
  seat (luật hạ `inconclusive` không phải lý do loop).

Mỗi cell (eval × arm) chạy **n = 3**. Báo cáo tỉ lệ đạt theo arm, không báo
số tuyệt đối từ một lần chạy — `claude -p` không tất định.

## 2. Runner và grader

`evals/run.mjs` (Node ≥ 20, không thêm dependency vào repo Rust — runner là
công cụ đo, không phải code product):

1. copy fixture ra `evals/.run/<slug>-<arm>-<n>/` (gitignored);
2. arm A: chèn nội dung skill vào prompt (hoặc file skill tại chỗ nếu cơ chế
   host cho phép); arm B: không;
3. gọi `claude -p "<prompt>"` headless trong thư mục đã copy, chụp
   `--output-format json` + transcript;
4. grader đọc `issues.jsonl`, `.pulse/receipts/`, evidence và transcript,
   đối chiếu `evals/evals.json` (mẫu `id/slug/prompt/expected/graders` của
   `references/repo-harness/evals/evals.json` — giữ cùng hình dạng để lần
   sau có thể tái dùng runner của họ nếu muốn);
5. in bảng tỉ lệ + ghi `evals/results/<ngày>.md`.

**Điểm phải xác minh khi thực thi, không được bịa** (cùng tinh thần lệch G1
đã ghi trong 0025): contract headless của `claude -p` (`--output-format`,
cách đếm tool calls, cách chèn skill) — chạy một lần smoke trước khi viết
grader, ghi hình dạng thật vào README của evals. Nếu contract không cho đếm
tool calls thì E3 đổi sang đếm số lần seal trong receipts + thời gian phiên.

## 3. Ngân sách và bar

- 18 runs (3 eval × 2 arm × 3) × ~30–60k tokens ≈ **0.5–1M tokens** một đợt
  đầy đủ. Chạy được theo eval lẻ — E3 rẻ nhất, chạy trước để thử runner.
- Bar "skill có tác dụng": arm A đạt ≥ 2/3 runs và arm B < 2/3, trên cùng
  fixture, cùng prompt. Không đạt bar ở eval nào thì kết luận là hướng sửa
  tiếp theo của skill/prompt đó (sửa rồi chạy lại eval đó — eval là regression
  test cho prompt).

## 4. Định nghĩa xong

1. Fixture mới (story ground-truth) commit, bất biến, không sinh `.pulse/`
   trong fixture.
2. `evals/run.mjs` + `evals/evals.json` + `evals/README.md` (ghi contract
   `claude -p` đã xác minh) commit.
3. Một báo cáo kết quả thật `evals/results/<ngày>.md` — eval không có số
   thì chưa xong.
4. `cargo fmt --check`, `cargo clippy --all-targets --quiet -- -D warnings`,
   `cargo test --all-targets` vẫn xanh (evals không đụng `src/`;
   `architecture_guards` phải vẫn pass — skill chỉ được nhắc lệnh CLI có
   thật).

## 5. Rủi ro và không làm

- **Non-determinism**: n=3 là minimum có ý nghĩa thống kê rất yếu — chấp
  nhận, vì mỗi run đắt và grader là binary. Nếu biên kết quả mù mờ (2/3 vs
  1/3) → chạy thêm n trước khi kết luận.
- **Grader cơ học mù giá trị mềm** (chất lượng `description`, tone) — chấp
  nhận: eval đo hành vi khách quan được; phần mềm đã được gate khác che.
- **Không làm** trong plan này: đo host hook (cần session sống — việc của
  dogfood, không của eval), đo learning enforcement (chặn ở `pulse learn
  activate` human-only), E5 `learn mine` (cần khảo sát transcript host).
