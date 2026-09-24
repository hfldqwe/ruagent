#!/usr/bin/env node
// t150 REVERSE EVIDENCE: point the SAME readiness wait at a page that will never
// render the app. The wait must give up and NAME the reason -- it must not
// silently pass, and it must not turn "blank" into a false green.
//
// Without this, a readiness gate degrades into "anything that renders eventually
// is fine, and anything that never renders is also fine" -- the empty-set green
// this team keeps guarding against.
import { chromium } from "playwright";

const READY_TIMEOUT_MS = 5000;
async function waitReady(page) {
  const t0 = Date.now();
  for (;;) {
    const st = await page.evaluate(() => ({
      sider: !!document.querySelector("aside.app-sider, .app-sider"),
      h1: document.querySelectorAll("h1").length,
    }));
    if (st.sider && st.h1 >= 1) return { ready: true, ms: Date.now() - t0, st };
    if (Date.now() - t0 > READY_TIMEOUT_MS) return { ready: false, ms: Date.now() - t0, st };
    await page.waitForTimeout(100);
  }
}
const browser = await chromium.launch();
const cases = [
  { name: "real app (#sessions)", url: "http://127.0.0.1:8787/?mode=dark#sessions" },
  { name: "blank page (no app)", url: "http://127.0.0.1:8787/definitely-not-a-page-t150.html" },
  { name: "empty document", url: "data:text/html,<html><body>nothing here</body></html>" },
];
for (const c of cases) {
  const page = await browser.newPage({ viewport: { width: 1440, height: 900 } });
  try {
    await page.goto(c.url, { waitUntil: "load", timeout: 20_000 });
    const r = await waitReady(page);
    console.log((r.ready ? "READY      " : "NOT_READY  ") + c.name +
      "  ms=" + r.ms + "  sider=" + r.st.sider + " h1=" + r.st.h1 +
      (r.ready ? "" : "  => judges must report not_measured: capture never reached the ready signal (no .app-sider or no h1)"));
  } catch (e) {
    console.log("ERROR      " + c.name + "  " + String(e.message).slice(0, 80));
  }
  await page.close();
}
await browser.close();
