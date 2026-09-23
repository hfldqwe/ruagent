#!/usr/bin/env node
// Row 28 probe — "API requests in an 11.5s window, per route".
//
// The audit tool judges row 28 only when --api-window >= 11500 (its default
// 2.5s window is informational). This script answers the question the row's
// verdict alone cannot: is a high count a POLLER (the same path repeating at a
// steady interval — what the row is about) or a LOAD BURST (many distinct paths
// in the first second, then silence — what the row is NOT about)?
//
// It therefore reports, per route:
//   · the raw 11.5s total (the number row 28 compares against <=7);
//   · the same total split into the first 2.5s and the remaining 9s;
//   · every request normalised to a path pattern (UUIDs -> :id) with its count.
//
//   cd panel && node e2e/api-window-probe.mjs            # human table
//   cd panel && node e2e/api-window-probe.mjs --json     # machine-readable
//
// Requires the daemon on 127.0.0.1:8787 serving panel/dist.

import { chromium } from "@playwright/test";

const BASE = process.env.E2E_BASE_URL ?? "http://127.0.0.1:8787";
const ROUTES = (process.env.PROBE_ROUTES ?? "graph,inbox,home,chat,task").split(",");
// --click=<visible text>: click one control (a Segmented tab, usually) before
// the measurement window starts, so sub-modes get measured too.
const CLICK = process.argv.find((a) => a.startsWith("--click="))?.slice(8);
const WINDOW_MS = 11_500;
const BURST_MS = 2_500;

const norm = (u) => {
  try {
    const url = new URL(u);
    return url.pathname
      .replace(/\/[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}/gi, "/:id")
      .replace(/\/\d+/g, "/:n");
  } catch {
    return u;
  }
};

const main = async () => {
  const browser = await chromium.launch();
  const out = [];
  for (const route of ROUTES) {
    const page = await browser.newPage({ viewport: { width: 1440, height: 900 }, locale: "zh-CN" });
    const hits = [];
    page.on("request", (r) => {
      const url = r.url();
      if (!url.includes("/api/")) return;
      hits.push({ t: Date.now(), path: norm(url) });
    });
    await page.goto(`${BASE}/?mode=dark#${route}`, { waitUntil: "load", timeout: 60_000 });
    await page.waitForTimeout(2500);
    if (CLICK) {
      await page.getByText(CLICK, { exact: true }).first().click();
      await page.waitForTimeout(600);
    }
    const t0 = Date.now();
    await page.waitForTimeout(WINDOW_MS);
    const inWindow = hits.filter((h) => h.t - t0 <= WINDOW_MS);
    const burst = inWindow.filter((h) => h.t - t0 <= BURST_MS);
    const rest = inWindow.filter((h) => h.t - t0 > BURST_MS);
    const byPath = {};
    for (const h of inWindow) byPath[h.path] = (byPath[h.path] ?? 0) + 1;
    const burstPaths = {};
    for (const h of burst) burstPaths[h.path] = (burstPaths[h.path] ?? 0) + 1;
    const restPaths = {};
    for (const h of rest) restPaths[h.path] = (restPaths[h.path] ?? 0) + 1;
    out.push({
      route,
      total11500: inWindow.length,
      first2500: burst.length,
      after2500: rest.length,
      distinctPaths: Object.keys(byPath).length,
      byPath,
      burstPaths,
      restPaths,
    });
    await page.close();
  }
  await browser.close();

  if (process.argv.includes("--json")) {
    process.stdout.write(JSON.stringify({ base: BASE, windowMs: WINDOW_MS, burstMs: BURST_MS, routes: out }, null, 2) + "\n");
    return;
  }
  console.log(`row 28 probe — ${BASE}, window ${WINDOW_MS}ms (burst = first ${BURST_MS}ms)`);
  for (const r of out) {
    console.log(
      `\n#${r.route}: ${r.total11500} in window (first ${BURST_MS}ms: ${r.first2500}, after: ${r.after2500}) · distinct paths ${r.distinctPaths}`,
    );
    const rows = Object.entries(r.byPath).sort((a, b) => b[1] - a[1]);
    for (const [p, n] of rows) {
      console.log(`   ${String(n).padStart(3)}x ${p}   [first ${BURST_MS}ms: ${r.burstPaths[p] ?? 0}, after: ${r.restPaths[p] ?? 0}]`);
    }
  }
};

main().catch((e) => {
  console.error(e);
  process.exit(2);
});
