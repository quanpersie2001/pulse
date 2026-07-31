// Deterministic fake Codex App Server used to exercise session-resume *failure*
// states. The behaviour is selected by the first CLI argument:
//
//   "resume_error"    -> thread/resume answers with a JSON-RPC error response.
//   "handle_mismatch" -> thread/resume returns a *different* thread id, so the
//                        daemon detects a provider-handle mismatch.
//
// thread/start always reports the same stable handle the real fixture uses, so
// a session created with `fake_codex_provider.mjs` can be resumed against this
// fixture. The process stays alive (it does not `exit`) so the daemon must
// terminate the candidate itself; this lets tests prove the candidate is no
// longer owned or running after the failure is recorded.
import readline from "node:readline";

const mode = process.argv[2] || "resume_error";

const lines = readline.createInterface({ input: process.stdin });
for await (const raw of lines) {
  if (!raw.trim()) continue;
  const request = JSON.parse(raw);
  if (request.id == null) continue;
  let result;
  switch (request.method) {
    case "initialize":
      result = { capabilities: {} };
      break;
    case "thread/start":
      result = { thread: { id: "thread-pulse-test" } };
      break;
    case "thread/resume":
      if (mode === "handle_mismatch") {
        result = { thread: { id: "thread-mismatched" } };
        break;
      }
      process.stdout.write(
        JSON.stringify({
          jsonrpc: "2.0",
          id: request.id,
          error: { code: -32000, message: "thread resume rejected" },
        }) + "\n",
      );
      continue;
    default:
      result = {};
  }
  process.stdout.write(
    JSON.stringify({ jsonrpc: "2.0", id: request.id, result }) + "\n",
  );
}
