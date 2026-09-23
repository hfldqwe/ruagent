#!/usr/bin/env node
// Order-perturbation probe (t99 A).
//
// The user reported "I click a session and it jumps to the top, but it took
// many tries to reproduce once". The captain's hypothesis is that the poll
// (t92, every 3-5s) replaces the whole history list, and that while teammates
// keep creating test chats a NEW row lands on top and shifts every existing
// row down one -- which reads as "my click moved it".
//
// This probe turns "rare" into "logged": it polls /api/v1/chats once a second,
// records EVERY change of the row-order sequence with a timestamp and the full
// per-chat state at that moment, and classifies the direct cause. It can also
// drive a click on a session row and snapshot before/after, which is the
// user's original accusation.
//
// Usage:
//   node tools/order-probe.mjs --seconds=480 [--interval=1000] [--click]
//        [--base=http://127.0.0.1:8787] [--out=../docs/screenshots/t99-order]
//
// If nothing changes it prints "NOT OBSERVED" with the observation window --
// "no change seen" and "no problem found" are different claims and this probe
// never conflates them.

import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const args = {};
for (const a of process.argv.slice(2)) {
  const m = a.match(/^--([^=]+)(?:=(.*))?$/);
  if (m) args[m[1]] = m[2] === undefined ? true : m[2];
}
const base = args.base || "http://127.0.0.1:8787";
const seconds = Number(args.seconds || 480);
const intervalMs = Number(args.interval || 1000);
const outDir = args.out || "../docs/screenshots/t99-order";
const driveClick = Boolean(args.click);

mkdirSync(outDir, { recursive: true });

const seq = (chats) => chats.map((c) => c.id);
const state = (chats) =>
  Object.fromEntries(
    chats.map((c) => [
      c.id,
      {
        title: (c.title || "").slice(0, 28),
        updated_at: c.updated_at,
        active: c.active,
        generating: c.generating,
        message_count: c.message_count,
      },
    ]),
  );

// Classify the DIRECT cause from the two snapshots. Deliberately mechanical:
// a guess that cannot be re-derived from the logged numbers is not a finding.
function classify(before, after, beforeState, afterState) {
  const b = new Set(before);
  const a = new Set(after);
  const added = after.filter((id) => !b.has(id));
  const removed = before.filter((id) => !a.has(id));
  const sameSet = added.length === 0 && removed.length === 0;
  const bumped = [];
  for (const id of after) {
    if (!b.has(id)) continue;
    const bs = beforeState[id];
    const as = afterState[id];
    if (bs && as && bs.updated_at !== as.updated_at) bumped.push(id);
  }
  const flipped = [];
  for (const id of after) {
    if (!b.has(id)) continue;
    const bs = beforeState[id];
    const as = afterState[id];
    if (!bs || !as) continue;
    if (bs.active !== as.active || bs.generating !== as.generating) flipped.push(id);
  }
  const counts = [];
  for (const id of after) {
    if (!b.has(id)) continue;
    const bs = beforeState[id];
    const as = afterState[id];
    if (bs && as && bs.message_count !== as.message_count) counts.push(id);
  }
  const moved = [];
  if (sameSet) {
    for (let i = 0; i < after.length; i++) if (after[i] !== before[i]) moved.push(after[i]);
  }
  let cause;
  if (added.length && !removed.length) cause = "NEW ROW(S) APPEARED at index " + after.indexOf(added[0]);
  else if (removed.length && !added.length) cause = "ROW(S) REMOVED";
  else if (added.length || removed.length) cause = "ROWS ADDED AND REMOVED";
  else if (bumped.length) cause = "updated_at BUMPED (no membership change)";
  else if (flipped.length) cause = "active/generating FLIPPED (no membership change)";
  else if (moved.length) cause = "REORDERED WITH NO STATE CHANGE";
  else cause = "UNKNOWN";
  return { added, removed, moved, bumped, flipped, counts, cause };
}

const changes = [];
const ticks = [];
let prev = null;
let prevState = null;
const t0 = Date.now();

async function fetchChats() {
  const res = await fetch(base + "/api/v1/chats");
  if (!res.ok) throw new Error("HTTP " + res.status);
  const body = await res.json();
  return body.chats || [];
}

async function tick() {
  const chats = await fetchChats();
  const s = seq(chats);
  const st = state(chats);
  const at = Date.now();
  ticks.push({ at, n: s.length });
  if (prev && (s.length !== prev.length || s.some((id, i) => id !== prev[i]))) {
    const info = classify(prev, s, prevState, st);
    changes.push({
      at,
      tRelMs: at - t0,
      before: prev,
      after: s,
      beforeState: prevState,
      afterState: st,
      ...info,
    });
    console.log(
      "[order] t+" + ((at - t0) / 1000).toFixed(1) + "s  " + info.cause +
        "  +" + info.added.length + " -" + info.removed.length +
        " moved=" + info.moved.length + " bumped=" + info.bumped.length
    );
  }
  prev = s;
  prevState = st;
}

// ---- click driving (the user's original accusation) -------------------------
async function clickTest() {
  const { chromium } = await import("@playwright/test");
  const b = await chromium.launch();
  const p = await b.newPage({ viewport: { width: 1440, height: 900 }, deviceScaleFactor: 1 });
  await p.goto(base + "/?mode=dark#chat", { waitUntil: "load" });
  await p.waitForTimeout(2500);
  const domOrder = () =>
    p.evaluate(() =>
      [...document.querySelectorAll(".chat-session-row")].map((r) => {
        const t = r.querySelector(".title");
        return (t ? t.textContent : "").trim().slice(0, 24);
      })
    );
  const apiOrder = async () => seq(await fetchChats());
  const beforeDom = await domOrder();
  const beforeApi = await apiOrder();
  const clicked = await p.evaluate(() => {
    const rows = [...document.querySelectorAll(".chat-session-row")];
    if (rows.length < 3) return null;
    const r = rows[2];
    const t = r.querySelector(".title");
    const label = (t ? t.textContent : "").trim().slice(0, 24);
    r.click();
    return label;
  });
  await p.waitForTimeout(4000);
  const afterDom = await domOrder();
  const afterApi = await apiOrder();
  await b.close();
  return {
    clicked,
    beforeDom,
    afterDom,
    beforeApi,
    afterApi,
    domChanged: beforeDom.join("|") !== afterDom.join("|"),
    apiChanged: beforeApi.join("|") !== afterApi.join("|"),
  };
}

// ---- run --------------------------------------------------------------------
console.log("[order] probing " + base + "/api/v1/chats every " + intervalMs + "ms for " + seconds + "s");
const deadline = Date.now() + seconds * 1000;
while (Date.now() < deadline) {
  try {
    await tick();
  } catch (e) {
    console.log("[order] tick failed: " + e.message);
  }
  await new Promise((r) => setTimeout(r, intervalMs));
}

const click = driveClick ? await clickTest() : null;
const observedMs = Date.now() - t0;
const summary = {
  base,
  observedMs,
  observedSeconds: Math.round(observedMs / 1000),
  intervalMs,
  ticks: ticks.length,
  rowsAtEnd: prev ? prev.length : 0,
  changes: changes.length,
  changeLog: changes,
  click,
  verdict:
    changes.length === 0
      ? "NOT OBSERVED: no row-order change in " + Math.round(observedMs / 1000) + "s of observation"
      : "OBSERVED " + changes.length + " order change(s) in " + Math.round(observedMs / 1000) + "s",
};
writeFileSync(join(outDir, "order-probe.json"), JSON.stringify(summary, null, 2));

const lines = [];
lines.push("# Order-perturbation probe (t99 A)");
lines.push("");
lines.push("- base: " + base);
lines.push("- observation window: **" + summary.observedSeconds + "s** (" + ticks.length + " ticks @" + intervalMs + "ms)");
lines.push("- rows at end: " + summary.rowsAtEnd);
lines.push("- order changes: **" + changes.length + "**");
lines.push("");
lines.push("**Verdict: " + summary.verdict + "**");
lines.push("");
if (changes.length) {
  lines.push("| # | t+ (s) | cause | added | removed | moved | updated_at bumped |");
  lines.push("|---|---|---|---|---|---|---|");
  changes.forEach((c, i) => {
    lines.push(
      "| " + (i + 1) + " | " + (c.tRelMs / 1000).toFixed(1) + " | " + c.cause + " | " +
        c.added.length + " | " + c.removed.length + " | " + c.moved.length + " | " + c.bumped.length + " |"
    );
  });
} else {
  lines.push("No change was observed. That is NOT the same as \"no problem exists\": it bounds the");
  lines.push("window, not the phenomenon. Re-run for longer, or run it while the suspected");
  lines.push("trigger (a teammate creating chats) is actually happening.");
}
if (click) {
  lines.push("");
  lines.push("## Click test (the user's original accusation)");
  lines.push("- clicked row: " + JSON.stringify(click.clicked));
  lines.push("- DOM row order changed by the click: **" + click.domChanged + "**");
  lines.push("- API row order changed by the click: **" + click.apiChanged + "**");
  lines.push("- before (DOM): " + JSON.stringify(click.beforeDom));
  lines.push("- after  (DOM): " + JSON.stringify(click.afterDom));
}
writeFileSync(join(outDir, "order-probe.md"), lines.join("\n"));
console.log("[order] " + summary.verdict);
console.log("[order] wrote " + outDir + "/order-probe.{json,md}");
