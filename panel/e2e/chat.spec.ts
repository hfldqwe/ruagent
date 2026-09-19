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
  // (The chat-bar's first field is the project directory now — target
  // the agent field by its label.)
  await page
    .locator(".chat-bar .chat-field")
    .filter({ hasText: /智能体|Agent/ })
    .first()
    .locator(".ant-select-input")
    .click();
  await page.locator(".ant-select-item-option").filter({ hasText: mock.name }).click();

  // The model picker fills from the runtime's advertised catalog —
  // first probe (a real mock process) then the cached copy. antd 6
  // shows the value in `.ant-select-content`.
  const modelField = page.locator(".chat-bar .chat-field").nth(1);
  await expect(modelField.locator(".ant-select-content")).toHaveText("mock-pro", {
    timeout: 15_000,
  });

  // One round-trip. alpha is a SCRIPTED mock (same file the judge spec
  // uses) — the CHAT MARKER keys its reply.
  await page.locator(".composer textarea").fill("CHAT MARKER: e2e history probe");
  await page.locator(".send-btn").click();
  await expect(page.locator(".chat-msg.user").last()).toContainText("CHAT MARKER");
  await expect(page.locator(".chat-msg.agent").last()).toContainText("answer from alpha chat", {
    timeout: 15_000,
  });

  // The first prompt appears exactly ONCE — the injection chip riding
  // between the optimistic copy and the live user_message echo used to
  // break the tail dedupe and double every first turn (the "你好"
  // report, 2026-09-19).
  await expect(page.locator(".chat-msg.user")).toHaveCount(1);

  // The platform injection is inspectable: a collapsed chip that opens
  // to the full render.
  await expect(page.locator(".chat-injection summary")).toBeVisible();
  await page.locator(".chat-injection summary").click();
  await expect(page.locator(".chat-injection pre")).toBeVisible();

  // History: the conversation is recorded with the first prompt as the
  // title, marked live — in the persistent left rail (2026-09-19: the
  // history drawer became a rail; the mobile toggle is the only other
  // entry point).
  const row = page.locator(".chat-side .row-btn").first();
  await expect(row).toContainText("CHAT MARKER", { timeout: 10_000 });

  // Live entry: click reattaches, the transcript replay restores both
  // turns in the log, and the row is the selected one.
  await row.click();
  await expect(page.locator(".chat-side .row-btn.selected")).toHaveCount(1);
  await expect(page.locator(".chat-msg.user").last()).toContainText("CHAT MARKER", {
    timeout: 8_000,
  });
  await expect(page.locator(".chat-msg.agent").last()).toContainText("answer from alpha chat", {
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
