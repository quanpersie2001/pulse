use pulse::daemon::persistence::StateStore;
use pulse::daemon::protocol::DaemonResponse;
use pulse::daemon::protocol::{DaemonRequest, RequestEnvelope, PROTOCOL_VERSION};
use pulse::daemon::transport::local::{self, LocalClient};
use std::io::Write;
use std::time::Duration;

#[test]
fn protocol_version_reflects_breaking_session_resume_contract() {
    assert_eq!(PROTOCOL_VERSION, 2);
}

#[test]
fn prior_daemon_state_shape_loads_with_defaulted_new_fields() {
    let home = tempfile::tempdir().unwrap();
    let store = StateStore::new(home.path());
    std::fs::write(
        store.state_path(),
        br#"{"assignment_sagas":{},"epoch":"epoch_old","next_sequence":1,"processes":{},"projects":{},"schema_version":1,"session_messages":{},"sessions":{},"timeline":[],"workspaces":{}}"#,
    )
    .unwrap();
    let state = store.load().unwrap();
    assert_eq!(state.epoch, "epoch_old");
    assert!(state.idempotency_results.is_empty());
    assert!(state.communication_grants.is_empty());
}

#[test]
fn concurrent_daemon_start_has_exactly_one_owner() {
    let home = tempfile::tempdir().unwrap();
    let store = StateStore::new(home.path());
    let first_store = store.clone();
    let second_store = store.clone();
    let first = std::thread::spawn(move || local::serve(first_store));
    local::wait_until_ready(&store, Duration::from_secs(5)).unwrap();
    let second = std::thread::spawn(move || local::serve(second_store));
    let second_error = second.join().unwrap().unwrap_err();
    assert_eq!(second_error.code(), "daemon_already_running");

    let client = LocalClient::discover(&store).unwrap();
    let status = client
        .request(RequestEnvelope::new(DaemonRequest::Status, ""))
        .unwrap()
        .response
        .unwrap();
    assert!(matches!(status, DaemonResponse::Status { .. }));
    client
        .request(RequestEnvelope::new(
            DaemonRequest::Shutdown,
            "stop-concurrent-owner",
        ))
        .unwrap()
        .response
        .unwrap();
    first.join().unwrap().unwrap();
}

#[test]
fn malformed_client_does_not_bypass_shutdown_cleanup() {
    let home = tempfile::tempdir().unwrap();
    let store = StateStore::new(home.path());
    let server_store = store.clone();
    let server = std::thread::spawn(move || local::serve(server_store));
    local::wait_until_ready(&store, Duration::from_secs(5)).unwrap();
    let client = LocalClient::discover(&store).unwrap();
    std::net::TcpStream::connect(&client.endpoint().address)
        .unwrap()
        .write_all(b"not-json\n")
        .unwrap();
    std::thread::sleep(Duration::from_millis(50));
    assert!(matches!(
        client
            .request(RequestEnvelope::new(DaemonRequest::Status, ""))
            .unwrap()
            .response
            .unwrap(),
        DaemonResponse::Status { .. }
    ));
    client
        .request(RequestEnvelope::new(
            DaemonRequest::Shutdown,
            "stop-after-malformed-client",
        ))
        .unwrap()
        .response
        .unwrap();
    server.join().unwrap().unwrap();
    assert!(!store.endpoint_path().exists());
    assert_eq!(
        store
            .load()
            .unwrap()
            .timeline
            .iter()
            .filter(|event| event.event_type == "daemon.shutdown")
            .count(),
        1
    );
}

#[test]
fn local_protocol_rejects_mismatch_before_mutation_and_shutdowns_cleanly() {
    let home = tempfile::tempdir().unwrap();
    let store = StateStore::new(home.path());
    let server_store = store.clone();
    let server = std::thread::spawn(move || local::serve(server_store));
    local::wait_until_ready(&store, Duration::from_secs(5)).unwrap();
    let client = LocalClient::discover(&store).unwrap();
    let handshake = client
        .request(RequestEnvelope::new(
            DaemonRequest::Handshake {
                client_name: "daemon-contract-test".to_string(),
                client_version: env!("CARGO_PKG_VERSION").to_string(),
            },
            "",
        ))
        .unwrap()
        .response
        .unwrap();
    match handshake {
        DaemonResponse::Handshake { capabilities, .. } => {
            assert!(capabilities.contains(&"session_resume_v1".to_string()));
            assert!(capabilities.contains(&"session_attach_v1".to_string()));
        }
        other => panic!("unexpected handshake response: {other:?}"),
    }

    let mut mismatch = RequestEnvelope::new(
        DaemonRequest::ProjectOpen {
            root: home.path().to_string_lossy().to_string(),
        },
        "protocol-mismatch",
    );
    mismatch.protocol_version = PROTOCOL_VERSION + 1;
    let response = client.request(mismatch).unwrap();
    assert_eq!(
        response.response.unwrap_err().code,
        "daemon_protocol_incompatible"
    );
    assert!(store.load().unwrap().projects.is_empty());

    let stop = RequestEnvelope::new(DaemonRequest::Shutdown, "stop-local-protocol");
    assert!(client.request(stop).unwrap().response.is_ok());
    server.join().unwrap().unwrap();
    assert!(!store.endpoint_path().exists());
    let state = store.load().unwrap();
    assert_eq!(
        state
            .timeline
            .iter()
            .filter(|event| event.event_type == "daemon.shutdown")
            .count(),
        1
    );
}

#[test]
fn local_protocol_rejects_unknown_required_capability_before_mutation() {
    let home = tempfile::tempdir().unwrap();
    let store = StateStore::new(home.path());
    let server_store = store.clone();
    let server = std::thread::spawn(move || local::serve(server_store));
    local::wait_until_ready(&store, Duration::from_secs(5)).unwrap();
    let client = LocalClient::discover(&store).unwrap();
    let mut request = RequestEnvelope::new(
        DaemonRequest::ProjectOpen {
            root: home.path().to_string_lossy().to_string(),
        },
        "unknown-required-capability",
    );
    request.required_capabilities = vec!["not_a_daemon_capability".to_string()];
    let response = client.request(request).unwrap();
    assert_eq!(
        response.response.unwrap_err().code,
        "daemon_capability_missing"
    );
    assert!(store.load().unwrap().projects.is_empty());
    client
        .request(RequestEnvelope::new(
            DaemonRequest::Shutdown,
            "stop-capability-parity",
        ))
        .unwrap();
    server.join().unwrap().unwrap();
}
