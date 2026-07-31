//! Capability-oriented provider registry.

pub mod codex;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use crate::{PulseError, Result};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ProviderCapabilities {
    pub create: bool,
    pub resume: bool,
    pub send: bool,
    pub observe: bool,
    pub interrupt: bool,
    pub close: bool,
    #[serde(default)]
    pub native_tools: BTreeSet<String>,
}

#[derive(Debug, Clone)]
pub struct ProviderLaunch {
    pub executable: PathBuf,
    pub args: Vec<String>,
    pub provider_detail: Value,
    pub native_protocol: bool,
}

#[derive(Debug, Clone)]
pub struct ProviderRequest {
    pub request_id: String,
    pub message: String,
}

pub trait AgentProvider: Send + Sync {
    fn provider_id(&self) -> &'static str;
    fn capabilities(&self) -> ProviderCapabilities;
    fn availability(&self) -> Result<()>;
    fn launch(&self, options: &Value) -> Result<ProviderLaunch>;
    fn initialize_request(&self) -> Result<ProviderRequest>;
    fn initialized_notification(&self) -> Result<String>;
    fn create_session_request(&self, cwd: &str, options: &Value) -> Result<ProviderRequest>;
    fn resume_session_request(
        &self,
        provider_handle: &str,
        cwd: &str,
        options: &Value,
    ) -> Result<ProviderRequest>;
    fn parse_session_handle(&self, response: &Value) -> Result<String>;
    fn encode_send(&self, provider_handle: &str, input: &str) -> Result<ProviderRequest>;
    fn parse_turn_handle(&self, response: &Value) -> Result<String>;
    fn encode_interrupt(&self, provider_handle: &str, turn_handle: &str)
        -> Result<ProviderRequest>;
}

pub struct ProviderRegistry {
    providers: BTreeMap<String, Box<dyn AgentProvider>>,
}

impl ProviderRegistry {
    pub fn built_in() -> Self {
        let mut registry = Self {
            providers: BTreeMap::new(),
        };
        registry.register(Box::new(codex::CodexNativeProvider));
        registry
    }

    pub fn register(&mut self, provider: Box<dyn AgentProvider>) {
        self.providers
            .insert(provider.provider_id().to_string(), provider);
    }

    pub fn get(&self, provider_id: &str) -> Result<&dyn AgentProvider> {
        self.providers
            .get(provider_id)
            .map(Box::as_ref)
            .ok_or_else(|| {
                PulseError::validation(
                    "provider_unknown",
                    format!("unknown provider {provider_id:?}"),
                )
            })
    }
}
