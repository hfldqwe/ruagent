// Registry editing UI: create a runtime + a role through the real forms, then
// delete both. Backend semantics (409 guards, hot reload) live in the Rust
// integration test; this spec only covers the panel wiring.
//
// THIS SPEC WRITES THE DAEMON'S REAL CONFIG. It used to say it was "self-cleaning
// so it is safe on any daemon (the user's real one included)" -- that claim was
// measured false on 2026-09-24: step 2 threw, step 3's UI cleanup never ran, and
// [runtime.e2e-rt] + [agent.e2e-role] were left in the user's agents.toml.
//
// Two independent protections now, because one was not enough:
//   1. the spec refuses to run at all unless the suite's entry point armed it
//      (see write-guard.ts), so a bare "npx playwright test" cannot reach it;
//   2. the cleanup runs from a finally block and goes straight to the API, so a
//      failure in ANY step still removes the residue.

import { expect, test, type Page } from "@playwright/test";

import { removeResidue, writeAccess } from "./write-guard";

// The gate. Skipped -- with a named reason -- unless e2e/run-e2e.mjs armed it.
const access = writeAccess();
test.skip(!access.allowed, access.reason);

test("runtimes and roles can be created and deleted from the panel", async ({ page, request, baseURL }) => {
  try {
    await run(page);
  } finally {
    // NOT through the UI, and NOT conditional: this is the half that was missing
    // on 2026-09-24. It is idempotent, so running it after a clean pass is a
    // no-op that reports 404s, and running it after a step-2 failure still
    // removes whatever was created.
    await removeResidue(request, baseURL ?? "http://127.0.0.1:8787");
  }
});

async function run(page: Page) {
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
}
