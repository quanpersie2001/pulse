use super::*;
use crate::graph::model::contract::{
    ContentRef, ContractItem, ContractScope, EffortMetadata, ImplementationContract,
    ImplementationMode, ImplementationSemanticImpact, Materialization, PlanPolicy, QaImpact,
    QaImpactPosture, QaMetadata, Risk, SurfaceRef, TicketRole, WorkSurface,
};

impl JsonGraphStore {
    fn write_ticket_template(&self, node: &mut Node) -> PulseResult<()> {
        if node.kind != WorkKind::Ticket {
            return Ok(());
        }
        let relative = format!("{}/ticket.md", node.content_dir);
        let path = self.repo_root.join(&relative);
        let materialization = node.materialization.unwrap_or(Materialization::R0);
        let contents = if node.role == Some(TicketRole::DecisionWork) {
            crate::graph::model::brief::decision_work_template(&node.id, &node.title)
        } else {
            crate::graph::model::brief::implementation_template(
                &node.id,
                &node.title,
                materialization,
            )
        };
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| PulseError::io(parent, error))?;
        }
        storage::atomic_write(&path, contents.as_bytes())?;
        node.brief_hash = Some(hash_bytes(contents.as_bytes()));
        Ok(())
    }

    /// Synchronize the graph binding and metadata from `works/<id>/ticket.md`.
    pub fn sync_ticket_with_context(
        &self,
        ticket_id: &str,
        expected_revision: u64,
        ctx: OperationContext,
    ) -> PulseResult<MutationOutcome<Node>> {
        let _guard = WriteGuard::acquire(&self.repo_root)?;
        self.bootstrap_unlocked()?;
        recover_prepared_transactions(&self.repo_root)?;
        let path = self.node_path(ticket_id);
        let before_bytes = fs::read(&path).map_err(|error| PulseError::io(&path, error))?;
        let mut node: Node = serde_json::from_slice(&before_bytes)
            .map_err(|error| PulseError::json(&path, error))?;
        if node.kind != WorkKind::Ticket {
            return Err(PulseError::validation(
                "ticket_brief_invalid",
                "work sync only applies to Tickets",
            ));
        }
        if node.revision != expected_revision {
            return Err(PulseError::CasConflict {
                subject: ticket_id.to_string(),
                expected_revision,
                current_revision: node.revision,
            });
        }
        let brief_path = self.repo_root.join(&node.content_dir).join("ticket.md");
        let bytes = fs::read(&brief_path).map_err(|error| PulseError::io(&brief_path, error))?;
        let markdown = std::str::from_utf8(&bytes).map_err(|_| {
            PulseError::validation("ticket_brief_invalid", "ticket.md must be UTF-8")
        })?;
        let brief = crate::graph::model::brief::parse_ticket_brief(markdown)?;
        let materialization = node.materialization.unwrap_or(Materialization::R0);
        brief.validate_for(materialization)?;
        if node.role == Some(TicketRole::Implementation) {
            node.implementation =
                Some(brief_to_legacy_contract(&brief, &node, hash_bytes(&bytes))?);
        }
        if let Some(docs) = &brief.documentation {
            node.documentation = Some(brief_docs_metadata(docs)?);
        }
        if let Some(qa) = &brief.qa {
            node.qa = Some(brief_qa_metadata(qa)?);
        }
        let new_hash = hash_bytes(&bytes);
        let changed = node.brief_hash.as_deref() != Some(new_hash.as_str());
        node.brief_hash = Some(new_hash);
        if changed {
            node.contract_revision += 1;
        }
        node.revision += 1;
        node.updated_at = ctx.now;
        validate_node_contract_result(&node, ContractValidationMode::CanonicalStorage)?;
        let after_bytes = to_canonical_bytes(&node)?;
        self.commit_mutation("work.contract.updated", ctx.actor, ticket_id, json!({"ticket_id": ticket_id, "expected_revision": expected_revision, "new_revision": node.revision, "new_contract_revision": node.contract_revision, "brief_hash": node.brief_hash, "gate_coverage": ["ticket_brief_parse", "node_revision_cas", "brief_hash"]}), &path, FileState::Present { hash: hash_bytes(&before_bytes), revision: expected_revision }, FileState::Present { hash: hash_bytes(&after_bytes), revision: expected_revision + 1 }, &after_bytes, ctx.now)?;
        Ok(MutationOutcome {
            schema_version: 1,
            code: "updated".to_string(),
            status: if changed {
                MutationStatus::Updated
            } else {
                MutationStatus::Unchanged
            },
            value: node,
        })
    }

    pub fn sync_ticket(
        &self,
        ticket_id: &str,
        expected_revision: u64,
        actor: String,
    ) -> PulseResult<MutationOutcome<Node>> {
        self.sync_ticket_with_context(
            ticket_id,
            expected_revision,
            OperationContext {
                actor,
                now: Utc::now(),
            },
        )
    }

    pub fn create_node_with_context(
        &self,
        kind: WorkKind,
        title: String,
        ctx: OperationContext,
    ) -> PulseResult<MutationOutcome<Node>> {
        self.create_node_with_classification_context(
            kind,
            title,
            PublicCreateClassification::default(),
            ContractValidationMode::CanonicalStorage,
            ctx,
        )
    }

    pub fn create_node_public_with_context(
        &self,
        kind: WorkKind,
        title: String,
        classification: PublicCreateClassification,
        ctx: OperationContext,
    ) -> PulseResult<MutationOutcome<Node>> {
        self.create_node_public_with_context_and_tags(kind, title, classification, vec![], ctx)
    }

    pub fn create_node_public_with_context_and_tags(
        &self,
        kind: WorkKind,
        title: String,
        classification: PublicCreateClassification,
        tags: Vec<String>,
        ctx: OperationContext,
    ) -> PulseResult<MutationOutcome<Node>> {
        validate_public_create_classification(kind, &classification)?;
        self.create_node_with_classification_context_and_tags(
            kind,
            title,
            classification,
            tags,
            ContractValidationMode::PublicCreate,
            ctx,
        )
    }

    pub(super) fn create_node_with_classification_context(
        &self,
        kind: WorkKind,
        title: String,
        classification: PublicCreateClassification,
        validation_mode: ContractValidationMode,
        ctx: OperationContext,
    ) -> PulseResult<MutationOutcome<Node>> {
        self.create_node_with_classification_context_and_tags(
            kind,
            title,
            classification,
            vec![],
            validation_mode,
            ctx,
        )
    }

    fn create_node_with_classification_context_and_tags(
        &self,
        kind: WorkKind,
        title: String,
        classification: PublicCreateClassification,
        tags: Vec<String>,
        validation_mode: ContractValidationMode,
        ctx: OperationContext,
    ) -> PulseResult<MutationOutcome<Node>> {
        let _guard = WriteGuard::acquire(&self.repo_root)?;
        self.bootstrap_unlocked()?;
        recover_prepared_transactions(&self.repo_root)?;
        let id = self.allocate_id(kind)?;
        let mut node = Node::new(id.clone(), kind, title, ctx.now)?;
        if kind == WorkKind::Ticket && classification.any_present() {
            node.role = classification.role.or(Some(TicketRole::Implementation));
            node.risk = classification.risk;
            node.materialization = classification
                .materialization
                .or_else(|| classification.risk.map(Risk::default_materialization));
            node.tags = tags;
            node.tags.sort();
            node.tags.dedup();
            self.write_ticket_template(&mut node)?;
        }
        let nodes = self.load_nodes()?;
        let edges = self.load_edges()?;
        validate_id_for_kind(&id, kind)?;
        let path = self.node_path(&id);
        if path.exists() {
            return Err(PulseError::AlreadyExists { subject: id });
        }
        let mut all_nodes = nodes.clone();
        all_nodes.insert(node.id.clone(), node.clone());
        let all_node_values = all_nodes.values().cloned().collect::<Vec<_>>();
        let edge_values = edges.iter().map(|(_, e)| e.clone()).collect::<Vec<_>>();
        validate_graph(
            &self.repo_root,
            &self.manifest()?,
            &all_node_values,
            &edge_values,
        )
        .into_result()?;
        crate::graph::validation::contract::validate_node_contract_result(&node, validation_mode)?;
        let after_bytes = to_canonical_bytes(&node)?;
        self.commit_mutation(
            "work.node.created",
            ctx.actor,
            &node.id,
            json!({"node": node}),
            &path,
            FileState::Absent,
            FileState::Present {
                hash: hash_bytes(&after_bytes),
                revision: node.revision,
            },
            &after_bytes,
            ctx.now,
        )?;
        Ok(MutationOutcome {
            schema_version: 1,
            code: "created".to_string(),
            status: MutationStatus::Created,
            value: node,
        })
    }

    pub fn create_node(&self, kind: WorkKind, title: String) -> PulseResult<MutationOutcome<Node>> {
        self.create_node_with_context(kind, title, OperationContext::default())
    }

    /// Read and parse the Markdown contract without mutating graph state.
    pub fn read_ticket_brief(
        &self,
        id: &str,
    ) -> PulseResult<crate::graph::model::brief::TicketBrief> {
        let node = self.show_node(id)?;
        if node.kind != WorkKind::Ticket {
            return Err(PulseError::validation(
                "ticket_brief_invalid",
                "briefs are only defined for Tickets",
            ));
        }
        let path = self.repo_root.join(&node.content_dir).join("ticket.md");
        let bytes = fs::read(&path).map_err(|error| PulseError::io(&path, error))?;
        let text = std::str::from_utf8(&bytes).map_err(|_| {
            PulseError::validation("ticket_brief_invalid", "ticket.md must be UTF-8")
        })?;
        crate::graph::model::brief::parse_ticket_brief(text)
    }

    pub fn show_node(&self, id: &str) -> PulseResult<Node> {
        let _guard = WriteGuard::acquire(&self.repo_root)?;
        self.bootstrap_unlocked()?;
        recover_prepared_transactions(&self.repo_root)?;
        let path = self.node_path(id);
        if !path.exists() {
            return Err(PulseError::NotFound {
                subject: id.to_string(),
            });
        }
        storage::read_json(&path)
    }

    pub fn list_nodes(&self, kind: Option<WorkKind>) -> PulseResult<ListOutcome<Node>> {
        let _guard = WriteGuard::acquire(&self.repo_root)?;
        self.bootstrap_unlocked()?;
        recover_prepared_transactions(&self.repo_root)?;
        let mut nodes: Vec<_> = self.load_nodes()?.into_values().collect();
        if let Some(kind) = kind {
            nodes.retain(|n| n.kind == kind);
        }
        nodes.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(ListOutcome {
            schema_version: 1,
            code: "ok".to_string(),
            items: nodes,
        })
    }

    pub fn edit_title_with_context(
        &self,
        id: &str,
        expected_revision: u64,
        title: String,
        ctx: OperationContext,
    ) -> PulseResult<MutationOutcome<Node>> {
        if title.trim().is_empty() {
            return Err(PulseError::validation(
                "invalid_title",
                "title must not be empty",
            ));
        }
        let _guard = WriteGuard::acquire(&self.repo_root)?;
        self.bootstrap_unlocked()?;
        recover_prepared_transactions(&self.repo_root)?;
        let path = self.node_path(id);
        if !path.exists() {
            return Err(PulseError::NotFound {
                subject: id.to_string(),
            });
        }
        let before_bytes = fs::read(&path).map_err(|error| PulseError::io(&path, error))?;
        let mut node: Node = serde_json::from_slice(&before_bytes)
            .map_err(|error| PulseError::json(&path, error))?;
        if node.revision != expected_revision {
            return Err(PulseError::CasConflict {
                subject: id.to_string(),
                expected_revision,
                current_revision: node.revision,
            });
        }
        node.title = title;
        node.revision += 1;
        node.updated_at = ctx.now;
        let node_values = self
            .load_nodes_with_override(node.clone())?
            .into_values()
            .collect::<Vec<_>>();
        let edge_values = self
            .load_edges()?
            .iter()
            .map(|(_, e)| e.clone())
            .collect::<Vec<_>>();
        validate_graph(
            &self.repo_root,
            &self.manifest()?,
            &node_values,
            &edge_values,
        )
        .into_result()?;
        let after_bytes = to_canonical_bytes(&node)?;
        self.commit_mutation(
            "work.node.updated",
            ctx.actor,
            id,
            json!({"node": node, "expected_revision": expected_revision}),
            &path,
            FileState::Present {
                hash: hash_bytes(&before_bytes),
                revision: expected_revision,
            },
            FileState::Present {
                hash: hash_bytes(&after_bytes),
                revision: expected_revision + 1,
            },
            &after_bytes,
            ctx.now,
        )?;
        Ok(MutationOutcome {
            schema_version: 1,
            code: "updated".to_string(),
            status: MutationStatus::Updated,
            value: node,
        })
    }

    pub fn edit_title(
        &self,
        id: &str,
        expected_revision: u64,
        title: String,
    ) -> PulseResult<MutationOutcome<Node>> {
        self.edit_title_with_context(id, expected_revision, title, OperationContext::default())
    }

    pub fn update_tags(
        &self,
        ticket_id: &str,
        expected_revision: u64,
        mut tags: Vec<String>,
        actor: String,
    ) -> PulseResult<MutationOutcome<Node>> {
        let _guard = WriteGuard::acquire(&self.repo_root)?;
        self.bootstrap_unlocked()?;
        recover_prepared_transactions(&self.repo_root)?;
        let path = self.node_path(ticket_id);
        let before_bytes = fs::read(&path).map_err(|error| PulseError::io(&path, error))?;
        let mut node: Node = serde_json::from_slice(&before_bytes)
            .map_err(|error| PulseError::json(&path, error))?;
        if node.revision != expected_revision {
            return Err(PulseError::CasConflict {
                subject: ticket_id.to_string(),
                expected_revision,
                current_revision: node.revision,
            });
        }
        tags.retain(|tag| !tag.trim().is_empty());
        tags.sort();
        tags.dedup();
        node.tags = tags;
        node.contract_revision += 1;
        node.revision += 1;
        node.updated_at = Utc::now();
        let after_bytes = to_canonical_bytes(&node)?;
        self.commit_mutation(
            "work.tags.updated",
            actor,
            ticket_id,
            json!({"node": node, "expected_revision": expected_revision}),
            &path,
            FileState::Present {
                hash: hash_bytes(&before_bytes),
                revision: expected_revision,
            },
            FileState::Present {
                hash: hash_bytes(&after_bytes),
                revision: expected_revision + 1,
            },
            &after_bytes,
            node.updated_at,
        )?;
        Ok(MutationOutcome {
            schema_version: 1,
            code: "updated".to_string(),
            status: MutationStatus::Updated,
            value: node,
        })
    }

    pub fn update_documentation_impact_with_context(
        &self,
        ticket_id: &str,
        expected_revision: u64,
        update: DocumentationImpactUpdate,
        ctx: OperationContext,
    ) -> PulseResult<MutationOutcome<Node>> {
        let documentation = DocumentationMetadata {
            impact: DocumentationImpact {
                posture: update.posture,
                rationale: update.rationale,
                required_documents: update.required_documents,
                deferred_to: update.deferred_to,
            },
            routing: DocumentationRouting {
                paths: update.paths,
                domains: update.domains,
                labels: update.labels,
            },
        };
        documentation.validate(true)?;
        let _guard = WriteGuard::acquire(&self.repo_root)?;
        self.bootstrap_unlocked()?;
        recover_prepared_transactions(&self.repo_root)?;
        let path = self.node_path(ticket_id);
        if !path.exists() {
            return Err(PulseError::NotFound {
                subject: ticket_id.to_string(),
            });
        }
        let before_bytes = fs::read(&path).map_err(|error| PulseError::io(&path, error))?;
        let mut node: Node = serde_json::from_slice(&before_bytes)
            .map_err(|error| PulseError::json(&path, error))?;
        if node.kind != WorkKind::Ticket {
            return Err(PulseError::validation(
                "documentation_impact_requires_ticket",
                format!("documentation impact can only be set on tickets: {ticket_id}"),
            ));
        }
        if node.revision != expected_revision {
            return Err(PulseError::CasConflict {
                subject: ticket_id.to_string(),
                expected_revision,
                current_revision: node.revision,
            });
        }
        let nodes = self.load_nodes()?;
        for target in &documentation.impact.deferred_to {
            if !nodes.contains_key(target) {
                return Err(PulseError::validation(
                    "documentation_defer_target_missing",
                    format!("deferred documentation target does not exist: {target}"),
                ));
            }
        }
        let previous_documentation = node.documentation.clone();
        node.documentation = Some(documentation.clone());
        node.contract_revision += 1;
        node.revision += 1;
        node.updated_at = ctx.now;
        let node_values = self
            .load_nodes_with_override(node.clone())?
            .into_values()
            .collect::<Vec<_>>();
        let edge_values = self
            .load_edges()?
            .iter()
            .map(|(_, e)| e.clone())
            .collect::<Vec<_>>();
        validate_graph(
            &self.repo_root,
            &self.manifest()?,
            &node_values,
            &edge_values,
        )
        .into_result()?;
        let after_bytes = to_canonical_bytes(&node)?;
        self.commit_mutation(
            "work.documentation_impact.updated",
            ctx.actor,
            ticket_id,
            json!({
                "ticket_id": ticket_id,
                "expected_revision": expected_revision,
                "new_revision": node.revision,
                "previous_documentation": previous_documentation,
                "documentation": documentation,
                "gate_coverage": ["ticket_kind", "node_revision_cas", "documentation_impact_validation", "deferred_work_refs", "graph_integrity"]
            }),
            &path,
            FileState::Present {
                hash: hash_bytes(&before_bytes),
                revision: expected_revision,
            },
            FileState::Present {
                hash: hash_bytes(&after_bytes),
                revision: expected_revision + 1,
            },
            &after_bytes,
            ctx.now,
        )?;
        Ok(MutationOutcome {
            schema_version: 1,
            code: "updated".to_string(),
            status: MutationStatus::Updated,
            value: node,
        })
    }

    pub fn update_documentation_impact(
        &self,
        ticket_id: &str,
        expected_revision: u64,
        update: DocumentationImpactUpdate,
        actor: String,
    ) -> PulseResult<MutationOutcome<Node>> {
        self.update_documentation_impact_with_context(
            ticket_id,
            expected_revision,
            update,
            OperationContext {
                actor,
                now: Utc::now(),
            },
        )
    }
}

fn brief_to_legacy_contract(
    brief: &crate::graph::model::brief::TicketBrief,
    node: &Node,
    hash: String,
) -> PulseResult<ImplementationContract> {
    let item = |text: &String, prefix: &str, index: usize| ContractItem {
        id: format!("{prefix}-{index}"),
        summary: text.clone(),
    };
    let current = brief
        .current_behavior
        .clone()
        .unwrap_or_else(|| "Not specified; see repository state.".to_string());
    let target = brief
        .target_behavior
        .clone()
        .unwrap_or_else(|| brief.objective.clone().unwrap_or_default());
    let invariants = brief
        .invariants
        .iter()
        .enumerate()
        .map(|(i, value)| item(value, "INV", i + 1))
        .collect();
    let semantic_impact = match brief
        .qa
        .as_ref()
        .and_then(|qa| crate::graph::model::brief::qa_posture(&qa.posture))
    {
        Some(QaImpactPosture::None) => ImplementationSemanticImpact::NoBehaviorOrPublicRiskChange,
        _ => ImplementationSemanticImpact::BehaviorOrPublicRiskChange,
    };
    Ok(ImplementationContract {
        mode: match brief.implementation_freedom.mode.as_str() {
            "locked" => ImplementationMode::Locked,
            "open" => ImplementationMode::Open,
            _ => ImplementationMode::Guided,
        },
        work_surface: WorkSurface::Code,
        plan_policy: PlanPolicy::None,
        semantic_impact,
        effort: EffortMetadata::default(),
        verification_profile: "default".to_string(),
        brief: Some(ContentRef {
            path: format!("{}/ticket.md", node.content_dir),
            content_hash: hash,
        }),
        objective: brief.objective.clone().unwrap_or_default(),
        current_behavior: current,
        target_behavior: target,
        code_anchors: brief
            .code_anchors
            .iter()
            .map(|path| SurfaceRef::path(path.clone()))
            .collect(),
        documentation_anchors: vec![],
        configuration_anchors: vec![],
        data_anchors: vec![],
        research_refs: vec![],
        required_changes: brief
            .required_changes
            .iter()
            .enumerate()
            .map(|(i, value)| item(value, "CHG", i + 1))
            .collect(),
        invariants,
        acceptance: brief
            .acceptance
            .iter()
            .map(|value| ContractItem {
                id: value.id.clone(),
                summary: value.summary.clone(),
            })
            .collect(),
        scope: ContractScope {
            included: brief.scope.clone(),
            excluded: brief.non_scope.clone(),
        },
        implementation_freedom: vec![],
        required_decisions: vec![],
        shared_approach_refs: vec![],
        expected_evidence: vec![],
        expected_handoff: vec![],
    })
}

fn brief_docs_metadata(
    brief: &crate::graph::model::brief::DocumentationImpactBrief,
) -> PulseResult<DocumentationMetadata> {
    let posture = match brief.posture.to_ascii_lowercase().as_str() {
        "required" => DocumentationImpactPosture::Required,
        "deferred" => DocumentationImpactPosture::Deferred,
        "none" => DocumentationImpactPosture::None,
        _ => DocumentationImpactPosture::Unknown,
    };
    let impact = DocumentationImpact {
        posture,
        rationale: brief.rationale.clone(),
        required_documents: brief.documents.clone(),
        deferred_to: vec![],
    };
    let metadata = DocumentationMetadata {
        impact,
        routing: DocumentationRouting::default(),
    };
    metadata.validate(true)?;
    Ok(metadata)
}

fn brief_qa_metadata(brief: &crate::graph::model::brief::QaImpactBrief) -> PulseResult<QaMetadata> {
    let posture =
        crate::graph::model::brief::qa_posture(&brief.posture).unwrap_or(QaImpactPosture::Unknown);
    Ok(QaMetadata {
        impact: QaImpact {
            posture,
            rationale: brief.reason.clone(),
            behavioral_owner: brief.owner.clone(),
            affected_case_ids: brief.cases.clone(),
        },
    })
}
