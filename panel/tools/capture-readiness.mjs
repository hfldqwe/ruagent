#!/usr/bin/env node
// t150: WHY does a capture sometimes photograph an unrendered page?
//
// Three people hit the same shape independently (t140: h1=0 / sider width null;
// t140: splitter read 1px; t147: an empty rail). Each time the judge read an
// EMPTY SET, which either mis-reds or lands on not_measured -- and not_measured
// costs the judge its signal.
//
// This probe replicates the audit's capture step EXACTLY (goto load + reload
// load + a fixed 2500ms sleep) and reports, per capture, whether the page was
// actually rendered at that moment, when it became ready, and what the console
// and pageerror streams said. It is deliberately lean: it judges nothing, so it
// can run many rounds and produce a FREQUENCY rather than an anecdote.
//
// Usage: node tools/capture-readiness.mjs [--rounds=3] [--base-url=URL]

import { chromium } from "playwright";

const args = process.argv.slice(2);
const opt = { rounds: 3, baseUrl: process.env.E2E_BASE_URL ?? "http://127.0.0.1:8787" };
for (let i = 0; i < args.length; i++) {
  const a = args[i];
  const [k, v] = a.includes("=") ? a.replace(/^--/, "").split(/=(.*)/s) : [a.replace(/^--/, ""), args[++i]];
  if (k === "rounds") opt.rounds = Number(v);
  if (k === "base-url") opt.baseUrl = String(v);
}

const ROUTES = [
  { id: "home", hash: "#home" },
  { id: "chat", hash: "#chat" },
  { id: "sessions", hash: "#sessions" },
  { id: "board", hash: "#board" },
  { id: "memory", hash: "#memory" },
  { id: "knowledge", hash: "#knowledge" },
  { id: "graph", hash: "#graph" },
  { id: "agents", hash: "#agents" },
  { id: "runtimes", hash: "#runtimes" },
  { id: "stats", hash: "#stats" },
  { id: "settings", hash: "#settings" },
  { id: "inbox", hash: "#inbox" },
];
const MODES = ["dark", "light"];
// The audit's own fixed settle, replicated so the reading is about the SAME
// moment the judges see.
const SETTLE_MS = 2500;
const POLL_MS = 150;
const MAX_WAIT_MS = 20_000;

async function main() {
  const browser = await chromium.launch();
  const all = [];
  for (let round = 1; round <= opt.rounds; round++) {
    const roundRows = [];
    for (const route of ROUTES) {
      for (const mode of MODES) {
        const page = await browser.newPage({ viewport: { width: 1440, height: 900 }, deviceScaleFactor: 1 });
        const consoleErrors = [];
        const pageErrors = [];
        page.on("console", (m) => { if (m.type() === "error") consoleErrors.push(m.text().slice(0, 120)); });
        page.on("pageerror", (e) => pageErrors.push(String(e.message || e).slice(0, 120)));
        const url = opt.baseUrl + "/?mode=" + mode + route.hash;
        const t0 = Date.now();
        let gotoMs = null;
        try {
          await page.goto(url, { waitUntil: "load", timeout: 60_000 });
          await page.reload({ waitUntil: "load", timeout: 60_000 });
          gotoMs = Date.now() - t0;
          // Poll for the ready signal: the app shell AND a rendered heading.
          let readyAt = null;
          let atSettle = null;
          const started = Date.now();
          for (;;) {
            const st = await page.evaluate(() => ({
              sider: !!document.querySelector("aside.app-sider, .app-sider"),
              h1: document.querySelectorAll("h1").length,
              nodes: document.querySelectorAll("*").length,
            }));
            const ready = st.sider && st.h1 >= 1;
            const elapsed = Date.now() - started;
            if (ready && readyAt == null) readyAt = elapsed;
            if (atSettle == null && elapsed >= SETTLE_MS) atSettle = { ...st, ready };
            if (atSettle && readyAt != null) break;
            if (elapsed > MAX_WAIT_MS) { atSettle = atSettle || { ...st, ready }; break; }
            await page.waitForTimeout(POLL_MS);
          }
          roundRows.push({
            round, route: route.id, mode,
            gotoMs,
            readyAtMs: readyAt,
            readyAtSettle: !!(atSettle && atSettle.ready),
            h1AtSettle: atSettle ? atSettle.h1 : null,
            siderAtSettle: atSettle ? atSettle.sider : null,
            nodesAtSettle: atSettle ? atSettle.nodes : null,
            consoleErrors: consoleErrors.length,
            pageErrors: pageErrors.length,
            firstConsole: consoleErrors[0] || null,
            firstPageError: pageErrors[0] || null,
          });
        } catch (e) {
          roundRows.push({ round, route: route.id, mode, gotoMs, error: String(e.message || e).slice(0, 140), consoleErrors: consoleErrors.length, pageErrors: pageErrors.length });
        }
        await page.close();
      }
    }
    all.push(...roundRows);
    const blanks = roundRows.filter((r) => !r.readyAtSettle);
    console.log("round " + round + ": " + roundRows.length + " captures · blank-at-settle " + blanks.length +
      (blanks.length ? " -> " + blanks.map((b) => b.route + "/" + b.mode + (b.error ? "(err)" : " h1=" + b.h1AtSettle)).join(", ") : ""));
  }
  const blanks = all.filter((r) => !r.readyAtSettle);
  console.log("\n=== TOTAL " + all.length + " captures over " + opt.rounds + " rounds ===");
  console.log("blank-at-settle: " + blanks.length + " (" + ((blanks.length / all.length) * 100).toFixed(1) + "%)");
  const errs = all.filter((r) => r.consoleErrors > 0 || r.pageErrors > 0);
  console.log("captures with console/page errors: " + errs.length);
  for (const e of errs.slice(0, 5)) console.log("  " + e.route + "/" + e.mode + " console=" + e.consoleErrors + " page=" + e.pageErrors + " :: " + (e.firstPageError || e.firstConsole || ""));
  const slow = all.filter((r) => r.readyAtMs != null).map((r) => r.readyAtMs).sort((a, b) => a - b);
  if (slow.length) console.log("readyAtMs: min " + slow[0] + " · p50 " + slow[Math.floor(slow.length / 2)] + " · max " + slow[slow.length - 1] + " · over-settle " + slow.filter((x) => x > SETTLE_MS).length);
  const g = all.filter((r) => r.gotoMs != null).map((r) => r.gotoMs).sort((a, b) => a - b);
  if (g.length) console.log("gotoMs: min " + g[0] + " · p50 " + g[Math.floor(g.length / 2)] + " · max " + g[g.length - 1]);
  const byRoute = {};
  for (const b of blanks) { const k = b.route + "/" + b.mode; byRoute[k] = (byRoute[k] || 0) + 1; }
  console.log("blank by route/mode: " + (Object.keys(byRoute).length ? JSON.stringify(byRoute) : "{}"));
  console.log("blank by round: " + JSON.stringify(blanks.reduce((a, b) => ((a[b.round] = (a[b.round] || 0) + 1), a), {})));
  await browser.close();
}
main();
