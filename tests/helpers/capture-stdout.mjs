export function captureStdout(run) {
  const writes = [];
  const originalWrite = process.stdout.write;
  process.stdout.write = (chunk, encoding, callback) => {
    writes.push(Buffer.isBuffer(chunk) ? chunk.toString("utf8") : String(chunk));
    if (typeof callback === "function") {
      callback();
    }
    return true;
  };

  try {
    const returnValue = run();
    return { returnValue, output: writes.join("") };
  } finally {
    process.stdout.write = originalWrite;
  }
}

export async function captureStdoutAsync(run) {
  const writes = [];
  const originalWrite = process.stdout.write;
  process.stdout.write = (chunk, encoding, callback) => {
    writes.push(Buffer.isBuffer(chunk) ? chunk.toString("utf8") : String(chunk));
    if (typeof callback === "function") {
      callback();
    }
    return true;
  };

  try {
    const returnValue = await run();
    return { returnValue, output: writes.join("") };
  } finally {
    process.stdout.write = originalWrite;
  }
}
