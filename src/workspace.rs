//! Workspace mode definitions and binding helpers (P2S2-I1/DTO layer).
//!
//! This module defines workspace modes and enums for managing execution
//! locations (in-place or isolated worktree). Pure value types only;
//! Git worktree commands and source validation live in later slices.
//!
//! Ownership: `src/workspace.rs` is the public neutral value owner.
//! Worktree creation logic belongs in the future `src/workspace.rs` expansion
//! or `src/source.rs`; this file defines only the type vocabulary.
//!
//! See `proposals/phase2-slice2-atomic-reservation-workspace-binding.md`.

use std::str::FromStr;

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Workspace mode
// ---------------------------------------------------------------------------

/// Describes how a workspace is provisioned.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceMode {
    /// Work in the repository root directly (only allowed when packet says
    /// `in_place_allowed`).
    InPlace,
    /// Work in a `git worktree add --detach` managed workspace.
    IsolatedWorktree,
}

impl WorkspaceMode {
    /// Canonical string representation.
    pub fn as_str(&self) -> &'static str {
        match self {
            WorkspaceMode::InPlace => "in_place",
            WorkspaceMode::IsolatedWorktree => "isolated_worktree",
        }
    }
}

// ---------------------------------------------------------------------------
// Workspace strategy (derived from risk)
// ---------------------------------------------------------------------------

/// The workspace strategy dictated by a work item's risk assessment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceStrategy {
    /// In-place is permitted; isolated worktree is optional.
    InPlaceAllowed,
    /// Isolated worktree is required; in-place is forbidden.
    IsolatedWorktreeRequired,
    /// Risk is unassessed — workspace strategy unknown.
    Unassessed,
}

impl FromStr for WorkspaceMode {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "in_place" => Ok(WorkspaceMode::InPlace),
            "isolated_worktree" => Ok(WorkspaceMode::IsolatedWorktree),
            _ => Err(()),
        }
    }
}

impl WorkspaceStrategy {
    /// Canonical string representation.
    pub fn as_str(&self) -> &'static str {
        match self {
            WorkspaceStrategy::InPlaceAllowed => "in_place_allowed",
            WorkspaceStrategy::IsolatedWorktreeRequired => "isolated_worktree_required",
            WorkspaceStrategy::Unassessed => "unassessed",
        }
    }

    /// Whether in-place is an acceptable mode for this strategy.
    pub fn allows_in_place(&self) -> bool {
        matches!(self, WorkspaceStrategy::InPlaceAllowed)
    }

    /// Whether isolated worktree is required.
    pub fn requires_isolated(&self) -> bool {
        matches!(self, WorkspaceStrategy::IsolatedWorktreeRequired)
    }
}

// ---------------------------------------------------------------------------
// Workspace binding status (projection)
// ---------------------------------------------------------------------------

/// Status of a workspace binding in a prepared assignment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BindingStatus {
    /// Workspace is bound and ready.
    Bound,
    /// Workspace has been released.
    Released,
    /// Workspace needs operator review.
    StaleNeedsOperator,
    /// Workspace not yet allocated.
    NotAllocated,
}

impl FromStr for WorkspaceStrategy {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "in_place_allowed" => Ok(WorkspaceStrategy::InPlaceAllowed),
            "isolated_worktree_required" => Ok(WorkspaceStrategy::IsolatedWorktreeRequired),
            "unassessed" => Ok(WorkspaceStrategy::Unassessed),
            _ => Err(()),
        }
    }
}

impl BindingStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            BindingStatus::Bound => "bound",
            BindingStatus::Released => "released",
            BindingStatus::StaleNeedsOperator => "stale_needs_operator",
            BindingStatus::NotAllocated => "not_allocated",
        }
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_mode_round_trip() {
        use std::str::FromStr;
        assert_eq!(WorkspaceMode::InPlace.as_str(), "in_place");
        assert_eq!(
            WorkspaceMode::IsolatedWorktree.as_str(),
            "isolated_worktree"
        );
        assert_eq!(
            WorkspaceMode::from_str("in_place"),
            Ok(WorkspaceMode::InPlace)
        );
        assert_eq!(
            WorkspaceMode::from_str("isolated_worktree"),
            Ok(WorkspaceMode::IsolatedWorktree)
        );
        assert!(WorkspaceMode::from_str("unknown").is_err());
    }

    #[test]
    fn workspace_strategy_mapping() {
        assert!(WorkspaceStrategy::InPlaceAllowed.allows_in_place());
        assert!(!WorkspaceStrategy::InPlaceAllowed.requires_isolated());
        assert!(!WorkspaceStrategy::IsolatedWorktreeRequired.allows_in_place());
        assert!(WorkspaceStrategy::IsolatedWorktreeRequired.requires_isolated());
        assert!(!WorkspaceStrategy::Unassessed.allows_in_place());
        assert!(!WorkspaceStrategy::Unassessed.requires_isolated());
    }

    #[test]
    fn binding_status_values() {
        assert_eq!(BindingStatus::Bound.as_str(), "bound");
        assert_eq!(BindingStatus::Released.as_str(), "released");
        assert_eq!(
            BindingStatus::StaleNeedsOperator.as_str(),
            "stale_needs_operator"
        );
        assert_eq!(BindingStatus::NotAllocated.as_str(), "not_allocated");
    }
}
