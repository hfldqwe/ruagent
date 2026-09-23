// Wiki tab (M2 panel surface): the knowledge manager gains a docs/wiki
// Segmented; the wiki tab shows the page inventory (or its empty
// state), the compile button, and build history. Data-conditional —
// CI runs on a fresh daemon with no wiki pages.

import { expect, test } from "@playwright/test";

test("knowledge view exposes the wiki tab with compile entry", async ({ page, request }) => {
  await page.goto("/#knowledge");
  await expect(page.locator(".view-bar h2")).toBeVisible();

  await page.locator(".ant-segmented-item-label").filter({ hasText: /^Wiki$/ }).click();

  // The compile entry belongs to the Wiki zone's OWN header (Wiki.tsx hands it
  // to Zone as its actions slot), so it renders on an empty wiki too -- that
  // is the entry point this test is about. Scope to that header: a fresh CI
  // daemon also renders the empty state's own CTA with the same accessible
  // name (Wiki.tsx Empty action), which made the previous unscoped
  // page.locator('button') name filter resolve to 2 elements. The ambiguity
  // was in the locator, not in the panel: the two buttons live in different
  // regions (header vs empty-state body) and only the header one is
  // state-independent.
  const headerCompile = page
    .locator(".zone-head button")
    .filter({ hasText: /编译 Wiki|Compile wiki/ });
  await expect(headerCompile).toHaveCount(1);
  await expect(headerCompile).toBeVisible();

  // Data-conditional: an empty library additionally offers the CTA inside the
  // empty state (this is the second button CI sees).
  const res = await request.get("/api/v1/knowledge/wiki/pages");
  const pages = res.ok() ? ((await res.json()) as { pages: unknown[] }).pages : [];
  if (pages.length === 0) {
    await expect(
      page.locator(".empty-state button").filter({ hasText: /编译 Wiki|Compile wiki/ }),
    ).toBeVisible();
  } else {
    // page inventory rows (a set: at least one row is the assertion)
    await expect(page.locator(".card .row-btn").first()).toBeVisible({
      timeout: 10_000,
    });
  }

  // and back to docs
  await page.locator(".ant-segmented-item-label").filter({ hasText: /文档|Docs/ }).click();
  await expect(page.locator(".search-bar input.ant-input")).toBeVisible();
});

test("recall returns wiki hits as their own labeled section", async ({ page, request }) => {
  const res = await request.get("/api/v1/knowledge/wiki/pages");
  test.skip(!res.ok(), "wiki pages API unavailable");
  const { pages } = (await res.json()) as { pages: { slug: string; title: string }[] };
  test.skip(pages.length === 0, "no wiki pages in this database");

  // query for a term that lives in a wiki page: its title text
  await page.goto("/#memory");
  await page.locator(".ant-segmented-item-label").filter({ hasText: "召回" }).click();
  await page.locator(".ant-segmented-item-label").filter({ hasText: "激进" }).click();
  await page.locator(".search-bar input.ant-input").fill(pages[0].slug);
  await page.locator(".search-bar button").filter({ hasText: /召 ?回|Recall/ }).click();

  await expect(page.locator(".recall-hit, .ant-empty").first()).toBeVisible({
    timeout: 10_000,
  });

  // the wiki stub: "wiki" tag + title, ALWAYS a stub (§13-2)
  const stub = page.locator(".recall-hit").filter({ hasText: "wiki" }).first();
  await expect(stub).toBeVisible();
  await expect(stub.locator(".recall-stub-head")).toContainText(pages[0].title || pages[0].slug);
});

test("the page viewer renders pages without leaking frontmatter", async ({ page, request }) => {
  const res = await request.get("/api/v1/knowledge/wiki/pages");
  test.skip(!res.ok(), "wiki pages API unavailable");
  const { pages } = (await res.json()) as { pages: { slug: string; title: string }[] };
  test.skip(pages.length === 0, "no wiki pages in this database");

  await page.goto("/#knowledge");
  await page.locator(".ant-segmented-item-label").filter({ hasText: /^Wiki$/ }).click();
  await page.locator(".card .row-btn").first().click();

  // the page renders as markdown (H1 present) — a raw dump would show
  // the frontmatter block instead
  await expect(page.locator(".wiki-view h1")).toBeVisible({ timeout: 8_000 });
  await expect(page.locator(".wiki-view")).not.toContainText("source_hashes");
  // and the raw editor is one click away
  await expect(
    page.locator("button").filter({ hasText: /编辑此页|Edit page/ }),
  ).toBeVisible();
});
