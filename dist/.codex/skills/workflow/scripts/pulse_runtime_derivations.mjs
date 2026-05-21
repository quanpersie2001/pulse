import { firstNonEmptyString } from "./core/strings.mjs";
import {
  inferGateNextAction,
  inferGateNextSkillRecommended,
} from "./pulse_recommendations.mjs";

export const CURRENT_FEATURE_SCHEMA_VERSION = "1.0";
export const RUNTIME_SNAPSHOT_SCHEMA_VERSION = "1.0";

function utcNow() {
  return new Date().toISOString();
}

function normalizeFeaturePointer(value) {
  const normalized = typeof value === "string" ? value.trim() : "";
  return normalized === "(none)" ? "" : normalized;
}

export function buildCurrentFeatureRecord(status) {
  const featureKey = firstNonEmptyString(
    normalizeFeaturePointer(status.state_json?.active_feature),
    normalizeFeaturePointer(status.state_markdown?.focus),
    normalizeFeaturePointer(status.current_feature?.feature_key),
  );
  const phase = firstNonEmptyString(
    status.state_json?.phase,
    status.state_markdown?.phase,
    status.runtime_snapshot?.phase,
    status.current_feature?.phase,
    featureKey ? "idle" : "",
  );
  const gate = firstNonEmptyString(
    status.state_markdown?.gate,
    status.state_json?.gate,
    status.current_feature?.gate,
  );
  const gateStatus = firstNonEmptyString(
    status.state_markdown?.gate_status,
    status.state_json?.gate_status,
    status.current_feature?.gate_status,
    status.runtime_snapshot?.gate_status,
  );
  const workShapeStatus = firstNonEmptyString(
    status.state_markdown?.work_shape_status,
    status.state_json?.work_shape_status,
    status.current_feature?.work_shape_status,
    status.runtime_snapshot?.work_shape_status,
  );
  const shapeArtifact = firstNonEmptyString(
    status.state_markdown?.shape_artifact,
    status.state_json?.shape_artifact,
    status.current_feature?.shape_artifact,
    status.runtime_snapshot?.shape_artifact,
  );
  const currentWorkId = firstNonEmptyString(
    status.state_markdown?.current_work_id,
    status.state_json?.current_work_id,
    status.current_feature?.current_work_id,
    status.runtime_snapshot?.current_work_id,
  );
  const currentWorkStatus = firstNonEmptyString(
    status.state_markdown?.current_work_status,
    status.state_json?.current_work_status,
    status.current_feature?.current_work_status,
    status.runtime_snapshot?.current_work_status,
  );
  const feasibilityStatus = firstNonEmptyString(
    status.state_markdown?.feasibility_status,
    status.state_json?.feasibility_status,
    status.current_feature?.feasibility_status,
    status.runtime_snapshot?.feasibility_status,
  );
  const readinessStatus = firstNonEmptyString(
    status.state_markdown?.readiness_status,
    status.state_json?.readiness_status,
    status.current_feature?.readiness_status,
    status.runtime_snapshot?.readiness_status,
  );
  const reviewStatus = firstNonEmptyString(
    status.state_markdown?.review_status,
    status.state_json?.review_status,
    status.current_feature?.review_status,
    status.runtime_snapshot?.review_status,
  );
  const currentStatus = featureKey
    ? (status.current_feature?.status && status.current_feature.status !== "idle"
        ? status.current_feature.status
        : "active")
    : firstNonEmptyString(status.current_feature?.status, "idle");
  const nextSkillRecommended = inferGateNextSkillRecommended(status, gate, gateStatus);
  const nextAction = inferGateNextAction(status, gateStatus, nextSkillRecommended);

  return {
    schema_version: CURRENT_FEATURE_SCHEMA_VERSION,
    feature_key: featureKey,
    phase,
    gate,
    gate_status: gateStatus,
    work_shape_status: workShapeStatus,
    shape_artifact: shapeArtifact,
    current_work_id: currentWorkId,
    current_work_status: currentWorkStatus,
    feasibility_status: feasibilityStatus,
    readiness_status: readinessStatus,
    review_status: reviewStatus,
    status: currentStatus,
    next_action: nextAction,
    next_skill_recommended: nextSkillRecommended,
    updated_at: utcNow(),
  };
}

export function buildRuntimeSnapshotRecord(status) {
  const source = {
    state_json: ".pulse/runtime/state.json",
    state_markdown: ".pulse/runtime/STATE.md",
  };
  const gate = firstNonEmptyString(
    status.current_feature?.gate,
    status.state_markdown?.gate,
    status.state_json?.gate,
  );
  const gateStatus = firstNonEmptyString(
    status.current_feature?.gate_status,
    status.state_markdown?.gate_status,
    status.state_json?.gate_status,
    status.runtime_snapshot?.gate_status,
  );
  const nextSkillRecommended = inferGateNextSkillRecommended(status, gate, gateStatus);
  const nextAction = inferGateNextAction(status, gateStatus, nextSkillRecommended);

  return {
    schema_version: RUNTIME_SNAPSHOT_SCHEMA_VERSION,
    active_feature: firstNonEmptyString(
      normalizeFeaturePointer(status.state_json?.active_feature),
      normalizeFeaturePointer(status.state_markdown?.focus),
      normalizeFeaturePointer(status.current_feature?.feature_key),
    ),
    active_skill: firstNonEmptyString(status.state_json?.active_skill, "pulse"),
    phase: firstNonEmptyString(
      status.current_feature?.phase,
      status.state_json?.phase,
      status.state_markdown?.phase,
      "idle",
    ),
    gate,
    gate_status: gateStatus,
    work_shape_status: firstNonEmptyString(
      status.current_feature?.work_shape_status,
      status.state_json?.work_shape_status,
      status.state_markdown?.work_shape_status,
    ),
    shape_artifact: firstNonEmptyString(
      status.current_feature?.shape_artifact,
      status.state_json?.shape_artifact,
      status.state_markdown?.shape_artifact,
    ),
    current_work_id: firstNonEmptyString(
      status.current_feature?.current_work_id,
      status.state_json?.current_work_id,
      status.state_markdown?.current_work_id,
    ),
    current_work_status: firstNonEmptyString(
      status.current_feature?.current_work_status,
      status.state_json?.current_work_status,
      status.state_markdown?.current_work_status,
    ),
    feasibility_status: firstNonEmptyString(
      status.current_feature?.feasibility_status,
      status.state_json?.feasibility_status,
      status.state_markdown?.feasibility_status,
    ),
    readiness_status: firstNonEmptyString(
      status.current_feature?.readiness_status,
      status.state_json?.readiness_status,
      status.state_markdown?.readiness_status,
    ),
    review_status: firstNonEmptyString(
      status.current_feature?.review_status,
      status.state_json?.review_status,
      status.state_markdown?.review_status,
    ),
    requested_mode: firstNonEmptyString(
      status.tooling_status?.requested_mode,
      status.state_json?.requested_mode,
    ),
    recommended_mode: firstNonEmptyString(
      status.tooling_status?.recommended_mode,
      status.state_json?.recommended_mode,
    ),
    next_action: nextAction,
    next_skill_recommended: nextSkillRecommended,
    updated_at: utcNow(),
    source,
  };
}

export function summarizeCurrentFeature(currentFeature) {
  if (!currentFeature || typeof currentFeature !== "object" || Array.isArray(currentFeature)) {
    return {
      exists: false,
      feature_key: "",
      phase: "",
      gate: "",
      gate_status: "",
      work_shape_status: "",
      shape_artifact: "",
      current_work_id: "",
      current_work_status: "",
      feasibility_status: "",
      readiness_status: "",
      review_status: "",
      updated_at: "",
      status: "",
      next_action: "",
      next_skill_recommended: "",
    };
  }

  return {
    exists: true,
    feature_key: typeof currentFeature.feature_key === "string" ? currentFeature.feature_key : "",
    phase: typeof currentFeature.phase === "string" ? currentFeature.phase : "",
    gate: typeof currentFeature.gate === "string" ? currentFeature.gate : "",
    gate_status: typeof currentFeature.gate_status === "string" ? currentFeature.gate_status : "",
    work_shape_status: typeof currentFeature.work_shape_status === "string" ? currentFeature.work_shape_status : "",
    shape_artifact: typeof currentFeature.shape_artifact === "string" ? currentFeature.shape_artifact : "",
    current_work_id: typeof currentFeature.current_work_id === "string" ? currentFeature.current_work_id : "",
    current_work_status: typeof currentFeature.current_work_status === "string" ? currentFeature.current_work_status : "",
    feasibility_status: typeof currentFeature.feasibility_status === "string" ? currentFeature.feasibility_status : "",
    readiness_status: typeof currentFeature.readiness_status === "string" ? currentFeature.readiness_status : "",
    review_status: typeof currentFeature.review_status === "string" ? currentFeature.review_status : "",
    updated_at: typeof currentFeature.updated_at === "string" ? currentFeature.updated_at : "",
    status: typeof currentFeature.status === "string" ? currentFeature.status : "",
    next_action: typeof currentFeature.next_action === "string" ? currentFeature.next_action : "",
    next_skill_recommended: typeof currentFeature.next_skill_recommended === "string"
      ? currentFeature.next_skill_recommended
      : "",
  };
}

export function summarizeRuntimeSnapshot(runtimeSnapshot) {
  if (!runtimeSnapshot || typeof runtimeSnapshot !== "object" || Array.isArray(runtimeSnapshot)) {
    return {
      exists: false,
      schema_version: "",
      active_feature: "",
      active_skill: "",
      phase: "",
      gate: "",
      gate_status: "",
      work_shape_status: "",
      shape_artifact: "",
      current_work_id: "",
      current_work_status: "",
      feasibility_status: "",
      readiness_status: "",
      review_status: "",
      requested_mode: "",
      recommended_mode: "",
      next_action: "",
      next_skill_recommended: "",
      updated_at: "",
      source: null,
    };
  }

  const source = runtimeSnapshot.source && typeof runtimeSnapshot.source === "object"
    ? {
        state_json: typeof runtimeSnapshot.source.state_json === "string"
          ? runtimeSnapshot.source.state_json
          : "",
        state_markdown: typeof runtimeSnapshot.source.state_markdown === "string"
          ? runtimeSnapshot.source.state_markdown
          : "",
        current_feature: typeof runtimeSnapshot.source.current_feature === "string"
          ? runtimeSnapshot.source.current_feature
          : "",
      }
    : null;

  return {
    exists: true,
    schema_version: typeof runtimeSnapshot.schema_version === "string"
      ? runtimeSnapshot.schema_version
      : "",
    active_feature: typeof runtimeSnapshot.active_feature === "string"
      ? runtimeSnapshot.active_feature
      : "",
    active_skill: typeof runtimeSnapshot.active_skill === "string" ? runtimeSnapshot.active_skill : "",
    phase: typeof runtimeSnapshot.phase === "string" ? runtimeSnapshot.phase : "",
    gate: typeof runtimeSnapshot.gate === "string" ? runtimeSnapshot.gate : "",
    gate_status: typeof runtimeSnapshot.gate_status === "string" ? runtimeSnapshot.gate_status : "",
    work_shape_status: typeof runtimeSnapshot.work_shape_status === "string" ? runtimeSnapshot.work_shape_status : "",
    shape_artifact: typeof runtimeSnapshot.shape_artifact === "string" ? runtimeSnapshot.shape_artifact : "",
    current_work_id: typeof runtimeSnapshot.current_work_id === "string" ? runtimeSnapshot.current_work_id : "",
    current_work_status: typeof runtimeSnapshot.current_work_status === "string" ? runtimeSnapshot.current_work_status : "",
    feasibility_status: typeof runtimeSnapshot.feasibility_status === "string" ? runtimeSnapshot.feasibility_status : "",
    readiness_status: typeof runtimeSnapshot.readiness_status === "string" ? runtimeSnapshot.readiness_status : "",
    review_status: typeof runtimeSnapshot.review_status === "string" ? runtimeSnapshot.review_status : "",
    requested_mode: typeof runtimeSnapshot.requested_mode === "string"
      ? runtimeSnapshot.requested_mode
      : "",
    recommended_mode: typeof runtimeSnapshot.recommended_mode === "string"
      ? runtimeSnapshot.recommended_mode
      : "",
    next_action: typeof runtimeSnapshot.next_action === "string" ? runtimeSnapshot.next_action : "",
    next_skill_recommended: typeof runtimeSnapshot.next_skill_recommended === "string"
      ? runtimeSnapshot.next_skill_recommended
      : "",
    updated_at: typeof runtimeSnapshot.updated_at === "string" ? runtimeSnapshot.updated_at : "",
    source,
  };
}
