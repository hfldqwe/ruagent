import { chromium } from "@playwright/test";
const browser = await chromium.launch();
const page = await browser.newPage({ viewport: { width: 1440, height: 900 } });

// settings: graph switch renders and toggles
await page.goto("http://127.0.0.1:8787/#settings", { waitUntil: "networkidle" });
await page.waitForTimeout(800);
const graphHint = await page.locator(".distill-settings").filter({ hasText: /知识图谱|Knowledge graph/ }).count();
console.log("1. settings graph card:", graphHint === 1);

// chat with a project dir — the session must run there and group in the rail
await page.goto("http://127.0.0.1:8787/#chat", { waitUntil: "networkidle" });
await page.waitForTimeout(900);
// project select in the rail
await page.locator(".chat-side .chat-field .ant-select").first().click();
await page.keyboard.type("C:/Users/19410/Documents/ai/ruagent");
await page.keyboard.press("Enter");
await page.keyboard.press("Escape");
await page.waitForTimeout(300);
// agent = review (dsh role)
const agentField = page.locator(".chat-bar .chat-field").filter({ hasText: /智能体|Agent/ }).first();
await agentField.locator(".ant-select-input").click();
await page.locator(".ant-select-item-option").filter({ hasText: /^review$/ }).first().click();
await page.waitForTimeout(700);
await page.locator(".composer textarea").fill("用一句话说明当前工作目录里最顶层目录名有哪些（只要目录名）");
await page.locator(".composer .send-btn").first().click();
await page.waitForSelector(".composer .send-btn:not(.stop-btn)", { timeout: 120000 });
const reply = await page.locator(".chat-msg.agent").last().innerText();
console.log("2. project chat reply mentions repo dirs:", /crates|panel|docs/.test(reply), "|", JSON.stringify(reply.slice(0, 80)));

// chats list carries cwd; rail groups by it
const info = await page.evaluate(() => fetch("/api/v1/chats").then((r) => r.json()));
const mine = info.chats.find((c) => c.active);
console.log("3. chats API cwd:", mine?.cwd);
await page.waitForTimeout(600);
await page.locator(".chat-side .chat-field .ant-select").first().click(); await page.keyboard.press("Escape");
const groups = await page.locator(".chat-group-head").allInnerTexts();
console.log("4. rail groups:", JSON.stringify(groups.map((g) => g.replace(/\n/g, " "))));

// cleanup
await page.evaluate(async () => {
  const d = await fetch("/api/v1/chats").then((r) => r.json());
  for (const c of d.chats.filter((x) => x.active)) await fetch(`/api/v1/chat/${x.id}`, { method: "DELETE" });
});
await browser.close();
