use super::*;

impl JsonGraphStore {
    pub fn add_edge_with_context(
        &self,
        edge_type: EdgeType,
        from: String,
        to: String,
        ctx: OperationContext,
    ) -> PulseResult<MutationOutcome<Edge>> {
        if edge_type == EdgeType::SupersededBy {
            return Err(PulseError::validation(
                "superseded_by_lifecycle_owned",
                "superseded_by edges are lifecycle-owned; use pulse work supersede",
            ));
        }
        let _guard = WriteGuard::acquire(&self.repo_root)?;
        self.bootstrap_unlocked()?;
        recover_prepared_transactions(&self.repo_root)?;
        let (from, to) = canonical_endpoints(edge_type, from, to);
        let id = deterministic_edge_id(edge_type, &from, &to);
        let path = self.edge_path(&id);
        if path.exists() {
            let existing: Edge = storage::read_json(&path)?;
            if existing.edge_type == edge_type && existing.from == from && existing.to == to {
                return Ok(MutationOutcome {
                    schema_version: 1,
                    code: "unchanged".to_string(),
                    status: MutationStatus::Unchanged,
                    value: existing,
                });
            }
            return Err(PulseError::validation(
                "edge_identity_conflict",
                format!("edge id {id} already exists with different payload"),
            ));
        }
        let edge = Edge::new(edge_type, from, to, ctx.actor.clone(), ctx.now)?;
        let nodes = self.load_nodes()?;
        let edges = self
            .load_edges()?
            .into_iter()
            .map(|(_, e)| e)
            .collect::<Vec<_>>();
        validate_edge_for_add(&nodes, &edges, &edge)?;
        let after_bytes = to_canonical_bytes(&edge)?;
        self.commit_mutation(
            "work.edge.created",
            ctx.actor,
            &edge.id,
            json!({"edge": edge}),
            &path,
            FileState::Absent,
            FileState::Present {
                hash: hash_bytes(&after_bytes),
                revision: edge.revision,
            },
            &after_bytes,
            ctx.now,
        )?;
        Ok(MutationOutcome {
            schema_version: 1,
            code: "created".to_string(),
            status: MutationStatus::Created,
            value: edge,
        })
    }

    pub fn add_edge(
        &self,
        edge_type: EdgeType,
        from: String,
        to: String,
        actor: String,
    ) -> PulseResult<MutationOutcome<Edge>> {
        self.add_edge_with_context(
            edge_type,
            from,
            to,
            OperationContext {
                actor,
                now: Utc::now(),
            },
        )
    }

    pub fn validate(&self) -> PulseResult<ValidationReport> {
        let _guard = WriteGuard::acquire(&self.repo_root)?;
        self.bootstrap_unlocked()?;
        recover_prepared_transactions(&self.repo_root)?;
        let manifest = self.manifest()?;
        let node_files = self.load_node_files()?;
        let edge_files = self.load_edge_files()?;
        let node_values = node_files
            .iter()
            .map(|(_, n)| n.clone())
            .collect::<Vec<_>>();
        let edge_values = edge_files
            .iter()
            .map(|(_, e)| e.clone())
            .collect::<Vec<_>>();
        let mut report = validate_graph(&self.repo_root, &manifest, &node_values, &edge_values);
        self.validate_manifest_files(&manifest, &mut report);
        for (path, node) in &node_files {
            if let Err(e) = validate_node_filename(path, node) {
                report.push_error(e.code(), e.to_string());
            }
            self.validate_canonical_file(path, node, "node_canonical_drift", &mut report);
            if !self.repo_root.join(&node.content_dir).exists() {
                report.push_warning(
                    "missing_draft_content_dir",
                    format!("draft content directory missing: {}", node.content_dir),
                );
            }
        }
        for (path, edge) in &edge_files {
            if let Err(e) = validate_edge_filename(path, edge) {
                report.push_error(e.code(), e.to_string());
            }
            self.validate_canonical_file(path, edge, "edge_canonical_drift", &mut report);
        }
        self.validate_runtime_state(&mut report);
        Ok(report)
    }

    pub fn export(&self) -> PulseResult<GraphProjection> {
        let _guard = WriteGuard::acquire(&self.repo_root)?;
        self.bootstrap_unlocked()?;
        recover_prepared_transactions(&self.repo_root)?;
        self.export_unlocked()
    }

    /// Build the graph projection assuming the repository fence is already held
    /// and transactions recovered. Used by readiness/gate evaluation which runs
    /// inside a transition's held guard and must not re-acquire the flock.
    pub fn export_unlocked(&self) -> PulseResult<GraphProjection> {
        let manifest = self.manifest()?;
        let node_files = self.load_node_files()?;
        let edge_files = self.load_edge_files()?;
        let node_values = node_files
            .iter()
            .map(|(_, n)| n.clone())
            .collect::<Vec<_>>();
        let edge_values = edge_files
            .iter()
            .map(|(_, e)| e.clone())
            .collect::<Vec<_>>();
        validate_graph(&self.repo_root, &manifest, &node_values, &edge_values).into_result()?;
        let node_files = self.load_node_files_rel()?;
        let edge_files = self.load_edge_files_rel()?;
        super::export_with_cache(&self.repo_root, &manifest, &node_files, &edge_files)
    }

    pub fn executability(&self, id: &str) -> PulseResult<StructuralExecutabilityReport> {
        let projection = self.export()?;
        structural_executability(&projection, id)
    }

    pub fn rollup(&self, id: &str) -> PulseResult<RollupReport> {
        let projection = self.export()?;
        rollup(&projection, id)
    }

    pub fn neighborhood(&self, id: &str, depth: usize) -> PulseResult<NeighborhoodReport> {
        let projection = self.export()?;
        neighborhood(&projection, id, depth)
    }

    pub fn affected_by(
        &self,
        id: &str,
        relation_filter: Option<EdgeType>,
    ) -> PulseResult<AffectedByReport> {
        let projection = self.export()?;
        affected_by(&projection, id, relation_filter)
    }
}
