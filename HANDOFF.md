# Handoff: Pulse — Bước 5 hoàn thành, golden path chạy thật trên examples/todolist

## Trạng thái bàn giao

- Repo: `/Users/quannv.dev/Workspace/Personal/pulse`
- Nhánh: `features/harness-experimental`
- HEAD: `a6e058b` (ngay trên `07bb38a`..`a6e058b` là 20 commit của Bước 5, xem
  `git log 257ef76..HEAD`)
- Working tree: sạch (chỉ file handoff này được cập nhật để bàn giao).
- Cả ba gate xanh trên working tree:

```text
cargo fmt --check          -> OK
cargo clippy --all-targets --quiet -- -D warnings  -> OK
cargo test --all-targets   -> 504 pass, 0 fail
```

## Bước 5 đã đạt: golden path §7 chạy thật, hai Ticket đóng bằng bằng chứng

Target: `examples/todolist/` (Node ESM thuần, không dependency) trong cùng Git
history, KHÔNG nested `.git`, mọi lệnh `pulse` chạy `--repo-root
examples/todolist` hoặc cwd tại đó. Không bao giờ chạy Pulse đột biến với
`--repo-root .` tại gốc repo phát triển.

1. **init** — `pulse init` trên thư mục con hoạt động (Decision 13.1 đã có từ
   trước); tracked đúng theo PRODUCT §4; `runners.json` mặc định bootstrap.
2. **Story + Ticket** — `ST-001` (`qa.md` fenced `pulse-qa`, 2 case qua 2
   risk) + `TK-001` (ticket.md đầy đủ, risk medium, QA required trên ST-001,
   docs required trên `DOC-TODOLIST-BEHAVIOR` đã register, scope `src/**`);
   sync → shaped → ready qua 10 gate families.
3. **Packet** — packet đủ context: ticket.md nguyên văn, story summary, QA
   baseline kèm hash, docs required/suggested, source fence, handoff protocol.
4. **Worker thật** — `pulse run worker` chạy Claude Code headless thật: agent
   đọc packet, implement `completeTodo` + CLI `done` + tests + doc, gọi
   `pulse work handoff --lease … --session … --source-commit …` đúng cú pháp
   (lease-bound, `run_handed_off`).
5. **QA + docs + reviewer + close** — `pulse run qa` chạy `scripts/qa-run.mjs`
   (script của repo) ghi `qa_checkpoint` passed cho cả 2 case; `docs validate
   --record` ghi `documentation_validation`; `pulse run reviewer` chạy agent
   khác, tự chạy verify + CLI round-trip, ghi verification passed với proofs
   AC→checks+receipts; `pulse work close` đóng TK-001 (close_4655176f).
6. **Kill giữa chừng + sửa source sau handoff** — kill -9 `pulse run` giữa
   flight: lease sống, ticket active, không run record; chạy lại resume cùng
   lease. Sửa file sau handoff làm dirty fence stale: verify bị từ chối
   `verification_source_mismatch`, và phải `work release` (mở rộng mới) để
   thoát. Lại có episode worker agent khai handed_off mà không làm gì
   (worktree-stranded, xem finding 12): reviewer chặn, không có receipt giả
   nào được dùng để close.
7. **Knowledge ratchet** — `knowledge capture --from TK-001` (LRN-001,
   provenance tự suy) → `validate-learning --evidence <receipt>` → `promote
   --document DOC-TODOLIST-BEHAVIOR`; `TK-002` (ticket sau chạm cùng path) thấy
   LRN-001 inject trong packet với why_applicable + required_checks.
   `TK-002` cũng đã chạy full cycle thật và đóng (close_a166a0ec).

## Commit của Bước 5 (mỗi commit xanh cả ba gate)

1. `07bb38a` — app todolist + AGENTS/PULSE/docs/verify/qa-run
2. `9acfe45` — runner bootstrap commands/prompts khớp CLI thật
3. `6e9826b` — pulse init examples/todolist + register docs
4. `f53cdd5` — ST-001 + TK-001 ready
5. `5384eea` — packet handoff protocol syntax + story.md
6. `1adf61c` — runner:worker provisioning state
7. `da8f654` — dirty identity hash theo normalized path (subdir repos)
8. `871b392` — content-hash binding phủ uncommitted bytes
9. `af3330c` — so sánh source binding theo subtree identity
10. `30030b7` — handoff idempotency key có attempt suffix
11. `470ca19` — release recovery phủ verifying tickets
12. `70a6c9c` — reviewer input mang proof receipts
13. `49becbb` — verify key cũng có attempt suffix
14. `0be448b` — receipt tham chiếu nhiều proof tính một lần
15. `f110efe` — fence receipt so với expected commit theo subtree
16. `1903c00` — close releases lease
17. `679879d` — knowledge ratchet ladder + packet injection
18. `0d75c6d`, `a6e058b` — land TK-001, TK-002 (code + evidence + events)

## Finding/Bài học khi dogfood (đã sửa code, trừ mục ghi rõ)

1. Default runner commands dùng flag CLI không tồn tại + `--output-format
   json` phá contract JSON-dòng-cuối → đổi sang pointer prompt + text output
   (PRODUCT §5.3 đã cập nhật mẫu).
2. `worker-prompt.md` chỉ sai cú pháp handoff → viết lại với lệnh đúng, điền
   sẵn lease/session/commit.
3. Packet `handoff.commands` sai cú pháp → sửa.
4. Dirty fence của repo-con: `git diff` với pathspec `:(top)` trả path
   top-relative, `ls-files --others` trả cwd-relative → mọi file dirty đều
   break hoặc bị bỏ sót. Đã fix bằng normalized path + `--full-name`.
5. `.pulse/policy/` + `.pulse/config/` chưa nằm trong Pulse metadata loại trừ
   khỏi dirty identity → provisioning runner làm fence bẩn. Đã fix.
6. Receipt content-binding từ chối file dirty-với-HEAD dù hash khớp, trong khi
   close lại chấp nhận → record-time chỉ check commit relation; dirty thuộc
   về handoff fence.
7. Source binding so raw commit id: commit ngoài subtree (chính repo Pulse!)
   làm gãy proof chain của repo-con → so theo tree hash tại prefix
   (`source::same_source_state`).
8. Handoff/verify idempotency key deterministic khiến resume replay receipt
   cũ → prompt dùng attempt suffix.
9. Không lối thoát cho ticket `verifying` khi proof stale → `work release`
   giờ demote verifying→ready; chain cũ không bao giờ close được nữa vì close
   bind revision.
10. Reviewer để sót docs receipt trong proofs → reviewer input giờ list sẵn
    receipt candidates.
11. Close đếm trùng receipt tham chiếu từ nhiều proof → dedupe.
12. Close không release lease (sai PRODUCT §5.5) → lease Zombie ép ticket sau
    vào worktree isolation; work của worker rơi vào worktree. Đã fix; worktree
    là runtime disposable — công việc trong đó phải được salvaged thủ công
    (đã làm cho TK-002).

## Khoảng cách còn lại (nhận thức, không phải việc đã xong)

- **Story close (ST-001) chưa chạy**: cần `qa_checkpoint` scope
  `story_close` passed trên HEAD hiện tại; `scripts/qa-run.mjs` mới chỉ ghi
  `ticket_checkpoint`. Cần runner input cho story scope hoặc flag script, rồi
  `pulse work close-story ST-001`.
- TK-002 review cycle dùng receipt docs do reviewer tự record; nếu muốn tách
  bạch actor, `docs validate --record` cho reviewer role cần xem lại grant.
- Reviewer output contract (`disposition/acceptance/findings`) vẫn chưa được
  parse/classify trong `classify_outcome` (reviewer "completed" chỉ cần JSON
  cuối parse được; content check thuộc review layer — đã thấy rõ trong episode
  TK-002).
- Artifact ingest (PRODUCT §5.3 bước 7) chưa wire vào `pulse run`.
- QA input gửi case id+revision, không gửi nguyên văn intent/steps/expected
  (script tự đọc qa.md trong repo — chấp nhận được cho local-first, cân nhắc
  đưa đầy đủ vào input).
- Knowledge: chưa có `applicable --work`, `reviewed` transition, usage
  feedback; naming `validate-learning` lệch PRODUCT (`validate` đã bị chiếm bởi
  store check).
- `work edit` vẫn chỉ sửa title; `ticket.md` + `work sync` là đường cập nhật.

## Việc tiếp theo đề xuất

1. Story qualification: thêm scope `story_close` vào qa flow + chạy
   `close-story ST-001` (đóng tiêu chí phụ §7).
2. `knowledge applicable --work <id>` CLI + usage feedback ở handoff.
3. Classifier cho reviewer: `rework` phải map verification rework thật
   (như worker `unproven_claim`), không chỉ JSON parse được.
4. PRODUCT §8 cập nhật cột hiện trạng cho 5.6 (ratchet đã có ladder + inject).
