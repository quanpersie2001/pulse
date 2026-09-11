# Decision 0018: Knowledge plane — đường ra của vòng đời và retrieval có corpus riêng

## Status

Accepted, 2026-09-12. Bổ sung PRODUCT.md §5.6 (recall, vòng đời learning) và
§5.8 (danh sách CLI). Khôi phục có thu hẹp phần đặc tả knowledge retrieval của
`pulse-reboot/12-knowledge-compounding.md`, đã xoá ở `30c4964` theo Decision
0008 và chưa bao giờ được PRODUCT.md hấp thụ đầy đủ.

## Context

Audit ngày 2026-09-11 phát hiện `PRODUCT.md` §5.8 liệt kê năm lệnh không tồn
tại trong CLI. Tra ngược `pulse-reboot/12` cho thấy bốn trong năm cái **có đặc
tả đầy đủ** và bị mất khi 0008 cắt phạm vi, chứ không phải viết nhầm.

### Vòng đời learning cụt một nửa

`pulse-reboot/12` vẽ thang trạng thái có nhánh xuống:

```text
candidate -> reviewed -> validated -> promoted
    |           |           |          |
    v           v           v          v
non_durable   disputed   superseded   retired
```

Code hôm nay có **đủ bảy** trạng thái trong `LearningStatus`, và
`projection.rs:228-231` **đã tôn trọng** chúng khi recall — learning `retired`
bị loại với lý do `learning_retired`.

Nhưng `transition_status` chỉ có hai nhánh: `→Validated` và `→Promoted`. Cả
cột dưới không có đường tới. `KnowledgeStatusArg` (chứa `Superseded`,
`Retired`) chỉ được dùng làm **bộ lọc cho `knowledge list --status`**, không
phải để chuyển trạng thái.

Hệ quả cụ thể: Decision 0009 §`pulse-ratchet` bước 5 ra lệnh *"`misleading`
hai lần thì retire và gỡ"*. **Lệnh đó không thi hành được.** Một learning sai
sẽ ở lại trong recall vĩnh viễn, và nó là loại lỗi tự khuếch đại: learning sai
được inject vào packet, worker làm theo, ma sát sinh ra, ratchet capture thêm
một learning nữa — không có ai gỡ cái gốc.

Đây là nửa ngược của nguyên tắc 12 ("Failure feeds the ratchet"). Ratchet chỉ
là ratchet khi nó quay được cả hai chiều.

### Recall chỉ còn hai chiều trên sáu

`PRODUCT.md` §5.6 mô tả thang score bốn bậc:

> explicit relation > path/symbol > tag/signal > lexical

`pulse-reboot/12` chi tiết hơn: sáu bậc, từ explicit reference xuống lexical
body similarity.

`applicable_knowledge_buckets` (`src/kernel/packet.rs:543`) hiện match đúng
**hai** chiều: `applicability.paths` khớp code anchor, và
`applicability.work_labels` khớp tag của Ticket. Cộng một đường thứ ba: learning
có `confidence == Enforced` vào `suggested` kể cả khi không match gì.

`Applicability` có **mười hai** trường. Hai được đọc. Mười trường còn lại —
`domains`, `surfaces`, `symbols`, `technologies`, `operations`, `risks`,
`signals`, `platforms`, `configurations`, `work_kinds` — ghi vào thì không có
tác dụng gì, im lặng. `validate_applicability` còn **bắt buộc** phải điền ít
nhất một chiều concrete, nên `signals` đang được điền một cách nghi lễ để qua
gate rồi không ai đọc (xem `completion.rs::friction_draft`).

Hệ quả: một learning viết đúng cho vấn đề đang gặp, nhưng `paths` không phủ
file đang sửa, thì **biến mất không dấu vết**. Không có lệnh nào tìm lại được
nó.

### Không có dedupe thì không có supersession

`pulse-reboot/12` nối hai lỗ hổng trên thành một:

> "Compound synthesis phải search prior learnings trước khi create: cùng insight
> + cùng applicability thì corroborate; guidance mâu thuẫn thì mark disputed;
> learning mới thu hẹp/thay thế cái cũ thì supersede với migration note."

Không có `search` thì không có bước dedupe. Không có dedupe thì không ai phát
hiện trùng lặp hay mâu thuẫn, nên `supersede` và `disputed` không có ai kích
hoạt. Corpus chỉ mọc thêm, không bao giờ được dọn.

### `work claim` không có thiết kế chống lưng

Khác với bốn cái trên. Tra cả mười ba file `pulse-reboot/`: **không có**
`pulse work claim`. `02-work-graph.md` nói ngược lại:

> "Claim/lease vẫn dùng runtime coordination contract; `unclaimed` không được
> persist như một status giả trong canonical graph."

Claim là việc của lease saga trong runtime, không phải một lệnh CLI.
`pulse work claim` xuất hiện lần đầu trong `PRODUCT.md` §5.1 dòng 240 sau khi
0008 gỡ daemon, và không ai kiểm lại. Nó là tàn dư của một câu viết vội, không
phải một tính năng bị mất.

## Decision

1. **Hai nhánh thoát của vòng đời được mở.**

   ```text
   pulse knowledge supersede <learning-id> --by <learning-id> --actor <actor>
   pulse knowledge retire <learning-id> --reason <text> --actor <actor>
   ```

   `supersede` ghi relation `superseded_by` từ cũ sang mới và đặt trạng thái
   `Superseded`; nó **từ chối** khi learning thay thế không tồn tại hoặc chính
   là nó. `retire` đặt `Retired` và bắt buộc có lý do — một learning bị gỡ mà
   không ai biết vì sao là mất bằng chứng, không phải dọn dẹp.

   Cả hai **giữ lịch sử**: record không bị xoá, chỉ ngừng route. Đây là cùng
   luật với receipt hết hiệu lực (§5.5): "Receipt cũ không bị sửa khi hết hiệu
   lực; gate chỉ không dùng nó nữa."

   Nguồn hợp lệ: bất kỳ trạng thái không terminal nào. Một `candidate` sai cũng
   phải retire được, không cần leo lên `validated` trước.

2. **`disputed` chưa mở trong đợt này.** `pulse-reboot/12` định nghĩa nó cho
   contradiction chưa resolve, và việc phát hiện contradiction cần search. Mở
   `disputed` trước khi có search sẽ tạo một trạng thái không ai đặt được —
   đúng lỗi mà quyết định 1 đang sửa. Làm cùng đợt search.

3. **Knowledge search là corpus typed riêng, không trộn với docs.**

   ```text
   pulse knowledge search "<query>" [--kind] [--surface] [--domain] [--signal]
                                    [--risk] [--limit N] [--json]
   pulse knowledge get <learning-id> [--summary] [--json]
   ```

   Dùng lại hạ tầng Tantivy của `src/docs/lexical.rs`, nhưng **field weight và
   filter schema riêng**. Không merge learning và doc vào một result list
   untyped: docs là current truth và mang authority; learning là reusable
   historical guidance và không mang authority. Trộn hai thứ làm mất đúng cái
   phân biệt mà nguyên tắc 4 ("one writable source per truth") dựng lên.

   Field boost, theo `pulse-reboot/12`: `title` rất cao; `summary` cao;
   `operations`/`signals`/`symbols` cao; `guidance.do` và `required_checks`
   medium-high; `domains`/`surfaces`/`technologies`/`risks` medium.

   Search mặc định chỉ trả `reviewed|validated|promoted`; loại
   `candidate|superseded|retired|disputed`. Trả summary và lý do, **không trả
   full content** — full learning và provenance phải `get` tường minh. Đây là
   cùng luật progressive disclosure với `docs search` → `docs get` (§5.4).

4. **`knowledge get` là lệnh thật, không phải alias của `show`.** Chúng trả lời
   hai câu khác nhau: `show` là inspect một record với relation và usage
   summary; `get` là **retrieval unit** cho agent đang làm việc, có `--summary`
   để lấy bản rút gọn trong context budget. `PRODUCT.md` dòng 490 đã nói agent
   gọi `pulse knowledge get` trong lúc chạy.

5. **Cache knowledge-search là plane disposable riêng.**
   `.pulse/cache/knowledge-search/` — gitignored, fingerprint theo content hash
   của entry, atomic replace, rebuild deterministic, incremental khi đổi. Cùng
   ràng buộc với `.pulse/cache/docs-search/`. Cache **không bao giờ** là truth
   thứ hai: mất cache thì rebuild, không mất dữ liệu.

6. **Applicability thu về đúng những chiều có người đọc.**

   Mười trường không ai đọc là nợ, không phải tính năng chờ. Chốt theo hai
   nhóm:

   - **Có người đọc sau đợt này:** `paths`, `symbols` (match theo anchor);
     `work_labels` (match theo tag); `domains`, `surfaces`, `operations`,
     `risks`, `signals`, `technologies` (thành **filter và field của search**,
     quyết định 3).
   - **Gỡ khỏi model:** `platforms`, `configurations`, `work_kinds`. Cả ba đến
     từ thiết kế multi-platform mà §6 PRODUCT đã loại (Windows không tier-1,
     không qualification matrix per platform). Giữ chúng là mời người ta điền
     vào chỗ không bao giờ có tác dụng.

   `validate_applicability` sửa theo: chiều concrete hợp lệ là `paths`,
   `symbols`, `signals`, `operations` — không còn chấp nhận một `signals` nghi
   lễ làm điều kiện duy nhất. `friction_draft` sẽ phải điền một chiều thật.

7. **`pulse work claim` bị gỡ khỏi `PRODUCT.md`, không implement.** §5.1 dòng
   240 sửa thành "`ready -> active`: chỉ qua lease (`pulse run`)"; §5.8 và danh
   sách MCP tool bỏ `claim`. Lease vẫn thả được bằng `pulse work release`; sự
   bất đối xứng đó là **chủ ý** — thả lease là thao tác phục hồi cho một run
   chết, còn cầm lease mà không chạy gì là mời người ta giả vờ có một run.

8. **Recall bucket `required` vẫn hẹp.** Quyết định 3 thêm chiều match nhưng
   **không** nới `required`: vẫn chỉ enforced ratchet/policy hoặc Ticket tham
   chiếu tường minh. Match mạnh hơn chỉ nâng tới `recommended`. Agent không tự
   nâng `suggested` thành `required`.

## Alternatives Considered

1. **Chỉ làm supersede/retire, hoãn search.** Hấp dẫn vì rẻ và đúng tinh thần
   "không thêm feature khi golden path chưa chạy". Loại **một phần**: vòng đời
   và retrieval nối nhau qua bước dedupe, nên ADR chốt cả hai. Nhưng thứ tự
   implement vẫn tách (xem plan) — supersede/retire trước, search sau.
2. **Trộn learning vào `docs search` cho rẻ.** Loại: mất phân biệt authority.
   Một learning `validated` không phải doc approved, và một result list untyped
   sẽ khiến agent đọc chúng như nhau. `pulse-reboot/12` loại phương án này với
   cùng lý do.
3. **Semantic/embedding search.** Loại, nhất quán với §6 và §11: lexical trước,
   semantic chỉ khi eval chứng minh recall gap. Thêm model vào một CLI offline
   là đổi hẳn hình dạng sản phẩm.
4. **Giữ mười trường `Applicability` và implement match cho tất cả.** Loại:
   ba trong số đó phục vụ một thiết kế multi-platform đã bị §6 loại bỏ. Giữ
   một trường chỉ vì nó đã tồn tại là cách corpus metadata phình ra mà không ai
   đọc — đúng dấu hiệu §12 "Registry bắt mọi markdown mang metadata dù không
   cần routing".
5. **Implement `work claim` cho đối xứng với `release`.** Loại: đối xứng không
   phải lý do. `pulse-reboot/02` nói rõ claim thuộc runtime coordination, và
   một lease cầm mà không có run nào chạy là state giả — đúng thứ nguyên tắc 2
   ("Evidence over assertion") chặn.

## Consequences

- Learning sai gỡ được. 0009 §ratchet bước 5 thi hành được, và vòng ratchet
  quay được cả hai chiều.
- Corpus knowledge dọn được: trùng lặp phát hiện bằng search, thay thế bằng
  supersede, sai bằng retire.
- Agent tìm lại được learning nằm ngoài phạm vi path/tag — đường duy nhất hiện
  nay để một bài học không bị chôn.
- Thêm một cache plane. `.pulse/cache/` đã có tiền lệ docs-search nên không
  phải cơ chế mới, nhưng là thêm bề mặt phải giữ disposable.
- Ba trường `Applicability` bị gỡ là **breaking change của knowledge record**.
  Theo Decision 0003 (pre-release baseline, không có predecessor decoder), record
  cũ được regenerate hoặc từ chối là schema drift, không viết migration.
- `friction_draft` phải điền một chiều applicability thật thay vì `signals`
  nghi lễ. Đây là cải thiện: candidate sinh tự động sẽ mang chiều match thật.
- `PRODUCT.md` bớt một lệnh không có code, và §5.8 khớp CLI thật lần đầu.

## Kiểm chứng

- `knowledge retire` rồi `knowledge applicable --work <id>` không còn liệt kê
  learning đó, và `excluded` nêu lý do `learning_retired`.
- `knowledge supersede A --by B`: relation `superseded_by` tồn tại, A có trạng
  thái `Superseded`, packet inject B chứ không inject A.
- `retire` không có `--reason` bị từ chối; `supersede --by` trỏ tới id không
  tồn tại bị từ chối; `supersede --by` trỏ chính nó bị từ chối.
- `knowledge search` với learning `retired` trong corpus không trả nó ở kết quả
  mặc định.
- `knowledge search` trả summary; `knowledge get` cùng id trả full content.
- Xoá `.pulse/cache/knowledge-search/` rồi search lại cho cùng kết quả
  (cache là projection, không phải truth).
- Guard: `PRODUCT.md` không còn `work claim`; guard parse lệnh (0009 phần B)
  phủ mọi lệnh knowledge mới.

## Thay đổi

- `src/knowledge/store.rs`: hai nhánh transition mới; `supersede` ghi relation
  trong cùng transaction với status.
- `src/cli/knowledge.rs`: `supersede`, `retire`, `search`, `get`.
- `src/knowledge/model.rs`: gỡ `platforms`, `configurations`, `work_kinds`.
- `src/knowledge/validate.rs`: tập chiều concrete hợp lệ.
- `src/kernel/completion.rs`: `friction_draft` điền chiều thật.
- `src/knowledge/lexical.rs` (mới): index và query, dùng lại abstraction của
  `src/docs/lexical.rs`.
- `src/knowledge/cache.rs` (mới): `.pulse/cache/knowledge-search/`.
- `src/kernel/packet.rs`: thêm chiều match cho `symbols`, `domains`,
  `surfaces`, `operations`, `risks`, `signals`.
- `PRODUCT.md` §5.1 dòng 240, §5.6, §5.8.
- Plan: [`docs/plans/0018-knowledge-plane.md`](../plans/0018-knowledge-plane.md).
