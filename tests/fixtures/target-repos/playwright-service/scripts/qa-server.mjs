#!/usr/bin/env node

import { createServer } from "node:http";
import { readFile, writeFile } from "node:fs/promises";

const [statePath, htmlPath] = process.argv.slice(2);
if (!statePath || !htmlPath) {
  throw new Error("qa-server requires state and HTML paths");
}

async function state() {
  return JSON.parse(await readFile(statePath, "utf8"));
}

function json(response, status, value) {
  response.writeHead(status, { "content-type": "application/json" });
  response.end(JSON.stringify(value));
}

const server = createServer(async (request, response) => {
  try {
    const current = await state();
    if (request.method === "GET" && request.url === "/health") {
      json(response, 200, current.actual_identity);
      return;
    }
    if (request.method === "POST" && request.url === "/reset") {
      current.reservation_count = 1;
      await writeFile(statePath, `${JSON.stringify(current)}\n`);
      json(response, 200, {
        reservation_count: current.reservation_count,
        ...current.actual_identity,
      });
      return;
    }
    if (request.method === "GET" && request.url === "/api/state") {
      json(response, 200, {
        reservation_count: current.reservation_count,
        ...current.actual_identity,
      });
      return;
    }
    if (request.method === "GET" && request.url === "/") {
      response.writeHead(200, { "content-type": "text/html; charset=utf-8" });
      response.end(await readFile(htmlPath));
      return;
    }
    json(response, 404, { error: "not_found" });
  } catch (error) {
    json(response, 500, { error: String(error) });
  }
});

server.listen(4173, "127.0.0.1");
for (const signal of ["SIGINT", "SIGTERM"]) {
  process.on(signal, () => server.close(() => process.exit(0)));
}
