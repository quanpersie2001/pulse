//! Durable daemon timeline reads and provider-notification persistence.
//!
//! This module touches ordered timeline rows, cursor/subscription reads, and
//! session lifecycle state through the single `StateStore`. Timeline events
//! and lifecycle mutations remain atomic; drained provider events are requeued
//! when the failpoint or durable commit fails. Dependencies are limited to the
//! parent application, `StateStore`, `ProcessOwner`, session/timeline value
//! types, and daemon error/protocol types; repository semantics are excluded.

use serde_json::Value;
use std::time::{Duration, Instant};

use super::DaemonApplication;
use crate::daemon::persistence::StateStore;
use crate::daemon::process::ProcessOwner;
use crate::daemon::protocol::DaemonResponse;
use crate::daemon::session::{SessionLifecycle, SessionRecord};
use crate::daemon::timeline::{TimelineCursor, TimelineEvent, TimelinePage};
use crate::{PulseError, Result};

impl DaemonApplication {
    pub(super) fn timeline_list(
        &self,
        cursor: Option<&TimelineCursor>,
        limit: usize,
        session_id: Option<&str>,
    ) -> Result<DaemonResponse> {
        self.refresh_all_provider_events()?;
        if !(1..=1000).contains(&limit) {
            return Err(PulseError::validation(
                "timeline_limit_invalid",
                "timeline limit must be between 1 and 1000",
            ));
        }
        self.store.with_state(false, |state| {
            let start = match cursor {
                None => 0,
                Some(cursor) => state
                    .timeline
                    .iter()
                    .position(|event| {
                        event.epoch == cursor.epoch && event.sequence == cursor.sequence
                    })
                    .map(|index| index + 1)
                    .ok_or_else(|| {
                        PulseError::validation(
                            "timeline_cursor_unknown",
                            "timeline cursor is not present in the authoritative log",
                        )
                    })?,
            };
            let filtered = state.timeline[start..]
                .iter()
                .filter(|event| {
                    session_id.map_or(true, |id| event.session_id.as_deref() == Some(id))
                })
                .cloned()
                .collect::<Vec<_>>();
            let events = filtered.iter().take(limit).cloned().collect::<Vec<_>>();
            let has_newer = filtered.len() > events.len();
            let next_cursor = events
                .last()
                .map(|event| TimelineCursor {
                    epoch: event.epoch.clone(),
                    sequence: event.sequence,
                })
                .or_else(|| cursor.cloned())
                .unwrap_or_else(|| TimelineCursor {
                    epoch: state.epoch.clone(),
                    sequence: 0,
                });
            Ok(DaemonResponse::Timeline {
                page: TimelinePage {
                    events,
                    next_cursor,
                    has_newer,
                },
            })
        })
    }

    pub(super) fn refresh_all_provider_events(&self) -> Result<()> {
        let session_ids = self.store.with_state(false, |state| {
            Ok(state.sessions.keys().cloned().collect::<Vec<_>>())
        })?;
        for session_id in session_ids {
            self.refresh_session_provider_events(&session_id)?;
        }
        Ok(())
    }

    pub(super) fn refresh_session_provider_events(&self, session_id: &str) -> Result<()> {
        let _session_guard = self
            .store
            .acquire_idempotency(&format!("session-operation:{session_id}"))?;
        self.refresh_session_provider_events_locked(session_id)
    }

    pub(super) fn refresh_session_provider_events_locked(&self, session_id: &str) -> Result<()> {
        let snapshot = self
            .store
            .with_state(false, |state| Ok(state.sessions.get(session_id).cloned()))?;
        let Some(snapshot) = snapshot else {
            return Ok(());
        };
        let Some(process_id) = snapshot.managed_process_id.as_deref() else {
            return Ok(());
        };
        let events = self.process_owner.drain_json(process_id)?;
        if events.is_empty() {
            return Ok(());
        }
        persist_provider_event_batch(
            &self.store,
            &self.process_owner,
            process_id,
            &snapshot,
            session_id,
            events,
        )
    }

    pub(super) fn timeline_subscribe(
        &self,
        cursor: &TimelineCursor,
        limit: usize,
        session_id: Option<&str>,
        wait_ms: u64,
    ) -> Result<DaemonResponse> {
        if !(1..=30_000).contains(&wait_ms) {
            return Err(PulseError::validation(
                "timeline_wait_invalid",
                "timeline subscription wait must be between 1 and 30000 milliseconds",
            ));
        }
        let deadline = Instant::now() + Duration::from_millis(wait_ms);
        loop {
            let response = self.timeline_list(Some(cursor), limit, session_id)?;
            if matches!(
                &response,
                DaemonResponse::Timeline { page } if !page.events.is_empty()
            ) || Instant::now() >= deadline
            {
                return Ok(response);
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

pub(super) fn append_event(
    state: &mut crate::daemon::persistence::DaemonState,
    event_type: &str,
    project_id: Option<&str>,
    workspace_id: Option<&str>,
    session_id: Option<&str>,
    payload: Value,
) {
    let event = TimelineEvent {
        schema_version: 1,
        event_id: format!("rtevt_{}", ulid::Ulid::new()),
        epoch: state.epoch.clone(),
        sequence: state.next_sequence,
        occurred_at: chrono::Utc::now().to_rfc3339(),
        event_type: event_type.to_string(),
        project_id: project_id.map(str::to_string),
        workspace_id: workspace_id.map(str::to_string),
        session_id: session_id.map(str::to_string),
        payload,
    };
    state.next_sequence += 1;
    state.timeline.push(event);
}

pub(super) fn persist_provider_event_batch(
    store: &StateStore,
    owner: &ProcessOwner,
    process_id: &str,
    snapshot: &SessionRecord,
    session_id: &str,
    events: Vec<Value>,
) -> Result<()> {
    let requeue = |error: PulseError| match owner.requeue_json(process_id, &events) {
        Ok(()) => Err(error),
        Err(requeue_error) => Err(PulseError::validation(
            "provider_event_requeue_failed",
            format!("provider event persistence failed: {error}; requeue failed: {requeue_error}"),
        )),
    };
    if let Err(error) = store.check_failpoint("before_provider_event_commit") {
        return requeue(error);
    }
    if let Err(error) = store.with_state(true, |state| {
        for event in &events {
            let method = event.get("method").and_then(Value::as_str);
            if method == Some("turn/completed") {
                let completed_turn = event.pointer("/params/turn/id").and_then(Value::as_str);
                if let Some(session) = state.sessions.get_mut(session_id) {
                    if completed_turn.is_none()
                        || completed_turn == session.active_turn_id.as_deref()
                    {
                        session.lifecycle = SessionLifecycle::Idle;
                        session.active_turn_id = None;
                        session.updated_at = chrono::Utc::now().to_rfc3339();
                    }
                }
            } else if method == Some("thread/started") {
                let provider_handle = event.pointer("/params/thread/id").and_then(Value::as_str);
                if let (Some(session), Some(provider_handle)) =
                    (state.sessions.get_mut(session_id), provider_handle)
                {
                    session.provider_handle = Some(provider_handle.to_string());
                    session.updated_at = chrono::Utc::now().to_rfc3339();
                }
            }
            append_event(
                state,
                "provider.notification",
                Some(&snapshot.project_id),
                Some(&snapshot.workspace_id),
                Some(session_id),
                event.clone(),
            );
        }
        Ok(())
    }) {
        return requeue(error);
    }
    Ok(())
}

/// Ingest provider output independently of request handling. This keeps
/// delayed notifications durable even when no client reads the timeline.
pub(super) fn ingest_provider_events(store: &StateStore, owner: &ProcessOwner) -> Result<()> {
    let session_ids = store.with_state(false, |state| {
        Ok(state.sessions.keys().cloned().collect::<Vec<_>>())
    })?;
    for session_id in session_ids {
        let _session_guard =
            store.acquire_idempotency(&format!("session-operation:{session_id}"))?;
        let snapshot =
            store.with_state(false, |state| Ok(state.sessions.get(&session_id).cloned()))?;
        let Some(snapshot) = snapshot else { continue };
        let Some(process_id) = snapshot.managed_process_id.as_deref() else {
            continue;
        };
        let events = owner.drain_json(process_id)?;
        if events.is_empty() {
            continue;
        }
        persist_provider_event_batch(store, owner, process_id, &snapshot, &session_id, events)?;
    }
    Ok(())
}
