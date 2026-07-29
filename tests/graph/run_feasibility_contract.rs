use pulse::run::{
    runner_profile_threat_model, RunnerProfileRegistryV1, RunnerProfileV1, PUBLIC_CODEX_ADAPTER,
};

fn profile() -> RunnerProfileV1 {
    RunnerProfileV1 {
        profile_id: "codex-local".to_string(),
        adapter: PUBLIC_CODEX_ADAPTER.to_string(),
        executable: "codex".to_string(),
        fixed_args: vec!["exec".to_string(), "--json".to_string()],
        environment_allow: vec![
            "PATH".to_string(),
            "HOME".to_string(),
            "CODEX_HOME".to_string(),
        ],
        environment_set: serde_json::Map::new(),
        start_timeout_seconds: 30,
        run_timeout_seconds: 7200,
        cancel_grace_seconds: 10,
        force_kill_after_seconds: 10,
        max_stdout_bytes: 16_777_216,
        max_stderr_bytes: 16_777_216,
    }
}

#[test]
fn profile_registry_validates_public_codex_only_contract() {
    let registry = RunnerProfileRegistryV1 {
        schema_version: 1,
        default_profile: "codex-local".to_string(),
        profiles: vec![profile()],
    };
    registry.validate().unwrap();
    assert!(registry
        .profile_fingerprint("codex-local")
        .unwrap()
        .starts_with("sha256:"));

    let mut invalid = registry.clone();
    invalid.profiles[0].adapter = "fixture_process_v1".to_string();
    assert_eq!(
        invalid.validate().unwrap_err().code(),
        "run_profile_invalid"
    );

    let mut invalid = registry;
    invalid.profiles[0]
        .environment_allow
        .push("lowercase".to_string());
    assert_eq!(
        invalid.validate().unwrap_err().code(),
        "run_profile_invalid"
    );
}

#[test]
fn threat_model_keeps_env_prompt_and_logs_private() {
    let model = runner_profile_threat_model();
    assert_eq!(model.public_adapter, PUBLIC_CODEX_ADAPTER);
    assert_eq!(model.shell_invocation, "never");
    assert!(!model.inherited_environment_values_recorded);
    assert!(model
        .environment_fingerprint_semantics
        .contains("no_inherited_values"));
    assert!(model.raw_prompt_storage.contains("runtime_private"));
    assert!(model.raw_log_storage.contains("bounded_prefix_tail"));
}
