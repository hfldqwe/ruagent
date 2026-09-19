import { chromium } from "@playwright/test";
const browser = await chromium.launch();
const page = await browser.newPage({ viewport: { width: 1440, height: 900 } });
await page.goto("http://127.0.0.1:8787/#memory", { waitUntil: "networkidle" });
await page.waitForTimeout(900);

// switch to the distill tab
await page.locator(".view-bar .ant-segmented-item").filter({ hasText: /蒸\s*馏|Distill/ }).click();
await page.waitForTimeout(800);

const autoOn = await page.locator(".distill-settings .ant-switch-checked").count();
const langVal = await page.locator(".distill-settings input").first().inputValue();
const agentShown = await page.locator(".distill-settings .ant-select-selection-item").first().innerText();
console.log(`auto switch on: ${autoOn === 1} | agent: ${agentShown} | language: ${langVal}`);

// builtin prompt collapsible
await page.locator(".distill-builtin summary").click();
await page.waitForTimeout(200);
const builtin = await page.locator(".distill-builtin pre").innerText();
console.log("builtin prompt visible:", builtin.startsWith("You are a memory distillation engine"), `(${builtin.length} chars)`);

// edit language → save → toast
await page.locator(".distill-settings input").first().fill("简体中文");
await page.locator(".distill-settings .ant-btn-primary").click();
await page.waitForTimeout(800);
const toasts = await page.locator(".ant-message-notice").allInnerTexts();
console.log("save toast:", JSON.stringify(toasts));
await browser.close();
