import readline from "node:readline";

const lines = readline.createInterface({ input: process.stdin });
for await (const raw of lines) {
  if (!raw.trim()) continue;
  const request = JSON.parse(raw);
  if (request.id == null) continue;
  let result;
  switch (request.method) {
    case "initialize": result = { capabilities: {} }; break;
    case "thread/start": result = { thread: { id: "thread-exit-zero" } }; break;
    case "turn/start": result = { turn: { id: "turn-exit-zero" } }; break;
    default: result = {};
  }
  process.stdout.write(JSON.stringify({ jsonrpc: "2.0", id: request.id, result }) + "\n");
  if (request.method === "turn/start") process.exit(0);
}
