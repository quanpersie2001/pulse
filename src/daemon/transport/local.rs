//! Authenticated loopback JSONL transport and endpoint discovery.

use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use crate::daemon::application::DaemonApplication;
use crate::daemon::persistence::StateStore;
use crate::daemon::protocol::{
    ProtocolError, RequestEnvelope, ResponseEnvelope, DAEMON_CAPABILITIES, PROTOCOL_VERSION,
};
use crate::{PulseError, Result};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EndpointRecord {
    pub schema_version: u32,
    pub protocol_version: u32,
    pub address: String,
    pub auth_token: String,
    pub pid: u32,
    pub started_at: String,
}

pub struct LocalClient {
    endpoint: EndpointRecord,
}

impl LocalClient {
    pub fn discover(store: &StateStore) -> Result<Self> {
        let path = store.endpoint_path();
        let bytes = fs::read(&path).map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                PulseError::validation("daemon_not_running", "daemon endpoint is not present")
            } else {
                PulseError::io(&path, error)
            }
        })?;
        let endpoint =
            serde_json::from_slice(&bytes).map_err(|error| PulseError::json(&path, error))?;
        Ok(Self { endpoint })
    }

    pub fn endpoint(&self) -> &EndpointRecord {
        &self.endpoint
    }

    pub fn request(&self, mut request: RequestEnvelope) -> Result<ResponseEnvelope> {
        if self.endpoint.protocol_version != PROTOCOL_VERSION {
            return Err(PulseError::validation(
                "daemon_protocol_incompatible",
                format!(
                    "endpoint protocol {} is incompatible with client protocol {}",
                    self.endpoint.protocol_version, PROTOCOL_VERSION
                ),
            ));
        }
        request.auth_token.clone_from(&self.endpoint.auth_token);
        let mut stream = TcpStream::connect(&self.endpoint.address)
            .map_err(|_| PulseError::validation("daemon_not_running", "daemon is unreachable"))?;
        stream
            .set_read_timeout(Some(Duration::from_secs(30)))
            .map_err(|error| PulseError::io("<daemon-socket>", error))?;
        serde_json::to_writer(&mut stream, &request).map_err(PulseError::from)?;
        stream
            .write_all(b"\n")
            .and_then(|_| stream.flush())
            .map_err(|error| PulseError::io("<daemon-socket>", error))?;
        let mut line = String::new();
        BufReader::new(stream)
            .read_line(&mut line)
            .map_err(|error| PulseError::io("<daemon-socket>", error))?;
        if line.is_empty() {
            return Err(PulseError::validation(
                "daemon_response_missing",
                "daemon closed the connection without a response",
            ));
        }
        serde_json::from_str(&line).map_err(PulseError::from)
    }
}

pub fn serve(store: StateStore) -> Result<()> {
    let _owner = store.acquire_owner()?;
    let listener =
        TcpListener::bind("127.0.0.1:0").map_err(|error| PulseError::io("<daemon-bind>", error))?;
    listener
        .set_nonblocking(true)
        .map_err(|error| PulseError::io("<daemon-bind>", error))?;
    let address = listener
        .local_addr()
        .map_err(|error| PulseError::io("<daemon-bind>", error))?
        .to_string();
    let application = Arc::new(DaemonApplication::new(store.clone(), address.clone())?);
    let endpoint = EndpointRecord {
        schema_version: 1,
        protocol_version: PROTOCOL_VERSION,
        address,
        auth_token: random_token(),
        pid: std::process::id(),
        started_at: chrono::Utc::now().to_rfc3339(),
    };
    let endpoint_bytes = crate::canonical_json::to_canonical_bytes(&endpoint)?;
    crate::storage::atomic_write_private(&store.endpoint_path(), &endpoint_bytes)?;

    let shutdown = application.shutdown_flag();
    let mut workers = Vec::new();
    let mut serve_error = None;
    while !shutdown.load(Ordering::SeqCst) {
        match listener.accept() {
            Ok((stream, _)) => {
                let application = Arc::clone(&application);
                let auth_token = endpoint.auth_token.clone();
                workers.push(thread::spawn(move || {
                    handle_connection(stream, application, &auth_token)
                }));
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                let mut index = 0;
                while index < workers.len() {
                    if workers[index].is_finished() {
                        let worker = workers.swap_remove(index);
                        if worker.join().is_err() {
                            serve_error.get_or_insert_with(|| {
                                PulseError::validation(
                                    "daemon_worker_panicked",
                                    "daemon request worker panicked",
                                )
                            });
                            application.request_shutdown();
                        }
                    } else {
                        index += 1;
                    }
                }
                thread::sleep(Duration::from_millis(25));
            }
            Err(error) => {
                serve_error = Some(PulseError::io("<daemon-accept>", error));
                application.request_shutdown();
            }
        }
    }
    for worker in workers {
        if worker.join().is_err() {
            serve_error.get_or_insert_with(|| {
                PulseError::validation("daemon_worker_panicked", "daemon request worker panicked")
            });
        }
    }
    if let Err(error) = application.shutdown_cleanup() {
        serve_error.get_or_insert(error);
    }
    let _ = fs::remove_file(store.endpoint_path());
    serve_error.map_or(Ok(()), Err)
}

fn handle_connection(
    mut stream: TcpStream,
    application: Arc<DaemonApplication>,
    auth_token: &str,
) -> Result<()> {
    let mut line = String::new();
    BufReader::new(
        stream
            .try_clone()
            .map_err(|error| PulseError::io("<daemon-socket>", error))?,
    )
    .read_line(&mut line)
    .map_err(|error| PulseError::io("<daemon-socket>", error))?;
    let envelope: RequestEnvelope = serde_json::from_str(&line).map_err(PulseError::from)?;
    let response = validate_request(&envelope, auth_token).map_or_else(Err, |_| {
        application.handle(&envelope.request, &envelope.idempotency_key)
    });
    let epoch = application
        .store()
        .load()
        .map(|state| state.epoch)
        .unwrap_or_else(|_| "epoch_unknown".to_string());
    let response = ResponseEnvelope {
        protocol_version: PROTOCOL_VERSION,
        request_id: envelope.request_id,
        daemon_epoch: epoch,
        response,
    };
    serde_json::to_writer(&mut stream, &response).map_err(PulseError::from)?;
    stream
        .write_all(b"\n")
        .and_then(|_| stream.flush())
        .map_err(|error| PulseError::io("<daemon-socket>", error))
}

fn validate_request(
    envelope: &RequestEnvelope,
    auth_token: &str,
) -> std::result::Result<(), ProtocolError> {
    if envelope.protocol_version != PROTOCOL_VERSION {
        return Err(ProtocolError::new(
            "daemon_protocol_incompatible",
            format!(
                "client protocol {} is incompatible with daemon protocol {}",
                envelope.protocol_version, PROTOCOL_VERSION
            ),
            false,
        ));
    }
    if envelope.auth_token != auth_token {
        return Err(ProtocolError::new(
            "daemon_authentication_failed",
            "daemon authentication token is invalid",
            false,
        ));
    }
    for capability in &envelope.required_capabilities {
        if !DAEMON_CAPABILITIES.contains(&capability.as_str()) {
            return Err(ProtocolError::new(
                "daemon_capability_missing",
                format!("daemon does not provide required capability {capability:?}"),
                false,
            ));
        }
    }
    Ok(())
}

fn random_token() -> String {
    let mut bytes = [0_u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}

pub fn wait_until_ready(store: &StateStore, timeout: Duration) -> Result<EndpointRecord> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Ok(client) = LocalClient::discover(store) {
            let request = RequestEnvelope::new(
                crate::daemon::protocol::DaemonRequest::Status,
                format!("status_{}", ulid::Ulid::new()),
            );
            if client.request(request).is_ok() {
                return Ok(client.endpoint().clone());
            }
        }
        if Instant::now() >= deadline {
            return Err(PulseError::validation(
                "daemon_start_timeout",
                "daemon did not become ready within the startup timeout",
            ));
        }
        thread::sleep(Duration::from_millis(50));
    }
}
