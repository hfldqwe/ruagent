#!/usr/bin/env node
// X4 end-to-end acceptance — the "narrow-screen readout strip" ruling
// (docs/design/views/README.md §3.4 X4, primitives §3.2 B7).
//
// X4 says two things and this script measures both against the SERVED bundle:
//
//   1. eight routes (home / board / memory / sessions / knowledge / graph /
//      runtimes / stats) must pass `grid` to ReadoutStrip, and at 390 the strip
//      must lay out as exactly 2 tracks (2-up, so four gauges are a 2x2);
//   2. >520 parity: the modifier is a <=520 specialization and NOTHING else, so
//      above 520 adding/removing `grid` must not move a single cell. Measured
//      as an A/B inside one page load: measure with the class, remove it,
//      measure again, compare every cell rect.
//
// Why an A/B instead of "two builds": the claim is "the modifier has no rules
// above 520", and the only way to see a no-op is to toggle the one thing that
// is supposed to be a no-op. Comparing two screenshots would also fold in
// unrelated loading differences.
//
// t20 and t21 each missed one width (t20 stopped at 1440 and only used a 4-item
// strip; t21 added 1920 and found home/memory diverged too), so this script
// always walks 520 / 768 / 1024 / 1280 / 1440 / 1920 — the last width is not
// optional.
//
//   cd panel && node e2e/x4-parity.mjs            # human table
//   cd panel && node e2e/x4-parity.mjs --json     # machine-readable
//
// Requires the daemon on 127.0.0.1:8787 serving panel/dist (run `npm run build`
// first — a stale dist measures the previous source).

import { chromium } from "@playwright/test";

const BASE = process.env.E2E_BASE_URL ?? "http://127.0.0.1:8787";
const ROUTES = ["home", "board", "memory", "sessions", "knowledge", "graph", "runtimes", "stats"];
const WIDTHS = [520, 768, 1024, 1280, 1440, 1920];
const JSON_OUT = process.argv.includes("--json");

// Wait for the strip AND for it to stop changing size: the gauges are fed by
// the API, so a strip that exists but is still filling in would report the
// pre-data geometry (the audit tool's domQuietMs idea, applied locally).
async function stripReady(page) {
  await page.waitForSelector(".readout-strip", { state: "attached", timeout: 20_000 });
  await page.waitForFunction(
    () => {
      const el = document.querySelector(".readout-strip");
      if (!el || el.children.length === 0) return false;
      const r = el.getBoundingClientRect();
      const key = `${el.children.length}:${Math.round(r.width)}x${Math.round(r.height)}`;
      const ok = window.__x4key === key;
      window.__x4key = key;
      return ok;
    },
    { timeout: 20_000, polling: 250 },
  );
  // waitForFunction needs the key to repeat: prime it once, then confirm.
  await page.waitForTimeout(300);
}

const rects = (page) =>
  page.evaluate(() => {
    const el = document.querySelector(".readout-strip");
    return [...el.children].map((c, i) => {
      const r = c.getBoundingClientRect();
      return {
        i,
        cls: c.className,
        x: Math.round(r.x * 100) / 100,
        y: Math.round(r.y * 100) / 100,
        w: Math.round(r.width * 100) / 100,
        h: Math.round(r.height * 100) / 100,
      };
    });
  });

const gridInfo = (page) =>
  page.evaluate(() => {
    const el = document.querySelector(".readout-strip");
    const cs = getComputedStyle(el);
    const kids = [...el.children];
    const tracks = cs.gridTemplateColumns === "none" ? 0 : cs.gridTemplateColumns.trim().split(/\s+/).length;
    const rows = new Set(kids.map((k) => Math.round(k.getBoundingClientRect().top))).size;
    const cols = new Set(kids.map((k) => Math.round(k.getBoundingClientRect().left))).size;
    return {
      hasGridClass: el.classList.contains("grid"),
      display: cs.display,
      tracks,
      rows,
      cols,
      items: kids.length,
    };
  });

const main = async () => {
  const browser = await chromium.launch();
  const page = await browser.newPage({ viewport: { width: 390, height: 900 }, locale: "zh-CN" });
  const out = { base: BASE, generatedAt: new Date().toISOString(), routes: [] };
  for (const route of ROUTES) {
    const rec = { route, at390: null, parity: [], note: "" };
    await page.setViewportSize({ width: 390, height: 900 });
    await page.goto(`${BASE}/?mode=dark#${route}`, { waitUntil: "load", timeout: 60_000 });
    try {
      await stripReady(page);
    } catch (e) {
      rec.note = `no .readout-strip with children after 20s (${e.message.split("\n")[0]})`;
      out.routes.push(rec);
      continue;
    }
    rec.at390 = await gridInfo(page);
    for (const width of WIDTHS) {
      await page.setViewportSize({ width, height: 900 });
      await page.waitForTimeout(250);
      const a = await rects(page); // with `grid`
      await page.evaluate(() => document.querySelector(".readout-strip").classList.remove("grid"));
      await page.waitForTimeout(120);
      const b = await rects(page); // without `grid`
      await page.evaluate(() => document.querySelector(".readout-strip").classList.add("grid"));
      await page.waitForTimeout(120);
      const diffs = [];
      for (let i = 0; i < Math.max(a.length, b.length); i++) {
        const x = a[i], y = b[i];
        if (!x || !y) {
          diffs.push({ i, why: "cell count differs", a: x ? `${x.w}x${x.h}` : "missing", b: y ? `${y.w}x${y.h}` : "missing" });
          continue;
        }
        for (const k of ["x", "y", "w", "h"]) {
          if (Math.abs(x[k] - y[k]) > 0.01) diffs.push({ i, prop: k, a: x[k], b: y[k] });
        }
      }
      rec.parity.push({
        width,
        equal: diffs.length === 0,
        items: a.length,
        a: a.map((r) => `${r.w}x${r.h}`).join(","),
        b: b.map((r) => `${r.w}x${r.h}`).join(","),
        diffs: diffs.slice(0, 6),
      });
    }
    out.routes.push(rec);
  }
  await browser.close();

  if (JSON_OUT) {
    process.stdout.write(JSON.stringify(out, null, 2) + "\n");
  } else {
    console.log(`X4 — readout strip (${BASE})`);
    for (const r of out.routes) {
      const g = r.at390;
      console.log(
        `\n#${r.route}: ${g ? `grid class=${g.hasGridClass} display=${g.display} @390 tracks=${g.tracks} rows=${g.rows} cols=${g.cols} items=${g.items}` : `NO STRIP — ${r.note}`}`,
      );
      for (const p of r.parity) {
        console.log(
          `  @${String(p.width).padEnd(4)} A/B ${p.equal ? "EQUAL  " : "DIFFER "} items=${p.items}  A=[${p.a}]  B=[${p.b}]${p.equal ? "" : "  " + JSON.stringify(p.diffs)}`,
        );
      }
    }
    const bad = out.routes.filter((r) => !r.at390 || !r.at390.hasGridClass || r.at390.tracks !== 2);
    console.log(`\n@390 2-track + grid passed: ${out.routes.length - bad.length}/${out.routes.length}`);
  }
};

main().catch((e) => {
  console.error(e);
  process.exit(2);
});
