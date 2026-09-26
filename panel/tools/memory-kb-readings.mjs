#!/usr/bin/env node
// t262/t263 - memory-kb-readings: the reproducible instrument for "what does each
// rendered number actually MEAN" on the memory/knowledge pages.
//
// Why it exists: t249 measured, with a one-off probe, that #knowledge's rendered
// "score" is an RRF rank score (0.016 = 1/61) and that #memory's "confidence" is
// only rendered below 0.5 while no row is below 0.5. Follow-up tasks need the
// same readings again; this is that probe, fixed and reusable.
//
// READ-ONLY. It never calls /api/v1/recall (that appends a recall_log row),
// never starts or stops the daemon, never writes under panel/src/, and does not
// touch design-audit.mjs or its lib/.
//
// Usage:
//   node tools/memory-kb-readings.mjs [--out <file>] [--self-test] [--routes=memory,knowledge,wiki,graph,stats]
// Output: JSON to --out, else $CARGO_TARGET_DIR/memory-kb-readings.json, else ./memory-kb-readings.json
//
// -- The extraction criterion (this is what --self-test falsifies) --
// A "number" is an element that (a) has NO element child carrying text, and
// (b) whose own trimmed text matches NUMBER_RE - i.e. it renders a numeral the
// reader can see. Deliberately NOT counted: numerals inside <script>/<style>,
// numerals in attributes (aria-label/title/value/datetime), and numerals glued
// into a longer word.
const VERSION = "t262.1";
const BASE = process.env.RUAGENT_BASE_URL || "http://127.0.0.1:8787";
const PAGES = [
  { id: "memory", hash: "#memory", note: "producer of the recall log" },
  { id: "knowledge", hash: "#knowledge" },
  { id: "wiki", hash: "#knowledge?tab=wiki", note: "#wiki is not a route; the wiki lives on #knowledge?tab=wiki" },
  { id: "graph", hash: "#graph" },
  { id: "stats", hash: "#stats", note: "the recall log is rendered HERE (Agents.tsx:793), not on #agents" },
];
function parseArgs(argv) {
  const o = { routes: null, out: null, selfTest: false };
  for (let i = 2; i < argv.length; i++) {
    const a = argv[i];
    if (a === "--self-test") o.selfTest = true;
    else if (a.indexOf("--out=") === 0) o.out = a.slice(6);
    else if (a === "--out") o.out = argv[++i];
    else if (a.indexOf("--routes=") === 0) o.routes = a.slice(9).split(",");
  }
  return o;
}
function outPath(o) {
  if (o.out) return o.out;
  if (process.env.CARGO_TARGET_DIR) return process.env.CARGO_TARGET_DIR + "/memory-kb-readings.json";
  return "memory-kb-readings.json";
}
/** The DOM-side extractor. Pure given a document, which is why --self-test can
 *  run it against synthetic markup instead of a live page. */
const EXTRACT = function () {
  const NUMBER_RE = /^-?\d{1,3}(?:,\d{3})*(?:\.\d+)?%?$|^-?\d+(?:\.\d+)?%?$|^\d+\s*\/\s*\d+$/;
  const SKIP = { SCRIPT: 1, STYLE: 1, NOSCRIPT: 1, TEMPLATE: 1, HEAD: 1, TITLE: 1 };
  const sel = (el) => {
    const parts = [];
    let n = el;
    for (let i = 0; i < 4 && n && n.nodeType === 1; i++) {
      let s = n.tagName.toLowerCase();
      if (n.className && typeof n.className === "string") {
        const c = n.className.trim().split(/\s+/).slice(0, 2).join(".");
        if (c) s += "." + c;
      }
      parts.unshift(s);
      const p = n.parentElement;
      if (!p) break;
      n = p;
      if (n.id || (n.className && String(n.className).indexOf("zone") >= 0)) break;
    }
    return parts.join(" > ");
  };
  const labelFor = (el) => {
    const own = (el.textContent || "").trim();
    const strip = own.replace(NUMBER_RE, "").replace(/[.:\-()]/g, " ").trim();
    if (strip) return strip.slice(0, 40);
    const al = el.getAttribute("aria-label") || el.getAttribute("title");
    if (al) return al.slice(0, 40);
    const prev = el.previousElementSibling;
    if (prev && (prev.textContent || "").trim()) return prev.textContent.trim().slice(0, 40);
    const p = el.parentElement;
    if (p) {
      const first = p.firstElementChild;
      if (first && first !== el && (first.textContent || "").trim()) return first.textContent.trim().slice(0, 40);
      const al2 = p.getAttribute("aria-label");
      if (al2) return al2.slice(0, 40);
    }
    return null;
  };
  // t262: the numeral is looked for INSIDE a text node, not required to be the
  // whole text node. The first version demanded a bare numeral and therefore
  // missed the very number this instrument exists to read: Knowledge renders
  // "score 0.016" as ONE text node. The lookbehind/lookahead keep a numeral
  // glued into a word (abc123def) out, which is what --self-test falsifies.
  const NUM_G = /(?<![\w.])(\d+\s*\/\s*\d+|-?\d{1,3}(?:,\d{3})*(?:\.\d+)?%?|-?\d+(?:\.\d+)?%?)(?![\w])/g;
  const found = [];
  const all = document.querySelectorAll("body *");
  for (const el of all) {
    if (SKIP[el.tagName]) continue;
    let hasTextChild = false;
    for (const c of el.children) if ((c.textContent || "").trim()) { hasTextChild = true; break; }
    if (hasTextChild) continue;
    const text = (el.textContent || "").trim();
    if (!text || text.length > 60) continue;
    const hits = text.match(NUM_G) || [];
    for (const h of hits) {
      const around = text.replace(h, " ").replace(/\s+/g, " ").trim();
      found.push({ text: h, label: around ? around.slice(0, 40) : labelFor(el), context: text.slice(0, 60), tag: el.tagName.toLowerCase(), selector: sel(el), container: sel(el.parentElement || el) });
    }
  }
  const zones = [];
  for (const z of document.querySelectorAll(".zone-title, .zone-note, h2, h3")) {
    const tx = (z.textContent || "").trim();
    if (tx) zones.push(tx);
    if (zones.length >= 24) break;
  }
  const recallLog = /最近召回|Recent recalls/.test(document.body.innerText || "");
  return { numbers: found, zones: zones, recallLogPresent: recallLog, domNodes: all.length };
};
const SYNTHETIC = '<!doctype html><html><body>'
  + '<div class="readout"><span class="k">画像</span><span class="v">43</span></div>'
  + '<div class="card"><span class="muted mono">score 0.016</span></div>'
  + '<div class="card"><span class="muted">已显示 13 / 共 13</span></div>'
  + '<div class="card"><span class="muted">2/5 页</span></div>'
  + '<div class="zone-title">最近召回</div>'
  + '<span aria-label="99">-</span>'
  + '<script>var x = 4242;</script>'
  + '<style>.a { width: 77px }</style>'
  + '<time datetime="2026-09-26">2 天前</time>'
  + '<span>abc123def</span>'
  + '</body></html>';
async function selfTest(chromium) {
  const b = await chromium.launch();
  const p = await b.newPage();
  await p.setContent(SYNTHETIC);
  const got = await p.evaluate(EXTRACT);
  await b.close();
  const texts = got.numbers.map((n) => n.text);
  const checks = [
    { name: "must-hit: a readout value (43)", pass: texts.indexOf("43") >= 0 },
    { name: "must-hit: a prefixed value (score 0.016)", pass: texts.indexOf("0.016") >= 0 },
    { name: "must-hit: a contiguous ratio (2/5, the shape the wiki renders)", pass: texts.indexOf("2/5") >= 0 },
    { name: "must-miss: a numeral inside <script> (4242)", pass: texts.indexOf("4242") < 0 },
    { name: "must-miss: a numeral inside <style> (77)", pass: texts.indexOf("77") < 0 },
    { name: "must-miss: a numeral in an attribute (aria-label 99)", pass: texts.indexOf("99") < 0 },
    { name: "must-miss: a numeral glued into a word (abc123def)", pass: texts.indexOf("abc123def") < 0 },
    { name: "must-hit: the recall-log zone is detected", pass: got.recallLogPresent === true },
  ];
  const failed = checks.filter((c) => !c.pass);
  return {
    checks: checks, pass: failed.length === 0,
    extracted: got.numbers.map((n) => ({ text: n.text, label: n.label })),
    doesNotJudge: [
      "whether the number is CORRECT (it reads what is rendered; the DB is not consulted)",
      "what the number MEANS (that mapping lives in t249's reading, not in this probe)",
      "an element that renders a numeral together with its unit in one text node (e.g. 36 分钟前) - not a bare numeral, deliberately skipped",
      "hidden nodes: everything in the DOM counts, including a display:none block",
      "whether a page is EMPTY because of a real empty state or because a bad payload blanked it (t249's false 'wiki empty state' came from exactly that) - read zones and domNodes together",
    ],
  };
}
async function main() {
  const o = parseArgs(process.argv);
  const { chromium } = await import("@playwright/test");
  if (o.selfTest) {
    const st = await selfTest(chromium);
    console.log(JSON.stringify(st, null, 1));
    process.exit(st.pass ? 0 : 1);
  }
  const pages = o.routes ? PAGES.filter((p) => o.routes.indexOf(p.id) >= 0) : PAGES;
  const b = await chromium.launch();
  const ctx = await b.newContext({ viewport: { width: 1440, height: 900 }, colorScheme: "dark" });
  const results = [];
  for (const pg of pages) {
    const p = await ctx.newPage();
    const reqs = [];
    p.on("request", (r) => { const u = new URL(r.url()); if (u.pathname.indexOf("/api/") === 0) reqs.push(r.method() + " " + u.pathname + (u.search || "")); });
    const errs = [];
    p.on("console", (m) => { if (m.type() === "error") errs.push(m.text().slice(0, 80)); });
    p.on("pageerror", (e) => errs.push("pageerror: " + String(e).slice(0, 80)));
    await p.goto(BASE + "/" + pg.hash, { waitUntil: "load" });
    await p.waitForTimeout(3200);
    const dom = await p.evaluate(EXTRACT);
    results.push({
      route: pg.id, hash: pg.hash, note: pg.note || null,
      provenance: { version: VERSION, base: BASE, viewport: "1440x900", colorScheme: "dark", at: new Date().toISOString() },
      numbers: dom.numbers, numberCount: dom.numbers.length,
      zones: dom.zones, domNodes: dom.domNodes, recallLogPresent: dom.recallLogPresent,
      requests: { count: reqs.length, paths: reqs },
      consoleErrors: errs,
    });
    await p.close();
  }
  await b.close();
  const doc = {
    version: VERSION, at: new Date().toISOString(), base: BASE,
    provenance: { viewport: "1440x900", colorScheme: "dark", routes: results.map((r) => r.route) },
    results: results,
    recallLog: { renderedOn: results.filter((r) => r.recallLogPresent).map((r) => r.route), producer: "memory (t249)" },
  };
  const { writeFileSync } = await import("node:fs");
  const out = outPath(o);
  writeFileSync(out, JSON.stringify(doc, null, 1));
  for (const r of results) console.log(r.route + ": numbers=" + r.numberCount + " requests=" + r.requests.count + " recallLog=" + r.recallLogPresent + " errors=" + r.consoleErrors.length);
  console.log("wrote " + out);
}
main().catch((e) => { console.error("memory-kb-readings failed: " + (e && e.stack ? e.stack : e)); process.exit(1); });
