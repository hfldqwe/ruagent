// The e2e suite's ONE entry point, so the specs that write can only run on
// purpose. See e2e/write-guard.ts for why this is a gate rather than a reminder.
//
//   npm run test:e2e      -> arms writes (RUAGENT_E2E_ALLOW_WRITES=1) and runs
//   npx playwright test   -> does NOT arm them; the writing specs skip themselves
//
// AND IT REFUSES TO GUESS THE TARGET (t107). The suite writes; the old default
// target was this machine's LIVE daemon on 127.0.0.1:8787, so a plain
// `npm run test:e2e` used to drive -- and write -- the operator's real ~/.ruagent
// data. If the caller does not name a daemon, nothing runs at all: playwright is
// never even started (an exit here cannot write anything, unlike a spec-level
// skip that would still have run the harness machinery).
//
// CI is exempt, and deliberately: `.github/workflows/e2e.yml` boots its OWN
// daemon from a runner TEMP root immediately before calling this script, so there
// is no operator data to protect. The exemption is keyed on CI, never on the port
// or the URL, so a LOCAL run cannot take that branch.
import { spawn } from "node:child_process";

const explicit = process.env.E2E_BASE_URL?.trim();
const target = explicit || (process.env.CI ? "http://127.0.0.1:8787" : "");

if (!target) {
  console.error(
    [
      "e2e REFUSED: E2E_BASE_URL is not set.",
      "",
      "This suite WRITES to whichever daemon it drives:",
      "  * consumption.spec.ts  writes a memory, a recall_log row, a document and an entity",
      "  * registry.spec.ts     creates a runtime and a role in the daemon's own agents.toml",
      "",
      "The default target used to be http://127.0.0.1:8787 -- the LIVE daemon holding your",
      "real ~/.ruagent data (measured 2026-09-29: this is how the operator's database got",
      "written by an e2e run). Boot a throwaway daemon and name it:",
      "",
      "  ruagent serve --root <temp dir> --addr 127.0.0.1:8899",
      "  E2E_BASE_URL=http://127.0.0.1:8899 npm run test:e2e",
      "",
      "Why: docs/design/reviews/gen3-e2e-live-daemon-guard.md",
    ].join("\n"),
  );
  process.exit(2);
}

// Writes are armed here and nowhere else -- and the ONLY caller-side value that
// survives is an explicit "0", which can only make the run MORE read-only. It
// exists so the gate itself can be exercised THROUGH this entry point (a control
// that needs a bare `npx playwright test` would be a control nobody may run):
//   RUAGENT_E2E_ALLOW_WRITES=0 npm run test:e2e
// then every writing spec skips itself, and the reason is printed.
const unarmed = process.env.RUAGENT_E2E_ALLOW_WRITES === "0";
if (!unarmed) process.env.RUAGENT_E2E_ALLOW_WRITES = "1";
console.log(
  `e2e target: ${target} (writes ${unarmed ? "UNARMED on request: RUAGENT_E2E_ALLOW_WRITES=0" : "armed via RUAGENT_E2E_ALLOW_WRITES=1"})`,
);

const args = process.argv.slice(2);
const child = spawn("npx", ["playwright", "test", ...args], {
  stdio: "inherit",
  shell: true,
  env: process.env,
});
child.on("exit", (code) => process.exit(code ?? 1));
