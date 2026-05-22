import { canonicalizeItemRecord } from "./model.mjs";

function sortByCreatedThenId(items) {
  return [...items].sort((left, right) => {
    const createdCompare = String(left.created_at || "").localeCompare(String(right.created_at || ""));
    if (createdCompare !== 0) {
      return createdCompare;
    }
    return String(left.id || "").localeCompare(String(right.id || ""));
  });
}

function sortReadyItems(items) {
  return [...items].sort((left, right) => {
    const priorityCompare = Number(left.priority ?? 2) - Number(right.priority ?? 2);
    if (priorityCompare !== 0) {
      return priorityCompare;
    }

    const createdCompare = String(left.created_at || "").localeCompare(String(right.created_at || ""));
    if (createdCompare !== 0) {
      return createdCompare;
    }

    return String(left.id || "").localeCompare(String(right.id || ""));
  });
}

function computeDescendantCounts(childrenByParent) {
  const memo = new Map();

  function countDescendants(itemId) {
    if (memo.has(itemId)) {
      return memo.get(itemId);
    }

    const directChildren = childrenByParent.get(itemId) || [];
    const total = directChildren.length + directChildren.reduce((sum, childId) => sum + countDescendants(childId), 0);
    memo.set(itemId, total);
    return total;
  }

  return countDescendants;
}

export function deriveViewState(items) {
  const records = [...(items || [])].map((item) => canonicalizeItemRecord(item));
  const recordsById = new Map(records.map((item) => [item.id, item]));
  const childrenByParent = new Map();
  const reverseDependencyMap = new Map(records.map((item) => [item.id, []]));

  for (const item of records) {
    if (item.parent_id) {
      const next = childrenByParent.get(item.parent_id) || [];
      next.push(item.id);
      childrenByParent.set(item.parent_id, next);
    }

    for (const dependencyId of item.depends_on) {
      const reverse = reverseDependencyMap.get(dependencyId) || [];
      reverse.push(item.id);
      reverseDependencyMap.set(dependencyId, reverse);
    }
  }

  const countDescendants = computeDescendantCounts(childrenByParent);

  return records.map((item) => {
    const unresolvedDependencies = item.depends_on.filter(
      (dependencyId) => recordsById.get(dependencyId)?.status !== "CLOSED",
    );
    const blockedByDependencies = unresolvedDependencies.length > 0;
    const ready = item.status === "OPEN" && item.blocked_reason === null && !blockedByDependencies;

    return {
      ...item,
      children: sortByCreatedThenId(
        (childrenByParent.get(item.id) || [])
          .map((childId) => recordsById.get(childId))
          .filter(Boolean),
      ).map((child) => child.id),
      reverse_dependencies: sortByCreatedThenId(
        (reverseDependencyMap.get(item.id) || [])
          .map((dependentId) => recordsById.get(dependentId))
          .filter(Boolean),
      ).map((dependent) => dependent.id),
      blocked_by_dependencies: unresolvedDependencies,
      ready,
      descendant_count: countDescendants(item.id),
    };
  });
}

export function buildGraphView(items) {
  const decorated = deriveViewState(items);
  return {
    nodes: sortByCreatedThenId(decorated),
    edges: {
      hierarchy: decorated
        .filter((item) => item.parent_id)
        .map((item) => ({ from: item.parent_id, to: item.id })),
      dependencies: decorated.flatMap((item) =>
        item.depends_on.map((dependencyId) => ({ from: item.id, to: dependencyId })),
      ),
    },
  };
}

export function buildViews(items) {
  const decorated = deriveViewState(items);
  return {
    active: sortByCreatedThenId(decorated.filter((item) => item.status !== "CLOSED")),
    closed: sortByCreatedThenId(decorated.filter((item) => item.status === "CLOSED")),
    ready: sortReadyItems(decorated.filter((item) => item.ready)),
    graph: buildGraphView(items),
  };
}
