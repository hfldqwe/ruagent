#!/usr/bin/env node
// Auto-focus probe — what ring does the element the VIEW focuses for you wear?
//
// The audit's row-15/16 focus walk presses Tab from the page's initial state,
// so the element a view auto-focuses on mount is the one stop it never visits:
// the first Tab moves OFF it. That is exactly where F-7 lives — an antd Input
// that shows antd's own `--ant-color-primary` border + box-shadow instead of
// the panel's signal ring, a position MASTER §3.2's whitelist W1–W6 does not
// list (the whitelist covers "手写 :focus-visible 环", not antd's own input
// focus treatment).
//
//   cd panel && node e2e/focus-owner-probe.mjs           # human table
//   cd panel && node e2e/focus-owner-probe.mjs --json    # machine-readable
//
// Reports, per route: whether document.activeElement is an input/textarea, and
// the computed outline / border-color / box-shadow, plus the values of
// --signal and --ant-color-primary so the reader can attribute the colour
// without trusting a hardcoded hex.

import { chromium } from "@playwright/test";

const BASE = process.env.E2E_BASE_URL ?? "http://127.0.0.1:8787";
const ROUTES = process.env.PROBE_ROUTES?.split(",") ?? ["knowledge", "memory", "board", "graph", "chat", "settings", "task", "home"];

const main = async () => {
  const browser = await chromium.launch();
  const page = await browser.newPage({ viewport: { width: 1440, height: 900 }, locale: "zh-CN" });
  const out = [];
  for (const route of ROUTES) {
    await page.goto(`${BASE}/?mode=dark#${route}`, { waitUntil: "load", timeout: 60_000 });
    await page.waitForTimeout(2500);
    const rec = await page.evaluate(() => {
      const el = document.activeElement;
      const cs = el ? getComputedStyle(el) : null;
      const root = getComputedStyle(document.documentElement);
      const box = el ? el.getBoundingClientRect() : null;
      const cls = el ? (typeof el.className === "string" ? el.className : "") : "";
      return {
        tag: el?.tagName,
        cls: cls.slice(0, 80),
        isField: !!el && /^(INPUT|TEXTAREA|SELECT)$/.test(el.tagName),
        box: box ? `${Math.round(box.width)}x${Math.round(box.height)}` : null,
        outline: cs ? `${cs.outlineStyle} ${cs.outlineWidth} ${cs.outlineColor}` : null,
        borderColor: cs?.borderColor ?? null,
        borderWidth: cs?.borderWidth ?? null,
        boxShadow: cs?.boxShadow ?? null,
        signal: root.getPropertyValue("--signal").trim(),
        primary: root.getPropertyValue("--ant-color-primary").trim(),
      };
    });
    out.push({ route, ...rec });
    await page.keyboard.press("Tab"); // move off, so the next route starts clean
    await page.waitForTimeout(100);
  }
  await browser.close();
  if (process.argv.includes("--json")) {
    process.stdout.write(JSON.stringify({ base: BASE, routes: out }, null, 2) + "\n");
    return;
  }
  console.log(`auto-focus probe — ${BASE} (dark)`);
  for (const r of out) {
    console.log(`\n#${r.route}: activeElement=${r.tag}.${r.cls} field=${r.isField} box=${r.box}`);
    console.log(`   outline=${r.outline} border=${r.borderWidth} ${r.borderColor}`);
    console.log(`   boxShadow=${r.boxShadow}`);
    console.log(`   --signal=${r.signal} --ant-color-primary=${r.primary}`);
  }
};

main().catch((e) => {
  console.error(e);
  process.exit(2);
});
