export const TokenOutcome = Object.freeze({
  EXPIRED: "TokenExpired",
  INVALID: "InvalidToken",
});

export function classifyRefreshTokenFailure(reason) {
  if (reason === "expired") {
    return TokenOutcome.EXPIRED;
  }

  return TokenOutcome.INVALID;
}
