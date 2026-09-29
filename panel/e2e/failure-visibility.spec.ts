// t102: t84's failure visibility (P2/P3), read at the DOM.
//
// WHAT THIS PROVES, AND HOW IT IS MADE FALSIFIABLE
//
// The two fixes are "a failed read must not render as a reading":
//
//   P2 (Memory)    `GET /api/v1/recall/log` failing used to `setRecallLog([])`,
//                  i.e. "we could not read the log" rendered as "the log is
//                  empty" -- and the `length > 0` guard hid the whole card, so
//                  the reader saw NOTHING. The fix keeps `null` (unknown) and
//                  shows `memory.recall.err` in a `Zone` (section.zone).
//   P3 (Runtimes)  the mount probe's `.catch(() => {})` left `counts[name]`
//                  undefined, so a failed probe rendered as the "not probed"
//                  chip AND the model total at the readout strip added 0 for it.
//                  The fix records `probeFailed[name]` and shows
//                  `runtimes.probeFailed` on the chip.
//
// Two ways this file avoids certifying itself with a tautology:
//
//   1. FAULTS ARE INJECTED WHERE THE PANEL CANNOT SEE THEM COMING. Every probe
//      is fulfilled by `page.route`, so the same spec runs against the PRE-fix
//      panel build and must FAIL there (the negative control; see the report
//      for the red readings).
//   2. `memory.recall.err` AND `runtimes.probeFailed` EACH PRE-EXIST SOMEWHERE
//      ELSE, so a bare `getByText(...)` would pass on the old panel too:
//        * `memory.recall.err` is used by the RECALL PLAYGROUND
//          (`0a39e5b:panel/src/views/Memory.tsx`), whose failure is an antd
//          `ErrorState`, not a `section.zone` on the audit-log tab;
//        * `runtimes.probeFailed` is used by the pre-existing R11 SYNC alert,
//          rendered on `probeErr` -- which the MOUNT probe never sets.
//      So reading (1) asserts on `section.zone` (the mount-failure card) and
//      reading (3) asserts on the CHIP (`.agent-card`'s first `.row` `.tag`),
//      with the R11 `[role=alert]` count logged separately as the
//      discriminator.
//
// No daemon config is written and no live data is touched: the run points at a
// throwaway `--root` (see the report for the commands that start it), and this
// spec only reads. `writeAccess()` is therefore not needed here.

import { expect, test, type APIRequestContext, type Locator, type Page } from "@playwright/test";

const RECALL_LOG = "**/api/v1/recall/log*";
const AGENT_OPTIONS = "**/api/v1/agents/*/options*";

/** A synthetic SUCCESS for one agent probe: exactly three models, so the
 *  expected chip/count is a constant this spec owns (never a live probe: the
 *  real one spawns `npx …` and is neither deterministic nor offline). */
const THREE_MODELS = JSON.stringify({
  agent: "claude",
  options: [
    {
      id: "model",
      name: "Model",
      category: "model",
      choices: [{ id: "m1" }, { id: "m2" }, { id: "m3" }],
      current: "m1",
    },
  ],
});

/** A synthetic SUCCESS for the recall log: an empty log is a READING ("no
 *  rows"), and it is the state the success-state comparison is made in. */
const EMPTY_LOG = JSON.stringify({
  log: [],
  retention: { policy: "keep-last", max_rows: 5000, rows: 0 },
});

const norm = async (loc: Locator) =>
  ((await loc.textContent()) ?? "").replace(/\s+/g, " ").trim();

// ── t109: this spec must NOT depend on the operator's live config ──────────────
// It used to hard-code the runtime name `claude`, which exists only in a real
// ~/.ruagent. CI registers mock agents, so three tests died at
// `cardOf(page,"claude")` with `element(s) not found` (run 36485138690). The
// name now comes from the DAEMON'S OWN list; when the daemon has no runtime at
// all the spec SKIPS with that reason instead of passing by accident.
let RT = "";
async function firstRuntime(request: APIRequestContext): Promise<{ name: string; all: string[] }> {
  // `GET /api/v1/runtimes` is 405 (that path only takes POST/PATCH/DELETE); the
  // panel itself lists runtimes from `GET /api/v1/agents`, where a runtime is the
  // entry with `kind === "runtime"` (measured on the live daemon: 10 entries,
  // kinds {role, runtime}). NOTE the old literal `claude` was not even a runtime
  // name there -- `claude-code` is -- so the pre-fix spec passed locally only
  // because `cardOf` matches by SUBSTRING.
  const res = await request.get("/api/v1/agents");
  if (!res.ok()) return { name: "", all: [] };
  const body = (await res.json()) as { agents?: Array<{ name?: string; kind?: string }> };
  const agents = body.agents ?? [];
  const all = agents
    .filter((a) => a.kind === "runtime")
    .map((a) => String(a.name ?? ""))
    .filter(Boolean);
  return { name: all[0] ?? "", all: agents.map((a) => `${a.name}:${a.kind}`) };
}
test.beforeEach(async ({ request }) => {
  const { name, all } = await firstRuntime(request);
  RT = name;
  test.skip(
    !name,
    `t109: this daemon exposes no runtime to probe (agents: [${all.join(", ")}]); this spec must not assume the operator's live config -- a machine configured with a runtime whose name merely CONTAINS "claude" used to be the only way it passed.`,
  );
});
const err500 = {
  status: 500,
  contentType: "application/json",
  body: '{"error":"e2e forced 500"}',
} as const;

/** The chip in a runtime card: the first `.row`'s `.tag`, which is NOT the R11
 *  alert (`role=alert > button.tag.err`). NOTE the card's FIRST `.tag` is the
 *  status tag ("已启用"), so the readings below assert on the CARD's text and
 *  count `[role=alert]` INSIDE that card: with zero in-card alerts, the only
 *  source of "探测失败" is the model chip (t84's new state). */
const cardOf = (page: Page, name: string) =>
  page.locator(".agent-card").filter({ hasText: name }).first();

const cardText = async (page: Page, name: string) => norm(await cardOf(page, name));

const inCardAlerts = (page: Page, name: string) =>
  cardOf(page, name).locator('[role="alert"]');

const readoutOf = (page: Page, label: string) =>
  page
    .locator(".readout-cell")
    .filter({ hasText: label })
    .first()
    .locator(".readout")
    .first();

/** The view area (`<Content className="content">`), i.e. everything except the
 *  sidebar: the region both builds must render identically in the SUCCESS
 *  state. */
const content = (page: Page) => page.locator(".content").first();

async function openAuditLog(page: Page) {
  await page.goto("/#memory");
  await page
    .locator(".ant-segmented-item-label")
    .filter({ hasText: /审计日志|Audit Log/ })
    .click();
}

test("P2: a 500 on /recall/log is an ERROR, not an empty log", async ({ page }) => {
  await page.route(RECALL_LOG, (r) => r.fulfill(err500));
  await openAuditLog(page);

  // (1) the error text appears, in the log's own Zone.
  const zone = page
    .locator("section.zone")
    .filter({ hasText: /召回失败|Recall failed/ })
    .first();
  await expect(zone).toBeVisible({ timeout: 10_000 });
  const zoneText = await norm(zone);
  console.log(`DOM READING P2 error zone: ${zoneText}`);
  expect(zoneText).toMatch(/召回失败|Recall failed/);
  // The pairing that makes (1) about THIS surface: the log's own zone title
  // ("最近召回" / "recent recalls") must be in the same zone as the failure --
  // the playground's ErrorState carries no such title.
  expect(zoneText).toMatch(/最近召回|Recent recalls|Recall log|召回日志/);

  // (2) no empty-result vocabulary anywhere on the page. These are the strings
  // that would claim a READING ("nothing matched" / "0 rows" / "no rows yet").
  const body = (await content(page).innerText()).replace(/\s+/g, " ");
  const emptyVocab = body.match(
    /没有命中|暂无有效事实|暂无记忆|还没有运行记录|0 条|0 rows|No hits|No current facts|No runs recorded yet/g,
  );
  console.log(`DOM READING P2 empty-vocabulary hits: ${JSON.stringify(emptyVocab)}`);
  expect(emptyVocab, "a failed read must not print empty-result vocabulary").toBeNull();
});

test("P2 success control: 200 + [] shows no error text, and the view is byte-stable", async ({
  page,
}) => {
  await page.route(RECALL_LOG, (r) =>
    r.fulfill({ status: 200, contentType: "application/json", body: EMPTY_LOG }),
  );
  await openAuditLog(page);
  await page.waitForTimeout(1200);

  const view = content(page);
  const body = (await view.innerText()).replace(/\s+/g, " ");
  console.log(
    `DOM READING P2-success error-text count: ${body.match(/召回失败|Recall failed/g)?.length ?? 0}`,
  );
  expect(body).not.toMatch(/召回失败|Recall failed/);
  expect(body).not.toMatch(/无法读取记忆|Cannot read memory/);
  console.log(`SNAP-BEGIN memory-success|${body}|SNAP-END`);
});

test("P3: a failed mount probe says so on the CHIP, and the R11 alert is not what is read", async ({
  page,
}) => {
  await page.route(AGENT_OPTIONS, (r) => r.fulfill(err500));
  await page.goto("/#runtimes");
  await expect(cardOf(page, RT)).toBeVisible({ timeout: 15_000 });
  await expect(cardOf(page, RT)).toHaveText(/探测失败|probe failed/, {
    timeout: 15_000,
  });

  // (3) the discriminator: the pre-existing R11 alert renders the SAME text,
  // but on `probeErr`, which the mount probe never sets. Zero in-card alerts
  // means the text above can only have come from the chip (t84's new state).
  const alerts = await inCardAlerts(page, RT).count();
  console.log(`DOM READING P3 in-card R11-alert nodes: ${alerts}`);
  expect(alerts, "the mount probe must not set the sync path's probeErr").toBe(0);
  const text3 = await cardText(page, RT);
  console.log(`DOM READING P3 claude card: ${text3}`);

  // (4) the model readout, reconciled against the cards: every runtime's probe
  // failed, so nothing may be COUNTED as a measured model.
  const models = await norm(await readoutOf(page, "模型"));
  console.log(`DOM READING P3 models readout (all probes failed): ${models}`);
  const cards: string[] = [];
  for (const name of [RT]) {
    if ((await cardOf(page, name).count()) > 0) cards.push(`${name}=[${await cardText(page, name)}]`);
  }
  console.log(`DOM READING P3 cards: ${JSON.stringify(cards)}`);

  // DIAGNOSTIC (no assertion here -- the finding is filed in the report, not
  // fixed in this task): a SUCCESSFUL retry after a failed mount probe. `sync`
  // records the fresh count but does not clear `probeFailed`, so the chip keeps
  // saying "probe failed · retry" and the readout keeps excluding the count.
  await page.unroute(AGENT_OPTIONS);
  await page.route(AGENT_OPTIONS, (r) =>
    r.fulfill({ status: 200, contentType: "application/json", body: THREE_MODELS }),
  );
  await cardOf(page, RT).locator("button[aria-label]").first().click();
  await page.waitForTimeout(2000);
  console.log(`DOM READING P3-after-successful-retry card: ${await cardText(page, RT)}`);
  console.log(
    `DOM READING P3-after-successful-retry models readout: ${await norm(await readoutOf(page, "模型"))}`,
  );
  console.log(
    `DOM READING P3-after-successful-retry in-card R11-alert nodes: ${await inCardAlerts(page, RT).count()}`,
  );
});

test("P3 success control: a real 0 models is 'not probed', not an error, and byte-stable", async ({
  page,
}) => {
  await page.route(AGENT_OPTIONS, (r) =>
    r.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({ agent: RT, options: [] }),
    }),
  );
  await page.goto("/#runtimes");
  await expect(cardOf(page, RT)).toBeVisible({ timeout: 15_000 });
  await page.waitForTimeout(1800);

  const text = await cardText(page, RT);
  const body = (await content(page).innerText()).replace(/\s+/g, " ");
  console.log(`DOM READING P3-success card (真 0 models): ${text}`);
  console.log(
    `DOM READING P3-success error-text count: ${body.match(/探测失败|probe failed/g)?.length ?? 0}`,
  );
  console.log(`DOM READING P3-success models readout: ${await norm(await readoutOf(page, "模型"))}`);
  expect(text).toMatch(/未探测|not probed/);
  expect(text).not.toMatch(/探测失败|probe failed/);
  expect(body).not.toMatch(/探测失败|probe failed/);

  const strip = page.locator(".readout-strip.grid").first();
  const snap = (await strip.count()) > 0 ? await norm(strip) : "<no readout strip rendered>";
  console.log(`SNAP-BEGIN runtimes-success|${snap}|SNAP-END`);
  console.log(`SNAP-ONLYCARD runtimes-success|${text}|SNAP-END`);
});

// ── t103: the two findings this task fixes (F1 aggregate third state, F2 latch) ──
test("F2 (t103): a successful retry after a failed mount probe CLEARS the failure", async ({
  page,
}) => {
  await page.route(AGENT_OPTIONS, (r) => r.fulfill(err500));
  await page.goto("/#runtimes");
  const card = cardOf(page, RT);
  await expect(card).toBeVisible({ timeout: 15_000 });
  await expect(card).toHaveText(/探测失败|probe failed/, { timeout: 15_000 });
  console.log(`DOM READING F2-before-retry card: ${await cardText(page, RT)}`);

  await page.unroute(AGENT_OPTIONS);
  await page.route(AGENT_OPTIONS, (r) =>
    r.fulfill({ status: 200, contentType: "application/json", body: THREE_MODELS }),
  );
  await card.locator("button[aria-label]").first().click();
  await expect(card).not.toHaveText(/探测失败|probe failed/, { timeout: 15_000 });
  const modelsAfter = await norm(await readoutOf(page, "模型"));
  console.log(`DOM READING F2-after-retry card: ${await cardText(page, RT)}`);
  console.log(`DOM READING F2-after-retry models readout: ${modelsAfter}`);
  expect(modelsAfter, "the recovered count must reach the aggregate").toMatch(/3/);
});

test("F1 (t103): with EVERY probe failed the models readout carries no number", async ({
  page,
}) => {
  await page.route(AGENT_OPTIONS, (r) => r.fulfill(err500));
  await page.goto("/#runtimes");
  await expect(cardOf(page, RT)).toBeVisible({ timeout: 15_000 });
  await expect(cardOf(page, RT)).toHaveText(/探测失败|probe failed/, { timeout: 15_000 });
  const models = await norm(await readoutOf(page, "模型"));
  console.log(`DOM READING F1-models-readout-all-failed: ${models}`);
  expect(models, "an all-failed readout must not present a measured number").not.toMatch(/\d/);
});
