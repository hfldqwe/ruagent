// lib/contract.mjs — read the *criteria* out of the design contract.
//
// WHY THIS EXISTS
// A threshold that lives only in the audit script is a number nobody signed.
// t17 found three rows whose judge had drifted from the contract (row 6's
// §12.1 conditional rule was never implemented, row 7 classified pages with a
// DOM heuristic instead of the specs, row 5 did not exclude .raw), and each
// drift made a real page report the wrong verdict. The fix is not to hardcode
// the right answer in a second place — it is to read the answer from
// docs/design/{MASTER.md, primitives.md, views/*.md} and to *say so* when the
// read fails.
//
// Every loader returns { value, source, warnings }. 'value' is null when the
// contract could not be read; callers then fall back to the built-in AND surface
// the warning, so a fallback is always visible in the report. Nothing here
// silently substitutes a built-in for a contract value.

import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";

export const cleanCell = (s) =>
  String(s ?? "").replace(/\*\*/g, "").replace(/\u0060/g, "").trim();

const cellSplit = (line) =>
  line.replace(/^\|/, "").replace(/\|\s*$/, "").split("|").map((c) => c.trim());

const numsIn = (s) => {
  const m = String(s ?? "").match(/-?\d[\d,]*(?:\.\d+)?/g);
  return m ? m.map((t) => parseFloat(t.replace(/,/g, ""))) : [];
};

// Strict numbers, for brace/enumerated lists. A comma-tolerant read of
// "{0,4,6,10,14,50%,999px,3px}" swallows "14,50" as one thousands-separated
// number and yields 1450 — which is exactly the kind of silent misread this
// whole module exists to prevent.
const listNums = (s) => {
  const m = String(s ?? "").match(/\d+(?:\.\d+)?/g);
  return m ? m.map(Number) : [];
};

// Every backticked run in 's', with its index (used to pair a selector with the
// px value written before it).
const tickedRuns = (s) => {
  const out = [];
  const re = /\u0060([^\u0060\n]+)\u0060/g;
  let m;
  while ((m = re.exec(String(s ?? "")))) out.push({ text: m[1].trim(), index: m.index });
  return out;
};

// A CSS selector, not prose that merely happens to be backticked. This guard is
// what stops the literal "fontSize >= 18px" from being read as a title carrier.
const SELECTOR_SHAPE = /^[.#][\w-]+((\s*[>+~]\s*|\s+)[.#]?[\w-]+)*$/;

export function readDoc(path) {
  try {
    return existsSync(path) ? readFileSync(path, "utf8") : null;
  } catch {
    return null;
  }
}

// The text of one section, from startRe up to (not including) endRe.
export function sliceSection(md, startRe, endRe) {
  if (!md) return null;
  const m = md.match(startRe);
  if (!m) return null;
  const rest = md.slice(m.index + m[0].length);
  const e = endRe ? rest.match(endRe) : null;
  return e ? rest.slice(0, e.index) : rest;
}

// Markdown tables inside 'section': [{ header, rows }]. A table starts at the
// first pipe-line (its header) and collects rows after the |---| separator.
// Returning per-table (not one flat row list) matters: MASTER §6.1 puts the
// radius ladder and the radius *violations* in two adjacent tables, and a flat
// read would legalise 8px / 12px / 100px.
export function tables(section) {
  const out = [];
  let cur = null;
  for (const line of String(section ?? "").split(/\r?\n/)) {
    if (!line.startsWith("|")) {
      cur = null;
      continue;
    }
    if (/^\|[\s:|-]+\|$/.test(line)) {
      if (cur) cur.body = true;
      continue;
    }
    if (!cur) {
      cur = { header: cellSplit(line), rows: [], body: false };
      out.push(cur);
      continue;
    }
    if (cur.body) cur.rows.push(cellSplit(line));
  }
  return out;
}

const firstTableWithHeader = (section, re) =>
  tables(section).find((t) => re.test(t.header.join(" | ")));

// ── MASTER §12.1: the row-6 exempt routes ───────────────────────────────────
// Read, never hardcoded: the list is the contract's, and §12.1 is explicit that
// the exemption is conditional ("含读数的页 >=3；无读数的页 >=1"), not a flat
// lowering of the floor.
export function loadExemptRoutes(masterMd) {
  const warnings = [];
  const value = new Map();
  const section = sliceSection(masterMd, /^### 12\.1/m, /^### 12\.2/m);
  if (!section) {
    warnings.push("MASTER §12.1 not found: row 6 has no exempt-route list (falling back to the flat threshold)");
    return { value, source: null, warnings };
  }
  const t = firstTableWithHeader(section, /豁免路由/);
  if (!t) {
    warnings.push("MASTER §12.1 has no route table: row 6 has no exempt-route list");
    return { value, source: null, warnings };
  }
  for (const r of t.rows) {
    const route = cleanCell(r[0]);
    if (!route.startsWith("#")) continue;
    const threshold = numsIn(cleanCell(r[r.length - 1]))[0];
    if (!Number.isFinite(threshold)) {
      warnings.push("MASTER §12.1 row " + route + " carries no numeric 门槛; skipped");
      continue;
    }
    value.set(route, { threshold, why: cleanCell(r[2] ?? "").slice(0, 140), raw: r.join(" | ") });
  }
  if (!value.size) warnings.push("MASTER §12.1 parsed but yielded no exempt routes");
  return { value, source: "MASTER §12.1", warnings };
}

// ── MASTER §12 row 6: the page-title ("层级中心") sub-assertion ─────────────
// §12.1 calls it the exemption's price: an exempt page may not manufacture
// readouts, but it must still have exactly one page title. The carriers and
// their sizes are read out of row 6's own judge cell, which reads
// "…恰好 1 个 20px 的页面标题（'.view-bar h2'；'#home' 用 32px 的 '.home-hero h1'）".
// thresholds.mjs hands back *cleaned* cells (backticks and ** stripped), and the
// row-6 carriers live inside backticks. Re-read the raw table line instead of
// losing them: a criterion that cannot be read is a criterion nobody can check.
const rawCell = (row, from) => {
  if (!row?.raw) return null;
  const parts = String(row.raw).replace(/^\|/, "").replace(/\|\s*$/, "").split("|");
  return parts.length > from ? parts.slice(from).join("|") : null;
};

export function loadPageTitleRule(masterMd, row6) {
  const warnings = [];
  const fallback = {
    default: { selectors: [".view-bar h2"], minPx: 20 },
    overrides: { home: { selectors: [".home-hero h1"], minPx: 32 } },
  };
  const cell = String(rawCell(row6, 4) ?? row6?.judge ?? "");
  if (!cell) {
    warnings.push("MASTER §12 row 6 judge cell missing: page-title rule uses the built-in carriers");
    return { value: fallback, source: "builtin", warnings };
  }
  const cands = [];
  for (const tk of tickedRuns(cell)) {
    // Must be a class-based selector: in this document a bare "#home" is a route
    // reference, not a carrier, and it is backticked just like the real ones.
    if (!SELECTOR_SHAPE.test(tk.text) || !tk.text.includes(".")) continue;
    const before = cell.slice(Math.max(0, tk.index - 56), tk.index);
    const px = [...before.matchAll(/(\d+)\s*px/g)].map((m) => +m[1]);
    cands.push({ selector: tk.text, minPx: px.length ? px[px.length - 1] : null, index: tk.index });
  }
  const homeIdx = cell.indexOf("#home");
  const isOverride = (c) => homeIdx >= 0 && c.index > homeIdx;
  const base = cands.filter((c) => !isOverride(c) && c.minPx != null);
  const over = cands.filter((c) => isOverride(c) && c.minPx != null);
  if (!base.length || !over.length) {
    warnings.push(
      "MASTER §12 row 6 page-title carriers not readable (base=" + base.length +
        ", home-override=" + over.length + "): using the built-in rule",
    );
    return { value: fallback, source: "builtin", warnings };
  }
  return {
    value: {
      default: { selectors: base.map((c) => c.selector), minPx: base[0].minPx },
      overrides: { home: { selectors: over.map((c) => c.selector), minPx: over[0].minPx } },
    },
    source: "MASTER §12 行 6",
    warnings,
  };
}

export const pageTitleFor = (rule, route) => rule?.overrides?.[route] ?? rule?.default ?? null;

// ── MASTER §4.1: the tier sizes a page title may sit on ─────────────────────
// §12 row 6 says the page title is "20px"; §4.1 says which tier each carrier
// belongs to and names them: the "title" row is ".view-bar h2" (20px) and the
// "display" row is "#home hero h1" (24px). Row 6's own parenthetical calls
// #home's h1 32px, which is the *readout* tier and contradicts §4.1 — so the
// tier sizes, not that parenthesis, are what the sub-assertion uses. The
// disagreement is reported rather than silently resolved.
export function loadTitleTiers(masterMd) {
  const warnings = [];
  const section = sliceSection(masterMd, /^### 4\.1/m, /^### 4\.2/m);
  const t = section && tables(section)[0];
  const out = { title: null, display: null };
  if (t) {
    for (const r of t.rows) {
      const name = cleanCell(r[0]).toLowerCase().replace(/\s+/g, "");
      const px = numsIn(cleanCell(r[1]))[0];
      if (!Number.isFinite(px)) continue;
      if (name === "title") out.title = px;
      if (name === "display") out.display = px;
    }
  }
  if (out.title == null) {
    warnings.push("MASTER §4.1 title tier not readable; the page-title floor falls back to 20px");
  }
  return { value: out, source: out.title != null ? "MASTER §4.1" : null, warnings };
}

// ── views/view-*.md: the per-route row-7 requirement ────────────────────────
// Row 7's target ("≥20px；含仪表盘的页 ≥32px") needs a page classification, and
// the classification is a design decision, not a DOM accident. Each spec states
// it in its own acceptance table (F5 / C3 / D8 / …): "≥32px（含仪表盘）" versus
// "≥20px（无仪表盘页）". Reading it here is why #task and #agents stop being
// judged as dashboards — both specs say 20px on purpose.
export function loadViewSpecs(repoRoot, routes) {
  const warnings = [];
  const value = new Map();
  const dir = join(repoRoot, "docs", "design", "views");
  const readme = readDoc(join(dir, "README.md"));

  // route -> spec file, from README §1's index table (#task/<id> -> task).
  const fileByRoute = new Map();
  const idx = sliceSection(readme, /^## 1 /m, /^## 2 /m);
  const idxTable = idx && firstTableWithHeader(idx, /路由（hash）/);
  if (idxTable) {
    for (const r of idxTable.rows) {
      const hash = cleanCell(r[1] ?? "");
      const link = String(r[2] ?? "").match(/\(\.\/([\w.-]+)\)/);
      if (hash.startsWith("#") && link) fileByRoute.set(hash.replace(/^#/, "").split("/")[0], link[1]);
    }
  }
  if (!fileByRoute.size) {
    warnings.push("views/README.md §1 index not readable: falling back to view-<route>.md naming");
  }

  for (const route of routes) {
    const file = fileByRoute.get(route.id) ?? (route.id === "task" ? "view-task-detail.md" : "view-" + route.id + ".md");
    const md = readDoc(join(dir, file));
    if (!md) {
      warnings.push("row 7: spec " + file + " for #" + route.id + " not found; the page is judged as a plain page");
      continue;
    }
    let found = null;
    for (const t of tables(md)) {
      for (const r of t.rows) {
        const joined = r.join(" | ");
        if (!/最大字号/.test(joined)) continue;
        const target = r.slice(3).join(" ") || joined;
        const px = [...target.matchAll(/≥\s*(\d+)\s*px/g)].map((m) => +m[1]);
        if (!px.length) continue;
        found = { file, maxFontPx: Math.max(...px), target: cleanCell(target).slice(0, 120), raw: cleanCell(joined).slice(0, 160) };
        break;
      }
      if (found) break;
    }
    if (!found) {
      warnings.push("row 7: " + file + " states no 最大字号 requirement for #" + route.id + "; judged as a plain page");
      continue;
    }
    value.set(route.id, found);
  }
  return { value, source: "docs/design/views/view-*.md", warnings };
}

// ── the ladders (rows 8 / 9 / 10 / 37) ──────────────────────────────────────
// These used to be module constants. They are the contract's numbers, so they
// are read from it: §4.1 (font), §4.3 (weight), §5.1 (spacing) and, for radius,
// primitives §11 row 10 — the only place that registers the 3px scrollbar thumb
// (without it the row could never reach 0).
export function loadLadders({ masterMd, primitivesMd }) {
  const warnings = [];
  const out = { font: null, weight: null, radius: null, radiusPill: null, spacing: null };

  const s41 = sliceSection(masterMd, /^### 4\.1/m, /^### 4\.2/m);
  const t41 = s41 && tables(s41)[0];
  if (t41) {
    const v = [...new Set(t41.rows.map((r) => numsIn(cleanCell(r[1]))[0]).filter(Number.isFinite))].sort((a, b) => a - b);
    if (v.length >= 8 && v.every((x) => x >= 10 && x <= 48)) out.font = v;
    else warnings.push("MASTER §4.1 font ladder read implausibly (" + v.join("/") + "); using the built-in ladder");
  } else warnings.push("MASTER §4.1 not readable; using the built-in font ladder");

  const s43 = sliceSection(masterMd, /^### 4\.3/m, /^### 4\.4/m);
  const t43 = s43 && tables(s43)[0];
  if (t43) {
    const v = [...new Set(t43.rows.map((r) => numsIn(cleanCell(r[0]))[0]).filter(Number.isFinite))].sort((a, b) => a - b);
    if (v.length >= 3 && v.every((x) => x >= 100 && x <= 900)) out.weight = v;
    else warnings.push("MASTER §4.3 weight ladder read implausibly (" + v.join("/") + "); using the built-in ladder");
  } else warnings.push("MASTER §4.3 not readable; using the built-in weight ladder");

  const s11 = sliceSection(primitivesMd, /^## 11 /m, /^## 12 /m);
  const t11 = s11 && tables(s11).find((t) => t.rows.some((r) => cleanCell(r[0]) === "10"));
  const row10 = t11?.rows.find((r) => cleanCell(r[0]) === "10");
  const brace = row10 ? String(row10[row10.length - 1]).match(/\{([^}]*)\}/) : null;
  if (brace) {
    const all = [...new Set(listNums(cleanCell(brace[1])))].sort((a, b) => a - b);
    const ladder = all.filter((x) => x < 50);
    const pill = all.find((x) => x >= 50) ?? null;
    if (ladder.length >= 4 && pill != null) {
      out.radius = ladder;
      out.radiusPill = pill;
    } else warnings.push("primitives §11 row 10 radius set read implausibly (" + all.join("/") + "); using the built-in ladder");
  } else warnings.push("primitives §11 row 10 not readable; using the built-in radius ladder");

  const s51 = sliceSection(masterMd, /^### 5\.1/m, /^### 5\.2/m);
  const t51 = s51 && tables(s51)[0];
  if (t51) {
    const v = [...new Set([0, ...t51.rows.map((r) => numsIn(cleanCell(r[1]))[0])].filter(Number.isFinite))].sort((a, b) => a - b);
    if (v.length >= 8) out.spacing = v;
    else warnings.push("MASTER §5.1 spacing ladder read implausibly (" + v.join("/") + "); using the built-in ladder");
  } else warnings.push("MASTER §5.1 not readable; using the built-in spacing ladder");

  return { value: out, source: "MASTER §4.1/§4.3/§5.1 + primitives §11 行 10", warnings };
}

// ── primitives §5.2.2: the hand-written expand carriers (row 23) ────────────
export function loadExpandSelectors(primitivesMd) {
  const warnings = [];
  const section = sliceSection(primitivesMd, /^#### 5\.2\.2/m, /^#### 5\.2\.3/m);
  const t = section && firstTableWithHeader(section, /选择器/);
  if (!t) {
    warnings.push("primitives §5.2.2 carrier table not readable; using the built-in carrier list");
    return { value: null, source: null, warnings };
  }
  const hand = [];
  let native = null;
  for (const r of t.rows) {
    const sel = cleanCell(r[0]);
    if (!sel.startsWith(".")) continue;
    if (/summary/.test(sel)) native = sel;
    else if (SELECTOR_SHAPE.test(sel)) hand.push(sel);
  }
  if (hand.length < 2) {
    warnings.push("primitives §5.2.2 carrier table yielded " + hand.length + " carriers; using the built-in list");
    return { value: null, source: null, warnings };
  }
  return { value: { hand, native }, source: "primitives §5.2.2", warnings };
}

// ── primitives §10.1 N21: the hand-written clickables (row 34) ──────────────
export function loadHandClasses(primitivesMd) {
  const warnings = [];
  const section = sliceSection(primitivesMd, /^### 10\.1/m, /^### 10\.2/m);
  let classes = null;
  if (section) {
    for (const t of tables(section)) {
      for (const r of t.rows) {
        if (cleanCell(r[0]) !== "N21") continue;
        const selPart = cleanCell(r[2] ?? "").split("{")[0];
        const found = [...selPart.matchAll(/\.([\w-]+)\s*:active/g)].map((m) => "." + m[1]);
        if (found.length) classes = [...new Set(found)];
      }
    }
  }
  if (!classes || classes.length < 3) {
    warnings.push("primitives §10.1 N21 not readable; using the built-in hand-clickable list");
    return { value: null, source: null, warnings };
  }
  return { value: classes, source: "primitives §10.1 N21", warnings };
}

// ── MASTER §3.1 F: the graph category tokens (row 33) ───────────────────────
export function loadGraphVars(masterMd) {
  const warnings = [];
  const section = sliceSection(masterMd, /\*\*F\. 图谱分类色\*\*/, /^\*\*G\./m);
  const t = section && tables(section)[0];
  if (!t) {
    warnings.push("MASTER §3.1 F not readable; using the built-in graph token list");
    return { value: null, source: null, warnings };
  }
  const vars = t.rows
    .map((r) => cleanCell(r[1]))
    .filter((v) => /^--graph-/.test(v) && !/^--graph-(edge|edge-hi|label)$/.test(v));
  if (vars.length < 5) {
    warnings.push("MASTER §3.1 F yielded " + vars.length + " category tokens; using the built-in list");
    return { value: null, source: null, warnings };
  }
  return { value: vars, source: "MASTER §3.1 F", warnings };
}

// ── view-task-detail.md D11: what row 5's count excludes ───────────────────
// MASTER row 5's own judge cell only names ".md th/td"; D11 adds .raw and says
// why ("判定必须带这条排除规则，否则本页永远不通过"). Read both from the spec so
// the exclusion is the contract's, not the audit script's.
export function loadRow5Exclusions(repoRoot) {
  const warnings = [];
  const fallback = { selectors: [".raw"], mdCells: true };
  const md = readDoc(join(repoRoot, "docs", "design", "views", "view-task-detail.md"));
  if (!md) {
    warnings.push("view-task-detail.md not readable; row 5 uses the built-in exclusion list");
    return { value: fallback, source: "builtin", warnings };
  }
  let row = null;
  for (const t of tables(md)) for (const r of t.rows) if (cleanCell(r[0]) === "D11") row = r;
  if (!row) {
    warnings.push("view-task-detail.md D11 not readable; row 5 uses the built-in exclusion list");
    return { value: fallback, source: "builtin", warnings };
  }
  const target = String(row[3] ?? "");
  const toks = tickedRuns(target).map((t) => t.text);
  const selectors = toks.filter((t) => t.startsWith(".") && !/td\/th|th\/td/.test(t));
  const mdCells = toks.some((t) => /td\/th|th\/td/.test(t));
  if (!selectors.length) {
    warnings.push("view-task-detail.md D11 states no class exclusion; row 5 uses the built-in list");
    return { value: fallback, source: "builtin", warnings };
  }
  return { value: { selectors, mdCells }, source: "view-task-detail.md D11", warnings };
}

// ── MASTER §12.6: the named bleed closed set (row 37) ───────────────────────
// Negatives are not spacing steps — they are optical bleed, a different
// judgement domain. But they enter by ticket only: a bare "{-4,-8,-12} is
// always fine" rule would let any element push its hover surface out of its row
// with -12, and the row would stop meaning anything. So the value must match AND
// the element must be one of the named carriers, each with a cited rule.
export function loadBleedSet(masterMd) {
  const warnings = [];
  const section = sliceSection(masterMd, /^### 12\.6/m, /^### 12\.7|^## 附录/m);
  const t = section && firstTableWithHeader(section, /载体/);
  if (!t) {
    warnings.push("MASTER §12.6 bleed table not readable; using the built-in closed set");
    return { value: null, source: null, warnings };
  }
  const value = [];
  for (const r of t.rows) {
    const nums = numsIn(cleanCell(r[0]));
    const sel = cleanCell(r[1]);
    if (!nums.length || !sel.startsWith(".")) continue;
    if (nums[0] >= 0) continue;
    value.push({ value: nums[0], selector: sel, source: cleanCell(r[2] ?? "").slice(0, 120) });
  }
  if (value.length < 2) {
    warnings.push("MASTER §12.6 bleed table yielded " + value.length + " carriers; using the built-in closed set");
    return { value: null, source: null, warnings };
  }
  return { value, source: "MASTER §12.6", warnings };
}

// ── MASTER §12.9: the shell's named closed set (row 18's third sub-assertion) ─
// Read, not hardcoded: t36 moved three buttons out of this set by raising
// controlHeightSM, and a literal list would have gone stale exactly as the
// document did. Membership is by identity (tag + class + geometry), not by a
// count — §12.9 says the count may change but "which ones" is the contract.
export function loadShellSet(masterMd) {
  const warnings = [];
  const section = sliceSection(masterMd, /^### 12\.9/m, /^### 12\.10|^## /m);
  if (!section) {
    warnings.push("MASTER §12.9 not found; the shell closed set is unknown");
    return { value: null, source: null, warnings };
  }
  const lines = section.split(/\r?\n/);
  const start = lines.findIndex((l) => /外壳清单\s*=\s*具名闭集/.test(l));
  if (start < 0) {
    warnings.push("MASTER §12.9 has no 「外壳清单 = 具名闭集」 block; the shell closed set is unknown");
    return { value: null, source: null, warnings };
  }
  const items = [];
  for (let i = start + 1; i < lines.length; i++) {
    const line = lines[i];
    // The block ends at the provenance note, the retired-members list, or the
    // verdict line — everything after that is commentary, not the set.
    if (/几何冻结依据|已退出集合|^\s*判定\s*[:：]|^\s*```/.test(line)) break;
    const m = line.match(/^\s*([A-Za-z][\w-]*(?:\.[\w-]+)+)\s+(\d+)\s*×\s*(\d+)\s*$/);
    if (!m) continue;
    const [tag, ...classes] = m[1].split(".");
    items.push({ tag: tag.toLowerCase(), classes, w: Number(m[2]), h: Number(m[3]), raw: m[1] });
  }
  if (!items.length) {
    warnings.push("MASTER §12.9's shell closed set yielded no parsable member; the shell check cannot be judged");
    return { value: null, source: null, warnings };
  }
  return { value: items, source: "MASTER §12.9", warnings };
}

// §12.9 ②: membership is identity (tag + class + geometry), so a member that
// merely shares a size with a retired one is NOT admitted.
export function shellSetAdmits(allowed, measured) {
  if (!allowed) return null;
  return allowed.some(
    (a) =>
      a.tag === String(measured.tag ?? "").toLowerCase() &&
      a.classes.every((cl) => String(measured.cls ?? "").split(/\s+/).includes(cl)) &&
      a.w === measured.w &&
      a.h === measured.h,
  );
}

// ── primitives §10.1 M91: the allowed breakpoints (row 35) ──────────────────
export function loadBreakpoints(primitivesMd) {
  const warnings = [];
  const section = sliceSection(primitivesMd, /^### 10\.1/m, /^### 10\.2/m);
  let row = null;
  if (section) {
    // M91 sits *after* the "编号说明" blockquote, i.e. orphaned from the 修改
    // table it belongs to — it is the only |M91| line with no header above it,
    // so a table-shaped read never sees it. Match the line directly.
    for (const line of section.split(/\r?\n/)) {
      if (!/^\|\s*M91\s*\|/.test(line)) continue;
      row = cellSplit(line);
    }
  }
  if (!row) {
    warnings.push("primitives §10.1 M91 not readable; using the built-in breakpoint set");
    return { value: null, source: null, warnings };
  }
  // "收敛后只允许 520 / 768 / 1024 / 1240；992 出现在 App.tsx 的 matchMedia（契约值，不在 CSS 里）"
  const cell = cleanCell(row[2] ?? "");
  const [cssPart, jsPart = ""] = cell.split("；");
  const css = [...new Set(listNums(cssPart))].sort((a, b) => a - b);
  const js = [...new Set(listNums(jsPart))].sort((a, b) => a - b);
  if (css.length < 3 || !js.length) {
    warnings.push("primitives §10.1 M91 breakpoint sets read implausibly; using the built-in set");
    return { value: null, source: null, warnings };
  }
  return { value: { css, js }, source: "primitives §10.1 M91", warnings };
}

// One call for the whole contract. 'routes' is the tool's ROUTES array.
export function loadContract(repoRoot, routes, thresholds) {
  const masterMd = readDoc(join(repoRoot, "docs", "design", "MASTER.md"));
  const primitivesMd = readDoc(join(repoRoot, "docs", "design", "primitives.md"));
  const parts = {
    exemptRoutes: loadExemptRoutes(masterMd),
    pageTitle: loadPageTitleRule(masterMd, thresholds?.rows?.get(6)),
    titleTiers: loadTitleTiers(masterMd),
    viewSpecs: loadViewSpecs(repoRoot, routes),
    ladders: loadLadders({ masterMd, primitivesMd }),
    expand: loadExpandSelectors(primitivesMd),
    handClasses: loadHandClasses(primitivesMd),
    graphVars: loadGraphVars(masterMd),
    breakpoints: loadBreakpoints(primitivesMd),
    row5Exclusions: loadRow5Exclusions(repoRoot),
    bleedSet: loadBleedSet(masterMd),
    shellSet: loadShellSet(masterMd),
  };
  const warnings = [];
  const sources = {};
  for (const [k, v] of Object.entries(parts)) {
    warnings.push(...(v.warnings ?? []));
    sources[k] = v.source;
  }
  return { ...parts, sources, warnings };
}
