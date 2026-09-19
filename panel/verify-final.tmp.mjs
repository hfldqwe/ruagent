import { chromium } from "@playwright/test";
const browser = await chromium.launch();
const page = await browser.newPage({ viewport: { width: 1440, height: 900 } });

// A) agents page: full prompt now (381 chars for impl)
await page.goto("http://127.0.0.1:8787/#agents", { waitUntil: "networkidle" });
await page.waitForTimeout(900);
const implCard = page.locator(".agent-grid .ant-card").filter({ hasText: "impl" }).first();
await implCard.locator(".prompt-view summary").click();
await page.waitForTimeout(200);
console.log("A. impl prompt chars:", (await implCard.locator(".prompt-view pre").innerText()).length, "(want 381)");

// B) chat: agent picker groups + project field
await page.goto("http://127.0.0.1:8787/#chat", { waitUntil: "networkidle" });
await page.waitForTimeout(900);
const agentField = page.locator(".chat-bar .chat-field").filter({ hasText: /智能体|Agent/ }).first();
await agentField.locator(".ant-select-input").click();
await page.waitForTimeout(300);
const hasGroup = await page.locator(".ant-select-item-group").filter({ hasText: /运行时|Runtimes/ }).count();
await page.keyboard.press("Escape");
console.log("B. runtime group in picker:", hasGroup >= 1);

// C) session switching: closed conversation → resume → continue
await agentField.locator(".ant-select-input").click();
await page.locator(".ant-select-item-option").filter({ hasText: /^review$/ }).first().click();
await page.waitForTimeout(800);
await page.locator(".composer textarea").fill("记住这个暗号：蓝鲸潜艇。只回复'收到'");
await page.locator(".composer .send-btn").first().click();
await page.waitForSelector(".composer .send-btn:not(.stop-btn)", { timeout: 60000 });
console.log("C1. review replied:", JSON.stringify((await page.locator(".chat-msg.agent").last().innerText()).slice(0, 20)));
// close it via API → then click its rail row → expect RESUME (composer usable), not read-only
const row = page.locator(".chat-side .row-btn").filter({ hasText: "蓝鲸" }).first();
const rowId = await row.getAttribute("title").then((t) => t).catch(() => null);
const chatInfo = await page.evaluate(async () => {
  const d = await fetch("/api/v1/chats").then((r) => r.json());
  const live = d.chats.find((c) => c.active);
  if (live) await fetch(`/api/v1/chat/${live.id}`, { method: "DELETE" });
  return live ? live.id : null;
});
await page.waitForTimeout(600);
await page.reload({ waitUntil: "networkidle" });
await page.waitForTimeout(1000);
const agentField2 = page.locator(".chat-bar .chat-field").filter({ hasText: /智能体|Agent/ }).first();
await agentField2.locator(".ant-select-input").click();
await page.locator(".ant-select-item-option").filter({ hasText: /^review$/ }).first().click();
await page.waitForTimeout(1200);
const row2 = page.locator(".chat-side .row-btn").filter({ hasText: "蓝鲸" }).first();
await row2.click();
await page.waitForTimeout(2000);
const composer = await page.locator(".composer textarea").count();
const readonlyBanner = await page.locator(".tag.warn").filter({ hasText: /只读|read-only/ }).count();
const replayMsgs = await page.locator(".chat-msg.user").allInnerTexts();
console.log(`C2. after click: composer=${composer === 1} readonly=${readonlyBanner} replayUserMsgs=${JSON.stringify(replayMsgs.map((m) => m.slice(0, 14)))}`);
// continue the conversation — the agent must remember the code phrase via handoff
await page.locator(".composer textarea").fill("我刚才说的暗号是什么？只回暗号本身");
await page.locator(".composer .send-btn").first().click();
await page.waitForSelector(".composer .send-btn:not(.stop-btn)", { timeout: 90000 });
const reply = await page.locator(".chat-msg.agent").last().innerText();
console.log("C3. resume reply:", JSON.stringify(reply.slice(0, 40)), "| remembers 暗号:", reply.includes("蓝鲸"));
// C4: same history thread (one row, not fragments)
const rows = await page.locator(".chat-side .row-btn").filter({ hasText: "蓝鲸" }).count();
console.log("C4. history rows for this thread:", rows, "(1 = same thread ✓)");

// cleanup
await page.evaluate(async () => {
  const d = await fetch("/api/v1/chats").then((r) => r.json());
  for (const c of d.chats.filter((x) => x.active)) await fetch(`/api/v1/chat/${x.id}`, { method: "DELETE" });
});
await browser.close();
