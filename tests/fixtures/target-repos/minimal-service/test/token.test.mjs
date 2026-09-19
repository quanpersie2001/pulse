import assert from "node:assert/strict";
import test from "node:test";

import {
  classifyRefreshTokenFailure,
  TokenOutcome,
} from "../src/token.mjs";

test("expired refresh tokens have a stable outcome", () => {
  assert.equal(classifyRefreshTokenFailure("expired"), TokenOutcome.EXPIRED);
});

test("malformed and revoked tokens remain invalid without leaking details", () => {
  assert.equal(classifyRefreshTokenFailure("malformed"), TokenOutcome.INVALID);
  assert.equal(classifyRefreshTokenFailure("revoked"), TokenOutcome.INVALID);
  assert.equal(classifyRefreshTokenFailure("signature-mismatch"), TokenOutcome.INVALID);
});
