use super::*;

impl JsonGraphStore {
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
        validate_public_create_classification(kind, &classification)?;
        self.create_node_with_classification_context(
            kind,
            title,
            classification,
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
        let _guard = WriteGuard::acquire(&self.repo_root)?;
        self.bootstrap_unlocked()?;
        recover_prepared_transactions(&self.repo_root)?;
        let id = self.allocate_id(kind)?;
        let mut node = Node::new(id.clone(), kind, title, ctx.now)?;
        if kind == WorkKind::Ticket && classification.any_present() {
            node.role = classification.role;
            node.risk = classification.risk;
            node.materialization = classification.materialization;
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
        crate::graph::contract::validate_node_contract_result(&node, validation_mode)?;
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
