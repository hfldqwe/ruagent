// i18n: switching language re-renders the navigation.

import { expect, test } from "@playwright/test";

test("language toggle switches the nav vocabulary", async ({ page }) => {
  await page.goto("/");
  await expect(page.locator("html")).toHaveAttribute("lang", "zh-CN");
  await expect(page.locator(".ant-menu-item").first()).toContainText("首页");

  await page.locator(".sidebar-foot button").filter({ hasText: "EN" }).click();

  await expect(page.locator("html")).toHaveAttribute("lang", "en");
  await expect(page.locator(".ant-menu-item").first()).toContainText("Home");

  // and back
  await page.locator(".sidebar-foot button").filter({ hasText: "中" }).click();
  await expect(page.locator("html")).toHaveAttribute("lang", "zh-CN");
  await expect(page.locator(".ant-menu-item").first()).toContainText("首页");
});
