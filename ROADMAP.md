# Pulse — Roadmap

> Now / Next / Later / Undecided. Chỗ này nói **thứ tự**; `PRODUCT.md` nói
> **cái gì và vì sao**, `ARCHITECTURE.md` nói **code hiện có**. Khi mâu thuẫn,
> `PRODUCT.md` thắng.
>
> Cập nhật 2026-09-12 sau đợt rà toàn bộ ADR và `PRODUCT.md` §8/§13.

## Cổng chặn mọi thứ khác

`PRODUCT.md` §7 nói: **chưa đạt golden path thì không thêm feature.** Hôm nay
cổng đó không thoả mãn được, vì lý do cơ học chứ không phải vì thiếu tính năng:

**Pulse không chạy thật ở đâu cả.** `examples/todolist/` bị gỡ 2026-09-07 (còn
ở tag `dogfood/track-b-final`). Lý do gỡ, ghi trong §13.1: Decision 0009 chưa
implement nên mọi lần chạy đều thiếu lớp skill mà thiết kế đòi, và friction thu
được lẫn *"thiếu một tầng"* với *"thiết kế sai"* — hai thứ cần phân biệt được
thì dogfood mới có giá trị.

Nên roadmap này có đúng một hình dạng: **dựng đủ tầng còn thiếu, rồi chạy thật,
rồi mới xét cái tiếp theo.** Mọi mục ở Later đều đứng sau cái cổng đó.

Trạng thái golden path §7:

| Tiêu chí | Thực tế |
|---|---|
| Mục 1–7 | Đạt 2026-09-05 trên Track B, HEAD `845ff01` |
| `close-story` trên baseline thật | Đạt — ST-001 (09-05), ST-002 (09-06) |
| Hai Ticket song song không va nhau | **Chưa.** TK-006/TK-007 đã va; Decision 0015 sửa nguyên nhân, có `tests/runner/worktree_dispatch.rs`, **chưa chạy lại thật lần nào** |

---

## Now — bề mặt hướng dẫn, rồi chạy thật

Decision 0009 phần B. Đây là **nợ chức năng duy nhất** còn lại trong cả 18 ADR;
mọi thứ khác đã landed hoặc là Later có chủ ý.

Thứ tự do `0009 §Thứ tự` chốt, không phải do độ khó:

1. **Guard parse lệnh** — thay guard cấm `skills/`. *(đã landed `484c935`)*
2. **`DOC-GLOSSARY`** — `pulse init` tạo và đăng ký `docs/domain/glossary.md`,
   để `pulse-grill` có chỗ ghi thuật ngữ thay vì tự chế đường dẫn.
3. **`pulse-grill` → `pulse-spec` → `pulse-tickets`** — chuỗi dẫn từ intent tới
   Ticket `ready`. Đây là giai đoạn Track B **chưa bao giờ** giao cho agent:
   TK-003..TK-008 đều đã shaped bằng tay, nên nhật ký friction **im lặng** về
   nửa này, không phải **phản đối** nó.
4. **Dựng dogfood target mới, chạy thật bằng agent tương tác** — một Story hai
   Ticket, không gõ lệnh tay.
5. **`pulse-ratchet`** — sau khi có friction thật từ bước 4. Cần 0018 G1
   trước (xem Next): bước 5 của nó ra lệnh retire learning, mà hôm nay không
   retire được.
6. **`pulse-wayfind`, `pulse-research`** — khi có một Epic thật.
7. **`pulse-handoff`** (Decision 0013) — cộng `.pulse/runtime/handoff/<node>.md`
   và hook mẫu cho host.
8. **`pulse-onboard`** — sau cùng, thử trên một repo thật ngoài dogfood target.

### Hai thứ chỉ bước 4 kiểm chứng được

- Hai Ticket song song không va nhau — tiêu chí phụ §7 duy nhất còn thiếu.
- Khối `AGENTS.md` có thật sự dẫn được agent từ intent tới `ready` không.

### Bug phải xử trước bước 4

`.gitignore` neo ở gốc repo: pattern `.pulse/runtime/` **không** khớp thư mục
con. Target cũ đã vô tình track runtime state vì lỗi này (§13.1).

---

## Next — knowledge plane

[Decision 0018](docs/decisions/0018-knowledge-plane-retrieval-and-lifecycle-exits.md),
kế hoạch chi tiết ở [`docs/plans/0018-knowledge-plane.md`](docs/plans/0018-knowledge-plane.md).

**G1 — đường ra của vòng đời** (`knowledge supersede`, `retire`) là mục duy
nhất ở Next có thể **chen lên Now**, vì hai lý do:

- Nó mở khoá `pulse-ratchet` (bước 5 của Now).
- Nó cầm máu một lỗi tự khuếch đại: learning sai được inject vào packet, worker
  làm theo, sinh ma sát, ratchet capture thêm learning — không ai gỡ cái gốc.
  Vòng đời cụt nửa dưới là nửa ngược của nguyên tắc 12.

G2–G5 (thu gọn `Applicability`, recall nhiều chiều, `knowledge search`/`get`,
dọn docs) đợi sau khi dogfood chạy — chính dogfood mới nói được recall hai
chiều hiện tại có đủ không.

### Các lệnh CLI còn thiếu khác

Hoãn có chủ ý, ghi lại để không bị hỏi lại:

- `pulse work claim` — **gỡ khỏi `PRODUCT.md`, không implement** (0018 §7).
- `knowledge get`/`search` — 0018 G4.
- `PRODUCT.md` §5.8 thiếu ~9 lệnh đang có (`work sync`, `work executability`,
  `work frontier`, `docs tags`, `docs status`, `knowledge check|export|status`,
  `graph bootstrap`, `evidence bootstrap`) — dọn ở 0018 G5.

---

## Later

Theo `PRODUCT.md` §11, giữ nguyên thứ tự ưu tiên ở đó. Mọi mục đứng sau golden
path.

| | Điều kiện để khởi động |
|---|---|
| MCP server (~10 tool) | Sau khi CLI path chạy thật |
| `pulse doctor` | Cần thang bằng chứng (xem Undecided) |
| `pulse commands --json` | Để khối AGENTS render từ inventory, guard kiểm hai chiều |
| `pulse ratchet bundle` | **Chỉ khi** dogfood thấy ba lane rò rỉ sang nhau |
| Compound run, retrieval eval | Sau 0018 G4 |
| Persisted shaping map, decision frontier | Khi R2/R3 thật sự chạy |
| Story qualification matrix | — |
| External tracker adapter | — |
| Semantic search adapter | **Chỉ khi** lexical eval chứng minh recall gap |
| Conductor agent loop | — |
| Windows tier-1 | Khi có người dùng Windows |
| `note --kind handoff`, `work resume` | Khi dogfood thấy phiên mới mò note chậm thật |

Mẫu chung của cột phải: phần lớn mục Later **không có ngày**, chúng có **điều
kiện**. Khởi động một mục vì nó thú vị thay vì vì điều kiện đã đến là đúng dấu
hiệu §12 "Feature được viết thành docs, schema, version trước khi có người
dùng".

---

## Undecided

Bốn thứ chưa chốt, mặc định giữ nguyên nếu không có ý kiến khác.

1. **Gộp `evidence/execution/*` vào `evidence/receipts/`.** Hai họ receipt vẫn
   tách: `handoff`/`verification`/`close` một bên, còn lại một bên. Decision
   0011 và 0017 đều chạm ranh giới này mà không gộp. Gộp khi `doctor` cần đọc
   chúng chung (§11).

2. **Thang bằng chứng `present|wired|exercised|outcome_supported`.** Decision
   0012 §1 định nghĩa từ vựng; **không có dòng code nào**. Hai người tiêu thụ
   duy nhất đều chưa xây: `pulse doctor` (Later) và lane `harness` của
   `pulse-ratchet` (Now bước 5). Phải chốt khi làm bước đó: lane tự tính bậc
   bằng prose, hay Pulse cấp một lệnh tính deterministic như 0012 §1 mô tả.

3. **Repo downstream phát hiện bug của Pulse thì ghi vào đâu.** Thiết kế hiện
   không có cửa nào. `note --kind friction` là cơ chế **của repo đích** —
   friction → `.pulse/events/` của repo đó → close gate → learning scope
   `harness` → promote vào `AGENTS.md`/`PULSE.md`/`runners.json` **của repo
   đó**. Không mắt xích nào chảy ngược về Pulse.

4. **Bốn quyết định implementation của 0018** (chỗ lưu `--reason`, chiều
   applicability cho friction candidate không có receipt, eligibility lọc ở
   index-time hay query-time, mức tái dùng với `src/docs/*`). Không đổi
   contract nên không cần ADR mới, nhưng phải ghi lựa chọn và lý do trong commit
   tương ứng.

---

## Cách đọc file này khi nó cũ đi

Roadmap sai nguy hiểm hơn roadmap thiếu. Ba chỗ tự khai trạng thái trong repo
này đã từng lệch khỏi code và không ai phát hiện trong nhiều ngày:
`PRODUCT.md` §8, danh sách "còn mở" §13, và danh sách CLI §5.8.

Nên: **đừng tin file này, kiểm bằng repo.** `git log`, `cargo test`, và
`--help` của binary là sự thật; file này là ý định.
