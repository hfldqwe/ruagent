// Wiki tab (M2 panel surface): the knowledge manager gains a docs/wiki
// Segmented; the wiki tab shows the page inventory (or its empty
// state), the compile button, and build history. Data-conditional —
// CI runs on a fresh daemon with no wiki pages.

import { expect, test } from "@playwright/test";

test("knowledge view exposes the wiki tab with compile entry", async ({ page }) => {
  await page.goto("/#knowledge");
  await expect(page.locator(".view-bar h2")).toBeVisible();

  await page.locator(".ant-segmented-item-label").filter({ hasText: /^Wiki$/ }).click();

  // the compile button and the stat line render on an empty wiki too
  await expect(
    page.locator("button").filter({ hasText: /编译 Wiki|Compile wiki/ }),
  ).toBeVisible();
  // page inventory rows or the empty state (fresh CI daemon has none)
  await expect(page.locator(".card .row-btn, .ant-empty").first()).toBeVisible({
    timeout: 10_000,
  });

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
