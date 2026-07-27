use chrono::{TimeZone, Utc};
use pulse::graph::contract::{
    ContentRef, ContractItem, ExpectedEvidence, ExpectedHandoff, ImplementationContract,
    ImplementationMode, ImplementationSemanticImpact, Materialization, PlanPolicy, QaImpact,
    QaImpactPosture, QaMetadata, Risk, SurfaceRef, WorkSurface,
};
use pulse::graph::node::Node;
use pulse::graph::validate::validate_node_schema_semantics;
use pulse::id::WorkKind;
use serde_json::{json, Value};

const HASH: &str = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const TICKET_ONLY_FIELDS: &[&str] = &[
    "role",
    "risk",
    "materialization",
    "qa",
    "implementation",
    "decision_work",
];

fn schema() -> Value {
    serde_json::from_str(include_str!("../../src/schema/node.schema.json")).unwrap()
}

fn valid_ticket_value() -> Value {
    let now = Utc.timestamp_opt(1, 0).unwrap();
    let mut node = Node::new(
        "TK-001".to_string(),
        WorkKind::Ticket,
        "Ticket".to_string(),
        now,
    )
    .unwrap();
    node.risk = Some(Risk::Low);
    node.materialization = Some(Materialization::R0);
    node.implementation = Some(ImplementationContract {
        mode: ImplementationMode::Open,
        work_surface: WorkSurface::Code,
        plan_policy: PlanPolicy::None,
        semantic_impact: ImplementationSemanticImpact::BehaviorOrPublicRiskChange,
        effort: Default::default(),
        verification_profile: "service-change".to_string(),
        brief: Some(ContentRef {
            path: "works/TK-001/ticket.md".to_string(),
            content_hash: HASH.to_string(),
        }),
        objective: "Objective".to_string(),
        current_behavior: "Current".to_string(),
        target_behavior: "Target".to_string(),
        code_anchors: vec![SurfaceRef::path("src/lib.rs")],
        documentation_anchors: vec![],
        configuration_anchors: vec![],
        data_anchors: vec![],
        research_refs: vec![],
        required_changes: vec![],
        invariants: vec![],
        acceptance: vec![ContractItem {
            id: "AC-OK".to_string(),
            summary: "Acceptance".to_string(),
        }],
        scope: Default::default(),
        implementation_freedom: vec![],
        required_decisions: vec![],
        shared_approach_refs: vec![],
        expected_evidence: vec![ExpectedEvidence::FocusedTestOutput],
        expected_handoff: vec![ExpectedHandoff::AcceptanceToEvidence],
    });
    node.qa = Some(QaMetadata {
        impact: QaImpact {
            posture: QaImpactPosture::Required,
            rationale: Some("Behavior changes require a case.".to_string()),
            behavioral_owner: Some("ST-001".to_string()),
            affected_case_ids: vec!["QA-AUTH".to_string()],
        },
    });
    serde_json::to_value(node).unwrap()
}

fn story_value() -> Value {
    serde_json::to_value(
        Node::new(
            "ST-001".to_string(),
            WorkKind::Story,
            "Story".to_string(),
            Utc.timestamp_opt(1, 0).unwrap(),
        )
        .unwrap(),
    )
    .unwrap()
}

fn decision_value() -> Value {
    serde_json::to_value(
        Node::new(
            "DEC-001".to_string(),
            WorkKind::Decision,
            "Decision".to_string(),
            Utc.timestamp_opt(1, 0).unwrap(),
        )
        .unwrap(),
    )
    .unwrap()
}

fn valid_decision_work_value() -> Value {
    json!({
        "destination_owner": {
            "id": "ST-001",
            "contract_revision": 1
        },
        "branch_id": "BR-TOKEN-COMPAT",
        "gap_kind": "tradeoff_gap",
        "question": "Which token compatibility branch should apply?",
        "expected_output": "A recorded decision.",
        "expected_evidence": ["client_contract_inventory"],
        "resolution_target": {
            "kind": "decision",
            "id": "DEC-006"
        },
        "provenance": {
            "shaping_receipt": "rcpt_01JTEST"
        }
    })
}

fn decision_work_ticket_value() -> Value {
    let mut value = serde_json::to_value(
        Node::new(
            "TK-002".to_string(),
            WorkKind::Ticket,
            "Decision work".to_string(),
            Utc.timestamp_opt(1, 0).unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    value["role"] = json!("decision_work");
    value["risk"] = json!("medium");
    value["materialization"] = json!("R2");
    value["qa"] = json!({"impact": {"posture": "unknown"}});
    value["decision_work"] = valid_decision_work_value();
    value
}

fn non_null_value_for(field: &str) -> Value {
    match field {
        "role" => json!("implementation"),
        "risk" => json!("low"),
        "materialization" => json!("R0"),
        "qa" => json!({"impact": {"posture": "unknown"}}),
        "implementation" => valid_ticket_value()["implementation"].clone(),
        "decision_work" => valid_decision_work_value(),
        _ => panic!("unexpected field {field}"),
    }
}

fn schema_contract_constraints_accept(schema: &Value, instance: &Value) -> bool {
    schema["allOf"]
        .as_array()
        .unwrap()
        .iter()
        .all(|rule| eval_schema_fragment(rule, instance))
}

fn schema_fragment_rejects(schema: &Value, instance: &Value) -> bool {
    !eval_schema_fragment(schema, instance)
}

fn eval_schema_fragment(schema: &Value, instance: &Value) -> bool {
    if let Some(if_schema) = schema.get("if") {
        if eval_schema_fragment(if_schema, instance) {
            return schema
                .get("then")
                .map(|then_schema| eval_schema_fragment(then_schema, instance))
                .unwrap_or(true);
        }
        return schema
            .get("else")
            .map(|else_schema| eval_schema_fragment(else_schema, instance))
            .unwrap_or(true);
    }

    if let Some(all_of) = schema.get("allOf").and_then(Value::as_array) {
        if !all_of
            .iter()
            .all(|rule| eval_schema_fragment(rule, instance))
        {
            return false;
        }
    }
    if let Some(any_of) = schema.get("anyOf").and_then(Value::as_array) {
        if !any_of
            .iter()
            .any(|rule| eval_schema_fragment(rule, instance))
        {
            return false;
        }
    }
    if let Some(not_schema) = schema.get("not") {
        if eval_schema_fragment(not_schema, instance) {
            return false;
        }
    }
    if let Some(required) = schema.get("required").and_then(Value::as_array) {
        let Some(object) = instance.as_object() else {
            return false;
        };
        if !required
            .iter()
            .filter_map(Value::as_str)
            .all(|field| object.contains_key(field))
        {
            return false;
        }
    }
    if let Some(properties) = schema.get("properties").and_then(Value::as_object) {
        let Some(object) = instance.as_object() else {
            return false;
        };
        for (field, property_schema) in properties {
            if let Some(property_value) = object.get(field) {
                if !eval_schema_fragment(property_schema, property_value) {
                    return false;
                }
            }
        }
    }
    if let Some(const_value) = schema.get("const") {
        if instance != const_value {
            return false;
        }
    }
    if let Some(enum_values) = schema.get("enum").and_then(Value::as_array) {
        if !enum_values.iter().any(|candidate| candidate == instance) {
            return false;
        }
    }
    if let Some(type_schema) = schema.get("type") {
        if !matches_json_type(instance, type_schema) {
            return false;
        }
    }
    if let Some(max_length) = schema.get("maxLength").and_then(Value::as_u64) {
        let Some(value) = instance.as_str() else {
            return false;
        };
        if value.len() as u64 > max_length {
            return false;
        }
    }
    if let Some(pattern) = schema.get("pattern").and_then(Value::as_str) {
        let Some(value) = instance.as_str() else {
            return false;
        };
        if !matches_test_pattern(pattern, value) {
            return false;
        }
    }
    true
}

fn matches_test_pattern(pattern: &str, value: &str) -> bool {
    match pattern {
        "^[A-Z0-9](?:[A-Z0-9-]{0,62}[A-Z0-9])?$" => {
            !value.is_empty()
                && value.len() <= 64
                && value
                    .chars()
                    .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '-')
                && value
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
                && value
                    .chars()
                    .last()
                    .is_some_and(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
        }
        "^BR-[A-Z0-9](?:[A-Z0-9-]{0,59}[A-Z0-9])?$" => {
            value.strip_prefix("BR-").is_some_and(|suffix| {
                !suffix.is_empty()
                    && value.len() <= 64
                    && suffix
                        .chars()
                        .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '-')
                    && suffix
                        .chars()
                        .next()
                        .is_some_and(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
                    && suffix
                        .chars()
                        .last()
                        .is_some_and(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
            })
        }
        other => panic!("unsupported test schema pattern {other}"),
    }
}

fn matches_json_type(instance: &Value, type_schema: &Value) -> bool {
    if let Some(types) = type_schema.as_array() {
        return types.iter().any(|ty| matches_json_type(instance, ty));
    }
    match type_schema.as_str().unwrap() {
        "null" => instance.is_null(),
        "string" => instance.is_string(),
        "object" => instance.is_object(),
        "array" => instance.is_array(),
        "integer" => instance.as_i64().is_some() || instance.as_u64().is_some(),
        "boolean" => instance.is_boolean(),
        other => panic!("unsupported test schema type {other}"),
    }
}

fn assert_deserializes_and_contract_validates(value: Value) {
    let node: Node = serde_json::from_value(value).unwrap();
    validate_node_schema_semantics(&node).unwrap();
}

fn assert_deserializes_and_contract_rejects(value: Value, code: &str) {
    let node: Node = serde_json::from_value(value).unwrap();
    assert_eq!(
        validate_node_schema_semantics(&node).unwrap_err().code(),
        code
    );
}

#[test]
fn node_schema_exposes_every_serialized_node_field() {
    let schema = schema();
    let properties = schema["properties"].as_object().unwrap();
    let node = valid_ticket_value();
    let object = node.as_object().unwrap();
    for field in object.keys() {
        assert!(
            properties.contains_key(field),
            "serialized field {field} missing from node.schema.json"
        );
    }
    assert_eq!(schema["properties"]["schema_version"]["const"], 1);
    assert_eq!(schema["properties"]["contract_revision"]["minimum"], 1);
    assert_eq!(schema["additionalProperties"], false);
}

#[test]
fn node_schema_declares_ticket_only_fields_and_contract_exclusivity() {
    let schema = schema();
    let all_of = schema["allOf"].as_array().unwrap();
    let ticket_rule = &all_of[0];
    let serialized = serde_json::to_string(ticket_rule).unwrap();
    assert!(serialized.contains("role"));
    assert!(serialized.contains("risk"));
    assert!(serialized.contains("materialization"));
    assert!(serialized.contains("implementation"));
    assert!(serialized.contains("decision_work"));
    assert!(
        serialized.contains("not"),
        "schema must reject mismatched/both contract shapes"
    );
    assert!(
        serde_json::to_string(&schema["properties"]["risk"])
            .unwrap()
            .contains("unassessed")
            && serde_json::to_string(&schema["properties"]["materialization"])
                .unwrap()
                .contains("unassessed"),
        "schema must preserve canonical-storage unassessed values"
    );
    assert!(
        serialized.contains("\"type\":\"null\""),
        "schema constraints must distinguish explicit null from non-null presence"
    );
}

#[test]
fn node_json_deserialization_rejects_unknown_contract_fields() {
    let mut value = valid_ticket_value();
    value["unexpected_future_field"] = serde_json::json!(true);
    let err = serde_json::from_value::<Node>(value).unwrap_err();
    assert!(err.to_string().contains("unknown field"));
}

fn assert_ref_array_max_items(schema: &Value, pointer: &str) {
    let value = schema
        .pointer(pointer)
        .unwrap_or_else(|| panic!("missing schema pointer {pointer}"));
    assert_eq!(
        value["maxItems"], 64,
        "array at {pointer} must mirror Rust MAX_COLLECTION"
    );
}

#[test]
fn node_schema_marks_serde_options_nullable_without_requiring_canonical_nulls() {
    let schema = schema();
    for pointer in [
        "/properties/status_reason",
        "/properties/documentation",
        "/properties/role",
        "/properties/risk",
        "/properties/materialization",
        "/properties/qa",
        "/properties/implementation",
        "/properties/decision_work",
        "/properties/shaping",
        "/$defs/implementation_contract/properties/brief",
        "/$defs/decision_work_contract/properties/resolution_target",
        "/$defs/decision_work_contract/properties/provenance/properties/fog_id",
        "/$defs/shaping_pointer/properties/map",
    ] {
        let value = schema
            .pointer(pointer)
            .unwrap_or_else(|| panic!("missing {pointer}"));
        let serialized = serde_json::to_string(value).unwrap();
        assert!(
            serialized.contains("null"),
            "schema pointer {pointer} should permit proposal-compatible null for serde Option"
        );
    }

    let node = valid_ticket_value();
    let object = node.as_object().unwrap();
    assert!(
        !object.contains_key("status_reason"),
        "canonical serialization should still omit absent Option fields"
    );
}

#[test]
fn node_schema_bounds_rust_max_collection_arrays() {
    let schema = schema();
    for pointer in [
        "/$defs/decision_work_contract/properties/expected_evidence",
        "/$defs/implementation_contract/properties/acceptance",
        "/$defs/implementation_contract/properties/code_anchors",
        "/$defs/implementation_contract/properties/configuration_anchors",
        "/$defs/implementation_contract/properties/data_anchors",
        "/$defs/implementation_contract/properties/documentation_anchors",
        "/$defs/implementation_contract/properties/expected_evidence",
        "/$defs/implementation_contract/properties/expected_handoff",
        "/$defs/implementation_contract/properties/implementation_freedom",
        "/$defs/implementation_contract/properties/invariants",
        "/$defs/implementation_contract/properties/required_changes",
        "/$defs/implementation_contract/properties/required_decisions",
        "/$defs/implementation_contract/properties/research_refs",
        "/$defs/implementation_contract/properties/shared_approach_refs",
        "/$defs/implementation_contract/properties/scope/properties/included",
        "/$defs/implementation_contract/properties/scope/properties/excluded",
        "/$defs/qa_impact/properties/affected_case_ids",
        "/properties/documentation/properties/impact/properties/deferred_to",
        "/properties/documentation/properties/impact/properties/required_documents",
        "/properties/documentation/properties/routing/properties/domains",
        "/properties/documentation/properties/routing/properties/labels",
        "/properties/documentation/properties/routing/properties/paths",
    ] {
        assert_ref_array_max_items(&schema, pointer);
    }
}

#[test]
fn node_schema_keeps_assessed_missing_contract_canonical_but_completeness_owned() {
    let schema = schema();
    let all_of = serde_json::to_string(schema["allOf"].as_array().unwrap()).unwrap();
    assert!(
        !all_of.contains("then\":{\"required\":[\"implementation\"]")
            && !all_of.contains("then\":{\"required\":[\"decision_work\"]"),
        "schema must not make missing role contracts canonical corruption"
    );

    let mut node = Node::new(
        "TK-001".to_string(),
        WorkKind::Ticket,
        "Ticket".to_string(),
        Utc.timestamp_opt(1, 0).unwrap(),
    )
    .unwrap();
    node.risk = Some(Risk::Low);
    node.materialization = Some(Materialization::R0);
    let value = serde_json::to_value(&node).unwrap();
    assert_eq!(value["risk"], "low");
    assert_eq!(value["materialization"], "R0");
    assert!(value.get("implementation").is_none());
}

#[test]
fn node_schema_requires_decision_work_shaping_receipt_but_not_fog() {
    let schema = schema();
    let provenance = schema
        .pointer("/$defs/decision_work_contract/properties/provenance")
        .unwrap();
    assert_eq!(
        provenance["required"],
        serde_json::json!(["shaping_receipt"])
    );
    assert_ne!(
        provenance["required"],
        serde_json::json!(["shaping_receipt", "fog_id"])
    );
}

#[test]
fn node_schema_decision_work_branch_id_profile_matches_rust_bounds() {
    let schema = schema();
    let branch_id = schema
        .pointer("/$defs/decision_work_contract/properties/branch_id")
        .unwrap();
    assert_eq!(branch_id["maxLength"], 64);
    assert_eq!(
        branch_id["pattern"],
        "^BR-[A-Z0-9](?:[A-Z0-9-]{0,59}[A-Z0-9])?$"
    );

    let mut valid_minimal = decision_work_ticket_value();
    valid_minimal["decision_work"]["branch_id"] = json!("BR-A");
    assert!(!schema_fragment_rejects(
        branch_id,
        &valid_minimal["decision_work"]["branch_id"]
    ));
    assert_deserializes_and_contract_validates(valid_minimal);

    let mut leading_suffix_hyphen = decision_work_ticket_value();
    leading_suffix_hyphen["decision_work"]["branch_id"] = json!("BR--BAD");
    assert!(schema_fragment_rejects(
        branch_id,
        &leading_suffix_hyphen["decision_work"]["branch_id"]
    ));
    assert_deserializes_and_contract_rejects(leading_suffix_hyphen, "decision_work_branch_missing");

    let mut trailing_hyphen = decision_work_ticket_value();
    trailing_hyphen["decision_work"]["branch_id"] = json!("BR-BAD-");
    assert!(schema_fragment_rejects(
        branch_id,
        &trailing_hyphen["decision_work"]["branch_id"]
    ));
    assert_deserializes_and_contract_rejects(trailing_hyphen, "decision_work_branch_missing");

    let mut max_length = decision_work_ticket_value();
    max_length["decision_work"]["branch_id"] = json!(format!("BR-{}", "A".repeat(61)));
    assert!(!schema_fragment_rejects(
        branch_id,
        &max_length["decision_work"]["branch_id"]
    ));
    assert_deserializes_and_contract_validates(max_length);

    let mut too_long = decision_work_ticket_value();
    too_long["decision_work"]["branch_id"] = json!(format!("BR-{}", "A".repeat(62)));
    assert!(schema_fragment_rejects(
        branch_id,
        &too_long["decision_work"]["branch_id"]
    ));
    assert_deserializes_and_contract_rejects(too_long, "decision_work_branch_missing");
}

#[test]
fn node_json_deserialization_accepts_null_for_option_fields_and_omits_on_serialize() {
    let mut value = valid_ticket_value();
    value["status_reason"] = serde_json::Value::Null;
    value["documentation"] = serde_json::Value::Null;
    value["implementation"]["brief"] = serde_json::Value::Null;
    value["implementation"]["code_anchors"][0]["symbol"] = serde_json::Value::Null;
    value["implementation"]["code_anchors"][0]["content_hash"] = serde_json::Value::Null;
    value["qa"]["impact"]["rationale"] = serde_json::Value::Null;

    let node: Node = serde_json::from_value(value).unwrap();
    assert!(node.status_reason.is_none());
    assert!(node.documentation.is_none());
    assert!(node.implementation.as_ref().unwrap().brief.is_none());

    let serialized = serde_json::to_value(&node).unwrap();
    assert!(serialized.get("status_reason").is_none());
    assert!(serialized.get("documentation").is_none());
    assert!(serialized["implementation"].get("brief").is_none());
}

#[test]
fn node_schema_ticket_only_allof_allows_null_but_rejects_non_null_on_non_tickets() {
    let schema = schema();
    for field in TICKET_ONLY_FIELDS {
        let mut null_story = story_value();
        null_story[*field] = Value::Null;
        assert!(
            schema_contract_constraints_accept(&schema, &null_story),
            "{field}: null should be equivalent to omitted for Ticket-only schema constraints"
        );
        assert_deserializes_and_contract_validates(null_story);

        let mut non_null_story = story_value();
        non_null_story[*field] = non_null_value_for(field);
        assert!(
            !schema_contract_constraints_accept(&schema, &non_null_story),
            "{field}: non-null Ticket-only field must be schema-rejected on non-Ticket nodes"
        );
        assert_deserializes_and_contract_rejects(non_null_story, "work_role_invalid");
    }
}

#[test]
fn node_schema_role_contract_allof_allows_null_but_rejects_non_null_wrong_contract() {
    let schema = schema();

    let mut implementation_with_null_decision_work = valid_ticket_value();
    implementation_with_null_decision_work["decision_work"] = Value::Null;
    assert!(schema_contract_constraints_accept(
        &schema,
        &implementation_with_null_decision_work
    ));
    assert_deserializes_and_contract_validates(implementation_with_null_decision_work);

    let mut implementation_with_non_null_decision_work = valid_ticket_value();
    implementation_with_non_null_decision_work["decision_work"] = valid_decision_work_value();
    assert!(!schema_contract_constraints_accept(
        &schema,
        &implementation_with_non_null_decision_work
    ));
    assert_deserializes_and_contract_rejects(
        implementation_with_non_null_decision_work,
        "work_role_invalid",
    );

    let mut decision_work_with_null_implementation = decision_work_ticket_value();
    decision_work_with_null_implementation["implementation"] = Value::Null;
    assert!(schema_contract_constraints_accept(
        &schema,
        &decision_work_with_null_implementation
    ));
    assert_deserializes_and_contract_validates(decision_work_with_null_implementation);

    let mut decision_work_with_non_null_implementation = decision_work_ticket_value();
    decision_work_with_non_null_implementation["implementation"] =
        valid_ticket_value()["implementation"].clone();
    assert!(!schema_contract_constraints_accept(
        &schema,
        &decision_work_with_non_null_implementation
    ));
    assert_deserializes_and_contract_rejects(
        decision_work_with_non_null_implementation,
        "work_role_invalid",
    );

    let mut implementation_with_both_null = valid_ticket_value();
    implementation_with_both_null["implementation"] = Value::Null;
    implementation_with_both_null["decision_work"] = Value::Null;
    assert!(schema_contract_constraints_accept(
        &schema,
        &implementation_with_both_null
    ));
    let node: Node = serde_json::from_value(implementation_with_both_null).unwrap();
    assert!(node.implementation.is_none());
    assert!(node.decision_work.is_none());
}

#[test]
fn node_schema_decision_shaping_constraint_allows_null_but_rejects_non_null() {
    let schema = schema();
    let mut null_shaping = decision_value();
    null_shaping["shaping"] = Value::Null;
    assert!(schema_contract_constraints_accept(&schema, &null_shaping));
    assert_deserializes_and_contract_validates(null_shaping);

    let mut non_null_shaping = decision_value();
    non_null_shaping["shaping"] = json!({
        "receipt": {
            "id": "rcpt_01JTEST",
            "hash": HASH
        },
        "applied_at": "1970-01-01T00:00:01Z",
        "applied_by": "human:test"
    });
    assert!(!schema_contract_constraints_accept(
        &schema,
        &non_null_shaping
    ));
    assert_deserializes_and_contract_rejects(non_null_shaping, "work_role_invalid");
}

#[test]
fn node_schema_case_id_profile_matches_rust_bounds() {
    let schema = schema();
    let case_id = schema.pointer("/$defs/case_id").unwrap();
    assert_eq!(case_id["maxLength"], 64);
    assert_eq!(case_id["pattern"], "^[A-Z0-9](?:[A-Z0-9-]{0,62}[A-Z0-9])?$");

    let mut too_long = valid_ticket_value();
    too_long["qa"]["impact"]["affected_case_ids"] = json!(["A".repeat(65)]);
    let node: Node = serde_json::from_value(too_long).unwrap();
    assert_eq!(
        validate_node_schema_semantics(&node).unwrap_err().code(),
        "qa_impact_invalid"
    );

    let mut duplicate = valid_ticket_value();
    duplicate["qa"]["impact"]["affected_case_ids"] = json!(["QA-AUTH", "QA-AUTH"]);
    let node: Node = serde_json::from_value(duplicate).unwrap();
    assert_eq!(
        validate_node_schema_semantics(&node).unwrap_err().code(),
        "qa_impact_invalid"
    );
}
