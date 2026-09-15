// Recall chain (conservative): query → stubs → inline expand → full
// text visible, cross-checked against the memory API. Read-only.
//
// Conservative recall always returns top matches for any query (the
// backend falls back to top items), so this works on an empty-ish
// database too — doctor seeds at least one memory in CI.

import { expect, test } from "@playwright/test";

test("conservative recall stubs expand to the full memory", async ({ page, request }) => {
  await page.goto("/#memory");
  await page.locator(".ant-segmented-item-label").filter({ hasText: "召回" }).click();
  await page.locator(".ant-segmented-item-label").filter({ hasText: "保守" }).click();

  await page.locator(".search-bar input.ant-input").fill("deploy");
  // antd auto-inserts a space between two CJK chars in buttons ("召回" → "召 回")
  await page.locator(".search-bar button").filter({ hasText: /召 ?回|Recall/ }).click();

  // memory stubs appear with the fetch affordance
  const stub = page.locator(".recall-stub-head").first();
  await expect(stub).toBeVisible();
  await expect(stub.locator(".stub-afford")).toBeVisible();

  // the stub carries #id — remember it for the API cross-check
  const headText = (await stub.textContent()) ?? "";
  const id = headText.match(/#(\d+)/)?.[1];
  expect(id, "stub exposes its memory id").toBeTruthy();

  // expand inline
  await stub.click();
  const body = page.locator(".recall-hit .recall-body").first();
  await expect(body).toBeVisible();
  const bodyText = (await body.textContent()) ?? "";
  expect(bodyText.length).toBeGreaterThan(10);

  // cross-check with the API — the expansion is memory_get, not a mock
  if (id) {
    const res = await request.get(`/api/v1/memory/${id}`);
    const { memory } = (await res.json()) as { memory: { content: string } };
    expect(bodyText.trim()).toContain(memory.content.trim().slice(0, 40));
  }

  // toggle collapses again (height animates to 0)
  await stub.click();
  await expect(page.locator(".recall-hit .recall-expand.open").first()).toHaveCount(0);
});
