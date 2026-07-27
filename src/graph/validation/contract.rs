use std::collections::BTreeSet;
use std::path::Path;

use crate::graph::model::contract::*;
use crate::graph::model::contract::{
    stable_code, ContractValidationMode, ContractValidationReport, MAX_COLLECTION, MAX_ID,
    MAX_LONG_TEXT, MAX_SHORT_TEXT,
};
use crate::graph::model::node::Node;
use crate::id::{kind_for_id, validate_work_id, WorkKind};
use crate::{PulseError, PulseResult};

pub fn validate_node_contract(
    node: &Node,
    mode: ContractValidationMode,
) -> ContractValidationReport {
    let mut report = ContractValidationReport::ok();

    if node.contract_revision < 1 {
        report.push(
            "contract_revision_invalid",
            "contract_revision must be >= 1 and distinct from normal revision",
        );
    }

    match node.kind {
        WorkKind::Ticket => validate_ticket_contract(node, mode, &mut report),
        WorkKind::Epic | WorkKind::Story => validate_non_ticket_contract_fields(node, &mut report),
        WorkKind::Decision => {
            validate_non_ticket_contract_fields(node, &mut report);
            if node.shaping.is_some() {
                report.push(
                    "work_role_invalid",
                    "current shaping pointer is only allowed on Epic, Story, or Ticket nodes",
                );
            }
        }
    }

    if let Some(shaping) = &node.shaping {
        validate_shaping_pointer(node, shaping, &mut report);
    }

    report
}

pub fn validate_node_contract_result(node: &Node, mode: ContractValidationMode) -> PulseResult<()> {
    validate_node_contract(node, mode).into_result()
}

pub fn stable_contract_code(code: &str) -> &'static str {
    stable_code(code)
}

pub fn validate_public_create_classification(
    kind: WorkKind,
    classification: &PublicCreateClassification,
) -> PulseResult<()> {
    if kind != WorkKind::Ticket {
        if classification.any_present() {
            return Err(PulseError::validation(
                "work_classification_not_allowed",
                "role, risk, and materialization classification flags are only valid for Ticket creation",
            ));
        }
        return Ok(());
    }

    match (classification.role, classification.risk, classification.materialization) {
        (Some(_), Some(risk), Some(materialization))
            if risk.is_assessed() && materialization.is_assessed() => Ok(()),
        (Some(_), Some(_), Some(_)) => Err(PulseError::validation(
            "risk_materialization_unassessed",
            "public Ticket creation requires assessed risk and materialization; unassessed is only valid for canonical draft storage",
        )),
        _ => Err(PulseError::validation(
            "work_classification_missing",
            "public Ticket creation requires explicit --role, --risk, and --materialization",
        )),
    }
}

fn validate_non_ticket_contract_fields(node: &Node, report: &mut ContractValidationReport) {
    if node.role.is_some()
        || node.risk.is_some()
        || node.materialization.is_some()
        || node.qa.is_some()
        || node.implementation.is_some()
        || node.decision_work.is_some()
    {
        report.push(
            "work_role_invalid",
            "role, risk, materialization, QA, implementation, and decision_work are Ticket-only fields",
        );
    }
}

fn validate_ticket_contract(
    node: &Node,
    mode: ContractValidationMode,
    report: &mut ContractValidationReport,
) {
    let Some(role) = node.role else {
        report.push("work_role_invalid", "Ticket nodes must declare a role");
        return;
    };
    let Some(risk) = node.risk else {
        report.push(
            "work_classification_missing",
            "Ticket nodes must declare risk",
        );
        return;
    };
    let Some(materialization) = node.materialization else {
        report.push(
            "work_classification_missing",
            "Ticket nodes must declare materialization",
        );
        return;
    };
    let Some(qa) = &node.qa else {
        report.push(
            "qa_impact_unknown",
            "Ticket nodes must carry QA impact metadata",
        );
        return;
    };

    if mode == ContractValidationMode::PublicCreate
        && (!risk.is_assessed() || !materialization.is_assessed())
    {
        report.push(
            "risk_materialization_unassessed",
            "public Ticket creation requires assessed risk and materialization",
        );
    }

    if node.implementation.is_some() && node.decision_work.is_some() {
        report.push(
            "work_role_invalid",
            "implementation and decision_work contracts are mutually exclusive",
        );
    }

    let require_role_contract = mode == ContractValidationMode::Completeness;
    match role {
        TicketRole::Implementation => {
            if node.decision_work.is_some() {
                report.push(
                    "work_role_invalid",
                    "implementation role must not carry a decision_work contract",
                );
            }
            match &node.implementation {
                Some(implementation) => {
                    report.extend(validate_implementation_contract(
                        implementation,
                        materialization,
                        node,
                    ));
                    validate_qa_impact(qa, Some(implementation), mode, report);
                }
                None if require_role_contract => {
                    report.push(
                        "implementation_contract_missing",
                        "complete implementation Ticket requires an implementation contract",
                    );
                    validate_qa_impact(qa, None, mode, report);
                }
                None => validate_qa_impact(qa, None, mode, report),
            }
        }
        TicketRole::DecisionWork => {
            if node.implementation.is_some() {
                report.push(
                    "work_role_invalid",
                    "decision_work role must not carry an implementation contract",
                );
            }
            match &node.decision_work {
                Some(decision_work) => {
                    report.extend(validate_decision_work_contract(decision_work));
                    validate_qa_impact(qa, None, mode, report);
                }
                None if require_role_contract => {
                    report.push(
                        "decision_work_contract_missing",
                        "complete decision-work Ticket requires a decision_work contract",
                    );
                    validate_qa_impact(qa, None, mode, report);
                }
                None => validate_qa_impact(qa, None, mode, report),
            }
        }
    }
}

fn validate_implementation_contract(
    contract: &ImplementationContract,
    materialization: Materialization,
    node: &Node,
) -> ContractValidationReport {
    let mut report = ContractValidationReport::ok();
    validate_non_empty_bounded(
        &contract.objective,
        "implementation_contract_invalid",
        "implementation objective must be non-empty and bounded",
        MAX_LONG_TEXT,
        &mut report,
    );
    validate_non_empty_bounded(
        &contract.current_behavior,
        "implementation_contract_invalid",
        "current_behavior must be non-empty and bounded",
        MAX_LONG_TEXT,
        &mut report,
    );
    validate_non_empty_bounded(
        &contract.target_behavior,
        "implementation_contract_invalid",
        "target_behavior must be non-empty and bounded",
        MAX_LONG_TEXT,
        &mut report,
    );
    validate_slugish(
        &contract.verification_profile,
        "implementation_contract_invalid",
        "verification_profile must be a bounded profile identifier",
        &mut report,
    );

    match &contract.brief {
        Some(brief) => validate_content_ref(
            brief,
            Some(&node.content_dir),
            "implementation_brief_hash_stale",
            &mut report,
        ),
        None => report.push(
            "implementation_brief_missing",
            "implementation contract requires a content-bound brief reference",
        ),
    }

    if matches!(
        contract.mode,
        ImplementationMode::Guided | ImplementationMode::Locked
    ) {
        let anchors = anchors_for_surface(contract, contract.work_surface);
        if anchors.is_empty() {
            report.push(
                "implementation_anchor_missing",
                "guided or locked implementation requires at least one typed anchor/reference for its work surface",
            );
        }
    }

    validate_surface_refs("code_anchors", &contract.code_anchors, &mut report);
    validate_surface_refs(
        "documentation_anchors",
        &contract.documentation_anchors,
        &mut report,
    );
    validate_surface_refs(
        "configuration_anchors",
        &contract.configuration_anchors,
        &mut report,
    );
    validate_surface_refs("data_anchors", &contract.data_anchors, &mut report);
    validate_surface_refs("research_refs", &contract.research_refs, &mut report);

    validate_items(
        "required_changes",
        &contract.required_changes,
        false,
        "implementation_contract_invalid",
        &mut report,
    );
    validate_items(
        "invariants",
        &contract.invariants,
        materialization.requires_invariant(),
        "implementation_invariant_missing",
        &mut report,
    );
    validate_items(
        "acceptance",
        &contract.acceptance,
        true,
        "implementation_acceptance_missing",
        &mut report,
    );
    validate_items(
        "implementation_freedom",
        &contract.implementation_freedom,
        false,
        "implementation_freedom_missing",
        &mut report,
    );
    validate_unique_texts("scope.included", &contract.scope.included, &mut report);
    validate_unique_texts("scope.excluded", &contract.scope.excluded, &mut report);

    if contract.mode == ImplementationMode::Locked
        && contract.required_decisions.is_empty()
        && contract.shared_approach_refs.is_empty()
    {
        report.push(
            "required_decision_missing",
            "locked implementation requires at least one required Decision or shared approach reference",
        );
    }

    if contract.required_decisions.len() > MAX_COLLECTION {
        report.push(
            "required_decision_missing",
            "required_decisions exceeds the bounded collection limit",
        );
    }
    let mut decision_ids = BTreeSet::new();
    for decision in &contract.required_decisions {
        if !decision_ids.insert(&decision.id) {
            report.push(
                "required_decision_missing",
                format!("duplicate required Decision reference {}", decision.id),
            );
        }
        match kind_for_id(&decision.id) {
            Ok(WorkKind::Decision) => {}
            _ => report.push(
                "required_decision_missing",
                format!("required Decision id must use DEC prefix: {}", decision.id),
            ),
        }
        if decision.contract_revision < 1 {
            report.push(
                "required_decision_revision_stale",
                "required Decision contract_revision must be >= 1",
            );
        }
        validate_receipt_ref_with_codes(
            &decision.acceptance_receipt,
            "decision_acceptance_missing",
            "decision_acceptance_stale",
            &mut report,
        );
    }

    if contract.shared_approach_refs.len() > MAX_COLLECTION {
        report.push(
            "implementation_contract_invalid",
            "shared_approach_refs exceeds the bounded collection limit",
        );
    }
    let mut approach_keys = BTreeSet::new();
    for approach in &contract.shared_approach_refs {
        if !approach_keys.insert((&approach.owner.id, &approach.path)) {
            report.push(
                "implementation_contract_invalid",
                format!("duplicate shared approach reference {}", approach.path),
            );
        }
        validate_work_ref(&approach.owner, None, &mut report);
        validate_path(
            &approach.path,
            None,
            "implementation_contract_invalid",
            &mut report,
        );
        validate_hash(
            &approach.content_hash,
            "implementation_brief_hash_stale",
            "shared approach content_hash must be sha256:<hex>",
            &mut report,
        );
    }

    validate_unique_enums(
        "expected_evidence",
        &contract.expected_evidence,
        "implementation_contract_invalid",
        &mut report,
    );
    validate_unique_enums(
        "expected_handoff",
        &contract.expected_handoff,
        "implementation_contract_invalid",
        &mut report,
    );

    report
}

fn anchors_for_surface(contract: &ImplementationContract, surface: WorkSurface) -> &[SurfaceRef] {
    match surface {
        WorkSurface::Code => &contract.code_anchors,
        WorkSurface::Documentation => &contract.documentation_anchors,
        WorkSurface::Configuration => &contract.configuration_anchors,
        WorkSurface::Data => &contract.data_anchors,
        WorkSurface::Research => &contract.research_refs,
    }
}

fn validate_decision_work_contract(contract: &DecisionWorkContract) -> ContractValidationReport {
    let mut report = ContractValidationReport::ok();
    validate_destination_owner(&contract.destination_owner, &mut report);
    if !is_portable_branch_id(&contract.branch_id) {
        report.push(
            "decision_work_branch_missing",
            "decision-work branch_id must be a 1-64 character portable uppercase branch identifier starting with BR- and ending with an uppercase letter or digit",
        );
    }
    validate_non_empty_bounded(
        &contract.question,
        "decision_work_question_invalid",
        "decision-work question must be precise and non-empty",
        MAX_LONG_TEXT,
        &mut report,
    );
    validate_non_empty_bounded(
        &contract.expected_output,
        "decision_work_question_invalid",
        "decision-work expected_output must be non-empty",
        MAX_LONG_TEXT,
        &mut report,
    );
    validate_unique_enums(
        "expected_evidence",
        &contract.expected_evidence,
        "decision_work_question_invalid",
        &mut report,
    );
    if let Some(target) = &contract.resolution_target {
        match target.kind {
            ResolutionTargetKind::Decision => match kind_for_id(&target.id) {
                Ok(WorkKind::Decision) => {}
                _ => report.push(
                    "decision_work_question_invalid",
                    "decision resolution_target must use a DEC id",
                ),
            },
            ResolutionTargetKind::Work => {
                if let Err(error) = validate_work_id(&target.id) {
                    report.push("decision_work_question_invalid", error.to_string());
                }
            }
            ResolutionTargetKind::Evidence => validate_non_empty_bounded(
                &target.id,
                "decision_work_question_invalid",
                "evidence resolution_target id must be non-empty",
                MAX_ID,
                &mut report,
            ),
        }
    }
    validate_receipt_id_with_code(
        &contract.provenance.shaping_receipt,
        "shaping_receipt_missing",
        &mut report,
    );
    if let Some(fog_id) = &contract.provenance.fog_id {
        validate_stable_id(
            "decision_work_question_invalid",
            fog_id,
            "fog provenance id must be a stable identifier",
            &mut report,
        );
    }
    report
}

fn validate_qa_impact(
    qa: &QaMetadata,
    implementation: Option<&ImplementationContract>,
    mode: ContractValidationMode,
    report: &mut ContractValidationReport,
) {
    let rationale_present = qa
        .impact
        .rationale
        .as_deref()
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false);
    validate_case_ids(&qa.impact.affected_case_ids, report);
    match qa.impact.posture {
        QaImpactPosture::Unknown => {
            if mode == ContractValidationMode::Completeness {
                report.push(
                    "qa_impact_unknown",
                    "complete Ticket requires assessed QA impact metadata",
                );
            }
            if rationale_present
                || qa.impact.behavioral_owner.is_some()
                || !qa.impact.affected_case_ids.is_empty()
            {
                report.push(
                    "qa_impact_invalid",
                    "unknown QA impact must not carry rationale, owner, or case ids",
                );
            }
        }
        QaImpactPosture::None => {
            if !rationale_present {
                report.push("qa_impact_invalid", "qa=none requires a rationale");
            }
            if let Some(contract) = implementation {
                match contract.semantic_impact {
                    ImplementationSemanticImpact::NoBehaviorOrPublicRiskChange => {}
                    ImplementationSemanticImpact::BehaviorOrPublicRiskChange => report.push(
                        "qa_impact_invalid",
                        "qa=none requires semantic_impact=no_behavior_or_public_risk_change",
                    ),
                }
            }
            if qa.impact.behavioral_owner.is_some() || !qa.impact.affected_case_ids.is_empty() {
                report.push(
                    "qa_impact_invalid",
                    "qa=none must not carry behavioral owner or case ids",
                );
            }
        }
        QaImpactPosture::CoveredByStoryClose => {
            if !rationale_present {
                report.push(
                    "qa_impact_invalid",
                    "covered_by_story_close requires a rationale",
                );
            }
            match qa.impact.behavioral_owner.as_deref() {
                Some(owner) => match kind_for_id(owner) {
                    Ok(WorkKind::Story) => {}
                    _ => report.push(
                        "qa_impact_invalid",
                        "covered_by_story_close behavioral_owner must be a Story id",
                    ),
                },
                None => report.push(
                    "qa_impact_invalid",
                    "covered_by_story_close requires a behavioral_owner Story id",
                ),
            }
        }
        QaImpactPosture::Required => {
            match qa.impact.behavioral_owner.as_deref() {
                Some(owner) => match kind_for_id(owner) {
                    Ok(WorkKind::Story) => {}
                    _ => report.push(
                        "qa_impact_invalid",
                        "required QA impact behavioral_owner must be a Story id",
                    ),
                },
                None => report.push(
                    "qa_impact_invalid",
                    "required QA impact requires a behavioral_owner Story id",
                ),
            }
            if qa.impact.affected_case_ids.is_empty() {
                report.push(
                    "qa_impact_invalid",
                    "required QA impact requires at least one affected case id",
                );
            }
        }
    }
}

fn validate_shaping_pointer(
    node: &Node,
    shaping: &ShapingPointer,
    report: &mut ContractValidationReport,
) {
    validate_receipt_ref(&shaping.receipt, report);
    if shaping.applied_by.trim().is_empty() {
        report.push(
            "shaping_receipt_missing",
            "shaping applied_by must be non-empty",
        );
    }
    if let Some(map) = &shaping.map {
        validate_path(
            &map.path,
            Some(&node.content_dir),
            "shaping_map_path_unsafe",
            report,
        );
        if map.revision < 1 {
            report.push(
                "shaping_map_revision_stale",
                "shaping map revision must be >= 1",
            );
        }
        validate_hash(
            &map.content_hash,
            "shaping_map_content_stale",
            "shaping map content_hash must be sha256:<hex>",
            report,
        );
    }
}

fn validate_content_ref(
    content: &ContentRef,
    required_prefix: Option<&str>,
    hash_code: &'static str,
    report: &mut ContractValidationReport,
) {
    validate_path(
        &content.path,
        required_prefix,
        "implementation_contract_invalid",
        report,
    );
    validate_hash(
        &content.content_hash,
        hash_code,
        "content_hash must be sha256:<hex>",
        report,
    );
}

fn validate_surface_refs(
    field: &'static str,
    refs: &[SurfaceRef],
    report: &mut ContractValidationReport,
) {
    if refs.len() > MAX_COLLECTION {
        report.push(
            "implementation_contract_invalid",
            format!("{field} exceeds the bounded collection limit"),
        );
    }
    let mut seen = BTreeSet::new();
    for reference in refs {
        validate_path(
            &reference.path,
            None,
            "implementation_contract_invalid",
            report,
        );
        if let Some(symbol) = &reference.symbol {
            validate_non_empty_bounded(
                symbol,
                "implementation_contract_invalid",
                "anchor symbol must be non-empty and bounded",
                MAX_SHORT_TEXT,
                report,
            );
        }
        if let Some(hash) = &reference.content_hash {
            validate_hash(
                hash,
                "implementation_brief_hash_stale",
                "anchor content_hash must be sha256:<hex>",
                report,
            );
        }
        if !seen.insert((&reference.path, &reference.symbol)) {
            report.push(
                "implementation_contract_invalid",
                format!("{field} contains duplicate anchor {}", reference.path),
            );
        }
    }
}

fn validate_items(
    field: &'static str,
    items: &[ContractItem],
    required: bool,
    missing_code: &'static str,
    report: &mut ContractValidationReport,
) {
    if required && items.is_empty() {
        report.push(missing_code, format!("{field} requires at least one item"));
    }
    if items.len() > MAX_COLLECTION {
        report.push(
            missing_code,
            format!("{field} exceeds the bounded collection limit"),
        );
    }
    let mut seen = BTreeSet::new();
    for item in items {
        validate_stable_id(
            missing_code,
            &item.id,
            format!("{field} item id must be stable and bounded"),
            report,
        );
        validate_non_empty_bounded(
            &item.summary,
            missing_code,
            format!("{field} item summary must be non-empty and bounded"),
            MAX_SHORT_TEXT,
            report,
        );
        if !seen.insert(&item.id) {
            report.push(
                missing_code,
                format!("{field} contains duplicate item id {}", item.id),
            );
        }
    }
}

fn validate_work_ref(
    reference: &RevisionedWorkRef,
    expected_kind: Option<WorkKind>,
    report: &mut ContractValidationReport,
) {
    match (kind_for_id(&reference.id), expected_kind) {
        (Ok(kind), Some(expected)) if kind == expected => {}
        (Ok(_), Some(_)) | (Err(_), _) => report.push(
            "implementation_contract_invalid",
            format!("work reference id has unexpected kind: {}", reference.id),
        ),
        (Ok(_), None) => {}
    }
    if reference.contract_revision < 1 {
        report.push(
            "contract_revision_invalid",
            "referenced contract_revision must be >= 1",
        );
    }
}

fn validate_destination_owner(
    reference: &RevisionedWorkRef,
    report: &mut ContractValidationReport,
) {
    match kind_for_id(&reference.id) {
        Ok(WorkKind::Epic | WorkKind::Story) => {}
        _ => report.push(
            "decision_work_destination_invalid",
            "decision-work destination_owner must be an Epic or Story id",
        ),
    }
    if reference.contract_revision < 1 {
        report.push(
            "decision_work_destination_invalid",
            "decision-work destination_owner contract_revision must be >= 1",
        );
    }
}

fn validate_receipt_ref(reference: &ReceiptRef, report: &mut ContractValidationReport) {
    validate_receipt_ref_with_codes(
        reference,
        "shaping_receipt_missing",
        "shaping_receipt_hash_mismatch",
        report,
    );
}

fn validate_receipt_ref_with_codes(
    reference: &ReceiptRef,
    id_code: &'static str,
    hash_code: &'static str,
    report: &mut ContractValidationReport,
) {
    validate_receipt_id_with_code(&reference.id, id_code, report);
    validate_hash(
        &reference.hash,
        hash_code,
        "receipt hash must be sha256:<hex>",
        report,
    );
}

fn validate_receipt_id_with_code(
    value: &str,
    code: &'static str,
    report: &mut ContractValidationReport,
) {
    if value.trim().is_empty() || value.len() > 128 || !value.starts_with("rcpt_") {
        report.push(
            code,
            format!("receipt id must be a bounded rcpt_ identifier: {value}"),
        );
    }
}

fn validate_hash(
    value: &str,
    code: &'static str,
    message: &'static str,
    report: &mut ContractValidationReport,
) {
    let Some(hex) = value.strip_prefix("sha256:") else {
        report.push(code, message);
        return;
    };
    if hex.len() != 64 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        report.push(code, message);
    }
}

fn validate_path(
    value: &str,
    required_prefix: Option<&str>,
    code: &'static str,
    report: &mut ContractValidationReport,
) {
    match crate::graph::model::node::safe_repo_relative(value) {
        Ok(path) => {
            if let Some(prefix) = required_prefix {
                if !path.starts_with(Path::new(prefix)) {
                    report.push(
                        code,
                        format!("path {value} must live under content_dir {prefix}"),
                    );
                }
            }
        }
        Err(error) => report.push(code, error.to_string()),
    }
}

fn validate_unique_texts(
    field: &'static str,
    values: &[String],
    report: &mut ContractValidationReport,
) {
    if values.len() > MAX_COLLECTION {
        report.push(
            "implementation_contract_invalid",
            format!("{field} exceeds the bounded collection limit"),
        );
    }
    let mut seen = BTreeSet::new();
    for value in values {
        validate_non_empty_bounded(
            value,
            "implementation_contract_invalid",
            format!("{field} entries must be non-empty and bounded"),
            MAX_SHORT_TEXT,
            report,
        );
        if !seen.insert(value) {
            report.push(
                "implementation_contract_invalid",
                format!("{field} contains duplicate entry {value}"),
            );
        }
    }
}

fn validate_case_ids(values: &[String], report: &mut ContractValidationReport) {
    if values.len() > MAX_COLLECTION {
        report.push(
            "qa_impact_invalid",
            "affected_case_ids exceeds the bounded collection limit",
        );
    }
    let mut seen = BTreeSet::new();
    for value in values {
        if !is_portable_case_id(value) {
            report.push(
                "qa_impact_invalid",
                format!(
                    "affected_case_id must be a 1-64 character portable uppercase case identifier: {value}"
                ),
            );
        }
        if !seen.insert(value) {
            report.push(
                "qa_impact_invalid",
                format!("affected_case_ids contains duplicate case id {value}"),
            );
        }
    }
}

fn is_portable_case_id(value: &str) -> bool {
    if value.is_empty() || value.len() > MAX_ID {
        return false;
    }
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !is_case_id_boundary_char(first) {
        return false;
    }
    let mut last = first;
    for character in chars {
        if !is_case_id_char(character) {
            return false;
        }
        last = character;
    }
    is_case_id_boundary_char(last)
}

fn is_case_id_char(character: char) -> bool {
    character.is_ascii_uppercase() || character.is_ascii_digit() || character == '-'
}

fn is_case_id_boundary_char(character: char) -> bool {
    character.is_ascii_uppercase() || character.is_ascii_digit()
}

fn is_portable_branch_id(value: &str) -> bool {
    value.len() <= MAX_ID && value.strip_prefix("BR-").is_some_and(is_portable_case_id)
}

fn validate_unique_enums<T>(
    field: &'static str,
    values: &[T],
    code: &'static str,
    report: &mut ContractValidationReport,
) where
    T: Ord + std::fmt::Debug,
{
    if values.len() > MAX_COLLECTION {
        report.push(
            code,
            format!("{field} exceeds the bounded collection limit"),
        );
    }
    let mut seen = BTreeSet::new();
    for value in values {
        if !seen.insert(value) {
            report.push(code, format!("{field} contains duplicate entry {value:?}"));
        }
    }
}

fn validate_stable_id(
    code: &'static str,
    value: &str,
    message: impl Into<String>,
    report: &mut ContractValidationReport,
) {
    if value.len() < 2
        || value.len() > MAX_ID
        || value.starts_with('-')
        || value.ends_with('-')
        || !value
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '-')
    {
        report.push(code, message.into());
    }
}

fn validate_slugish(
    value: &str,
    code: &'static str,
    message: impl Into<String>,
    report: &mut ContractValidationReport,
) {
    if value.trim().is_empty()
        || value.len() > MAX_ID
        || value.starts_with('-')
        || value.ends_with('-')
        || !value
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
    {
        report.push(code, message.into());
    }
}

fn validate_non_empty_bounded(
    value: &str,
    code: &'static str,
    message: impl Into<String>,
    max: usize,
    report: &mut ContractValidationReport,
) {
    if value.trim().is_empty() || value.len() > max {
        report.push(code, message.into());
    }
}
