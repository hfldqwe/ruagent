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
// CI USED TO BE EXEMPT FROM THE "NAME THE TARGET" RULE, AND THAT EXEMPTION WAS THE
// HOLE (t179 / C60, measured rather than argued). The old line was
//
//   const target = explicit || (process.env.CI ? "http://127.0.0.1:8787" : "");
//
// and the reasoning was that `.github/workflows/e2e.yml` boots its OWN daemon from a
// runner TEMP root, so there is no operator data to protect. Two things were wrong:
//
//   * the branch keyed on the CI VARIABLE, not on "am I inside that workflow". A local
//     shell that exports CI (or a harness that does it for you) took the branch, and
//     this script then drove the RESIDENT daemon on this machine -- where the operator's
//     real ~/.ruagent data lives;
//   * the workflow's steps, copied to a laptop, seed $HOME/.ruagent/config/agents.toml.
//     On a runner $HOME is throwaway; here $HOME IS the operator's real config, so
//     "reproduce CI locally" quietly rewrote their registered agents.
//
// So: the target is ALWAYS explicit, and the daemon's default port is refused BY NAME.
// The workflow now sets E2E_BASE_URL explicitly and boots on a private port (8891).
import { spawn } from "node:child_process";

const explicit = process.env.E2E_BASE_URL?.trim();
const target = explicit || "";

// The daemon's default bind is 127.0.0.1:8787 (AGENTS.md, "Binding is loopback by
// default"). On a working machine that port IS the operator's resident daemon, so a
// target on it is refused before playwright is started -- refusing here cannot write
// anything. A throwaway daemon on a private port is the supported shape.
const LIVE_PORT = "8787";
const onLivePort = (() => {
  try {
    const u = new URL(target);
    const host = u.hostname.replace(/^\[|\]$/g, "");
    const loopback = host === "127.0.0.1" || host === "localhost" || host === "::1";
    return loopback && (u.port || "80") === LIVE_PORT;
  } catch {
    return false;
  }
})();

if (!target || onLivePort) {
  console.error(
    [
      `e2e REFUSED: ${target ? `E2E_BASE_URL points at the resident daemon's default port (${LIVE_PORT})` : "E2E_BASE_URL is not set"}.`,
      "",
      "This suite WRITES to whichever daemon it drives:",
      "  * consumption.spec.ts  writes a memory, a recall_log row, a document and an entity",
      "  * registry.spec.ts     creates a runtime and a role in the daemon's own agents.toml",
      "",
      "The default target used to be http://127.0.0.1:8787 -- the LIVE daemon holding your",
      "real ~/.ruagent data (measured 2026-09-29: this is how the operator's database got",
      "written by an e2e run). 8787 is the daemon's DEFAULT bind, so it is refused by name,",
      "and `CI` no longer substitutes for naming a target: boot a throwaway daemon on a",
      "private port and name it, with a THROWAWAY root:",
      "",
      "  RUAGENT_HOME=<temp dir> ruagent serve --root <temp dir> --addr 127.0.0.1:8899",
      "  E2E_BASE_URL=http://127.0.0.1:8899 npm run test:e2e",
      "",
      "RUAGENT_HOME is not decoration: the mock-agent seeding step shown in",
      "`.github/workflows/e2e.yml` writes <root>/config/agents.toml, and with a real root",
      "that file is the operator's own agent registry.",
      "",
      "Why: docs/design/reviews/gen3-e2e-live-daemon-guard.md",
      "     docs/design/reviews/gen4-e2e-local-safety.md",
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
