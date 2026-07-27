use super::*;

impl JsonGraphStore {
    pub fn set_contract_with_context(
        &self,
        ticket_id: &str,
        expected_revision: u64,
        request: ContractSetRequest,
        ctx: OperationContext,
    ) -> PulseResult<MutationOutcome<Node>> {
        let role_contract_present = match request.role {
            TicketRole::Implementation => request.implementation.is_some(),
            TicketRole::DecisionWork => request.decision_work.is_some(),
        };
        if !role_contract_present {
            return Err(PulseError::validation(
                "implementation_contract_missing",
                "contract set request must supply the contract matching the declared role",
            ));
        }
        if request.role == TicketRole::Implementation && request.decision_work.is_some() {
            return Err(PulseError::validation(
                "work_role_invalid",
                "implementation role must not carry a decision_work contract",
            ));
        }
        if request.role == TicketRole::DecisionWork && request.implementation.is_some() {
            return Err(PulseError::validation(
                "work_role_invalid",
                "decision_work role must not carry an implementation contract",
            ));
        }

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
                "work_role_invalid",
                format!("contract can only be set on tickets: {ticket_id}"),
            ));
        }
        if node.revision != expected_revision {
            return Err(PulseError::CasConflict {
                subject: ticket_id.to_string(),
                expected_revision,
                current_revision: node.revision,
            });
        }
        if node.role != Some(request.role) {
            return Err(PulseError::validation(
                "work_role_invalid",
                format!(
                    "contract role {:?} does not match ticket role {:?}",
                    request.role, node.role
                ),
            ));
        }

        let previous_implementation = node.implementation.clone();
        let previous_decision_work = node.decision_work.clone();
        node.normalize_contract_fields();
        match request.role {
            TicketRole::Implementation => {
                node.implementation = request.implementation;
                node.decision_work = None;
            }
            TicketRole::DecisionWork => {
                node.decision_work = request.decision_work;
                node.implementation = None;
            }
        }
        node.normalize_contract_fields();
        node.contract_revision += 1;
        node.revision += 1;
        node.updated_at = ctx.now;

        validate_node_contract_result(&node, ContractValidationMode::CanonicalStorage)?;
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
            "work.contract.updated",
            ctx.actor,
            ticket_id,
            json!({
                "ticket_id": ticket_id,
                "expected_revision": expected_revision,
                "new_revision": node.revision,
                "previous_contract_revision": node.contract_revision.saturating_sub(1),
                "new_contract_revision": node.contract_revision,
                "role": request.role,
                "previous_implementation": previous_implementation,
                "previous_decision_work": previous_decision_work,
                "implementation": node.implementation,
                "decision_work": node.decision_work,
                "gate_coverage": ["ticket_kind", "node_revision_cas", "role_match", "contract_validation", "graph_integrity"]
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

    pub fn set_contract(
        &self,
        ticket_id: &str,
        expected_revision: u64,
        request: ContractSetRequest,
        actor: String,
    ) -> PulseResult<MutationOutcome<Node>> {
        self.set_contract_with_context(
            ticket_id,
            expected_revision,
            request,
            OperationContext {
                actor,
                now: Utc::now(),
            },
        )
    }

    /// Minimal readiness-only QA impact posture mutation.
    ///
    /// QA impact is a semantic contract input: mutation bumps both `revision`
    /// and `contract_revision`. The `none` and `covered_by_story_close`
    /// postures are authority-gated against the local default-deny policy.
    pub fn set_qa_impact_with_context(
        &self,
        ticket_id: &str,
        expected_revision: u64,
        update: QaImpactUpdate,
        ctx: OperationContext,
    ) -> PulseResult<MutationOutcome<Node>> {
        let qa = QaMetadata {
            impact: crate::graph::contract::QaImpact {
                posture: update.posture,
                rationale: update.rationale,
                behavioral_owner: update.behavioral_owner,
                affected_case_ids: update.affected_case_ids,
            },
        };

        let required_grants: Vec<&str> = match update.posture {
            QaImpactPosture::None => vec!["qa.none.approve"],
            QaImpactPosture::CoveredByStoryClose => vec!["qa.defer_to_story_close"],
            QaImpactPosture::Unknown | QaImpactPosture::Required => vec![],
        };

        let _guard = WriteGuard::acquire(&self.repo_root)?;
        self.bootstrap_unlocked()?;
        recover_prepared_transactions(&self.repo_root)?;
        // Authority is resolved under the held fence so a concurrent policy
        // revocation cannot authorize a stale grant (consistency with shaping
        // apply/invalidate and the ready transition).
        if !required_grants.is_empty() {
            let report = crate::policy::load_authority_policy(&self.repo_root)?;
            let actor = crate::policy::parse_actor(&ctx.actor);
            crate::policy::authorize(&report, &actor, &required_grants)?;
        }
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
                "qa_impact_invalid",
                format!("qa impact can only be set on tickets: {ticket_id}"),
            ));
        }
        if node.revision != expected_revision {
            return Err(PulseError::CasConflict {
                subject: ticket_id.to_string(),
                expected_revision,
                current_revision: node.revision,
            });
        }

        let previous_qa = node.qa.clone();
        node.qa = Some(qa.clone());
        node.normalize_contract_fields();
        node.contract_revision += 1;
        node.revision += 1;
        node.updated_at = ctx.now;

        validate_node_contract_result(&node, ContractValidationMode::CanonicalStorage)?;
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
            "work.qa_impact.updated",
            ctx.actor,
            ticket_id,
            json!({
                "ticket_id": ticket_id,
                "expected_revision": expected_revision,
                "new_revision": node.revision,
                "previous_contract_revision": node.contract_revision.saturating_sub(1),
                "new_contract_revision": node.contract_revision,
                "previous_qa": previous_qa,
                "qa": qa,
                "gate_coverage": ["ticket_kind", "node_revision_cas", "qa_impact_validation", "authority", "graph_integrity"]
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

    pub fn set_qa_impact(
        &self,
        ticket_id: &str,
        expected_revision: u64,
        update: QaImpactUpdate,
        actor: String,
    ) -> PulseResult<MutationOutcome<Node>> {
        self.set_qa_impact_with_context(
            ticket_id,
            expected_revision,
            update,
            OperationContext {
                actor,
                now: Utc::now(),
            },
        )
    }

    pub fn show_contract(&self, ticket_id: &str) -> PulseResult<ContractView> {
        let node = self.show_node(ticket_id)?;
        Ok(ContractView {
            schema_version: 1,
            code: "ok".to_string(),
            ticket_id: node.id,
            revision: node.revision,
            contract_revision: node.contract_revision,
            role: node.role,
            implementation: node.implementation,
            decision_work: node.decision_work,
        })
    }

    pub fn show_qa_impact(&self, ticket_id: &str) -> PulseResult<QaImpactView> {
        let node = self.show_node(ticket_id)?;
        Ok(QaImpactView {
            schema_version: 1,
            code: "ok".to_string(),
            ticket_id: node.id,
            revision: node.revision,
            contract_revision: node.contract_revision,
            qa: node.qa,
        })
    }
}
