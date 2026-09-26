#!/usr/bin/env node
// Merge several design-audit metrics.json files into ONE full-run reading.
//
// WHY THIS EXISTS (t235/t238): one full run is 26 captures at ~28.5s each
// (~12.3 minutes) and the browser page dies part-way through, so a single
// snapshot of "the whole site" is not obtainable in one process. Batching is
// already expressible with --routes=..., so what was missing was not a batch
// switch but a MERGER -- and the merger must say out loud what it is doing.
//
// WHAT IT IS NOT: a single snapshot. Each batch is its own moment, so the
// merged reading is a CONCATENATION ACROSS TIME. That is stated in the output
// (per-batch timestamps + the overall window) because otherwise the word
// "full run" becomes an inference again -- the exact thing t235 removed.
//
// VERDICT AGGREGATION (per row, over the merged captures):
//   any capture FAILS          -> FAIL      (a defect anywhere is a defect)
//   else any capture is PASS   -> PASS      (measured somewhere, nowhere red)
//   else                       -> not_measured
// Row 52 is the reason the first rule matters: it is red on sessions and green
// on home, and the merged reading must not let home's green hide sessions' red.
import fs from "node:fs";
import path from "node:path";

const argv = process.argv.slice(2);
const val = (n, d) => { const eq = argv.find((a) => a.startsWith(n + "=")); if (eq) return eq.slice(n.length + 1); const i = argv.indexOf(n); return i >= 0 ? argv[i + 1] : d; };
const files = String(val("--in", "")).split(",").map((s) => s.trim()).filter(Boolean);
const json = argv.includes("--json");
if (!files.length) { console.log("usage: node tools/merge-audit.mjs --in=a/metrics.json,b/metrics.json [--json]"); process.exit(2); }

const batches = [];
for (const f of files) {
  const st = fs.statSync(f);
  const m = JSON.parse(fs.readFileSync(f, "utf8"));
  const caps = (m.captures || []).map((c) => ({ route: c.route, mode: c.mode }));
  batches.push({
    file: f,
    mtime: new Date(st.mtimeMs).toISOString().slice(11, 19),
    ms: st.mtimeMs,
    captures: caps,
    ms_total: (m.captures || []).reduce((a, c) => a + (c.ms || 0), 0),
    checks: m.checks || [],
  });
}

// Dedupe: the same (route, mode) captured twice is ONE object. The LAST batch
// wins, because a later capture of the same object is the more recent reading.
const byKey = new Map();
for (const b of batches) for (const c of b.captures) byKey.set(c.route + "/" + c.mode, { ...c, batch: b.file, mtime: b.ms });
const keys = [...byKey.keys()].sort();

// Per row: collect each capture's pass value out of its own evidence.
const rows = new Map();
for (const b of batches) {
  for (const r of b.checks) {
    if (!rows.has(r.n)) rows.set(r.n, { n: r.n, title: r.title, fails: new Set(), passes: new Set(), reasons: new Set(), captures: new Set() });
    const acc = rows.get(r.n);
    for (const e of r.evidence || []) {
      acc.captures.add(e.route + "/" + e.mode);
      if (e.pass === true) acc.passes.add(e.route + "/" + e.mode);
      else if (e.pass === false) acc.fails.add(e.route + "/" + e.mode);
      else if (e.display) acc.reasons.add(String(e.display).slice(0, 90));
    }
  }
}
let pass = 0, fail = 0, nm = 0;
const failingRows = [];
for (const r of [...rows.values()].sort((a, b) => a.n - b.n)) {
  if (r.fails.size) { fail++; failingRows.push(r.n); r.verdict = "fail"; }
  else if (r.passes.size) { pass++; r.verdict = "pass"; }
  else { nm++; r.verdict = "not_measured"; }
}
const win = batches.map((b) => b.mtime).sort();
console.log("merged from " + batches.length + " batches: " + batches.map((b) => b.mtime + " (" + b.captures.length + " captures, " + b.file + ")").join(" | "));
console.log("TIME WINDOW: " + win[0] + " - " + win[win.length - 1] + "  -- this is a CONCATENATION ACROSS TIME, not one snapshot");
console.log("captures: " + keys.length + " distinct (route/mode) of " + batches.reduce((a, b) => a + b.captures.length, 0) + " seen");
console.log("checks: " + pass + " pass / " + fail + " fail / " + nm + " not measured");
console.log("failing rows: " + (failingRows.length ? failingRows.join(",") : "(none)"));
if (json) console.log(JSON.stringify({ window: win, batches: batches.map((b) => ({ file: b.file, mtime: b.mtime, captures: b.captures.length })), captures: keys, pass, fail, notMeasured: nm, failingRows }, null, 1));
