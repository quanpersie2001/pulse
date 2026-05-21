export function writeJson(value, output = process.stdout) {
  output.write(`${JSON.stringify(value, null, 2)}\n`);
}

export function writeText(text, output = process.stdout) {
  output.write(`${String(text)}\n`);
}

export function writePayload(value, { json = false, render = String, output = process.stdout } = {}) {
  if (json) {
    writeJson(value, output);
  } else {
    writeText(render(value), output);
  }
}
