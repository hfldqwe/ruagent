// The capability OPTION EDITOR behind #settings -- the panel work this
// increment rewrote, and the one surface the suite never opened.
//
// WHY THIS FILE EXISTS (ruagent-close-the-gaps t13). The rewrite replaced a
// per-key table COPIED INTO the panel with an editor driven by what the daemon
// declares (`options_schema`), validated against the daemon's own bounds before
// the request, and written as one whole-table read-modify-write. The e2e gate
// certified a panel WITHOUT any of it: `grep -n settings panel/e2e/*.spec.ts`
// matched nothing, `views.spec.ts` walks 12 views and settings is not one of
// them, and `responsive.spec.ts`'s phone-route list stops at `graph`. Every claim
// about the editor therefore rested on `tsc` and a human reading -- the exact
// position this effort keeps finding insufficient, which is why the gate was run.
//
// WHAT THE ASSERTIONS CARRY (this is not "the view renders"):
//
//   * `the value the user typed REACHES THE DAEMON`. After typing 1.5 and
//     clicking Apply: the DAEMON's own answer must report
//     `options_set.weight === true` AND `options.weight === 1.5`, and the
//     daemon's policy.toml ON DISK must carry `weight = 1.5` in that row's table.
//     An Apply that silently did nothing fails all three. An editor that was
//     removed fails earlier still -- the input does not exist, so there is
//     nothing to fill.
//
//   * `Reset REMOVES the key instead of writing the default in`. After Reset:
//     `options_set.weight === false` (the FILE no longer carries the key) and the
//     row's table in policy.toml holds no `weight` line at all, while the
//     RESOLVED value falls back to the registry default (1) and the input shows
//     it tagged as the default. A Reset that persisted `weight = 1.0` -- the
//     defect the task names -- flips `options_set.weight` back to true and leaves
//     the line in the file: BOTH assertions go red. The file assertion is not
//     vacuous: the same parser found `weight = 1.5` in the same table one step
//     earlier, and that is asserted too.
//
//   * `a value the daemon would refuse is refused LOCALLY`. Covered by the second
//     test: an out-of-range number never reaches the wire (read off the browser's
//     own request log, not inferred), the field quotes the daemon's OWN
//     `expectation` phrase, and the schema-driven validation that replaced the
//     copied table is therefore exercised rather than merely compiled.
//
// WHAT THIS SPEC WRITES, AND WHERE. `#settings` saves the daemon's own
// policy.toml -- specifically the file the daemon reports as `config_file` under
// the root whose `E2E_BASE_URL` names it (a throwaway root: run-e2e.mjs refuses
// to start without one). Two protections, the same two registry.spec.ts has:
//
//   1. writeAccess() -- true only when the suite's entry point armed it
//      (npm run test:e2e -> e2e/run-e2e.mjs sets RUAGENT_E2E_ALLOW_WRITES=1). A
//      bare `npx playwright test` skips this file and prints why.
//   2. the `finally` restores the config THROUGH THE API, never through the UI.
//      It restores the WHOLE `[capabilities]` table, not just the row it edited,
//      because the PUT replaces the table outright: putting only `weight` back
//      would still leave an empty `[capabilities]` table behind, and the mere
//      PRESENCE of that table is itself a behaviour change -- the registry
//      defaults start applying (llm-tier rows default OFF) from the moment it
//      exists. The snapshot comes from the GET taken before the first click, and
//      the restore body is rebuilt from it by the panel's own rule
//      (`fileOptions` + `enabled` only where the file carries it), so a row
//      nobody configured is left out rather than materialised as an opinion.

import { expect, test, type APIRequestContext, type Page } from "@playwright/test";
import { readFileSync } from "node:fs";

import { writeAccess } from "./write-guard";

const access = writeAccess("edits the `[capabilities]` table of the daemon's own policy.toml");
test.skip(!access.allowed, access.reason);

/** The row this file edits. `recall_leg_memory_fts` is FREE-tier (no cost
 *  confirmation dialog in the way), and on a fresh daemon the file does not carry
 *  its `weight` -- so "did the write land" has an unambiguous answer. */
const ROW = "recall_leg_memory_fts";
const KEY = "weight";
/** Deliberately NOT the registry default (1.0): a write that silently did nothing,
 *  or one that persisted the default, would otherwise be indistinguishable from a
 *  pass. The literal is what the input is filled with; the number is what the
 *  daemon must answer with. */
const TYPED_TEXT = "1.5";
const TYPED = 1.5;

/** The local-refusal probe. It writes NOTHING (the value is refused before any
 *  request), and it uses a DIFFERENT row from the round-trip test so the two can
 *  run in parallel without reading each other's writes. */
const REFUSED_ROW = "session_extract_rules";
const REFUSED_KEY = "max_per_input";
/** The daemon declares `1..=10000` for this key; this is above it. */
const REFUSED_TEXT = "99999";

/** One entry of a row's `options_schema`: what the REGISTRY DECLARES for one
 *  option key. Spelled here as the narrow slice this spec reads rather than
 *  imported from panel/src -- the e2e specs run against a live daemon and
 *  deliberately do not depend on the app's modules. */
interface OptionSchemaEntry {
  key: string;
  kind: string;
  min: number;
  max: number;
  default: number | null;
  /** The daemon's own phrase for what it accepts, which is what a refusal prints. */
  expectation: string;
}

/** One row of `GET /api/v1/capabilities`, as far as this spec reads it. */
interface CapabilityRow {
  id: string;
  tier: string;
  /** "file" when policy.toml carries an entry for this row. */
  configured: string;
  enabled: boolean;
  /** RESOLVED values: registry defaults merged with the file. */
  options: Record<string, number | null>;
  /** Whether the FILE carries each key (the half that says "the user set this"). */
  options_set: Record<string, boolean>;
  options_schema: OptionSchemaEntry[];
}

interface CapabilitiesPayload {
  /** The daemon's policy.toml, as an absolute path on the daemon's machine. */
  config_file: string;
  /** Whether a `[capabilities]` table exists in that file. */
  table_present: boolean;
  capabilities: CapabilityRow[];
}

async function capabilities(request: APIRequestContext): Promise<CapabilitiesPayload> {
  const res = await request.get("/api/v1/capabilities");
  expect(res.status(), "GET /api/v1/capabilities").toBe(200);
  return (await res.json()) as CapabilitiesPayload;
}

function findRow(payload: CapabilitiesPayload, id: string): CapabilityRow | undefined {
  return payload.capabilities.find((row) => row.id === id);
}

function mustFindRow(payload: CapabilitiesPayload, id: string): CapabilityRow {
  const row = findRow(payload, id);
  expect(row, `the daemon must report the capability \`${id}\``).toBeTruthy();
  return row as CapabilityRow;
}

/** One `[capabilities.<id>]` table of the daemon's policy.toml, as text.
 *
 *  Deliberately dumb: the header line, then everything up to the next table
 *  header. `""` means the table is not in the file -- which for the Reset
 *  assertion means exactly what the removal assertion means (no key left behind),
 *  and for the Apply assertion would be the failure. Both use ONE parser, and the
 *  Apply one proves it can find a key inside this very table, so the Reset one
 *  cannot pass by failing to look. */
function fileSection(policyToml: string, id: string): string {
  const lines = policyToml.split(/\r?\n/);
  const start = lines.findIndex((line) => line.trim() === `[capabilities.${id}]`);
  if (start < 0) return "";
  const body: string[] = [];
  for (const line of lines.slice(start + 1)) {
    if (/^\s*\[/.test(line)) break;
    body.push(line);
  }
  return body.join("\n");
}

/** A TOML assignment line `key = <literal>`, whatever spacing the writer chose. */
function keySetTo(key: string, literal: string): RegExp {
  return new RegExp(`^\\s*${key}\\s*=\\s*${literal}\\s*$`, "m");
}

/** Any `key = ...` line at all. */
function keyPresent(key: string): RegExp {
  return new RegExp(`^\\s*${key}\\s*=`, "m");
}

/** The PUT body that puts `[capabilities]` back exactly as `snapshot` reported it.
 *
 *  The PUT REPLACES the whole table, and the daemon's editor DELETES every row and
 *  every key a body omits -- so a faithful restore has to re-emit every row the
 *  FILE configures. The rule is the panel's own (`capabilitiesBody` on a row that
 *  is not being edited): the keys `options_set` says the file carries, plus
 *  `enabled` only when the file carries THAT -- never a registry default, which
 *  would turn a default into a persisted opinion of the user's. A row nobody
 *  configured is left out, so a daemon that had no table at all restores to `{}`,
 *  which removes the table again. */
function restoreBody(snapshot: CapabilitiesPayload): Record<string, Record<string, number | boolean>> {
  const out: Record<string, Record<string, number | boolean>> = {};
  for (const row of snapshot.capabilities) {
    if (row.configured !== "file") continue;
    const entry: Record<string, number | boolean> = {};
    for (const [key, carried] of Object.entries(row.options_set ?? {})) {
      if (key === "enabled" || carried !== true) continue;
      const value = row.options?.[key];
      if (typeof value === "number") entry[key] = value;
    }
    if (row.options_set?.enabled === true) entry.enabled = row.enabled;
    out[row.id] = entry;
  }
  return out;
}

test("a capability option typed in #settings reaches the daemon, and Reset removes it", async ({
  page,
  request,
}) => {
  // Snapshot BEFORE anything: what the config holds now is what the finally puts
  // back. Taken through the API, so it is the daemon's own account of the file.
  const snapshot = await capabilities(request);
  const before = mustFindRow(snapshot, ROW);
  const schema = before.options_schema ?? [];
  expect(
    schema.length,
    `the daemon must declare options for \`${ROW}\`; without them there is no editor to test`,
  ).toBeGreaterThan(0);

  let restoreStatus: number | null = null;
  try {
    await editorRoundTrip(page, request, snapshot, schema);
  } finally {
    // NOT through the UI, and NOT conditional: this is the half that was missing
    // in the 2026-09-24 registry incident, where a step-2 failure skipped the
    // step-3 cleanup and left e2e rows in the operator's config. Idempotent --
    // putting back a state that is already back is a no-op.
    //
    // `confirm_cost: true` on purpose: this payload is the state the daemon was
    // ALREADY in, so it is not an unconfirmed llm-tier enable, and a 409 here
    // would be a restore that did not happen.
    const res = await request.put("/api/v1/capabilities", {
      data: { confirm_cost: true, capabilities: restoreBody(snapshot) },
      timeout: 15_000,
    });
    restoreStatus = res.status();
    console.log(`DOM READING settings-capabilities cleanup: PUT restore -> ${res.status()}`);
  }

  // Asserted OUTSIDE the finally so that a failure in the round trip keeps ITS
  // error instead of being replaced by a cleanup error.
  expect(restoreStatus, "the finally block must leave the daemon's config where it found it").toBe(200);
  const restored = await capabilities(request);
  expect(
    restored.table_present,
    "the restore must put `[capabilities]` back where it was -- an empty table left behind is a behaviour change, not a harmless leftover",
  ).toBe(snapshot.table_present);
});

async function editorRoundTrip(
  page: Page,
  request: APIRequestContext,
  snapshot: CapabilitiesPayload,
  schema: OptionSchemaEntry[],
): Promise<void> {
  await page.goto("/#settings");

  // ── the EDITOR renders: one row per capability, one input per DECLARED key ──
  const card = page.locator(".capabilities-settings");
  await expect(card).toBeVisible({ timeout: 10_000 });
  // Every capability the daemon reports has its row -- one the panel dropped, or
  // one it invented, fails here.
  await expect(card.locator("[data-capability]")).toHaveCount(snapshot.capabilities.length);

  const editor = page.locator(`[data-options-for="${ROW}"]`);
  await expect(editor).toBeVisible();
  // The input set is asserted against the DAEMON'S OWN schema, not against a key
  // list copied into this spec: an editor that stopped reading `options_schema`
  // (the regression that deleting the copied table exists to prevent) renders a
  // different number of inputs.
  await expect(editor.locator(".capability-option-input")).toHaveCount(schema.length);
  for (const entry of schema) {
    await expect(
      editor.locator(`[data-option="${entry.key}"] .capability-option-input`),
      `the editor must render an input for the declared key \`${entry.key}\``,
    ).toBeVisible();
  }

  // ── type a value, then APPLY ────────────────────────────────────────────────
  const input = editor.locator(`[data-option="${KEY}"] .capability-option-input`);
  await expect(input).toBeVisible();
  await input.fill(TYPED_TEXT);
  // What is in the field before the click -- so "the daemon holds 1.5" cannot be
  // satisfied by a value that was never typed.
  await expect(input).toHaveValue(TYPED_TEXT);

  const apply = page.locator(`[data-apply-options="${ROW}"] button`);
  // Apply is disabled until the row is dirty, and a click that lands on a disabled
  // button is a no-op -- so the enabled state is an assertion of its own.
  await expect(apply).toBeEnabled();
  await apply.click();

  // THE ASSERTION THIS FILE EXISTS FOR: the typed value is in the DAEMON, not just
  // on the screen. `options_set` is the half that says the FILE carries the key,
  // so a save that did nothing leaves it false and this poll times out.
  await expect
    .poll(
      async () => {
        const row = findRow(await capabilities(request), ROW);
        return row?.options_set?.[KEY] === true ? row.options?.[KEY] : null;
      },
      { timeout: 10_000, message: `the daemon must report ${ROW}.${KEY} = ${TYPED} after Apply` },
    )
    .toBe(TYPED);

  // ...and it is in the daemon's FILE, which is what "saved" means for a config.
  expect(
    fileSection(readFileSync(snapshot.config_file, "utf8"), ROW),
    `policy.toml must carry \`${KEY} = ${TYPED_TEXT}\` in [capabilities.${ROW}]`,
  ).toMatch(keySetTo(KEY, "1\\.5"));

  // The input now shows the DAEMON's answer rather than the local draft, and the
  // "default" tag is gone because the file carries the key.
  await expect(input).toHaveValue(TYPED_TEXT);
  await expect(editor.locator(`[data-option="${KEY}"] [data-default-of="${KEY}"]`)).toHaveCount(0);

  // ── RESET ───────────────────────────────────────────────────────────────────
  await page.locator(`[data-reset-options="${ROW}"] button`).click();

  // Reset REMOVES the key rather than writing the default in: `options_set` must
  // go back to false. A Reset that persisted the default would leave it TRUE --
  // this is the assertion that catches it, and it is a different code path from
  // writing a value (`RESET_OPTIONS` is `{}`, and the daemon's editor removes what
  // a body omits).
  await expect
    .poll(
      async () => findRow(await capabilities(request), ROW)?.options_set?.[KEY] === true,
      { timeout: 10_000, message: `Reset must REMOVE ${ROW}.${KEY} from policy.toml` },
    )
    .toBe(false);

  const reset = mustFindRow(await capabilities(request), ROW);
  const entry = schema.find((e) => e.key === KEY) as OptionSchemaEntry;
  // Still in effect -- as the registry's default. The value did not vanish; the
  // file just stopped stating it as the user's own opinion.
  expect(reset.options?.[KEY]).toBe(entry.default);

  // ...and the FILE no longer carries it. Same parser, same table: the step above
  // found `weight = 1.5` with it, so this is not a lookup that found nothing.
  expect(
    fileSection(readFileSync(snapshot.config_file, "utf8"), ROW),
    `Reset must leave no \`${KEY}\` line in [capabilities.${ROW}]`,
  ).not.toMatch(keyPresent(KEY));

  // The input is back to the registry default, tagged as a default rather than as
  // a value the file states.
  await expect(input).toHaveValue(String(entry.default));
  await expect(editor.locator(`[data-option="${KEY}"] [data-default-of="${KEY}"]`)).toBeVisible();
}

test("an out-of-range capability option is refused in #settings without reaching the daemon", async ({
  page,
  request,
}) => {
  const snapshot = await capabilities(request);
  const row = mustFindRow(snapshot, REFUSED_ROW);
  const entry = (row.options_schema ?? []).find((e) => e.key === REFUSED_KEY);
  expect(entry, `the daemon must declare \`${REFUSED_ROW}.${REFUSED_KEY}\``).toBeTruthy();

  // The browser's own request log. "Refused locally" has to mean "no PUT went
  // out", and counting requests is the only way to read that -- the config after
  // a local refusal and after a daemon 400 look identical on disk.
  const puts: string[] = [];
  page.on("request", (req) => {
    if (req.method() === "PUT" && req.url().includes("/api/v1/capabilities")) puts.push(req.url());
  });

  await page.goto("/#settings");
  const editor = page.locator(`[data-options-for="${REFUSED_ROW}"]`);
  await expect(editor).toBeVisible({ timeout: 10_000 });
  const input = editor.locator(`[data-option="${REFUSED_KEY}"] .capability-option-input`);
  await expect(input).toBeVisible();
  await input.fill(REFUSED_TEXT);
  await page.locator(`[data-apply-options="${REFUSED_ROW}"] button`).click();

  const refusal = editor.locator(`[data-option="${REFUSED_KEY}"] [data-refusal]`);
  await expect(refusal).toBeVisible();
  await expect(refusal).toHaveAttribute("data-refusal", "outOfRange");
  // The range in that sentence is the DAEMON's own `expectation` phrase, taken off
  // the schema -- so this fails if the panel went back to carrying its own bounds,
  // and it is the schema-driven validation (not the type check) that proves it.
  await expect(refusal).toContainText(entry?.expectation ?? "");
  // Nothing the user typed is lost by a refusal.
  await expect(input).toHaveValue(REFUSED_TEXT);

  // The request event is delivered over CDP, so give it a beat before reading an
  // ABSENCE -- a bounded wait on a negative assertion, not a sync crutch.
  await page.waitForTimeout(500);
  expect(puts, "a value refused before the request must never be sent").toEqual([]);
});
