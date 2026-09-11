# Plan: Knowledge plane (Decision 0018)

> Kế hoạch thực hiện cho [Decision 0018](../decisions/0018-knowledge-plane-retrieval-and-lifecycle-exits.md).
> ADR chốt **cái gì và vì sao**; file này chỉ nói **làm thế nào và theo thứ tự
> nào**. Khi hai file mâu thuẫn, ADR thắng.
>
> Trạng thái: chưa bắt đầu. Không có bước nào trong đây được thực hiện.

## Nguyên tắc xếp thứ tự

Ba ràng buộc quyết định thứ tự dưới đây, không phải độ khó:

1. **Bước nào gỡ được một lỗi đang chảy máu thì đi trước.** Learning sai không
   gỡ được là lỗi tự khuếch đại (ADR §Context); mỗi ngày trôi qua là thêm
   corpus rác.
2. **Bước nào mở khoá một skill đang chết thì đi trước bước làm skill đó.**
   `pulse-ratchet` (0009 phần B) không viết được cho tới khi có `retire`.
3. **Breaking change của record đi trước khi có nhiều record.** Gỡ ba trường
   `Applicability` rẻ nhất lúc corpus còn gần rỗng.

## Giai đoạn 1 — Đường ra của vòng đời

**Mở khoá:** 0009 §ratchet bước 5. **Kích thước:** nhỏ. **Phụ thuộc:** không.

### 1.1 `knowledge retire <id> --reason <text>`

`transition_status` thêm nhánh `* -> Retired`. Nguồn hợp lệ: mọi trạng thái
không terminal (`candidate`, `reviewed`, `validated`, `promoted`). Lý do bắt
buộc, non-empty sau trim, đi qua `redaction` như mọi trường text của plane
tracked (Decision 0012 §4).

Lưu `reason` ở đâu: `Promotion.rationale` đang dùng cho promote, không mượn.
Thêm trường riêng hoặc ghi vào payload event `knowledge.learning.retired` —
**chốt khi implement**, ưu tiên phương án không thêm trường model nếu event đủ
cho `knowledge show`.

### 1.2 `knowledge supersede <id> --by <id>`

Nhánh `* -> Superseded`, **và** relation `superseded_by` từ cũ sang mới, ghi
trong **cùng một transaction** với status. Hai thứ lệch nhau là một learning
superseded không ai biết bởi cái gì.

Từ chối: `--by` trỏ id không tồn tại; `--by` trỏ chính nó; `--by` trỏ một
learning đã `retired` hoặc `superseded` (thay thế bằng thứ đã chết).

**Bẫy đã biết:** `KnowledgeStore::create` và `list` tự lấy write guard, flock
không reentrant. Nếu `supersede` cần đọc learning đích trong fence thì dùng
biến thể `_unlocked`, theo tiền lệ `create_unlocked` và
`register_unlocked`.

### 1.3 CLI và test

`src/cli/knowledge.rs` hai subcommand. Test: mỗi ca từ chối ở §Kiểm chứng của
ADR, cộng một ca end-to-end — retire một learning đang được packet inject, rồi
khẳng định packet không còn nó và `excluded` nêu `learning_retired`.

### Ra khỏi giai đoạn 1 khi

`pulse knowledge retire` và `supersede` chạy, ba gate xanh, và 0009 §ratchet
bước 5 thi hành được bằng lệnh thật.

---

## Giai đoạn 2 — Thu gọn Applicability

**Mở khoá:** giai đoạn 3 (search filter bám trên các chiều còn lại).
**Kích thước:** nhỏ. **Phụ thuộc:** không, nhưng nên đi trước 3 để khỏi index
trường sắp gỡ.

### 2.1 Gỡ ba trường

`platforms`, `configurations`, `work_kinds` khỏi `Applicability`. Theo Decision
0003 (pre-release, không predecessor decoder): record cũ mang chúng bị từ chối
là schema drift, **không viết migration**.

Kiểm trước khi gỡ: `grep -rn "platforms\|configurations\|work_kinds" src/ tests/`
— nếu có nơi đọc thật thì ADR §6 sai và phải quay lại ADR, không lặng lẽ giữ.

### 2.2 Siết `validate_applicability`

Chiều concrete hợp lệ thu về `paths`, `symbols`, `signals`, `operations`.
Hiện `signals` là chiều duy nhất mà gate điền được "một cách trung thực" cho
candidate tự động (xem `completion.rs` và HANDOFF cũ) — sau bước này
`friction_draft` phải điền chiều thật.

**Rủi ro:** friction candidate là đường tự động, không có người chọn chiều.
Phương án: dùng `paths` từ `changed_paths` của handoff receipt. Nếu friction
đến từ `note` (không có receipt) thì không có path — khi đó giữ `signals` hợp
lệ cho riêng ca này, và ghi rõ vì sao. **Chốt khi implement.**

### Ra khỏi giai đoạn 2 khi

Model chỉ còn chiều có người đọc, `friction_draft` điền chiều thật, gate xanh.

---

## Giai đoạn 3 — Recall nhiều chiều

**Mở khoá:** giá trị thật của các chiều vừa giữ lại. **Kích thước:** trung
bình. **Phụ thuộc:** giai đoạn 2.

Mở rộng `applicable_knowledge_buckets` (`src/kernel/packet.rs:543`) từ hai
chiều lên các chiều ADR §6 giữ: `symbols` theo anchor, `domains`, `surfaces`,
`operations`, `risks`, `signals`.

Giữ nguyên hai luật: `required` **không nới** (ADR §8), và `knowledge
applicable` dùng chung hàm với packet by construction — đừng tách thành hai
đường tính.

`why_applicable` phải nói đúng chiều nào khớp. Nó là thứ duy nhất giải thích
được vì sao một learning xuất hiện trong packet.

### Ra khỏi giai đoạn 3 khi

Một learning khớp bằng `operations` hoặc `risks` (không khớp path) vẫn vào
`recommended`, và `why_applicable` nêu đúng chiều.

---

## Giai đoạn 4 — Search và get

**Kích thước:** lớn nhất. **Phụ thuộc:** 2 và 3.

### 4.1 Khảo sát trước khi viết

`src/docs/{lexical,cache,index,search}.rs` là ~2100 dòng. Việc đầu tiên **không
phải viết code** mà là xác định ranh giới tái dùng:

- Cái gì generic thật (tokenize, BM25 field boost, atomic generation swap,
  fingerprint) → tách ra dùng chung.
- Cái gì thuộc docs (section extraction theo ATX heading, `section_ref`,
  line range) → **không** dùng cho knowledge; unit của knowledge là một
  learning record, không phải một section markdown.

Nguy cơ lớn nhất của giai đoạn này là copy 2100 dòng rồi hai bản trôi khỏi
nhau. Tách abstraction trước, hoặc chấp nhận trùng lặp **có chủ ý và ghi lý do**
— không để nó xảy ra một cách tình cờ.

### 4.2 Index và cache

`.pulse/cache/knowledge-search/` theo ADR §5: gitignored, fingerprint theo
content hash entry, atomic replace, incremental, rebuild deterministic.

Field boost theo ADR §3.

Lọc eligibility ở tầng index hay tầng query: **chốt khi implement**. Index-time
rẻ hơn nhưng phải rebuild khi status đổi; query-time đắt hơn nhưng đúng ngay.
Trạng thái đổi thường xuyên (mỗi `validate`, `promote`, `retire`), nghiêng về
query-time.

### 4.3 `search` và `get`

`search` trả summary + reason + score, **không** full content. `get` trả full,
`--summary` trả bản rút gọn. Default status filter theo ADR §3.

### 4.4 Mở `disputed`

Sau khi có search (ADR §2). Contradiction phát hiện được thì mới có người đặt
trạng thái này.

### Ra khỏi giai đoạn 4 khi

Mọi mục §Kiểm chứng của ADR xanh, gồm cả ca xoá cache rồi search lại cho cùng
kết quả.

---

## Giai đoạn 5 — Dọn tài liệu

Làm **sau cùng**, khi code đã đúng, để không phải sửa hai lần.

- `PRODUCT.md` §5.1 dòng 240: bỏ `work claim`.
- `PRODUCT.md` §5.8: bỏ `claim`; danh sách CLI khớp binary thật (hiện thiếu
  ~9 lệnh đang có: `work sync`, `work executability`, `work frontier`,
  `docs tags`, `docs status`, `knowledge check|export|status`,
  `graph bootstrap`, `evidence bootstrap`).
- `PRODUCT.md` §5.8 danh sách MCP tool: bỏ `claim`.
- `PRODUCT.md` §5.6: recall nhiều chiều, vòng đời đủ nhánh.
- ADR 0018 Status: `Accepted` + ngày implement từng giai đoạn.

## Thứ tự tổng, và điểm dừng an toàn

```text
G1 vòng đời ──► G2 thu Applicability ──► G3 recall ──► G4 search ──► G5 docs
   │                                                      │
   └── mở khoá 0009 §ratchet                             └── mở khoá disputed
```

**Mỗi giai đoạn là một điểm dừng an toàn.** Dừng sau G1 cho một sản phẩm nhất
quán (vòng đời đủ, recall vẫn hai chiều như hôm nay). Dừng sau G3 cũng vậy.
Không giai đoạn nào để lại nửa vời nếu dừng đúng ranh giới.

Riêng G2 **không được** dừng giữa chừng: gỡ trường mà chưa siết validate, hoặc
ngược lại, để model và gate lệch nhau.

## Quan hệ với 0009 phần B

G1 là điều kiện cần của skill `pulse-ratchet`. Hai việc có thể chạy song song
sau G1: `pulse-ratchet` không đợi search, vì bước dedupe của nó là Later theo
chính 0009.

Ba skill `grill → spec → tickets` **không** phụ thuộc gì ở đây.

## Chưa chốt, để lại cho lúc implement

1. `retire --reason` lưu ở model hay chỉ ở event log (§1.1).
2. Friction candidate điền chiều applicability nào khi không có receipt (§2.2).
3. Eligibility filter ở index-time hay query-time (§4.2).
4. Mức tái dùng giữa docs search và knowledge search (§4.1).

Cả bốn đều là quyết định implementation, không đổi contract, nên không cần ADR
mới — nhưng phải ghi lại lựa chọn và lý do trong commit tương ứng.
