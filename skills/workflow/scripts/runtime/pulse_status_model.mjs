import { readPulseStatus as readPulseStatusFromState } from "./pulse_state.mjs";

export async function readPulseStatus(repoRoot) {
  return readPulseStatusFromState(repoRoot);
}
