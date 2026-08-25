# Phase 3 Slice 3 — Documentation validation receipts

Status: implemented 2026-08-25.

## Outcome

`pulse docs validate --record --actor <actor>` chạy deterministic repository
validation rồi ghi một immutable `documentation_validation` receipt. CLI chỉ
record khi toàn bộ check pass; nếu validation fail, output vẫn là report Slice
2 và không tạo receipt.

Payload version 2 bind mỗi current document bằng exact:

- document ID và registry revision;
- `verification_profile`;
- repository-relative path và content hash;
- source commit/repository identity;
- registry, generated sources/outputs và generated navigation projections đã
  tham gia validation.

Payload version 1 và schema cũ không bị rewrite. Bootstrap chỉ cài thêm schema
v2/manifest entry còn thiếu và giữ nguyên `repository_id`, nên receipt lịch sử
vẫn integrity-valid theo contract đã ghi.

## Snapshot và failure posture

Kernel chụp bounded input snapshot trước khi chạy effectful declared freshness
commands, chụp lại sau khi pass, rồi Evidence revalidate source/content ngay
trước record. Registry, document, generated input/output hoặc projection drift
trong khoảng đó làm record fail closed.

Snapshot:

- canonicalize target repo root trước traversal;
- reject symlink trong generated trees;
- tối đa 10.000 files và 128 MiB;
- không bind `.git`, runtime/cache, receipt store và event log vốn thay đổi do
  chính operation record;
- không dùng shell và không tự regenerate content.

## Current verification và authority

`pulse evidence receipt verify <id> --current` giờ yêu cầu content/source,
registry và review-policy structure đều current/applicable. Với payload v2,
registry đổi `verification_profile` tạo
`document_receipt_profile_mismatch` và command trả non-zero.

`review_policy=none` có thể gate-eligible khi các mechanical checks pass.
`standard`, `independent` và `human` không được suy diễn semantic review hoặc
approval từ việc chạy validator: missing checks/authority tiếp tục unresolved
hoặc ineligible theo docs receipt policy hiện có.

## Verification

Acceptance dùng mutable external copy của fixture `minimal-service`: validate,
record v2, verify current/gate eligibility, mutate profile và chứng minh
`--current` fail. Focused tests còn khóa registry/profile drift, historical v1
compatibility và bootstrap upgrade v2 không đổi repository identity.

## Deferred

- Profile-specific check selection/timeout contract ngoài mechanical check set
  hiện tại.
- Semantic/human/independent review acquisition và authority resolver.
- Docs close-gate consumption cho Ticket `required` đã được triển khai ở Phase
  3 Slice 4; `deferred` promotion authority vẫn chưa có resolver.
- `pulse docs validate --changed` và `pulse doctor` aggregation/ratchet routing.
