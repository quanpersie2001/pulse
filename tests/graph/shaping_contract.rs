use chrono::{TimeZone, Utc};
use pulse::canonical_json::to_canonical_bytes;
use pulse::graph::contract::{
    ContentRef, ContractItem, ContractValidationMode, DecisionWorkContract, DecisionWorkProvenance,
    EffortMetadata, ExpectedEvidence, ExpectedHandoff, GapKind, ImplementationContract,
    ImplementationMode, ImplementationSemanticImpact, Materialization, PlanPolicy,
    PublicCreateClassification, QaImpactPosture, QaMetadata, ReceiptRef, ResolutionTarget,
    ResolutionTargetKind, RevisionedWorkRef, Risk, SurfaceRef, TicketRole, WorkSurface,
};
use pulse::graph::node::Node;
use pulse::graph::validate::validate_node_schema_semantics;
use pulse::id::WorkKind;

const HASH: &str = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const RECEIPT_HASH: &str =
    "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

fn now() -> chrono::DateTime<Utc> {
    Utc.timestamp_opt(1, 0).unwrap()
}

fn ticket(id: &str) -> Node {
    Node::new(
        id.to_string(),
        WorkKind::Ticket,
        "Ticket".to_string(),
        now(),
    )
    .unwrap()
}

fn item(id: &str) -> ContractItem {
    ContractItem {
        id: id.to_string(),
        summary: format!("{id} summary"),
    }
}

fn qa(posture: QaImpactPosture) -> QaMetadata {
    QaMetadata {
        impact: pulse::graph::contract::QaImpact {
            posture,
            rationale: None,
            behavioral_owner: None,
            affected_case_ids: vec![],
        },
    }
}

fn valid_implementation() -> ImplementationContract {
    ImplementationContract {
        mode: ImplementationMode::Guided,
        work_surface: WorkSurface::Code,
        plan_policy: PlanPolicy::None,
        semantic_impact: ImplementationSemanticImpact::BehaviorOrPublicRiskChange,
        effort: EffortMetadata::default(),
        verification_profile: "service-change".to_string(),
        brief: Some(ContentRef {
            path: "works/TK-001/ticket.md".to_string(),
            content_hash: HASH.to_string(),
        }),
        objective: "Change refresh-token failure taxonomy.".to_string(),
        current_behavior: "Expired and invalid tokens share one error.".to_string(),
        target_behavior: "Expired and invalid tokens have distinct semantics.".to_string(),
        code_anchors: vec![SurfaceRef::path("src/auth/refresh.rs")],
        documentation_anchors: vec![],
        configuration_anchors: vec![],
        data_anchors: vec![],
        research_refs: vec![],
        required_changes: vec![item("CHG-ERROR")],
        invariants: vec![item("INV-NO-SECRET-LEAK")],
        acceptance: vec![item("AC-EXPIRED")],
        scope: Default::default(),
        implementation_freedom: vec![item("FREE-HELPER")],
        required_decisions: vec![],
        shared_approach_refs: vec![],
        expected_evidence: vec![ExpectedEvidence::FocusedTestOutput],
        expected_handoff: vec![ExpectedHandoff::AcceptanceToEvidence],
    }
}

fn assessed_implementation_ticket() -> Node {
    let mut node = ticket("TK-001");
    node.risk = Some(Risk::Low);
    node.materialization = Some(Materialization::R1);
    node.implementation = Some(valid_implementation());
    node.qa = Some(QaMetadata {
        impact: pulse::graph::contract::QaImpact {
            posture: QaImpactPosture::Required,
            rationale: Some("Behavior changes require a targeted case.".to_string()),
            behavioral_owner: Some("ST-001".to_string()),
            affected_case_ids: vec!["QA-AUTH-EXPIRED".to_string()],
        },
    });
    node
}

fn decision_work() -> DecisionWorkContract {
    DecisionWorkContract {
        destination_owner: RevisionedWorkRef {
            id: "ST-001".to_string(),
            contract_revision: 3,
        },
        branch_id: "BR-TOKEN-COMPAT".to_string(),
        gap_kind: GapKind::TradeoffGap,
        question: "Must legacy clients retain the generic invalid-token path?".to_string(),
        expected_output: "A Decision with compatibility direction.".to_string(),
        expected_evidence: vec![ExpectedEvidence::ClientContractInventory],
        resolution_target: Some(ResolutionTarget {
            kind: ResolutionTargetKind::Decision,
            id: "DEC-006".to_string(),
        }),
        provenance: DecisionWorkProvenance {
            shaping_receipt: "rcpt_01JTEST".to_string(),
            fog_id: None,
        },
    }
}

fn error_codes(node: &Node, mode: ContractValidationMode) -> Vec<String> {
    pulse::graph::contract::validate_node_contract(node, mode)
        .errors
        .into_iter()
        .map(|finding| finding.code)
        .collect()
}

#[test]
fn canonical_storage_ticket_defaults_do_not_fabricate_contract() {
    let node = ticket("TK-001");
    assert_eq!(node.schema_version, 1);
    assert_eq!(node.contract_revision, 1);
    assert_eq!(node.role, Some(TicketRole::Implementation));
    assert_eq!(node.risk, Some(Risk::Unassessed));
    assert_eq!(node.materialization, Some(Materialization::Unassessed));
    assert!(node.implementation.is_none());
    assert!(
        pulse::graph::contract::validate_node_contract(
            &node,
            ContractValidationMode::CanonicalStorage
        )
        .valid
    );
    assert!(error_codes(&node, ContractValidationMode::Completeness)
        .contains(&"implementation_contract_missing".to_string()));
    assert_eq!(
        pulse::graph::contract::validate_public_create_classification(
            WorkKind::Ticket,
            &PublicCreateClassification::default(),
        )
        .unwrap_err()
        .code(),
        "work_classification_missing"
    );
}

#[test]
fn non_ticket_role_contract_and_qa_fields_reject() {
    let mut story = Node::new(
        "ST-001".to_string(),
        WorkKind::Story,
        "Story".to_string(),
        now(),
    )
    .unwrap();
    story.role = Some(TicketRole::Implementation);
    story.risk = Some(Risk::Low);
    story.materialization = Some(Materialization::R0);
    story.qa = Some(qa(QaImpactPosture::Unknown));

    let err = validate_node_schema_semantics(&story).unwrap_err();
    assert_eq!(err.code(), "work_role_invalid");
}

#[test]
fn role_contract_mismatch_and_both_blocks_reject() {
    let mut node = assessed_implementation_ticket();
    node.decision_work = Some(decision_work());
    assert!(error_codes(&node, ContractValidationMode::CanonicalStorage)
        .contains(&"work_role_invalid".to_string()));

    let mut mismatch = assessed_implementation_ticket();
    mismatch.role = Some(TicketRole::DecisionWork);
    assert!(
        error_codes(&mismatch, ContractValidationMode::CanonicalStorage)
            .contains(&"work_role_invalid".to_string())
    );
    assert!(error_codes(&mismatch, ContractValidationMode::Completeness)
        .contains(&"decision_work_contract_missing".to_string()));
}

#[test]
fn completeness_requires_contract_and_contract_validation_requires_acceptance() {
    let mut missing = ticket("TK-001");
    missing.risk = Some(Risk::Low);
    missing.materialization = Some(Materialization::R0);
    assert!(error_codes(&missing, ContractValidationMode::Completeness)
        .contains(&"implementation_contract_missing".to_string()));
    assert!(
        !error_codes(&missing, ContractValidationMode::CanonicalStorage)
            .contains(&"implementation_contract_missing".to_string())
    );

    let mut no_acceptance = assessed_implementation_ticket();
    no_acceptance
        .implementation
        .as_mut()
        .unwrap()
        .acceptance
        .clear();
    assert!(
        error_codes(&no_acceptance, ContractValidationMode::CanonicalStorage)
            .contains(&"implementation_acceptance_missing".to_string())
    );
}

#[test]
fn guided_code_requires_code_anchor_but_documentation_uses_typed_document_anchor() {
    let mut missing_code = assessed_implementation_ticket();
    missing_code
        .implementation
        .as_mut()
        .unwrap()
        .code_anchors
        .clear();
    assert!(
        error_codes(&missing_code, ContractValidationMode::CanonicalStorage)
            .contains(&"implementation_anchor_missing".to_string())
    );

    let mut docs = assessed_implementation_ticket();
    let implementation = docs.implementation.as_mut().unwrap();
    implementation.work_surface = WorkSurface::Documentation;
    implementation.code_anchors.clear();
    implementation.documentation_anchors = vec![SurfaceRef::path("docs/auth.md")];
    assert!(
        !error_codes(&docs, ContractValidationMode::CanonicalStorage)
            .contains(&"implementation_anchor_missing".to_string())
    );
}

#[test]
fn r1_to_r3_require_invariant_but_r0_can_be_concise() {
    let mut r1 = assessed_implementation_ticket();
    r1.implementation.as_mut().unwrap().invariants.clear();
    assert!(error_codes(&r1, ContractValidationMode::CanonicalStorage)
        .contains(&"implementation_invariant_missing".to_string()));

    let mut r0 = r1.clone();
    r0.materialization = Some(Materialization::R0);
    r0.implementation.as_mut().unwrap().mode = ImplementationMode::Open;
    assert!(!error_codes(&r0, ContractValidationMode::CanonicalStorage)
        .contains(&"implementation_invariant_missing".to_string()));
}

#[test]
fn brief_hash_and_content_dir_are_structural() {
    let mut node = assessed_implementation_ticket();
    node.implementation.as_mut().unwrap().brief = Some(ContentRef {
        path: "works/ST-001/not-this-ticket.md".to_string(),
        content_hash: "sha256:not-a-real-hash".to_string(),
    });
    let codes = error_codes(&node, ContractValidationMode::CanonicalStorage);
    assert!(codes.contains(&"implementation_contract_invalid".to_string()));
    assert!(codes.contains(&"implementation_brief_hash_stale".to_string()));
}

#[test]
fn locked_work_requires_decision_or_shared_approach_ref() {
    let mut locked = assessed_implementation_ticket();
    locked.implementation.as_mut().unwrap().mode = ImplementationMode::Locked;
    assert!(
        error_codes(&locked, ContractValidationMode::CanonicalStorage)
            .contains(&"required_decision_missing".to_string())
    );

    locked.implementation.as_mut().unwrap().shared_approach_refs =
        vec![pulse::graph::contract::SharedApproachRef {
            owner: RevisionedWorkRef {
                id: "ST-001".to_string(),
                contract_revision: 2,
            },
            path: "works/ST-001/approach.md".to_string(),
            content_hash: HASH.to_string(),
        }];
    assert!(
        !error_codes(&locked, ContractValidationMode::CanonicalStorage)
            .contains(&"required_decision_missing".to_string())
    );
}

#[test]
fn valid_concise_r0_needs_no_plan_map_or_decision() {
    let mut node = assessed_implementation_ticket();
    node.materialization = Some(Materialization::R0);
    let implementation = node.implementation.as_mut().unwrap();
    implementation.mode = ImplementationMode::Open;
    implementation.plan_policy = PlanPolicy::None;
    implementation.invariants.clear();
    implementation.required_decisions.clear();
    implementation.shared_approach_refs.clear();
    assert!(
        pulse::graph::contract::validate_node_contract(
            &node,
            ContractValidationMode::CanonicalStorage
        )
        .valid
    );
}

#[test]
fn decision_work_invalid_destination_and_missing_branch_reject() {
    let mut node = ticket("TK-002");
    node.role = Some(TicketRole::DecisionWork);
    node.risk = Some(Risk::Medium);
    node.materialization = Some(Materialization::R2);
    node.qa = Some(qa(QaImpactPosture::Unknown));
    node.decision_work = Some(decision_work());
    let work = node.decision_work.as_mut().unwrap();
    work.destination_owner.id = "TK-001".to_string();
    work.branch_id = "BAD".to_string();

    let codes = error_codes(&node, ContractValidationMode::CanonicalStorage);
    assert!(codes.contains(&"decision_work_destination_invalid".to_string()));
    assert!(codes.contains(&"decision_work_branch_missing".to_string()));
}

#[test]
fn decision_work_branch_id_validates_portable_suffix_boundaries() {
    let mut node = ticket("TK-002");
    node.role = Some(TicketRole::DecisionWork);
    node.risk = Some(Risk::Medium);
    node.materialization = Some(Materialization::R2);
    node.qa = Some(qa(QaImpactPosture::Unknown));
    node.decision_work = Some(decision_work());

    node.decision_work.as_mut().unwrap().branch_id = "BR-A".to_string();
    assert!(
        !error_codes(&node, ContractValidationMode::CanonicalStorage)
            .contains(&"decision_work_branch_missing".to_string())
    );

    node.decision_work.as_mut().unwrap().branch_id = "BR--BAD".to_string();
    assert!(error_codes(&node, ContractValidationMode::CanonicalStorage)
        .contains(&"decision_work_branch_missing".to_string()));
}

#[test]
fn precise_decision_work_contract_is_model_valid_without_nested_shaping() {
    let mut node = ticket("TK-002");
    node.role = Some(TicketRole::DecisionWork);
    node.risk = Some(Risk::Medium);
    node.materialization = Some(Materialization::R2);
    node.qa = Some(qa(QaImpactPosture::Unknown));
    node.decision_work = Some(decision_work());
    assert!(
        pulse::graph::contract::validate_node_contract(
            &node,
            ContractValidationMode::CanonicalStorage
        )
        .valid
    );
    assert!(
        node.shaping.is_none(),
        "decision work does not require a nested shaping pointer"
    );
}

#[test]
fn stable_item_ids_are_unique_bounded_and_normalized_deterministically() {
    let mut node = assessed_implementation_ticket();
    let implementation = node.implementation.as_mut().unwrap();
    implementation.acceptance = vec![item("AC-B"), item("AC-A")];
    implementation.required_changes = vec![item("CHG-B"), item("CHG-A")];
    node.normalize_contract_fields();
    let bytes = to_canonical_bytes(&node).unwrap();
    assert!(
        std::str::from_utf8(&bytes).unwrap().find("AC-A").unwrap()
            < std::str::from_utf8(&bytes).unwrap().find("AC-B").unwrap()
    );

    let mut duplicate = node.clone();
    duplicate.implementation.as_mut().unwrap().acceptance = vec![item("AC-DUP"), item("AC-DUP")];
    assert!(
        error_codes(&duplicate, ContractValidationMode::CanonicalStorage)
            .contains(&"implementation_acceptance_missing".to_string())
    );
}

#[test]
fn effort_flags_and_typed_semantic_impact_are_present() {
    let mut node = assessed_implementation_ticket();
    let implementation = node.implementation.as_mut().unwrap();
    implementation.effort = EffortMetadata {
        multi_session: true,
        multiple_dependent_decisions: true,
        resume_or_audit_continuity: true,
    };
    implementation.semantic_impact = ImplementationSemanticImpact::NoBehaviorOrPublicRiskChange;
    node.qa = Some(QaMetadata {
        impact: pulse::graph::contract::QaImpact {
            posture: QaImpactPosture::None,
            rationale: Some(
                "Internal refactor with no behavior or public-risk change.".to_string(),
            ),
            behavioral_owner: None,
            affected_case_ids: vec![],
        },
    });
    assert!(implementation.effort.requires_r2_map());
    assert!(
        pulse::graph::contract::validate_node_contract(
            &node,
            ContractValidationMode::CanonicalStorage
        )
        .valid
    );
}

#[test]
fn shaping_pointer_allowed_for_story_ticket_but_not_decision() {
    let pointer = pulse::graph::contract::ShapingPointer {
        receipt: ReceiptRef {
            id: "rcpt_01JTEST".to_string(),
            hash: RECEIPT_HASH.to_string(),
        },
        map: None,
        applied_at: now(),
        applied_by: "human:test".to_string(),
    };
    let mut story = Node::new(
        "ST-001".to_string(),
        WorkKind::Story,
        "Story".to_string(),
        now(),
    )
    .unwrap();
    story.shaping = Some(pointer.clone());
    assert!(
        pulse::graph::contract::validate_node_contract(
            &story,
            ContractValidationMode::CanonicalStorage
        )
        .valid
    );

    let mut decision = Node::new(
        "DEC-001".to_string(),
        WorkKind::Decision,
        "Decision".to_string(),
        now(),
    )
    .unwrap();
    decision.shaping = Some(pointer);
    assert!(
        error_codes(&decision, ContractValidationMode::CanonicalStorage)
            .contains(&"work_role_invalid".to_string())
    );
}

#[test]
fn revision_and_contract_revision_are_distinct() {
    let mut node = assessed_implementation_ticket();
    node.revision = 9;
    node.contract_revision = 4;
    assert_ne!(node.revision, node.contract_revision);
    assert!(
        pulse::graph::contract::validate_node_contract(
            &node,
            ContractValidationMode::CanonicalStorage
        )
        .valid
    );

    node.contract_revision = 0;
    assert!(error_codes(&node, ContractValidationMode::CanonicalStorage)
        .contains(&"contract_revision_invalid".to_string()));
}

#[test]
fn public_create_classification_requires_assessed_ticket_values_only() {
    let complete = PublicCreateClassification {
        role: Some(TicketRole::Implementation),
        risk: Some(Risk::Low),
        materialization: Some(Materialization::R0),
    };
    pulse::graph::contract::validate_public_create_classification(WorkKind::Ticket, &complete)
        .unwrap();

    let mut unassessed = complete.clone();
    unassessed.risk = Some(Risk::Unassessed);
    assert_eq!(
        pulse::graph::contract::validate_public_create_classification(
            WorkKind::Ticket,
            &unassessed,
        )
        .unwrap_err()
        .code(),
        "risk_materialization_unassessed"
    );

    assert_eq!(
        pulse::graph::contract::validate_public_create_classification(WorkKind::Story, &complete)
            .unwrap_err()
            .code(),
        "work_classification_not_allowed"
    );
}

#[test]
fn canonical_storage_validation_allows_assessed_ticket_missing_contract_but_completeness_reports_it(
) {
    let mut node = ticket("TK-001");
    node.risk = Some(Risk::Low);
    node.materialization = Some(Materialization::R0);

    assert!(
        pulse::graph::contract::validate_node_contract(
            &node,
            ContractValidationMode::CanonicalStorage,
        )
        .valid
    );
    assert!(pulse::graph::contract::validate_node_contract(
        &node,
        ContractValidationMode::PublicCreate,
    )
    .valid);
    assert!(error_codes(&node, ContractValidationMode::Completeness)
        .contains(&"implementation_contract_missing".to_string()));
}

#[test]
fn completeness_reports_unknown_qa_impact_but_storage_modes_allow_it_for_implementation_ticket() {
    let mut node = assessed_implementation_ticket();
    node.qa = Some(qa(QaImpactPosture::Unknown));

    assert!(
        pulse::graph::contract::validate_node_contract(
            &node,
            ContractValidationMode::CanonicalStorage,
        )
        .valid
    );
    assert!(pulse::graph::contract::validate_node_contract(
        &node,
        ContractValidationMode::PublicCreate,
    )
    .valid);

    let completeness = error_codes(&node, ContractValidationMode::Completeness);
    assert!(completeness.contains(&"qa_impact_unknown".to_string()));
    assert!(!completeness.contains(&"qa_impact_invalid".to_string()));
}

#[test]
fn completeness_reports_unknown_qa_impact_but_storage_modes_allow_it_for_decision_work_ticket() {
    let mut node = ticket("TK-002");
    node.role = Some(TicketRole::DecisionWork);
    node.risk = Some(Risk::Medium);
    node.materialization = Some(Materialization::R2);
    node.qa = Some(qa(QaImpactPosture::Unknown));
    node.decision_work = Some(decision_work());

    assert!(
        pulse::graph::contract::validate_node_contract(
            &node,
            ContractValidationMode::CanonicalStorage,
        )
        .valid
    );
    assert!(pulse::graph::contract::validate_node_contract(
        &node,
        ContractValidationMode::PublicCreate,
    )
    .valid);

    let completeness = error_codes(&node, ContractValidationMode::Completeness);
    assert_eq!(
        completeness
            .iter()
            .filter(|code| code.as_str() == "qa_impact_unknown")
            .count(),
        1
    );
    assert!(!completeness.contains(&"decision_work_contract_missing".to_string()));
}

#[test]
fn qa_case_ids_are_portable_bounded_and_unique() {
    let mut valid = assessed_implementation_ticket();
    valid.qa.as_mut().unwrap().impact.affected_case_ids =
        vec!["QA-AUTH-001".to_string(), "A".repeat(64)];
    assert!(
        pulse::graph::contract::validate_node_contract(
            &valid,
            ContractValidationMode::CanonicalStorage,
        )
        .valid
    );

    for (case_id, expected_detail) in [
        ("A".repeat(65), "1-64 character"),
        ("qa-lowercase".to_string(), "portable uppercase"),
        ("QA-".to_string(), "portable uppercase"),
    ] {
        let mut invalid = assessed_implementation_ticket();
        invalid.qa.as_mut().unwrap().impact.affected_case_ids = vec![case_id];
        let report = pulse::graph::contract::validate_node_contract(
            &invalid,
            ContractValidationMode::CanonicalStorage,
        );
        assert!(!report.valid);
        assert!(report.errors.iter().any(|finding| {
            finding.code == "qa_impact_invalid" && finding.message.contains(expected_detail)
        }));
    }

    let mut duplicate = assessed_implementation_ticket();
    duplicate.qa.as_mut().unwrap().impact.affected_case_ids =
        vec!["QA-AUTH-DUP".to_string(), "QA-AUTH-DUP".to_string()];
    let report = pulse::graph::contract::validate_node_contract(
        &duplicate,
        ContractValidationMode::CanonicalStorage,
    );
    assert!(report.errors.iter().any(|finding| {
        finding.code == "qa_impact_invalid" && finding.message.contains("duplicate case id")
    }));
}

#[test]
fn decision_work_provenance_requires_shaping_receipt_and_optional_fog() {
    let mut node = ticket("TK-002");
    node.role = Some(TicketRole::DecisionWork);
    node.risk = Some(Risk::Medium);
    node.materialization = Some(Materialization::R2);
    node.qa = Some(qa(QaImpactPosture::Unknown));
    let mut work = decision_work();
    work.provenance.shaping_receipt.clear();
    node.decision_work = Some(work);

    assert!(error_codes(&node, ContractValidationMode::CanonicalStorage)
        .contains(&"shaping_receipt_missing".to_string()));

    let mut fogged = decision_work();
    fogged.provenance.fog_id = Some("FOG-AUTH-TELEMETRY".to_string());
    node.decision_work = Some(fogged);
    assert!(
        pulse::graph::contract::validate_node_contract(
            &node,
            ContractValidationMode::CanonicalStorage,
        )
        .valid
    );
}

#[test]
fn decision_acceptance_receipt_refs_use_decision_specific_error_codes() {
    let mut node = assessed_implementation_ticket();
    node.implementation.as_mut().unwrap().required_decisions =
        vec![pulse::graph::contract::RequiredDecisionRef {
            id: "DEC-006".to_string(),
            contract_revision: 1,
            acceptance_receipt: ReceiptRef {
                id: "not-a-receipt".to_string(),
                hash: "sha256:not-a-real-hash".to_string(),
            },
        }];
    let codes = error_codes(&node, ContractValidationMode::CanonicalStorage);
    assert!(codes.contains(&"decision_acceptance_missing".to_string()));
    assert!(codes.contains(&"decision_acceptance_stale".to_string()));
    assert!(!codes.contains(&"shaping_receipt_missing".to_string()));
    assert!(!codes.contains(&"shaping_receipt_hash_mismatch".to_string()));
}

#[test]
fn every_contract_finding_has_stable_return_code_mapping() {
    for code in [
        "contract_revision_invalid",
        "work_role_invalid",
        "work_classification_missing",
        "work_classification_not_allowed",
        "risk_materialization_unassessed",
        "implementation_contract_missing",
        "implementation_mode_missing",
        "implementation_surface_missing",
        "implementation_plan_policy_missing",
        "implementation_anchor_missing",
        "implementation_invariant_missing",
        "implementation_acceptance_missing",
        "implementation_freedom_missing",
        "implementation_brief_missing",
        "implementation_brief_hash_stale",
        "implementation_contract_invalid",
        "required_decision_missing",
        "required_decision_revision_stale",
        "decision_acceptance_missing",
        "decision_acceptance_stale",
        "decision_work_contract_missing",
        "decision_work_destination_invalid",
        "decision_work_branch_missing",
        "decision_work_question_invalid",
        "qa_impact_unknown",
        "qa_impact_invalid",
        "shaping_receipt_missing",
        "shaping_receipt_hash_mismatch",
        "shaping_map_path_unsafe",
        "shaping_map_revision_stale",
        "shaping_map_content_stale",
    ] {
        assert_eq!(pulse::graph::contract::stable_contract_code(code), code);
    }
}
