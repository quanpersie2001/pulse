# Story QA Và Behavioral Proof

[Trang vào](../PULSE_REBOOT.md) | [Work graph](02-work-graph.md) | [Runtime harness](04-runtime-harness.md) | [Verification ratchet](07-verification-ratchet.md) | [Documentation system](10-documentation-system.md)

**Đọc khi:** cần biết QA trong Pulse là gì, test case được sinh và sở hữu ở đâu, khi nào test theo Ticket hay Story, executor nào phù hợp cho web/API/CLI/data và Story đóng bằng bằng chứng nào.
**Sở hữu:** behavioral QA model, Story QA case baseline, QA execution scopes, surface/executor selection, typed receipts, validity, failure classification và Story close gate.

## Mục tiêu và ranh giới

QA trong Pulse là một **behavioral assurance system**: nó chứng minh capability mà user hoặc consuming system quan tâm còn hoạt động đúng trên một source snapshot xác định.

QA không phải:

- tên khác của unit/integration/E2E test;
- một bước manual chung chung sau khi code xong;
- đồng nghĩa với Playwright hoặc browser agent;
- quyền để QA Agent tự sửa acceptance cho khớp implementation;
- một gate chỉ có câu báo cáo “đã test, trông ổn”.

QA gồm ba lớp tách biệt:

1. **Behavioral contract:** Story acceptance, protected risks và expected observations.
2. **Execution:** executor chạy case trên surface/environment phù hợp.
3. **Evidence:** immutable receipt gắn source, environment, observations và artifacts.

## Developer verification khác behavioral QA

Hai hoạt động có thể dùng cùng framework nhưng trả lời hai câu hỏi khác nhau.

### Ticket verification/validation

**Ticket verification** trả lời:

> Implementation của Ticket này có thỏa implementation contract, acceptance và các checks kỹ thuật bắt buộc không?

Nó thường gồm:

- unit tests;
- component tests;
- integration tests;
- API/consumer contract tests;
- automated E2E tests do developer duy trì;
- lint, typecheck, build, schema/static analysis;
- focused regression checks.

`works/TK-031/validation.md` là proof của implementation Ticket trên một source snapshot.

### Behavioral QA

**Behavioral QA** trả lời:

> Trên runnable surface hoặc proof surface phù hợp, user/consumer có quan sát được behavior Story cam kết không, gồm cả cross-Ticket flow, negative path và recovery?

Nó tập trung vào:

- acceptance behavior và user/system outcome;
- cross-module, cross-service hoặc cross-Ticket flow;
- user-visible/system-visible state transitions;
- negative, boundary, recovery và compatibility paths;
- exploratory risk chưa thể encode hoàn toàn thành assertion;
- independent evidence trên candidate snapshot.

`works/ST-014/qa.md` là behavioral baseline sống lâu hơn nhiều Tickets.

### Test level và assurance purpose là hai trục khác nhau

Không được suy ra purpose chỉ từ framework:

| Ví dụ | Test level/tool | Assurance purpose |
|---|---|---|
| Worker chạy Playwright test trong lúc implement | E2E/Playwright | Ticket verification |
| QA Agent replay cùng flow trên frozen candidate, collect trace và tạo receipt | E2E/Playwright | Behavioral QA |
| Browser agent khám phá refresh timeout và ghi finding | Exploratory browser | Behavioral QA |
| Chrome DevTools inspect network để tìm root cause | Browser diagnostics | Debug evidence, chưa tự là QA pass |

Điểm phân biệt là **intent, actor independence khi policy yêu cầu, source binding, acceptance/risk mapping, observation và receipt**, không phải tên tool.

Nếu chỉ có Ticket validation, một chuỗi Tickets đều pass vẫn có thể làm hỏng capability end-to-end. Nếu chỉ có Story QA cuối cùng, defect integration và testability gap bị phát hiện quá muộn. Pulse vì vậy dùng focused QA sớm và full Story qualification ở close.

## Canonical ownership: một baseline cho mỗi Story

Mỗi Story phải khai báo QA posture và có `works/<STORY-ID>/qa.md` khi Story được materialize thành behavioral work. File này là canonical owner của:

- QA scope và capability cần bảo vệ;
- acceptance/risk coverage matrix;
- persistent behavioral cases;
- applicability và environment expectations;
- exploratory charters;
- Story QA exit criteria.

Receipt không ghi trực tiếp vào `qa.md`; receipt là immutable evidence dưới `.pulse/evidence/`. `qa.md` là contract, receipt là observation.

Child Ticket không copy test case sang `works/<TICKET-ID>/qa.md`. Ticket chỉ khai báo QA impact và reference case IDs thuộc Story. Điều này tránh duplicate expected behavior và giữ cross-Ticket flow ở đúng owner.

Ngoại lệ cho standalone Ticket:

- Internal-only Ticket có thể chỉ cần `validation.md`.
- Standalone Ticket tạo public/user-visible behavior có thể sở hữu `works/<TICKET-ID>/qa.md` nếu policy yêu cầu.
- Nếu baseline cần sống qua nhiều follow-up changes hoặc có nhiều behavior branches, nên shape thành Story thay vì giữ một Ticket ngày càng lớn.

## Hai QA execution scopes

Pulse phân biệt **execution scope**, không tạo hai hệ thống case độc lập.

### Ticket QA checkpoint

Ticket QA checkpoint là focused replay sau một Ticket hoặc một integrated slice. Nó chọn subset của Story cases bị thay đổi có khả năng ảnh hưởng.

Checkpoint thường bắt buộc khi Ticket:

- thay đổi user-visible behavior;
- thay đổi public API, CLI output/exit behavior hoặc SDK contract;
- chạm critical/high-risk path;
- hoàn thành một vertical slice runnable;
- sửa escaped defect;
- chạm shared infrastructure có blast radius lớn;
- thêm acceptance hoặc protected risk mới;
- được verification/QA policy yêu cầu independent execution.

Checkpoint có thể chạy:

- affected existing cases;
- cases mới được đề xuất bởi Ticket;
- critical smoke cases bảo vệ đường chính;
- một exploratory charter bounded cho vùng thay đổi.

Ticket checkpoint không thay Story qualification. Receipt của nó có `qa_scope: ticket_checkpoint` và `ticket_id`.

### Story qualification/close QA

Story qualification chạy trên integrated candidate snapshot khi các child outcomes cần thiết đã có thể quan sát cùng nhau.

Nó phải chạy:

- toàn bộ required applicable cases;
- cross-Ticket flows;
- required negative/recovery cases;
- required configuration/platform matrix;
- regression cases được thêm trong quá trình thực hiện Story;
- exploratory charters bắt buộc theo risk policy.

Receipt của nó có `qa_scope: story_close`. Đây là authoritative behavioral gate để đóng Story.

### Execution cadence

```text
Story shaped
  -> materialize QA scope + coverage + critical cases

Ticket A implemented
  -> developer verification
  -> affected-case checkpoint nếu applicable

Ticket B implemented
  -> developer verification
  -> affected-case checkpoint nếu applicable

Integrated Story candidate frozen
  -> full applicable Story qualification
  -> close Story hoặc rework/requeue
```

Không chờ cuối Story mới chạy mọi QA, nhưng cũng không replay toàn bộ baseline sau mọi Ticket.

## QA posture và progressive materialization

Không phải Story nào cũng cần cùng độ sâu, nhưng mọi Story phải disposition QA rõ ràng:

| Posture | Khi dùng | Yêu cầu tối thiểu |
|---|---|---|
| `automated` | Có deterministic runnable surface | Required cases, executor capabilities, machine observations và artifacts |
| `hybrid` | Một phần deterministic, một phần semantic/manual | Automated assertions + structured exploratory/manual cases |
| `manual_structured` | Platform/tooling chưa tự động hóa đủ | Bounded actions, expected observations, actor, screenshots/logs và attestation receipt |
| `static_proof` | Chưa có runnable surface hợp lệ | Static/source proof, limitation, owner và trigger nâng cấp |
| `not_applicable` | Story thực sự không có behavioral QA surface | Rationale + authority; không dùng để né validation |

`not_applicable` cho cả Story là trường hợp hiếm vì Story được định nghĩa là behavioral slice. Một change thuần internal thường nên là standalone Ticket hoặc child Ticket chứ không phải Story riêng.

Risk-adaptive depth:

- R0/R1: concise coverage, happy path + material negative path, focused checkpoint khi behavior đổi.
- R2: coverage matrix đầy đủ, cross-module cases, independent review/checkpoint và Story close qualification.
- R3: platform/config matrix, rollback/recovery/security cases, independent QA, explicit waiver authority và sâu hơn về evidence.

## Cấu trúc normative của `qa.md`

Ví dụ:

```markdown
# ST-014 Behavioral QA

## QA scope
Capability: User tiếp tục checkout khi access token hết hạn mà không mất cart.

Protected risks:
- Refresh loop vô hạn.
- Cart bị mất sau recovery.
- Revoked token vẫn được chấp nhận.
- User mắc kẹt ở loading state.

Posture: automated

## Coverage matrix
| Acceptance/Risk | Cases | Coverage |
|---|---|---|
| AC-01 Refresh thành công và checkout tiếp tục | QA-001 | behavioral |
| AC-02 Revoked token đưa user về login | QA-002 | behavioral |
| RISK-01 Không refresh loop | QA-002, QA-004 | behavioral |
| AC-03 Error envelope giữ compatibility | TK-031 contract receipt | ticket-proof |

## Required cases

### QA-001 Checkout tiếp tục sau access-token expiry
- Type: acceptance, state-transition
- Priority: critical
- Requirement refs: AC-01
- Risk refs: RISK-02
- Preconditions:
  - User đã đăng nhập.
  - Cart có một sản phẩm.
  - Access token hết hạn.
  - Refresh token còn hợp lệ.
- Actions:
  1. Mở checkout.
  2. Chờ authentication recovery hoàn tất.
- Expected observations:
  - Refresh request thành công đúng một lần.
  - User vẫn ở checkout.
  - Cart vẫn còn nguyên.
  - Không có uncaught console error.
- Surface: web
- Executor preference:
  - Primary: playwright
  - Fallback: browser-agent
- Required capabilities:
  - browser
  - network-observation
  - fixture-reset
- Required evidence:
  - action-log
  - final-url
  - network-summary
  - screenshot
  - trace
- Applicability:
  - Environments: local-web, preview
  - Browsers: chromium
- Cleanup:
  - Reset auth fixture và cart.
- Failure owner: auth-domain

## Conditional cases

### QA-010 Safari token recovery
- Applicable when: release profile includes WebKit support.
...

## Exploratory charters

### EX-001 Authentication recovery interruption
Explore:
- refresh request chậm;
- user điều hướng trong lúc refresh;
- refresh trả 5xx;
- nhiều tabs cùng refresh.

Required evidence:
- action log;
- relevant screenshots;
- console/network summary;
- findings với reproduction steps.

## Story exit criteria
- Tất cả critical required applicable cases pass.
- Không có product failure, flaky hoặc inconclusive bắt buộc.
- Blocking exploratory findings đã resolve hoặc accepted bằng Decision/risk.
- Receipts bind vào Story candidate source snapshot.
```

### QA case schema

Mỗi case cần:

- Stable case ID trong owner scope, ví dụ `QA-001`; exploratory charter dùng `EX-001`.
- Intent và type.
- Priority/criticality.
- Requirement refs và risk refs.
- Preconditions, fixture/test data và permissions.
- Bounded actions.
- Expected observations có thể đánh giá.
- Surface.
- Executor preference và required capabilities.
- Required evidence types.
- Applicability theo environment/platform/config/feature flag.
- Cleanup/isolation contract.
- Owner hoặc escalation path khi không chạy được.

Case không được khóa vào implementation detail không cần thiết. Behavioral case nên nói “user về login, không lặp refresh” thay vì embed DOM selector. Selector, invocation hoặc adapter detail thuộc harness/executor config.

## Sinh và review test case

`pulse-qa` không được invent cases từ prompt trống. QA planner phải ground trên:

1. Story outcome và acceptance criteria.
2. Product/domain/API/CLI contracts applicable.
3. Accepted Decisions và Story approach.
4. Invariants, public compatibility và security boundaries.
5. Known risks, failure modes và rollback/recovery expectations.
6. Child Ticket scope và QA impact declarations.
7. Historical defects, escaped regressions và prior QA findings.
8. Supported environment/platform/configuration matrix.
9. Accessibility, performance, privacy hoặc reliability requirements khi applicable.

### Generation procedure

#### 1. Lập coverage matrix

Mỗi acceptance/risk phải map tới một trong:

- behavioral QA case;
- Ticket-level deterministic proof đủ mạnh;
- explicit non-applicability/limitation có rationale và authority.

Không được claim coverage bằng prose không reference evidence/case.

#### 2. Derive material risk paths

Với từng capability, QA planner xem xét tối thiểu:

- invalid, empty, boundary hoặc malformed input;
- dependency timeout/unavailable/partial failure;
- interruption và recovery;
- retry, duplicate và idempotency;
- authorization/permission boundary;
- old client/config/platform compatibility;
- state persistence và cleanup;
- secret/internal detail leakage;
- concurrency hoặc multiple actors khi relevant.

#### 3. Classify cases

Case taxonomy tối thiểu:

- `acceptance`
- `happy-path`
- `negative`
- `boundary`
- `state-transition`
- `recovery`
- `idempotency`
- `permission`
- `compatibility`
- `regression`
- `accessibility`
- `visual`
- `performance`
- `reliability`
- `exploratory`

Không bắt mọi Story có mọi type. Risk/profile quyết định required set.

#### 4. Chọn automation boundary

Ưu tiên deterministic assertion cho critical expectations. Dùng semantic/exploratory observation cho hành vi khó encode hoặc để khám phá unknown risks. Manual observation phải structured, không được giảm thành “looks good”.

#### 5. Review case quality

Reviewer kiểm tra:

- case có bảo vệ một intent/risk rõ không;
- fixture có reproducible và isolated không;
- expected observation có đủ cụ thể để pass/fail không;
- evidence yêu cầu có đủ chứng minh observation không;
- applicability có tránh chạy vô nghĩa không;
- case có duplicate behavior của case khác không;
- case có vô tình khóa implementation detail không;
- suite có bỏ sót cross-Ticket hoặc recovery path quan trọng không.

### Change control

Worker hoặc QA Agent có thể đề xuất:

- case mới;
- case applicability mới;
- executor/evidence improvement;
- protected risk mới phát hiện.

Họ không được tự đổi expected behavior nếu thay đổi đó làm đổi acceptance, product contract, invariant hoặc accepted Decision. Thay đổi semantic như vậy cần owning Story/Decision authority và reconciliation với durable docs.

Case edit phải tăng baseline revision/content hash. Receipt cũ vẫn immutable nhưng có thể không còn đủ cho close nếu baseline revision mới thay đổi required behavior.

## Surface và executor selection

QA là contract; executor phụ thuộc surface và required capabilities.

| Surface | Primary executors | Typical observations |
|---|---|---|
| Web UI | Playwright, browser agent, visual/a11y runner | URL, visible state, network, console, storage, screenshot/trace |
| API/service | Structured HTTP runner, contract validator, service test, shell/curl | status/schema, headers/body, side effects, logs/traces, idempotency |
| CLI/TUI | Shell process runner, PTY executor | exit code, stdout/stderr, prompt sequence, signals, filesystem diff |
| Library/SDK | Consumer fixture, compile/typecheck runner, API compatibility checker | imports/types, example output, runtime behavior, supported versions |
| Desktop/mobile | Platform automation, emulator/device runner, structured manual attestation | screen/state, platform logs, gestures, screenshots/video |
| Data/migration | Query assertions, before/after snapshot, reconciliation, rollback rehearsal | schema/data diff, row counts/checksums, rollback result |
| Không có runnable surface | Static proof + structured manual gate | source/docs proof, limitation, trigger nâng cấp |

Executor selection flow:

```text
case surface + required capabilities + applicability
  -> available environment profiles
  -> compatible executor candidates
  -> deterministic executor preferred for required assertions
  -> semantic/manual fallback only when policy allows
  -> otherwise inconclusive/tool-gap, không giả pass
```

### Web UI

- **Playwright:** deterministic replay cho acceptance/regression.
- **Playwright MCP:** agent-driven control của Playwright; output vẫn phải map về same observation/artifact schema.
- **Browser agent:** exploratory interaction và semantic observation.
- **Chrome DevTools MCP:** console/network/storage/performance diagnostics; thường là supporting observer, không tự động là authoritative pass executor.
- **Visual comparison:** layout/appearance regression khi visual fidelity là requirement.
- **Accessibility runner:** automated semantic/a11y checks; manual keyboard/screen-reader charter khi policy yêu cầu.

Browser agent không được tự diễn giải “trông ổn” thành pass. Semantic observation phải map tới expected behavior; deterministic assertion nên dùng ở nơi có thể.

### API/service

Structured HTTP/contract runner nên capture request/response đã redact, schema validation, correlation ID và side effects. `curl` hữu ích cho smoke/manual reproduction nhưng persistent baseline nên dùng deterministic adapter khi có thể.

Material cases thường cover:

- status, headers, body/error schema;
- authn/authz;
- pagination/filtering;
- idempotency/retry;
- concurrency/conflict;
- database/event side effects;
- old-client compatibility;
- rate limiting và timeout khi contractual.

### CLI/TUI

Non-interactive command dùng shell process runner; interactive prompt/TUI/signal behavior cần PTY capability.

Material observations:

- exit code;
- stdout/stderr separation và stable output format;
- TTY/non-TTY behavior;
- prompt order và confirmation;
- Ctrl-C/termination handling;
- timeout;
- filesystem before/after;
- idempotency;
- platform-specific behavior.

### Data/migration

High-risk migration case cần before/after proof, sample reconciliation và rollback rehearsal khi policy yêu cầu. “Command exit 0” không đủ chứng minh data correctness.

### Structured manual execution

Khi automation chưa khả thi, manual receipt vẫn phải có:

- actor identity;
- source/environment identity;
- case revision;
- action log;
- expected/actual observations;
- required screenshots/logs;
- result classification;
- limitation và follow-up automation trigger nếu relevant.

## Repository QA capability contract

Surface, environment và executor nằm trong target repository, không hard-code trong Pulse. Ví dụ minh họa trong `.pulse/config.yaml`:

```yaml
qa:
  environments:
    local-web:
      start: ["pnpm", "dev:test"]
      healthcheck: "http://127.0.0.1:4173/health"
      reset: ["pnpm", "fixtures:reset"]
      stop: ["pnpm", "dev:test:stop"]

    local-api:
      start: ["docker", "compose", "up", "-d"]
      healthcheck: "http://127.0.0.1:3000/health"
      reset: ["pnpm", "db:test:reset"]
      stop: ["docker", "compose", "down"]

  surfaces:
    web:
      executors: [playwright, browser-agent, chrome-devtools]
      default_environment: local-web
    api:
      executors: [http-contract, shell]
      default_environment: local-api
    cli:
      executors: [shell, pty]

  policies:
    critical_case_requires_deterministic_assertion: true
    high_risk_requires_independent_qa: true
    required_case_inconclusive_blocks_close: true
    failed_attempts_are_preserved: true
```

Executor manifest phải khai báo:

- supported surfaces/capabilities;
- permission và side effects;
- environment requirements;
- artifact types có thể tạo;
- timeout/cancellation behavior;
- redaction contract;
- failure taxonomy.

`pulse doctor` phải phát hiện ít nhất:

- case required nhưng không có compatible executor;
- executor declared nhưng unavailable;
- environment thiếu start/healthcheck/reset/cleanup cần thiết;
- case yêu cầu PTY/network/storage/visual capability mà adapter không có;
- required evidence type không collect được;
- fixture không deterministic hoặc không isolation;
- retry/flaky/waiver policy không rõ;
- baseline có acceptance/risk chưa cover.

## QA execution flow

```text
select source snapshot
  -> load Story baseline revision
  -> select ticket_checkpoint hoặc story_close scope
  -> resolve applicable cases
  -> select environment profile
  -> start + healthcheck environment
  -> reset/seed fixtures
  -> resolve executor by surface/capabilities
  -> execute bounded actions
  -> collect deterministic + semantic observations
  -> collect/redact/hash artifacts
  -> create immutable receipt
  -> validate receipt schema/source/baseline/environment/artifacts
  -> classify result/finding
  -> attach evidence to checkpoint hoặc Story close gate
  -> cleanup environment/fixtures
```

Story close nên dùng frozen source snapshot hoặc immutable deployed artifact. Nếu environment chạy artifact khác source được claim, receipt invalid.

## Typed QA receipt

Ví dụ JSON minh họa:

```json
{
  "receipt_version": 1,
  "qa_scope": "ticket_checkpoint",
  "story_id": "ST-014",
  "ticket_id": "TK-031",
  "case_id": "QA-001",
  "case_revision": 3,
  "baseline_content_hash": "sha256:...",
  "run_id": "run_01J...",
  "attempt": 1,
  "actor": {"kind": "agent", "id": "qa_codex_07"},
  "executor": {
    "name": "playwright",
    "version": "1.55.0",
    "capabilities": ["browser", "network-observation", "trace"]
  },
  "source": {
    "commit": "7d31c2a",
    "dirty_diff_hash": null,
    "workspace_id": "wt_TK-031",
    "artifact_id": "web-build-sha256:..."
  },
  "environment": {
    "profile": "local-web",
    "base_url": "http://127.0.0.1:4173",
    "platform": "linux",
    "browser": "chromium-140",
    "fixture_revision": "auth-fixture-v4",
    "feature_flags": {"new_checkout": true}
  },
  "result": "passed",
  "started_at": "2026-07-18T01:20:00Z",
  "finished_at": "2026-07-18T01:20:42Z",
  "observations": [
    {
      "kind": "url",
      "actual": "/checkout",
      "expected": "/checkout",
      "result": "passed"
    },
    {
      "kind": "network",
      "actual": {"refresh_requests": 1, "status": 200},
      "expected": {"refresh_requests": 1, "status": 200},
      "result": "passed"
    }
  ],
  "artifacts": [
    {"kind": "trace", "path": "trace.zip", "sha256": "..."},
    {"kind": "screenshot", "path": "final.png", "sha256": "..."}
  ],
  "cleanup": {"result": "passed"}
}
```

Receipt là immutable. Nếu cần rerun hoặc sửa kết quả, tạo receipt mới và liên kết attempt/supersession; không mutate receipt cũ.

## Receipt validity

Receipt chỉ hợp lệ khi:

- Schema version được hỗ trợ.
- QA scope có owner Story/Ticket hợp lệ.
- Case ID/revision và baseline content hash khớp contract cần chứng minh.
- `source` khớp snapshot/artifact cần close hoặc ancestor policy cho phép rõ ràng.
- Environment profile và applicability thỏa case/policy.
- Executor có đủ declared capabilities.
- Fixture revision/reset result hợp lệ.
- Required observations không bị skip.
- Required artifacts tồn tại, hash khớp và qua redaction policy.
- Receipt không quá TTL với case nhạy environment.
- Cleanup requirement đã pass hoặc failure được classify.
- Actor independence thỏa risk policy.

`passed` nhưng thiếu trace/network assertion bắt buộc vẫn là invalid receipt.

Ticket checkpoint receipt không tự động đủ cho Story close nếu:

- source snapshot đã đổi ngoài ancestor policy;
- case/baseline revision đã đổi;
- Story close policy yêu cầu full integrated replay;
- environment/platform khác required qualification profile.

## Result, retry và flakiness

Kết quả execution tối thiểu:

- `passed`
- `product_failure`
- `test_failure`
- `environment_failure`
- `inconclusive`
- `not_applicable`
- `flaky`
- `waived`

Ý nghĩa:

- `product_failure`: observed behavior lệch contract.
- `test_failure`: selector, script, assertion, fixture logic hoặc executor hỏng.
- `environment_failure`: app/dependency/credential/network/setup không sẵn sàng.
- `inconclusive`: execution xong nhưng evidence không đủ kết luận.
- `not_applicable`: case không áp dụng theo rule đã declared.
- `flaky`: cùng source/environment class cho kết quả không ổn định.
- `waived`: required case được authority cho phép bỏ qua có expiry/risk record.

Không được retry rồi che mất failed attempt:

```text
attempt 1 fail
attempt 2 pass
=> result không tự động là passed; classify flaky cho tới khi policy/root cause xử lý
```

Mọi attempt giữ receipt riêng. Required critical case ở trạng thái `flaky`, `inconclusive` hoặc unapproved `waived` chặn Story close theo mặc định.

Chỉ `product_failure` tự động requeue product implementation. Các loại khác tạo test/harness/environment work tương ứng, nhưng vẫn có thể block close vì chưa có proof đáng tin.

## Findings và triage

Mỗi failure/finding cần:

- source/environment/case identity;
- expected vs actual observation;
- reproduction/action log;
- artifacts;
- severity và affected acceptance/risk;
- primary classification + contributing classes;
- suggested owner/work item;
- suggested verification/QA replay.

Exploratory finding không tự động thành product failure nếu chưa reproduce hoặc expected contract mơ hồ. Nó có thể route thành:

- product defect Ticket;
- test/harness Ticket;
- environment Ticket;
- Decision/policy clarification;
- new regression case;
- accepted risk/known limitation với authority.

## QA planning và impact từ Ticket

Mỗi behavior-affecting Ticket khai báo `QA impact` trong `ticket.md`:

```markdown
## QA impact
- Behavioral owner: ST-014
- Posture: required
- Affected cases: QA-001, QA-004
- New proposed cases: QA-009
- Checkpoint: required
- Reason: thay đổi refresh-token recovery và public error mapping.
```

Allowed posture:

- `required`: targeted checkpoint phải pass trước Ticket close.
- `covered_by_story_close`: checkpoint không bắt buộc; rationale chứng minh current Ticket chưa có runnable/material behavior và Story close sẽ cover.
- `none`: không ảnh hưởng behavior, có rationale.
- `unknown`: không được `ready` với implementation Ticket có khả năng đổi public/user-visible behavior.

QA impact không thay verification profile. Ticket vẫn phải chạy developer verification dù checkpoint là `none`.

Khi Ticket thêm acceptance hoặc risk mới, Story baseline phải được cập nhật/review trước khi behavior tương ứng được coi là fully qualified. Worker có thể đề xuất case; owner/reviewer có authority accept semantic baseline change.

## Story close gate

Story chỉ đóng khi:

1. QA baseline revision hiện tại có scope, protected risks và exit criteria rõ.
2. Mọi acceptance/risk được map tới valid behavioral case, Ticket proof hoặc authorized limitation.
3. Các child Ticket outcomes cần thiết đã integrate vào candidate source snapshot.
4. Mọi required applicable Story case có receipt hợp lệ trên snapshot/artifact đủ mới.
5. Required environment/platform/config matrix đã cover.
6. Không còn required `product_failure`, `inconclusive`, `flaky` hoặc unapproved waiver.
7. Blocking exploratory findings đã resolve, requeue hoặc accepted bằng authority rõ.
8. Known limitation được ghi thành Decision/risk, không bị giấu.
9. Required product/domain/operations docs đã update hoặc defer hợp policy với valid documentation receipt.
10. Không còn blocking contradiction giữa QA baseline, accepted Decision và durable docs.
11. Conductor/Orchestration Agent hoặc human thực hiện close gate.

Worker Agent không được tự đóng Story vì chính nó vừa implement child Ticket. QA Agent không được đổi acceptance hoặc approve waiver ngoài authority được cấp.

## QA Agent

QA có thể là một Agent độc lập với:

- task/thread riêng;
- read-only frozen source snapshot hoặc dedicated QA workspace;
- capability `playwright`, `browser`, `chrome-devtools`, `api`, `cli`, `pty`, `data` hoặc `manual-review`;
- assignment lease trên QA execution, không phải implementation ownership;
- quyền tạo receipt, finding, regression-case proposal, docs finding và promotion candidate;
- không có quyền sửa acceptance, accepted Decision hoặc approved durable contract ngầm.

Independence là risk-adaptive:

- Low-risk checkpoint có thể do same Worker chạy nếu policy cho phép và receipt vẫn source-bound.
- Story close cho R2/R3 hoặc critical behavior nên dùng independent QA actor/task.
- Security, destructive migration hoặc production qualification có thể cần human/specialist gate ngoài agent QA.

Điều phối QA Agent tuân theo [`05-cross-agent-coordination.md`](05-cross-agent-coordination.md).
