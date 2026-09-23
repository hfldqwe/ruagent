#!/usr/bin/env node
// Interaction smoothness baseline (t99 B).
//
// "The UI is not smooth, make every component interaction silky" is not
// measurable as stated. This probe measures PHYSICAL quantities instead --
// layout shift, scroll position, focus, and DOM node identity -- on the key
// interactions, so a criterion can later be written against numbers rather
// than against a feeling.
//
// What it records per interaction:
//   * layoutShift  -- sum of PerformanceObserver 'layout-shift' values with no
//                     recent input (the CLS quantity, scoped to the window);
//   * scrollTop    -- the scroll container's scrollTop before/after;
//   * focus        -- document.activeElement before/after (tag + class);
//   * nodeIdentity -- elements marked before the interaction that are STILL THE
//                     SAME NODE afterwards. A re-render that replaces nodes
//                     destroys identity even when the pixels look identical,
//                     which is what makes a list "feel" jumpy.
//
// Usage: node tools/interaction-probe.mjs [--out=../docs/screenshots/t99-interaction]
//        [--base=http://127.0.0.1:8787]

import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const args = {};
for (const a of process.argv.slice(2)) {
  const m = a.match(/^--([^=]+)(?:=(.*))?$/);
  if (m) args[m[1]] = m[2] === undefined ? true : m[2];
}
const base = args.base || "http://127.0.0.1:8787";
const outDir = args.out || "../docs/screenshots/t99-interaction";
mkdirSync(outDir, { recursive: true });

const { chromium } = await import("@playwright/test");
const browser = await chromium.launch();
const page = await browser.newPage({ viewport: { width: 1440, height: 900 }, deviceScaleFactor: 1 });

// The observer is installed per document; every navigation re-installs it via
// addInitScript, so the counter survives route changes.
await page.addInitScript(() => {
  window.__shift = { sum: 0, n: 0, max: 0, sources: [], sumAll: 0, nAll: 0 };
  try {
    new PerformanceObserver((list) => {
      for (const e of list.getEntries()) {
        // Two counters on purpose. CLS discards shifts within 500ms of input,
        // but a shift the user OWN click causes is precisely what not-silky
        // means -- the input-filtered counter alone would be blind to it.
        window.__shift.sumAll += e.value;
        window.__shift.nAll += 1;
        if (e.hadRecentInput) continue;
        window.__shift.sum += e.value;
        window.__shift.n += 1;
        window.__shift.max = Math.max(window.__shift.max, e.value);
        for (const s of e.sources || []) {
          const node = s.node;
          if (!node) continue;
          const cls = node.className && typeof node.className === "string" ? node.className.split(" ").slice(0, 3).join(".") : "";
          window.__shift.sources.push(node.tagName.toLowerCase() + (cls ? "." + cls : ""));
        }
      }
    }).observe({ type: "layout-shift", buffered: true });
  } catch (e) {}
});

const snapshot = () =>
  page.evaluate(() => {
    const scroller =
      document.querySelector("main.ant-layout-content.content") ||
      document.querySelector(".ant-layout-content") ||
      document.documentElement;
    const ae = document.activeElement;
    return {
      shift: JSON.parse(JSON.stringify(window.__shift || { sum: 0, n: 0, max: 0, sources: [], sumAll: 0, nAll: 0 })),
      scrollTop: scroller ? scroller.scrollTop : null,
      listScrollTop: (function () {
        const l = document.querySelector(".chat-session-list, .chat-sessions, .app-sider .ant-layout-sider-children");
        return l ? l.scrollTop : null;
      })(),
      active: ae ? ae.tagName.toLowerCase() + (ae.className && typeof ae.className === "string" ? "." + ae.className.split(" ").slice(0, 2).join(".") : "") : null,
      nodes: document.querySelectorAll("body *").length,
    };
  });

// Mark a set of elements so identity loss is detectable after the interaction.
const mark = (selector) =>
  page.evaluate((sel) => {
    const els = [...document.querySelectorAll(sel)];
    els.forEach((el, i) => {
      el.__pid = "p" + i;
    });
    return els.length;
  }, selector);

const identity = (selector) =>
  page.evaluate((sel) => {
    const els = [...document.querySelectorAll(sel)];
    let same = 0;
    for (const el of els) if (el.__pid) same += 1;
    return { total: els.length, same };
  }, selector);

async function run(name, selector, action) {
  const marked = selector ? await mark(selector) : 0;
  const before = await snapshot();
  await page.evaluate(() => {
    window.__shift = { sum: 0, n: 0, max: 0, sources: [], sumAll: 0, nAll: 0 };
  });
  const ok = await action();
  // MUST outlast the 3-5s history poll: a shorter window misses the
  // re-render that poll causes and reports a false zero.
  await page.waitForTimeout(6500);
  const after = await snapshot();
  const id = selector ? await identity(selector) : { total: 0, same: 0 };
  const rec = {
    name,
    acted: ok,
    marked,
    shift: after.shift.sum,
    shiftAll: after.shift.sumAll,
    shiftAllCount: after.shift.nAll,
    shiftCount: after.shift.n,
    shiftSources: [...new Set(after.shift.sources)].slice(0, 5),
    scrollTopBefore: before.scrollTop,
    scrollTopAfter: after.scrollTop,
    scrollTopChanged: before.scrollTop !== after.scrollTop,
    activeBefore: before.active,
    activeAfter: after.active,
    activeChanged: before.active !== after.active,
    identityTotal: id.total,
    identityKept: id.same,
    identityLost: id.total - id.same,
    nodesBefore: before.nodes,
    nodesAfter: after.nodes,
  };
  console.log(
    "[" + name + "] acted=" + ok + " shift=" + rec.shift.toFixed(4) + " (" + rec.shiftCount + ")" +
      " scrollTop " + rec.scrollTopBefore + "->" + rec.scrollTopAfter +
      " focusChanged=" + rec.activeChanged +
      " identity " + rec.identityKept + "/" + rec.identityTotal + " kept"
  );
  return rec;
}

const results = [];

// 1. Route switch: home -> chat (a hash change; the SPA re-mounts a view).
await page.goto(base + "/?mode=dark#home", { waitUntil: "load" });
await page.waitForTimeout(2500);
results.push(
  await run("route:home->chat", ".app-sider", async () => {
    await page.evaluate(() => {
      location.hash = "#chat";
    });
    return true;
  })
);

// 2. Open a session row (the interaction the user accused).
await page.waitForTimeout(1500);
results.push(
  await run("session:open-row", ".chat-session-row", async () => {
    return page.evaluate(() => {
      const rows = [...document.querySelectorAll(".chat-session-row")];
      if (rows.length < 2) return false;
      rows[1].click();
      return true;
    });
  })
);

// 3. Expand / collapse a session group, when the sidebar has one.
results.push(
  await run("sidebar:group-toggle", ".chat-group", async () => {
    return page.evaluate(() => {
      const heads = [...document.querySelectorAll(".chat-group-head, .chat-group-more, .chat-group")];
      if (!heads.length) return false;
      heads[0].click();
      return true;
    });
  })
);

// 4. Open a dropdown (antd Select), the most common "not silky" complaint.
results.push(
  await run("dropdown:open", ".ant-select", async () => {
    return page.evaluate(() => {
      const s = document.querySelector(".ant-select");
      if (!s) return false;
      s.click();
      return true;
    });
  })
);

// 5. Theme switch in-app, if the shell exposes one (the ?mode= reload would
//    not be an interaction measurement).
results.push(
  await run("theme:toggle", ".app-sider", async () => {
    return page.evaluate(() => {
      const btns = [...document.querySelectorAll("button, .icon-btn")];
      const t = btns.find((b) => /theme|dark|light|主题|暗|亮/i.test((b.getAttribute("aria-label") || "") + (b.title || "") + (b.textContent || "")));
      if (!t) return false;
      t.click();
      return true;
    });
  })
);

await browser.close();

const summary = { base, at: new Date().toISOString(), interactions: results };
writeFileSync(join(outDir, "interaction-probe.json"), JSON.stringify(summary, null, 2));
const lines = ["# Interaction smoothness baseline (t99 B)", "", "| interaction | acted | layout shift | shift entries | scrollTop changed | focus changed | DOM identity kept | nodes |", "|---|---|---|---|---|---|---|---|"];
for (const r of results) {
  lines.push(
    "| " + r.name + " | " + r.acted + " | " + r.shift.toFixed(4) + " | " + r.shiftCount + " | " +
      r.scrollTopChanged + " | " + r.activeChanged + " | " + r.identityKept + "/" + r.identityTotal +
      (r.identityLost ? " (**lost " + r.identityLost + "**)" : "") + " | " + r.nodesBefore + "->" + r.nodesAfter + " |"
  );
}
lines.push("");
lines.push("Shift sources (first few, per interaction):");
for (const r of results) if (r.shiftSources.length) lines.push("- " + r.name + ": " + r.shiftSources.join(", "));
writeFileSync(join(outDir, "interaction-probe.md"), lines.join("\n"));
console.log("[interaction] wrote " + outDir + "/interaction-probe.{json,md}");
