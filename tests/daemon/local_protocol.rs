use pulse::daemon::persistence::StateStore;
use pulse::daemon::protocol::{DaemonRequest, RequestEnvelope, PROTOCOL_VERSION};
use pulse::daemon::transport::local::{self, LocalClient};
use std::time::Duration;

#[test]
fn local_protocol_rejects_mismatch_before_mutation_and_shutdowns_cleanly() {
    let home = tempfile::tempdir().unwrap();
    let store = StateStore::new(home.path());
    let server_store = store.clone();
    let server = std::thread::spawn(move || local::serve(server_store));
    local::wait_until_ready(&store, Duration::from_secs(5)).unwrap();
    let client = LocalClient::discover(&store).unwrap();

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
}
