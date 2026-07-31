//! Native Codex App Server provider.

use serde_json::{json, Value};
use std::collections::BTreeSet;

use super::{AgentProvider, ProviderCapabilities, ProviderLaunch, ProviderRequest};
use crate::daemon::process::resolve_executable;
use crate::{PulseError, Result};

pub struct CodexNativeProvider;

impl AgentProvider for CodexNativeProvider {
    fn provider_id(&self) -> &'static str {
        "codex"
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            create: true,
            resume: true,
            send: true,
            observe: true,
            interrupt: true,
            close: true,
            native_tools: BTreeSet::new(),
        }
    }

    fn availability(&self) -> Result<()> {
        let executable =
            std::env::var("PULSE_CODEX_EXECUTABLE").unwrap_or_else(|_| "codex".to_string());
        resolve_executable(&executable).map(|_| ())
    }

    fn launch(&self, options: &Value) -> Result<ProviderLaunch> {
        let executable_name = options
            .get("executable")
            .and_then(Value::as_str)
            .map(str::to_string)
            .or_else(|| std::env::var("PULSE_CODEX_EXECUTABLE").ok())
            .unwrap_or_else(|| "codex".to_string());
        let executable = resolve_executable(&executable_name)?;
        let args = match options.get("args") {
            Some(Value::Array(values)) => values
                .iter()
                .map(|value| {
                    value.as_str().map(str::to_string).ok_or_else(|| {
                        PulseError::validation(
                            "provider_options_invalid",
                            "provider args must contain strings",
                        )
                    })
                })
                .collect::<Result<Vec<_>>>()?,
            Some(_) => {
                return Err(PulseError::validation(
                    "provider_options_invalid",
                    "provider args must be an array",
                ))
            }
            None => vec!["app-server".to_string()],
        };
        let native_protocol =
            options.get("protocol_mode").and_then(Value::as_str) != Some("opaque_test");
        Ok(ProviderLaunch {
            executable,
            args,
            provider_detail: json!({
                "transport": "app_server_jsonl",
                "provider": "codex",
                "native_protocol": native_protocol,
            }),
            native_protocol,
        })
    }

    fn initialize_request(&self) -> Result<ProviderRequest> {
        encode_request(
            "pulse-initialize",
            "initialize",
            json!({
                "clientInfo": {
                    "name": "pulse",
                    "title": "Pulse daemon",
                    "version": env!("CARGO_PKG_VERSION"),
                },
                "capabilities": {
                    "experimentalApi": true,
                }
            }),
        )
    }

    fn initialized_notification(&self) -> Result<String> {
        serde_json::to_string(&json!({
            "jsonrpc": "2.0",
            "method": "initialized",
        }))
        .map_err(PulseError::from)
    }

    fn create_session_request(&self, cwd: &str, options: &Value) -> Result<ProviderRequest> {
        let mut params = json!({
            "cwd": cwd,
            "threadSource": "subagent",
        });
        for field in [
            "approvalPolicy",
            "baseInstructions",
            "developerInstructions",
            "model",
            "sandbox",
            "serviceTier",
        ] {
            if let Some(value) = options.get(field) {
                params[field] = value.clone();
            }
        }
        encode_request("pulse-thread-start", "thread/start", params)
    }

    fn resume_session_request(
        &self,
        provider_handle: &str,
        cwd: &str,
        options: &Value,
    ) -> Result<ProviderRequest> {
        let mut params = json!({
            "threadId": provider_handle,
            "cwd": cwd,
        });
        for field in [
            "approvalPolicy",
            "baseInstructions",
            "developerInstructions",
            "model",
            "sandbox",
            "serviceTier",
        ] {
            if let Some(value) = options.get(field) {
                params[field] = value.clone();
            }
        }
        encode_request("pulse-thread-resume", "thread/resume", params)
    }

    fn parse_session_handle(&self, response: &Value) -> Result<String> {
        response
            .pointer("/result/thread/id")
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| {
                PulseError::validation(
                    "provider_protocol_invalid",
                    "thread/start response is missing result.thread.id",
                )
            })
    }

    fn encode_send(&self, provider_handle: &str, input: &str) -> Result<ProviderRequest> {
        let request_id = format!("pulse-turn-start-{}", ulid::Ulid::new());
        encode_request(
            &request_id,
            "turn/start",
            json!({
                "threadId": provider_handle,
                "input": [{
                    "type": "text",
                    "text": input,
                }],
            }),
        )
    }

    fn parse_turn_handle(&self, response: &Value) -> Result<String> {
        response
            .pointer("/result/turn/id")
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| {
                PulseError::validation(
                    "provider_protocol_invalid",
                    "turn/start response is missing result.turn.id",
                )
            })
    }

    fn encode_interrupt(
        &self,
        provider_handle: &str,
        turn_handle: &str,
    ) -> Result<ProviderRequest> {
        let request_id = format!("pulse-turn-interrupt-{}", ulid::Ulid::new());
        encode_request(
            &request_id,
            "turn/interrupt",
            json!({
                "threadId": provider_handle,
                "turnId": turn_handle,
            }),
        )
    }
}

fn encode_request(request_id: &str, method: &str, params: Value) -> Result<ProviderRequest> {
    Ok(ProviderRequest {
        request_id: request_id.to_string(),
        message: serde_json::to_string(&json!({
            "jsonrpc": "2.0",
            "id": request_id,
            "method": method,
            "params": params,
        }))?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codex_requests_follow_app_server_thread_and_turn_contracts() {
        let provider = CodexNativeProvider;
        let initialize = provider.initialize_request().unwrap();
        let initialize: Value = serde_json::from_str(&initialize.message).unwrap();
        assert_eq!(initialize["method"], "initialize");
        assert_eq!(initialize["params"]["clientInfo"]["name"], "pulse");

        let create = provider
            .create_session_request("/tmp/project", &json!({}))
            .unwrap();
        let create: Value = serde_json::from_str(&create.message).unwrap();
        assert_eq!(create["method"], "thread/start");
        assert_eq!(create["params"]["cwd"], "/tmp/project");

        let resume = provider
            .resume_session_request("thread-1", "/tmp/project", &json!({}))
            .unwrap();
        let resume: Value = serde_json::from_str(&resume.message).unwrap();
        assert_eq!(resume["method"], "thread/resume");
        assert_eq!(resume["params"]["threadId"], "thread-1");
        assert_eq!(resume["params"]["cwd"], "/tmp/project");

        let send = provider.encode_send("thread-1", "hello").unwrap();
        let send: Value = serde_json::from_str(&send.message).unwrap();
        assert_eq!(send["method"], "turn/start");
        assert_eq!(send["params"]["threadId"], "thread-1");
        assert_eq!(send["params"]["input"][0]["type"], "text");

        let interrupt = provider.encode_interrupt("thread-1", "turn-1").unwrap();
        let interrupt: Value = serde_json::from_str(&interrupt.message).unwrap();
        assert_eq!(interrupt["method"], "turn/interrupt");
        assert_eq!(interrupt["params"]["turnId"], "turn-1");
    }

    #[test]
    fn provider_handles_are_parsed_separately_from_pulse_ids() {
        let provider = CodexNativeProvider;
        assert_eq!(
            provider
                .parse_session_handle(&json!({"result":{"thread":{"id":"thread-native"}}}))
                .unwrap(),
            "thread-native"
        );
        assert_eq!(
            provider
                .parse_turn_handle(&json!({"result":{"turn":{"id":"turn-native"}}}))
                .unwrap(),
            "turn-native"
        );
    }
}
