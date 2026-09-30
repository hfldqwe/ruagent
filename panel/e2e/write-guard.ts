// SPECS THAT WRITE THE DAEMON'S REAL CONFIG -- and the gate in front of them.
//
// WHY THIS FILE EXISTS (a real incident, 2026-09-24): registry.spec.ts creates a
// runtime and a role in the daemon's OWN config (agents.toml) through the UI,
// then deletes them through the UI. Its header used to claim that made it "safe
// on any daemon (the user's real one included)". It is not. Step 2 failed on a
// manual run, step 3 (the cleanup) never executed, and the user's config was left
// holding [runtime.e2e-rt] and [agent.e2e-role].
//
// The lesson is not "remember to pass a flag": a countermeasure that depends on
// the caller remembering is not a countermeasure. So the gate is the DEFAULT --
// the write-access flag is set by the suite's own entry point (npm run test:e2e
// -> e2e/run-e2e.mjs) and by nothing else. A bare "npx playwright test", which is
// exactly how the incident happened, does not set it, so these specs skip
// themselves and say why.
//
// WHICH SPECS WRITE REAL DATA (kept next to the code, not in a report):
//   registry.spec.ts              CREATES a runtime and a role in the daemon config
//                                 (agents.toml), and deletes them again.
//   settings-capabilities.spec.ts edits the `[capabilities]` table of the daemon's
//                                 own policy.toml (a weight), and restores it.
//   consumption.spec.ts           writes a wiki page, a memory, a `recall_log` row,
//                                 a document and a graph entity. It removes the
//                                 wiki page and the document; the memory and the
//                                 `recall_log` row are left behind on purpose
//                                 (on a throwaway root they are the evidence the
//                                 run happened).
//   recall.spec.ts                writes ONE memory fixture and deletes it again
//                                 (the cleanup is a write too, so it is gated as
//                                 well).
//   every other spec              read-only over the API and the panel.
//
// ADDING A SPEC THAT WRITES? Import writeAccess(), pass it what YOUR spec writes,
// and test.skip() on the result -- then add the spec to the list above.
import type { APIRequestContext } from "@playwright/test";

export const WRITE_ACCESS_ENV = "RUAGENT_E2E_ALLOW_WRITES";

/** The gate. `writes` is what THIS spec does to the daemon, in its own words.
 *
 *  It is a required argument on purpose: the reason a skip prints is the only
 *  thing a reader sees of a spec that did not run, so it has to describe the
 *  thing being skipped. One shared sentence naming registry.spec.ts's runtime and
 *  role, printed for a spec that edits policy.toml, is a skip reason that
 *  misinforms about the very damage it exists to prevent. */
export function writeAccess(writes: string): { allowed: boolean; reason: string } {
  if (process.env[WRITE_ACCESS_ENV] === "1") return { allowed: true, reason: "" };
  return {
    allowed: false,
    reason:
      `this spec ${writes} -- the daemon's REAL config -- so it must not run ` +
      "against a daemon whose config matters, and it must not run by accident. " +
      "Run it through the suite entry point instead: npm run test:e2e sets " +
      WRITE_ACCESS_ENV + "=1. A bare npx playwright test deliberately does not.",
  };
}

// The names this suite creates. They are the residue signature: if either is
// present in a config, a previous run left it behind.
export const E2E_SCRATCH = { runtime: "e2e-rt", role: "e2e-role" };

// IDEMPOTENT CLEANUP, deliberately NOT through the UI.
//
// Going through the UI is what failed: cleanup lived in step 3, so a failure in
// step 2 skipped it. This runs from a finally block and calls the API directly, so
// it works whether the test created nothing, one thing, or both -- and it is safe
// to run twice (a 404 means there was nothing to remove).
export async function removeResidue(
  request: APIRequestContext,
  baseURL: string,
): Promise<{ role: number; runtime: number }> {
  const del = async (path: string): Promise<number> => {
    try {
      const r = await request.delete(baseURL + path, { timeout: 10_000 });
      return r.status();
    } catch {
      return 0;
    }
  };
  // The role first: the runtime is in use while the role references it.
  const role = await del("/api/v1/agents/" + E2E_SCRATCH.role);
  const runtime = await del("/api/v1/runtimes/" + E2E_SCRATCH.runtime);
  return { role, runtime };
}
