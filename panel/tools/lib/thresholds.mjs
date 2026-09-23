// Threshold-table reader for docs/design/MASTER.md §12.
//
// The audit must not invent its own targets: §12 is the contract. This module
// pulls the markdown table out of the doc and hands the raw cells to the audit,
// which then parses the numbers it needs *out of that text*. If the doc (or a
// row) is missing, the audit falls back to built-in defaults and says so in
// its warnings — it never silently judges against a number nobody wrote down.

import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";

/** Strip markdown decoration (`code`, **bold**) from a table cell. */
export function cleanCell(s) {
  return String(s ?? "")
    .replace(/`/g, "")
    .replace(/\*\*/g, "")
    .trim();
}

// `1,234` / `3.0` / `-12` / `2.4` -> numbers. Thousands separators are dropped.
/**
 * Read a requirement by MEANING, not by position.
 *
 * A §12 target cell mixes the requirement with baseline values, historical
 * values and section cross-references, and `extractNums` turns 「见 §12.7.2」
 * into the numbers 12.7 and 2. Three separate defects came from reading the
 * i-th number of the cell instead of the number the sentence points at:
 *   · row 6  — an upper bound (≤40) was read as an exemption floor;
 *   · row 18 — 「见 §12.7.2」 pushed the <32px cap to 2;
 *   · row 1  — a baseline value became the goal.
 * Every row therefore anchors on the phrase/comparator that carries the
 * requirement, and falls back to the built-in target only when the anchor is
 * absent (which the audit reports as a contract-drift warning).
 */
export function pick(text, re, fallback, group = 1) {
  const m = String(text ?? "").match(re);
  if (!m) {
    // A silent fallback is how a criterion rots: MASTER gets reworded, the anchor
    // stops matching, and the row keeps judging against a built-in number nobody
    // re-read. Record the miss so the caller can say so out loud.
    pickMisses.push({ re: String(re), fallback });
    return fallback;
  }
  const v = parseFloat(String(m[group]).replace(/,/g, ""));
  return Number.isFinite(v) ? v : fallback;
}

let pickMisses = [];
export function resetPickMisses() {
  pickMisses = [];
}
export function takePickMisses() {
  const m = pickMisses;
  pickMisses = [];
  return m;
}

/**
 * Read a number that the contract may phrase more than one way.
 *
 * §12.16 reworded row 35's target from 「≤4 CSS 断点 + 1 契约值(992)」 to
 * 「① CSS 侧 ≤4 个断点 ∧ …」 — same number, different sentence. Chaining two
 * pick() calls cannot express that: the first one records a miss for the wording
 * that is absent even when the second matches, and the row is then reported as
 * contract drift when it is not. This tries each phrasing and records a miss
 * only when NONE of them match.
 */
export function pickAny(text, res, fallback) {
  for (const re of res) {
    const m = String(text ?? "").match(re);
    if (!m) continue;
    const v = parseFloat(String(m[1]).replace(/,/g, ""));
    if (Number.isFinite(v)) return v;
  }
  pickMisses.push({ re: res.map(String).join(" | "), fallback });
  return fallback;
}

/**
 * The leading integer of a §12 target cell.
 *
 * For a group of rows the requirement IS the first token of the cell, but the
 * cell also carries markdown bold and, increasingly, an inline observation
 * record after it. Row 13 today reads:
 *
 *     **0**（当前观测：工具 02:26:57 … build DKadudCh 上报 knowledge/dark = 1 …
 *            需以最新工具复测为准）
 *
 * Anchoring on /^\s*(\d+)\s*$/ stopped matching that, and the row silently fell
 * back to its built-in 0 — the exact rot the miss-recording above exists to
 * surface. This tolerates exactly TWO decorations and nothing else: bold
 * markers, and a parenthetical (or end of cell) right after the number. A
 * baseline or historical value further along the cell therefore still cannot
 * move a target, and a cell that STARTS with a non-integer (13.7–83.5%) does not
 * match at all — it keeps its built-in default and records a miss.
 */
export function pickLeadingNumber(text, fallback = 0) {
  return pick(text, /^\s*\**\s*(\d+)\s*\**\s*(?=$|（|\()/, fallback);
}

export function extractNums(text) {
  const m = String(text ?? "").match(/\d[\d,]*(?:\.\d+)?/g);
  return m ? m.map((t) => parseFloat(t.replace(/,/g, ""))) : [];
}

/**
 * Load §12 of MASTER.md.
 * @returns {{ path: string, found: boolean, rows: Map<number, object>, warnings: string[] }}
 */
export function loadThresholds(repoRoot) {
  const path = join(repoRoot, "docs", "design", "MASTER.md");
  const warnings = [];
  const rows = new Map();
  if (!existsSync(path)) {
    warnings.push(
      `threshold table not found at ${path}; every check falls back to its built-in target`,
    );
    return { path, found: false, rows, warnings };
  }
  const md = readFileSync(path, "utf8");
  const start = md.indexOf("## 12");
  const end = start >= 0 ? md.indexOf("\n## 附录", start) : -1;
  let section = md;
  if (start < 0) {
    warnings.push("MASTER.md has no `## 12` section header; scanning the whole file");
  } else {
    section = md.slice(start, end > 0 ? end : undefined);
  }
  for (const line of section.split(/\r?\n/)) {
    if (!line.startsWith("|")) continue;
    const cells = line
      .replace(/^\|/, "")
      .replace(/\|\s*$/, "")
      .split("|")
      .map((c) => c.trim());
    if (cells.length < 5) continue;
    // The row number must be a BARE integer. parseInt("1.00") === 1, so §12.10.4's
    // alpha table (| 0.22 | … | 1.00 |) silently overwrote row 1 with the cell
    // "13.645" — a whole row's target replaced by a number from a different
    // table, with no warning anywhere.
    if (!/^\d+$/.test(cells[0])) continue;
    const n = Number(cells[0]);
    rows.set(n, {
      n,
      metric: cleanCell(cells[1]),
      baseline: cleanCell(cells[2]),
      target: cleanCell(cells[3]),
      judge: cleanCell(cells[4]),
      raw: line,
    });
  }
  if (!rows.size) warnings.push("§12 parsed but contained no numbered rows");
  return { path, found: rows.size > 0, rows, warnings };
}

/**
 * Resolve one row's judgeable target: the raw text plus the numbers pulled out
 * of the doc. `fallback` is used (and flagged) when the row is absent or its
 * target cell carries no number.
 */
export function targetFor(thresholds, n, fallback) {
  const row = thresholds.rows.get(n);
  const nums = row ? extractNums(row.target) : [];
  const source = row && nums.length ? "MASTER.md" : "builtin";
  return {
    row: n,
    text: row?.target ?? "(row missing)",
    nums,
    source,
    fallback,
  };
}
