// Command palette: real keyboard events end to end — open, filter,
// navigate, execute, close.

import { expect, test } from "@playwright/test";

test("Ctrl+K opens the palette with all four groups", async ({ page }) => {
  await page.goto("/");
  await expect(page.locator(".app-sider")).toBeVisible();

  await page.keyboard.press("Control+k");
  await expect(page.locator(".cmdk")).toBeVisible();
  for (const group of ["导航", "智能体", "会话", "动作"]) {
    await expect(page.locator(".cmdk-group").filter({ hasText: group })).toBeVisible();
  }
  // input is focused and the first item is selected
  await expect(page.locator(".cmdk-input-row input")).toBeFocused();
  await expect(page.locator(".cmdk-item.selected").first()).toContainText(/首页|Home/);
});

test("typing filters the command list", async ({ page }) => {
  await page.goto("/");
  await page.keyboard.press("Control+k");
  await expect(page.locator(".cmdk")).toBeVisible();

  await page.keyboard.type("知识");
  const items = page.locator(".cmdk-item");
  expect(await items.count()).toBeLessThan(10);
  await expect(
    page.locator(".cmdk-label").filter({ hasText: /知识库|Knowledge/ }).first(),
  ).toBeVisible();

  // nonsense narrows to the empty state
  await page.keyboard.press("Control+a");
  await page.keyboard.type("qqqzzz");
  await expect(page.locator(".cmdk-empty")).toBeVisible();
});

test("arrow keys move the selection", async ({ page }) => {
  await page.goto("/");
  await page.keyboard.press("Control+k");
  await expect(page.locator(".cmdk")).toBeVisible();

  const first = await page.locator(".cmdk-item.selected").first().textContent();
  await page.keyboard.press("ArrowDown");
  const second = await page.locator(".cmdk-item.selected").first().textContent();
  expect(second).not.toBe(first);

  await page.keyboard.press("ArrowUp");
  const back = await page.locator(".cmdk-item.selected").first().textContent();
  expect(back).toBe(first);
});

test("Enter executes a navigation command", async ({ page }) => {
  await page.goto("/");
  await page.keyboard.press("Control+k");
  await page.keyboard.type("知识");
  await page.keyboard.press("Enter");

  await expect(page.locator(".cmdk")).toHaveCount(0);
  await expect(page.locator(".view-bar h2")).toContainText(/知识库|Knowledge/);
  expect(page.url()).toContain("#knowledge");
});

test("Escape closes the palette", async ({ page }) => {
  await page.goto("/");
  await page.keyboard.press("Control+k");
  await expect(page.locator(".cmdk")).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(page.locator(".cmdk")).toHaveCount(0);
});

test("agent command navigates to #chat?agent= (when agents exist)", async ({ page, request }) => {
  const list = await request.get("/api/v1/agents");
  const { agents } = (await list.json()) as {
    agents: { name: string; enabled: boolean }[];
  };
  const enabled = agents.filter((a) => a.enabled);
  test.skip(enabled.length === 0, "no enabled agents configured");

  await page.goto("/");
  await page.keyboard.press("Control+k");
  await page.keyboard.type("对话");
  const target = page.locator(".cmdk-item").filter({ hasText: `对话：${enabled[0].name}` });
  await expect(target).toBeVisible();
  await target.click();
  await expect(page.locator(".cmdk")).toHaveCount(0);
  expect(page.url()).toContain(`#chat?agent=${encodeURIComponent(enabled[0].name)}`);
  await expect(page.locator(".composer")).toBeVisible();
});

test("the sidebar hint opens the palette too", async ({ page }) => {
  await page.goto("/");
  await page.locator(".kbd-hint").click();
  await expect(page.locator(".cmdk")).toBeVisible();
  await page.keyboard.press("Escape");
});
