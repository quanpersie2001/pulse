use super::*;

impl JsonGraphStore {
    pub fn bootstrap(&self) -> PulseResult<()> {
        let _guard = WriteGuard::acquire(&self.repo_root)?;
        self.bootstrap_unlocked()?;
        recover_prepared_transactions(&self.repo_root)?;
        Ok(())
    }

    pub fn bootstrap_unlocked(&self) -> PulseResult<()> {
        // Fresh baseline initialization writes the node schema before durable
        // manifest/edge schema markers. If interrupted, only the safe current
        // partial layout may be completed; unknown existing state remains
        // refused without overwrite.
        match self.classify_workgraph_bootstrap_state()? {
            WorkgraphBootstrapState::Empty | WorkgraphBootstrapState::SafePartialCurrent => {
                self.ensure_current_workgraph_baseline_unlocked()?;
            }
            WorkgraphBootstrapState::ExistingCurrent => {
                self.ensure_workgraph_layout_unlocked()?;
            }
            WorkgraphBootstrapState::MissingNodeSchemaWithState => {
                return Err(PulseError::validation(
                    "node_schema_missing_refused",
                    "node schema is missing while existing workgraph state is present; refusing bootstrap without overwrite",
                ));
            }
            WorkgraphBootstrapState::NodeSchemaDrift { hash } => {
                return Err(PulseError::validation(
                    "node_schema_drift_refused",
                    format!(
                        "refusing to overwrite node schema drift {}; resolve schema state explicitly",
                        hash
                    ),
                ));
            }
            WorkgraphBootstrapState::UnexpectedPartialState => {
                return Err(PulseError::validation(
                    "workgraph_partial_state_refused",
                    "workgraph contains partial state that is not a safe current baseline initialization; refusing bootstrap without overwrite",
                ));
            }
        }
        Ok(())
    }

    /// Create the complete current baseline. The node schema is written first so
    /// a fresh initialization interrupted after durable marker writes remains a
    /// recognizable current partial layout.
    pub(crate) fn ensure_current_workgraph_baseline_unlocked(&self) -> PulseResult<()> {
        let wg = self.workgraph_dir();
        fs::create_dir_all(wg.join("schemas"))
            .map_err(|e| PulseError::io(wg.join("schemas"), e))?;
        self.write_current_node_schema_if_absent_unlocked()?;
        self.ensure_workgraph_layout_unlocked()
    }

    /// Create workgraph directories, manifest, and edge schema without touching an
    /// existing node schema. Used after classification has already proven the
    /// repository is either fresh or already on the current baseline.
    pub(crate) fn ensure_workgraph_layout_unlocked(&self) -> PulseResult<()> {
        let wg = self.workgraph_dir();
        fs::create_dir_all(wg.join("nodes")).map_err(|e| PulseError::io(wg.join("nodes"), e))?;
        fs::create_dir_all(wg.join("edges")).map_err(|e| PulseError::io(wg.join("edges"), e))?;
        fs::create_dir_all(wg.join("schemas"))
            .map_err(|e| PulseError::io(wg.join("schemas"), e))?;
        self.write_if_absent(&wg.join("manifest.json"), &Manifest::default())?;
        self.write_bytes_if_absent(&wg.join("schemas/edge.schema.json"), EDGE_SCHEMA.as_bytes())?;
        Ok(())
    }

    pub(crate) fn graph_fingerprint_current_unlocked(&self) -> PulseResult<String> {
        let manifest = self.manifest()?;
        let node_files = self.load_node_files_rel()?;
        let edge_files = self.load_edge_files_rel()?;
        graph_fingerprint(&manifest, &node_files, &edge_files)
    }

    pub(crate) fn graph_fingerprint_with_planned_workgraph(
        &self,
        node_override: &Node,
        edge_override: Option<&Edge>,
    ) -> PulseResult<String> {
        let manifest = self.manifest()?;
        let mut node_files = self.load_node_files_rel()?;
        let node_path = self.rel_path(&self.node_path(&node_override.id));
        let mut node_replaced = false;
        for (path, node) in &mut node_files {
            if path == &node_path {
                *node = node_override.clone();
                node_replaced = true;
                break;
            }
        }
        if !node_replaced {
            node_files.push((node_path, node_override.clone()));
        }
        node_files.sort_by(|left, right| left.0.cmp(&right.0));

        let mut edge_files = self.load_edge_files_rel()?;
        if let Some(edge) = edge_override {
            let edge_path = self.rel_path(&self.edge_path(&edge.id));
            let mut edge_replaced = false;
            for (path, existing) in &mut edge_files {
                if path == &edge_path {
                    *existing = edge.clone();
                    edge_replaced = true;
                    break;
                }
            }
            if !edge_replaced {
                edge_files.push((edge_path, edge.clone()));
            }
            edge_files.sort_by(|left, right| left.0.cmp(&right.0));
        }

        graph_fingerprint(&manifest, &node_files, &edge_files)
    }

    pub fn recover(&self) -> PulseResult<()> {
        let _guard = WriteGuard::acquire(&self.repo_root)?;
        // Recovery must not create layout/bootstrap files just to locate prepared
        // intents. Runtime transaction recovery alone is enough to roll forward
        // partial work.
        recover_prepared_transactions(&self.repo_root)?;
        Ok(())
    }

    pub(super) fn classify_workgraph_bootstrap_state(
        &self,
    ) -> PulseResult<WorkgraphBootstrapState> {
        classify_workgraph_bootstrap_state(&self.workgraph_dir())
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn commit_mutation(
        &self,
        event_type: &str,
        actor: String,
        subject: &str,
        payload: serde_json::Value,
        target_path: &Path,
        before: FileState,
        after: FileState,
        canonical_bytes: &[u8],
        now: DateTime<Utc>,
    ) -> PulseResult<()> {
        debug_assert_eq!(
            current_file_state(target_path, file_state_revision(&before))?,
            before
        );
        let event = EventEnvelope::new(
            new_event_id(),
            event_type,
            actor.clone(),
            subject,
            payload,
            now,
        );
        let intent = TransactionIntent::prepared(
            event.id.clone(),
            event_type,
            actor,
            target_path.to_path_buf(),
            event_path(&self.repo_root, &event),
            before,
            after,
            serde_json::to_value(event)?,
        )?;
        let prepared = prepare_transaction(&self.repo_root, intent)?;
        commit_prepared_transaction(&prepared, canonical_bytes, self.failpoint)
    }

    pub(crate) fn workgraph_dir(&self) -> PathBuf {
        self.repo_root.join(".pulse/workgraph")
    }

    pub(crate) fn node_path(&self, id: &str) -> PathBuf {
        self.workgraph_dir()
            .join("nodes")
            .join(format!("{id}.json"))
    }

    pub(crate) fn edge_path(&self, id: &str) -> PathBuf {
        self.workgraph_dir()
            .join("edges")
            .join(format!("{id}.json"))
    }

    pub(crate) fn manifest(&self) -> PulseResult<Manifest> {
        storage::read_json(&self.workgraph_dir().join("manifest.json"))
    }

    pub(super) fn validate_canonical_file<T: Serialize>(
        &self,
        path: &Path,
        value: &T,
        code: &'static str,
        report: &mut ValidationReport,
    ) {
        match (fs::read(path), to_canonical_bytes(value)) {
            (Ok(actual), Ok(expected)) if actual != expected => report.push_warning(
                code,
                format!("{} is not in canonical JSON byte form", path.display()),
            ),
            (Err(error), _) => report.push_error(
                "io_error",
                format!("cannot read {}: {error}", path.display()),
            ),
            (_, Err(error)) => report.push_error(error.code(), error.to_string()),
            _ => {}
        }
    }

    pub(super) fn validate_runtime_state(&self, report: &mut ValidationReport) {
        match recover_prepared_transactions(&self.repo_root) {
            Ok(actions) => {
                for action in actions {
                    report.push_warning(
                        "transaction_recovered",
                        format!("recovered local transaction state: {action:?}"),
                    );
                }
            }
            Err(error) => report.push_error(error.code(), error.to_string()),
        }
    }

    pub(super) fn validate_manifest_files(
        &self,
        manifest: &Manifest,
        report: &mut ValidationReport,
    ) {
        match crate::storage::paths::configured_content_root(
            &self.repo_root,
            &manifest.content_root,
        ) {
            Ok(root) => {
                match crate::storage::paths::configured_content_root(&self.repo_root, "../../works")
                {
                    Ok(expected) if root != expected => report.push_error(
                        "content_root_violation",
                        format!(
                            "manifest content_root must resolve to repository works/ root, got {}",
                            manifest.content_root
                        ),
                    ),
                    Ok(_) => {}
                    Err(error) => report.push_error(error.code(), error.to_string()),
                }
            }
            Err(error) => report.push_error(error.code(), error.to_string()),
        }
        if manifest.id_pattern != "^(EP|ST|TK|DEC)-[0-9]{3,}$" {
            report.push_error(
                "invalid_manifest",
                format!(
                    "manifest id_pattern is unsupported: {}",
                    manifest.id_pattern
                ),
            );
        }
        self.validate_schema_file(
            &manifest.node_schema,
            "node_schema_drift",
            NODE_SCHEMA,
            report,
        );
        self.validate_schema_file(
            &manifest.edge_schema,
            "edge_schema_drift",
            EDGE_SCHEMA,
            report,
        );
    }

    pub(super) fn write_current_node_schema_if_absent_unlocked(&self) -> PulseResult<()> {
        let path = self.workgraph_dir().join("schemas/node.schema.json");
        if !path.exists() {
            storage::atomic_write(&path, NODE_SCHEMA.as_bytes())?;
            return Ok(());
        }
        let current = fs::read(&path).map_err(|error| PulseError::io(&path, error))?;
        if current == NODE_SCHEMA.as_bytes() {
            return Ok(());
        }
        let current_hash = hash_bytes(&current);
        Err(PulseError::validation(
            "node_schema_drift_refused",
            format!(
                "refusing to overwrite node schema drift {}; resolve schema state explicitly",
                current_hash
            ),
        ))
    }

    pub(super) fn validate_schema_file(
        &self,
        schema_path: &str,
        drift_code: &'static str,
        expected_embedded_schema: &str,
        report: &mut ValidationReport,
    ) {
        let rel = match crate::storage::safe_repo_relative(schema_path) {
            Ok(rel) => rel,
            Err(e) => {
                report.push_error(e.code(), e.to_string());
                return;
            }
        };
        let full = self.workgraph_dir().join(rel);
        match fs::read(&full) {
            Ok(bytes) => match serde_json::from_slice::<serde_json::Value>(&bytes) {
                Ok(repo_schema) => {
                    match serde_json::from_str::<serde_json::Value>(expected_embedded_schema) {
                        Ok(embedded_schema) if repo_schema != embedded_schema => report.push_error(
                            drift_code,
                            format!(
                                "schema {} differs from embedded schema template",
                                full.display()
                            ),
                        ),
                        Ok(_) => match to_canonical_bytes(&repo_schema) {
                            Ok(canonical) if canonical != bytes => report.push_warning(
                                "schema_canonical_drift",
                                format!(
                                    "schema {} is not in canonical JSON byte form",
                                    full.display()
                                ),
                            ),
                            Err(error) => report.push_error(error.code(), error.to_string()),
                            _ => {}
                        },
                        Err(e) => report.push_error(
                            "embedded_schema_parse_error",
                            format!("embedded schema is not valid JSON: {e}"),
                        ),
                    }
                }
                Err(e) => report.push_error(
                    "schema_parse_error",
                    format!("schema {} is not valid JSON: {}", full.display(), e),
                ),
            },
            Err(e) => report.push_error(
                "schema_missing",
                format!("cannot read schema {}: {}", full.display(), e),
            ),
        }
    }

    pub(super) fn allocate_id(&self, kind: WorkKind) -> PulseResult<String> {
        let prefix = kind.prefix();
        let mut max = 0;
        for entry in fs::read_dir(self.workgraph_dir().join("nodes"))
            .map_err(|e| PulseError::io(self.workgraph_dir().join("nodes"), e))?
        {
            let entry = entry.map_err(|e| PulseError::io(self.workgraph_dir().join("nodes"), e))?;
            let Some(stem) = entry
                .path()
                .file_stem()
                .and_then(|s| s.to_str())
                .map(str::to_string)
            else {
                continue;
            };
            if let Some(n) = parse_numeric(&stem, prefix) {
                max = max.max(n);
            }
        }
        Ok(format_id(kind, max + 1))
    }

    pub(crate) fn load_nodes(&self) -> PulseResult<BTreeMap<String, Node>> {
        let mut out = BTreeMap::new();
        for (_, node) in self.load_node_files()? {
            out.insert(node.id.clone(), node);
        }
        Ok(out)
    }

    pub(crate) fn load_nodes_with_override(
        &self,
        node: Node,
    ) -> PulseResult<BTreeMap<String, Node>> {
        let mut nodes = self.load_nodes()?;
        nodes.insert(node.id.clone(), node);
        Ok(nodes)
    }

    pub(crate) fn load_edges(&self) -> PulseResult<Vec<(PathBuf, Edge)>> {
        self.load_edge_files()
    }

    pub(crate) fn load_node_files(&self) -> PulseResult<Vec<(PathBuf, Node)>> {
        let dir = self.workgraph_dir().join("nodes");
        let mut out = vec![];
        if !dir.exists() {
            return Ok(out);
        }
        for entry in fs::read_dir(&dir).map_err(|e| PulseError::io(&dir, e))? {
            let entry = entry.map_err(|e| PulseError::io(&dir, e))?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("json") {
                out.push((path.clone(), storage::read_json(&path)?));
            }
        }
        out.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(out)
    }

    pub(crate) fn load_edge_files(&self) -> PulseResult<Vec<(PathBuf, Edge)>> {
        let dir = self.workgraph_dir().join("edges");
        let mut out = vec![];
        if !dir.exists() {
            return Ok(out);
        }
        for entry in fs::read_dir(&dir).map_err(|e| PulseError::io(&dir, e))? {
            let entry = entry.map_err(|e| PulseError::io(&dir, e))?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("json") {
                out.push((path.clone(), storage::read_json(&path)?));
            }
        }
        out.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(out)
    }

    pub(crate) fn load_node_files_rel(&self) -> PulseResult<Vec<(PathBuf, Node)>> {
        Ok(self
            .load_node_files()?
            .into_iter()
            .map(|(p, n)| (self.rel_path(&p), n))
            .collect())
    }

    pub(crate) fn load_edge_files_rel(&self) -> PulseResult<Vec<(PathBuf, Edge)>> {
        Ok(self
            .load_edge_files()?
            .into_iter()
            .map(|(p, e)| (self.rel_path(&p), e))
            .collect())
    }

    pub(crate) fn rel_path(&self, path: &Path) -> PathBuf {
        path.strip_prefix(&self.repo_root)
            .unwrap_or(path)
            .to_path_buf()
    }

    pub(super) fn write_if_absent<T: Serialize>(&self, path: &Path, value: &T) -> PulseResult<()> {
        if path.exists() {
            return Ok(());
        }
        storage::atomic_write(path, &to_canonical_bytes(value)?)
    }

    pub(super) fn write_bytes_if_absent(&self, path: &Path, bytes: &[u8]) -> PulseResult<()> {
        if path.exists() {
            return Ok(());
        }
        storage::atomic_write(path, bytes)
    }
}

pub(super) enum WorkgraphBootstrapState {
    Empty,
    SafePartialCurrent,
    ExistingCurrent,
    MissingNodeSchemaWithState,
    NodeSchemaDrift { hash: String },
    UnexpectedPartialState,
}

struct WorkgraphBootstrapInspection {
    has_manifest: bool,
    has_node_schema: bool,
    has_edge_schema: bool,
    has_node_files: bool,
    has_edge_files: bool,
    has_only_safe_entries: bool,
    manifest_matches: bool,
    node_schema_matches: Option<bool>,
    edge_schema_matches: bool,
    node_schema_hash: Option<String>,
}

impl WorkgraphBootstrapInspection {
    pub(super) fn has_any_current_marker(&self) -> bool {
        self.has_manifest || self.has_node_schema || self.has_edge_schema
    }

    pub(super) fn all_present_markers_match(&self) -> bool {
        self.manifest_matches && self.node_schema_matches != Some(false) && self.edge_schema_matches
    }
}

fn classify_workgraph_bootstrap_state(wg: &Path) -> PulseResult<WorkgraphBootstrapState> {
    let inspection = inspect_workgraph_bootstrap_state(wg)?;
    if !inspection.has_only_safe_entries {
        return Ok(WorkgraphBootstrapState::UnexpectedPartialState);
    }
    if inspection.node_schema_matches == Some(false) {
        return Ok(WorkgraphBootstrapState::NodeSchemaDrift {
            hash: inspection.node_schema_hash.unwrap_or_default(),
        });
    }
    if !inspection.all_present_markers_match() {
        return Ok(WorkgraphBootstrapState::UnexpectedPartialState);
    }
    if inspection.has_node_files || inspection.has_edge_files {
        return Ok(if inspection.has_node_schema {
            WorkgraphBootstrapState::ExistingCurrent
        } else {
            WorkgraphBootstrapState::MissingNodeSchemaWithState
        });
    }
    if inspection.has_any_current_marker() {
        return Ok(
            if inspection.has_manifest && inspection.has_node_schema && inspection.has_edge_schema {
                WorkgraphBootstrapState::ExistingCurrent
            } else {
                WorkgraphBootstrapState::SafePartialCurrent
            },
        );
    }
    Ok(WorkgraphBootstrapState::Empty)
}

fn inspect_workgraph_bootstrap_state(wg: &Path) -> PulseResult<WorkgraphBootstrapInspection> {
    if !wg.exists() {
        return Ok(WorkgraphBootstrapInspection {
            has_manifest: false,
            has_node_schema: false,
            has_edge_schema: false,
            has_node_files: false,
            has_edge_files: false,
            has_only_safe_entries: true,
            manifest_matches: true,
            node_schema_matches: None,
            edge_schema_matches: true,
            node_schema_hash: None,
        });
    }

    let manifest_path = wg.join("manifest.json");
    let node_schema_path = wg.join("schemas/node.schema.json");
    let edge_schema_path = wg.join("schemas/edge.schema.json");
    let manifest_matches =
        current_marker_matches(&manifest_path, &to_canonical_bytes(&Manifest::default())?)?;
    let node_schema_bytes = read_optional_bytes(&node_schema_path)?;
    let node_schema_matches = node_schema_bytes
        .as_deref()
        .map(|current| current == NODE_SCHEMA.as_bytes());
    let node_schema_hash = node_schema_bytes
        .as_deref()
        .filter(|current| *current != NODE_SCHEMA.as_bytes())
        .map(hash_bytes);
    Ok(WorkgraphBootstrapInspection {
        has_manifest: manifest_path.exists(),
        has_node_schema: node_schema_path.exists(),
        has_edge_schema: edge_schema_path.exists(),
        has_node_files: directory_has_json_files(&wg.join("nodes"))?,
        has_edge_files: directory_has_json_files(&wg.join("edges"))?,
        has_only_safe_entries: workgraph_subtree_has_only_allowed_entries(
            wg,
            &[
                "manifest.json",
                "schemas",
                "schemas/node.schema.json",
                "schemas/edge.schema.json",
                "nodes",
                "edges",
            ],
        )?,
        manifest_matches,
        node_schema_matches,
        edge_schema_matches: current_marker_matches(&edge_schema_path, EDGE_SCHEMA.as_bytes())?,
        node_schema_hash,
    })
}

fn current_marker_matches(path: &Path, expected: &[u8]) -> PulseResult<bool> {
    Ok(match read_optional_bytes(path)? {
        Some(current) => current == expected,
        None => true,
    })
}

fn read_optional_bytes(path: &Path) -> PulseResult<Option<Vec<u8>>> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(PulseError::io(path, error)),
    }
}

fn directory_has_json_files(dir: &Path) -> PulseResult<bool> {
    if !dir.exists() {
        return Ok(false);
    }
    for entry in fs::read_dir(dir).map_err(|e| PulseError::io(dir, e))? {
        let entry = entry.map_err(|e| PulseError::io(dir, e))?;
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) == Some("json") {
            return Ok(true);
        }
    }
    Ok(false)
}

fn workgraph_subtree_has_only_allowed_entries(root: &Path, allowed: &[&str]) -> PulseResult<bool> {
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).map_err(|e| PulseError::io(&dir, e))? {
            let entry = entry.map_err(|e| PulseError::io(&dir, e))?;
            let path = entry.path();
            let Ok(relative) = path.strip_prefix(root) else {
                return Ok(false);
            };
            let relative = relative.to_string_lossy();
            if !allowed.iter().any(|candidate| *candidate == relative)
                && !relative.starts_with("nodes/")
                && !relative.starts_with("edges/")
            {
                return Ok(false);
            }
            if entry
                .file_type()
                .map_err(|e| PulseError::io(&path, e))?
                .is_dir()
            {
                stack.push(path);
            }
        }
    }
    Ok(true)
}

fn file_state_revision(state: &FileState) -> Option<u64> {
    match state {
        FileState::Absent => None,
        FileState::Present { revision, .. } => Some(*revision),
    }
}
