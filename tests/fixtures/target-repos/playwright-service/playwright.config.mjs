import { defineConfig } from "@playwright/test";

export const pulseBrowser = Object.freeze({
  engine: "chromium",
  headless: true,
  baseURL: "http://127.0.0.1:4173",
});

export default defineConfig({
  use: {
    baseURL: pulseBrowser.baseURL,
    browserName: pulseBrowser.engine,
    headless: pulseBrowser.headless,
    trace: "on",
  },
});
