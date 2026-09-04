# Design archive

Historical design material. Nothing here is a current contract or a request
to implement anything. The current product definition is
[`PRODUCT.md`](../../PRODUCT.md); the reason this material was retired is
[Decision 0008](../../docs/decisions/0008-narrow-scope-to-truth-layer.md).

- `proposals/`: implementation-slice proposals from the Rust reboot
  (Phase 1 to Phase 3, 2026-07 to 2026-08). They describe what was built and
  why at the time. Relative links inside them assumed the repository root and
  may be broken here.
- The `pulse-reboot/` design set and `PULSE_REBOOT.md` were deleted on
  2026-09-05 after their content was absorbed into `PRODUCT.md`. Recover them
  from Git history before that commit if needed.
- `daemon-assignment-saga.rs`: the daemon's reservation-to-delivery
  acknowledgement and typed Core activation saga, archived 2026-09 when
  `src/daemon/` was removed. Kept uncompiled as a reference for the runner's
  lease, crash and recovery semantics in a later step.
