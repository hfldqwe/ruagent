// The consumption surface, four pages, one DOM-level reading each.
//
// What this spec exists for is the two display rules the integration contract
// owes the reader — both are about NOT inventing a number:
//
//   (1) SIDE-BY-SIDE READINGS CARRY THEIR UNIT (or are made non-comparable).
//       The memory page's recall log puts two scores next to each other that
//       have different dimensions: the memory leg's cosine (0.86) and the
//       knowledge leg's unitless RRF rank score (0.016). The memory leg used to
//       render as a bare `m 0.86`, which invites comparing them.
//
//   (2) `number | null` RENDERS ITS THIRD STATE (`unknown`), NEVER 0.00/1.00,
//       AND THE KEY IS NEVER OMITTED. `cite_coverage` is the build's reading and
//       is `null` when no build recorded one (a hand-written page) or when the
//       recorded one no longer applies. Rendering that as 0.00 or 1.00 would be
//       a fabricated measurement (RV-D-1).
//
// It runs against whatever daemon `E2E_BASE_URL` points at — a THROWAWAY root in
// this task's run, never the operator's live daemon. The fixtures it writes go
// through the same APIs the panel uses and are removed in afterAll.

import { expect, test } from "@playwright/test";

const WIKI_SLUG = "e2e-consumption-probe";
const DOC = "e2e-consumption-doc";
const ENTITY = "e2e-consumption-entity";
const WIKI_PAGE = [
  "---",
  "title: Consumption probe",
  'summary: "三态显示探针"',
  "aliases: []",
  "entities: []",
  "sources: []",
  "status: generated",
  'generated_at: "2026-09-28T00:00:00+00:00"',
  "generator: mock",
  "build: 0",
  "---",
  "# Consumption probe",
  "",
  "A hand-written page: no build ever recorded a verifiability reading for it.",
  "",
].join("\n");

test.beforeAll(async ({ request }) => {
  // A hand-written page lands with NO `wiki_pages` row ⇒ its coverage is
  // unknown by construction — which is exactly the state this rule is about.
  await request.put(`/api/v1/knowledge/raw/wiki/${WIKI_SLUG}`, {
    data: { content: WIKI_PAGE },
  });
  // One memory, so the recall log row below has a memory-leg score to show.
  // `namespace: user` — `global` is refused by the write path (`HTTP 200
  // {"outcome":"RejectedNamespace"}`), which is reported as a finding in
  // gen2-wiki-consumption.md rather than worked around silently here.
  await request.post("/api/v1/memory/write", {
    data: {
      store: "observation",
      namespace: "user",
      content: "consumption probe memory: the recall log reads the memory leg",
    },
  });
  // One recall call, so the recall log has a row. This writes `recall_log` in
  // the THROWAWAY database this run points at.
  await request.get("/api/v1/recall?q=consumption&strategy=aggressive&top_n=3");
});

test.afterAll(async ({ request }) => {
  await request.delete(`/api/v1/knowledge/raw/wiki/${WIKI_SLUG}`).catch(() => {});
  await request.delete(`/api/v1/knowledge/raw/${DOC}`).catch(() => {});
});

/** The row's text, whitespace-normalised: what a reader actually sees. */
async function textOf(locator: import("@playwright/test").Locator) {
  return ((await locator.textContent()) ?? "").replace(/\s+/g, " ").trim();
}

test("Wiki page: a page with no build reading shows coverage unknown", async ({
  page,
  request,
}) => {
  // The API first, so the DOM reading below is not read as "nothing to show".
  const res = await request.get("/api/v1/knowledge/wiki/pages");
  expect(res.ok(), "wiki pages API").toBeTruthy();
  const { pages } = (await res.json()) as {
    pages: { slug: string; cite_coverage?: number | null }[];
  };
  const row = pages.find((p) => p.slug === WIKI_SLUG);
  expect(row, `the probe page must be listed (${WIKI_SLUG})`).toBeTruthy();
  expect(
    row?.cite_coverage,
    "no build wrote this page, so its reading is null",
  ).toBeNull();

  await page.goto("/#knowledge");
  await page
    .locator(".ant-segmented-item-label")
    .filter({ hasText: /^Wiki$/ })
    .click();
  const dom = page.locator(".card .row-btn").filter({ hasText: WIKI_SLUG });
  await expect(dom).toBeVisible({ timeout: 10_000 });
  const text = await textOf(dom);
  console.log(`DOM READING wiki page row: ${text}`);

  // The key is present and reads `unknown` — in the same vocabulary the other
  // third states use.
  const badge = dom.locator('[aria-label*="coverage"], [aria-label*="覆盖"]');
  await expect(badge).toBeVisible();
  const label = (await badge.getAttribute("aria-label")) ?? "";
  console.log(`DOM READING wiki coverage badge: ${label}`);
  expect(label).toMatch(/unknown/i);
  // …and it is NOT a fabricated 0.00/1.00.
  expect(text).not.toMatch(/coverage 0\.0|覆盖 0\.0|coverage 1\.0|覆盖 1\.0/);
  expect(text).not.toMatch(/coverage 0[.,]00|覆盖 0[.,]00/);
});

test("Memory page: the recall log labels each score with its dimension", async ({
  page,
}) => {
  await page.goto("/#memory");
  await page
    .locator(".ant-segmented-item-label")
    .filter({ hasText: /审计日志|Audit Log/ })
    .click();

  const row = page
    .locator(".row")
    .filter({ hasText: /余弦|cosine/ })
    .first();
  await expect(row).toBeVisible({ timeout: 10_000 });
  const text = await textOf(row);
  console.log(`DOM READING memory recall-log row: ${text}`);

  // (1) the memory leg carries its unit…
  expect(text).toMatch(/余弦|cosine/);
  // …and the knowledge leg beside it keeps its own, so the two cannot be read
  // as the same kind of number.
  expect(text).toMatch(/名次分|rank score/);
  // The falsifier for the fix: a bare `m 0.86` next to a labelled rank score.
  expect(text).not.toMatch(/(?:^|\s)m \d/);
});

test("Knowledge page: a hit's score carries its dimension", async ({
  page,
  request,
}) => {
  await request.put(`/api/v1/knowledge/raw/${DOC}`, {
    data: {
      content: "# consumption probe\n\nThe consumption probe document body is searchable.",
    },
  });

  await page.goto("/#knowledge");
  await page.locator(".search-bar input.ant-input").fill("consumption");
  await page
    .locator(".search-bar button")
    .filter({ hasText: /搜 ?索|Search/ })
    .click();

  const hit = page.locator(".search-hit").first();
  await expect(hit).toBeVisible({ timeout: 10_000 });
  const text = await textOf(hit);
  console.log(`DOM READING knowledge hit: ${text}`);
  // The fusion score is unitless and says so; the leg scores name their own
  // dimension (distance / bm25) when the panel shows them.
  expect(text).toMatch(/名次分|rank score|语义腿|semantic leg/);
});

test("Graph page: an entity reaches the DOM", async ({ page, request }) => {
  // `POST /api/v1/graph/entity` (singular) is the create route; `/entities` is
  // the read-only list.
  const made = await request.post("/api/v1/graph/entity", {
    data: { name: ENTITY, kind: "probe", summary: "consumption probe entity" },
  });
  test.skip(!made.ok(), "graph entity API unavailable");

  await page.goto("/#graph");
  await expect(page.locator(".view-bar")).toBeVisible();
  const bar = await textOf(page.locator(".view-bar"));
  console.log(`DOM READING graph view bar: ${bar}`);

  // The entity list is the `list` mode of the same view (the graph mode paints a
  // canvas, which carries no readable text). Switching is how a reader reads it.
  await page
    .locator(".ant-segmented-item-label")
    .filter({ hasText: /^列表$|^List$/ })
    .click();

  const hit = page
    .locator(".card .row-btn, .row")
    .filter({ hasText: ENTITY })
    .first();
  await expect(hit).toBeVisible({ timeout: 10_000 });
  const text = await textOf(hit);
  console.log(`DOM READING graph entity row: ${text}`);
  expect(text).toContain(ENTITY);
});
