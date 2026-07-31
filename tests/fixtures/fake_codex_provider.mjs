import readline from "node:readline";

const lines = readline.createInterface({ input: process.stdin });
for await (const raw of lines) {
  if (!raw.trim()) continue;
  const request = JSON.parse(raw);
  if (request.id == null) continue;
  let result;
  switch (request.method) {
    case "initialize": result = { capabilities: {} }; break;
    case "thread/start": result = { thread: { id: "thread-pulse-test" } }; break;
    case "thread/resume": result = { thread: { id: request.params.threadId } }; break;
    case "turn/start": result = { turn: { id: "turn-pulse-test" } }; break;
    case "turn/interrupt": result = {}; break;
    default:
      process.stdout.write(JSON.stringify({ jsonrpc: "2.0", id: request.id, error: { message: request.method } }) + "\n");
      continue;
  }
  process.stdout.write(JSON.stringify({ jsonrpc: "2.0", id: request.id, result }) + "\n");
}
