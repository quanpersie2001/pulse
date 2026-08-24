#!/usr/bin/env node

import { readFile, mkdir } from "node:fs/promises";
import { resolve } from "node:path";
import { chromium } from "playwright";
import { pulseBrowser } from "../playwright.config.mjs";

const [inputPath] = process.argv.slice(2);
if (!inputPath) throw new Error("qa-playwright requires the Pulse input path");
const input = JSON.parse(await readFile(inputPath, "utf8"));
const identity = input.environment;
if (!identity?.deployment) throw new Error("Pulse deployment identity is required");

const runtime = resolve(".pulse/runtime/real-browser");
const tracePath = resolve(runtime, "trace.zip");
await mkdir(runtime, { recursive: true });

const consoleErrors = [];
const networkErrors = [];
const browser = await chromium.launch({ headless: pulseBrowser.headless });
const context = await browser.newContext();
await context.tracing.start({ screenshots: true, snapshots: true, sources: true });
const page = await context.newPage();
page.on("console", (message) => {
  if (message.type() === "error") consoleErrors.push(message.text());
});
page.on("requestfailed", (request) => {
  networkErrors.push(`${request.method()} ${request.url()} ${request.failure()?.errorText ?? "failed"}`);
});
page.on("response", (response) => {
  if (response.status() >= 400) {
    networkErrors.push(`${response.request().method()} ${response.url()} HTTP ${response.status()}`);
  }
});

await page.goto(identity.deployment.base_url, { waitUntil: "networkidle" });
await page.getByTestId("status").waitFor({ state: "visible" });
const reservationCount = (await page.getByTestId("reservation-count").textContent())?.trim() ?? "";
const body = page.locator("body");
const actualSource = (await body.getAttribute("data-source-commit")) ?? "";
const actualBuild = (await body.getAttribute("data-build-id")) ?? "";
const actualDeployment = (await body.getAttribute("data-deployment-id")) ?? "";
const assertions = input.cases.flatMap((testCase) => [
  {
    case_id: testCase.id,
    kind: "visible_reservation_count",
    expected: "1",
    actual: reservationCount,
    passed: reservationCount === "1",
  },
  {
    case_id: testCase.id,
    kind: "candidate_source_commit",
    expected: input.source_commit,
    actual: actualSource,
    passed: actualSource === input.source_commit,
  },
  {
    case_id: testCase.id,
    kind: "candidate_build_id",
    expected: identity.deployment.build_id,
    actual: actualBuild,
    passed: actualBuild === identity.deployment.build_id,
  },
  {
    case_id: testCase.id,
    kind: "deployment_instance_id",
    expected: identity.deployment.deployment_id,
    actual: actualDeployment,
    passed: actualDeployment === identity.deployment.deployment_id,
  },
]);
const allPassed = assertions.every((assertion) => assertion.passed);
await context.tracing.stop({ path: tracePath });
await browser.close();

process.stdout.write(
  JSON.stringify({
    schema_version: 1,
    cases: input.cases.map((testCase) => ({
      case_id: testCase.id,
      case_revision: testCase.revision,
      outcome: allPassed ? "passed" : "product_failure",
    })),
    observations: [
      `rendered ${identity.deployment.base_url} with ${assertions.length} deterministic assertions`,
      `console_errors=${consoleErrors.length} network_errors=${networkErrors.length}`,
    ],
    artifacts: [
      {
        path: ".pulse/runtime/real-browser/trace.zip",
        role: "trace",
        kind: "playwright_trace",
        media_type: "application/zip",
      },
    ],
    browser: {
      engine: pulseBrowser.engine,
      base_url: identity.deployment.base_url,
      trace_role: "trace",
      deployment: {
        build_id: actualBuild,
        deployment_id: actualDeployment,
        base_url: identity.deployment.base_url,
      },
      assertions,
      console_errors: consoleErrors,
      network_errors: networkErrors,
    },
    cleanup_passed: true,
  }),
);
