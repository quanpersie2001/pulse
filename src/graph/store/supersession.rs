use super::*;

impl JsonGraphStore {
    pub fn supersede_work_with_context(
        &self,
        old_id: &str,
        target: SupersessionTarget,
        expected_revision: u64,
        reason: String,
        assertion: SupersessionAssertion,
        ctx: OperationContext,
    ) -> PulseResult<MutationOutcome<SupersededWork>> {
        let _guard = WriteGuard::acquire(&self.repo_root)?;
        self.bootstrap_unlocked()?;
        recover_prepared_transactions(&self.repo_root)?;
        let old_path = self.node_path(old_id);
        if !old_path.exists() {
            return Err(PulseError::NotFound {
                subject: old_id.to_string(),
            });
        }
        let before_bytes = fs::read(&old_path).map_err(|error| PulseError::io(&old_path, error))?;
        let mut old: Node = serde_json::from_slice(&before_bytes)
            .map_err(|error| PulseError::json(&old_path, error))?;

        let nodes = self.load_nodes()?;
        let edges = self
            .load_edges()?
            .into_iter()
            .map(|(_, e)| e)
            .collect::<Vec<_>>();
        if old.revision != expected_revision {
            if let Some(existing) = self.same_supersession(&old, &target, &assertion, &edges) {
                return Ok(MutationOutcome {
                    schema_version: 1,
                    code: "unchanged".to_string(),
                    status: MutationStatus::Unchanged,
                    value: SupersededWork {
                        node: old,
                        edge: existing,
                        target,
                        assertion: Some(assertion),
                        reconciliation_receipt: None,
                    },
                });
            }
            return Err(PulseError::CasConflict {
                subject: old_id.to_string(),
                expected_revision,
                current_revision: old.revision,
            });
        }
        if reason.trim().is_empty() {
            return Err(PulseError::validation(
                "reason_required",
                "supersession requires a non-empty reason",
            ));
        }
        validate_supersession_assertion(&assertion, &nodes)?;
        let existing_outgoing = superseded_by_edges(&edges, old_id);
        if old.status == NodeStatus::Superseded {
            if let Some(existing) = self.same_supersession(&old, &target, &assertion, &edges) {
                return Ok(MutationOutcome {
                    schema_version: 1,
                    code: "unchanged".to_string(),
                    status: MutationStatus::Unchanged,
                    value: SupersededWork {
                        node: old,
                        edge: existing,
                        target,
                        assertion: Some(assertion),
                        reconciliation_receipt: None,
                    },
                });
            }
            return Err(PulseError::validation(
                "supersession_conflict",
                "work is already superseded by a different target or assertion",
            ));
        }
        if !matches!(
            old.status,
            NodeStatus::Draft | NodeStatus::Shaped | NodeStatus::Ready | NodeStatus::Blocked
        ) {
            return Err(PulseError::validation(
                "supersession_unavailable",
                format!("status {:?} cannot be superseded", old.status),
            ));
        }
        if !existing_outgoing.is_empty() {
            return Err(PulseError::validation(
                "supersession_conflict",
                "work already has an outgoing superseded_by edge",
            ));
        }

        let edge = match &target {
            SupersessionTarget::Replacement { id } => {
                let replacement = nodes.get(id).ok_or_else(|| PulseError::NotFound {
                    subject: id.clone(),
                })?;
                if id == old_id {
                    return Err(PulseError::validation(
                        "supersession_cycle",
                        "work cannot supersede itself",
                    ));
                }
                if matches!(
                    replacement.status,
                    NodeStatus::Cancelled | NodeStatus::Superseded
                ) {
                    return Err(PulseError::validation(
                        "invalid_supersession_target",
                        "replacement must not be cancelled or superseded",
                    ));
                }
                let planned_edge = Edge::new(
                    EdgeType::SupersededBy,
                    old_id.to_string(),
                    id.clone(),
                    ctx.actor.clone(),
                    ctx.now,
                )?;
                let mut all_edges = edges.clone();
                all_edges.push(planned_edge.clone());
                if supersession_reaches(&all_edges, id, old_id) {
                    return Err(PulseError::validation(
                        "supersession_cycle",
                        "supersession edge would create a cycle",
                    ));
                }
                Some(planned_edge)
            }
            SupersessionTarget::Decision { id } => {
                let decision = nodes.get(id).ok_or_else(|| PulseError::NotFound {
                    subject: id.clone(),
                })?;
                if decision.kind != WorkKind::Decision {
                    return Err(PulseError::validation(
                        "invalid_supersession_target",
                        "decision target must have kind Decision",
                    ));
                }
                None
            }
        };

        old.status = NodeStatus::Superseded;
        old.status_reason = Some(StatusReason::new(
            "superseded",
            reason.clone(),
            match &target {
                SupersessionTarget::Replacement { .. } => None,
                SupersessionTarget::Decision { id } => Some(id.clone()),
            },
        )?);
        old.revision += 1;
        old.updated_at = ctx.now;

        let mut all_nodes = nodes.clone();
        all_nodes.insert(old.id.clone(), old.clone());
        let mut all_edges = edges.clone();
        if let Some(edge) = &edge {
            all_edges.push(edge.clone());
        }
        validate_graph(
            &self.repo_root,
            &self.manifest()?,
            &all_nodes.values().cloned().collect::<Vec<_>>(),
            &all_edges,
        )
        .into_result()?;

        let graph_fingerprint_before = self.graph_fingerprint_current_unlocked()?;
        let graph_fingerprint_after =
            self.graph_fingerprint_with_planned_workgraph(&old, edge.as_ref())?;
        let old_after_bytes = to_canonical_bytes(&old)?;
        let event = EventEnvelope::new(
            new_event_id(),
            "work.node.superseded",
            ctx.actor.clone(),
            old_id,
            json!({
                "old_id": old_id,
                "expected_revision": expected_revision,
                "new_revision": old.revision,
                "target": target,
                "reason": reason,
                "assertion": assertion,
                "graph_fingerprint_before": graph_fingerprint_before,
                "graph_fingerprint_after": graph_fingerprint_after,
                "gate_coverage": ["supersession_preconditions", "assertion_identity", "graph_integrity"],
            }),
            ctx.now,
        );
        match &edge {
            Some(edge) => {
                let edge_path = self.edge_path(&edge.id);
                if edge_path.exists() {
                    return Err(PulseError::AlreadyExists {
                        subject: edge.id.clone(),
                    });
                }
                let edge_after_bytes = to_canonical_bytes(edge)?;
                let targets = vec![
                    TransactionTarget::new(
                        old_path.clone(),
                        FileState::Present {
                            hash: hash_bytes(&before_bytes),
                            revision: expected_revision,
                        },
                        FileState::Present {
                            hash: hash_bytes(&old_after_bytes),
                            revision: expected_revision + 1,
                        },
                        &old_after_bytes,
                    ),
                    TransactionTarget::new(
                        edge_path,
                        FileState::Absent,
                        FileState::Present {
                            hash: hash_bytes(&edge_after_bytes),
                            revision: edge.revision,
                        },
                        &edge_after_bytes,
                    ),
                ];
                let intent = MultiTargetTransactionIntent::prepared(
                    event.id.clone(),
                    event.event_type.clone(),
                    ctx.actor,
                    targets,
                    event_path(&self.repo_root, &event),
                    serde_json::to_value(&event)?,
                )?;
                let prepared = prepare_multi_target_transaction(&self.repo_root, intent)?;
                commit_prepared_multi_target_transaction(&prepared, self.failpoint)?;
            }
            None => {
                self.commit_mutation(
                    "work.node.superseded",
                    ctx.actor,
                    old_id,
                    serde_json::to_value(&event.payload)?,
                    &old_path,
                    FileState::Present {
                        hash: hash_bytes(&before_bytes),
                        revision: expected_revision,
                    },
                    FileState::Present {
                        hash: hash_bytes(&old_after_bytes),
                        revision: expected_revision + 1,
                    },
                    &old_after_bytes,
                    ctx.now,
                )?;
            }
        }

        Ok(MutationOutcome {
            schema_version: 1,
            code: "superseded".to_string(),
            status: MutationStatus::Updated,
            value: SupersededWork {
                node: old,
                edge,
                target,
                assertion: Some(assertion),
                reconciliation_receipt: None,
            },
        })
    }

    pub fn supersede_work(
        &self,
        old_id: &str,
        target: SupersessionTarget,
        expected_revision: u64,
        reason: String,
        assertion: SupersessionAssertion,
        actor: String,
    ) -> PulseResult<MutationOutcome<SupersededWork>> {
        self.supersede_work_with_context(
            old_id,
            target,
            expected_revision,
            reason,
            assertion,
            OperationContext {
                actor,
                now: Utc::now(),
            },
        )
    }

    pub fn supersede_work_with_receipt(
        &self,
        old_id: &str,
        target: SupersessionTarget,
        expected_revision: u64,
        reason: String,
        receipt_id: String,
        actor: String,
    ) -> PulseResult<MutationOutcome<SupersededWork>> {
        let _guard = WriteGuard::acquire(&self.repo_root)?;
        self.bootstrap_unlocked()?;
        recover_prepared_transactions(&self.repo_root)?;
        crate::evidence::manifest::bootstrap(&self.repo_root)?;
        let old_path = self.node_path(old_id);
        if !old_path.exists() {
            return Err(PulseError::NotFound {
                subject: old_id.to_string(),
            });
        }
        let before_bytes = fs::read(&old_path).map_err(|error| PulseError::io(&old_path, error))?;
        let mut old: Node = serde_json::from_slice(&before_bytes)
            .map_err(|error| PulseError::json(&old_path, error))?;
        let nodes = self.load_nodes()?;
        let edges = self
            .load_edges()?
            .into_iter()
            .map(|(_, e)| e)
            .collect::<Vec<_>>();
        let target_id = match &target {
            SupersessionTarget::Replacement { id } | SupersessionTarget::Decision { id } => {
                id.clone()
            }
        };
        if old.revision != expected_revision {
            if let Some((existing_edge, receipt_ref)) =
                self.same_supersession_receipt(old_id, &target, &receipt_id, &edges)?
            {
                return Ok(MutationOutcome {
                    schema_version: 1,
                    code: "unchanged".to_string(),
                    status: MutationStatus::Unchanged,
                    value: SupersededWork {
                        node: old,
                        edge: existing_edge,
                        target,
                        assertion: None,
                        reconciliation_receipt: Some(receipt_ref),
                    },
                });
            }
            return Err(PulseError::CasConflict {
                subject: old_id.to_string(),
                expected_revision,
                current_revision: old.revision,
            });
        }
        if reason.trim().is_empty() {
            return Err(PulseError::validation(
                "reason_required",
                "supersession requires a non-empty reason",
            ));
        }
        let target_node = nodes.get(&target_id).ok_or_else(|| PulseError::NotFound {
            subject: target_id.clone(),
        })?;
        let receipt_ref = crate::evidence::receipt::validate_for_supersession(
            &self.repo_root,
            &receipt_id,
            old_id,
            expected_revision,
            &target_id,
            target_node.revision,
        )?;
        let existing_outgoing = superseded_by_edges(&edges, old_id);
        if old.status == NodeStatus::Superseded {
            if let Some((existing_edge, receipt_ref)) =
                self.same_supersession_receipt(old_id, &target, &receipt_id, &edges)?
            {
                return Ok(MutationOutcome {
                    schema_version: 1,
                    code: "unchanged".to_string(),
                    status: MutationStatus::Unchanged,
                    value: SupersededWork {
                        node: old,
                        edge: existing_edge,
                        target,
                        assertion: None,
                        reconciliation_receipt: Some(receipt_ref),
                    },
                });
            }
            return Err(PulseError::validation(
                "supersession_conflict",
                "work is already superseded by a different target or receipt",
            ));
        }
        if !matches!(
            old.status,
            NodeStatus::Draft | NodeStatus::Shaped | NodeStatus::Ready | NodeStatus::Blocked
        ) {
            return Err(PulseError::validation(
                "supersession_unavailable",
                format!("status {:?} cannot be superseded", old.status),
            ));
        }
        if !existing_outgoing.is_empty() {
            return Err(PulseError::validation(
                "supersession_conflict",
                "work already has an outgoing superseded_by edge",
            ));
        }
        let edge = match &target {
            SupersessionTarget::Replacement { id } => {
                if id == old_id {
                    return Err(PulseError::validation(
                        "supersession_cycle",
                        "work cannot supersede itself",
                    ));
                }
                let replacement = nodes.get(id).ok_or_else(|| PulseError::NotFound {
                    subject: id.clone(),
                })?;
                if matches!(
                    replacement.status,
                    NodeStatus::Cancelled | NodeStatus::Superseded
                ) {
                    return Err(PulseError::validation(
                        "invalid_supersession_target",
                        "replacement must not be cancelled or superseded",
                    ));
                }
                let planned_edge = Edge::new(
                    EdgeType::SupersededBy,
                    old_id.to_string(),
                    id.clone(),
                    actor.clone(),
                    Utc::now(),
                )?;
                let mut all_edges = edges.clone();
                all_edges.push(planned_edge.clone());
                if supersession_reaches(&all_edges, id, old_id) {
                    return Err(PulseError::validation(
                        "supersession_cycle",
                        "supersession edge would create a cycle",
                    ));
                }
                Some(planned_edge)
            }
            SupersessionTarget::Decision { id } => {
                if target_node.kind != WorkKind::Decision {
                    return Err(PulseError::validation(
                        "invalid_supersession_target",
                        "decision target must have kind Decision",
                    ));
                }
                let _ = id;
                None
            }
        };
        let now = Utc::now();
        old.status = NodeStatus::Superseded;
        old.status_reason = Some(StatusReason::new(
            "superseded",
            reason.clone(),
            match &target {
                SupersessionTarget::Replacement { .. } => None,
                SupersessionTarget::Decision { id } => Some(id.clone()),
            },
        )?);
        old.revision += 1;
        old.updated_at = now;
        let graph_fingerprint_before = self.graph_fingerprint_current_unlocked()?;
        let graph_fingerprint_after =
            self.graph_fingerprint_with_planned_workgraph(&old, edge.as_ref())?;
        let old_after_bytes = to_canonical_bytes(&old)?;
        let event = EventEnvelope::new(
            new_event_id(),
            "work.node.superseded",
            actor.clone(),
            old_id,
            json!({
                "old_id": old_id, "expected_revision": expected_revision, "new_revision": old.revision, "target": target, "reason": reason,
                "reconciliation_receipt": receipt_ref, "graph_fingerprint_before": graph_fingerprint_before,
                "graph_fingerprint_after": graph_fingerprint_after,
                "gate_coverage": ["supersession_preconditions", "receipt_identity", "graph_integrity"]
            }),
            now,
        );
        match &edge {
            Some(edge) => {
                let edge_after_bytes = to_canonical_bytes(edge)?;
                let targets = vec![
                    TransactionTarget::new(
                        old_path.clone(),
                        FileState::Present {
                            hash: hash_bytes(&before_bytes),
                            revision: expected_revision,
                        },
                        FileState::Present {
                            hash: hash_bytes(&old_after_bytes),
                            revision: expected_revision + 1,
                        },
                        &old_after_bytes,
                    ),
                    TransactionTarget::new(
                        self.edge_path(&edge.id),
                        FileState::Absent,
                        FileState::Present {
                            hash: hash_bytes(&edge_after_bytes),
                            revision: edge.revision,
                        },
                        &edge_after_bytes,
                    ),
                ];
                let intent = MultiTargetTransactionIntent::prepared(
                    event.id.clone(),
                    event.event_type.clone(),
                    actor,
                    targets,
                    event_path(&self.repo_root, &event),
                    serde_json::to_value(&event)?,
                )?;
                let prepared = prepare_multi_target_transaction(&self.repo_root, intent)?;
                commit_prepared_multi_target_transaction(&prepared, self.failpoint)?;
            }
            None => self.commit_mutation(
                "work.node.superseded",
                actor,
                old_id,
                serde_json::to_value(&event.payload)?,
                &old_path,
                FileState::Present {
                    hash: hash_bytes(&before_bytes),
                    revision: expected_revision,
                },
                FileState::Present {
                    hash: hash_bytes(&old_after_bytes),
                    revision: expected_revision + 1,
                },
                &old_after_bytes,
                now,
            )?,
        }
        Ok(MutationOutcome {
            schema_version: 1,
            code: "superseded".to_string(),
            status: MutationStatus::Updated,
            value: SupersededWork {
                node: old,
                edge,
                target,
                assertion: None,
                reconciliation_receipt: Some(receipt_ref),
            },
        })
    }
}

fn superseded_by_edges(edges: &[Edge], from: &str) -> Vec<Edge> {
    edges
        .iter()
        .filter(|edge| edge.edge_type == EdgeType::SupersededBy && edge.from == from)
        .cloned()
        .collect()
}

fn supersession_reaches(edges: &[Edge], start: &str, needle: &str) -> bool {
    let mut current = start;
    let mut seen = std::collections::BTreeSet::new();
    while seen.insert(current.to_string()) {
        let Some(next) = edges
            .iter()
            .find(|edge| edge.edge_type == EdgeType::SupersededBy && edge.from == current)
            .map(|edge| edge.to.as_str())
        else {
            return false;
        };
        if next == needle {
            return true;
        }
        current = next;
    }
    false
}

fn validate_supersession_assertion(
    assertion: &SupersessionAssertion,
    nodes: &BTreeMap<String, Node>,
) -> PulseResult<()> {
    if assertion.assertion_version != 1 {
        return Err(PulseError::validation(
            "invalid_supersession_assertion",
            "assertion_version must be 1",
        ));
    }
    if assertion.asserted_by.trim().is_empty() {
        return Err(PulseError::validation(
            "invalid_supersession_assertion",
            "asserted_by must not be empty",
        ));
    }
    for source in &assertion.source_revisions {
        let (id, revision) = source.split_once('@').ok_or_else(|| {
            PulseError::validation(
                "invalid_supersession_assertion",
                format!("source revision must be ID@revision: {source}"),
            )
        })?;
        let revision = revision.parse::<u64>().map_err(|_| {
            PulseError::validation(
                "invalid_supersession_assertion",
                format!("source revision must contain numeric revision: {source}"),
            )
        })?;
        let node = nodes.get(id).ok_or_else(|| PulseError::NotFound {
            subject: id.to_string(),
        })?;
        if node.revision != revision {
            return Err(PulseError::validation(
                "assertion_revision_mismatch",
                format!(
                    "assertion source {id}@{revision} does not match current revision {}",
                    node.revision
                ),
            ));
        }
    }
    for reference in &assertion.references {
        if !nodes.contains_key(reference) {
            return Err(PulseError::NotFound {
                subject: reference.clone(),
            });
        }
    }
    if assertion.claim == SupersessionClaim::FollowUpRequired
        && !assertion.references.iter().any(|reference| {
            nodes
                .get(reference)
                .is_some_and(|node| node.kind != WorkKind::Decision)
        })
    {
        return Err(PulseError::validation(
            "follow_up_reference_required",
            "follow_up_required assertions must reference at least one work item",
        ));
    }
    Ok(())
}

impl JsonGraphStore {
    pub(super) fn same_supersession(
        &self,
        old: &Node,
        target: &SupersessionTarget,
        assertion: &SupersessionAssertion,
        edges: &[Edge],
    ) -> Option<Option<Edge>> {
        if old.status != NodeStatus::Superseded
            || !self.supersession_event_matches(&old.id, target, assertion)
        {
            return None;
        }
        match target {
            SupersessionTarget::Replacement { id } => {
                let outgoing = superseded_by_edges(edges, &old.id);
                if outgoing.len() == 1 && outgoing[0].to == *id {
                    Some(Some(outgoing[0].clone()))
                } else {
                    None
                }
            }
            SupersessionTarget::Decision { id } => {
                if old
                    .status_reason
                    .as_ref()
                    .and_then(|reason| reason.reference.as_ref())
                    == Some(id)
                {
                    Some(None)
                } else {
                    None
                }
            }
        }
    }

    pub(super) fn supersession_event_matches(
        &self,
        old_id: &str,
        target: &SupersessionTarget,
        assertion: &SupersessionAssertion,
    ) -> bool {
        let events_dir = self.repo_root.join(".pulse/events");
        let Ok(date_dirs) = fs::read_dir(events_dir) else {
            return false;
        };
        let target_value = match serde_json::to_value(target) {
            Ok(value) => value,
            Err(_) => return false,
        };
        let assertion_value = match serde_json::to_value(assertion) {
            Ok(value) => value,
            Err(_) => return false,
        };
        for date_dir in date_dirs.flatten() {
            let Ok(entries) = fs::read_dir(date_dir.path()) else {
                continue;
            };
            for entry in entries.flatten() {
                if entry.path().extension().and_then(|ext| ext.to_str()) != Some("json") {
                    continue;
                }
                let Ok(event) = storage::read_json::<EventEnvelope>(&entry.path()) else {
                    continue;
                };
                if event.event_type == "work.node.superseded"
                    && event.subject.id == old_id
                    && event.payload.get("target") == Some(&target_value)
                    && event.payload.get("assertion") == Some(&assertion_value)
                {
                    return true;
                }
            }
        }
        false
    }

    pub(super) fn same_supersession_receipt(
        &self,
        old_id: &str,
        target: &SupersessionTarget,
        receipt_id: &str,
        edges: &[Edge],
    ) -> PulseResult<Option<(Option<Edge>, crate::evidence::model::ReceiptReference)>> {
        let events_dir = self.repo_root.join(".pulse/events");
        let Ok(date_dirs) = fs::read_dir(events_dir) else {
            return Ok(None);
        };
        let target_value = serde_json::to_value(target)?;
        for date_dir in date_dirs.flatten() {
            let Ok(entries) = fs::read_dir(date_dir.path()) else {
                continue;
            };
            for entry in entries.flatten() {
                if entry.path().extension().and_then(|ext| ext.to_str()) != Some("json") {
                    continue;
                }
                let Ok(event) = storage::read_json::<EventEnvelope>(&entry.path()) else {
                    continue;
                };
                let Some(receipt_value) = event.payload.get("reconciliation_receipt") else {
                    continue;
                };
                if event.event_type == "work.node.superseded"
                    && event.subject.id == old_id
                    && event.payload.get("target") == Some(&target_value)
                    && receipt_value.get("id").and_then(|v| v.as_str()) == Some(receipt_id)
                {
                    let receipt_ref: crate::evidence::model::ReceiptReference =
                        serde_json::from_value(receipt_value.clone())?;
                    let edge = match target {
                        SupersessionTarget::Replacement { .. } => {
                            superseded_by_edges(edges, old_id).into_iter().next()
                        }
                        SupersessionTarget::Decision { .. } => None,
                    };
                    return Ok(Some((edge, receipt_ref)));
                }
            }
        }
        Ok(None)
    }
}
