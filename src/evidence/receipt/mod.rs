//! Receipt recording, verification and payload validation facade.
//!
//! This module re-exports the stable public receipt API and delegates to focused
//! submodules:
//! - [`store`]: persistence and read entrypoints (record/show/list/load/verify),
//!   receipt paths, recording-event idempotency and transaction integration.
//! - [`envelope`]: generic envelope/manifest validation, normalization and
//!   kind dispatch.
//! - [`bindings`]: generic work/content/source/artifact binding currentness.
//! - [`supersession`], [`shaping`], [`decision`], [`documentation`]:
//!   kind-specific payload validators.
//! - [`helpers`]: shared generic validation primitives reused across payload
//!   validators.
//!
//! Evidence owns immutable envelope integrity, generic bindings, recording and
//! kind dispatch. Documentation registry lifecycle/review-policy interpretation
//! lives in [`crate::docs::receipt_validation`] and is consumed here only as a
//! narrow validator by `verify_receipt`; evidence does not implement docs policy.

mod bindings;
mod decision;
mod documentation;
mod envelope;
mod helpers;
mod shaping;
mod store;
mod supersession;

pub use bindings::{code_to_static, content_source_binding_codes};
pub use store::{
    list_receipts, load_receipt, record_receipt, show_receipt, verify_receipt, ReceiptList,
    ReceiptOutcome, ReceiptStatus, ReceiptSummary,
};
pub use supersession::validate_for_supersession;
