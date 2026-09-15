// Playwright E2E: the panel against a REAL running daemon
// (127.0.0.1:8787, serving panel/dist). No webServer section on
// purpose — the daemon is external, booted by the operator locally or
// by the CI workflow before this step. No mocks anywhere: these are
// integration tests over live data.

import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "./e2e",
  timeout: 30_000,
  fullyParallel: true,
  retries: process.env.CI ? 1 : 0,
  workers: process.env.CI ? 2 : undefined,
  expect: { timeout: 8_000 },
  // Every test gets a fresh browser context (own localStorage): theme
  // and language toggles in one test cannot leak into another.
  use: {
    baseURL: "http://127.0.0.1:8787",
    screenshot: "only-on-failure",
    trace: "retain-on-failure",
    locale: "zh-CN",
  },
  reporter: process.env.CI ? [["github"], ["html", { open: "never" }]] : [["list"]],
});
