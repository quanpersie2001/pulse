# Phase 3 Slice 2 — Deterministic documentation validation

Status: implemented 2026-08-25.

## Outcome

`pulse docs validate` giờ là read-only repository check thay vì chỉ validate
registry envelope. Command tạo một report có thứ tự ổn định cho bốn lớp:

1. registry structure và registered-content contract;
2. repository-relative Markdown links của current documents;
3. declared `generated.freshness_check` của current generated documents;
4. generated navigation projections (`docs/**/_index.md`).

Invalid report trả non-zero. Registry-invalid dùng
`invalid_docs_registry`; repository check failure dùng
`docs_validation_failed` và finding code cụ thể.

## Ownership và failure posture

- `src/docs/validate.rs` tiếp tục chỉ sở hữu structural validation. Register,
  edit, retire và supersede không execute repository code.
- `src/docs/check.rs` sở hữu repository-content checks được gọi explicit từ
  `pulse docs validate`; CLI chỉ load registry, gọi facade và render report.
- Structural failure fail closed: link, generator và projection checks được
  report là `skipped`, không chạy trên registry chưa hợp lệ.
- Freshness command được parse thành argv và execute trực tiếp với target repo
  làm working directory. Pulse không gọi shell; `&&`, redirect, `$()` và shell
  expansion không có authority đặc biệt.
- Exact declared check được cache theo command trong một validation run để hai
  records dùng cùng generator không chạy lặp. Exit zero là current; non-zero là
  `docs_generated_stale`; parse/spawn failure là
  `docs_generated_freshness_check_failed`.
- Command output trong finding bị bound ở 4 KiB.

Source và generator vẫn là truth; Pulse không đo freshness bằng mtime và không
tự regenerate hay rewrite output. Với `editable: false`, declared check phải
phát hiện output khác deterministic generator; Pulse không giả lập semantic
generator contract.

## Link và projection scope

Link check hiện tại cố ý local-first:

- check inline Markdown links/images trong current `.md`/`.markdown` records;
- resolve relative links từ owning document, normalize `.`/`..`, percent-decode
  target và reject repository escape/symlink escape;
- bỏ qua fenced/inline-code examples, anchors-only và URI có scheme;
- không gọi network để probe external links;
- chưa validate anchor tồn tại, reference-style links, executable snippets hay
  semantic consistency.

Navigation dùng projection renderer/checker hiện có. Missing, stale và existing
user-authored conflict lần lượt tạo
`docs_index_projection_missing|stale|conflict`; validator không overwrite file.

## Machine contract

Report schema version 1 giữ `errors`/`warnings` findings và thêm ordered checks:

```json
{
  "schema_version": 1,
  "code": "ok",
  "valid": true,
  "registry_revision": 3,
  "checks": [
    {"kind": "registry", "result": "passed", "checked": 2},
    {"kind": "internal_links", "result": "passed", "checked": 0},
    {"kind": "generated_freshness", "result": "passed", "checked": 1},
    {"kind": "navigation_projections", "result": "passed", "checked": 1}
  ],
  "errors": [],
  "warnings": []
}
```

## Verification

Acceptance chạy trên external mutable copy của
`tests/fixtures/target-repos/minimal-service`, không bootstrap repository Pulse.
Nó cover current/pass, broken internal link, declared freshness non-zero,
missing projection và user-authored projection conflict. Unit coverage khóa argv
quoting/no-shell behavior, Windows-style executable path, fenced-link exclusion,
percent decoding và parent-relative normalization.

## Deferred

- Profile registry quyết định check nào là close-gating và policy cho timeout.
- External-link connectivity, Markdown reference links và anchor validation.
- Tạo/bind typed `documentation_validation` receipt từ report này.
- `pulse docs validate --changed` và `pulse doctor` aggregation/ratchet routing.
