// Playwright E2E: the panel against a REAL running daemon
// (127.0.0.1:8787, serving panel/dist). No webServer section on
// purpose — the daemon is external, booted by the operator locally or
// by the CI workflow before this step. No mocks anywhere: these are
// integration tests over live data.

import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "./e2e",
  // EVERY RUN WRITES TO ITS OWN DIRECTORY.
  //
  // Playwright EMPTIES its output directory when it starts. Two suites running
  // at the same time in this repo therefore shared test-results, and each one
  // deleted the other's artefacts: the failures that surfaced were
  // browserContext.close: ENOENT ...traces/*.trace|.network, never an
  // assertion -- a red that says nothing about the panel. Measured before this
  // change: two concurrent full runs produced ENOENT failures; after: none.
  //
  // Why PID + timestamp: the PID separates CONCURRENT runs on one machine, the
  // timestamp separates SEQUENTIAL ones. It stays under test-results/ (already
  // in .gitignore) so artefacts remain easy to find -- unlike a mkdtemp path --
  // and it needs no flag, unlike --output= : a countermeasure that depends on
  // the caller remembering to pass it is not a countermeasure.
  outputDir: process.env.PLAYWRIGHT_OUTPUT_DIR ?? "test-results/run-" + process.pid + "-" + Date.now(),
  timeout: 30_000,
  fullyParallel: true,
  retries: process.env.CI ? 1 : 0,
  workers: process.env.CI ? 2 : undefined,
  expect: { timeout: 8_000 },
  // Every test gets a fresh browser context (own localStorage): theme
  // and language toggles in one test cannot leak into another.
  use: {
    // E2E_BASE_URL: point the suite at a throwaway daemon (e.g. a
    // fresh --root home + doctor seed) to reproduce CI conditions.
    baseURL: process.env.E2E_BASE_URL ?? "http://127.0.0.1:8787",
    screenshot: "only-on-failure",
    trace: "retain-on-failure",
    locale: "zh-CN",
  },
  reporter: process.env.CI ? [["github"], ["html", { open: "never" }]] : [["list"]],
});
