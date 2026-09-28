// Playwright E2E: the panel against a REAL running daemon
// (127.0.0.1:8787, serving panel/dist). No webServer section on
// purpose — the daemon is external, booted by the operator locally or
// by the CI workflow before this step. No mocks anywhere: these are
// integration tests over live data.

import { defineConfig } from "@playwright/test";

// ---------------------------------------------------------------------------
// WHERE THIS SUITE POINTS -- and why an unset E2E_BASE_URL is no longer a silent
// choice (t107; report: docs/design/reviews/gen3-e2e-live-daemon-guard.md).
//
// The default used to be `http://127.0.0.1:8787` for EVERY caller. On this
// machine that is the OPERATOR'S LIVE DAEMON (`~/.ruagent`), and this suite does
// not merely read it: `consumption.spec.ts` writes a memory and a `recall_log`
// row (and a document and an entity), `registry.spec.ts` edits the daemon's own
// `agents.toml`. So `npm run test:e2e` with no environment drove -- and wrote --
// the operator's real data. Measured 2026-09-29: the live DB carried the spec's
// probe memory and four `recall_log` rows, and a red that looked like a panel bug
// (`cite_coverage: undefined`) was really a TWO-DAY-OLD live binary answering the
// request, because nobody had named a target.
//
// The rule now: the caller NAMES the daemon.
//   * `E2E_BASE_URL` set    -> that is the target. The intended way; point it at
//                              a throwaway `--root`, whose port is usually not 8787.
//   * unset, CI             -> the workflow's own daemon on 8787: `.github/workflows/
//                              e2e.yml` boots it from a runner TEMP root seconds
//                              before this suite runs, so it holds nobody's real
//                              data. This branch is keyed on CI -- never on the
//                              port or the URL -- so a LOCAL run cannot take it,
//                              and CI needs no workflow change.
//   * unset, not CI         -> REFUSE, loudly, naming the command to run instead.
//                              A silent default here is exactly what wrote the
//                              live database.
function e2eBaseUrl(): string {
  const explicit = process.env.E2E_BASE_URL?.trim();
  if (explicit) return explicit;
  if (process.env.CI) return "http://127.0.0.1:8787";
  throw new Error(
    "E2E_BASE_URL is not set, so this suite has no target it may safely drive. " +
      "It WRITES (consumption.spec.ts writes a memory, a recall_log row, a document and an " +
      "entity; registry.spec.ts edits the daemon's own agents.toml), and the old default " +
      "(http://127.0.0.1:8787) is this machine's LIVE daemon with your real ~/.ruagent data. " +
      "Boot a throwaway daemon and name it:\n" +
      "  ruagent serve --root <temp dir> --addr 127.0.0.1:8899\n" +
      "  E2E_BASE_URL=http://127.0.0.1:8899 npm run test:e2e\n" +
      "Why: docs/design/reviews/gen3-e2e-live-daemon-guard.md",
  );
}

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
    baseURL: e2eBaseUrl(),
    screenshot: "only-on-failure",
    trace: "retain-on-failure",
    locale: "zh-CN",
  },
  reporter: process.env.CI ? [["github"], ["html", { open: "never" }]] : [["list"]],
});
