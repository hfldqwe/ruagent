// The graph inspector's ALIAS row: the last hop of a chain that had already
// crossed three tasks.
//
// WHY THIS FILE EXISTS (ruagent-close-the-gaps t27). `GET /api/v1/graph/entity/{id}`
// has carried `aliases` since t22 — the folded spellings, i.e. the graph's record
// that `AMBIGUOUS（模糊）` and `模糊` are one concept — and the panel showed none of
// it: `GraphEntity` (panel/src/api.ts) had no such field, and the one accessor for
// that route returned `.facts` alone while having no caller at all. The suite was
// green over it (`grep -n alias panel/e2e/*.spec.ts` matched nothing at the time).
// This is the coverage for the row that closes that loop.
//
// THIS SPEC IS READ-ONLY. It writes no daemon config and no graph row, so
// `writeAccess()` is deliberately NOT called and adding it to AGENTS.md's
// writer table would be wrong — the same class as failure-visibility.spec.ts,
// which also injects responses instead of writing.
//
// WHY IT STUBS TWO RESPONSES — and why that is not the tautology this suite has
// been caught by before. An entity that HAS aliases cannot be obtained read-only,
// and it cannot be obtained by WRITING either without permanent damage, MEASURED:
// the only route that writes an alias is `POST /api/v1/graph/resolution/merge`,
// and after that merge `DELETE /api/v1/graph/entity/{id}` answers
// **500 `sqlite error: FOREIGN KEY constraint failed`** (the row in
// `entity_aliases` references `entities(id)` with no ON DELETE CASCADE and the
// daemon runs `foreign_keys=ON`), while a plain entity deletes with 200. A spec
// that merged two entities to test a ROW would therefore plant an entity in
// whatever daemon it ran against that the operator could not remove from the
// panel either — a data-loss-shaped mistake of exactly the kind the write guard
// exists to prevent, and one no cleanup block could undo.
//
// So this spec stubs ONLY the two responses whose SHAPE it is testing — the row
// list (to have a row to click) and the per-entity payload (to supply aliases) —
// and lets every other request reach the real daemon. The stub is keyed on an id
// no daemon has (999001) on purpose: the panel's own extra requests for that
// entity (`/facts`, `/neighbors`) then get the daemon's REAL answers, which is how
// the inspector-renders-facts assertions below stay live-data assertions.
//
// WHAT THIS SPEC ASSERTS THAT WOULD BREAK:
//   * the WIRE (not stubbed): the real route answers 200 with `name`, `kind`,
//     `aliases`, `facts` and `aliases` is an array. Drop or rename the field on the
//     route side and this is what fails first.
//   * the ROW: the inspector renders each alias VERBATIM, under a label, with the
//     count on the row. Delete the alias row from EntityDetail (or rename the field
//     the panel reads) and it fails — see the negative control in the report.
//   * the COMMON CASE: an entity with `[]` — and a daemon answering without the
//     field at all — renders NO alias section, while the inspector itself still
//     renders. Those two are one case in the view, and both are pinned.

import { expect, test, type Page } from "@playwright/test";

/** The two responses this file does NOT take from the live daemon.
 *
 *  These are URL PREDICATES, not globs, and the exactness is load-bearing: a glob
 *  ending in a single star raises the question whether that star crosses a slash,
 *  and if it did it would swallow the `/facts` sub-route too and hand the facts
 *  request the alias payload. These match the list route exactly and the
 *  per-entity route with NO further path segment — so `/facts` and `/neighbors`
 *  are always real requests. */
const ENTITY_LIST = (url: URL) => url.pathname === "/api/v1/graph/entities";
const ENTITY_DETAIL = (url: URL) => {
  const base = "/api/v1/graph/entity/";
  return url.pathname.startsWith(base) && !url.pathname.slice(base.length).includes("/");
};

/** The entity the inspector is opened for, and its folded spellings. The name and
 *  aliases are the shape the graph really records (t22's own example). */
const NAME = "AMBIGUOUS（模糊）";
const ALIASES = ["模糊", "AMBIGUOUS"];
/** An id no daemon has: the panel's other requests for it get the real daemon's
 *  own "empty" answers instead of touching live rows. */
const ID = 999001;

/** Open the inspector for the stubbed row, with `detail` as the per-entity
 *  payload. Only the LIST route is stubbed besides it. */
async function openInspector(page: Page, detail: unknown): Promise<void> {
  await page.route(ENTITY_LIST, (route) =>
    route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({
        entities: [[{ id: ID, name: NAME, kind: "concept", summary: null }, 0]],
      }),
    }),
  );
  await page.route(ENTITY_DETAIL, (route) =>
    route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify(detail),
    }),
  );

  await page.goto("/#graph?mode=list");
  const row = page.locator(".row-btn").filter({ hasText: NAME }).first();
  await expect(row).toBeVisible({ timeout: 10_000 });
  await row.click();
  // The inspector is showing an ENTITY. The unselected placeholder shares the
  // `.graph-detail` class but has no `h3` (it is a zone-head plus a hint), so `h3`
  // is what says the assertions below are about an entity rather than about a
  // panel that never opened.
  await expect(page.locator(".graph-detail h3")).toBeVisible({ timeout: 10_000 });
}

test("the per-entity route carries `aliases`: the field the inspector reads", async ({
  request,
}) => {
  // NOT stubbed, and not data-dependent: for an id the graph does not know the
  // route answers 200 with the four keys and empty collections (it deliberately
  // does not 404), so this pins the WIRE without needing a merged entity. It is the
  // half a stub cannot cover -- rename the field on the route side and the panel
  // renders nothing, which no stubbed assertion would notice.
  const res = await request.get(`/api/v1/graph/entity/${ID}`);
  expect(res.status(), "GET /api/v1/graph/entity/{id}").toBe(200);
  const body = (await res.json()) as Record<string, unknown>;
  for (const key of ["name", "kind", "aliases", "facts"]) {
    expect(Object.keys(body), `the response must carry \`${key}\``).toContain(key);
  }
  expect(Array.isArray(body.aliases), "`aliases` must be an array").toBe(true);
});

test("the inspector shows an entity's aliases, verbatim", async ({ page }) => {
  await openInspector(page, { name: NAME, kind: "concept", aliases: ALIASES, facts: [] });

  const row = page.locator(".graph-detail .entity-aliases");
  await expect(row).toBeVisible();
  // The row is LABELLED: tags floating without a name for what they are would make
  // the reader guess that these are other names for the entity.
  await expect(row).toContainText(/别名|Aliases/);
  await expect(row).toHaveAttribute("data-alias-count", String(ALIASES.length));
  // THE ASSERTION THIS FILE EXISTS FOR: each folded name is on screen as text.
  // A row that rendered a count, a placeholder, or nothing at all fails here, and so
  // does one that renders a different set.
  for (const alias of ALIASES) {
    await expect(row.locator(`[data-alias="${alias}"]`)).toHaveText(alias);
  }
});

test("an entity with NO aliases renders no alias section at all", async ({ page }) => {
  await openInspector(page, { name: NAME, kind: "concept", aliases: [], facts: [] });

  // The inspector IS showing an entity -- so the absence asserted below is about
  // the alias row, not about a view that failed to load. The facts zone is the
  // discriminator, and it is a REAL response (the daemon answers for id 999001).
  await expect(page.locator(".graph-detail h3")).toContainText(NAME);
  await expect(page.locator(".graph-detail .zone-title").first()).toBeVisible();
  // Most entities have no aliases: an empty section here would state that a section
  // exists where the data says nothing.
  await expect(page.locator(".graph-detail .entity-aliases")).toHaveCount(0);
});

test("a daemon that answers WITHOUT `aliases` still renders the inspector", async ({ page }) => {
  // The field is ADDITIVE on the wire, so the panel must not require it. This is the
  // response an older daemon gives, and `undefined` has to mean what `[]` means: no
  // row, and above all no blanked inspector.
  await openInspector(page, { name: NAME, kind: "concept", facts: [] });

  await expect(page.locator(".graph-detail h3")).toContainText(NAME);
  await expect(page.locator(".graph-detail .zone-title").first()).toBeVisible();
  await expect(page.locator(".graph-detail .entity-aliases")).toHaveCount(0);
});
