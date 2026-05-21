#!/usr/bin/env node

import test from "node:test";
import assert from "node:assert/strict";

import {
  firstNonEmptyString,
  normalizeSlashPath,
  stripLeadingDotSlash,
  uniqueStrings,
} from "../../skills/workflow/scripts/core/strings.mjs";

test("firstNonEmptyString returns the first trimmed non-empty string", () => {
  for (const { values, expected } of [
    { values: [null, undefined, "", "  next  "], expected: "next" },
    { values: ["  ", 7, false, "fallback"], expected: "fallback" },
    { values: ["first", "second"], expected: "first" },
    { values: [null, undefined], expected: "" },
  ]) {
    assert.equal(firstNonEmptyString(...values), expected);
  }
});

test("uniqueStrings trims strings, drops non-strings, and preserves first occurrence order", () => {
  assert.deepEqual(uniqueStrings([" a ", "b", "a", 1, "", " c ", "b"]), ["a", "b", "c"]);
  assert.deepEqual(uniqueStrings(null), []);
});

test("path string helpers normalize slashes and leading dot slash", () => {
  for (const { value, normalized, stripped } of [
    { value: "works\\item\\SPEC.md", normalized: "works/item/SPEC.md", stripped: "works/item/SPEC.md" },
    { value: "./.pulse/runtime/state.json", normalized: "./.pulse/runtime/state.json", stripped: ".pulse/runtime/state.json" },
    { value: null, normalized: "", stripped: "" },
  ]) {
    assert.equal(normalizeSlashPath(value), normalized);
    assert.equal(stripLeadingDotSlash(value), stripped);
  }
});
