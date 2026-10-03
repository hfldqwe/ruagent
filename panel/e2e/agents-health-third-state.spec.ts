// t128: the MCP health row's THIRD state (t123 §3, same family as t102/t103).
//
// WHAT THIS PROVES, AND HOW IT IS MADE FALSIFIABLE
//
// `Agents.tsx`'s MCP registry row rendered `s.health.tools ?? 0`, so a daemon
// that answered `health` WITHOUT a tool count (the key is `null`, or absent
// altogether) was drawn as a confident "0 tools" -- "this server exposes
// nothing" when the truth was "the daemon did not tell us". The fix keeps the
// three states apart:
//
//   tools = N            -> `up · N`                       (byte-identical to before)
//   tools = null/absent  -> `up · 未告知工具数 · 重试`      (third state, no digit)
//   state = "down"       -> `已下线` / `down`               (byte-identical to before)
//
// HOW IT IS MADE RED-CAPABLE (three readings, all from injected responses):
//   1. FAULT IS INJECTED WHERE THE PANEL CANNOT SEE IT COMING. `GET /api/v1/mcp`
//      is fulfilled by `page.route`, so this same spec runs unchanged against a
//      PRE-FIX panel build, where reading (2) must FAIL (that is the negative
//      control; the report carries the red output and the window).
//   2. READING (2) ASSERTS ON A STRING THAT DOES NOT EXIST PRE-FIX -- the new
//      `agents.mcp.toolsUnknown` copy -- AND on the ABSENCE of a digit in that
//      cell, so an "assert the chip is visible" tautology cannot pass it.
//   3. READING (1) IS THE SUCCESS CONTROL (the number is still a number) and
//      reading (3) is the UNCHANGED control (down still renders as before), so a
//      spec that simply deleted the row could not pass either.
//
// No daemon config is written and no live data is read: the run points at a
// throwaway `--root` on a throwaway port (see the report for the commands), and
// this spec only reads. `writeAccess()` is therefore NOT needed here -- and it is
// deliberately not called: calling it would claim this spec writes when it does
// not, which is how a guard stops meaning anything.

import { expect, test, type Locator, type Page } from "@playwright/test";

const MCP = "**/api/v1/mcp";

/** A synthetic registry whose `health` is decided per reading by the caller. */
const registry = (health: unknown) =>
  JSON.stringify({
    servers: [
      {
        name: "e2e-health",
        command: null,
        url: "http://127.0.0.1:9/mcp",
        inject_for: null,
        health,
      },
    ],
    profiles: [],
  });

const norm = async (loc: Locator) =>
  ((await loc.textContent()) ?? "").replace(/\s+/g, " ").trim();

/** The MCP row's health tag. Scoped to the MCP zone (`Zone` title = mcp.registry)
 *  and to the row of THIS server, so an `inject_for` tag or another card's chip
 *  can never be what the reading is about. */
const healthTag = (page: Page) =>
  page
    .locator("section.zone")
    .filter({ hasText: /MCP 注册表|MCP Registry/ })
    .first()
    .locator(".row")
    .filter({ hasText: "e2e-health" })
    .first()
    .locator(".tag")
    .first();

async function openAgents(page: Page, health: unknown) {
  await page.route(MCP, (r) =>
    r.fulfill({ status: 200, contentType: "application/json", body: registry(health) }),
  );
  await page.goto("/#agents");
  const tag = healthTag(page);
  await expect(tag).toBeVisible({ timeout: 10_000 });
  return tag;
}

test("(2c) the third state's retry is a REAL re-read (route hit count rises)", async ({ page }) => {
  // The label promises a retry, so the control must actually re-issue the read --
  // the argument Runtimes.tsx:305-310 already makes for `runtimes.probeFailed`.
  // The re-read is itself intercepted by the same `page.route`, so this reading
  // stays inside the injection and never depends on the daemon's own answer.
  let calls = 0;
  await page.route(MCP, (r) => {
    calls += 1;
    return r.fulfill({
      status: 200,
      contentType: "application/json",
      body: registry({ state: "ok", tools: null, latency_ms: 33, error: null, checked_at: 1 }),
    });
  });
  await page.goto("/#agents");
  const tag = healthTag(page);
  await expect(tag).toBeVisible({ timeout: 10_000 });
  const before = calls;
  await tag.click();
  await expect.poll(() => calls, { timeout: 5_000 }).toBeGreaterThan(before);
  console.log(`DOM READING (2c) route hits before=${before} after=${calls}`);
  expect(calls).toBeGreaterThan(before);
});

test("(1) tools=N still renders the measured number (success control)", async ({ page }) => {
  const tag = await openAgents(page, {
    state: "ok",
    tools: 7,
    latency_ms: 33,
    error: null,
    checked_at: 1,
  });
  const text = await norm(tag);
  console.log(`DOM READING (1) tools=7 -> ${JSON.stringify(text)}`);
  expect(text).toMatch(/^(在线|up) · 7$/);
  // "byte-identical" includes the STATE CLASS: the measured branch is still the
  // green `.tag.ok` (the new third state is neither `.ok` nor `.warn`).
  await expect(tag).toHaveClass(/\btag\b/);
  await expect(tag).toHaveClass(/\bok\b/);
  // NOT measured here, deliberately: the Tooltip's text. antd's tooltip does not
  // open for either `hover()` or dispatched `mouseover`/`mouseenter` in this
  // harness (two attempts, no spec in this repo reads one). Rather than assert
  // on something that never appears, the "existing rendering was not rewritten"
  // claim is carried by the CELL readings above plus the verbatim `git diff` in
  // the report -- see the report's "未覆盖什么" for the named gap.
});

test("(2) tools=null is the THIRD state: no confident zero", async ({ page }) => {
  const tag = await openAgents(page, {
    state: "ok",
    tools: null,
    latency_ms: 33,
    error: null,
    checked_at: 1,
  });
  const text = await norm(tag);
  console.log(`DOM READING (2) tools=null -> ${JSON.stringify(text)}`);
  // the third state is named ...
  expect(text).toMatch(/未告知工具数|tools not reported/);
  // ... and the cell carries NO digit at all: `0 tools` was the failure.
  expect(text, "the missing-tool-count cell must not print a digit").not.toMatch(/\d/);
  expect(text).not.toMatch(/0 (个工具|tools)/);
  // the label promises a retry, so it must be a real control (Runtimes.tsx:305)
  await expect(tag).toHaveRole("button");
});

test("(2b) an ABSENT tools key is the same third state as null", async ({ page }) => {
  // `?? 0` absorbed `undefined` exactly as it absorbed `null`; the fix must keep
  // both out of the "measured" branch, so this reading drops the key entirely.
  const tag = await openAgents(page, {
    state: "ok",
    latency_ms: 33,
    error: null,
    checked_at: 1,
  });
  const text = await norm(tag);
  console.log(`DOM READING (2b) tools key deleted -> ${JSON.stringify(text)}`);
  expect(text).toMatch(/未告知工具数|tools not reported/);
  expect(text).not.toMatch(/\d/);
});

test("(3) down is unchanged, and it is not the third state", async ({ page }) => {
  const tag = await openAgents(page, {
    state: "down",
    tools: null,
    latency_ms: 0,
    error: "e2e forced down",
    checked_at: 1,
  });
  const text = await norm(tag);
  console.log(`DOM READING (3) state=down -> ${JSON.stringify(text)}`);
  expect(text).toMatch(/^(已下线|down)$/);
  expect(text).not.toMatch(/未告知工具数|tools not reported/);
  // the down branch is untouched: same neutral-warn class, no retry affordance,
  // and the tag is NOT a button (a `down` is a measurement, not an unknown).
  await expect(tag).toHaveClass(/\bwarn\b/);
  await expect(tag).not.toHaveRole("button");
});
