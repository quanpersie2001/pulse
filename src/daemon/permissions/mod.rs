//! Runtime authorization independent from Core work authority.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RuntimePrincipal {
    pub principal_id: String,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub capabilities: BTreeSet<String>,
}

impl RuntimePrincipal {
    pub fn local_cli() -> Self {
        Self {
            principal_id: "local_cli".to_string(),
            session_id: None,
            capabilities: ["runtime.read", "runtime.write", "runtime.admin"]
                .into_iter()
                .map(str::to_string)
                .collect(),
        }
    }

    pub fn require(&self, capability: &str) -> Result<(), &'static str> {
        if self.capabilities.contains(capability) {
            Ok(())
        } else {
            Err("runtime_permission_denied")
        }
    }

    pub fn require_session_sender(&self, sender_session_id: &str) -> Result<(), &'static str> {
        if self.capabilities.contains("runtime.admin")
            || self.session_id.as_deref() == Some(sender_session_id)
        {
            Ok(())
        } else {
            Err("session_sender_identity_required")
        }
    }

    pub fn require_session_access(&self, session_id: &str) -> Result<(), &'static str> {
        if self.capabilities.contains("runtime.admin")
            || self.session_id.as_deref() == Some(session_id)
        {
            Ok(())
        } else {
            Err("session_access_denied")
        }
    }
}
