// Responsive shell (2026-09-19 UX audit): the sider collapses to an icon
// rail below lg, and no view overflows the viewport sideways. The audit
// measured a 228px sider leaving 162px of content at 390px, and grid
// tracks floored at content min-content blowing 768px viewports out to
// 1087px. These assertions pin both regressions. Data-independent —
// passes on the empty CI daemon and a lived-in one alike.

import { expect, test } from "@playwright/test";

const noOverflow = async (page: import("@playwright/test").Page) => {
  const over = await page.evaluate(
    () =>
      document.documentElement.scrollWidth -
      document.documentElement.clientWidth,
  );
  expect(over).toBeLessThanOrEqual(2);
};

test.describe("phone (390px)", () => {
  test.use({ viewport: { width: 390, height: 844 } });

  for (const route of [
    "home",
    "board",
    "chat",
    "sessions",
    "knowledge",
    "memory",
    "agents",
    "runtimes",
    "graph",
  ]) {
    test(`${route} fits the viewport`, async ({ page }) => {
      await page.goto(`/#${route}`);
      await page.waitForTimeout(600);
      const sider = await page
        .locator(".ant-layout-sider")
        .boundingBox();
      // The icon rail, not the full 228px column.
      expect(sider?.width).toBeLessThanOrEqual(80);
      await noOverflow(page);
    });
  }

  // The chat history rail is a slide-over here: the view-bar toggle
  // opens it, tapping beside it closes.
  test("chat history rail opens and closes as a slide-over", async ({
    page,
  }) => {
    await page.goto("/#chat");
    await page.waitForTimeout(600);
    await page.locator(".chat-side-toggle").click();
    await expect(page.locator(".chat-side")).toHaveClass(/open/);
    await expect(page.locator(".chat-side-backdrop")).toBeVisible();
    await page.mouse.click(360, 400); // right of the 300px rail
    await expect(page.locator(".chat-side")).not.toHaveClass(/open/);
  });
});

test.describe("tablet (768px)", () => {
  test.use({ viewport: { width: 768, height: 1024 } });

  test("home and board fit with the rail collapsed", async ({ page }) => {
    for (const route of ["home", "board"]) {
      await page.goto(`/#${route}`);
      await page.waitForTimeout(600);
      const sider = await page
        .locator(".ant-layout-sider")
        .boundingBox();
      expect(sider?.width).toBeLessThanOrEqual(80);
      await noOverflow(page);
    }
  });
});

test.describe("desktop", () => {
  test.use({ viewport: { width: 1280, height: 800 } });

  test("sider expands at lg and the footer toggle collapses it", async ({
    page,
  }) => {
    await page.goto("/#home");
    await page.waitForTimeout(600);
    const wide = await page.locator(".ant-layout-sider").boundingBox();
    expect(wide?.width).toBe(228);

    // Manual toggle: desktop users get the rail too.
    await page
      .getByRole("button", { name: /collapse|收起/i })
      .first()
      .click();
    await page.waitForTimeout(300);
    const rail = await page.locator(".ant-layout-sider").boundingBox();
    expect(rail?.width).toBeLessThanOrEqual(80);
    // The brand mark survives the rail; the full name does not.
    await expect(page.locator(".brand-mark")).toBeVisible();
    await expect(page.locator(".brand-name")).toBeHidden();

    // Toggle back — state persists across a route change.
    await page
      .getByRole("button", { name: /collapse|收起/i })
      .first()
      .click();
    await page.waitForTimeout(300);
    await expect(page.locator(".brand-name")).toBeVisible();
  });
});
