// Chat experience (2026-09-18): the model picker fills from the
// daemon's cached catalog (mock advertises model/mode/effort), an echo
// round-trip lands in the history drawer, and clicking the live entry
// reattaches — the SSE replays the transcript. Only runs when the e2e
// mock agents are registered (the workflow does; a personal daemon with
// real agents skips — the Rust integration test covers the flow there,
// without spending real agent runs).

import { expect, test } from "@playwright/test";

test("chat round-trip lands in history and reattaches live", async ({ page, request }) => {
  const list = await request.get("/api/v1/agents");
  test.skip(!list.ok(), "agents API unavailable");
  const { agents } = (await list.json()) as {
    agents: { name: string; harness: string; enabled: boolean; prompt: string | null }[];
  };
  // A bare mock runtime card (no role prompt) — the direct-chat case.
  const mock = agents.find((a) => a.enabled && a.harness === "Mock" && !a.prompt);
  test.skip(!mock, "needs an e2e mock agent (chat with real runtimes costs money)");

  await page.goto("/#chat");
  await expect(page.locator(".view-bar h2")).toBeVisible();

  // The agent picker defaults to the first enabled agent; pick the mock.
  await page.locator(".chat-bar .chat-field").first().locator(".ant-select-input").click();
  await page.locator(".ant-select-item-option").filter({ hasText: mock.name }).click();

  // The model picker fills from the runtime's advertised catalog —
  // first probe (a real mock process) then the cached copy. antd 6
  // shows the value in `.ant-select-content`.
  const modelField = page.locator(".chat-bar .chat-field").nth(1);
  await expect(modelField.locator(".ant-select-content")).toHaveText("mock-pro", {
    timeout: 15_000,
  });

  // One round-trip: the echo reply streams back.
  await page.locator(".composer textarea").fill("e2e history probe");
  await page.locator(".send-btn").click();
  await expect(page.locator(".chat-msg.user").last()).toContainText("e2e history probe");
  await expect(page.locator(".chat-msg.agent").last()).toContainText("e2e history probe", {
    timeout: 15_000,
  });

  // History: the conversation is recorded with the first prompt as the
  // title, marked live.
  await page
    .locator("button")
    .filter({ hasText: /历\s*史|History/ })
    .first()
    .click();
  const row = page.locator(".ant-drawer .row-btn").first();
  await expect(row).toContainText("e2e history probe", { timeout: 10_000 });

  // Live entry: click reattaches, the transcript replay restores both
  // turns in the log. (antd 6 keeps `.ant-drawer` mounted when closed —
  // the open class is the reliable signal.)
  await row.click();
  await expect(page.locator(".ant-drawer.ant-drawer-open")).toHaveCount(0);
  await expect(page.locator(".chat-msg.user").last()).toContainText("e2e history probe", {
    timeout: 8_000,
  });
  await expect(page.locator(".chat-msg.agent").last()).toContainText("e2e history probe", {
    timeout: 8_000,
  });

  // Close the live chat through the API so the mock process exits
  // (the idle reaper would catch it, but the daemon stays clean).
  const hist = await request.get(`/api/v1/chats?agent=${encodeURIComponent(mock.name)}`);
  if (hist.ok()) {
    const { chats } = (await hist.json()) as { chats: { id: string; active: boolean }[] };
    const live = chats.find((c) => c.active);
    if (live) await request.delete(`/api/v1/chat/${live.id}`);
  }
});
