// THE PANEL HAS NO RUST GATE. "cargo test --workspace" compiles the crates and is
// blind to panel/src/**: a wrong antd prop, a dead import, a readout rendered from
// the wrong field -- none of that turns a Rust gate red, so this suite IS the
// panel's gate. This file is the piece of it that watches the FRONT DOOR
// (docs/design/primary-agent.md): the readout on Home, the jump from it to
// #agents, and the badge Agents puts on that agent. All three changed in one round
// and nothing automated was watching them.
//
// READ-ONLY, ON PURPOSE: nothing here calls writeAccess(), because nothing here
// writes -- not the daemon's config and not its data. The one fault it injects
// (reading D) lives in the BROWSER (page.route), so the daemon never sees it and
// this spec is safe on any target. The rest of the suite is why the entry point
// still has to name a throwaway daemon.
//
// WHY EACH READING CAN GO RED (a green that cannot fail is not a gate):
//   A  the DOM is compared against the daemon's OWN answer (name + which carrier
//      won). A panel that hard-coded a name, read the primary FLAG instead of the
//      effective answer, or dropped the source chip fails it.
//   B  the jump must actually land on #agents. A readout whose only action is dead,
//      or that navs to the wrong route, fails it.
//   C  the CARDS are counted: exactly one role card may carry the "this one
//      answers" badge, and it must be the card the daemon named. A second badge, or
//      a badge on a different card, fails it -- the task's "do not mis-mark a
//      second one".
//   D  THE NEGATIVE CONTROL, and the reason this file earns its runtime:
//      GET /api/v1/primary answering 500 must render as FAILED, never as "there is
//      no primary agent". Home states that rule itself (Dash.frontDoorErr), and the
//      two sentences are mutually exclusive on screen, so these same assertions fail
//      on a panel that collapsed the failure into the empty state -- the shape
//      failure-visibility.spec.ts exists for.
//
// WHAT A GREEN HERE DOES NOT CLAIM: it says nothing about the #chat landing rule (a
// different view, a different spec), and it does not exercise a DISABLED agent that
// still carries the primary flag (that chip is a different one -- see reading C).
//
// IT RUNS ON A FRESH DAEMON TOO. Before t64 this file skipped on one: Home renders the
// first-run guide when there are no agents OR no sessions, and the readout card lived
// only in the dashboard branch, so A, B and D had nothing to read. CI's daemon is
// fresh by construction, so three of the four readings -- the ones the task was asked
// for -- were a silent zero there. Home.tsx now renders ONE component
// (FrontDoorReadout) on both branches, and `openHomeWithReadout` below asserts the
// card instead of asking whether the dashboard happens to be on screen.
//
// HOW THIS WAS RUN (the shape a reader needs to reproduce it; the suite refuses to
// guess a target -- t107 -- so E2E_BASE_URL is not optional):
//
//   RUAGENT_HOME=<tmp>/root  ruagent serve --root <tmp>/root --addr 127.0.0.1:8899
//   E2E_BASE_URL=http://127.0.0.1:8899 npm run test:e2e -- --output=<private dir>
//
// Three properties of the daemon decide which readings RUN, and each skip above
// names the one that stopped it:
//   * the front door must name an ENABLED ROLE card, or reading C has no card to
//     badge (CI's own fixture -- roles alpha/beta/judge over mock runtimes --
//     satisfies this; the plain "harness = mock" runtime cards CI used to write do
//     not, because the role grid cannot carry a runtime card);
//   * a brand-new root is FINE (no session needed): Home's first-run guide carries the
//     readout since t64, and that is the branch a fresh daemon renders. If the card is
//     missing there, A/B/D fail rather than skip;
//   * no RUNTIME may contain a role's name as a substring, or dropdown-picking specs
//     elsewhere in this suite resolve two options (measured with a runtime called
//     "mock-judge": judge.spec.ts died on a strict-mode violation).

import { expect, test, type APIRequestContext, type Page } from "@playwright/test";

/** Home's readout card (t57). The attribute is the contract; the classes inside it
 *  are the view's own. */
const CARD = '[data-readout="front-door"]';
const FRONT_DOOR_API = "**/api/v1/primary";

/** The two readout sentences that must never stand in for each other, in both
 *  languages: the panel follows the browser's locale, so a spec that matched only
 *  Chinese would pass on this machine and fail on any English runner. */
const FAILED_TEXT = /读不到主智能体|Cannot read the primary agent/;
const NONE_TEXT = /还没有主智能体|No primary agent yet/;
/** The Agents page's own empty-state sentence (agents.primary.none) — a DIFFERENT
 *  string from Home's, because the two pages offer different next steps. Kept apart
 *  so a rename on one page cannot silently weaken the other's negative assertion. */
const NONE_AGENTS_TEXT = /还没有主智能体：|No primary agent yet — pick a role/;
/** home.frontDoor.source.* -- the chip states WHICH carrier produced the answer,
 *  which is exactly what routing.toml's precedence makes non-obvious. */
const sourceText = (source: string | null | undefined): RegExp =>
  source === "routing-default"
    ? /来自 routing 默认|from routing's default/
    : source === "primary-flag"
      ? /来自 primary 标记|from the primary flag/
      : /来源未告知|source not reported/;
/** The "this one answers" badge family on a role card (agents.primary.effective*).
 *  Deliberately NOT the "flagged but not answering" chip: that one is a statement
 *  about a different agent, and counting it as a badge would make reading C lie. */
const EFFECTIVE_BADGE = /^(主智能体|primary)(（|\s*\(|$)/;

type FrontDoor = { name: string | null; source?: string | null; why?: string | null };
type AgentRow = { name: string; kind: string; enabled: boolean; primary: boolean };

async function frontDoor(request: APIRequestContext): Promise<FrontDoor | null> {
  const res = await request.get("/api/v1/primary");
  return res.ok() ? ((await res.json()) as FrontDoor) : null;
}

async function agentRows(request: APIRequestContext): Promise<AgentRow[] | null> {
  const res = await request.get("/api/v1/agents");
  if (!res.ok()) return null;
  const body = (await res.json()) as { agents?: AgentRow[] };
  return body.agents ?? null;
}

/** Open Home and REQUIRE the readout, whichever landing branch renders.
 *
 *  Both branches carry it now: the dashboard's side card and the first-run guide's card
 *  are the SAME component (Home.tsx, FrontDoorReadout). That is why this is an
 *  assertion and not a question -- before that repair this helper asked "is the
 *  dashboard on screen?" and a daemon with no session history made A, B and D skip.
 *  CI's root is fresh by construction, so such a skip is a silent zero: a panel that
 *  dropped the readout entirely would still have been green. A missing readout now
 *  FAILS, which is the reading this file exists for. */
async function openHomeWithReadout(page: Page): Promise<void> {
  await page.goto("/");
  await expect(page.locator(".home-hero h1")).toBeVisible();
  // either branch is legal; WHICH one renders does not change what must be shown
  await expect(page.locator(".dash-grid, .guide-grid").first()).toBeVisible({ timeout: 15_000 });
  await expect(page.locator(CARD)).toBeVisible({ timeout: 15_000 });
}

test("home's readout is the daemon's answer, name and carrier both", async ({ page, request }) => {
  const fd = await frontDoor(request);
  test.skip(
    fd === null,
    "GET /api/v1/primary is not readable on this daemon, so there is no answer to compare the DOM against",
  );
  await openHomeWithReadout(page);

  const card = page.locator(CARD);
  await expect(card).toBeVisible();

  if (fd!.name === null) {
    // A DEFINITE answer -- and the one that must not be confused with a failure.
    await expect(card).toContainText(NONE_TEXT);
    await expect(card).not.toContainText(FAILED_TEXT);
    return;
  }
  // The name, and WHICH carrier produced it. Both are the daemon's words, not the
  // panel's guess: toHaveText is exact, so a trimmed, duplicated or re-cased name
  // fails here too.
  await expect(card.locator(".status-row strong")).toHaveText(fd!.name);
  await expect(card.locator(".status-row .tag")).toHaveText(sourceText(fd!.source));
  await expect(card).not.toContainText(FAILED_TEXT);
  await expect(card).not.toContainText(NONE_TEXT);
});

test("the readout walks to the page that owns the choice", async ({ page, request }) => {
  const fd = await frontDoor(request);
  test.skip(
    fd === null,
    "GET /api/v1/primary is not readable on this daemon: the card is in its failure state, so a jump reading would be about the wrong card",
  );
  await openHomeWithReadout(page);

  const card = page.locator(CARD);
  // Scoped to the card on purpose: a page-level "button" locator clicks whichever
  // action happens to be first in the DOM (measured while writing the t57 probe: it
  // hit the sessions card's "view all" and produced #sessions).
  await card.locator(".zone-head button").first().click();
  await expect(page).toHaveURL(/#agents$/);
  // ...and the page that answer belongs to is really there.
  await expect(page.locator(".agent-card, .ant-empty").first()).toBeVisible({ timeout: 15_000 });
});

test("agents badges the front door and no second card", async ({ page, request }) => {
  const fd = await frontDoor(request);
  test.skip(
    fd === null,
    "GET /api/v1/primary is not readable on this daemon, so nothing is the front door and the badge would have nothing to point at",
  );
  test.skip(
    fd!.name === null,
    "this daemon reports no front door, so no card may carry the badge (reading A covers the empty wording)",
  );

  const rows = await agentRows(request);
  test.skip(rows === null, "GET /api/v1/agents is not readable on this daemon");
  const roles = (rows ?? []).filter((a) => a.kind === "role" && a.enabled).map((a) => a.name);
  test.skip(
    !roles.includes(fd!.name as string),
    "the front door is " +
      String(fd!.name) +
      " and this daemon has no ENABLED ROLE card by that name (enabled role cards: [" +
      roles.join(", ") +
      "]). The role grid cannot carry a badge for a runtime card or a disabled one, so this reading needs a daemon whose front door is a role.",
  );

  await page.goto("/#agents");
  const cards = page.locator(".agent-card");
  await expect(cards.first()).toBeVisible({ timeout: 15_000 });

  // One pass over the DOM: which cards carry the "this one answers" badge?
  const badged = await cards.evaluateAll(
    (els, pattern) => {
      const re = new RegExp(pattern as string);
      return els
        .map((el) => ({
          name: (el.querySelector(".row strong")?.textContent ?? "").trim(),
          chips: Array.from(el.querySelectorAll(".tag")).map((c) => (c.textContent ?? "").trim()),
        }))
        .filter((c) => c.chips.some((t) => re.test(t)));
    },
    EFFECTIVE_BADGE.source,
  );
  // EXACTLY ONE, and it is the one the daemon named. toEqual (not "contains") is the
  // whole point: a second badged card is the failure this reading was asked for.
  expect(badged.map((c) => c.name)).toEqual([fd!.name]);
});

test("a FAILED front-door read is a failure, never 'no primary agent'", async ({ page }) => {
  // The daemon is not asked to misbehave: the fault is injected into the one request
  // this reading is about, and a playwright route outranks the network.
  await page.route(FRONT_DOOR_API, (route) =>
    route.fulfill({
      status: 500,
      contentType: "application/json",
      body: JSON.stringify({ error: "e2e: injected /api/v1/primary failure" }),
    }),
  );
  await openHomeWithReadout(page);

  const card = page.locator(CARD);
  await expect(card).toBeVisible();
  // 1. the failure is shown, with the daemon's own reason and a retry
  await expect(card.locator(".error-state")).toBeVisible({ timeout: 15_000 });
  await expect(card).toContainText(FAILED_TEXT);
  await expect(card.locator(".error-state button")).toBeVisible();
  // 2. AND the empty-state sentence is NOT on screen: "we could not read it" is not
  // "there is nothing". This line is what makes the pair falsifiable instead of a
  // text match that the empty state would satisfy forever.
  await expect(card).not.toContainText(NONE_TEXT);
});

// READING E — the same rule on the OTHER surface that states it. Home says "cannot
// read" and so does the Agents page, whose primary band renders on every daemon; this
// reading pins the Agents wording AND its consequence (a failed read badges nothing --
// t48's ruling: the row shows only what it itself claims).
//
// It was written because A-D were invisible on a fresh daemon (4/4 skipped: the guide
// carried no readout), i.e. the file protected nothing in CI. t64 fixed the cause by
// moving the readout onto the guide, so A-D now run everywhere; E is kept because the
// two surfaces can regress independently -- Agents was already right, and nothing
// guarded it either.
test("agents shows 'cannot read', never 'no primary', and badges nothing", async ({ page }) => {
  await page.route(FRONT_DOOR_API, (route) =>
    route.fulfill({
      status: 500,
      contentType: "application/json",
      body: JSON.stringify({ error: "e2e: injected /api/v1/primary failure" }),
    }),
  );
  await page.goto("/#agents");
  await expect(page.locator(".agent-card, .ant-empty").first()).toBeVisible({ timeout: 15_000 });

  // 1. the failed read is stated (agents.primary.readErr, whose own hint forbids the
  //    wrong conclusion: "that is not the same as there is no primary agent")
  await expect(page.locator(".error-state")).toBeVisible({ timeout: 15_000 });
  await expect(page.locator("body")).toContainText(FAILED_TEXT);
  // 2. the empty-state sentence is nowhere on the page (agents.primary.none)
  await expect(page.locator("body")).not.toContainText(NONE_AGENTS_TEXT);
  // 3. a failed read badges nothing -- t48's ruling: the row shows only what it
  //    itself claims, because who actually answers is unknown this round
  const badged = await page.locator(".agent-card").evaluateAll(
    (els, pattern) => {
      const re = new RegExp(pattern as string);
      return els.filter((el) =>
        Array.from(el.querySelectorAll(".tag")).some((c) => re.test((c.textContent ?? "").trim())),
      ).length;
    },
    EFFECTIVE_BADGE.source,
  );
  expect(badged).toBe(0);
});
