use jsonschema::JSONSchema;
use pulse::assignment::{
    AssignmentDispatch, AssignmentGateFamily, AssignmentLeaseSummary, AssignmentLifecycle,
    AssignmentSubjectSnapshot, AssignmentTransaction, AssignmentWorkspaceSummary,
    CapabilityMatchReport, PreparedAssignmentV1, RevalidatedSnapshot, DISPATCH_AUTHORIZED_STATUS,
    LEASE_STATE_PREPARED, LIFECYCLE_GATE_PROFILE, LIFECYCLE_READY_TO_ACTIVE,
    PREPARED_ASSIGNMENT_PROFILE, RUNNER_STATUS_NOT_STARTED, WORKSPACE_MODE_ISOLATED,
    WORKSPACE_STATE_BOUND,
};
use pulse::canonical_json::{to_canonical_bytes, to_canonical_value_from};
use pulse::run::{
    runner_profile_threat_model, NativeResumeStatusV1, ProcessIdentityV1, RunAssignmentV1,
    RunAttemptInputRefV1, RunAttemptLogsV1, RunAttemptProcessV1, RunAttemptRecordV1,
    RunAttemptStateV1, RunCancelReportV1, RunCancelStateV1, RunExitKindV1, RunExitResultV1,
    RunInputModeV1, RunInputResumeContextV1, RunInputRunnerProfileV1, RunInputV1,
    RunInstructionsV1, RunLogHashScopeV1, RunLogRefV1, RunRecordV1, RunRecoveryClassificationV1,
    RunRecoveryReportV1, RunRunnerV1, RunStartReportV1, RunStateV1, RunSubjectV1, RunViewV1,
    RunWorkspaceBindingV1, RunnerAdapterV1, RunnerProfileRegistryV1, RunnerProfileV1,
    WorkspaceCleanlinessV1, WorkspaceModeV1, WorkspaceOperationStateV1, WorkspaceSnapshotStatusV1,
    WorkspaceSnapshotV1, DEFAULT_LOG_REDACTION_STATUS, PUBLIC_CODEX_ADAPTER,
    RUNNER_PROFILES_SCHEMA, RUN_ATTEMPT_SCHEMA, RUN_CANCEL_REPORT_SCHEMA, RUN_INPUT_PROFILE,
    RUN_INPUT_SCHEMA, RUN_KIND_SINGLE_AGENT_IMPLEMENTATION, RUN_RECOVERY_REPORT_SCHEMA, RUN_SCHEMA,
    RUN_SCHEMA_VERSION, RUN_START_REPORT_SCHEMA, WORKSPACE_SNAPSHOT_SCHEMA,
};
use pulse::work_packet::{
    PacketAssurance, PacketBudget, PacketCapabilities, PacketContext, PacketDecisionFrontier,
    PacketDispatch, PacketDocsApplicability, PacketDocsIndex, PacketDocumentation,
    PacketDocumentationImpact, PacketFutureGate, PacketGraph, PacketImplementationContractV1,
    PacketKnowledge, PacketQaStatus, PacketReadBudget, PacketRelationBundle, PacketScope,
    PacketScopeEnforcement, PacketScopeHints, PacketShaping, PacketShapingDestination,
    PacketShapingWorkBinding, PacketSource, PacketSuggestionQuery, PacketWorkspace, SnapshotReport,
    SubjectSnapshot, WorkPacketV1, BUDGET_PROFILE, MAX_INITIAL_LINES, MAX_SNIPPET_BYTES_EACH,
    MAX_SUGGESTED_SECTIONS, PACKET_PROFILE,
};
use serde::Serialize;
use serde_json::{json, Value};

const ZERO: &str = "sha256:0000000000000000000000000000000000000000000000000000000000000000";
const ONE: &str = "sha256:1111111111111111111111111111111111111111111111111111111111111111";
const TWO: &str = "sha256:2222222222222222222222222222222222222222222222222222222222222222";
const THREE: &str = "sha256:3333333333333333333333333333333333333333333333333333333333333333";
const FOUR: &str = "sha256:4444444444444444444444444444444444444444444444444444444444444444";

fn schema_validate(schema: &str, value: &Value) {
    let schema_json: Value = serde_json::from_str(schema).unwrap();
    let compiled = JSONSchema::compile(&schema_json).unwrap();
    if !compiled.is_valid(value) {
        let errors = compiled
            .validate(value)
            .unwrap_err()
            .map(|error| error.to_string())
            .collect::<Vec<_>>()
            .join("; ");
        panic!("schema validation failed: {errors}");
    }
}

fn assert_no_floats(value: &Value) {
    match value {
        Value::Number(number) => assert!(!number.is_f64(), "float in canonical contract"),
        Value::Array(items) => items.iter().for_each(assert_no_floats),
        Value::Object(map) => map.values().for_each(assert_no_floats),
        Value::Null | Value::Bool(_) | Value::String(_) => {}
    }
}

fn canonical_value<T: Serialize>(value: &T) -> Value {
    let canonical = to_canonical_value_from(value).unwrap();
    assert_no_floats(&canonical);
    canonical
}

fn minimal_packet() -> WorkPacketV1 {
    WorkPacketV1 {
        schema_version: 1,
        profile: PACKET_PROFILE.to_string(),
        code: "reservation_candidate".to_string(),
        subject: SubjectSnapshot {
            id: "TK-031".to_string(),
            kind: "ticket".to_string(),
            role: "implementation".to_string(),
            title: "Run value contracts".to_string(),
            revision: 8,
            contract_revision: 4,
            status: "ready".to_string(),
            risk: "medium".to_string(),
            materialization: "R1".to_string(),
            content_dir: "works/TK-031".to_string(),
        },
        snapshot: SnapshotReport {
            graph_fingerprint: ZERO.to_string(),
            readiness_profile: "phase1_contract_readiness_v1".to_string(),
            readiness_fingerprint: ONE.to_string(),
            readiness_status: "ready".to_string(),
            authority_policy_revision: 1,
            authority_policy_fingerprint: TWO.to_string(),
            docs_registry_revision: 1,
            docs_registry_fingerprint: THREE.to_string(),
            docs_index_fingerprint: FOUR.to_string(),
            source_commit: "0123456789abcdef0123456789abcdef01234567".to_string(),
        },
        contract: PacketImplementationContractV1 {
            mode: "guided".to_string(),
            work_surface: "code".to_string(),
            plan_policy: "worker_optional".to_string(),
            semantic_impact: "behavior_or_public_risk_change".to_string(),
            effort: Default::default(),
            verification_profile: "service-change".to_string(),
            brief: None,
            objective: "Implement run DTOs".to_string(),
            current_behavior: "No run DTOs".to_string(),
            target_behavior: "Strict run DTOs".to_string(),
            code_anchors: vec![],
            documentation_anchors: vec![],
            configuration_anchors: vec![],
            data_anchors: vec![],
            research_refs: vec![],
            required_changes: vec![],
            invariants: vec![],
            acceptance: vec![],
            scope: Default::default(),
            implementation_freedom: vec![],
            required_decisions: vec![],
            shared_approach_refs: vec![],
            expected_evidence: vec![],
            expected_handoff: vec![],
        },
        context: PacketContext {
            parents: vec![],
            decisions: vec![],
        },
        shaping: PacketShaping {
            status: "current".to_string(),
            receipt_id: "rcpt_00000000000000000000000000".to_string(),
            receipt_hash: ZERO.to_string(),
            owning_work: PacketShapingWorkBinding {
                id: "ST-001".to_string(),
                revision_observed: 3,
                contract_revision: 2,
            },
            shape_mode: "focused_branches".to_string(),
            destination: Some(PacketShapingDestination {
                summary: "Deliver run value contracts".to_string(),
                scope_boundary: vec![],
                exit_conditions: vec![],
            }),
            map: None,
            critical_branches: vec![],
            bounded_fog: vec![],
            remaining_uncertainty: vec![],
            decision_frontier: PacketDecisionFrontier {
                status: "evaluated".to_string(),
                items: vec![],
            },
        },
        graph: PacketGraph {
            structural_state: "executable".to_string(),
            hard_blockers: vec![],
            soft_preferences: vec![],
            supersession: None,
            relations: PacketRelationBundle::default(),
        },
        documentation: PacketDocumentation {
            applicability: PacketDocsApplicability {
                status: "complete".to_string(),
                required: vec![],
                optional: vec![],
                write_candidates: vec![],
                excluded: vec![],
            },
            suggestion_query: PacketSuggestionQuery {
                text: "run value contracts".to_string(),
                normalized_terms: vec!["run".to_string(), "contracts".to_string()],
            },
            suggested_sections: vec![],
            read_budget: PacketReadBudget {
                required_sections: 0,
                recommended_initial_sections: 4,
                max_initial_lines: MAX_INITIAL_LINES as u64,
                suggestion_limit: MAX_SUGGESTED_SECTIONS as u64,
                snippet_max_bytes_each: MAX_SNIPPET_BYTES_EACH as u64,
            },
            index: PacketDocsIndex {
                state: "current".to_string(),
                fingerprint: ZERO.to_string(),
                mode: "lexical".to_string(),
            },
        },
        knowledge: PacketKnowledge {
            status: "not_installed".to_string(),
            owner_phase: 4,
            knowledge_fingerprint: None,
            required: vec![],
            recommended: vec![],
            suggested: vec![],
            excluded: vec![],
        },
        source: PacketSource {
            repository_id: "repo_test".to_string(),
            kind: "git_commit".to_string(),
            commit: "0123456789abcdef0123456789abcdef01234567".to_string(),
            head_ref: None,
            worktree_root_kind: "primary_or_existing_worktree".to_string(),
            cleanliness: "clean".to_string(),
            operation_state: "normal".to_string(),
            currentness: "current".to_string(),
        },
        workspace: PacketWorkspace {
            binding_status: "not_allocated".to_string(),
            workspace_id: None,
            required_strategy: "isolated_worktree_required".to_string(),
            base_repository_id: "repo_test".to_string(),
            base_commit: "0123456789abcdef0123456789abcdef01234567".to_string(),
            requirements: vec![],
        },
        capabilities: PacketCapabilities {
            evaluation_status: "not_evaluated".to_string(),
            required: vec!["source.write".to_string()],
            optional: vec![],
            missing: vec![],
            inventory_identity: None,
        },
        scope: PacketScope {
            scope_hints: PacketScopeHints::default(),
            implementation_freedom: vec![],
            hard_stops: vec!["stop_on_source_or_contract_drift".to_string()],
            enforcement: PacketScopeEnforcement {
                status: "not_installed".to_string(),
                owner_phase: 2,
            },
        },
        assurance: PacketAssurance {
            verification_profile: "service-change".to_string(),
            expected_evidence: vec![],
            expected_handoff: vec![],
            documentation_impact: PacketDocumentationImpact::default(),
            qa: PacketQaStatus {
                posture: "none".to_string(),
                status: "ready_gate_satisfied".to_string(),
                affected_case_ids: vec![],
            },
            promotion_policy: PacketFutureGate {
                status: "not_installed".to_string(),
                owner_phase: 2,
            },
            close_gate: PacketFutureGate {
                status: "not_installed".to_string(),
                owner_phase: 2,
            },
        },
        dispatch: PacketDispatch::default(),
        budget: PacketBudget {
            profile: BUDGET_PROFILE.to_string(),
            ..PacketBudget::default()
        },
        packet_fingerprint: ZERO.to_string(),
        reason_codes: vec![],
    }
}

fn prepared_assignment() -> PreparedAssignmentV1 {
    PreparedAssignmentV1 {
        schema_version: 1,
        profile: PREPARED_ASSIGNMENT_PROFILE.to_string(),
        code: "prepared_assignment".to_string(),
        prepared_assignment_id: "pa_01JTEST".to_string(),
        subject: AssignmentSubjectSnapshot {
            id: "TK-031".to_string(),
            kind: "ticket".to_string(),
            revision_before: 8,
            revision_after: 9,
            contract_revision: 4,
            status_before: "ready".to_string(),
            status_after: "active".to_string(),
        },
        packet: minimal_packet(),
        packet_fingerprint: ZERO.to_string(),
        revalidated_snapshot: RevalidatedSnapshot {
            graph_fingerprint: ZERO.to_string(),
            readiness_profile: "phase1_contract_readiness_v1".to_string(),
            readiness_fingerprint: ONE.to_string(),
            authority_policy_fingerprint: TWO.to_string(),
            docs_registry_fingerprint: THREE.to_string(),
            docs_index_fingerprint: FOUR.to_string(),
            source_commit: "0123456789abcdef0123456789abcdef01234567".to_string(),
            source_cleanliness: "clean".to_string(),
            repository_id: "repo_test".to_string(),
        },
        lease: AssignmentLeaseSummary {
            lease_id: "lease_01JTEST".to_string(),
            state: LEASE_STATE_PREPARED.to_string(),
            assignee: "agent:codex-local".to_string(),
            issued_by: "human:test".to_string(),
            issued_at: "2026-07-29T10:00:00Z".to_string(),
            expires_at: "2026-07-29T10:30:00Z".to_string(),
            ttl_seconds: 1800,
            exclusive: true,
        },
        workspace: workspace_summary(),
        capability_match: CapabilityMatchReport {
            inventory_identity: ZERO.to_string(),
            principal: "agent:codex-local".to_string(),
            status: "matched".to_string(),
            required: vec!["source.write".to_string()],
            matched: vec!["source.write".to_string()],
            missing: vec![],
            extra: vec![],
            reason_codes: vec![],
        },
        lifecycle: AssignmentLifecycle {
            transition: LIFECYCLE_READY_TO_ACTIVE.to_string(),
            gate_profile: LIFECYCLE_GATE_PROFILE.to_string(),
            gate_status: "passed".to_string(),
            expected_revision: 8,
            new_revision: 9,
            event_id: "evt_01JTEST".to_string(),
        },
        dispatch: AssignmentDispatch {
            dispatch_authorized: true,
            authorization_status: DISPATCH_AUTHORIZED_STATUS.to_string(),
            runner_status: RUNNER_STATUS_NOT_STARTED.to_string(),
            gate_families: vec![AssignmentGateFamily {
                family: "lease".to_string(),
                status: "passed".to_string(),
                reason_codes: vec![],
            }],
        },
        transaction: AssignmentTransaction::default(),
        prepared_assignment_fingerprint: ZERO.to_string(),
        reason_codes: vec![],
    }
}

fn workspace_summary() -> AssignmentWorkspaceSummary {
    AssignmentWorkspaceSummary {
        workspace_id: "wt_TK-031_01JTEST".to_string(),
        binding_status: WORKSPACE_STATE_BOUND.to_string(),
        mode: WORKSPACE_MODE_ISOLATED.to_string(),
        path: ".pulse/runtime/workspaces/wt_TK-031_01JTEST".to_string(),
        repository_id: "repo_test".to_string(),
        base_commit: "0123456789abcdef0123456789abcdef01234567".to_string(),
        cleanliness: "clean".to_string(),
        owner_lease_id: "lease_01JTEST".to_string(),
    }
}

fn snapshot() -> WorkspaceSnapshotV1 {
    let mut value = WorkspaceSnapshotV1 {
        schema_version: RUN_SCHEMA_VERSION,
        repository_id: "repo_test".to_string(),
        workspace_id: "wt_TK-031_01JTEST".to_string(),
        workspace_mode: WorkspaceModeV1::IsolatedWorktree,
        base_commit: "0123456789abcdef0123456789abcdef01234567".to_string(),
        head_commit: "0123456789abcdef0123456789abcdef01234567".to_string(),
        diff_base_commit: "0123456789abcdef0123456789abcdef01234567".to_string(),
        operation_state: WorkspaceOperationStateV1::None,
        cleanliness: WorkspaceCleanlinessV1::Clean,
        tracked_diff_identity: ZERO.to_string(),
        untracked_manifest_identity: ONE.to_string(),
        status_identity: TWO.to_string(),
        snapshot_status: WorkspaceSnapshotStatusV1::Complete,
        captured_at: "2026-07-29T10:00:00Z".to_string(),
        snapshot_identity: String::new(),
    };
    value.snapshot_identity = value.compute_identity().unwrap();
    value
}

fn log_ref(stream: &str) -> RunLogRefV1 {
    RunLogRefV1 {
        path: format!(".pulse/runtime/run/logs/run_01JTEST/attempt_01JTEST.{stream}.prefix.log"),
        retained_prefix_path: None,
        retained_tail_path: None,
        bytes_seen: 0,
        bytes_retained: 0,
        bytes_truncated: 0,
        content_hash: ZERO.to_string(),
        hash_scope: RunLogHashScopeV1::FullUntruncatedContent,
        truncated: false,
        redaction_status: DEFAULT_LOG_REDACTION_STATUS.to_string(),
    }
}

fn run_record() -> RunRecordV1 {
    let mut value = RunRecordV1 {
        schema_version: RUN_SCHEMA_VERSION,
        run_id: "run_01JTEST".to_string(),
        kind: RUN_KIND_SINGLE_AGENT_IMPLEMENTATION.to_string(),
        state: RunStateV1::Running,
        subject: RunSubjectV1 {
            kind: "ticket".to_string(),
            id: "TK-031".to_string(),
            active_revision: 9,
            contract_revision: 4,
        },
        assignment: RunAssignmentV1 {
            lease_id: "lease_01JTEST".to_string(),
            prepared_assignment_id: "pa_01JTEST".to_string(),
            prepared_assignment_fingerprint: ZERO.to_string(),
            packet_fingerprint: ZERO.to_string(),
            assignee: "agent:codex-local".to_string(),
        },
        workspace: RunWorkspaceBindingV1 {
            workspace_id: "wt_TK-031_01JTEST".to_string(),
            mode: WorkspaceModeV1::IsolatedWorktree,
            path: ".pulse/runtime/workspaces/wt_TK-031_01JTEST".to_string(),
            repository_id: "repo_test".to_string(),
            base_commit: "0123456789abcdef0123456789abcdef01234567".to_string(),
        },
        runner: RunRunnerV1 {
            adapter: RunnerAdapterV1::CodexProcessV1,
            profile_id: "codex-local".to_string(),
            profile_fingerprint: ZERO.to_string(),
            resolved_executable_identity: Some("best_effort:test".to_string()),
            native_resume_status: NativeResumeStatusV1::NotInstalled,
            native_thread_id: None,
        },
        current_attempt_id: "attempt_01JTEST".to_string(),
        attempt_ids: vec!["attempt_01JTEST".to_string()],
        created_by: "human:test".to_string(),
        created_at: "2026-07-29T10:00:00Z".to_string(),
        updated_at: "2026-07-29T10:00:01Z".to_string(),
        last_heartbeat_at: Some("2026-07-29T10:00:05Z".to_string()),
        latest_exit: None,
        latest_workspace_snapshot_identity: Some(snapshot().snapshot_identity),
        reason_codes: vec![],
        run_fingerprint: String::new(),
    };
    value.run_fingerprint = value.compute_fingerprint().unwrap();
    value
}

fn attempt_record() -> RunAttemptRecordV1 {
    let mut value = RunAttemptRecordV1 {
        schema_version: RUN_SCHEMA_VERSION,
        attempt_id: "attempt_01JTEST".to_string(),
        run_id: "run_01JTEST".to_string(),
        attempt_number: 1,
        state: RunAttemptStateV1::Running,
        input: RunAttemptInputRefV1 {
            run_input_identity: ZERO.to_string(),
            json_path: ".pulse/runtime/run/inputs/run_01JTEST.attempt_01JTEST.json".to_string(),
            rendered_prompt_identity: ONE.to_string(),
            rendered_prompt_path: ".pulse/runtime/run/inputs/run_01JTEST.attempt_01JTEST.md"
                .to_string(),
        },
        process: RunAttemptProcessV1 {
            identity: Some(ProcessIdentityV1 {
                supervisor_pid: 41001,
                child_pid: 41002,
                process_group_id: Some(41002),
                supervisor_nonce_hash: TWO.to_string(),
                started_at: "2026-07-29T10:00:01Z".to_string(),
                platform_start_marker: "linux_proc_starttime:123".to_string(),
                argv_hash: THREE.to_string(),
                executable_identity: "best_effort:test".to_string(),
                identity_status: "verified".to_string(),
            }),
            started_at: Some("2026-07-29T10:00:01Z".to_string()),
            ended_at: None,
            exit: None,
        },
        workspace_before: snapshot(),
        workspace_after: None,
        logs: RunAttemptLogsV1 {
            stdout: log_ref("stdout"),
            stderr: log_ref("stderr"),
        },
        timeout_seconds: 7200,
        cancel: RunCancelStateV1 {
            requested_at: None,
            requested_by: None,
            reason: None,
            grace_seconds: None,
            force_allowed: None,
        },
        created_at: "2026-07-29T10:00:00Z".to_string(),
        updated_at: "2026-07-29T10:00:01Z".to_string(),
        reason_codes: vec![],
        attempt_fingerprint: String::new(),
    };
    value.attempt_fingerprint = value.compute_fingerprint().unwrap();
    value
}

fn run_input() -> RunInputV1 {
    let mut value = RunInputV1 {
        schema_version: RUN_SCHEMA_VERSION,
        profile: RUN_INPUT_PROFILE.to_string(),
        run_id: "run_01JTEST".to_string(),
        attempt_id: "attempt_01JTEST".to_string(),
        attempt_number: 1,
        mode: RunInputModeV1::Start,
        prepared_assignment: prepared_assignment(),
        workspace: workspace_summary(),
        runner_profile: RunInputRunnerProfileV1 {
            profile_id: "codex-local".to_string(),
            adapter: RunnerAdapterV1::CodexProcessV1,
            profile_fingerprint: ZERO.to_string(),
        },
        instructions: RunInstructionsV1 {
            objective: "Implement run DTOs".to_string(),
            acceptance: vec!["schemas pass".to_string()],
            required_changes: vec!["add run.rs".to_string()],
            invariants: vec!["do not close ticket".to_string()],
            hard_stops: vec!["stop on drift".to_string()],
            expected_evidence: vec!["cargo test".to_string()],
            expected_handoff: vec!["commit hash".to_string()],
            authority_boundary: vec![
                "do_not_change_acceptance".to_string(),
                "do_not_close_ticket".to_string(),
                "do_not_merge_or_deploy".to_string(),
            ],
        },
        resume: RunInputResumeContextV1 {
            previous_attempt_id: None,
            workspace_snapshot_identity: None,
            previous_exit_kind: None,
            redacted_log_tail: None,
            native_resume_status: NativeResumeStatusV1::NotInstalled,
        },
        input_fingerprint: String::new(),
    };
    value.input_fingerprint = value.compute_fingerprint().unwrap();
    value
}

fn profile_registry() -> RunnerProfileRegistryV1 {
    RunnerProfileRegistryV1 {
        schema_version: RUN_SCHEMA_VERSION,
        default_profile: "codex-local".to_string(),
        profiles: vec![RunnerProfileV1 {
            profile_id: "codex-local".to_string(),
            adapter: RunnerAdapterV1::CodexProcessV1,
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
        }],
    }
}

#[test]
fn run_contracts_round_trip_validate_and_use_nullable_optionals() {
    let run = run_record();
    let attempt = attempt_record();
    let input = run_input();
    let snapshot = snapshot();
    let registry = profile_registry();
    registry.validate().unwrap();

    let run_value = canonical_value(&run);
    assert!(run_value["last_heartbeat_at"].is_string());
    assert!(run_value["latest_exit"].is_null());
    assert!(run_value["runner"]["native_thread_id"].is_null());
    schema_validate(RUN_SCHEMA, &run_value);
    // Attempt schema references the independent snapshot schema by URL; the
    // full resolver is covered by Cargo compile/include_str, while the DTO and
    // snapshot schemas validate independently here.
    assert!(
        RUN_ATTEMPT_SCHEMA.contains("WorkspaceSnapshotV1")
            || RUN_ATTEMPT_SCHEMA.contains("workspace-snapshot.schema.json")
    );
    schema_validate(RUN_INPUT_SCHEMA, &canonical_value(&input));
    schema_validate(WORKSPACE_SNAPSHOT_SCHEMA, &canonical_value(&snapshot));
    schema_validate(RUNNER_PROFILES_SCHEMA, &canonical_value(&registry));

    let start_report = RunStartReportV1 {
        schema_version: 1,
        run: run.clone(),
        attempt: attempt.clone(),
        terminal_observation_pending: false,
        handoff_status: "not_installed".to_string(),
        verification_status: "not_installed".to_string(),
    };
    let start_value = canonical_value(&start_report);
    assert_eq!(start_value["handoff_status"], "not_installed");
    assert!(RUN_START_REPORT_SCHEMA.contains("run.schema.json"));
    let cancel_report = RunCancelReportV1 {
        schema_version: 1,
        run_id: run.run_id.clone(),
        attempt_id: attempt.attempt_id.clone(),
        state: RunStateV1::Running,
        already_terminal: false,
        reason_codes: vec![],
    };
    schema_validate(RUN_CANCEL_REPORT_SCHEMA, &canonical_value(&cancel_report));
    let recovery_report = RunRecoveryReportV1 {
        schema_version: 1,
        classifications: vec![RunRecoveryClassificationV1 {
            run_id: Some(run.run_id.clone()),
            attempt_id: Some(attempt.attempt_id.clone()),
            classification: "live".to_string(),
            mutation_available: false,
            reason_codes: vec![],
        }],
        mutations_applied: vec![],
        reason_codes: vec![],
    };
    schema_validate(
        RUN_RECOVERY_REPORT_SCHEMA,
        &canonical_value(&recovery_report),
    );
    drop(start_report);

    let _: RunRecordV1 = serde_json::from_value(run_value).unwrap();
    let _: RunAttemptRecordV1 = serde_json::from_value(canonical_value(&attempt)).unwrap();
    let _: RunInputV1 = serde_json::from_value(canonical_value(&input)).unwrap();
}

#[test]
fn serde_and_schemas_reject_unknown_future_transport_fields() {
    let mut value = canonical_value(&run_record());
    value["runner"]["native_mailbox_id"] = json!("mbox_01J");
    assert!(serde_json::from_value::<RunRecordV1>(value.clone()).is_err());
    let schema_json: Value = serde_json::from_str(RUN_SCHEMA).unwrap();
    let compiled = JSONSchema::compile(&schema_json).unwrap();
    assert!(compiled.validate(&value).is_err());

    let mut input = canonical_value(&run_input());
    input["handoff_proof"] = json!("not_slice_3");
    assert!(serde_json::from_value::<RunInputV1>(input.clone()).is_err());
    let schema_json: Value = serde_json::from_str(RUN_INPUT_SCHEMA).unwrap();
    let compiled = JSONSchema::compile(&schema_json).unwrap();
    assert!(compiled.validate(&input).is_err());
}

#[test]
fn fingerprints_exclude_only_self_and_heartbeat_fields() {
    let mut left = run_record();
    let mut right = left.clone();
    left.run_fingerprint =
        "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string();
    right.run_fingerprint =
        "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_string();
    right.last_heartbeat_at = Some("2026-07-29T10:99:99Z".to_string());
    assert_eq!(
        left.compute_fingerprint().unwrap(),
        right.compute_fingerprint().unwrap()
    );
    right.updated_at = "2026-07-29T10:00:02Z".to_string();
    assert_ne!(
        left.compute_fingerprint().unwrap(),
        right.compute_fingerprint().unwrap()
    );

    let mut attempt_left = attempt_record();
    let mut attempt_right = attempt_left.clone();
    attempt_left.attempt_fingerprint =
        "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string();
    attempt_right.attempt_fingerprint =
        "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_string();
    assert_eq!(
        attempt_left.compute_fingerprint().unwrap(),
        attempt_right.compute_fingerprint().unwrap()
    );
    attempt_right.timeout_seconds += 1;
    assert_ne!(
        attempt_left.compute_fingerprint().unwrap(),
        attempt_right.compute_fingerprint().unwrap()
    );
}

#[test]
fn normalization_is_deterministic_for_set_like_fields() {
    let mut registry = profile_registry();
    registry.profiles[0].environment_allow =
        vec!["PATH".to_string(), "HOME".to_string(), "PATH".to_string()];
    registry.profiles[0].fixed_args =
        vec!["--json".to_string(), "exec".to_string(), "exec".to_string()];
    registry.normalize();
    assert_eq!(registry.profiles[0].environment_allow, vec!["HOME", "PATH"]);
    assert_eq!(registry.profiles[0].fixed_args, vec!["--json", "exec"]);

    let mut input = run_input();
    input.instructions.acceptance = vec!["b".to_string(), "a".to_string(), "a".to_string()];
    input.normalize();
    assert_eq!(input.instructions.acceptance, vec!["a", "b"]);
}

#[test]
fn profile_fingerprint_and_environment_spec_do_not_expose_environment_values() {
    let mut registry = profile_registry();
    registry.profiles[0]
        .environment_set
        .insert("TOKEN".to_string(), Value::String("secret-one".to_string()));
    let first_env = registry.profiles[0].environment_spec_fingerprint().unwrap();
    let first_profile = registry.profiles[0].fingerprint().unwrap();
    registry.profiles[0]
        .environment_set
        .insert("TOKEN".to_string(), Value::String("secret-two".to_string()));
    let second_env = registry.profiles[0].environment_spec_fingerprint().unwrap();
    let second_profile = registry.profiles[0].fingerprint().unwrap();
    assert_eq!(first_env, second_env);
    assert_ne!(
        first_profile, second_profile,
        "tracked literal config still changes profile identity"
    );
    let encoded = String::from_utf8(to_canonical_bytes(&registry).unwrap()).unwrap();
    assert!(
        encoded.contains("secret-two"),
        "tracked environment_set remains config, not report output"
    );

    let model = runner_profile_threat_model();
    assert_eq!(model.public_adapter, RunnerAdapterV1::CodexProcessV1);
    assert!(!model.inherited_environment_values_recorded);
    assert_eq!(model.shell_invocation, "never");
}

#[test]
fn public_report_projection_keeps_missing_fields_nullable() {
    let view = RunViewV1 {
        schema_version: 1,
        run: None,
        current_attempt: None,
        resume_eligibility: pulse::run::ResumeEligibilityV1::NotEvaluated,
        resume_blockers: vec![],
        terminal_observation_pending: false,
        invalid_reason: None,
    };
    let value = canonical_value(&view);
    assert!(value["run"].is_null());
    assert!(value["current_attempt"].is_null());
    assert!(value["invalid_reason"].is_null());
    assert_eq!(PUBLIC_CODEX_ADAPTER, "codex_process_v1");
}

#[test]
fn slice1_and_slice2_fingerprints_are_not_reinterpreted_by_run_input() {
    let prepared = prepared_assignment();
    let before = serde_json::to_vec(&prepared).unwrap();
    let input = run_input();
    let after = serde_json::to_vec(&input.prepared_assignment).unwrap();
    assert_eq!(
        before, after,
        "RunInputV1 embeds PreparedAssignmentV1 without mutation"
    );
    assert!(
        !input
            .prepared_assignment
            .packet
            .dispatch
            .dispatch_authorized
    );
    assert_eq!(
        input.prepared_assignment.dispatch.runner_status,
        RUNNER_STATUS_NOT_STARTED
    );
}

#[test]
fn canonical_json_rejects_floats_in_public_contracts() {
    let value = json!({ "schema_version": 1, "bad": 1.25 });
    assert!(pulse::canonical_json::to_canonical_value(&value).is_err());
    assert_no_floats(&canonical_value(&run_record()));
}

#[test]
fn profile_validation_rejects_test_adapter_and_bad_bounds() {
    let mut value = canonical_value(&profile_registry());
    value["profiles"][0]["adapter"] = json!("fixture_process_v1");
    assert!(serde_json::from_value::<RunnerProfileRegistryV1>(value).is_err());

    let mut registry = profile_registry();
    registry.profiles[0].executable = "tools/codex".to_string();
    assert_eq!(
        registry.validate().unwrap_err().code(),
        "run_profile_invalid"
    );
    registry.profiles[0].executable = "codex".to_string();
    registry.profiles[0].run_timeout_seconds = 59;
    assert_eq!(
        registry.validate().unwrap_err().code(),
        "run_profile_invalid"
    );
}

#[test]
fn exit_result_is_observation_not_acceptance_or_handoff() {
    let mut run = run_record();
    run.state = RunStateV1::Exited;
    run.latest_exit = Some(RunExitResultV1 {
        kind: RunExitKindV1::Exited,
        code: Some(0),
        signal: None,
        timed_out: false,
        cancelled: false,
        observed_at: "2026-07-29T11:00:00Z".to_string(),
    });
    let mut attempt = attempt_record();
    attempt.state = RunAttemptStateV1::Exited;
    attempt.process.exit = run.latest_exit.clone();
    let report = RunStartReportV1 {
        schema_version: 1,
        run,
        attempt,
        terminal_observation_pending: true,
        handoff_status: "not_installed".to_string(),
        verification_status: "not_installed".to_string(),
    };
    let value = canonical_value(&report);
    assert_eq!(value["handoff_status"], "not_installed");
    assert_eq!(value["verification_status"], "not_installed");
    assert_eq!(value["run"]["latest_exit"]["code"], 0);
}
