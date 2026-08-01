import readline from "node:readline";

const lines = readline.createInterface({ input: process.stdin });
for await (const raw of lines) {
  if (!raw.trim()) continue;
  const request = JSON.parse(raw);
  if (request.id == null) continue;
  let result;
  switch (request.method) {
    case "initialize": result = { capabilities: {} }; break;
    case "thread/start": result = { thread: { id: "thread-high-volume" } }; break;
    case "turn/start":
      for (let index = 0; index < 128; index += 1) {
        process.stdout.write(JSON.stringify({
          jsonrpc: "2.0",
          method: "thread/started",
          params: { thread: { id: "thread-high-volume" }, index },
        }) + "\n");
      }
      process.stdout.write(JSON.stringify({
        jsonrpc: "2.0",
        method: "turn/completed",
        params: { turn: { id: "turn-high-volume" } },
      }) + "\n");
      result = { turn: { id: "turn-high-volume" } };
      break;
    default:
      process.stdout.write(JSON.stringify({
        jsonrpc: "2.0",
        id: request.id,
        result: {},
      }) + "\n");
      continue;
  }
  process.stdout.write(JSON.stringify({ jsonrpc: "2.0", id: request.id, result }) + "\n");
}
