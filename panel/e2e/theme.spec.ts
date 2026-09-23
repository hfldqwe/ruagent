// Theme toggle: the mode attribute flips AND the painted background
// actually changes — checked at the pixel level (screenshot buffer),
// not just the computed style.

import { expect, test } from "@playwright/test";

test("theme toggle switches mode and repaints the background", async ({ page }) => {
  await page.goto("/?mode=dark");
  await expect(page.locator(".app-sider")).toBeVisible();

  await expect(page.locator("html")).toHaveAttribute("data-mode", "dark");
  const darkBg = await page.evaluate(
    () => getComputedStyle(document.documentElement).backgroundColor,
  );
  const darkShot = await page.screenshot({ clip: { x: 2, y: 2, width: 12, height: 12 } });

  // The accessible name is localized now (t79): select by role + either
  // language instead of pinning the English string.
  await page.getByRole("button", { name: /toggle theme|切换主题/i }).click();
  await expect(page.locator("html")).toHaveAttribute("data-mode", "light");

  const lightBg = await page.evaluate(
    () => getComputedStyle(document.documentElement).backgroundColor,
  );
  const lightShot = await page.screenshot({ clip: { x: 2, y: 2, width: 12, height: 12 } });

  expect(darkBg).not.toBe(lightBg);
  // pixel-level: the same corner must actually repaint
  expect(darkShot.equals(lightShot)).toBe(false);

  // the persisted choice survives a reload — navigate to a CLEAN url
  // (/?mode=dark is a deep link and legitimately re-applies dark on
  // every load; persistence is about localStorage winning without it)
  await page.goto("/");
  await expect(page.locator("html")).toHaveAttribute("data-mode", "light");
});
