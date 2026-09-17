// View smoke tests: every route renders its key elements.
//
// Data discipline: assertions tolerate an empty database (CI runs the
// suite right after doctor, which seeds only one legacy knowledge doc,
// one memory, and one graph entity — no sessions, no tasks, maybe no
// agents). Where a view can show either content or its empty state,
// we assert "one of them" rather than counting rows.

import { expect, test } from "@playwright/test";

/** The view did not crash and shows one of its possible bodies.
 * Polls the union locator — an instant isVisible() check races view
 * loading when the suite runs in parallel (canvas/doc rows render
 * after the API call lands). */
async function expectOneVisible(
  page: import("@playwright/test").Page,
  selectors: string[],
) {
  await expect(page.locator(selectors.join(", ")).first()).toBeVisible({
    timeout: 10_000,
  });
}

test("home renders the dashboard or the first-run guide", async ({ page }) => {
  await page.goto("/");
  await expect(page.locator(".home-hero h1")).toBeVisible();
  // sessions === 0 (fresh daemon) -> intro guide; otherwise dashboard
  await expectOneVisible(page, [".dash-grid", ".guide-grid"]);
  // the dashboard variant must show the recent-sessions panel header
  if (await page.locator(".dash-grid").count()) {
    await expect(page.locator(".dash-head").first()).toContainText(/会话|Sessions/);
  }
});

test("chat renders pickers and the composer", async ({ page }) => {
  await page.goto("/#chat");
  await expect(page.locator(".composer")).toBeVisible();
  await expect(page.locator(".composer .send-btn")).toBeVisible();
  // agent + model pickers always render; runtime / reasoning-effort
  // appear only when the selected agent advertises them.
  const pickers = page.locator(".chat-field");
  await expect(pickers.first()).toBeVisible();
  expect(await pickers.count()).toBeGreaterThanOrEqual(2);
});

test("sessions renders the list or its empty state", async ({ page }) => {
  await page.goto("/#sessions");
  await expect(page.locator(".view-bar h2")).toBeVisible();
  await expectOneVisible(page, [".row-btn", ".ant-empty"]);
});

test("board renders kanban columns or its empty state", async ({ page }) => {
  await page.goto("/#board");
  await expect(page.locator(".view-bar h2")).toBeVisible();
  await expectOneVisible(page, [".kanban-col", ".ant-empty"]);
});

test("memory renders its three tabs", async ({ page }) => {
  await page.goto("/#memory");
  for (const label of ["浏览", "召回", "审计日志"]) {
    await expect(page.locator(".ant-segmented-item-label").filter({ hasText: label })).toBeVisible();
  }
});

test("knowledge renders docs, rebuild and search", async ({ page }) => {
  await page.goto("/#knowledge");
  await expect(page.locator(".view-bar h2")).toBeVisible();
  await expect(page.locator(".search-bar input")).toBeVisible();
  await expect(
    page.locator(".view-bar button").filter({ hasText: /重建索引|Rebuild index/ }),
  ).toBeVisible();
  await expectOneVisible(page, [".row-btn", ".ant-empty"]);
});

test("graph renders the canvas or its empty state, with the mode toggle", async ({ page }) => {
  await page.goto("/#graph");
  for (const label of [/^图$/, /^列表$/]) {
    await expect(page.locator(".ant-segmented-item-label").filter({ hasText: label })).toBeVisible();
  }
  await expectOneVisible(page, ["canvas.graph-canvas", ".ant-empty"]);
});

test("agents view lists roles (runtimes have their own page)", async ({ page }) => {
  await page.goto("/#agents");
  await expect(page.locator(".view-bar h2")).toBeVisible();
  // Roles only — no tab switcher (the two layers are separate pages).
  await expect(page.locator(".ant-segmented")).toHaveCount(0);
  await expectOneVisible(page, [".agent-card", ".ant-empty"]);
});

test("runtimes page lists the execution backends", async ({ page }) => {
  await page.goto("/#runtimes");
  await expect(page.locator(".view-bar h2")).toBeVisible();
  await expectOneVisible(page, [".agent-card", ".ant-empty"]);
});

test("stats renders the table or its empty state", async ({ page }) => {
  await page.goto("/#stats");
  await expect(page.locator(".view-bar h2")).toBeVisible();
  await expectOneVisible(page, [".ant-table", ".ant-empty"]);
});

test("inbox renders the empty state or the pending list", async ({ page }) => {
  await page.goto("/#inbox");
  await expect(page.locator(".view-bar h2")).toBeVisible();
  await expectOneVisible(page, [".inbox-card", ".ant-empty"]);
});

test("task detail renders the launcher and run sections", async ({ page, request }) => {
  // Idempotent: create a scratch task, open it, delete it after.
  const made = await request.post("/api/v1/tasks", {
    data: { title: "e2e smoke task", intent: "e2e smoke task" },
  });
  test.skip(!made.ok(), "task creation unavailable");
  const task = (await made.json()) as { id: string };
  try {
    await page.goto(`/#task/${task.id}`);
    await expect(page.locator(".view-bar h2")).toContainText("e2e smoke task");
    await expect(page.locator(".card.launcher")).toBeVisible();
  } finally {
    await request.delete(`/api/v1/tasks/${task.id}`);
  }
});
