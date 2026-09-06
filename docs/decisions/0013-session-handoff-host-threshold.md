# Decision 0013: Bàn giao phiên theo ngưỡng của host

## Status

Accepted, 2026-09-06. Bổ sung PRODUCT.md §4 (layout `runtime/handoff/`),
§5.3 (worker `context_exhausted`), §5.7 (note nhắm mọi node, bàn giao
phiên), §5.8 (route và skill `pulse-handoff`), §6, §11. Thay dòng "đầy thì
`pulse note` để bàn giao, không compact giữa chừng" trong Decision 0009 và
thêm skill thứ tám vào bảng skill của 0009.

## Context

Hai chữ "handoff" đang va nhau và phải tách trước:

| | `pulse work handoff` | bàn giao phiên |
|---|---|---|
| Đơn vị | một Ticket | một cuộc trò chuyện với agent |
| Bản chất | receipt bất biến, gate đọc | tài liệu cho phiên mới đọc |
| Khi nào | worker làm xong | context sắp đầy, hết ngày, đổi agent |
| Bắt buộc | luôn | theo ngưỡng developer đặt |

Decision này về cái thứ hai. Yêu cầu của developer: khi context đạt một
ngưỡng (ví dụ 70% hoặc một số token cố định), **bắt buộc** bàn giao sang
phiên mới thay vì để host compact. Lý do: compaction là nén mất mát do host
quyết, developer không kiểm soát được cái gì bị bỏ; bàn giao có chủ ý ép phần
durable vào artifact có người đọc lại.

Ba bên liên quan, chỉ một bên đếm được token:

```text
 Model      không biết chính xác mình đã dùng bao nhiêu; khi đầy thì thường quên
 Host       biết chính xác, có hook trước compaction và cuối mỗi lượt
 Pulse      không nhìn thấy phiên (Decision 0009 mục 6: không đọc transcript)
```

Nên "bắt buộc" phải là hook của host, và hook đó ép model chạy thủ tục của
Pulse. Nhờ model tự phán "đã đầy" thì không ép được. Đặt vào Pulse thì Pulse
phải đếm token, tức là quản lý phiên, trái §6.

Nguồn tham khảo: `references/mattpocock/skills/skills/productivity/handoff`.
Skill `/handoff` của Matt có năm luật: nén không chép (thứ đã nằm trong spec,
plan, ADR, commit, diff thì chỉ trỏ path); chỉ giữ live thread; có mục
suggested skills; redact secret và PII; ghi ra thư mục tạm của OS, không vào
workspace, chỉ user gọi (`disable-model-invocation: true`). Biến thể
`claude-handoff` không ghi file mà spawn ngay phiên nền với bản nén làm
prompt.

Pulse đã có bàn giao phiên cho một trường hợp mà không cần gọi tên: Ticket
đang chạy dưới `pulse run`. Agent chết hay hết context, `pulse run` lại cùng
Ticket nhận cùng lease, cùng packet, note hiện trong packet. Đó chính là
`claude-handoff` do Pulse điều khiển. Lỗ hổng nằm ở ba chỗ:

- Trước khi có Ticket (wayfind, grill, spec) neo là Epic hay Story; cờ
  `--ticket` của `pulse note` nói dối vì code đã nhận mọi node id.
- `MAX_NOTE_CHARS` là 2000 và packet cắt mỗi note còn 500 ký tự; một bàn giao
  thật không vừa.
- Không có thủ tục nào nói "flush trước khi bàn giao", nên bàn giao có xu
  hướng chép lại những gì lẽ ra phải nằm trong `works/` và `docs/`.

Luật "ghi vào `/tmp`" của Matt đúng cho repo không có control plane. Với Pulse
nó ngược trục `agent memory → repository-legible context`: một file trong
`/tmp` mất khi đổi máy và không ai audit. Nhưng Decision 0009 cũng đúng khi
cấm `HANDOFF.json`: một file trạng thái trong plane tracked sẽ thành truth
thứ hai. Cách hoà là đảo thứ tự: flush trước, bàn giao sau, và bàn giao chỉ
là con trỏ.

## Decision

1. **Host đếm và ép ngưỡng. Pulse cung cấp thủ tục và chỗ ghi.** Pulse không
   đọc transcript, không đếm token, không gắn hook, không tự spawn phiên mới.
   Đổi host thì đổi hook, Pulse không đổi.
2. **Skill `pulse-handoff`, user-invoked, `disable-model-invocation: true`.**
   Skill thứ tám bên cạnh bảy skill của 0009. Đây là loại skill "explicit và
   hiếm" mà 0009 cho phép; không router, không state riêng, mọi mutation là
   lệnh `pulse` nguyên văn. Ba bước, theo thứ tự:
   1. **Flush** về đúng plane bằng lệnh `pulse`: câu hỏi treo vào
      `## Open questions` với disposition, quyết định vào `brief.md`
      `Decisions so far` hoặc Decision node, thuật ngữ vào glossary, research
      dở vào `works/<id>/research/`, graph qua `work create`, `edge add`,
      `transition`.
   2. **Doc** chỉ giữ live thread: đang làm gì, vì sao, tiếp theo là gì, đọc
      gì, skill nào. Trỏ path, không chép. Redact secret, PII, absolute path.
      Ghi tại `.pulse/runtime/handoff/<node-id>.md`, ghi đè cùng tên nên mỗi
      node một doc, luôn là bản mới nhất.
   3. **Note** một dòng làm con trỏ: `pulse note --work <node-id> --message
      "handoff: .pulse/runtime/handoff/<node-id>.md | tiếp theo: <một câu>"`.
      Rồi in lệnh mở phiên mới và dừng, không làm thêm.
   Repo Pulse tự thân không phải target repo (`--repo-root .` bị cấm): nó
   dùng quy ước dev sẵn có là `HANDOFF.md` ở gốc, commit cùng code, cùng ba
   bước flush, live thread, trỏ path. Đó là quy ước của repo phát triển
   Pulse, không phải plane của target repo, nên không mâu thuẫn với "không
   HANDOFF.json".
3. **Hook mẫu cho host.** `context-guard.sh` cho Claude Code là Stop hook: đọc
   `transcript_path`, đo bytes, vượt ngưỡng thì trả `{"decision":"block",
   "reason":"…"}` với reason là "gọi `pulse-handoff` rồi dừng"; chốt
   `stop_hook_active` chống lặp. Ngưỡng đặt bằng bytes và hiệu chỉnh bằng
   `/context` vì input hook không có số token. `PreCompact` là lưới sau.
   `UserPromptSubmit` chỉ để nhắc. Hook mẫu nằm trong `docs/operations/` của
   repo đích; `pulse init` chỉ chép khi được hỏi, không tự gắn.
4. **Worker headless hết context.** Bootstrap prompt của `pulse run worker`
   thêm: nếu không thể hoàn thành vì context, chạy bước 2 và 3 ở trên rồi
   kết thúc bằng `{"status":"blocked","reason":"context_exhausted"}`. Lease
   giữ nguyên; `pulse run worker` lần sau resume với cùng packet và note
   handoff hiện trong packet. Không có trạng thái mới.
5. **`pulse note --work <id>`.** Đổi tên cờ cho đúng với code, `--ticket` giữ
   làm alias. Note nhắm được Epic, Story, Ticket, Decision. Giới hạn 2000 ký
   tự giữ nguyên: note là con trỏ, không phải doc.
6. **Later, làm khi phiên mới mò note chậm thật:** `--kind handoff` bên cạnh
   `--kind friction` của 0009; `pulse work resume` là query thuần: liệt kê
   node chưa terminal có note handoff mới nhất, in note và lệnh mở theo
   lifecycle (`pulse run worker` cho Ticket `active` còn lease, `work
   release` rồi `run` khi lease hết TTL, `run reviewer` cho `verifying`,
   `work packet` hoặc `work show` cho phần còn lại). Không spawn, không lấy
   lease, không sửa gì; `--run` không làm vì gộp query với mutation. Event
   `handoff.resumed` chỉ thêm khi kịch bản hai phiên chồng nhau xảy ra thật.
7. **Không làm.** Không HANDOFF.json hay file bàn giao trong `works/`; không
   tài liệu dài trong `/tmp` cho target repo; không để model tự phán "đã đầy";
   không bắt mọi phiên kết thúc bằng handoff; không tự phát hiện ngưỡng.

## Thủ tục bàn giao phiên

```text
 context sắp đầy (hook host ép, hoặc user gọi /pulse-handoff)
   │
   ├─ (1) FLUSH: mọi thứ durable về đúng plane của nó qua lệnh pulse
   │      câu hỏi đang treo    ─► ## Open questions (blocking|deferred) trong story.md / ticket.md
   │      quyết định đã chốt   ─► Decisions so far trong brief.md, hoặc Decision node
   │      thuật ngữ            ─► docs/domain/glossary.md
   │      research dở          ─► works/<id>/research/<topic>.md
   │      graph                ─► pulse work create / edge add / transition
   │
   ├─ (2) DOC: chỉ live thread, trỏ path, không chép, redact
   │      .pulse/runtime/handoff/<node-id>.md
   │        Đang | Vì sao | Tiếp theo | Đọc | Skill | Mở
   │
   ├─ (3) NOTE: một dòng con trỏ
   │      pulse note --work <node-id> --message "handoff: <path> | tiếp theo: …"
   │
   └─ (4) in lệnh mở phiên mới, rồi dừng
```

Nếu sau bước 1 mà live thread vẫn dài, đó không phải lỗi giới hạn. Đó là dấu
hiệu artifact chưa đủ: có quyết định chưa ghi, câu hỏi chưa có disposition.
Quay lại bước 1.

Phiên mới:

```text
 claude (hoặc host khác)
   ├─ 1. khối AGENTS: "phiên mới → pulse work list --status active|shaped, events tail"
   │     (sau khi có work resume: một lệnh)
   ├─ 2. đọc doc handoff tại path trong note, và artifact doc trỏ tới
   ├─ 3. chạy lệnh Mở: work show / work packet / pulse run worker
   └─ 4. tiếp tục từ dòng Tiếp theo; skill nếu doc ghi
```

Note handoff không có trạng thái "đã tiêu thụ". Nó hết vai trò khi có note
handoff mới hơn cho cùng node hoặc node terminal. Kịch bản hai phiên chồng
nhau (A resume rồi làm 40 phút không handoff, B mở sau thấy doc cũ) được
chấp nhận vì bước flush của A đã đưa việc vào `works/` và graph; B đọc doc cũ
rồi mở `work show` thấy trạng thái thật. Doc là con trỏ, artifact là truth.

## Hook mẫu

`docs/operations/context-guard.sh` trong repo đích:

```bash
#!/usr/bin/env bash
# Stop hook cho Claude Code: ép bàn giao phiên khi transcript vượt ngưỡng.
input=$(cat)
active=$(jq -r '.stop_hook_active' <<<"$input")
[ "$active" = "true" ] && exit 0            # đang trong vòng ép, không ép lại
path=$(jq -r '.transcript_path' <<<"$input")
bytes=$(wc -c <"$path")
limit=${PULSE_HANDOFF_BYTES:-1200000}       # hiệu chỉnh bằng /context
[ "$bytes" -lt "$limit" ] && exit 0
jq -n --arg b "$bytes" '{decision:"block",
  reason:("Context guard: transcript " + $b + " bytes vượt ngưỡng. "
        + "Gọi skill pulse-handoff ngay: flush về works/ và docs/ qua lệnh pulse, "
        + "ghi .pulse/runtime/handoff/<node>.md, pulse note --work <node>, "
        + "in lệnh mở phiên mới, rồi dừng. Không làm việc khác.")}'
```

`.claude/settings.json`:

```json
{"hooks": {"Stop": [{"hooks": [{"type": "command", "command": "docs/operations/context-guard.sh"}]}]}}
```

Ba điều cần cân: ngưỡng phần trăm tốt hơn số token tuyệt đối vì phụ thuộc
model, nhưng hook chỉ thấy bytes nên đặt bytes và chỉnh vài lần; ép cứng cho
mọi việc kể cả Ticket R0 sắp xong là chấp nhận được vì resume rẻ, muốn nương
thì hook bỏ qua khi `pulse work list --status active --json` rỗng; 2000 ký tự
cho note giữ nguyên vì phần dài đã ở doc và artifact.

## Thay đổi

CLI và kernel:

- `src/cli/args.rs`, `src/kernel/communication.rs`: `pulse note --work <id>`,
  `--ticket` alias; `record_note` đổi tên tham số; `NoteRecorded.ticket_id`
  thành `work_id` với alias serde.
- `src/kernel/run.rs`: bootstrap prompt worker thêm bước `context_exhausted`;
  `classify_worker` chấp nhận `reason` đó như `blocked` thường.
- `src/kernel/init.rs`: `pulse init --with-context-guard` chép hook mẫu vào
  `docs/operations/`; mặc định không chép.

Skill và template:

- `skills/pulse-handoff/SKILL.md` ngắn, `disable-model-invocation: true`,
  `references/handoff-doc.md` là template sáu mục Đang, Vì sao, Tiếp theo,
  Đọc, Skill, Mở.
- Khối AGENTS (template của 0009): route "context sắp đầy → pulse-handoff"
  và "phiên mới → `work list --status active|shaped`, `events tail`, đọc doc
  handoff".
- `.gitignore` của repo đích do `pulse init` ghi đã có `runtime/`; không thêm.

Later: `--kind handoff`; `pulse work resume`; `handoff.resumed`.

Docs: PRODUCT.md như trong Status; Decision 0009 dòng "đầy thì `pulse note`"
và bảng skill; glossary tách `Session handoff` khỏi `Handoff receipt`.

## Consequences

- Bàn giao phiên trở thành thủ tục có ép, không phụ thuộc model tự giác.
  Developer kiểm soát ngưỡng ở host, Pulse không phình thêm trách nhiệm.
- Phần dài của bàn giao bị đẩy vào `works/` và `docs/`, nơi có người đọc lại
  và có gate; doc runtime chỉ là con trỏ, mất cũng không mất truth.
- Tám skill thay vì bảy. Skill mới user-invoked nên không đổi cách route.
- Hook mẫu chỉ cho Claude Code. Host khác cần hook tương đương do developer
  viết; Pulse chỉ hứa thủ tục, không hứa hook.
- `pulse work resume` chưa có nên phiên mới còn phải gõ vài lệnh; chấp nhận
  cho tới khi dogfood cho thấy chậm thật.
