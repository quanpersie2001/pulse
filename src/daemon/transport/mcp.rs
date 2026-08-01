//! MCP tool adapter over the transport-neutral daemon application service.
//!
//! An MCP host supplies the authenticated runtime principal and maps its tool
//! arguments to `RequestEnvelope`. Authorization, idempotency and semantics
//! remain identical to local protocol calls.

use crate::daemon::application::DaemonApplication;
use crate::daemon::permissions::RuntimePrincipal;
use crate::daemon::protocol::{
    validate_envelope, RequestEnvelope, ResponseEnvelope, PROTOCOL_VERSION,
};

pub struct McpToolAdapter<'a> {
    application: &'a DaemonApplication,
    principal: RuntimePrincipal,
}

impl<'a> McpToolAdapter<'a> {
    pub fn new(application: &'a DaemonApplication, principal: RuntimePrincipal) -> Self {
        Self {
            application,
            principal,
        }
    }

    pub fn invoke(&self, envelope: RequestEnvelope) -> ResponseEnvelope {
        let epoch = self
            .application
            .store()
            .load()
            .map(|state| state.epoch)
            .unwrap_or_else(|_| "epoch_unknown".to_string());
        let response = validate_envelope(&envelope, None).and_then(|()| {
            self.application.handle_as(
                &self.principal,
                &envelope.request,
                &envelope.idempotency_key,
            )
        });
        ResponseEnvelope {
            protocol_version: PROTOCOL_VERSION,
            request_id: envelope.request_id,
            daemon_epoch: epoch,
            response,
        }
    }
}
