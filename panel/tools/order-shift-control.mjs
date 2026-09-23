#!/usr/bin/env node
// Control run for the row-anchor criterion (t99 B, criterion 2).
//
// The criterion: when the history poll changes MEMBERSHIP, a row the user is
// looking at must not move on screen. Today the list is replaced and new rows
// insert at index 0, so every existing row shifts down by one row height.
//
// This forces the perturbation deterministically -- it CREATES a chat through
// the API, then waits for the 3-5s poll to pick it up -- instead of hoping a
// teammate happens to create one during the run.
//
// Usage: node tools/order-shift-control.mjs [--out=../docs/screenshots/t99-control]

import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const args = {};
for (const a of process.argv.slice(2)) {
  const m = a.match(/^--([^=]+)(?:=(.*))?$/);
  if (m) args[m[1]] = m[2] === undefined ? true : m[2];
}
const base = args.base || "http://127.0.0.1:8787";
const outDir = args.out || "../docs/screenshots/t99-control";
mkdirSync(outDir, { recursive: true });

const { chromium } = await import("@playwright/test");
const browser = await chromium.launch();
const page = await browser.newPage({ viewport: { width: 1440, height: 900 }, deviceScaleFactor: 1 });
await page.goto(base + "/?mode=dark#chat", { waitUntil: "load" });
await page.waitForTimeout(3000);

// Track the rows by their TITLE (stable across a re-render; a DOM node would
// not survive one, which is the point of the measurement).
const rects = () =>
  page.evaluate(() =>
    [...document.querySelectorAll(".chat-session-row")].map((r) => {
      const t = r.querySelector(".title");
      const b = r.getBoundingClientRect();
      return { title: (t ? t.textContent : "").trim().slice(0, 24), top: Math.round(b.top * 100) / 100, h: Math.round(b.height * 100) / 100 };
    })
  );

const before = await rects();
console.log("[control] rows before: " + before.length);

// Force the perturbation. A chat with NO MESSAGES is filtered out of the list
// (t90), so creating one perturbs nothing -- measured: the first cut created an
// empty chat and the list did not change at all, which would have been a
// vacuous PASS for this criterion. Send one message so the row really appears,
// exactly like a teammate creating a test chat does.
const created = await page.evaluate(async (b) => {
  const res = await fetch(b + "/api/v1/chat", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ agent: "dsh" }),
  });
  const body = await res.text();
  let id = null;
  try { id = JSON.parse(body).id; } catch (e) {}
  if (id) {
    await fetch(b + "/api/v1/chat/" + id + "/messages", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ text: "t99 control perturbation (delete me)" }),
    });
  }
  return { status: res.status, id, body: body.slice(0, 90) };
}, base);
console.log("[control] created: " + JSON.stringify(created));

// Wait past the poll interval so the list actually refreshes AND the new row
// (which only shows once it has a message) has landed.
for (let i = 0; i < 80; i++) {
  await page.waitForTimeout(1000);
  const now = await rects();
  if (now.length > before.length) break;
}
const after = await rects();
console.log("[control] rows after: " + after.length);

const beforeByTitle = Object.fromEntries(before.map((r) => [r.title, r]));
const deltas = [];
for (const r of after) {
  const b = beforeByTitle[r.title];
  if (!b) continue;
  deltas.push({ title: r.title, topBefore: b.top, topAfter: r.top, delta: Math.round((r.top - b.top) * 100) / 100 });
}
const moved = deltas.filter((d) => Math.abs(d.delta) > 0.5);
const rowHeight = before.length ? before[0].h : null;

// The criterion, applied by the same rule the contract would carry.
const THRESHOLD_PX = 0.5;
const verdict = moved.length === 0 ? "PASS" : "FAIL";
console.log(
  "[control] tracked " + deltas.length + " rows; moved > " + THRESHOLD_PX + "px: " + moved.length +
    "  => criterion 2 is " + verdict
);
for (const d of moved.slice(0, 4)) {
  console.log("    " + JSON.stringify(d.title) + "  " + d.topBefore + " -> " + d.topAfter + "  (" + d.delta + "px)");
}

const summary = {
  base,
  at: new Date().toISOString(),
  created,
  rowsBefore: before.length,
  rowsAfter: after.length,
  rowHeight,
  thresholdPx: THRESHOLD_PX,
  tracked: deltas.length,
  movedCount: moved.length,
  moved: moved.slice(0, 12),
  verdict,
};
writeFileSync(join(outDir, "order-shift-control.json"), JSON.stringify(summary, null, 2));
// Clean up after ourselves with the endpoint t95 added -- the same one whose
// absence left test chats stuck in the user list in the first place.
if (created.id) {
  const del = await fetch(base + "/api/v1/chats/" + created.id, { method: "DELETE" });
  console.log("[control] cleanup delete -> " + del.status);
}
await browser.close();
