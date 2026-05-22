const UNRESOLVED_RUNTIME_PLACEHOLDERS = ["{{pulse_command}}"];

export function renderPulsePlaceholders(content, { pulseCommand }) {
  return content.replace(/\{\{pulse_command\}\}/g, pulseCommand);
}

export function findUnresolvedRuntimePlaceholders(content) {
  return UNRESOLVED_RUNTIME_PLACEHOLDERS.filter((placeholder) => content.includes(placeholder));
}

export function assertNoUnresolvedRuntimePlaceholders(content, filePath) {
  const unresolved = findUnresolvedRuntimePlaceholders(content);
  if (unresolved.length > 0) {
    throw new Error(`${filePath} contains unresolved runtime placeholder(s): ${unresolved.join(", ")}`);
  }
}
