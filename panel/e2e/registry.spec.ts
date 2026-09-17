// Registry editing UI: create a runtime + a role through the real forms,
// then delete both — self-cleaning so it is safe on any daemon (the user's
// real one included; agents.toml is round-tripped and restored).
// Backend semantics (409 guards, hot reload) live in the Rust integration
// test; this spec only covers the panel wiring.

import { expect, test } from "@playwright/test";

test("runtimes and roles can be created and deleted from the panel", async ({ page }) => {
  await page.goto("/#runtimes");
  await expect(page.locator(".view-bar h2")).toBeVisible();
  await expect(
    page.locator("button").filter({ hasText: /新建运行时|New Runtime/ }),
  ).toBeVisible();

  // 1. Create a runtime through the modal.
  const before = await page.locator(".agent-grid .agent-card").count();
  await page.locator("button").filter({ hasText: /新建运行时|New Runtime/ }).first().click();
  await page.locator(".ant-modal input.ant-input").first().fill("e2e-rt");
  await page.locator(".ant-modal .ant-select-input").click();
  await page.locator(".ant-select-item-option").filter({ hasText: "mock" }).click();
  const inputs = page.locator(".ant-modal input.ant-input");
  await inputs.nth(1).fill("ruagent-mock-agent --behavior echo");
  await page
    .locator(".ant-modal button")
    .filter({ hasText: /^新建运行时$|^New Runtime$/ })
    .click();
  await expect(
    page.locator(".agent-grid .agent-card").filter({ hasText: "e2e-rt" }),
  ).toBeVisible({ timeout: 10_000 });
  expect(await page.locator(".agent-grid .agent-card").count()).toBe(before + 1);

  // 2. Create a role on it (agents page).
  await page.goto("/#agents");
  await page.locator("button").filter({ hasText: /新建角色|New Role/ }).first().click();
  await page.locator(".ant-modal input.ant-input").first().fill("e2e-role");
  await page.locator(".ant-modal .ant-select-input").first().click();
  await page.locator(".ant-select-item-option").filter({ hasText: "e2e-rt" }).click();
  await page.locator(".ant-modal textarea").fill("You are an e2e probe.");
  await page
    .locator(".ant-modal button")
    .filter({ hasText: /^新建角色$|^New Role$/ })
    .click();
  const roleCard = page.locator(".agent-grid .agent-card").filter({ hasText: "e2e-role" });
  await expect(roleCard).toBeVisible({ timeout: 10_000 });
  await expect(roleCard.locator(".tag").first()).toContainText("e2e-rt");

  // 3. Cleanup through the UI: role first (the runtime is in use).
  await page.reload();
  await page
    .locator(".agent-grid .agent-card")
    .filter({ hasText: "e2e-role" })
    .locator("button")
    .filter({ hasText: /删 除|Delete/ })
    .click();
  await page
    .locator(".ant-popover button")
    .filter({ hasText: /删 除|Delete/ })
    .first()
    .click();
  await expect(
    page.locator(".agent-grid .agent-card").filter({ hasText: "e2e-role" }),
  ).toHaveCount(0, { timeout: 10_000 });

  await page.goto("/#runtimes");
  await page
    .locator(".agent-grid .agent-card")
    .filter({ hasText: "e2e-rt" })
    .locator("button")
    .filter({ hasText: /删 除|Delete/ })
    .click();
  await page
    .locator(".ant-popover button")
    .filter({ hasText: /删 除|Delete/ })
    .first()
    .click();
  await expect(
    page.locator(".agent-grid .agent-card").filter({ hasText: "e2e-rt" }),
  ).toHaveCount(0, { timeout: 10_000 });
});
