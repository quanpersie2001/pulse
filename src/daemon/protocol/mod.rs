//! Versioned transport-neutral daemon protocol.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::daemon::assignment::AssignmentSagaRecord;
use crate::daemon::project::ProjectRecord;
use crate::daemon::session::{CommunicationGrantRecord, SessionMessageRecord, SessionRecord};
use crate::daemon::timeline::{TimelineCursor, TimelinePage};
use crate::daemon::workspace::{IsolationMode, WorkspaceRecord};
use crate::execution::{
    HandoffReceiptV1, VerificationCheck, VerificationDisposition, VerificationReceiptV1,
};

pub const PROTOCOL_VERSION: u32 = 1;
pub const DAEMON_CAPABILITIES: &[&str] = &[
    "project_registry_v1",
    "workspace_manager_v1",
    "session_manager_v1",
    "provider_registry_v1",
    "process_owner_v1",
    "timeline_cursor_v1",
    "timeline_subscription_v1",
    "assignment_saga_v1",
    "session_mailbox_v1",
    "mcp_tool_adapter_v1",
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RequestEnvelope {
    pub protocol_version: u32,
    pub request_id: String,
    pub idempotency_key: String,
    pub auth_token: String,
    #[serde(default)]
    pub required_capabilities: Vec<String>,
    pub request: DaemonRequest,
}

impl RequestEnvelope {
    pub fn new(request: DaemonRequest, idempotency_key: impl Into<String>) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            request_id: format!("req_{}", ulid::Ulid::new()),
            idempotency_key: idempotency_key.into(),
            auth_token: String::new(),
            required_capabilities: Vec::new(),
            request,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DaemonRequest {
    Handshake {
        client_name: String,
        client_version: String,
    },
    Status,
    Shutdown,
    ProjectOpen {
        root: String,
    },
    ProjectList {
        include_archived: bool,
    },
    ProjectArchive {
        project_id: String,
    },
    WorkspaceCreate {
        project_id: String,
        name: String,
        isolation: IsolationMode,
        base_commit: Option<String>,
    },
    WorkspaceList {
        project_id: Option<String>,
        include_archived: bool,
    },
    WorkspaceArchive {
        workspace_id: String,
    },
    WorkspaceRestore {
        workspace_id: String,
    },
    SessionCreate {
        workspace_id: String,
        provider_id: String,
        parent_session_id: Option<String>,
        provider_options: Value,
    },
    SessionList {
        workspace_id: Option<String>,
        include_archived: bool,
    },
    SessionShow {
        session_id: String,
    },
    SessionSend {
        session_id: String,
        input: String,
    },
    SessionInterrupt {
        session_id: String,
    },
    SessionClose {
        session_id: String,
    },
    SessionArchive {
        session_id: String,
    },
    SessionCommunicationGrant {
        sender_session_id: String,
        recipient_session_id: String,
    },
    SessionMessageSend {
        sender_session_id: String,
        recipient_session_id: String,
        body: String,
    },
    SessionMessages {
        session_id: String,
    },
    AssignmentStart {
        project_id: String,
        ticket_id: String,
        actor: String,
        assignee: String,
        capabilities: Vec<String>,
        isolation: IsolationMode,
        provider_id: String,
        provider_options: Value,
        ttl_seconds: u64,
    },
    AssignmentAcknowledge {
        saga_id: String,
        acknowledgement_id: String,
    },
    AssignmentInspect {
        saga_id: String,
    },
    HandoffSubmit {
        saga_id: String,
        source_commit: String,
        summary: String,
        changed_paths: Vec<String>,
        evidence_receipt_ids: Vec<String>,
    },
    VerificationComplete {
        saga_id: String,
        actor: String,
        source_commit: String,
        disposition: VerificationDisposition,
        summary: String,
        checks: Vec<VerificationCheck>,
    },
    TimelineList {
        cursor: Option<TimelineCursor>,
        limit: usize,
        session_id: Option<String>,
    },
    TimelineSubscribe {
        cursor: TimelineCursor,
        limit: usize,
        session_id: Option<String>,
        wait_ms: u64,
    },
}

impl DaemonRequest {
    pub fn is_mutating(&self) -> bool {
        !matches!(
            self,
            Self::Handshake { .. }
                | Self::Status
                | Self::ProjectList { .. }
                | Self::WorkspaceList { .. }
                | Self::SessionList { .. }
                | Self::SessionShow { .. }
                | Self::SessionMessages { .. }
                | Self::AssignmentInspect { .. }
                | Self::TimelineList { .. }
                | Self::TimelineSubscribe { .. }
        )
    }

    pub fn runtime_capability(&self) -> &'static str {
        match self {
            Self::Shutdown
            | Self::ProjectArchive { .. }
            | Self::SessionCommunicationGrant { .. } => "runtime.admin",
            Self::Handshake { .. }
            | Self::Status
            | Self::ProjectList { .. }
            | Self::WorkspaceList { .. }
            | Self::SessionList { .. }
            | Self::SessionShow { .. }
            | Self::SessionMessages { .. }
            | Self::AssignmentInspect { .. }
            | Self::TimelineList { .. }
            | Self::TimelineSubscribe { .. } => "runtime.read",
            _ => "runtime.write",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ResponseEnvelope {
    pub protocol_version: u32,
    pub request_id: String,
    pub daemon_epoch: String,
    pub response: Result<DaemonResponse, ProtocolError>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DaemonResponse {
    Handshake {
        daemon_version: String,
        capabilities: Vec<String>,
    },
    Status {
        pid: u32,
        epoch: String,
        started_at: String,
        endpoint: String,
    },
    ShuttingDown,
    Project {
        project: ProjectRecord,
    },
    Projects {
        projects: Vec<ProjectRecord>,
    },
    Workspace {
        workspace: WorkspaceRecord,
    },
    Workspaces {
        workspaces: Vec<WorkspaceRecord>,
    },
    Session {
        session: SessionRecord,
    },
    Sessions {
        sessions: Vec<SessionRecord>,
    },
    CommunicationGrant {
        grant: CommunicationGrantRecord,
    },
    SessionMessage {
        message: SessionMessageRecord,
    },
    SessionMessages {
        messages: Vec<SessionMessageRecord>,
    },
    Assignment {
        saga: AssignmentSagaRecord,
    },
    Handoff {
        handoff: HandoffReceiptV1,
    },
    Verification {
        verification: VerificationReceiptV1,
    },
    Accepted {
        resource_id: String,
    },
    Timeline {
        page: TimelinePage,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ProtocolError {
    pub code: String,
    pub message: String,
    pub retryable: bool,
    #[serde(default)]
    pub details: Box<Value>,
}

impl ProtocolError {
    pub fn new(code: impl Into<String>, message: impl Into<String>, retryable: bool) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            retryable,
            details: Box::new(Value::Null),
        }
    }
}
