# Phase 3 Slice 4 — Documentation proof close gate

Status: implemented 2026-08-25.

## Outcome

Core proof close giờ hỗ trợ low-risk Ticket có documentation impact
`required`. Caller không truyền thêm boolean hay receipt flag vào close:
`documentation_validation` receipt phải nằm trong immutable
`verification.acceptance_proofs`, cùng proof surface với QA/evidence hiện có.

Close gate revalidate:

- current typed docs registry và applicability của exact Ticket revision;
- mọi `required_documents` vẫn current, authoritative và content-readable;
- ít nhất một passed, current, gate-eligible documentation receipt;
- exact verified source commit/repository identity;
- exact document ID, revision, path, content hash và current
  `verification_profile`;
- union receipt coverage chứa đủ toàn bộ required document IDs.

Receipt có thể cover thêm current documents vì `pulse docs validate --record`
là repository-level validation. Coverage gate chỉ yêu cầu không thiếu document
đã được Ticket khai báo explicit.

## Contract và authority

Pulse chưa release nên close gate consume trực tiếp documentation contract hiện
hành, không duy trì historical/current payload split. `review_policy=none` là
policy duy nhất hiện gate-eligible. Receipt của
`light|standard|independent|human` tiếp tục ineligible nếu semantic review hoặc
authority chưa được chứng minh; close không suy diễn approval từ mechanical
validator.

Documentation posture `none` giữ behavior cũ. `unknown` và `deferred` vẫn fail
closed; `deferred` cần promotion/defer authority riêng, không được thay bằng một
repository validation receipt.

## Lock ordering

Close và verification giữ Core write fence xuyên suốt proof revalidation.
Documentation receipt verification vì vậy dùng registry snapshot được load qua
preserve-only under-fence path; nó không reacquire repository lock, không
bootstrap/migrate state và không giảm timeout để che self-deadlock. Public
receipt verification vẫn giữ API hiện tại.

## Verification

Acceptance chạy trên external mutable copy của fixture `minimal-service` và
cover:

- missing documentation receipt giữ Ticket ở `verifying`;
- unsupported payload version bị reject ở evidence boundary;
- gate-eligible receipt thiếu required document bị reject;
- profile drift làm receipt ineligible;
- restore exact registry snapshot cho phép close sang `done`.

## Deferred

- Documentation `deferred` promotion/defer resolver và authority.
- Profile-specific semantic/human/independent review acquisition.
- Medium-or-higher risk close policy.
- `pulse doctor` aggregation/ratchet routing.
