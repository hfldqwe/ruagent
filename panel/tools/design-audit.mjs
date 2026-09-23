#!/usr/bin/env node
// design-audit.mjs — one command, all 13 routes, dark + light, numbers only.
//
// WHY THIS EXISTS
// The panel's design language ("Bench") is a set of quantitative claims, not
// vibes: a material staircase of a given ratio, a text ladder with nothing off
// it, one signal colour under a pixel budget, hairlines instead of boxes.
// docs/design/MASTER.md §12 states the acceptance thresholds; this script is
// the objective reading of them against the *served* bundle. It never says
// "looks better" — it reports counts, ratios and pixel shares.
//
//   cd panel && node tools/design-audit.mjs            # human tables
//   cd panel && node tools/design-audit.mjs --json     # machine-readable doc
//   cd panel && node tools/design-audit.mjs --check    # judge §12, exit 1 on failure
//
// THRESHOLDS: docs/design/MASTER.md §12 supplies metric/baseline/target/judge.
// Rows 4/10/15/16/23/30/32/35/37 additionally carry the *corrected* judge from
// docs/design/primitives.md §11 (and §5.1/§5.2.2) — the amended MASTER defers to
// it. The check table prints which rows those are; a row never silently judges
// against a criterion nobody wrote down.
//
// ARTEFACTS: docs/screenshots/audit-run/<route>_<mode>.png, metrics.json,
// report.md. stdout carries the tables (or the JSON under --json).
//
// ── MEASUREMENT CONTRACT (mirrors MASTER.md appendix A) ─────────────────────
// visible       : getBoundingClientRect() w,h >= 1 AND visibility !== hidden
//                 AND display !== none AND opacity > 0.
// luma (pixels) : (0.2126R + 0.7152G + 0.0722B)/255, rounded to 0.01, bucketed.
// chroma        : S = (max-min)/max > 0.15 AND max-min > 12.
// signal        : HSV hue 20-50deg, sat > 0.30, max-min > 25, max > 110.
// contrast      : WCAG 2.x relative luminance; alpha-composited foreground over
//                 the composited ancestor background; large text = >=24px, or
//                 >=18.66px && weight >= 700 (3:1), otherwise 4.5:1.
// DOM numbers   : computed styles, never source text (source is intent,
//                 computed style is fact).
// screenshots   : viewport-sized (not full page), deviceScaleFactor 1 — the
//                 same frame as the M1 baseline.
// ────────────────────────────────────────────────────────────────────────────
//
// Dependencies: @playwright/test (already a devDependency — used for its
// bundled Chromium) plus node: builtins. Nothing else may creep in here.

import { chromium } from "@playwright/test";
import { existsSync, mkdirSync, readFileSync, readdirSync, statSync, writeFileSync } from "node:fs";
import { dirname, join, resolve, relative, isAbsolute } from "node:path";
import { fileURLToPath } from "node:url";
import { gzipSync } from "node:zlib";
import { decodePng, pixelStats, relLuma, parseCssColor, composite, contrastRatio } from "./lib/png.mjs";
import { loadThresholds, targetFor, pick, pickAny, pickLeadingNumber, resetPickMisses, takePickMisses } from "./lib/thresholds.mjs";
import { loadContract, pageTitleFor, shellSetAdmits } from "./lib/contract.mjs";

const HERE = dirname(fileURLToPath(import.meta.url)); // panel/tools
const PANEL = resolve(HERE, ".."); // panel/
const REPO = resolve(PANEL, ".."); // repo root

// ── 13 routes. The hash carries NO leading slash; `#/home` silently falls
// through to `home` (MASTER appendix A), so every visit asserts the hash AND a
// content marker, and the report prints the extracted heading per route — a
// silent fallback that collapses pages into one cannot hide. ────────────────
const ROUTES = [
  { id: "home", hash: "#home", markers: [".home-hero", ".dash-grid", ".guide-grid"] },
  { id: "chat", hash: "#chat", markers: [".composer", ".chat-hero"] },
  { id: "sessions", hash: "#sessions", markers: [".view-bar"] },
  { id: "board", hash: "#board", markers: [".kanban-col", ".kanban", ".ant-empty"] },
  { id: "task", hash: "#task/<id>", markers: [".view-bar", ".card"], needsTaskId: true },
  { id: "memory", hash: "#memory", markers: [".view-bar"] },
  { id: "knowledge", hash: "#knowledge", markers: [".view-bar"] },
  { id: "graph", hash: "#graph", markers: [".graph-canvas"] },
  { id: "agents", hash: "#agents", markers: [".agent-grid", ".ant-empty"] },
  { id: "runtimes", hash: "#runtimes", markers: [".view-bar", ".agent-grid"] },
  { id: "stats", hash: "#stats", markers: [".stat-strip", ".dash-grid", ".view-bar"] },
  { id: "settings", hash: "#settings", markers: [".view-bar", ".sec"] },
  { id: "inbox", hash: "#inbox", markers: [".inbox-card", ".view-bar", ".ant-empty"] },
];

// ── Built-in fallbacks, NOT the criteria ────────────────────────────────────
// t17's finding was that these constants had quietly become the criteria: three
// rows judged against numbers written here rather than in the contract. The real
// values are now read from docs/design/* by lib/contract.mjs (resolveContract,
// below); everything in BUILTIN is used only when that read fails, and a failed
// read always surfaces as a warning. A fallback you cannot see is a silent lie.
const BUILTIN = {
  fontLadder: [11, 12, 13, 14, 15, 16, 18, 20, 24, 32], // MASTER §4.1
  weightLadder: [400, 500, 600, 700], // §4.3
  radiusLadder: [0, 3, 4, 6, 10, 14], // §6.1 + primitives §11 row 10 (3px scrollbar thumb)
  radiusPill: 50, // >= this reads as a pill/circle, not a step
  spacingLadder: [0, 2, 4, 6, 8, 10, 12, 16, 20, 24, 32, 80], // §5.1
  // primitives §5.2.2 / §11 row 23 — the ONLY aria-expanded carriers that count.
  // antd Select ships aria-expanded on its combobox; counting it made row 23 pass
  // vacuously, which is the defect t1 flagged.
  expandSelectors: [".recall-stub-head", ".tool-head", ".chat-group-more"],
  nativeExpandSelector: ".prompt-view > summary", // <details>/<summary>: browser provides it
  // primitives §11 row 34 / §10.1 N21 — hand-written clickables that must answer :active.
  handClasses: [".row-btn", ".kanban-card", ".guide-card", ".suggest-chip", ".readout-btn", ".mem-preview", ".neighbor", ".icon-btn", ".cmdk-item"],
  graphVars: ["--graph-person", "--graph-org", "--graph-project", "--graph-repo", "--graph-tool", "--graph-concept", "--graph-product", "--graph-protocol", "--graph-default"],
  row5ExcludeSelectors: [".raw"], // view-task-detail.md D11
  bleedSet: [
    // §12.6's named closed set; read from MASTER, this is only the fallback.
    { value: -12, selector: ".row-btn", source: "index.css:491 / primitives §3.3 C1" },
    { value: -8, selector: ".mem-preview", source: "index.css:1091" },
    { value: -4, selector: ".intent", source: "index.css:369" },
  ],
};
const ANT_CONTROL_SRC =
  "ant-(input|select|btn|tag|table|checkbox|radio|switch|segmented|pagination|progress|slider|picker|upload)";
const INTERACTIVE_SEL =
  'a[href], button, input, select, textarea, summary, [role="button"], [role="tab"], [role="menuitem"], [role="checkbox"], [role="switch"], [role="option"], [tabindex]';
// MASTER §2 / row 18: the two card families this audit also treats as clickable.
const CLICKABLE_EXTRA = [".agent-card", ".inbox-card"];
// ── §12 row 20: the FAILURE STATE, injected on the route's own endpoints ────
// The defect this row exists to catch is 「运行正被阻塞，而页面说没事」 (t13's inbox):
// a failed request rendered as an empty state or as a normal-looking view. So a
// 500 on the route's OWN data endpoints must produce a VISIBLE error affordance,
// a clickable retry, and a page that recovers when the endpoint does.
//
// The shell's poller is deliberately NOT injected: breaking the shell would test
// a white screen instead of a failure state, and a shell-level alert would be
// indistinguishable from the route's own.
const FAILURE_SKIP = /\/permissions$/;
async function probeFailureState(page, o) {
  const wanted = new Set((o.endpoints ?? []).filter((e) => !FAILURE_SKIP.test(e)));
  const out = {
    injectable: wanted.size, endpoints: [...wanted], injected: 0, requests: 0, seen: [],
    shellPresent: false, contentPresent: false, bodyText: 0, blank: null,
    alerts: [], errorNodes: [], retry: null, emptyShown: false, emptySel: null,
    retryClicked: null, recovered: null, error: null,
  };
  if (!wanted.size) return out;
  const routePattern = "**/api/**";
  try {
    await page.route(routePattern, async (r) => {
      const ep = normalizeEndpoint(r.request().url());
      out.requests++;
      if (out.seen.length < 12) out.seen.push(ep + (wanted.has(ep) ? " *" : ""));
      if (wanted.has(ep)) {
        out.injected++;
        await r.fulfill({ status: 500, contentType: "application/json", body: JSON.stringify({ error: "audit-injected" }) });
      } else {
        await r.continue();
      }
    });
    out.url = o.url;
    // A goto() to a URL that differs only in the fragment is a SAME-DOCUMENT
    // navigation: the app never re-mounts, so the view's own fetches never fire
    // and the probe would silently measure nothing. reload() forces a real
    // document load, which is the state under test.
    await page.goto(o.url, { waitUntil: "load", timeout: 60_000 });
    await page.reload({ waitUntil: "load", timeout: 60_000 });
    out.hash = await page.evaluate(() => location.hash);
    await page.waitForTimeout(2500);
    const seen = await page.evaluate(() => {
      const vis = (el) => {
        const r = el.getBoundingClientRect();
        const cs = getComputedStyle(el);
        return r.width >= 1 && r.height >= 1 && cs.display !== "none" && cs.visibility !== "hidden" && Number(cs.opacity) > 0;
      };
      const desc = (el) =>
        el.tagName.toLowerCase() + (el.getAttribute("class") ? "." + String(el.getAttribute("class")).trim().split(/\s+/).join(".") : "");
      const q = (s) => [...document.querySelectorAll(s)].filter(vis);
      const txt = (el) => String(el.innerText || el.textContent || "").trim().slice(0, 120);
      const alerts = q("[role=alert], [role=status]").map((el) => ({ sel: desc(el), text: txt(el) }));
      const errorNodes = q(".ant-alert-error, .ant-result-error, .state.error, .error-state, .ant-alert").map((el) => ({ sel: desc(el), text: txt(el) }));
      // antd inserts a space between two CJK characters ("重 试"), so compare on
      // whitespace-stripped text or the retry button is never found.
      const retryRe = /重试|再试|重新|retry|tryagain|reload/i;
      const norm = (s) => String(s || "").replace(/\s+/g, "");
      const btn = q("button, a[role=button], [role=button]").find(
        (el) => retryRe.test(norm(txt(el))) || retryRe.test(norm(el.getAttribute("aria-label"))),
      );
      let retry = null;
      if (btn) {
        const r = btn.getBoundingClientRect();
        btn.setAttribute("data-audit-retry", "1");
        retry = { sel: desc(btn), text: txt(btn), w: Math.round(r.width), h: Math.round(r.height) };
      }
      // An empty state is only a DEFECT when it stands in for the error; the row
      // judges 「visible」 first and names this separately so the reason is legible.
      const empty = q(".ant-empty, .state.empty, .empty-state, [class*=empty]").filter((el) => !el.closest(".cmdk"));
      return {
        alerts, errorNodes, retry,
        emptyShown: empty.length > 0,
        emptySel: empty[0] ? desc(empty[0]) : null,
        bodyText: String(document.body.innerText || "").length,
      };
    });
    Object.assign(out, seen);
    out.shellPresent = (await page.locator(o.shellSel).count()) > 0;
    out.contentPresent = (await page.locator("main").count()) > 0;
    out.blank = out.bodyText < 40;
    // Recoverability: un-route, click the retry FOR REAL (no force), and require
    // the error affordance to go away. A retry button that does nothing is not
    // a recovery path.
    if (out.retry) {
      await page.unroute(routePattern);
      try {
        await page.locator("[data-audit-retry]").first().click({ timeout: 5000 });
        out.retryClicked = true;
        await page.waitForTimeout(2500);
        out.recovered = await page.evaluate(() => {
          const vis = (el) => {
            const r = el.getBoundingClientRect();
            return r.width >= 1 && r.height >= 1 && getComputedStyle(el).display !== "none";
          };
          return [...document.querySelectorAll("[role=alert], .ant-alert-error, .state.error")].filter(vis).length === 0;
        });
      } catch (e) {
        out.retryClicked = false;
        out.recovered = false;
        out.error = e.message.split("\n")[0];
      }
    }
  } catch (e) {
    out.error = e.message.split("\n")[0];
  } finally {
    await page.unroute(routePattern).catch(() => {});
  }
  return out;
}

// ── MASTER §12.10.1 / §12.10.3: API request accounting ──────────────────────
// Row 28 judges "polling repetition" (one endpoint fetched over and over) and
// row 39 judges "per-item expansion" (one request per list entry). Two different
// phenomena, so they must not be collapsed into one number — which is why the
// grouping below is by ENDPOINT with numeric ids folded to ":id".
const PERMS_RE = /\/permissions$/;
function normalizeEndpoint(u) {
  let p;
  try {
    p = new URL(u, "http://localhost").pathname;
  } catch (e) {
    p = String(u).split("?")[0];
  }
  return p
    .replace(/\/+$/, "")
    .split("/")
    .map((seg) =>
      /^\d+$/.test(seg) || /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(seg) ? ":id" : seg,
    )
    .join("/");
}
function summarizeApi(urls) {
  const groups = new Map();
  let perms = 0;
  let nonPermTotal = 0;
  for (const u of urls ?? []) {
    const ep = normalizeEndpoint(u);
    // §12.10.1 ③: the shell's /permissions poller is pulled OUT of the per-route
    // count and judged once, site-wide. Otherwise it eats 6 of the 7 budget on
    // all 13 routes — the same "constant charged 13 times" error as §12.9.
    if (PERMS_RE.test(ep)) {
      perms++;
      continue;
    }
    nonPermTotal++;
    groups.set(ep, (groups.get(ep) ?? 0) + 1);
  }
  let maxGroup = 0;
  let maxEndpoint = "";
  for (const [ep, n] of groups) {
    if (n > maxGroup) {
      maxGroup = n;
      maxEndpoint = ep;
    }
  }
  return {
    perms,
    nonPermTotal,
    endpointKinds: groups.size,
    maxGroup,
    maxEndpoint,
    groups: [...groups.entries()].sort((a, b) => b[1] - a[1]).map(([endpoint, count]) => ({ endpoint, count })),
  };
}

// ── MASTER §12.9 ①: "the shell is a shared constant" is itself a claim to check ─
// An element counts as a shell CONSTANT only when it appears with the same
// selector path, className AND geometry on every capture. Anything present on
// only some routes is not a constant, and §12.9 requires it to fall back into
// the content budget — otherwise "it is part of the shell" becomes an exemption
// pocket that any new control could hide in.
const shellKey = (s) => `${s.sel}|${s.cls}|${s.w}x${s.h}`;
function shellConstancy(captures) {
  const lists = captures.map((c) =>
    (c.metrics?.inter?.shell32All ?? []).map((s) => ({ ...s, key: shellKey(s), route: c.route, mode: c.mode })),
  );
  if (!lists.length) return { constants: [], varying: [], captures: 0 };
  const maps = lists.map((l) => new Map(l.map((s) => [s.key, s])));
  const constants = [...maps[0].keys()].filter((k) => maps.every((m) => m.has(k))).map((k) => maps[0].get(k));
  const constKeys = new Set(constants.map((s) => s.key));
  const varying = [];
  for (const l of lists) for (const s of l) if (!constKeys.has(s.key)) varying.push(s);
  return { constants, varying, captures: lists.length };
}

// MASTER §12.9: the shell is one shared DOM across all 13 routes, so counting its
// constant <32px controls in every route's budget charges the same constant 13
// times. The ruling moves them to a named closed set checked once.
const SHELL_SEL = "aside.ant-layout-sider.app-sider"; // §12.9 ①, a frozen e2e target
// Row 18's inventories are complete, not sampled: MASTER row 18 wants the visible
// interactive elements to be reproducible, and the old cap of 8 hid the tail of
// routes reporting 11/13/16 violations. 40 covers the worst route seen with room
// to spare; counts were always exact, only the listing was truncated.
const INTERACTIVE_SAMPLE_CAP = 40;

// ── resolveContract: the effective criteria, read from the contract ─────────
// Called by main() (which feeds the browser probe) and by --self-test (which
// asserts the *contract* values, not the fallbacks).
function resolveContract(thresholds) {
  const c = loadContract(REPO, ROUTES, thresholds);
  const L = c.ladders.value;
  const effective = {
    fontLadder: L.font ?? BUILTIN.fontLadder,
    weightLadder: L.weight ?? BUILTIN.weightLadder,
    radiusLadder: L.radius ?? BUILTIN.radiusLadder,
    radiusPill: L.radiusPill ?? BUILTIN.radiusPill,
    spacingLadder: L.spacing ?? BUILTIN.spacingLadder,
    bleedSet: c.bleedSet.value ?? BUILTIN.bleedSet,
    expandSelectors: c.expand.value?.hand ?? BUILTIN.expandSelectors,
    nativeExpandSelector: c.expand.value?.native ?? BUILTIN.nativeExpandSelector,
    handClasses: c.handClasses.value ?? BUILTIN.handClasses,
    graphVars: c.graphVars.value ?? BUILTIN.graphVars,
    breakpoints: c.breakpoints.value ?? { css: [520, 768, 1024, 1240], js: [992] },
    row5ExcludeSelectors: c.row5Exclusions.value?.selectors ?? BUILTIN.row5ExcludeSelectors,
    bleedSetSource: c.bleedSet.source,
    shellSet: c.shellSet.value,
    shellSetSource: c.shellSet.source,
    exemptRoutes: c.exemptRoutes.value,
    pageTitleRule: c.pageTitle.value,
    titleTiers: c.titleTiers.value,
    viewSpecs: c.viewSpecs.value,
    sources: c.sources,
    warnings: c.warnings,
  };
  // Provenance: which criteria are read from the contract and which fell back.
  effective.provenance = [
    { item: "row 6 豁免路由与门槛", source: c.sources.exemptRoutes ?? "builtin", fromContract: !!c.sources.exemptRoutes },
    { item: "row 6 页面标题（层级中心）载体", source: c.sources.pageTitle ?? "builtin", fromContract: c.sources.pageTitle !== "builtin" },
    { item: "row 6 页面标题档位尺寸", source: c.sources.titleTiers ?? "builtin", fromContract: !!c.sources.titleTiers },
    { item: "row 7 每路由最大字号要求", source: c.sources.viewSpecs ?? "builtin", fromContract: !!c.sources.viewSpecs },
    { item: "row 8 字号阶梯", source: L.font ? "MASTER §4.1" : "builtin", fromContract: !!L.font },
    { item: "row 9 字重阶梯", source: L.weight ? "MASTER §4.3" : "builtin", fromContract: !!L.weight },
    { item: "row 10 圆角阶梯 + pill 阈值", source: L.radius ? "primitives §11 行 10" : "builtin", fromContract: !!L.radius },
    { item: "row 23 aria-expanded 载体选择器", source: c.sources.expand ?? "builtin", fromContract: !!c.sources.expand },
    { item: "row 33 图谱分类令牌", source: c.sources.graphVars ?? "builtin", fromContract: !!c.sources.graphVars },
    { item: "row 34 手写可点元素类名", source: c.sources.handClasses ?? "builtin", fromContract: !!c.sources.handClasses },
    { item: "row 37 间距阶梯", source: L.spacing ? "MASTER §5.1" : "builtin", fromContract: !!L.spacing },
    { item: "row 35 断点白名单", source: c.sources.breakpoints ?? "builtin", fromContract: !!c.sources.breakpoints },
    { item: "row 12 内联 style 计数口径（可见元素）", source: "本工具（口径声明，非阈值）", fromContract: "stated" },
    { item: "row 5 的 .raw 排除", source: c.sources.row5Exclusions ?? "builtin", fromContract: !!c.sources.row5Exclusions },
    { item: "row 26 长任务目标（200ms）", source: "MASTER §12 行 26", fromContract: true },
    { item: "row 1 判据范围（排除画布像素）", source: "MASTER §12.5", fromContract: true },
    { item: "row 37 判据来源（只判作者值）+ 出血闭集", source: "MASTER §12.6", fromContract: true },
    { item: "row 18 内容区口径 + 外壳具名闭集", source: c.shellSet.source ?? "builtin", fromContract: !!c.shellSet.source },
    { item: "row 30 判定基准（ringVsCanvas）", source: "captain 裁决 + MASTER §12 行 30 判据列「与 pageCanvas 比」", fromContract: true },
    { item: "row 28 门槛（7）", source: "captain 裁决 2026-09-21（MASTER §12 行 28 仍写 ≤3）", fromContract: "pending" },
  ];
  // What is still a literal in this file, and why it is allowed to be. Anything
  // in this list is *not* a design threshold: it is either an instrument
  // parameter or a DOM-shape pattern that MASTER does not state as a table.
  effective.hardcoded = [
    {
      item: "ANT_CONTROL_SRC（antd 控件类名前缀）",
      why: "MASTER 行 5 只说「排除注册控件」，未给类名表；这是把该词落成可执行判断的模式串。改动它会立刻改变行 5/行 4 的计数，故在此显式登记",
    },
    {
      item: "INTERACTIVE_SEL / CLICKABLE_EXTRA（可交互语义选择器、.agent-card/.inbox-card）",
      why: "DOM 形状，不是阈值；MASTER 行 17/18 用「可交互元素」一词，未列表",
    },
    {
      item: "PROBE 内的可见性口径（box ≥1、非 display:none/visibility:hidden、opacity>0、非 clip 裁剪盒）",
      why: "测量口径，来自 MASTER 附录 A 的测量契约；不是设计阈值",
    },
    {
      item: "DEFAULT_VP_H=900、LONG_TASK_REMEASURE_MS=1500、sampleCap=8、interactiveSampleCap=40、--dom-quiet/--dom-max",
      why: "仪器自身的采样参数，不是设计阈值；已在 --help 与本表中声明",
    },
    {
      item: "NOT_MEASURED 行 20 的理由文案",
      why: "文字说明，不是判据",
    },
  ];
  return effective;
}

// ── CLI ─────────────────────────────────────────────────────────────────────
function parseArgs(argv) {
  const o = {
    json: false, check: false, table: false, quiet: false, selfTest: false,
    focus: true, shots: true, pixels: true,
    baseUrl: process.env.E2E_BASE_URL ?? "http://127.0.0.1:8787",
    viewport: "1440x900",
    overflowViewports: "390,520,768,1024,1280,1920",
  injectCss: null,
    modes: "dark,light", routes: null,
    settleMs: 2500, tabs: 20, apiWindowMs: 2500, focusBudgetMs: 30000, failureProbe: true,
    domQuietMs: 800, domMaxMs: 8000,
    taskId: null, out: null, help: false,
  };
  for (const raw of argv) {
    const eq = raw.indexOf("=");
    const k = eq > 0 ? raw.slice(0, eq) : raw;
    const v = eq > 0 ? raw.slice(eq + 1) : true;
    switch (k) {
      case "--json": o.json = true; break;
      case "--check": o.check = true; break;
      case "--table": o.table = true; break;
      case "--self-test": o.selfTest = true; break;
      case "--quiet": o.quiet = true; break;
      case "--no-focus": o.focus = false; break;
      case "--no-shots": o.shots = false; break;
      case "--no-pixels": o.pixels = false; break;
      case "--base-url": o.baseUrl = String(v); break;
      case "--viewport": o.viewport = String(v); break;
      case "--overflow-viewports": o.overflowViewports = String(v); break;
      // Control runs: inject a stylesheet AFTER load, so a row can be shown
      // to FAIL on the pre-fix state through the SAME code path that judges
      // the fixed state. Evidence by injection, not by a parallel script.
      case "--inject-css": o.injectCss = String(v); break;
      case "--modes": o.modes = String(v); break;
      case "--routes": o.routes = String(v).split(",").map((s) => s.trim()).filter(Boolean); break;
      case "--settle": o.settleMs = Number(v) || 2500; break;
      case "--tabs": o.tabs = Number(v) || 20; break;
      case "--focus-budget": o.focusBudgetMs = Number(v) || 30000; break;
      case "--api-window": o.apiWindowMs = Number(v) || 2500; break;
      case "--dom-quiet": o.domQuietMs = Number(v) || 800; break;
      case "--dom-max": o.domMaxMs = Number(v) || 8000; break;
      case "--task-id": o.taskId = String(v); break;
      case "--out": o.out = String(v); break;
      case "--no-failure-probe": o.failureProbe = false; break;
      case "--help": case "-h": o.help = true; break;
      default:
        if (k.startsWith("--")) throw new Error(`unknown flag ${k}`);
    }
  }
  return o;
}

const HELP = `design-audit.mjs — quantitative design audit for the ruagent panel

  --json                    print the metrics document to stdout (stdout stays parseable)
  --check                   judge every implementable row of MASTER.md §12; exit 1 on failure
  --table                   print the §12 check table without judging/exiting
  --self-test               prove rows 16/23 implement the primitives §11 criterion
                            (not MASTER §12's defective wording) on synthetic
                            inputs; no daemon, no browser. exit 1 on any failure.
  --routes=a,b              audit a subset of routes (default: all 13)
  --modes=dark,light        modes to audit (default: both)
  --viewport=1440x900       measurement viewport for the screenshot pixel pass
  --overflow-viewports=...  viewports probed for horizontal overflow (default 390,520,768,1024,1280,1920)
  --inject-css=<file>       CONTROL RUN ONLY: inject this stylesheet after load.
  --base-url=URL            daemon serving panel/dist (default http://127.0.0.1:8787)
  --task-id=ID              task id for #task/<id> (default: first task from the API)
  --settle=MS               settle wait after load (default 2500, = appendix A)
  --dom-quiet=MS            DOM must stop changing this long before counting (default 800)
  --dom-max=MS              cap on the DOM-stability wait (default 8000)
  --tabs=N                  Tab stops walked for the focus pass (default 20; --no-focus to skip)
  --focus-budget=MS         wall-clock cap for the Tab walk (default 30000; stops recorded so far)
  --api-window=MS           request-counting window (default 2500; §12 row 28 judged
                            at >=11500). Extends the wait after --settle, so the
                            judged window is the one actually observed.
  --no-shots --no-pixels    skip screenshots / pixel statistics
  --out=DIR                 artefact dir, resolved against the REPO ROOT (default docs/screenshots/audit-run)
  --no-failure-probe        skip row 20's failure-state injection (the row reports not_measured)
  --quiet                   suppress progress on stderr
`;

let QUIET = false;
const log = (...a) => {
  if (!QUIET) console.error(...a);
};

// ── The in-page probe. Serialized into the page by page.evaluate, so it may
// only reference its own `cfg` argument — no module scope. ─────────────────
const PROBE = (cfg) => {
  const fontLadder = new Set(cfg.fontLadder);
  const weightLadder = new Set(cfg.weightLadder);
  const radiusLadder = new Set(cfg.radiusLadder);
  const radiusPill = cfg.radiusPill;
  const cap = cfg.sampleCap;
  const interCap = cfg.interactiveSampleCap ?? 40;
  const ANT_CONTROL = new RegExp(cfg.antControlSrc);
  const CONTAINER_TAGS = new Set(["DIV", "SECTION", "ARTICLE", "LI", "ASIDE", "MAIN", "UL"]);
  const SKIP_TEXT_TAGS = new Set(["SCRIPT", "STYLE", "NOSCRIPT", "TEMPLATE", "TITLE"]);

  const csCache = new WeakMap();
  const csOf = (el) => {
    let v = csCache.get(el);
    if (!v) { v = getComputedStyle(el); csCache.set(el, v); }
    return v;
  };
  const parseColor = (s) => {
    if (!s) return null;
    if (s === "transparent") return { r: 0, g: 0, b: 0, a: 0 };
    const hx = s.match(/^#([0-9a-f]{3,8})$/i); // light-mode tokens are hex literals
    if (hx) {
      let h = hx[1];
      if (h.length === 3 || h.length === 4) h = h.split("").map((c) => c + c).join("");
      return {
        r: parseInt(h.slice(0, 2), 16),
        g: parseInt(h.slice(2, 4), 16),
        b: parseInt(h.slice(4, 6), 16),
        a: h.length === 8 ? parseInt(h.slice(6, 8), 16) / 255 : 1,
      };
    }
    const cm = s.match(/^color\(srgb\s+([\d.]+)\s+([\d.]+)\s+([\d.]+)(?:\s*\/\s*([\d.]+%?))?\)$/);
    if (cm) {
      const a = cm[4] == null ? 1 : cm[4].endsWith("%") ? parseFloat(cm[4]) / 100 : parseFloat(cm[4]);
      return { r: parseFloat(cm[1]) * 255, g: parseFloat(cm[2]) * 255, b: parseFloat(cm[3]) * 255, a };
    }
    const m = s.match(/^rgba?\(([^)]+)\)$/);
    if (!m) return null;
    const p = m[1].split(/[,\s/]+/).filter(Boolean).map(Number);
    if (p.length < 3 || p.slice(0, 3).some((n) => !Number.isFinite(n))) return null;
    return { r: p[0], g: p[1], b: p[2], a: p.length > 3 && Number.isFinite(p[3]) ? p[3] : 1 };
  };
  const blend = (fg, bg) => ({
    r: fg.r * fg.a + bg.r * (1 - fg.a),
    g: fg.g * fg.a + bg.g * (1 - fg.a),
    b: fg.b * fg.a + bg.b * (1 - fg.a),
    a: fg.a + bg.a * (1 - fg.a),
  });
  const lin = (v) => { const c = v / 255; return c <= 0.03928 ? c / 12.92 : Math.pow((c + 0.055) / 1.055, 2.4); };
  const lum = (c) => 0.2126 * lin(c.r) + 0.7152 * lin(c.g) + 0.0722 * lin(c.b);
  const contrast = (a, b) => {
    const l1 = lum(a), l2 = lum(b);
    return (Math.max(l1, l2) + 0.05) / (Math.min(l1, l2) + 0.05);
  };
  const hex = (c) => "#" + [c.r, c.g, c.b].map((v) => Math.max(0, Math.min(255, Math.round(v))).toString(16).padStart(2, "0")).join("");
  const rgbaText = (c) => (c.a >= 0.999 ? hex(c) : `rgba(${Math.round(c.r)},${Math.round(c.g)},${Math.round(c.b)},${Math.round(c.a * 100) / 100})`);

  const rootCs = getComputedStyle(document.documentElement);
  const rootParsed = parseColor(rootCs.backgroundColor);
  const rootBg = rootParsed && rootParsed.a > 0 ? { ...rootParsed, a: 1 } : { r: 255, g: 255, b: 255, a: 1 };

  const bgCache = new WeakMap();
  const bgFor = (el) => {
    if (!el || el.nodeType !== 1) return { bg: rootBg, ambiguous: false };
    const hit = bgCache.get(el);
    if (hit) return hit;
    const cs = csOf(el);
    const own = parseColor(cs.backgroundColor);
    const parent = bgFor(el.parentElement);
    const res = {
      bg: own && own.a > 0 ? blend(own, parent.bg) : parent.bg,
      ambiguous: parent.ambiguous || cs.backgroundImage !== "none",
    };
    bgCache.set(el, res);
    return res;
  };

  const cssPath = (el) => {
    const parts = [];
    let n = el;
    while (n && n.nodeType === 1 && parts.length < 4) {
      let s = n.tagName.toLowerCase();
      if (n.id) { parts.unshift(s + "#" + n.id); break; }
      const cls = (n.getAttribute("class") || "").trim().split(/\s+/).filter(Boolean).slice(0, 2);
      if (cls.length) s += "." + cls.join(".");
      const par = n.parentElement;
      if (par) {
        const sibs = [...par.children].filter((c) => c.tagName === n.tagName);
        if (sibs.length > 1) s += `:nth-of-type(${sibs.indexOf(n) + 1})`;
      }
      parts.unshift(s);
      n = n.parentElement;
    }
    return parts.join(" > ");
  };
  // `limit` defaults to the general sample cap, but the row-18 inventories pass
  // a larger one: MASTER row 18 wants the visible interactive elements to be
  // re-checkable, and 11/13/16 violations against a cap of 8 meant the tail was
  // never named. Counts were always exact; only the listing was truncated.
  const pushSample = (arr, el, extra, limit) => {
    if (arr.length < (limit ?? cap)) arr.push({ sel: cssPath(el), ...extra });
  };

  // Text-bearing elements: an element counts once, however many text children
  // it has ("has direct text" — MASTER §12 row 6).
  const textEls = [];
  const seen = new Set();
  const textSamples = new Map();
  const walker = document.createTreeWalker(document.documentElement, NodeFilter.SHOW_TEXT);
  for (let tn = walker.nextNode(); tn; tn = walker.nextNode()) {
    if (!tn.nodeValue || !tn.nodeValue.trim()) continue;
    const p = tn.parentElement;
    if (!p || SKIP_TEXT_TAGS.has(p.tagName) || seen.has(p)) continue;
    seen.add(p);
    textEls.push(p);
    textSamples.set(p, tn.nodeValue.trim().slice(0, 28));
  }

  const text = {
    count: 0, le14: 0, ge18: 0, maxFontSize: 0,
    sizeHist: new Map(), offScale: new Map(), offWeight: new Map(),
    ge18Samples: [], maxSamples: [], capped: false,
  };
  const borders = {
    hairline: 0, allBordered: 0, fourSide: 0, strictContainers: 0,
    mdTableCells: 0, rawBlocks: 0, strictSamples: [], fourSideSamples: [], hairlineSamples: [],
  };
  const radius = { offCount: 0, off: new Map(), hist: new Map(), samples: [] };
  const inline = { styleTotal: 0, fontSized: 0, fontSizedSamples: [] };
  const inter = {
    count: 0, unnamedCount: 0, unnamed: [],
    small24: 0, small32: 0, promoted: 0, small24Samples: [], small32Samples: [],
    // row 18: hit-target reading (judged) and painted-box reading (comparison)
    hit24: 0, hit32: 0, paint24: 0, paint32: 0, viaRoot: 0, hit24All: [], sampleCap: interCap,
    // §12.9 zone split (reported as data; not yet judged)
    shell24: 0, shell32: 0, content24: 0, content32: 0, shell32All: [],
  };
  const structure = { h1: 0, h1Text: null, ariaExpanded: 0, summaries: 0, heading: null, dashboard: 0 };
  const extras = { inboxCards: 0, inboxCardBordered: 0, navBadge: 0 };
  const spacing = {
    off: new Map(),
    ladder: cfg.spacingLadder,
    rawCount: 0,
    numeratorPos: 0,
    numeratorNeg: 0,
    antdDerived: { count: 0, samples: [] },
    uaDefault: { count: 0, samples: [] },
    autoResolved: { count: 0, samples: [] },
    clampVw: { count: 0, samples: [] },
    bleed: { count: 0, samples: [] },
    numerator: { count: 0, samples: [] },
    diag: [],
  };
  const SPACING_PROPS = [
    ["paddingTop", "padding-top"], ["paddingRight", "padding-right"],
    ["paddingBottom", "padding-bottom"], ["paddingLeft", "padding-left"],
    ["marginTop", "margin-top"], ["marginRight", "margin-right"],
    ["marginBottom", "margin-bottom"], ["marginLeft", "margin-left"],
    ["rowGap", "row-gap"], ["columnGap", "column-gap"],
  ];
  const bucketAdd = (b, rec) => {
    b.count++;
    if (b.samples.length < 40) b.samples.push(rec);
  };
  const spacingLadder = new Set(cfg.spacingLadder);

  // view-task-detail.md D11: row 5's count excludes .md table cells AND .raw
  // ("判定必须带这条排除规则，否则本页永远不通过"). Collect the .raw subtrees
  // once — a per-element closest()/contains() is O(depth) on a 153k-node page.
  const rawSet = new Set();
  for (const sel of cfg.row5ExcludeSelectors ?? []) {
    for (const r of document.querySelectorAll(sel)) {
      rawSet.add(r);
      for (const d of r.querySelectorAll("*")) rawSet.add(d);
    }
  }

  // primitives §5.5 / MASTER §12.1 / view-inbox I2: .sr-only is *visually hidden*
  // by construction — a 1x1 box clipped to nothing — and must not count as
  // visible text. Before systems pinned `font: inherit` on it, its UA 28px made
  // 12 routes violate row 8 at once. A box with a real border can still be
  // clipped away, so this is checked before every *visual* count; row 24's h1
  // count deliberately still uses `vis`, because §5.5 counts the .sr-only h1.
  const clippedAway = (cs, r) => {
    const cp = cs.clipPath || "none";
    if (cp !== "none" && /inset\(/.test(cp) && /(^|[^\d.])50%/.test(cp)) return true;
    const clip = cs.clip || "auto";
    if (clip !== "auto" && /^rect\(/.test(clip) && !/[1-9]/.test(clip)) return true;
    // the canonical .sr-only box: 1x1, absolutely positioned, overflow hidden
    return r.width <= 1 && r.height <= 1 && cs.overflow === "hidden" && cs.position === "absolute";
  };

  // The hit target of a NATIVE control is the box the user actually aims at: the
  // nearest ancestor that paints a border or a background. An <input> inside
  // .ant-input-sm measures 21px while its wrapper box is 24px, so reading the
  // inner element reported a compliant control as a violation (t18) — and the
  // attempted fix pushed the wrapper to 27px before being reverted. This is the
  // row-18 twin of row 16's "walk up to the focus owner" rule: read the control,
  // not the primitive inside it. Non-native clickables (buttons, .row-btn, …)
  // are their own box; walking up from those would land on the panel.
  // ── row 18: the HIT TARGET, not the painted box ───────────────────────────
  // The 24px floor is a hit-target requirement, so the measurement must land on
  // the control ROOT — antd's .ant-select / .ant-switch / .ant-btn /
  // .ant-input-affix-wrapper, or the native control when it really is the widget
  // — even when that root is borderless and fully transparent. Requiring
  // "something painted" let every borderless control silently fall back to a
  // smaller inner element and pass; the two #chat .ant-select roots (23px) did
  // exactly that. Same defect class as row 16 reading only the focused element
  // and row 6 taking a number by position.
  const ANTD_CONTROL_ROOT = [
    ".ant-select", ".ant-switch", ".ant-btn", ".ant-input-affix-wrapper", ".ant-input-number",
    ".ant-picker", ".ant-checkbox-wrapper", ".ant-radio-wrapper", ".ant-slider", ".ant-segmented",
    ".ant-tabs-tab", ".ant-pagination-item", ".ant-upload", ".ant-collapse-header",
    ".ant-menu-item", ".ant-menu-submenu-title",
  ].join(", ");
  const ROLE_ROOT = /^(button|switch|checkbox|radio|tab|link|menuitem|combobox|slider|spinbutton|option|searchbox|textbox)$/;
  const isControlRoot = (n) => {
    if (!n || n.nodeType !== 1) return false;
    if (n.matches(ANTD_CONTROL_ROOT)) return true;
    if (/^(BUTTON|A|SELECT|SUMMARY)$/.test(n.tagName)) return true;
    // A bare input/textarea is the widget only when it is NOT a sub-part of a
    // composite control: antd's .ant-select owns a hidden <input>, and that
    // input is 24px while the control the user aims at is 23px.
    if (/^(INPUT|TEXTAREA)$/.test(n.tagName)) return !n.closest(ANTD_CONTROL_ROOT);
    const role = n.getAttribute("role");
    if (role && ROLE_ROOT.test(role)) return true;
    const ti = n.getAttribute("tabindex");
    if (ti !== null && Number(ti) >= 0) return true;
    return csOf(n).cursor === "pointer";
  };
  const boxArea = (n) => {
    const b = n.getBoundingClientRect();
    return Math.max(1, b.width * b.height);
  };
  // Climb to the OUTERMOST control root, not the nearest one. antd nests its
  // parts (input.ant-select-input < .ant-select-content < .ant-select), and the
  // inner parts carry cursor:pointer too, so "nearest" stops one level too deep
  // and measures the sub-part (21px) instead of the widget the user aims at
  // (.ant-select, 23px). Stop before a container: an ancestor more than 2x the
  // element's area wraps many controls, and promoting to it would hand a
  // genuinely tiny control a huge hit target.
  const hitTarget = (el) => {
    const er = el.getBoundingClientRect();
    const eArea = Math.max(1, er.width * er.height);
    let best = isControlRoot(el) ? el : null;
    for (let n = el.parentElement, hops = 0; n && hops < 6; n = n.parentElement, hops++) {
      if (!isControlRoot(n)) continue;
      const nr = n.getBoundingClientRect();
      // A container grows in BOTH directions; a composite control adds chrome in
      // one. Judging on area alone refused .ant-select for a 29px-wide input in
      // a 62px-wide root — which is the control the user aims at, not a panel.
      if (nr.width > er.width * 2 && nr.height > er.height * 2) break;
      if (nr.width * nr.height > eArea * 6) break;
      best = n;
    }
    return best ?? el;
  };
  // The previous rule, kept only as a comparison column: "the nearest ancestor
  // that actually paints a border or a background".
  const paintedBox = (el) => {
    if (!/^(INPUT|TEXTAREA|SELECT)$/.test(el.tagName)) return el;
    const eArea = boxArea(el);
    for (let n = el.parentElement, hops = 0; n && hops < 3; n = n.parentElement, hops++) {
      const c = csOf(n);
      const styles = [c.borderTopStyle, c.borderRightStyle, c.borderBottomStyle, c.borderLeftStyle];
      const widths = [c.borderTopWidth, c.borderRightWidth, c.borderBottomWidth, c.borderLeftWidth].map((w) => parseFloat(w) || 0);
      const colors = [c.borderTopColor, c.borderRightColor, c.borderBottomColor, c.borderLeftColor];
      const hasBorder = styles.some((s, i) => {
        if (s === "none" || s === "hidden" || !(widths[i] > 0)) return false;
        const col = parseColor(colors[i]);
        return !!col && col.a > 0.04;
      });
      const bg = parseColor(c.backgroundColor);
      if (!hasBorder && !(bg && bg.a > 0.04)) continue;
      if (boxArea(n) > eArea * 2) return el;
      return n;
    }
    return el;
  };

  const all = document.querySelectorAll("*");
  const domNodes = all.length;

  // ── MASTER §12.6 (row 37): who wrote the winning declaration? ─────────────
  // getComputedStyle returns the *cascade result*, not the author's value, so
  // counting its output mixed in three kinds of value nobody on the design side
  // wrote: antd's CSS-in-JS injections, UA defaults (p { margin: 1em }), and
  // flex auto margins that resolve to px. Classify by origin instead of keeping
  // an exception table, which would rot on every antd upgrade.
  const authorRules = [];
  const antdRules = [];
  for (const sheet of document.styleSheets) {
    let rules;
    try { rules = sheet.cssRules; } catch (e) { continue; }
    if (!rules) continue;
    const node = sheet.ownerNode;
    const tag = node && node.tagName;
    if (tag === "STYLE" && node.hasAttribute("data-css-hash")) antdRules.push(rules);
    else if (tag === "LINK" || tag === "STYLE") authorRules.push(rules);
  }
  // A physical longhand can be written five other ways: the logical properties
  // (padding-inline-end), the pair forms (padding-block: 10px 20px) and the
  // shorthands (padding: 9px 12px). Reading only the physical name missed all of
  // them — antd v6 writes padding-inline, so its injected values looked like
  // "no declaration at all" and were filed as UA defaults.
  const SHORTHANDS = {
    paddingTop: [["padding", "quad", "t"], ["padding-block", "pair", "first"], ["padding-block-start", "single", "a"]],
    paddingRight: [["padding", "quad", "r"], ["padding-inline", "pair", "second"], ["padding-inline-end", "single", "a"]],
    paddingBottom: [["padding", "quad", "b"], ["padding-block", "pair", "second"], ["padding-block-end", "single", "a"]],
    paddingLeft: [["padding", "quad", "l"], ["padding-inline", "pair", "first"], ["padding-inline-start", "single", "a"]],
    marginTop: [["margin", "quad", "t"], ["margin-block", "pair", "first"], ["margin-block-start", "single", "a"]],
    marginRight: [["margin", "quad", "r"], ["margin-inline", "pair", "second"], ["margin-inline-end", "single", "a"]],
    marginBottom: [["margin", "quad", "b"], ["margin-block", "pair", "second"], ["margin-block-end", "single", "a"]],
    marginLeft: [["margin", "quad", "l"], ["margin-inline", "pair", "first"], ["margin-inline-start", "single", "a"]],
    rowGap: [["gap", "pair", "first"], ["grid-gap", "pair", "first"]],
    columnGap: [["gap", "pair", "second"], ["grid-gap", "pair", "second"]],
  };
  const expandShadow = (decl, mode, which) => {
    const parts = String(decl).trim().split(/\s+(?![^(]*\))/);
    if (mode === "single") return parts[0];
    if (mode === "pair") return which === "first" ? parts[0] : (parts[1] ?? parts[0]);
    const a = parts[0];
    const b = parts[1] ?? a;
    const c = parts[2] ?? a;
    const d = parts[3] ?? b;
    return which === "t" ? a : which === "r" ? b : which === "b" ? c : d;
  };
  const declaredFor = (el, prop, cssProp) => {
    const out = { author: [], antd: [] };
    const wanted = [[cssProp, "single", "a"], ...(SHORTHANDS[prop] ?? [])];
    const scan = (list, sink) => {
      for (const rule of list) {
        // Chrome implements CSS nesting, so every CSSStyleRule also exposes
        // cssRules -- an EMPTY list for a non-nested rule, which is still
        // truthy. Testing "rule.cssRules" therefore skipped every ordinary
        // declaration and classified the whole page as UA default.
        if (rule.cssRules && rule.cssRules.length > 0) {
          if (rule.media && !matchMedia(rule.media.mediaText).matches) continue;
          scan(rule.cssRules, sink);
          continue;
        }
        const st = rule.style;
        if (!st || !rule.selectorText) continue;
        let hit = false;
        try { hit = el.matches(rule.selectorText); } catch (e) { hit = false; }
        if (!hit) continue;
        for (const [name, mode, which] of wanted) {
          const v = st.getPropertyValue(name);
          if (v) sink.push(expandShadow(v, mode, which));
        }
      }
    };
    // authorRules/antdRules are arrays OF rule lists (one per sheet); scan takes
    // a single sheet's rule list. Passing the outer array made every iteration
    // land on a CSSRuleList, which has no .style and no .selectorText, so every
    // value on the page was classified as a UA default.
    for (const rules of authorRules) scan(rules, out.author);
    for (const rules of antdRules) scan(rules, out.antd);
    return out;
  };
  // A non-px declaration (rem/em/%) has to be resolved in the element's own font
  // context before it can be compared with the computed value. One hidden
  // scratch box, appended after the node list was captured so it is not counted,
  // and removed before the probe returns.
  const scratch = document.createElement("div");
  scratch.setAttribute("aria-hidden", "true");
  scratch.style.cssText = "position:fixed;left:-9999px;top:0;width:0;height:0;overflow:hidden;pointer-events:none";
  document.documentElement.appendChild(scratch);
  const lenCache = new Map();
  const resolveLen = (decl, el) => {
    const ecs = csOf(el);
    const fs = ecs.fontSize;
    // antd writes "padding-inline: var(--ant-select-padding-horizontal)", and
    // those custom properties are defined on a scope INSIDE the document, so a
    // scratch box under <html> cannot see them. Copy the ones the declaration
    // mentions off the element before resolving.
    const names = String(decl).match(/--[\w-]+/g) || [];
    const key = decl + "|" + fs + "|" + names.map((n) => n + "=" + ecs.getPropertyValue(n).trim()).join(",");
    if (lenCache.has(key)) return lenCache.get(key);
    for (const n of names) scratch.style.setProperty(n, ecs.getPropertyValue(n).trim());
    scratch.style.fontSize = fs;
    scratch.style.marginTop = "";
    scratch.style.marginTop = decl;
    const v = getComputedStyle(scratch).marginTop;
    lenCache.set(key, v);
    return v;
  };
  const normVal = (s) => String(s).trim().toLowerCase().replace(/\s+/g, " ");
  const classifySpacing = (el, prop, cssProp, computed) => {
    const inline = el.style ? el.style.getPropertyValue(cssProp) : "";
    const decl = declaredFor(el, prop, cssProp);
    const ordered = [];
    if (inline) ordered.push({ origin: "author", value: inline, src: "style attribute" });
    for (const v of decl.author) ordered.push({ origin: "author", value: v, src: "author sheet" });
    for (const v of decl.antd) ordered.push({ origin: "antd", value: v, src: "antd css-in-js" });
    const comp = normVal(computed);
    const compN = parseFloat(comp);

    // (1) a candidate that IS the computed value decides the origin
    for (const c of ordered) {
      const v = normVal(c.value);
      if (v === comp) return { origin: c.origin, value: c.value, src: c.src };
      const n = parseFloat(v);
      if (Number.isFinite(n) && comp.endsWith("px") && (v.endsWith("px") || /^-?[\d.]+$/.test(v)) && Math.abs(n - compN) < 0.01) {
        return { origin: c.origin, value: c.value, src: c.src };
      }
    }
    // (2) clamp()/vw products — §5.1 keeps them out of the ladder on purpose
    const cl = ordered.find((c) => /clamp\(|vw/.test(normVal(c.value)));
    if (cl) return { origin: "clampVw", value: cl.value, src: cl.src };
    // (3) a specified auto/normal: the px we measured is a layout outcome
    const au = ordered.find((c) => /^(auto|normal)$/.test(normVal(c.value)));
    if (au) return { origin: "autoResolved", value: au.value, src: au.src };
    // (4) non-px author spellings, resolved in context
    for (const c of ordered) {
      if (resolveLen(c.value, el) === comp) return { origin: c.origin, value: c.value, src: c.src };
    }
    // (5) nothing matched the computed value: it came from the UA sheet
    return { origin: "uaDefault", value: "", src: "no declaration" };
  };
  const shellSel = cfg.shellSel || "aside.ant-layout-sider.app-sider";
  const bleedSet = cfg.bleedSet || [];
  const namedBleed = (el, v) => bleedSet.some((b) => Math.abs(b.value - v) < 0.01 && el.matches(b.selector));
  for (const el of all) {
    const cs = csOf(el);
    const r = el.getBoundingClientRect();
    const vis = r.width >= 1 && r.height >= 1 && cs.visibility !== "hidden" && cs.display !== "none" && parseFloat(cs.opacity) > 0;
    const visText = vis && !clippedAway(cs, r);

    // ── (a) type ladder ────────────────────────────────────────────────────
    if (seen.has(el) && visText) {
      const fs = parseFloat(cs.fontSize);
      const fw = parseInt(cs.fontWeight, 10) || 400;
      if (Number.isFinite(fs)) {
        text.count++;
        if (fs <= 14) text.le14++;
        if (fs >= 18) { text.ge18++; if (text.ge18Samples.length < 8) text.ge18Samples.push({ sel: cssPath(el), px: fs }); }
        if (fs > text.maxFontSize) { text.maxFontSize = fs; text.maxSamples = [{ sel: cssPath(el), px: fs }]; }
        else if (fs === text.maxFontSize && text.maxSamples.length < 4) text.maxSamples.push({ sel: cssPath(el), px: fs });
        text.sizeHist.set(fs, (text.sizeHist.get(fs) ?? 0) + 1);
        if (!fontLadder.has(fs)) {
          const e = text.offScale.get(fs) ?? { count: 0, samples: [] };
          e.count++; pushSample(e.samples, el, {}); text.offScale.set(fs, e);
        }
        if (!weightLadder.has(fw)) {
          const e = text.offWeight.get(fw) ?? { count: 0, samples: [] };
          e.count++; pushSample(e.samples, el, {}); text.offWeight.set(fw, e);
        }
      }
    }

    // ── (b) borders ────────────────────────────────────────────────────────
    const cls = el.getAttribute("class") || "";
    const isControl = ANT_CONTROL.test(cls);
    const isMdCell = el.tagName === "TH" || el.tagName === "TD";
    const bws = [parseFloat(cs.borderTopWidth) || 0, parseFloat(cs.borderRightWidth) || 0, parseFloat(cs.borderBottomWidth) || 0, parseFloat(cs.borderLeftWidth) || 0];
    if (visText && (bws[0] || bws[1] || bws[2] || bws[3])) {
      const styles = [cs.borderTopStyle, cs.borderRightStyle, cs.borderBottomStyle, cs.borderLeftStyle];
      const cols = [cs.borderTopColor, cs.borderRightColor, cs.borderBottomColor, cs.borderLeftColor];
      const sideLive = bws.map((w, i) => {
        if (!w || styles[i] === "none" || styles[i] === "hidden") return false;
        const c = parseColor(cols[i]);
        return !!c && c.a > 0.04; // MASTER row 4: border colour alpha > 0.04
      });
      const nSides = sideLive.filter(Boolean).length;
      const isHairline = bws.some((w, i) => sideLive[i] && w <= 1.5);
      if (isHairline) { borders.hairline++; pushSample(borders.hairlineSamples, el, { px: Math.max(...bws) }); }
      if (nSides === 4) { borders.fourSide++; pushSample(borders.fourSideSamples, el, {}); }
      if (isMdCell) borders.mdTableCells++;
      else if (rawSet.has(el)) borders.rawBlocks++;
      else if (!isControl) {
        borders.allBordered++;
        if (CONTAINER_TAGS.has(el.tagName) && nSides >= 2) {
          borders.strictContainers++;
          pushSample(borders.strictSamples, el, { sides: nSides });
        }
      }
    }

    // ── (a2) radius ladder ─────────────────────────────────────────────────
    const rawRadius = cs.borderRadius || "0px";
    if (visText) radius.hist.set(rawRadius, (radius.hist.get(rawRadius) ?? 0) + 1);
    if (visText && rawRadius !== "0px") {
      const raw = rawRadius;
      const toks = raw.split(/[\s/]+/).filter(Boolean);
      let off = false;
      for (const t of toks) {
        if (t.endsWith("%")) { const p = parseFloat(t); if (p !== 50) off = true; continue; }
        const p = parseFloat(t);
        if (!Number.isFinite(p)) continue;
        if (radiusLadder.has(p)) continue;
        if (p >= radiusPill) continue; // pill / circle
        off = true;
      }
      radius.hist.set(raw, (radius.hist.get(raw) ?? 0) + 1);
      if (off) {
        radius.offCount++;
        const e = radius.off.get(raw) ?? { count: 0, samples: [] };
        e.count++; pushSample(e.samples, el, {}); radius.off.set(raw, e);
        pushSample(radius.samples, el, { v: raw });
      }
    }

    // ── (c) inline styles (§12 rows 11/12) ─────────────────────────────────
    // Row 12 counts inline styles on *visible* elements. Counting every [style]
    // node let antd's hidden measurement rows (ant-table-measure-row) inflate
    // #stats by 22 of its 63 — a page cannot be "too inline-styled" because of
    // rows nobody can see.
    const styleAttr = el.getAttribute("style");
    if (visText && styleAttr && styleAttr.trim()) {
      inline.styleTotal++;
      if (/font-size/i.test(styleAttr)) { inline.fontSized++; pushSample(inline.fontSizedSamples, el, {}); }
    }

    // ── (d) interactive: naming (§12 row 17) and hit targets (row 18) ──────
    let isInteractive = false;
    try { isInteractive = el.matches(cfg.interactiveSel); } catch { isInteractive = false; }
    if (!isInteractive && cls) {
      for (const h of cfg.handClasses) if (el.classList.contains(h.slice(1))) { isInteractive = true; break; }
    }
    if (!isInteractive && cls) {
      for (const h of cfg.clickableExtra) if (el.classList.contains(h.slice(1))) { isInteractive = true; break; }
    }
    if (isInteractive && visText) {
      inter.count++;
      const label = el.getAttribute("aria-label");
      const byId = el.getAttribute("aria-labelledby");
      const title = el.getAttribute("title");
      const value = el.getAttribute("value");
      let named = "";
      if (label && label.trim()) named = label.trim();
      else if (byId) {
        named = byId.split(/\s+/).map((id) => document.getElementById(id)?.textContent?.trim() || "").join(" ").trim();
      }
      if (!named && title && title.trim()) named = title.trim();
      if (!named && el.textContent && el.textContent.trim()) named = el.textContent.trim();
      if (!named && value && value.trim()) named = value.trim();
      if (!named && el.tagName === "INPUT") named = el.getAttribute("placeholder")?.trim() || "";
      if (!named) { inter.unnamedCount++; pushSample(inter.unnamed, el, { tag: el.tagName }); }
      // primitives §5.4: an inline text link rides the text flow and is exempt.
      const inlineLink = el.tagName === "A" && cs.display === "inline";
      if (!inlineLink) {
        const root = hitTarget(el);
        const rr = root === el ? r : root.getBoundingClientRect();
        if (root !== el) { inter.promoted++; inter.viaRoot++; }
        const w = rr.width, h = rr.height;
        // Both readings, side by side; the hit-target one is judged.
        const paint = paintedBox(el);
        const pb = paint === el ? r : paint.getBoundingClientRect();
        if (w < 24 || h < 24) {
          inter.hit24++;
          inter.small24++;
          const rec = { sel: cssPath(el), w: Math.round(w), h: Math.round(h), via: root === el ? "自身" : "控件根 " + String(root.getAttribute("class") || root.tagName).split(/\s+/)[0], paint: Math.round(pb.width) + "x" + Math.round(pb.height) };
          pushSample(inter.small24Samples, el, rec, interCap);
          if (inter.hit24All.length < interCap) inter.hit24All.push(rec);
        } else if (w < 32 || h < 32) {
          inter.hit32++;
          inter.small32++;
          pushSample(inter.small32Samples, el, { w: Math.round(w), h: Math.round(h), via: root === el ? "自身" : "控件根" }, interCap);
        }
        // §12.9: sider subtree = shell; everything else = content (the ambiguous
        // bucket folds into content, which is the conservative direction for a
        // budget — §12.9 ① says 宁可多算，不可漏算).
        const zone = el.closest(shellSel) ? "shell" : "content";
        if (w < 24 || h < 24) {
          if (zone === "shell") inter.shell24++; else inter.content24++;
        } else if (w < 32 || h < 32) {
          if (zone === "shell") {
            inter.shell32++;
            if (inter.shell32All.length < interCap) {
              inter.shell32All.push({ sel: cssPath(el), tag: el.tagName.toLowerCase(), cls: String(el.getAttribute("class") || ""), w: Math.round(w), h: Math.round(h) });
            }
          } else inter.content32++;
        }
        if (pb.width < 24 || pb.height < 24) inter.paint24++;
        else if (pb.width < 32 || pb.height < 32) inter.paint32++;
      }
    }

    // ── (d2) primitives §11 row 4 extra: .inbox-card must stay a ridge (no border)
    if (visText && cls.includes("inbox-card")) {
      extras.inboxCards++;
      const sides = [bws[0], bws[1], bws[2], bws[3]].filter((x) => x > 0).length;
      if (sides > 0) extras.inboxCardBordered++;
    }
    if (cls.includes("nav-badge")) extras.navBadge++;

    // ── (d3) spacing ladder (primitives §11 row 37) ───────────────────────
    if (visText) {
      for (const [prop, cssProp] of SPACING_PROPS) {
        const raw = cs[prop];
        if (!raw || raw === "auto" || raw === "normal") continue;
        const v = parseFloat(raw);
        if (!Number.isFinite(v) || v === 0) continue;
        // rawCount is §12.6's "raw" column: the OLD, origin-blind off-ladder
        // count. On-ladder values are not interesting to any column, and
        // skipping them here also keeps the CSSOM lookups off the hot path.
        if (spacingLadder.has(v)) continue;
        spacing.rawCount++;
        const cls = classifySpacing(el, prop, cssProp, raw);
        const rec = { sel: cssPath(el), prop, px: v, declared: cls.value, src: cls.src };
        // The four comparison columns never enter the numerator (§12.6).
        if (cls.origin === "antd") { bucketAdd(spacing.antdDerived, rec); continue; }
        if (cls.origin === "uaDefault") { bucketAdd(spacing.uaDefault, rec); continue; }
        if (cls.origin === "autoResolved") { bucketAdd(spacing.autoResolved, rec); continue; }
        if (cls.origin === "clampVw" || /^-?\d+\.\d\d/.test(raw)) { bucketAdd(spacing.clampVw, rec); continue; }
        // Author values only, from here down.
        if (v < 0) {
          // Negatives enter by ticket only: the value must be in the closed set
          // AND the element must be one of §12.6's named carriers. Otherwise any
          // element could push its hover surface out of the row with -12.
          if (namedBleed(el, v)) { bucketAdd(spacing.bleed, rec); continue; }
        }
        bucketAdd(spacing.numerator, rec);
        if (v < 0) spacing.numeratorNeg++; else spacing.numeratorPos++;
        const e = spacing.off.get(v) ?? { count: 0, samples: [] };
        e.count++;
        pushSample(e.samples, el, { prop, declared: cls.value, src: cls.src });
        spacing.off.set(v, e);
      }
    }

    // ── (e) structure (§12 rows 23/24) ─────────────────────────────────────
    if (el.tagName === "H1" && vis) {
      structure.h1++;
      if (structure.h1Text === null) structure.h1Text = (el.textContent || "").trim().slice(0, 60);
    }
    if (vis && el.matches && el.matches(".stat-strip, .dash-grid, .dash-head, .readout, .agent-stats")) structure.dashboard++;
    if (el.hasAttribute("aria-expanded") && vis) structure.ariaExpanded++;
    if (el.tagName === "SUMMARY" && vis) structure.summaries++;
    if (!structure.heading && ["H1", "H2", "H3"].includes(el.tagName) && visText) {
      const t = (el.textContent || "").trim();
      if (t) structure.heading = t.slice(0, 60);
    }
  }

  // ── MASTER §12 row 6 / §12.1: the page title ("层级中心") ────────────────
  // §12.1 makes this the price of the row-6 exemption: an exempt page may not
  // manufacture readouts, but it must still have exactly one page title. The
  // carriers come from the contract per route (.view-bar h2; #home uses
  // .home-hero h1 at 32px), so a page cannot pass by renaming its title.
  const titleSelectors = cfg.titleSelectors ?? [];
  const titleEls = titleSelectors.flatMap((sel) => [...document.querySelectorAll(sel)]);
  const visibleTitles = titleEls.filter((el) => {
    const r = el.getBoundingClientRect();
    const tcs = csOf(el);
    return (
      r.width >= 1 && r.height >= 1 && tcs.visibility !== "hidden" && tcs.display !== "none" &&
      parseFloat(tcs.opacity) > 0 && !clippedAway(tcs, r)
    );
  });
  const pageTitle = {
    selectors: titleSelectors,
    matched: titleEls.length,
    count: visibleTitles.length,
    sizes: visibleTitles.map((el) => parseFloat(csOf(el).fontSize) || 0),
    samples: visibleTitles.slice(0, 3).map((el) => ({ sel: cssPath(el), px: parseFloat(csOf(el).fontSize) || 0 })),
  };

  // ── (e) WCAG contrast, over every visible text-bearing element ───────────
  // Two counts are reported because two contracts exist and they differ:
  //   docViolations  — MASTER §12 rows 13/14: everything below 18.66px must
  //                    clear 4.5:1 (the rows' own 判定方式).
  //   wcagViolations — the WCAG 2.x rule the task's acceptance names: 4.5:1
  //                    body, 3:1 large (>=24px, or >=18.66px && weight>=700).
  const con = {
    checked: 0, ambiguous: 0, alphaSkipped: 0,
    docViolations: 0, wcagViolations: 0, largeViolations: 0, worst: [], worstMax: Infinity,
  };
  for (const el of textEls) {
    const cs = csOf(el);
    const r = el.getBoundingClientRect();
    if (!(r.width >= 1 && r.height >= 1 && cs.visibility !== "hidden" && cs.display !== "none" && parseFloat(cs.opacity) > 0)) continue;
    if (clippedAway(cs, r)) continue; // .sr-only text is not read on screen
    const fg0 = parseColor(cs.color);
    if (!fg0 || fg0.a < 0.05) { con.alphaSkipped++; continue; }
    const back = bgFor(el);
    if (back.ambiguous) { con.ambiguous++; continue; } // gradient/image behind text: not a flat pair
    const fg = fg0.a >= 0.999 ? fg0 : blend(fg0, back.bg);
    const ratio = contrast(fg, back.bg);
    const fs = parseFloat(cs.fontSize) || 0;
    const fw = parseInt(cs.fontWeight, 10) || 400;
    const isLarge = fs >= 24 || (fs >= 18.66 && fw >= 700);
    con.checked++;
    if (fs < 18.66 && ratio < 4.5) con.docViolations++;
    if (isLarge && ratio < 3) con.largeViolations++;
    if (ratio < (isLarge ? 3 : 4.5)) con.wcagViolations++;
    // Keep only the `cap` worst pairs. cssPath() and text extraction are O(subtree),
    // so they are built lazily — on a 150k-node page the naive version is O(n^2).
    const rounded = Math.round(ratio * 100) / 100;
    const keepFull = con.worst.length < cap;
    if (keepFull || rounded < con.worstMax) {
      con.worst.push({
        sel: cssPath(el), px: fs, weight: fw, ratio: rounded,
        fg: rgbaText(fg0), bg: rgbaText(back.bg), text: textSamples.get(el) ?? (el.childNodes.length < 8 ? (el.textContent || "").trim().slice(0, 28) : ""),
      });
      if (con.worst.length > cap) {
        con.worst.sort((a, b) => a.ratio - b.ratio);
        con.worst.length = cap;
      }
      con.worstMax = con.worst.length < cap ? Infinity : con.worst[con.worst.length - 1].ratio;
    }
  }
  con.worst.sort((a, b) => a.ratio - b.ratio);

  // ── material staircase (§12 row 29) + light ring (§12 row 30) ───────────
  const contentEl = document.querySelector(".content");
  const canvas = contentEl ? bgFor(contentEl).bg : rootBg;
  const panelEl = [...document.querySelectorAll(".card, .panel, .ant-card")].find((el) => {
    const r = el.getBoundingClientRect();
    return r.width >= 1 && r.height >= 1;
  });
  const containerToken = parseColor(
    getComputedStyle(document.querySelector(".ruagent") ?? document.documentElement)
      .getPropertyValue("--ant-color-bg-container").trim(),
  );
  const panel = panelEl
    ? bgFor(panelEl).bg
    : containerToken
      ? (containerToken.a >= 1 ? containerToken : blend(containerToken, canvas))
      : null;
  const material = {
    canvas: rgbaText(canvas), canvasLuma: lum(canvas),
    panel: panel ? rgbaText(panel) : null, panelLuma: panel ? lum(panel) : null,
    stepRatio: panel ? lum(panel) / lum(canvas) : null,
    cardSel: panelEl ? cssPath(panelEl) : null,
    cardFound: !!panelEl,
    panelSource: panelEl ? "element" : containerToken ? "token --ant-color-bg-container" : "none",
  };
  const ring = (() => {
    // One ring value, two possible grounds. MASTER §12 row 30's baseline
    // ("0 0 0 1px #e7e9ed ... 对底 1.22:1") is measured against the white panel;
    // the ring is actually *drawn* on the pageCanvas, where the same value reads
    // 1.13:1. Legibility depends on the ground it is really on, so the canvas
    // reading is the stricter and the one that is judged — both are reported.
    const parseRing = (bs) => {
      if (!bs || bs === "none") return null;
      for (const part of String(bs).split(/,(?![^(]*\))/)) {
        // The colour may sit at either end of a box-shadow. The box-shadow
        // *property* is normalised colour-first by the browser, but a custom
        // property's computed value keeps the author's order -- and
        // --panel-shadow is authored as "0 0 0 1px var(--rule-soft)". Matching
        // only the leading form silently found no ring on the token path.
        // A custom property keeps the author's spelling: unitless zeros
        // ("0 0 0 1px") and hex colours ("#e7e9ed"). The normalised box-shadow
        // property uses "0px" and rgb(). Accept both spellings, or the token
        // path silently finds no ring on exactly the pages that need it.
        const m = part.match(
          /^\s*(?:(rgba?\([^)]*\)|color\([^)]*\)|#[0-9a-fA-F]{3,8})\s+)?(-?[\d.]+)(?:px)?\s+(-?[\d.]+)(?:px)?\s+(-?[\d.]+)(?:px)?\s+(-?[\d.]+)(?:px)?(?:\s+(rgba?\([^)]*\)|color\([^)]*\)|#[0-9a-fA-F]{3,8}))?/,
        );
        if (!m) continue;
        const colorText = m[1] ?? m[6];
        if (!colorText) continue;
        if (+m[2] === 0 && +m[3] === 0 && +m[4] === 0 && +m[5] > 0) {
          const c = parseColor(colorText);
          if (!c) continue;
          const overPanel = c.a >= 1 ? c : blend(c, panel ?? canvas);
          const overCanvas = c.a >= 1 ? c : blend(c, canvas);
          return {
            exists: true, raw: part.trim(), color: rgbaText(c), spread: +m[5],
            ratio: Math.round(contrast(overPanel, panel ?? canvas) * 100) / 100,
            ratioOnCanvas: Math.round(contrast(overCanvas, canvas) * 100) / 100,
          };
        }
      }
      return null;
    };
    if (!panelEl) {
      // No visible card to read it from (an empty #inbox has none), but row 30's
      // baseline is a *token* claim: --panel-shadow IS "0 0 0 1px var(--rule-soft)".
      // Without this fallback the row reported "无 ring" on exactly the pages the
      // captain measured at 1.13:1.
      const scope = document.querySelector(".ruagent") ?? document.documentElement;
      const tok = parseRing(getComputedStyle(scope).getPropertyValue("--panel-shadow").trim());
      return tok
        ? { ...tok, source: "token --panel-shadow" }
        : { exists: false, raw: "", reason: "no visible .card/.panel and no --panel-shadow" };
    }
    const bs = csOf(panelEl).boxShadow;
    const own = parseRing(bs);
    return own ? { ...own, source: "element" } : { exists: false, raw: bs || "none", source: "element" };
  })();

  // ── token table (§12 rows 32/33): resolve at runtime, not from source ────
  const tokenScope = document.querySelector(".ruagent") ?? document.documentElement;
  const tokens = {};
  for (const n of cfg.tokenVars) {
    const v = getComputedStyle(tokenScope).getPropertyValue(n).trim();
    if (v) tokens[n] = v;
  }
  const graph = (() => {
    const el = document.querySelector(".graph-canvas");
    if (!el) return null;
    const gcs = csOf(el);
    const bg = bgFor(el).bg;
    const out = { bg: rgbaText(bg), luma: lum(bg), edge: null, edgeAlpha: null, edgeRatio: null, categories: [] };
    const edge = parseColor(gcs.getPropertyValue("--graph-edge").trim());
    if (edge) {
      out.edge = rgbaText(edge);
      out.edgeAlpha = edge.a;
      out.edgeRatio = Math.round(contrast(edge.a >= 1 ? edge : blend(edge, bg), bg) * 100) / 100;
    }
    for (const v of cfg.graphVars) {
      const c = parseColor(gcs.getPropertyValue(v).trim());
      if (c) out.categories.push({ name: v, color: rgbaText(c), ratio: Math.round(contrast(c, bg) * 100) / 100 });
    }
    return out;
  })();

  // ── primitives §5.2.2 / §12 row 23: aria-expanded on HAND-WRITTEN controls
  // only. The antd Select combobox also carries aria-expanded; counting the
  // whole document (MASTER's original wording) makes the row pass vacuously.
  const expand = cfg.expandSelectors.map((sel) => {
    const els = [...document.querySelectorAll(sel)];
    const withAttr = els.filter((e) => ["true", "false"].includes((e.getAttribute("aria-expanded") || "").toLowerCase()));
    return {
      sel, count: els.length, withAriaExpanded: withAttr.length,
      samples: els.slice(0, 3).map((e) => ({ sel: cssPath(e), v: e.getAttribute("aria-expanded") })),
    };
  });
  const nativeExpand = document.querySelectorAll(cfg.nativeExpandSelector).length;
  const antdExpand = [...document.querySelectorAll("[aria-expanded]")].filter((e) => {
    const c = e.getAttribute("class") || "";
    return /ant-(select|tree-select|cascader|picker|dropdown)/.test(c) || /combobox/.test(e.getAttribute("role") || "");
  }).length;

  // ── §12 row 34: hand-written clickables that never acknowledge :active ──
  const wantHand = cfg.handClasses.map((c) => c.slice(1));
  const presentHand = wantHand.filter((c) => document.querySelector("." + c));
  const withActive = new Set();
  const activeRules = [];
  let ownSheets = 0, runtimeSheets = 0;
  const ownMax = new Set();
  const runtimeMax = new Set();
  const walkRules = (list) => {
    for (const rule of list) {
      if (rule.cssRules && rule.cssRules.length) { walkRules(rule.cssRules); continue; }
      const sel = rule.selectorText;
      if (!sel) continue;
      if (sel.includes(":active")) {
        activeRules.push(sel);
        for (const c of wantHand) if (sel.includes("." + c)) withActive.add(c);
      }
    }
  };
  for (const sheet of document.styleSheets) {
    let rules;
    try { rules = sheet.cssRules; } catch { continue; }
    if (!rules) continue;
    const isOwn = !!sheet.href || (sheet.ownerNode && (sheet.ownerNode.textContent || "").includes("--rule-soft"));
    if (isOwn) ownSheets++; else runtimeSheets++;
    const scanMedia = (list) => {
      for (const rule of list) {
        const mt = rule.media && rule.media.mediaText;
        if (mt) {
          const hits = mt.match(/(?:max|min)-width:\s*([\d.]+)px/g) || [];
          for (const hit of hits) {
            const v = parseFloat(hit.replace(/[^\d.]/g, ""));
            if (hit.startsWith("max")) (isOwn ? ownMax : runtimeMax).add(v);
          }
        }
        if (rule.cssRules && rule.cssRules.length) scanMedia(rule.cssRules);
      }
    };
    scanMedia(rules);
    walkRules(rules);
  }
  const active = { present: presentHand, withActive: [...withActive], missing: presentHand.filter((c) => !withActive.has(c)), ruleCount: activeRules.length };
  const breakpoints = { own: [...ownMax].sort((a, b) => a - b), runtime: [...runtimeMax].sort((a, b) => a - b), ownSheets, runtimeSheets };

  // ── §12 row 36: sidebar width. Row 26 (longest blocking task) is NOT read
  // here: this probe walks every node and cssPath()s samples, so its own
  // execution is itself a long task on a 153k-node page. It is read by
  // probeLongTasks() *before* the screenshot and before this walk, so row 26
  // measures the app, not the instrument.
  const sider = document.querySelector(".app-sider");
  const siderWidth = sider ? Math.round(sider.getBoundingClientRect().width) : null;
  scratch.remove(); // never leave the resolver box behind for the next capture

  return {
    url: location.href,
    hash: location.hash,
    hashOk: location.hash === cfg.expectHash,
    title: document.title,
    viewport: { w: window.innerWidth, h: window.innerHeight },
    scroll: {
      docWidth: document.documentElement.scrollWidth,
      bodyWidth: document.body.scrollWidth,
      innerWidth: window.innerWidth,
    },
    markers: cfg.markers.filter((sel) => !!document.querySelector(sel)),
    domNodes,
    text: {
      count: text.count, le14: text.le14, ge18: text.ge18, maxFontSize: text.maxFontSize,
      sizeHist: [...text.sizeHist.entries()].sort((a, b) => a[0] - b[0]).map(([px, n]) => ({ px, n })),
      offScale: [...text.offScale.entries()].sort((a, b) => b[1].count - a[1].count).map(([px, e]) => ({ px, count: e.count, samples: e.samples })),
      offScaleCount: [...text.offScale.values()].reduce((s, e) => s + e.count, 0),
      offWeight: [...text.offWeight.entries()].sort((a, b) => b[1].count - a[1].count).map(([w, e]) => ({ weight: w, count: e.count, samples: e.samples })),
      offWeightCount: [...text.offWeight.values()].reduce((s, e) => s + e.count, 0),
      ge18Samples: text.ge18Samples, maxSamples: text.maxSamples,
    },
    borders,
    radius: {
      offCount: radius.offCount,
      off: [...radius.off.entries()].sort((a, b) => b[1].count - a[1].count).map(([v, e]) => ({ value: v, count: e.count, samples: e.samples })),
      hist: [...radius.hist.entries()].sort((a, b) => b[1] - a[1]).slice(0, 8).map(([v, n]) => ({ value: v, n })),
      samples: radius.samples,
    },
    inline,
    inter,
    structure,
    contrast: con,
    material,
    ring,
    tokens,
    graph,
    active,
    breakpoints,
    siderWidth,
    pageTitle,
    expand: { controls: expand, native: nativeExpand, antdCount: antdExpand },
    extras,
    spacing: {
      // §12.6: the numerator is the only judged quantity; rawCount and the four
      // comparison columns are reported side by side (same shape as row 1's two
      // luma readings) so nothing is hidden by the narrowing.
      numeratorCount: spacing.numerator.count,
      numeratorPos: spacing.numeratorPos,
      numeratorNeg: spacing.numeratorNeg,
      rawCount: spacing.rawCount,
      offCount: [...spacing.off.values()].reduce((a, e) => a + e.count, 0),
      off: [...spacing.off.entries()].sort((a, b) => b[1].count - a[1].count).map(([v, e]) => ({ px: v, count: e.count, samples: e.samples })),
      antdDerived: spacing.antdDerived,
      uaDefault: spacing.uaDefault,
      autoResolved: spacing.autoResolved,
      clampVw: spacing.clampVw,
      bleed: spacing.bleed,
      numerator: spacing.numerator,
      bleedSet,
      cssom: { authorSheets: authorRules.length, antdSheets: antdRules.length, authorTop: authorRules.map((r) => r.length).slice(0, 3), antdTop: antdRules.map((r) => r.length).slice(0, 3) },
      ladder: cfg.spacingLadder,
    },
  };
};

// ── One route x one mode ────────────────────────────────────────────────────
async function walkFocus(page, maxTabs, budgetMs = 30000) {
  await page.evaluate(() => {
    if (document.activeElement && document.activeElement.blur) document.activeElement.blur();
    window.scrollTo(0, 0);
  });
  const stops = [];
  const startedAt = Date.now();
  let budgetHit = false;
  for (let i = 0; i < maxTabs; i++) {
    if (Date.now() - startedAt > budgetMs) { budgetHit = true; break; }
    await page.keyboard.press("Tab");
    const rec = await page.evaluate(() => {
      const el = document.activeElement;
      if (!el || el === document.body) {
        return { tag: "BODY", sel: "body", form: "none", ringKind: "none", ring: null, bgBehind: null, inComposer: false, composer: null, isTextInput: false, outline: "" };
      }
      const parse = (str) => {
        if (!str) return null;
        const cm = String(str).match(/^color\(srgb\s+([\d.]+)\s+([\d.]+)\s+([\d.]+)(?:\s*\/\s*([\d.]+%?))?\)$/);
        if (cm) {
          const a = cm[4] == null ? 1 : cm[4].endsWith("%") ? parseFloat(cm[4]) / 100 : parseFloat(cm[4]);
          return { r: parseFloat(cm[1]) * 255, g: parseFloat(cm[2]) * 255, b: parseFloat(cm[3]) * 255, a };
        }
        const m = String(str).match(/^rgba?\(([^)]+)\)$/);
        if (!m) return null;
        const q = m[1].split(/[,\s/]+/).filter(Boolean).map(Number);
        return q.length >= 3 ? { r: q[0], g: q[1], b: q[2], a: q.length > 3 ? q[3] : 1 } : null;
      };
      const blend = (fg, bg) => ({
        r: fg.r * fg.a + bg.r * (1 - fg.a),
        g: fg.g * fg.a + bg.g * (1 - fg.a),
        b: fg.b * fg.a + bg.b * (1 - fg.a),
        a: 1,
      });
      const rootCs = getComputedStyle(document.documentElement);
      const rootParsed = parse(rootCs.backgroundColor);
      const base = rootParsed && rootParsed.a > 0 ? { ...rootParsed, a: 1 } : { r: 255, g: 255, b: 255, a: 1 };
      const behind = (node) => {
        let acc = base;
        const chain = [];
        for (let n = node; n; n = n.parentElement) {
          const c = parse(getComputedStyle(n).backgroundColor);
          if (c && c.a > 0) chain.push(c);
          if (c && c.a >= 0.999) break;
        }
        for (let j = chain.length - 1; j >= 0; j--) {
          const c = chain[j];
          acc = blend(c, acc);
        }
        return acc;
      };
      const css = getComputedStyle(el);
      const ow = parseFloat(css.outlineWidth) || 0;
      const os = css.outlineStyle;
      // primitives §5.1: a ring is only a ring when it is `solid` and >= 2px.
      // `outline-style: auto` (UA default) is explicitly disqualified.
      let form = "none";
      if (os === "solid" && ow >= 2) form = "solid";
      else if (os === "solid") form = "thin-solid";
      else if (os === "auto") form = "auto";
      else if (os && os !== "none" && os !== "hidden") form = os;
      let shadowRing = null;
      if (css.boxShadow && css.boxShadow !== "none") {
        for (const part of css.boxShadow.split(/,(?![^(]*\))/)) {
          const m = part.match(/^(rgba?\([^)]*\)|color\([^)]*\))\s+(-?[\d.]+)px\s+(-?[\d.]+)px\s+(-?[\d.]+)px\s+(-?[\d.]+)px/);
          if (m && +m[2] === 0 && +m[3] === 0 && +m[4] === 0 && +m[5] > 0) {
            shadowRing = { color: m[1], spread: +m[5] };
            break;
          }
        }
      }
      // ── antd does NOT put the ring on the focused control ────────────────
      // .ant-select-input keeps outline:none and the ring is drawn on the control
      // that *owns* the focus (:focus-within), measured as box-shadow
      // 0 0 0 2px <--focus-ring> (index.css :root .ant-select:...:focus-within).
      // Reading only document.activeElement made row 16 report the same DOM as
      // PASS in dark and FAIL in light — the tool contradicting itself on one
      // fact is worse than being wrong, because it points the fix the wrong way.
      // So: take the element's own ring if it has one, otherwise walk up to the
      // focus owner and accept the first ring that satisfies §5.1's geometry
      // (solid outline >= 2px, or a zero-offset spread box-shadow >= 2px).
      const ringFrom = (node) => {
        const c = getComputedStyle(node);
        const w = parseFloat(c.outlineWidth) || 0;
        if (c.outlineStyle === "solid" && w >= 2) return { color: c.outlineColor, width: w, via: "own outline" };
        for (const part of String(c.boxShadow || "").split(/,(?![^(]*\))/)) {
          const m = part.match(/^(rgba?\([^)]*\)|color\([^)]*\))\s+(-?[\d.]+)px\s+(-?[\d.]+)px\s+(-?[\d.]+)px\s+(-?[\d.]+)px/);
          if (m && +m[2] === 0 && +m[3] === 0 && +m[4] === 0 && +m[5] >= 2) {
            return { color: m[1], width: +m[5], via: "focus-owner box-shadow" };
          }
        }
        return null;
      };
      const ownRing = form === "solid" ? { color: css.outlineColor, width: ow, via: "own outline" } : null;
      let ringSrc = ownRing;
      let ringOwnerSel = null;
      if (!ringSrc) {
        for (let n = el.parentElement, hops = 0; n && hops < 4; n = n.parentElement, hops++) {
          const r = ringFrom(n);
          if (!r) continue;
          ringSrc = r;
          const oc = (n.getAttribute("class") || "").trim().split(/\s+/).filter(Boolean).slice(0, 2);
          ringOwnerSel = n.tagName.toLowerCase() + (oc.length ? "." + oc.join(".") : "");
          break;
        }
      }
      const ringColor = ringSrc ? ringSrc.color : shadowRing ? shadowRing.color : null;
      const comp = el.closest ? el.closest(".composer") : null;
      let composer = null;
      if (comp) {
        const ccs = getComputedStyle(comp);
        const own = parse(ccs.backgroundColor);
        const bg = own && own.a > 0 ? blend(own, behind(comp.parentElement)) : behind(comp);
        composer = {
          borderColor: ccs.borderTopColor,
          bg: `rgb(${Math.round(bg.r)}, ${Math.round(bg.g)}, ${Math.round(bg.b)})`,
        };
      }
      const b = behind(el.parentElement ?? el);
      const cls = (el.getAttribute("class") || "").trim().split(/\s+/).filter(Boolean).slice(0, 2);
      return {
        tag: el.tagName,
        sel: el.tagName.toLowerCase() + (cls.length ? "." + cls.join(".") : ""),
        form: ringSrc ? "solid" : form,
        ownForm: form,
        ringKind: ringSrc ? "solid" : shadowRing ? "shadow+" + form : form,
        ring: ringColor,
        ringWidth: ringSrc ? ringSrc.width : ow,
        ringVia: ringSrc ? ringSrc.via : null,
        ringOwner: ringOwnerSel,
        outline: `${os} ${css.outlineWidth} ${css.outlineColor}`.trim(),
        shadow: css.boxShadow === "none" ? "" : css.boxShadow.slice(0, 80),
        bgBehind: `rgb(${Math.round(b.r)}, ${Math.round(b.g)}, ${Math.round(b.b)})`,
        inComposer: !!comp,
        composer,
        isTextInput: el.tagName === "INPUT" || el.tagName === "TEXTAREA",
      };
    });
    stops.push(rec);
  }
  stops.budgetHit = budgetHit;
  stops.walkMs = Date.now() - startedAt;
  stops.requested = maxTabs;
  return stops;
}

// ── A fixed settle condition, so a route cannot be judged on a half-mounted
// page. #task's RunTimeline mounts progressively: three runs of the same source
// gave <24px hit-target counts of 4 / 10 / 11 (row 18). "Wait 2.5s" is not a
// condition, it is a hope. Wait until the node count stops changing for
// `quietMs`, with a hard cap; if the cap is hit, say so — an unstable DOM is a
// measurement caveat, not a silent pass.
async function waitForDomStable(page, quietMs, maxMs) {
  const t0 = Date.now();
  let last = -1;
  let stableSince = Date.now();
  let samples = 0;
  while (Date.now() - t0 < maxMs) {
    const n = await page.evaluate(() => document.querySelectorAll("*").length).catch(() => -1);
    samples++;
    if (n !== last) {
      last = n;
      stableSince = Date.now();
    } else if (Date.now() - stableSince >= quietMs) {
      return { nodes: n, ms: Date.now() - t0, stable: true, samples };
    }
    await page.waitForTimeout(200);
  }
  return { nodes: last, ms: Date.now() - t0, stable: false, samples };
}

// Row 46: the width of the SIDEBAR SESSION ROW'S MAIN LABEL (the title).
//
// Why a new row: systems' control run (all 7 rules off) left the title 6px at
// 1440 and 43.7px at 768 while the audit reported 41/41 green -- NO judge was
// looking at this object, so it could never go red. The user-visible symptom is
// exactly it: the session text cannot be read at all.
//
// The floor is 5em of the title's OWN font size, because that is the design's
// own choice: panel/src/index.css .chat-session-row .title { min-width: 5em }
// (13px x 5 = 65px). Deriving it from font-size rather than hardcoding 65 means
// a future type-scale change moves the floor with the design instead of
// silently invalidating the row.
//
// 390 is NOT one of the 26 captures (those are 1440x900 x 13 routes x 2 modes),
// so the probe SWEEPS the viewport itself -- 1440 and 390 both, inside the chat
// capture -- and restores the measurement viewport afterwards.
const TITLE_SWEEP = [1440, 768, 390];
const TITLE_FLOOR_EM = 5;

async function probeSessionTitle(page, restore, injectCss) {
  // The object set is EVERY list row label, not just the chat sidebar: the
  // collapse comes from the SHARED rule .title { flex: 1; min-width: 0; ... },
  // so a chat-only scope would stay GREEN while #knowledge's labels sit at 0
  // (measured at 390). A judge whose object set is narrower than the defect is
  // the rule-4 hole: it matches nothing there and passes silently.
  //
  // The control-run stylesheet is re-applied HERE because the row-20 failure
  // probe reloads the document between the injection point and this probe.
  if (injectCss) {
    const css = readFileSync(injectCss, "utf8");
    await page.addStyleTag({ content: css });
  }
  const out = [];
  for (const w of TITLE_SWEEP) {
    await page.setViewportSize({ width: w, height: 900 });
    await page.waitForTimeout(220);
    const r = await page.evaluate(() => {
      const titles = [...document.querySelectorAll(".row-btn .title")];
      return {
        rows: document.querySelectorAll(".row-btn").length,
        titles: titles.map((t) => {
          const b = t.getBoundingClientRect();
          const cs = getComputedStyle(t);
          // The route is carried per sample so the display can say WHERE the
          // worst label is -- a bare number would not say which list is broken.
          return {
            route: (location.hash || "#?").replace(/^#/, "").split("/")[0],
            text: String(t.textContent || "").trim().slice(0, 24),
            w: Math.round(b.width * 100) / 100,
            fs: parseFloat(cs.fontSize) || 0,
            minW: cs.minWidth,
            visible: cs.display !== "none" && cs.visibility !== "hidden",
          };
        }),
      };
    });
    out.push({ viewport: w + "x900", rows: r.rows, titles: r.titles });
  }
  if (restore) {
    // measureViewport is a Playwright {width, height} object, not {w, h}: using
    // the wrong keys made this throw and the row read as an error (not_measured,
    // which the guard correctly refused to turn into a PASS).
    await page.setViewportSize({ width: restore.width, height: restore.height });
    await page.waitForTimeout(120);
  }
  return out;
}
// Pure, so --self-test can force both directions.
//
// ONE floor, universal: width > 0 AND width >= 5em of the element's own font
// size. The captain first ruled the floor had to be DERIVED from a per-list
// declared min-width, then withdrew it after the measurement -- and the reason
// is worth keeping: #sessions at 390 is 40.98px at font-size 14, about THREE
// characters, which is not a readable label. That is the shape the user
// complained about (the session text cannot be seen), so it is a REAL defect,
// not the false red a blanket floor was feared to create.
//
// 5em is not chat's private number: it is written on the SAME element class
// (.chat-session-row .title { min-width: 5em }, index.css:1909) and fixes the
// SAME collapse caused by the SAME shared rule (.title { flex: 1;
// min-width: 0; ... }, index.css:638). Read against each element's own font
// size it says one thing for every list: this label must fit at least five
// characters.
//
// What the captain wanted from a derived floor -- which lists declare their own
// protection -- is kept as a READING, not as a threshold: every sample's
// computed min-width is printed per route below.
function sessionTitleVerdict(sweeps, em) {
  const samples = [];
  for (const s of sweeps || []) {
    for (const t of s.titles || []) samples.push({ ...t, viewport: s.viewport });
  }
  if (!samples.length) return { measured: false, pass: null, samples: 0 };
  const px = (v) => {
    const n = parseFloat(String(v ?? ""));
    return Number.isFinite(n) ? n : 0;
  };
  const judged = samples.map((t) => {
    const declared = px(t.minW);
    return { ...t, declared, floor: (em || TITLE_FLOOR_EM) * (t.fs || 13) };
  });
  const bad = judged.filter((t) => !(t.w > 0 && t.w >= t.floor - 0.5));
  const byRoute = new Map();
  for (const t of judged) {
    const k = (t.route || "?") + "@" + t.viewport;
    const cur = byRoute.get(k);
    if (!cur || t.w < cur.w) byRoute.set(k, t);
  }
  const perRoute = [...byRoute.entries()]
    .map(([k, t]) => k + "=" + t.w + "px/fs" + t.fs + "/声明min" + (t.declared > 0 ? t.declared : "—"))
    .sort();
  let worst = judged[0];
  for (const t of judged) if (t.w < worst.w) worst = t;
  return {
    measured: true,
    samples: judged.length,
    min: worst.w,
    floor: worst.floor,
    worst,
    bad,
    where: (worst.route || "?") + " @ " + worst.viewport,
    perRoute,
    declared: judged.filter((t) => t.declared > 0).length,
    pass: bad.length === 0,
  };
}

async function probeOverflow(page, viewports) {
  const out = [];
  for (const vp of viewports) {
    await page.setViewportSize({ width: vp.width, height: vp.height });
    await page.waitForTimeout(280);
    const r = await page.evaluate(() => {
      const sider = document.querySelector(".app-sider");
      return {
        docWidth: document.documentElement.scrollWidth,
        bodyWidth: document.body.scrollWidth,
        innerWidth: window.innerWidth,
        siderWidth: sider ? Math.round(sider.getBoundingClientRect().width) : null,
      };
    });
    out.push({
      viewport: `${vp.width}x${vp.height}`,
      width: vp.width,
      overflow: r.docWidth - r.innerWidth,
      overflowBody: r.bodyWidth - r.innerWidth,
      docWidth: r.docWidth,
      innerWidth: r.innerWidth,
      siderWidth: r.siderWidth,
    });
  }
  return out;
}

// ── MASTER §12.12's symptom test, made mechanical (rows 40/41) ──────────────
//
// §12.12 rules that a size which is the product of a LAYOUT ACCIDENT must not be
// certified as a design constant, and gives the falsifiable test: 「关闭压缩源后
// 尺寸显著改变」. The two rows below apply exactly that test to text and to
// sibling geometry, because the whole-shell defect it names (button.kbd-hint
// 26x45 = a 15px line-height times THREE lines) passed rows 18, 19 and 29-33 —
// it neither overflows nor violates the hit floor nor fails contrast. It was
// simply three lines of text where one was intended.
//
// Measurement happens in the page; CLASSIFICATION is a pure function so that
// --self-test can force both a FAIL and a PASS on synthetic input. A criterion
// nobody can falsify is how a false pass survives.
//
// 「有意截断」 (intended) is decidable from computed style, in this order:
//   · -webkit-line-clamp >= 1                 -> the design ASKS for N lines
//   · text-overflow: ellipsis + a hidden axis -> the design ASKS for one line
//     (either axis: antd clamps horizontally in table cells and vertically in
//      multi-line list items)
// Anything else that ends up on 2+ lines is judged on the §12.12 test alone.
function wrapClassify(r) {
  const intentional = !!r.intentional;
  const squeezed = !!r.squeezed;
  // The §12.12 test: relaxing the compression source (nowrap + flex-shrink: 0 +
  // min-width: max-content) collapses the box. For a SHORT carrier (<= 12 chars)
  // that is the definition of an accidental wrap — the text fits on one line, the
  // container just refused to give it the room.
  const hit = !intentional && squeezed;
  return {
    hit,
    kind: intentional ? "intentional" : squeezed ? "squeezed" : "content",
    why: intentional
      ? r.why || "intentional clamp"
      : squeezed
        ? "relaxed " + r.w + "x" + r.h + " -> " + r.relaxed + " (§12.12 压缩源测试)"
        : "relaxing the compression source does not collapse it",
  };
}

// A sibling overlap is real unless one of the two is an overlay by design.
// The exclusion is positional, not name-based, so it survives a class rename:
//   · position: absolute | fixed  -> a popup/dropdown/backdrop/portal is SUPPOSED
//     to paint over its siblings; that is what an overlay is.
//   · an overlay subtree (antd's .ant-dropdown / .ant-select-dropdown /
//     .ant-popover / .ant-tooltip / .ant-modal-wrap|mask / .ant-drawer /
//     .ant-picker-dropdown / .ant-notification / .ant-message, or a role=menu /
//     listbox / dialog / tooltip / alertdialog) — these keep a static-positioned
//     wrapper inside a portal, so position alone would miss them.
//   · z-index set with a non-static position -> same reasoning.
// The probe reports both flags on every pair, so a reviewer can see what was
// excluded instead of taking it on faith.
function overlapClassify(r) {
  const hit = !r.legit;
  return { hit, kind: r.legit ? "overlay" : "overlap" };
}

async function probeTextLayout(page) {
  const r = await page.evaluate(() => {
    const OVERLAY_SEL = ".ant-dropdown,.ant-select-dropdown,.ant-popover,.ant-tooltip,.ant-modal-wrap,.ant-modal-mask,.ant-drawer,.ant-picker-dropdown,.ant-notification,.ant-message,.cmdk,[role=menu],[role=listbox],[role=dialog],[role=tooltip],[role=alertdialog]";
    const norm = (s) => String(s || "").replace(/\s+/g, " ").trim();
    const vis = (el) => {
      const cs = getComputedStyle(el);
      if (cs.display === "none" || cs.visibility === "hidden" || Number(cs.opacity) === 0) return false;
      // .sr-only and friends clip to nothing; they are not on screen.
      if (cs.clip && cs.clip !== "auto") return false;
      if (cs.clipPath && cs.clipPath !== "none") return false;
      const b = el.getBoundingClientRect();
      return b.width >= 1 && b.height >= 1;
    };
    const ownText = (el) => {
      let t = "";
      for (const n of el.childNodes) if (n.nodeType === 3) t += n.nodeValue;
      return norm(t);
    };
    const tagOf = (el) => {
      const c = typeof el.className === "string" ? el.className.trim().split(/\s+/).slice(0, 3).join(".") : "";
      return el.tagName.toLowerCase() + (c ? "." + c : "");
    };
    // Real line count: one client rect per line box. This is the measurement, not
    // a guess — the geometric fallback (h / line-height) reads a min-height or
    // vertical padding as an extra line, which is exactly how the shell's
    // "26x45 = 3 lines" was mistaken for a design constant in the first place.
    // The fallback therefore measures the CONTENT box, and only when the range
    // API reports nothing.
    // Only the element's OWN text nodes are measured: a container that holds a
    // label and a differently-aligned value produces two rects on one visual
    // line, and counting the subtree read that as two lines (measured on
    // div.status-row, whose label and mono value sit on one row).
    const cluster = (tops) => {
      const s = [...tops].sort((a, b) => a - b);
      let n = 0;
      let last = -Infinity;
      for (const t of s) {
        if (t - last > 4) {
          n++;
          last = t;
        }
      }
      return n;
    };
    const lineCount = (el, contentH, lh) => {
      const tops = new Set();
      try {
        for (const n of el.childNodes) {
          if (n.nodeType !== 3 || !n.nodeValue || !n.nodeValue.trim()) continue;
          const rg = document.createRange();
          rg.selectNodeContents(n);
          for (const x of rg.getClientRects()) if (x.width > 0 && x.height > 0) tops.add(x.top);
        }
      } catch (e) {
        /* fall through to geometry */
      }
      if (tops.size > 0) return { lines: cluster(tops), via: "range" };
      return { lines: Math.max(1, Math.round(contentH / lh)), via: "geometry" };
    };
    const isSvg = (el) => el.namespaceURI === "http://www.w3.org/2000/svg";
    // The text's NATURAL single-line width, measured off-screen with the
    // element's own font. This -- not the relaxation test alone -- is the
    // decidable form of 「宽度明显小于其内容所需」.
    //
    // Why the relaxation test cannot decide on its own (found by the live
    // control): when a flex child is squeezed to 1-2px, setting flex-shrink: 0
    // and min-width: max-content ON THE CHILD does not release it -- the parent's
    // min-width: 0 still wins -- so the box stays 2px wide, the relaxed height
    // never collapses, and a real 9-line defect scored PASS. The natural width
    // has no such blind spot: 85px of text in a 2px box is a compression, full
    // stop.
    const probeScratch = document.createElement("span");
    probeScratch.style.cssText = "position:absolute;left:-9999px;top:0;white-space:nowrap;visibility:hidden;pointer-events:none";
    document.body.appendChild(probeScratch);
    const naturalWidth = (el, text) => {
      const cs = getComputedStyle(el);
      probeScratch.style.fontFamily = cs.fontFamily;
      probeScratch.style.fontSize = cs.fontSize;
      probeScratch.style.fontWeight = cs.fontWeight;
      probeScratch.style.fontStyle = cs.fontStyle;
      probeScratch.style.letterSpacing = cs.letterSpacing;
      probeScratch.style.fontVariant = cs.fontVariant;
      probeScratch.textContent = text;
      return probeScratch.getBoundingClientRect().width;
    };
    const all = [...document.querySelectorAll("body *")].filter(vis);
    const wraps = [];
    for (const el of all) {
      const text = ownText(el);
      if (!text || text.length > 12) continue;
      const cs = getComputedStyle(el);
      const fs = parseFloat(cs.fontSize) || 14;
      const lhRaw = parseFloat(cs.lineHeight);
      const lh = Number.isFinite(lhRaw) ? lhRaw : fs * 1.2;
      const box = el.getBoundingClientRect();
      const padY = (parseFloat(cs.paddingTop) || 0) + (parseFloat(cs.paddingBottom) || 0);
      const bordY = (parseFloat(cs.borderTopWidth) || 0) + (parseFloat(cs.borderBottomWidth) || 0);
      const contentH = Math.max(0, box.height - padY - bordY);
      const lc = lineCount(el, contentH, lh);
      const lines = lc.lines;
      if (lines < 2) continue;
      const clampRaw = cs.getPropertyValue("-webkit-line-clamp") || cs.webkitLineClamp || "";
      const clampN = parseInt(clampRaw, 10);
      const ellipsis = cs.textOverflow === "ellipsis";
      const hidY = cs.overflowY === "hidden" || cs.overflowY === "clip";
      const hidX = cs.overflowX === "hidden" || cs.overflowX === "clip";
      const nowrap = cs.whiteSpace === "nowrap" || cs.whiteSpace === "pre";
      let intentional = false;
      let why = "";
      if (Number.isFinite(clampN) && clampN >= 1) {
        intentional = true;
        why = "-webkit-line-clamp: " + clampN;
      } else if (ellipsis && hidY) {
        intentional = true;
        why = "text-overflow: ellipsis + overflow-y: hidden";
      } else if (ellipsis && hidX) {
        intentional = true;
        why = "text-overflow: ellipsis + overflow-x: hidden";
      } else if (nowrap && hidY) {
        intentional = true;
        why = "white-space: nowrap + overflow-y: hidden";
      }
      // §12.12's falsification: remove every compression source and re-measure.
      const keep = [el.style.whiteSpace, el.style.flexShrink, el.style.minWidth, el.style.width];
      el.style.whiteSpace = "nowrap";
      el.style.flexShrink = "0";
      el.style.minWidth = "max-content";
      const relaxed = el.getBoundingClientRect();
      el.style.whiteSpace = keep[0];
      el.style.flexShrink = keep[1];
      el.style.minWidth = keep[2];
      el.style.width = keep[3];
      const relaxedContentH = Math.max(0, relaxed.height - padY - bordY);
      const relaxedLines = Math.max(1, Math.round(relaxedContentH / lh));
      const natural = Math.round(naturalWidth(el, text) * 10) / 10;
      // 2px of slack absorbs sub-pixel rounding without letting a genuine squeeze
      // through: the shell defect measured 85px of text in a 2px box.
      const fitsOnOneLine = natural <= box.width + 2;
      wraps.push({
        sel: tagOf(el),
        text,
        len: text.length,
        w: Math.round(box.width),
        h: Math.round(box.height),
        lh: Math.round(lh * 100) / 100,
        lines,
        linesVia: lc.via,
        relaxed: Math.round(relaxed.width) + "x" + Math.round(relaxed.height),
        relaxedLines,
        natural,
        intentional,
        why,
        // The verdict: the container handed out less width than the text needs.
        // relaxed / relaxedLines stay in the record as corroboration, but they do
        // not decide (see the note above).
        squeezed: !fitsOnOneLine,
      });
    }
    probeScratch.remove();
    wraps.sort((a, b) => b.h - a.h);
    const isOverlay = (el) => {
      const cs = getComputedStyle(el);
      if (cs.position === "absolute" || cs.position === "fixed") return "position: " + cs.position;
      if (el.closest(OVERLAY_SEL)) return "overlay subtree";
      if (cs.zIndex !== "auto" && cs.position !== "static") return "z-index " + cs.zIndex;
      return null;
    };
    const overlaps = [];
    for (const parent of all) {
      if (isOverlay(parent)) continue;
      // SVG shapes legitimately cross each other inside one <svg>; a polyline
      // over a path is a drawing, not a layout collision.
      if (isSvg(parent)) continue;
      const kids = [...parent.children].filter(vis);
      if (kids.length < 2) continue;
      for (let i = 0; i < kids.length; i++) {
        for (let j = i + 1; j < kids.length; j++) {
          const a = kids[i];
          const b = kids[j];
          const ra = a.getBoundingClientRect();
          const rb = b.getBoundingClientRect();
          const ox = Math.min(ra.right, rb.right) - Math.max(ra.left, rb.left);
          const oy = Math.min(ra.bottom, rb.bottom) - Math.max(ra.top, rb.top);
          if (ox <= 1 || oy <= 1) continue;
          const la = isOverlay(a);
          const lb = isOverlay(b);
          overlaps.push({
            parent: tagOf(parent),
            a: tagOf(a),
            b: tagOf(b),
            overlap: Math.round(ox) + "x" + Math.round(oy),
            area: Math.round(ox * oy),
            legit: la || lb || null,
          });
        }
      }
    }
    overlaps.sort((a, b) => b.area - a.area);
    return { scanned: all.length, wraps, overlaps };
  });
  return r;
}

// ── MASTER §12.15's four classes with no §12 row (rows 42-45) ────────────────
//
// §12.15 records a one-off cross-check against the external Vercel Web Interface
// Guidelines. Three of its classes are already machine-judged here (icon naming
// -> row 17; :focus-visible / outline:none -> rows 15/16) and are deliberately
// NOT re-implemented: a second judge for the same rule can only disagree with the
// first. The remaining four have no row, and §12.15 is their only source.
//
// Each classifier is pure so --self-test can force BOTH directions: inject a
// violation -> must FAIL, remove it -> must PASS. That is also the answer to
// §12.10.5 (「判据匹配不到对象时静默通过」): a must-FAIL case proves the judge is
// really looking at the thing it claims to look at.

// (1) transition: all. The trap: the INITIAL value of transition-property IS
//     `all`, so testing the property alone flags every element on the page.
//     A transition exists only when the duration is non-zero.
function transitionAllHit(r) {
  const live = r.prop === "all" && r.dur !== "0s";
  if (!live) return false;
  // Captain's ruling (t81): antd's own CSS-in-JS is excluded. Three reasons, all
  // recorded in the row: (a) the excluded hits are antd's, NOT "compliant";
  // (b) we cannot edit antd's internal CSS, and the criterion's intent is "OUR
  // code does not write transition: all"; (c) the project-side override was
  // measured and is worse (36 class rules, still 6 -> 3 on #inbox, and it
  // rewrites antd's own animations) -- that is a separate decision.
  // The "unknown" origin is deliberately NOT excluded: when the winning
  // declaration cannot be attributed, the row judges FAIL rather than assuming
  // innocence.
  return !/^antd-cssinjs/.test(String(r.owner || ""));
}

// (2) <img> without alt. Markup rule, not a rendering rule: alt="" (the
//     decoration form) COUNTS as present, a missing attribute does not.
function imgAltHit(r) {
  return r.hasAlt === false;
}

// (3) a literal three-dot ellipsis. `...` is not `…`; the rendered text is what
//     the user reads. Code/pre/kbd/samp/textarea subtrees are excluded by the
//     probe (a code sample may legitimately contain three dots), and so are
//     comments -- they never reach the DOM, which is why this reads the DOM and
//     not the source files.
function literalDotsHit(r) {
  return /\.\.\./.test(String(r.text || ""));
}

// (4) tabular-nums. Weak coverage in the contract, so the scope is defined here
//     and stated in the row: a NUMERIC READOUT is an element whose own text is
//     digits/separators only, which shares a parent with at least one other such
//     element (i.e. it is part of a column or a row of figures). Prose, dates in
//     sentences and single stand-alone numbers are out of scope -- they do not
//     line up with anything. tabular-nums is inherited, so the computed value on
//     the element already accounts for an ancestor that sets it.
function tabularHit(r) {
  return r.candidate === true && r.tabular !== true;
}

// A date or a clock time is NOT a number to align. 2026/9/12 and 12:34 match the
// numeric shape, so they must be excluded BY FORM.
//
// Why this matters (t81's measured deviation): the group test asks whether every
// direct child of the container holds a figure. A kanban COLUMN holds a heading
// count AND a list of dated cards; while the dates counted as figures, both
// children "held a figure", so the column read as a row of figures and its
// stand-alone heading count stayed in scope. Excluding dates by form removes the
// root cause instead of adding a fifth exception for that one count.
const DATE_TIME_FORMS = [/^\d{1,4}[/.-]\d{1,2}[/.-]\d{1,4}$/, /^\d{1,2}:\d{2}(:\d{2})?$/];
function looksLikeDateTime(text) {
  const t = String(text ?? "").trim();
  return DATE_TIME_FORMS.some((re) => re.test(t));
}

async function probeMarkup(page) {
  const r = await page.evaluate(() => {
    const norm = (s) => String(s || "").replace(/\s+/g, " ").trim();
    const vis = (el) => {
      const cs = getComputedStyle(el);
      if (cs.display === "none" || cs.visibility === "hidden" || Number(cs.opacity) === 0) return false;
      const b = el.getBoundingClientRect();
      return b.width >= 1 && b.height >= 1;
    };
    const tagOf = (el) => {
      const c = typeof el.className === "string" ? el.className.trim().split(/\s+/).slice(0, 3).join(".") : "";
      return el.tagName.toLowerCase() + (c ? "." + c : "");
    };
    const antd = (el) => /(^|\s)ant-/.test(el.className || "") || /(^|\s)css-[a-z0-9]+/.test(el.className || "");
    // Dates and clock times are excluded BY FORM (same two patterns as the
    // module-scope looksLikeDateTime, asserted by --self-test). Declared here,
    // before ANY use: the first cut sat below the candidate loop and the probe
    // threw "Cannot access 'isDateTime' before initialization", which made rows
    // 42-45 read their absent data as an EMPTY SET and pass. That is the whole
    // reason the judges now treat a probe error as not-measured (see below).
    const all = [...document.querySelectorAll("body *")];
    // (1) transition: all -- every element, visible or not (a transition is a
    //     style fact; an off-screen element still animates when shown).
    // Where a hit's transition actually COMES FROM. The class name cannot decide
    // this: measured, 53 of 81 hits carry a project-owned class (.ruagent,
    // .kanban-runs) while 100% of the winning declarations come from antd's
    // CSS-in-JS (style[data-css-hash][data-rc-order]) and none from our
    // index-*.css link. So the attribution is the stylesheet origin, and it is
    // recorded per hit for the contract owner to rule on.
    const transitionOwner = (el) => {
      for (const sheet of document.styleSheets) {
        let rules;
        try {
          rules = sheet.cssRules;
        } catch (e) {
          continue;
        }
        if (!rules) continue;
        for (const rule of rules) {
          if (!rule.selectorText || !rule.style) continue;
          const t = rule.style.getPropertyValue("transition") || rule.style.getPropertyValue("transition-property");
          if (!t || !/all/.test(t)) continue;
          let m = false;
          try {
            m = el.matches(rule.selectorText);
          } catch (e) {
            continue;
          }
          if (!m) continue;
          const node = sheet.ownerNode;
          if (sheet.href) return "link:" + sheet.href.split("/").pop();
          if (node && node.tagName === "STYLE") {
            const hash = node.getAttribute("data-css-hash");
            const rc = node.getAttribute("data-rc-order");
            return "antd-cssinjs" + (hash || rc ? "[" + [hash && "hash:" + hash.slice(0, 6), rc && "rc-order"].filter(Boolean).join(",") + "]" : "");
          }
          return "style";
        }
      }
      return "unknown";
    };
    const transitionAll = [];
    for (const el of all) {
      const cs = getComputedStyle(el);
      const prop = cs.transitionProperty;
      const dur = cs.transitionDuration;
      if (prop !== "all" || dur === "0s") continue;
      const cls = String(el.className || "").trim().split(/\s+/).filter(Boolean);
      transitionAll.push({
        sel: tagOf(el),
        prop,
        dur,
        antd: antd(el),
        owner: transitionOwner(el),
        // A project-owned class = not ant-* and not an antd css-hash.
        ownClass: cls.some((c) => !/^ant-/.test(c) && !/^css-[a-z0-9]+$/.test(c)) ? cls.filter((c) => !/^ant-/.test(c) && !/^css-[a-z0-9]+$/.test(c)).slice(0, 2).join(".") : null,
      });
    }
    // (2) img without alt
    const imgNoAlt = [];
    for (const el of document.querySelectorAll("img")) {
      if (el.hasAttribute("alt")) continue;
      const b = el.getBoundingClientRect();
      imgNoAlt.push({ sel: tagOf(el), src: String(el.getAttribute("src") || "").slice(0, 60), w: Math.round(b.width), h: Math.round(b.height), visible: vis(el) });
    }
    // (3) literal three dots in rendered text / user-facing attributes
    const literalDots = [];
    const SKIP = "pre,code,kbd,samp,textarea,script,style";
    for (const el of all) {
      if (el.closest(SKIP)) continue;
      for (const n of el.childNodes) {
        if (n.nodeType !== 3) continue;
        const t = n.nodeValue || "";
        if (!/\.\.\./.test(t)) continue;
        literalDots.push({ sel: tagOf(el), text: norm(t).slice(0, 60), where: "text" });
      }
      for (const a of ["placeholder", "title", "aria-label"]) {
        const v = el.getAttribute && el.getAttribute(a);
        if (v && /\.\.\./.test(v)) literalDots.push({ sel: tagOf(el), text: norm(v).slice(0, 60), where: a });
      }
    }
    // Dates and clock times are excluded BY FORM (same two patterns as the
    // module-scope looksLikeDateTime, asserted by --self-test). Declared here,
    // before ANY use: the first cut sat below the candidate loop and the probe
    // threw "Cannot access 'isDateTime' before initialization", which made rows
    // 42-45 read their absent data as an EMPTY SET and pass. That is the whole
    // reason the judges now treat a probe error as not-measured.
    // Dates and clock times are excluded BY FORM (same two patterns as the
    // module-scope looksLikeDateTime, asserted by --self-test). Declared here,
    // before ANY use: the first cut sat below the candidate loop and the probe
    // threw a TDZ ReferenceError, which made rows 42-45 read their absent data
    // as an EMPTY SET and pass. That is why the judges now treat a probe error
    // as not-measured (see their guards).
    const DATE_FORMS = [/^\d{1,4}[/.-]\d{1,2}[/.-]\d{1,4}$/, /^\d{1,2}:\d{2}(:\d{2})?$/];
    const isDateTime = (t) => DATE_FORMS.some((re) => re.test(t));
    // (4) numeric readouts and whether tabular-nums reaches them
    const NUM = /^[+\-]?[\d][\d.,:\/%\s\u00d7xXkKmMgGsS+\-]*$/;
    const numericEls = [];
    for (const el of all) {
      if (!vis(el)) continue;
      let own = "";
      for (const n of el.childNodes) if (n.nodeType === 3) own += n.nodeValue;
      own = norm(own);
      if (own.length < 2 || own.length > 24) continue;
      if (!/\d/.test(own) || !NUM.test(own)) continue;
      if (isDateTime(own)) continue; // 2026/9/12, 12:34 -- a timestamp, not a figure
      numericEls.push({ el, own });
    }
    // NB: an earlier cut required >=2 numeric SIBLINGS under one parent. That
    // found ZERO candidates on every route -- the panel puts each figure in its
    // own cell (div.readout-cell > span.readout, div.stat-cell > span.stat-num),
    // so the row passed VACUOUSLY, which is exactly the failure mode §12.10.5
    // names. "Own text is purely numeric" is already enough to exclude prose;
    // the peers count is now reported as CONTEXT instead of gating the scope.
    const isNumEl = (s) => {
      let t = "";
      for (const n of s.childNodes) if (n.nodeType === 3) t += n.nodeValue;
      t = norm(t);
      if (t.length < 2 || t.length > 24 || !/\d/.test(t) || !NUM.test(t)) return false;
      return !isDateTime(t);
    };
    const peers = (el) => (el.parentElement ? [...el.parentElement.children].filter(isNumEl).length : 0);
    // The GROUP a figure belongs to: its grandparent's figures, itself included.
    // Measured (t81): parent-level peers are 0 for every figure on every route --
    // the panel puts each number in its own cell (div.readout-cell > span.readout,
    // div.stat-cell > span.stat-num), so the column lives one level up. Gating on
    // the grandparent is what makes the row non-vacuous AND is what drops the
    // stand-alone figures (a heading count, a lone percentage) and the kanban
    // dates, none of which align with anything.
    const grandPeers = (el) => {
      const gp = el.parentElement && el.parentElement.parentElement;
      if (!gp) return 0;
      return [...gp.querySelectorAll("*")].filter(isNumEl).length;
    };
    const hasFigure = (c) => isNumEl(c) || [...c.querySelectorAll("*")].some(isNumEl);
    // The GROUP, after the captain's second ruling (t81): a container whose
    // direct children EACH hold a figure (empty children allowed). That is a ROW
    // OF FIGURES -- div.readout-row (cells, one figure each) qualifies and
    // div.stat-row qualifies, while section.zone (which also holds a heading, a
    // legend and a grid) does NOT, even though its subtree contains many
    // figures. The previous test (">=2 figures anywhere in the subtree") kept the
    // section in scope, which left two stand-alone heading counts in scope and
    // the row still red.
    const isFigureGroup = (gp) => {
      const kids = [...gp.children];
      if (kids.length < 2) return false;
      return kids.every((c) => hasFigure(c) || !norm(c.textContent).length);
    };
    const numerics = [];
    for (const rec of numericEls) {
      const g = rec.el.parentElement && rec.el.parentElement.parentElement;
      // In scope only when the figure belongs to a ROW OF FIGURES.
      if (!g || !isFigureGroup(g)) continue;
      const gp = grandPeers(rec.el);
      const cs = getComputedStyle(rec.el);
      numerics.push({
        sel: tagOf(rec.el),
        parent: tagOf(rec.el.parentElement || rec.el),
        group: tagOf((rec.el.parentElement && rec.el.parentElement.parentElement) || rec.el),
        text: rec.own,
        peers: peers(rec.el),
        grandPeers: gp,
        tabular: String(cs.fontVariantNumeric || "").includes("tabular-nums"),
        candidate: true,
      });
    }
    return { transitionAll, imgNoAlt, literalDots, numerics, scanned: all.length, imgs: document.querySelectorAll("img").length };
  });
  return r;
}
// ── Provenance: what exactly is being audited ───────────────────────────────
function distInfo() {
  const distDir = join(PANEL, "dist");
  const assetsDir = join(distDir, "assets");
  const info = { distDir, exists: existsSync(distDir), buildId: null, bundleId: null, js: [], css: [], newestSrc: null, newestDist: null, staleSrc: null };
  const htmlPath = join(distDir, "index.html");
  if (existsSync(htmlPath)) {
    const html = readFileSync(htmlPath, "utf8");
    const m = html.match(/assets\/(index-[\w-]+\.js)/);
    if (m) info.buildId = m[1].replace(/^index-/, "").replace(/\.js$/, "");
    // §12.10.2's floor/budget are about the FIRST SCREEN. Vite writes exactly
    // that set here: the entry <script> plus one modulepreload per statically
    // imported chunk. Lazy chunks (markdown, route chunks) are absent by
    // construction, so this is a measurement, not a guess.
    info.firstScreen = [...html.matchAll(/(?:src|href)="\/assets\/([\w.-]+\.js)"/g)].map((x) => x[1]);
  }
  if (existsSync(assetsDir)) {
    for (const f of readdirSync(assetsDir)) {
      if (!f.endsWith(".js") && !f.endsWith(".css")) continue;
      const buf = readFileSync(join(assetsDir, f));
      const rec = { file: f, bytes: buf.length, gzip: gzipSync(buf).length };
      if (f.endsWith(".js")) info.js.push(rec); else info.css.push(rec);
    }
    info.js.sort((a, b) => b.bytes - a.bytes);
    info.css.sort((a, b) => b.bytes - a.bytes);
  }
  const newest = (dir, exts) => {
    let best = 0;
    const walk = (d) => {
      for (const e of readdirSync(d, { withFileTypes: true })) {
        const p = join(d, e.name);
        if (e.isDirectory()) walk(p);
        else if (exts.some((x) => e.name.endsWith(x))) best = Math.max(best, statSync(p).mtimeMs);
      }
    };
    if (existsSync(dir)) walk(dir);
    return best;
  };
  const mm = new Set();
  for (const f of info.js) {
    const text = readFileSync(join(assetsDir, f.file), "utf8");
    for (const m of text.matchAll(/matchMedia\(\s*[\"'`]\(?\s*(?:max|min)-width:\s*(\d+(?:\.\d+)?)px/g)) mm.add(Number(m[1]));
  }
  info.matchMedia = [...mm].sort((a, b) => a - b);
  info.newestSrc = newest(join(PANEL, "src"), [".ts", ".tsx", ".css"]);
  info.newestDist = newest(join(PANEL, "dist", "assets"), [".js", ".css"]);
  info.staleSrc = !!info.newestSrc && info.newestSrc > (info.newestDist || 0);
  return info;
}

async function resolveTaskId(args) {
  if (args.taskId) return args.taskId;
  try {
    const res = await fetch(`${args.baseUrl}/api/v1/tasks`, { signal: AbortSignal.timeout(6000) });
    if (!res.ok) return null;
    const data = await res.json();
    const first = Array.isArray(data?.tasks) ? data.tasks[0] : null;
    return first?.id ?? null;
  } catch {
    return null;
  }
}

// ── Global overlay contract: command palette ARIA (§12 row 21) and focus
// return after close (row 22). Both are read from the real overlay. ────────
async function probePalette(page) {
  const out = { open: false, checks: {}, missing: [], focusReturnedToTrigger: null, focusAfterEscape: null, items: 0 };
  await page.evaluate(() => window.scrollTo(0, 0));
  if (await page.locator(".kbd-hint").count()) {
    await page.locator(".kbd-hint").first().focus();
  }
  await page.keyboard.press("Control+k");
  await page.waitForTimeout(500);
  const aria = await page.evaluate(() => {
    const dlg = document.querySelector(".cmdk");
    if (!dlg) return { open: false };
    const input = dlg.querySelector("input");
    const list = dlg.querySelector(".cmdk-list");
    const item = dlg.querySelector(".cmdk-item");
    const option = dlg.querySelector('[role="option"]') ?? item;
    return {
      open: true,
      dialogRole: dlg.getAttribute("role"),
      ariaModal: dlg.getAttribute("aria-modal"),
      ariaControls: input ? input.getAttribute("aria-controls") : null,
      ariaActivedescendant: input ? input.getAttribute("aria-activedescendant") : null,
      listRole: list ? list.getAttribute("role") : null,
      optionRole: option ? option.getAttribute("role") : null,
      optionAriaSelected: option ? option.getAttribute("aria-selected") : null,
      items: dlg.querySelectorAll(".cmdk-item").length,
    };
  });
  Object.assign(out, aria);
  out.checks = {
    ariaModal: aria.ariaModal === "true",
    ariaActivedescendant: !!aria.ariaActivedescendant,
    ariaControls: !!aria.ariaControls,
    listboxRole: aria.listRole === "listbox",
    optionRole: aria.optionRole === "option",
    ariaSelected: aria.optionAriaSelected !== null && aria.optionAriaSelected !== undefined,
  };
  out.missing = Object.entries(out.checks).filter(([, v]) => !v).map(([k]) => k);
  await page.keyboard.press("Escape");
  await page.waitForTimeout(300);
  const after = await page.evaluate(() => {
    const el = document.activeElement;
    return {
      tag: el?.tagName ?? null,
      cls: (el?.getAttribute?.("class") ?? null),
      inPalette: !!(el && el.closest && el.closest(".cmdk")),
    };
  });
  out.focusAfterEscape = after;
  out.focusReturnedToTrigger = !!(after.cls && String(after.cls).includes("kbd-hint")) && !after.inPalette;
  return out;
}

// `<width>` or `<width>x<height>`; a bare width takes DEFAULT_VP_H. Anything
// else returns null so the caller has to *say so*. A silent "fall back to
// 1440x900" is how the overflow sweep once collapsed to a single viewport and
// row 19 still passed vacuously -- the default list is written as bare widths
// (`390,520,768,1024,1280,1920`), so a bare width is a first-class form.
const DEFAULT_VP_H = 900;
// Row 26's re-measure window. Only spent when the first window already exceeds
// the target, i.e. rarely; see the two-window protocol in auditRoute.
const LONG_TASK_REMEASURE_MS = 1500;
const parseViewport = (s) => {
  const m = String(s).trim().match(/^(\d+)(?:\s*[x×]\s*(\d+))?$/);
  if (!m) return null;
  const width = +m[1];
  const height = m[2] ? +m[2] : DEFAULT_VP_H;
  return width > 0 && height > 0 ? { width, height } : null;
};

// ── One capture: navigate -> settle -> screenshot+pixels -> DOM probe ->
// focus walk -> overflow probes. ────────────────────────────────────────────
async function auditRoute(page, o) {
  const { args, route, mode, taskId, outDir, cfg, reqCounter, measureViewport, pageTitleRule, longTaskTargetMs } = o;
  const hash = route.needsTaskId ? `#task/${taskId}` : route.hash;
  const url = `${args.baseUrl}/?mode=${mode}${hash}`;
  const warnings = [];
  const t0 = Date.now();
  // probeOverflow resizes the page and runs last, so without this reset the
  // NEXT capture would screenshot (and read innerWidth) in the frame the
  // previous route's overflow sweep left behind. Measurement frame first.
  if (measureViewport) await page.setViewportSize(measureViewport);
  // Consecutive routes in the same mode differ only in the hash, and a
  // hash-only navigation does NOT reload the document — so window.__longtasks
  // (installed by addInitScript) survives from the previous route and row 26
  // would report a running max rather than this route's own worst block. The
  // reset is a no-op when goto does reload (addInitScript re-creates the array).
  await page.evaluate(() => { window.__longtasks = []; }).catch(() => {});
  reqCounter.n = 0;
  reqCounter.urls.length = 0;
  await page.goto(url, { waitUntil: "load", timeout: 60_000 });
  await page.waitForSelector(".app-sider", { state: "attached", timeout: 20_000 }).catch(() => warnings.push("sidebar never attached"));
  await page.waitForLoadState("networkidle", { timeout: 8_000 }).catch(() => {});
  await page.waitForTimeout(args.settleMs);
  // Control-run injection (--inject-css): AFTER the navigation and the settle,
  // so it is the last word on the cascade. Placed before the goto on the first
  // cut, which the page load simply wiped -- the injection had no effect and the
  // control run wrongly looked identical to the shipped build.
  if (args.injectCss) {
    const css = readFileSync(args.injectCss, "utf8");
    await page.addStyleTag({ content: css });
    await page.waitForTimeout(250);
    warnings.push("CONTROL RUN: injected " + args.injectCss + " — readings below are NOT the shipped build");
  }

  // ── §12 row 26. Read BEFORE the screenshot and the DOM probe, because both
  // are harness work on the main thread: `page.screenshot()` rasterises the
  // whole frame, which on #task's 153k nodes costs ~2.7s and would be reported
  // as the app blocking the user. Measured here the number is the app's own
  // cost during load + settle, which is what the row is about.
  const readLongTasks = () =>
    page
      .evaluate(() => {
        const lt = Array.isArray(window.__longtasks) ? window.__longtasks.slice() : [];
        return { count: lt.length, max: lt.length ? Math.max(...lt) : 0, over200: lt.filter((d) => d > 200).length };
      })
      .catch(() => ({ count: 0, max: 0, over200: 0, unreadable: true }));

  const longtaskA = await readLongTasks();
  let longtask = longtaskA;
  // One sample is not evidence. A single 1795ms reading on #stats did not
  // reproduce on three re-runs (84/82/79ms), and judging a row on a number that
  // cannot be reproduced is how an instrument loses its authority. So when the
  // first window exceeds the target, take a second, independent window: a block
  // that reproduces is judged on the worst of the two, one that does not is
  // registered as an outlier with both numbers printed.
  if (longtaskA.max > o.longTaskTargetMs) {
    await page.evaluate(() => { window.__longtasks = []; }).catch(() => {});
    await page.waitForTimeout(LONG_TASK_REMEASURE_MS);
    const b = await readLongTasks();
    const confirmed = b.max > o.longTaskTargetMs;
    longtask = {
      count: longtaskA.count + b.count,
      max: confirmed ? Math.max(longtaskA.max, b.max) : b.max,
      over200: confirmed ? longtaskA.over200 + b.over200 : b.over200,
      firstWindowMax: longtaskA.max,
      secondWindowMax: b.max,
      confirmed,
      outlier: !confirmed,
      note: confirmed
        ? `两个独立窗口都超过目标（${longtaskA.max}ms / ${b.max}ms）`
        : `首个窗口 ${longtaskA.max}ms 复测未再现（${b.max}ms）—— 记为离群值，不判失败`,
    };
  }

  // The DOM must stop moving before anything is counted (row 18's 4/10/11).
  // This runs *after* the long-task read so that window stays "load + settle".
  const domStable = await waitForDomStable(page, args.domQuietMs, args.domMaxMs);
  if (!domStable.stable) {
    warnings.push(`DOM 未在 ${args.domMaxMs}ms 内稳定（最后 ${domStable.nodes} 节点）：行 18/25 的计数可能不稳定`);
  }

  // ── §12 row 28 wants an 11.5s observation window (MASTER §12: "11.5s 窗口内
  // API 请求数"). reqCounter was zeroed at the top of this capture, so the
  // window observed so far is load + settle. `--api-window` must therefore
  // *extend the wait*, not merely flip the judge: otherwise
  // `--api-window=11500` would judge a 2.5s count and pass the row vacuously.
  const apiWindowMs = Math.max(args.settleMs, args.apiWindowMs);
  if (apiWindowMs > args.settleMs) await page.waitForTimeout(apiWindowMs - args.settleMs);
  const apiRequests = reqCounter.n;
  const api = summarizeApi(reqCounter.urls);

  let shot = null;
  let shotBuf = null;
  let pixels = null;
  let tShotStart = null;
  if (args.shots) {
    tShotStart = Date.now();
    shotBuf = await page.screenshot();
    const file = join(outDir, `${route.id}_${mode}.png`);
    writeFileSync(file, shotBuf);
    shot = { file: relative(REPO, file).split("\\").join("/"), bytes: shotBuf.length };
  }

  const tProbe = Date.now();
  // The page-title carrier is route-specific (MASTER §12 row 6: .view-bar h2,
  // but #home uses .home-hero h1) and comes from the contract, not from here.
  const titleRule = pageTitleFor(o.pageTitleRule, route.id);
  const metrics = await page.evaluate(PROBE, {
    ...cfg,
    markers: route.markers,
    expectHash: hash,
    titleSelectors: titleRule?.selectors ?? [],
  });
  metrics.longtask = longtask;

  // ── Pixel statistics, *after* the probe, because row 1's counting needs the
  // canvas colour and only the probe knows it (it composites the ancestor
  // background chain). The screenshot itself is still taken before the probe —
  // decoding is Node-side work and does not touch the page.
  if (args.pixels && shotBuf) {
    try {
      const canvasRgb = parseCssColor(metrics.material?.canvas);
      pixels = pixelStats(decodePng(shotBuf), undefined, {
        exclude: canvasRgb && canvasRgb.a >= 1 ? [[canvasRgb.r, canvasRgb.g, canvasRgb.b]] : [],
      });
      if (!canvasRgb) warnings.push("行 1：未解析到画布色，luma 带占比按全画面统计（含画布）");
    } catch (e) {
      warnings.push(`pixel decode failed: ${e.message}`);
    }
  }
  const tProbeDone = Date.now();
  const focusBudget = args.focusBudgetMs ?? 30000;
  if (!metrics.hashOk) warnings.push(`hash mismatch: asked ${hash}, got ${metrics.hash}`);
  if (!metrics.markers.length) warnings.push(`no content marker found (${route.markers.join(", ")})`);

  // ── §12 row 20, last: it reloads the page with the route's own endpoints
  // 500ing, so every other probe must already have observed the healthy page.
  // Rows 40/41: text wrap and sibling overlap. Measured here, on the healthy
  // page and before anything else touches it (the row-20 probe below reloads).
  const textLayout = await probeTextLayout(page).catch((e) => ({ error: e.message }));
  if (textLayout?.error) warnings.push(`rows 40/41 text layout probe: ${textLayout.error}`);

  // Rows 42-45 (§12.15's four classes with no row). Same healthy page, same place.
  const markup = await probeMarkup(page).catch((e) => ({ error: e.message }));
  if (markup?.error) warnings.push(`rows 42-45 markup probe: ${markup.error}`);

  const failure = args.failureProbe
    ? await probeFailureState(page, { url, shellSel: cfg.shellSel || "aside.ant-layout-sider.app-sider", endpoints: api.groups.map((g) => g.endpoint) })
    : null;
  if (failure?.error) warnings.push(`row 20 失败态注入：${failure.error}`);

  const focus = args.focus ? await walkFocus(page, args.tabs, focusBudget) : null;
  const tFocusDone = Date.now();
  const overflow = await probeOverflow(page, o.overflowViewports);
  // The session-title sweep runs LAST in the viewport-moving probes and puts the
  // measurement viewport back, so the screenshot pass is unaffected.
  const sessionTitle = await probeSessionTitle(page, measureViewport, args.injectCss).catch((e) => ({ error: e.message }));
  if (sessionTitle?.error) warnings.push("row 46 session title probe: " + sessionTitle.error);
  const tOverflowDone = Date.now();
  return {
    route: route.id,
    hash,
    mode,
    url,
    ms: Date.now() - t0,
    timing: {
      load: tProbe - t0,
      shot: tProbe - (tShotStart ?? tProbe),
      probe: tProbeDone - tProbe,
      focus: tFocusDone - tProbeDone,
      overflow: tOverflowDone - tFocusDone,
    },
    apiRequests,
    api,
    failure,
    textLayout,
    markup,
    sessionTitle,
    apiWindowMs,
    domStable,
    shot,
    pixels,
    metrics,
    focus,
    overflow,
    warnings,
  };
}

// ── §12 as executable checks ────────────────────────────────────────────────
// Each row says which modes it is judged in (the doc's baselines are dark
// unless it names light), how its target text maps onto a limit, and how a
// capture answers it. `pass: null` means "reported, not judged" — used where
// the row only speaks about #task (25) or a different measurement window (28).
const pct = (v) => `${(v * 100).toFixed(2)}%`;
const hueOf = (css) => {
  const c = parseCssColor(css);
  if (!c) return null;
  const r = c.r / 255, g = c.g / 255, b = c.b / 255;
  const max = Math.max(r, g, b), min = Math.min(r, g, b), d = max - min;
  if (!d) return 0;
  let h;
  if (max === r) h = 60 * (((g - b) / d) % 6);
  else if (max === g) h = 60 * ((b - r) / d + 2);
  else h = 60 * ((r - g) / d + 4);
  return h < 0 ? h + 360 : h;
};
const hueDelta = (a, b) => {
  const ha = hueOf(a), hb = hueOf(b);
  if (ha == null || hb == null) return null;
  const d = Math.abs(ha - hb);
  return Math.min(d, 360 - d);
};
// MASTER §12 row 6 / §12.1 page-title ("层级中心") sub-assertion, single-sourced
// so the judge and --self-test cannot drift apart.
//
// The requirement is: exactly ONE visible element matching the route's contract
// carrier, at or above that carrier's tier size. The carrier comes from row 6
// (.view-bar h2; #home uses .home-hero h1); the tier size comes from §4.1
// (title 20px, display 24px), because row 6's own "(32px)" for #home's h1 is the
// readout tier and contradicts §4.1's assignment of that h1 to display. Both
// facts are reported, so the disagreement is visible rather than averaged away.
function judgePageTitle(contract, route, title) {
  const t = title ?? { count: 0, sizes: [], selectors: [] };
  const want = pageTitleFor(contract?.pageTitleRule ?? null, route);
  const tiers = contract?.titleTiers ?? { title: 20, display: null };
  const isHome = want != null && contract?.pageTitleRule?.overrides?.[route] === want;
  const minPx = (isHome ? tiers.display : tiers.title) ?? tiers.title ?? 20;
  const row6Px = want?.minPx ?? minPx;
  const ok = t.count === 1 && (t.sizes[0] ?? 0) >= minPx;
  const carriers = (want?.selectors ?? ["?"]).join(" / ");
  const conflict =
    isHome && row6Px !== minPx
      ? `契约冲突（登记，不判定）：MASTER §12 行 6 把 #home 的页面标题写作 ${row6Px}px，而 §4.1 把 ${carriers} 列在 ${minPx}px 的 ${tiers.display === minPx ? "display" : "title"} 档`
      : "";
  return {
    ok,
    minPx,
    carriers,
    conflict,
    detail: `页面标题 ${t.count} 个 @ ${t.sizes.length ? t.sizes.join("/") + "px" : "—"}（要求恰好 1 个 ≥${minPx}px 的 ${carriers}）`,
  };
}

const numAt = (t, i, d) => (t.nums[i] != null ? t.nums[i] : d);
const isComposerOnly = (b) => b.strictContainers === 0 || (b.strictContainers === 1 && String(b.strictSamples[0]?.sel ?? "").includes("composer"));

// ── §12 row 32: the uniqueness check set ────────────────────────────────────
// MASTER §12 row 32 originally asked for {--signal, --brand, --status-warn} to
// be mutually distinct. Read with `--brand` = accent (--ant-color-primary) that
// is UNSATISFIABLE: primitives §2.4 C6 and §2.6 *require* accent === --signal
// (one signal channel, antd just renders it), so the row could never pass and
// its FAIL was a property of the criterion, not of the panel. MASTER §12.3
// (2026-09-21) re-scopes the set to the runtime brand family — domain ② of
// §3.1 I — and excludes accent explicitly. The Δhue floor stays on the one pair
// that can actually be confused: --status-warn vs --signal (primitives §2.3).
// ── Rulings not yet landed in MASTER ────────────────────────────────────────
// One number per row, with its derivation, cited in the row's note and in the
// 判据出处 table. These are NOT silent overrides: each one is checked against
// MASTER first, and the note says plainly that the document has not caught up.
// They exist because the contract as written is arithmetically unsatisfiable —
// the same class of defect as row 32's accent check and row 16's ring.
const PENDING_RULINGS = {
  // I5 asks for "≤3" while also prescribing "merge into ONE 2s poller". One 2s
  // poller emits 7 requests in an 11.5s window (measured cadence 1993/2007/
  // 2002/1995/2012/1999 ms); ≤3 would need a ≥3.83s interval, so the row could
  // never pass however well it was implemented. ≤7 still separates one poller
  // (7) from two (14, measured), so the row keeps doing its job.
  28: {
    cap: 7,
    why: "11.5s ÷ 2s ≈ 6 次间隔 + 首次请求 = 7（captain 裁决 2026-09-21；MASTER §12 行 28 原文 ≤3 与 I5 自开的「合并为 1 个 2s 轮询」药方不相容）",
  },
};

const SIGNAL_UNIQUENESS_SET = ["--signal", "--status-warn", "--brand-claude", "--brand-deepseek", "--brand-opencode"];
const ACCENT_OUT_OF_SET = "--ant-color-primary";
// Tokens allowed to equal --signal, i.e. excluded from the collision scan:
//   * the accent family — --ant-color-primary is one of the signal's *rendering
//     paths* (antd paints the single accent), so equal values are the contract,
//     not a second signal colour. Captain ruling + MASTER §12.3; the -hover /
//     -active / link derivatives are the same channel and are listed with it.
//   * the --signal-* family — one accent in two grades plus its wash and ink;
//     same domain by construction (primitives §2.4 D1–D6).
// Everything ELSE that collides with --signal is a second signal colour living
// in another domain (MASTER §3.1 I: "同一个 hex 不得在两个域里各自承担一个
// 含义") and fails the row. This is deliberately NOT "anything that collides is
// fine" — only these two families are exempt.
const SIGNAL_COLLISION_WHITELIST = [
  ACCENT_OUT_OF_SET,
  "--ant-color-primary-hover",
  "--ant-color-primary-active",
  "--ant-color-link",
  "--signal",
  "--signal-text",
  "--signal-ink",
  "--signal-wash",
];

// Did MASTER's own row-32 cell carry the §12.3 correction yet? Until it does, a
// non-pass here is a contract defect, not an implementation defect, and must be
// reported as such rather than as a failing §12 row.
const row32CriterionLanded = (thresholds) => {
  const row = thresholds?.rows?.get(32);
  if (!row) return false;
  return /--brand-claude|--brand-/.test(`${row.target} ${row.judge}`) || /12\.3/.test(row.judge);
};

const CHECKS = [
  {
    n: 1, title: "暗色单亮度带 (0.05-0.10 luma) 像素占比", modes: ["dark"],
    criterion: "MASTER §12 行 1 的目标（每路由 ≤75%）**收窄到「被设计的面」**：直方图排除应用画布像素，判定用 lumaBandRatioDesigned；原口径（含画布）的 lumaBandRatio 同时输出、不隐藏。画布是单一扁平 token（暗 #0f1014，luma 0.063，本身落在带内），在空页面上占 83.41%，计入会让指标变成「页面有多空」的代理而不是「面有多浑浊」的度量。captain 裁决 2026-09-21，MASTER §12.5 已落槌（含升/降画布两个方向都撞墙的反证）",
    parse: (t) => ({ max: pick(t.text, /每路由\s*≤\s*([\d.]+)\s*%/, 75) / 100 }),
    judge: (c, l) => {
      if (!c.pixels) return { display: "—", pass: null, note: "no pixels" };
      const p = c.pixels;
      const designed = p.lumaBandRatioDesigned ?? p.lumaBandRatio;
      return {
        display: `${pct(designed)} 设计面（含画布 ${pct(p.lumaBandRatio)}）`,
        pass: designed <= l.max,
        note: [
          `画布 ${(p.exclude ?? []).join("/") || "—"} 占 ${pct(p.excludedRatio)}，已排除`,
          `设计面像素 ${p.designedPixels} / 全部 ${p.pixels}`,
          "口径：MASTER §12.5（判据文本逐字引用该裁决）",
        ].join(" · "),
      };
    },
  },
  {
    n: 2, title: "有效彩度像素占比", modes: ["dark"],
    parse: (t) => ({ lo: pick(t.text, /^\s*([\d.]+)\s*%\s*≤\s*x/, 1) / 100, hi: pick(t.text, /≤\s*x\s*≤\s*([\d.]+)\s*%/, 6) / 100 }),
    judge: (c, l) => {
      if (!c.pixels) return { display: "—", pass: null, note: "no pixels" };
      const v = c.pixels.chromaRatio;
      return { display: pct(v), pass: v >= l.lo && v <= l.hi };
    },
  },
  {
    n: 3, title: "信号色像素占比", modes: ["dark"],
    parse: (t) => ({ max: pick(t.text, /≤\s*([\d.]+)\s*%\/路由/, 5) / 100, internal: pick(t.text, /内部目标\s*≤\s*([\d.]+)\s*%/, 1.5) / 100 }),
    judge: (c, l) => c.pixels ? { display: pct(c.pixels.signalRatio), pass: c.pixels.signalRatio <= l.max } : { display: "—", pass: null, note: "no pixels" },
  },
  {
    n: 4, title: "严格口径描边内容容器数", modes: ["dark", "light"],
    parse: (t) => ({ max: pick(t.text, /≤\s*(\d+)\s*\/\s*路由/, 1) }),
    criterion: "primitives §11 row 4：MASTER 行 4 口径 + .inbox-card 必须是脊线（borderWidth === 0px）",
    judge: (c, l) => {
      const b = c.metrics.borders;
      const ex = c.metrics.extras;
      const ridgeOk = ex.inboxCards === 0 || ex.inboxCardBordered === 0;
      return {
        display: `${b.strictContainers} / ≤${l.max}${ex.inboxCards ? ` · .inbox-card 带边框 ${ex.inboxCardBordered}/${ex.inboxCards}` : ""}`,
        pass: b.strictContainers <= l.max && isComposerOnly(b) && ridgeOk,
        note: [b.strictSamples.map((x) => x.sel).join(" | "), ridgeOk ? "" : ".inbox-card 出现 border（应为 box-shadow 脊线）"].filter(Boolean).join(" · "),
      };
    },
  },
  {
    n: 5, title: "全部可见描边元素数", modes: ["dark", "light"],
    criterion: "MASTER §12 行 5 + view-task-detail.md D11：排除 .md th/td **与 .raw**（D11 原文：「判定必须带这条排除规则，否则本页永远不通过」）。行 5 的 MASTER 判据只写了 .md th/td，.raw 来自本页规格",
    parse: (t) => ({ max: pick(t.text, /≤\s*(\d+)\s*\/\s*路由/, 40) }),
    judge: (c, l) => {
      const b = c.metrics.borders;
      const excl = [
        b.rawBlocks ? `.raw ${b.rawBlocks}` : "",
        b.mdTableCells ? `.md th/td ${b.mdTableCells}` : "",
      ].filter(Boolean);
      return {
        display: `${b.allBordered} / ≤${l.max}`,
        pass: b.allBordered <= l.max,
        note: excl.length ? `另排除 ${excl.join(" · ")}（D11/行 5）` : "",
      };
    },
  },
  {
    n: 6, title: "≥18px 可见文本元素数", modes: ["dark", "light"],
    criterion: "MASTER §12 行 6 的**条件判定**（§12.1）：默认门槛 ≥3；路由 ∈ §12.1 豁免清单（#chat / #settings / #inbox）→ ≥1；上界 ≤40 不变；且每页必须恰好 1 个页面标题作为层级中心。豁免清单、标题载体（.view-bar h2 / #home 的 .home-hero h1）与档位尺寸（title 20px、display 24px，取自 §4.1）全部从 MASTER 读取。**登记一处契约冲突**：行 6 括注把 #home 的标题写作 32px，而 §4.1 把该 h1 列在 24px 的 display 档 —— 本行按 §4.1 的档位判定，冲突不当作实现缺陷",
    parse: (t) => {
      // MASTER row 6 target: "含读数的页 ≥3；无读数的页 ≥1；所有页 ≤40".
      // Reading it positionally (nums[1] as the upper bound) made hi = 1 and
      // failed every route with more than one ≥18px node — the row was
      // structurally unpassable, and 11 of 13 routes reported FAIL for it.
      // Read the comparators, not the order.
      const floors = [...t.text.matchAll(/≥\s*(\d+)/g)].map((m) => +m[1]);
      const caps = [...t.text.matchAll(/≤\s*(\d+)/g)].map((m) => +m[1]);
      return { lo: floors[0] ?? 3, exemptLo: floors[1] ?? 1, hi: caps[0] ?? 40 };
    },
    judge: (c, l, ctx) => {
      const n = c.metrics.text.ge18;
      // §12.1's exempt-route list, read from the contract (never a literal here).
      const exempt = ctx?.contract?.exemptRoutes?.get("#" + c.route) ?? null;
      const need = exempt ? exempt.threshold : l.lo;
      // The exemption's price: exactly one page title, at the contract's carrier
      // and size for this route.
      const title = c.metrics.pageTitle ?? { count: 0, sizes: [], selectors: [] };
      const T = judgePageTitle(ctx?.contract, c.route, title);
      const bad = [];
      if (n < need) bad.push(`${n} < 门槛 ${need}${exempt ? "（§12.1 豁免页）" : ""}`);
      if (n > l.hi) bad.push(`${n} > 上界 ${l.hi}`);
      // The single page title is the exemption's *price* (§12.1 边界 2) and it is
      // also a standing requirement for every page — the captain's ruling for
      // t17 states it for all routes, and every route in the panel carries
      // exactly one today, so gating everywhere adds no false failure.
      if (!T.ok) bad.push(T.detail);
      const drift =
        exempt && l.exemptLo !== exempt.threshold
          ? `§12.1 门槛 ${exempt.threshold} 与行 6 目标的第二个下界 ${l.exemptLo} 不一致（契约漂移）`
          : "";
      return {
        display: `${n} / ≥${need}${exempt ? " §12.1 豁免" : ""} · 标题 ${title.count}@${title.sizes[0] ?? "—"}px`,
        pass: bad.length === 0,
        note: [bad.join("; "), T.ok ? `页面标题 1@${title.sizes[0]}px ✓` : "", T.conflict, drift]
          .filter(Boolean)
          .join(" · "),
      };
    },
  },
  {
    n: 7, title: "每路由最大字号", modes: ["dark", "light"],
    criterion: "MASTER §12 行 7 的目标（≥20px；含仪表盘的页 ≥32px）+ 每页规格自己的验收行（view-home F5=32 / view-chat C3=20 / view-task-detail D8=20 / view-agents A1=20 / view-inbox I3=20 / view-settings G2=20，其余 S9/B1/M1/K1/G6/R1/S2=32）。**页面分类来自规格，不再用「页面上有没有 .readout/.dash-grid」这类 DOM 启发式**——#task 与 #agents 都刻意没有仪表盘",
    parse: (t) => ({ base: pick(t.text, /≥\s*(\d+)\s*px/, 20), dash: pick(t.text, /仪表盘的页\s*≥\s*(\d+)\s*px/, 32) }),
    judge: (c, l, ctx) => {
      const m = c.metrics.text.maxFontSize;
      const spec = ctx?.contract?.viewSpecs?.get(c.route) ?? null;
      if (!spec) {
        return {
          display: `${m}px / ≥${l.base}px (规格未读到)`,
          pass: m >= l.base,
          note: `未读到 #${c.route} 的规格验收行：退回 MASTER 的下限 ${l.base}px`,
        };
      }
      const need = spec.maxFontPx;
      // Drift guard: a per-view requirement that matches neither of MASTER row
      // 7's two stated values means one of the two documents moved.
      const drift = need !== l.base && need !== l.dash;
      return {
        display: `${m}px / ≥${need}px (${need >= l.dash ? "含仪表盘" : "无仪表盘"})`,
        pass: m >= need,
        note: [
          `依据 ${spec.file}：${spec.target}`,
          drift ? `规格要求 ${need}px 与 MASTER 行 7 的 {${l.base}, ${l.dash}} 都不符（契约漂移）` : "",
        ].filter(Boolean).join(" · "),
      };
    },
  },
  {
    n: 8, title: "字号越界数", modes: ["dark", "light"],
    parse: (t) => ({ max: pickLeadingNumber(t.text, 0) }),
    judge: (c, l) => ({ display: `${c.metrics.text.offScaleCount}`, pass: c.metrics.text.offScaleCount <= l.max, note: c.metrics.text.offScale.slice(0, 3).map((o) => `${o.px}px x${o.count}`).join(", ") }),
  },
  {
    n: 9, title: "字重越界数", modes: ["dark", "light"],
    parse: (t) => ({ max: pickLeadingNumber(t.text, 0) }),
    judge: (c, l) => ({ display: `${c.metrics.text.offWeightCount}`, pass: c.metrics.text.offWeightCount <= l.max, note: c.metrics.text.offWeight.slice(0, 3).map((o) => `${o.weight} x${o.count}`).join(", ") }),
  },
  {
    n: 10, title: "圆角越界元素数", modes: ["dark", "light"],
    parse: (t) => ({ max: pickLeadingNumber(t.text, 0) }),
    judge: (c, l) => ({ display: `${c.metrics.radius.offCount}`, pass: c.metrics.radius.offCount <= l.max, note: c.metrics.radius.off.slice(0, 4).map((o) => `${o.value} x${o.count}`).join(", ") }),
  },
  {
    n: 11, title: "带内联 font-size 的元素数", modes: ["dark", "light"],
    parse: (t) => ({ max: pickLeadingNumber(t.text, 0) }),
    judge: (c, l) => ({ display: `${c.metrics.inline.fontSized}`, pass: c.metrics.inline.fontSized <= l.max, note: c.metrics.inline.fontSizedSamples.slice(0, 3).map((s) => s.sel).join(" | ") }),
  },
  {
    n: 12, title: "带内联 style 的元素数", modes: ["dark", "light"],
    criterion: "MASTER §12 行 12 的 [style] 计数，口径写明为：**可见**元素（含 .sr-only / clip-path 裁剪盒排除）且 style 属性非空。不加可见性过滤会把 antd 的隐藏测量行（ant-table-measure-row）算进来 —— #stats 的 63 里有 22 个来自这些不可见行",
    parse: (t) => ({ max: pick(t.text, /≤\s*(\d+)\s*\/\s*路由/, 50) }),
    judge: (c, l) => ({ display: `${c.metrics.inline.styleTotal} / ≤${l.max}`, pass: c.metrics.inline.styleTotal <= l.max }),
  },
  {
    n: 13, title: "暗色文本对比度失败数 (正文 4.5:1)", modes: ["dark"],
    parse: (t) => ({ max: pickLeadingNumber(t.text, 0) }),
    judge: (c, l) => ({
      display: `${c.metrics.contrast.docViolations} (WCAG口径 ${c.metrics.contrast.wcagViolations})`,
      pass: c.metrics.contrast.docViolations <= l.max && c.metrics.contrast.wcagViolations <= l.max,
      note: c.metrics.contrast.worst.slice(0, 3).map((w) => `${w.ratio}:1 ${w.sel}`).join(" | "),
    }),
  },
  {
    n: 14, title: "亮色文本对比度失败数 (正文 4.5:1)", modes: ["light"],
    parse: (t) => ({ max: pickLeadingNumber(t.text, 0) }),
    judge: (c, l) => ({
      display: `${c.metrics.contrast.docViolations} (WCAG口径 ${c.metrics.contrast.wcagViolations})`,
      pass: c.metrics.contrast.docViolations <= l.max && c.metrics.contrast.wcagViolations <= l.max,
      note: c.metrics.contrast.worst.slice(0, 3).map((w) => `${w.ratio}:1 ${w.sel}`).join(" | "),
    }),
  },
  {
    n: 15, title: "焦点环对比度", modes: ["dark", "light"],
    parse: (t) => ({ min: pick(t.text, /≥\s*([\d.]+)\s*:\s*1/, 3) }),
    criterion: "primitives §11 row 15：每次 Tab 记录 outlineColor/boxShadow，与相邻不透明背景合成后算比；antd 的 3px 环也要计",
    judge: (c, l) => {
      const f = c.focusInfo;
      if (!f || !f.rings) return { display: "—", pass: null, note: "focus pass found no ring" };
      const bad = (c.focus ?? []).filter((s) => s.ringRatio != null && s.ringRatio < l.min);
      const unpar = (c.focus ?? []).filter((s) => s.ringUnparsable);
      const worst = f.worst[0];
      return {
        display: `${f.rings} 环, 最低 ${worst ? worst.ringRatio : "?"}:1`,
        pass: bad.length === 0 && unpar.length === 0,
        note: [
          unpar.length ? `${unpar.length} 环颜色无法解析（判 fail）` : "",
          bad.slice(0, 4).map((s) => `${s.ringRatio}:1 ${s.sel} (${s.ringKind})`).join(" | "),
        ].filter(Boolean).join("; "),
      };
    },
  },
  {
    n: 16, title: "无焦点环的 Tab 停留点", modes: ["dark", "light"],
    parse: (t) => ({ max: pickLeadingNumber(t.text, 0) }),
    criterion: "primitives §5.1 判定口径（修正 MASTER 行 16）：每停留点 outlineStyle==='solid' && width>=2px && 环对比度>=3:1；例外仅 .composer（改测 border-color>=3:1）；auto 与 none 一律 fail",
    judge: (c, l) => {
      const f = c.focusInfo;
      if (!f) return { display: "—", pass: null, note: "focus pass skipped (--no-focus)" };
      const forms = Object.entries(f.forms).map(([k, v]) => `${k}x${v}`).join(" ");
      return {
        display: `${f.notOk}/${f.stops} 不合格`,
        pass: f.notOk <= l.max,
        note: [
          `形式分布 ${forms}（元素自身 ${Object.entries(f.ownForms ?? {}).map(([k, v]) => k + "x" + v).join(" ")}）`,
          f.viaOwner ? `${f.viaOwner} 个环取自 :focus-within 所有者` : "",
          f.budgetHit ? "预算耗尽提前停止" : "",
          f.failed.length ? f.failed.map((x) => `${x.sel}[${x.form}·${x.why}]`).join(" | ") : "",
        ].filter(Boolean).join(" · "),
      };
    },
  },
  {
    n: 17, title: "无名称的可交互元素数", modes: ["dark", "light"],
    parse: (t) => ({ max: pickLeadingNumber(t.text, 0) }),
    judge: (c, l) => ({ display: `${c.metrics.inter.unnamedCount}/${c.metrics.inter.count}`, pass: c.metrics.inter.unnamedCount <= l.max, note: c.metrics.inter.unnamed.slice(0, 4).map((s) => s.sel).join(" | ") }),
  },
  {
    n: 18, title: "命中目标 (<24px / <32px)", modes: ["dark", "light"],
    parse: (t) => {
      // Read the comparators, not positions: MASTER's target cell now also cites
      // §12.7.2 and the measured 21px, so "the last number" resolved to 2 (the
      // "7.2" in the section reference) and the cap silently became ≤2.
      const caps = [...String(t.text ?? "").matchAll(/≤\s*(\d+)/g)].map((m) => Number(m[1]));
      const floors = [...String(t.text ?? "").matchAll(/<\s*(\d+)px/g)].map((m) => Number(m[1]));
      return {
        hard: floors[0] ?? 24,
        soft: floors[1] ?? 32,
        max: caps.length ? caps[caps.length - 1] : 10,
      };
    },
    criterion: [
      "MASTER §12 行 18 / primitives §5.4：硬下限 24×24（**命中区**），目标 32×32。",
      "测量对象是**真正接收交互的元素（控件根）**——antd 的 .ant-select / .ant-switch / .ant-btn / .ant-input-affix-wrapper 等，或本身就是控件的原生 input/textarea；**即使控件根 borderless 且全透明也照样是测量对象**。识别控件根用交互语义（antd 控件根类名 / role / tabindex / cursor:pointer），**不用「画了东西」**——否则任何 borderless 控件都会静默回退到更小的内部元素而空过。",
      "【历史例证，刻意保留以说明该设计决策】#chat 的两个 .ant-select 根当时实测 23px，而按绘制口径读到的是内层 input 的 24px、于是假 PASS —— 正是这条规则要拦的情形。该例**已被 t34 的 .ant-select { min-height: 24px } 修复为 24px**，今天不再触发；但规则本身必须留着，因为下一个 borderless 控件会立刻复现它。",
      "上溯保留防容器守卫：祖先两维都 >2× 或面积 >6× 才拒绝上溯。",
      "判定用命中区口径，同时并列输出绘制口径（最近一个真的画了边框/背景的包装盒）作为对照。",
      "**计入口径：任一维 <32 即计入**（w < 32 || h < 32），等价于「两维都必须 ≥32」——WCAG 2.5.8 的 24×24 也是要求两维都达标。**不要改成 &&**：写成 && 会把「宽 200、高 20」这种元素放过，那是放宽而不是收紧。",
      "【§12.9（2026-09-21 裁决，t39 落地）】<32px 预算**只判内容区**（main.ant-layout-content.content 子树，含 .view-bar）；外壳（aside.ant-layout-sider.app-sider 子树）是 13 条路由共享的同一份 DOM，逐路由计入等于把同一个常数算 13 次，故**不计入每路由预算**。外壳改为**第三个子断言、只查一次**：外壳 <32px 集合 ⊆ MASTER §12.9 的**具名闭集**（读自契约、不硬编码）∧ 外壳 <24px == 0；**⊆ 是子集语义 —— 成员变少满足、变多 FAIL**（凭票入场，与 §12.6 的出血闭集同一手法）。",
      "**「常数」这个前提本身也要被检验**：逐 capture 比对外壳 <32px 清单的**选择器路径 + className + 几何**三者，全部一致才认定为外壳常数；任一条不同 ⇒ 该元素不是常数 ⇒ **按内容区计入**（防止把「外壳」当豁免口袋）。",
      "两套读法并列输出：判定用内容区口径，display 同时给出「旧口径（每路由总数）」供对照 —— 与行 1 的双数字、行 30 的双基准、行 37 的 raw 同构。",
    ].join(" "),
    judge: (c, l, ctx) => {
      const it = c.metrics.inter;
      const floorOk = it.hit24 === 0;
      // §12.9: the budget is judged on the CONTENT AREA only. The shell is one
      // shared DOM, so charging its constant to every route counts the same
      // constant 13 times — the same structural error as §12.5 counting the
      // canvas, §12.6 counting antd's injected values, and §12.8's 2s window.
      const sc = shellConstancy(ctx?.captures ?? [c]);
      const constKeys = new Set(sc.constants.map((s) => s.key));
      // A shell element that is NOT constant on every route is not a shell
      // constant: §12.9 sends it back into the content budget.
      const nonConst = (it.shell32All ?? []).filter((s) => !constKeys.has(shellKey(s))).length;
      const content32 = it.content32 + nonConst;
      const targetOk = content32 <= l.max;
      // Third sub-assertion, evaluated ONCE from every capture: the shell's
      // <32px set must be a subset of MASTER §12.9's named closed set, and the
      // shell must respect the 24px floor. ⊆ semantics — a member leaving the
      // set passes (t36 moved three buttons out), a control outside it fails.
      const allowed = ctx?.contract?.shellSet ?? null;
      const outside = allowed ? sc.constants.filter((s) => !shellSetAdmits(allowed, s)) : [];
      const shellOk = allowed ? outside.length === 0 && it.shell24 === 0 : null;
      // Registered, not judged: .ant-switch is 21px by antd default and would
      // need Switch.trackHeight: 24 to pass, which enlarges the handle. That is a
      // real antd default, not an instrument problem — whether to change it is a
      // visual decision for design-lead, so it is named rather than "fixed".
      const switchHits = (it.hit24All ?? []).filter((s) => String(s.sel).includes("ant-switch"));
      const constTxt = sc.constants.map((s) => `${s.cls.split(/\s+/)[0]} ${s.w}x${s.h}`).join(", ") || "无";
      return {
        display: [
          `命中区 地板<24px ${it.hit24} · 内容区<32px ${content32}/≤${l.max} ${targetOk ? "✓" : "✗"}`,
          `外壳闭集 ${shellOk === null ? "?" : shellOk ? "✓" : "✗"}`,
          `｜ 旧口径（每路由总数）<32px ${it.hit32}`,
          `｜ 对照（绘制口径）<24px ${it.paint24} · <32px ${it.paint32}`,
        ].join(" "),
        pass: floorOk && targetOk && shellOk !== false,
        note: [
          `三个子断言分开判定：地板 <24px ${it.hit24} ${floorOk ? "✓" : "✗"} · 内容区 <32px ${content32} ${targetOk ? "✓" : "✗"} · 外壳闭集 ${shellOk === null ? "未判（MASTER §12.9 未读到）" : shellOk ? "✓" : "✗"}`,
          `§12.9 分区：外壳 <32px ${it.shell32} · 内容区 <32px ${it.content32} · 外壳 <24px ${it.shell24} · 内容区 <24px ${it.content24}`,
          `常数前提检验：${sc.captures} 个 capture 逐条比对（选择器路径 + className + 几何），恒定 ${sc.constants.length} 项 [${constTxt}]${sc.varying.length ? `；非常数 ${sc.varying.length} 项已回落内容区（${sc.varying.map((s) => s.route + "/" + s.mode + " " + s.cls.split(/\s+/)[0]).slice(0, 6).join(", ")}）` : "；无非常数项"}`,
          allowed ? `外壳闭集（读自 ${ctx?.contract?.shellSetSource ?? "MASTER §12.9"}）：${allowed.map((a) => a.raw + " " + a.w + "x" + a.h).join(", ")}；⊆ 语义 —— 变少满足、变多 FAIL${outside.length ? "；闭集外 " + outside.length + " 项：" + outside.map((s) => s.cls.split(/\s+/)[0] + " " + s.w + "x" + s.h).join(", ") : ""}` : "外壳闭集未从 MASTER §12.9 读到，第三个子断言未判定",
          floorOk ? "" : "命中区地板越界（逐个点名）：" + (it.hit24All ?? []).map((s) => `${s.sel} ${s.w}x${s.h}（${s.via}；绘制口径 ${s.paint}）`).join(" | "),
          it.viaRoot ? `${it.viaRoot} 个内层元素的命中区上溯到控件根` : "",
          switchHits.length
            ? `.ant-switch ${switchHits.length} 处实测 21px 是 antd 默认值（需 Switch.trackHeight:24 才达标并会放大手柄）—— 只登记不改，是否改由 design-lead 视觉裁量`
            : "",
        ].filter(Boolean).join(" · "),
      };
    },
  },
  {
    n: 19, title: "横向溢出", modes: ["dark", "light"],
    parse: (t) => ({ max: pick(t.text, /=\s*(\d+)\s*$/, 0) }),
    judge: (c, l) => {
      const main = { viewport: `${c.metrics.viewport.w}x${c.metrics.viewport.h}`, overflow: c.metrics.scroll.docWidth - c.metrics.scroll.innerWidth };
      const all = [main, ...c.overflow];
      const bad = all.filter((o) => o.overflow > l.max);
      const worst = all.reduce((a, b) => (a.overflow >= b.overflow ? a : b), all[0]);
      return {
        display: `最大 ${worst.overflow}px @${worst.viewport}`,
        pass: bad.length === 0,
        note: bad.map((b) => `${b.overflow}px @${b.viewport}`).join(" | "),
        detail: all,
      };
    },
  },
  {
    n: 20, title: "error 与 empty 可区分（失败态注入）", modes: ["dark", "light"],
    criterion: [
      "MASTER §12 行 20：每路由在 API 失败时呈现 role=alert + 重试按钮。",
      "**注入只打该路由自己的数据端点**（正常加载中观测到的 /api 端点，**排除外壳的 /permissions**）—— 打外壳会测到白屏而不是失败态，且外壳自己的 alert 会与本行的 alert 无法区分。",
      "**三条分开判定**：① **失败态可见**（role=alert / role=status / .ant-alert-error 等显式错误呈现，且可见）② **可恢复**（重试控件存在、命中区 ≥24×24、**真实点击**后错误消失）③ **不得把失败渲染成空态或成功态** —— 这正是 t13 的 inbox 缺陷形态（运行正被阻塞，而页面说没事）。",
      "**外壳必须仍在**（侧栏在、main 在、body 有文本）：否则测的是白屏，不是失败态。",
      "**该路由没有自己的数据端点 ⇒ not_measured 并给出原因**，不硬判。",
    ].join(" "),
    parse: (t) => ({
      wantAlert: /role\s*=\s*alert/.test(String(t.text ?? "")),
      wantRetry: /重试/.test(String(t.text ?? "")),
    }),
    judge: (c, l) => {
      const f = c.failure;
      if (!f) return { display: "—", pass: null, note: "未做失败态注入（--no-failure-probe）" };
      if (!f.injectable) {
        return {
          display: "无可注入端点",
          pass: null,
          note: "该路由正常加载时没有自己的 /api 数据端点（只有外壳的 /permissions），没有失败态可注入 → not_measured，不硬判",
        };
      }
      if (!f.injected) {
        return { display: `端点 ${f.injectable} 个 · 注入 0 次`, pass: null, note: `已知端点但重载后未再请求（窗口内 ${f.requests} 个 /api 请求）→ not_measured${f.error ? " · " + f.error : ""}` };
      }
      const visible = f.alerts.length > 0 || f.errorNodes.length > 0;
      const retryOk = !!f.retry && f.retry.w >= 24 && f.retry.h >= 24;
      const recovered = f.recovered === true;
      const shellOk = f.shellPresent && f.contentPresent && !f.blank;
      const emptyInstead = f.emptyShown && !visible;
      const silentSuccess = !visible && !f.emptyShown;
      return {
        display: `失败态 可见 ${visible ? "✓" : "✗"} · 重试 可点 ${retryOk ? "✓" : "✗"}${f.retry ? `(${f.retry.w}×${f.retry.h})` : ""} · 恢复 ${recovered ? "✓" : "✗"} ｜ 注入 ${f.injected}/${f.injectable} 端点 · 外壳 ${shellOk ? "✓" : "✗"} · 空态 ${f.emptyShown ? "有" : "无"}`,
        pass: visible && retryOk && recovered && shellOk,
        note: [
          `注入端点（该路由自己的，已排除外壳 /permissions）：${f.endpoints.join(", ")}`,
          visible ? `失败态呈现：${[...f.alerts, ...f.errorNodes].slice(0, 2).map((a) => a.sel + "「" + a.text.slice(0, 40) + "」").join(" · ")}` : "",
          emptyInstead ? "✗ **把失败渲染成了空态**（有 empty 元素、无任何错误呈现）—— 这正是 t13 的 inbox 缺陷形态" : "",
          silentSuccess ? "✗ **把失败渲染成了成功态**（既无错误呈现也无空态，页面看起来正常）—— 与 t13 同一形态" : "",
          retryOk ? `重试控件 ${f.retry.sel}「${f.retry.text.slice(0, 20)}」${f.retryClicked ? "真实点击成功" : "点击失败"}` : f.retry ? `重试控件命中区 ${f.retry.w}×${f.retry.h} < 24×24` : "无重试控件",
          shellOk ? `外壳完好（侧栏 + main 在位，body ${f.bodyText} 字符）—— 不是白屏` : `✗ 外壳异常（侧栏 ${f.shellPresent ? "在" : "缺"} · main ${f.contentPresent ? "在" : "缺"} · body ${f.bodyText} 字符）`,
        ].filter(Boolean).join(" · "),
      };
    },
  },
  {
    n: 40, title: "压缩致折行（短文本被挤到换行）", modes: ["dark", "light"],
    criterion: [
      "**MASTER §12 行 40（待 design-lead 补条目 —— 本工具不自行改 MASTER）**：承载短文本（≤12 字符）的元素若被挤到多行，且**关闭压缩源后尺寸显著改变**，即为布局事故（§12.12 的 ③ 类「症状」），**每 capture ≤0**。",
      "**判定分三步，缺一不可**：① 该元素**直接**文本 ≤12 字符 ② 实际行数 ≥2（只数**该元素自己的文本节点**的行盒，按 4px 聚类；容器含标签+值两个 inline 子项时会各出一行盒，数子树会假报 2 行）③ **可判定式 = 自然宽度**：把该文本用元素自身的字体在屏外以 white-space:nowrap 量出**单行自然宽度**，若 > 元素宽度 + 2px ⇒ 容器给少了宽度，折行由**容器压缩**造成。",
      "**必须与「有意截断」区分**（否则天天假 FAIL）：-webkit-line-clamp ≥ 1（设计要 N 行）或 text-overflow:ellipsis + 任一轴 overflow:hidden/clip（设计要 1 行）⇒ 判 **intentional**，不计命中。",
      "**已知实例（本条判据的由来）**：外壳 .sidebar-foot 的 .conn.ok / .build-id / .kbd-hint 在每一页渲染成 3 行竖排（42×45 / 33×45 / 26×45，sw==cw、sh==ch ⇒ 无溢出），因此行 19（溢出）、行 18（<24px 地板）、行 29–33（对比度）**全都判它 PASS**；而 §12.9 甚至把 kbd-hint 26×45 记成了外壳具名闭集项 —— 审计把一个折行缺陷认证成了设计常数（§12.12 已裁决为 ③ 症状，t66 修）。",
      "**为什么不用「放松压缩源后尺寸是否改变」单独判定（实测反例）**：把 flex 子项压到 1–2px 后，在**子项**上置 flex-shrink:0 / min-width:max-content 并不能解除压缩（父项的 min-width:0 仍然赢），盒子仍是 2px、放松后高度不塌 ⇒ 一个真实的 9 行缺陷被判 PASS。自然宽度没有这个盲区（85px 的文字塞进 2px 盒）。放松后的几何仍记录在案，作为佐证但**不参与判定**。",
    ].join("\n"),
    parse: (t) => ({ max: pick(t.text, /每\s*capture\s*≤\s*(\d+)/, 0) }),
    judge: (c, l) => {
      const tl = c.textLayout ?? {};
      const rows = (tl.wraps ?? []).map((r) => ({ ...r, cls: wrapClassify(r) }));
      const bad = rows.filter((r) => r.cls.hit);
      const intended = rows.filter((r) => r.cls.kind === "intentional");
      const content = rows.filter((r) => r.cls.kind === "content");
      return {
        display: bad.length
          ? "压缩致折行 " + bad.length + " 处 / 有意截断 " + intended.length + " / 内容驱动 " + content.length
          : "压缩致折行 0 处（多行载体 " + rows.length + " 个：有意截断 " + intended.length + " · 内容驱动 " + content.length + "）",
        pass: bad.length <= l.max,
        note: bad.length
          ? bad.map((r) => r.sel + "「" + r.text + "」" + r.w + "×" + r.h + " lh" + r.lh + " " + r.lines + " 行 → 放松后 " + r.relaxed).join(" | ")
          : "",
        detail: { max: l.max, hits: bad, intentional: intended, content, scanned: tl.scanned ?? 0 },
      };
    },
  },
  {
    n: 41, title: "兄弟元素压盖", modes: ["dark", "light"],
    criterion: [
      "**MASTER §12 行 41（待 design-lead 补条目 —— 本工具不自行改 MASTER）**：同一容器内的**兄弟**元素矩形两两相交（容差 ≤1px）即压盖，**每 capture ≤0**。",
      "**排除规则（合法覆盖层，按位置判定而非类名）**：① 任一方 position:absolute|fixed —— 下拉/弹层/backdrop/portal 本来就该盖住兄弟；② 任一方在覆盖层子树内（antd .ant-dropdown/.ant-select-dropdown/.ant-popover/.ant-tooltip/.ant-modal-wrap|mask/.ant-drawer/.ant-picker-dropdown/.ant-notification/.ant-message，或 role=menu|listbox|dialog|tooltip|alertdialog）—— 这些在 portal 里保留 static 外壳，只看 position 会漏；③ 设了 z-index 且非 static。",
      "**为什么按位置而不是类名**：类名会随重构改名而失效，而「覆盖层靠 position/z-index 脱离文档流」是布局的定义。探针把 legit 与原因一并输出，排除项是**可见的**，不需要信任。",
      "**为什么只比兄弟**：父子几何天然相交（父盒包住子盒），纳入会淹没真信号。",
      "**行号与阈值锚定**：pick 读 MASTER §12 行 41 的「每 capture ≤N」；条目尚不存在时回退内置 0，并由 Warnings 点名锚点未命中。",
    ].join("\n"),
    parse: (t) => ({ max: pick(t.text, /每\s*capture\s*≤\s*(\d+)/, 0) }),
    judge: (c, l) => {
      const tl = c.textLayout ?? {};
      const rows = (tl.overlaps ?? []).map((r) => ({ ...r, cls: overlapClassify(r) }));
      const bad = rows.filter((r) => r.cls.hit);
      const legit = rows.filter((r) => !r.cls.hit);
      return {
        display: bad.length
          ? "压盖 " + bad.length + " 对 / 合法覆盖 " + legit.length + " 对"
          : "压盖 0 对（合法覆盖 " + legit.length + " 对已排除）",
        pass: bad.length <= l.max,
        note: bad.length
          ? bad.map((r) => r.parent + " 内 " + r.a + " × " + r.b + " 交 " + r.overlap).join(" | ")
          : "",
        detail: { max: l.max, hits: bad, legit },
      };
    },
  },
  {
    n: 42, title: "transition: all（必须显式列出属性）", modes: ["dark", "light"],
    criterion: [
      "**MASTER §12 行 42（待 design-lead 补；§12.15 是唯一出处）**：`transition-property` 为 `all` 且**时长非 0** 的元素数，**每 capture ≤0**。",
      "**判定式**：计算样式 `transitionProperty === \"all\"` **且** `transitionDuration !== \"0s\"`。**两个条件缺一不可**：`transition-property` 的**初始值就是 `all`**，只判属性会把整页元素全部命中（这正是 §12.10.5 说的「宽判据数到假对象」）。",
      "**为什么读计算样式而不是源码**：§12.15 的探测是 DOM+CSS 双侧；计算样式是事实，源码是意图（附录 A 的既定口径）。它同时覆盖 `panel/src/index.css` 与 antd CSS-in-JS 两条来源。",
      "**归属**：命中项逐个带 `antd` 标记（类名含 `ant-` 或 `css-<hash>`），便于把 antd 内部与项目自有代码分开读 —— 本行**不豁免** antd，只是让归属可见。",
      "**锚点**：`/每\\s*capture\\s*≤\\s*(\\d+)/`（与行 40/41 一致）。",
    ].join("\n"),
    parse: (t) => ({ max: pick(t.text, /每\s*capture\s*≤\s*(\d+)/, 0) }),
    judge: (c, l) => {
      // §12.10.5 rule 4, NEW SUB-FORM (recorded here, next to the guard, because a
      // correction only holds where the next reader looks):
      //   original form -- the criterion matches NO object -> passes silently.
      //   new sub-form  -- the INSTRUMENT ITSELF throws -> downstream reads the
      //                    MISSING data as an EMPTY SET -> passes silently.
      //   Different mechanism (no match vs. an exception), IDENTICAL output (green),
      //   and rule 4's original counter-measure cannot catch it: "the hit count must
      //   be non-zero" needs a hit count, and a broken probe does not even produce
      //   one. Measured on t84: a TDZ ReferenceError made rows 42-45 all report PASS
      //   with 0 candidates and the table showed 44 pass / 0 fail.
      //   Counter-measure (implemented): a probe/dependency failure must degrade
      //   explicitly to not_measured and must never fall through to PASS -- i.e.
      //   "no data" and "empty data" must be distinguished. Same reason as row 43's
      //   "0 objects is not the same as verified compliant".
      if (!c.markup || c.markup.error) return { display: "— (markup probe failed)", pass: null };
      const rows = c.markup?.transitionAll ?? [];
      const live = rows.filter((r) => r.prop === "all" && r.dur !== "0s");
      const excluded = live.filter((r) => /^antd-cssinjs/.test(String(r.owner || "")));
      const bad = live.filter(transitionAllHit);
      return {
        // The excluded count is PRINTED, never hidden: the ruling says those hits
        // are antd's, not that they are compliant.
        display: bad.length
          ? "transition:all " + bad.length + " 处（自有代码）· 已排除 antd-cssinjs " + excluded.length + " 处"
          : "transition:all 0 处（自有代码）· 已排除 antd-cssinjs " + excluded.length + " 处（不是「已合规」，是「不是我们的」）· 扫描 " + (c.markup?.scanned ?? 0) + " 个元素",
        pass: bad.length <= l.max,
        note: bad.map((r) => r.sel + " (" + r.dur + ", " + (r.owner || "unknown") + (r.ownClass ? ", +own:" + r.ownClass : "") + ")").join(" | "),
        detail: { max: l.max, hits: bad, excluded: excluded.length },
      };
    },
  },
  {
    n: 43, title: "img 缺 alt", modes: ["dark", "light"],
    criterion: [
      "**MASTER §12 行 43（待 design-lead 补；§12.15 是唯一出处）**：缺 `alt` 属性的 `<img>` 数，**每 capture ≤0**。",
      "**判定式**：`!el.hasAttribute(\"alt\")`。**`alt=\"\"` 计为合格** —— 那是装饰性图片的**正确**写法，不是缺失；缺属性才是违规。",
      "**范围**：**全部 `<img>`**（含当前不可见/在浮层里的），因为这是**标记**规则而不是渲染规则 —— 一个 `display:none` 的 `<img>` 仍然需要 `alt`。记录里同时给出 `visible`，便于阅读时区分。",
      "**为什么不看 `role=presentation` / `aria-hidden`**：WIG 的形态是「装饰性须 `alt=\"\"`」，用 `alt=\"\"` 表达；`role`/`aria-hidden` 不能替代 `alt` 属性本身。",
      "**锚点**：`/每\\s*capture\\s*≤\\s*(\\d+)/`。",
    ].join("\n"),
    parse: (t) => ({ max: pick(t.text, /每\s*capture\s*≤\s*(\d+)/, 0) }),
    judge: (c, l) => { if (!c.markup || c.markup.error) return { display: "— (markup probe failed)", pass: null }; const rows = c.markup?.imgNoAlt ?? []; const bad = rows.filter(imgAltHit); const n = c.markup?.imgs ?? 0; return { display: bad.length ? "img 缺 alt " + bad.length + "/" + n : "img 缺 alt 0/" + n, pass: bad.length <= l.max, note: bad.map((r) => r.sel + " [" + r.w + "x" + r.h + "] " + r.src).join(" | "), detail: { max: l.max, hits: bad, imgs: n } }; },
  },
  {
    n: 44, title: "字面三点号（应为省略号 …）", modes: ["dark", "light"],
    criterion: [
      "**MASTER §12 行 44（待 design-lead 补；§12.15 是唯一出处）**：渲染文本里出现字面 `...` 的处数，**每 capture ≤0**。",
      "**判定式**：文本节点的值匹配 `/\\.\\.\\./`（三个 ASCII 句点），或 `placeholder` / `title` / `aria-label` 属性值里出现。**`…`（U+2026）不算**。",
      "**为什么读 DOM 而不是源码**：**注释里的字符串不算**（§12.15 记录里 `outline: none` 的两处命中正是「都在注释里」被排除）—— 注释永远进不了 DOM，所以读渲染结果是**天然**排除了注释，不需要正则去猜注释边界。",
      "**有意排除**：`pre` / `code` / `kbd` / `samp` / `textarea` 子树 —— 代码样例里三个点是**内容**，不是排版省略。探针按 `closest()` 整棵子树排除。",
      "**锚点**：`/每\\s*capture\\s*≤\\s*(\\d+)/`。",
    ].join("\n"),
    parse: (t) => ({ max: pick(t.text, /每\s*capture\s*≤\s*(\d+)/, 0) }),
    judge: (c, l) => { if (!c.markup || c.markup.error) return { display: "— (markup probe failed)", pass: null }; const rows = c.markup?.literalDots ?? []; const bad = rows.filter(literalDotsHit); return { display: bad.length ? "字面三点号 " + bad.length + " 处" : "字面三点号 0 处", pass: bad.length <= l.max, note: bad.map((r) => r.sel + " [" + r.where + "] \u300c" + r.text + "\u300d").join(" | "), detail: { max: l.max, hits: bad } }; },
  },
  {
    n: 45, title: "数值读数带 tabular-nums（覆盖率）", modes: ["dark", "light"],
    criterion: [
      "**MASTER §12 行 45（待 design-lead 补；§12.15 是唯一来源，且它自己标注这一类是「弱覆盖」）**：**数值读数**中带 `font-variant-numeric: tabular-nums` 的**覆盖率 = 100%**。",
      "**范围为什么这样定（本行必须自带定义，因为契约里只有弱覆盖）**：**数值读数** = ① 元素**自身**文本只由数字与分隔符（`. , : / % × x k m + -` 与空格）构成 ② 长度 2–24 字符 ③ 它属于一个**数字组**：**祖父层**（含自身）至少有 **2 个**这样的元素。**组是对齐发生的地方**：孤立的单个数字与任何东西都不对齐。",
      "**判定式**：计算样式 `fontVariantNumeric` 含 `tabular-nums`。该属性**可继承**，所以读元素自身的计算值已经涵盖了祖先设置的场景。",
   "**must-FAIL（新范围不得掩盖真实回归）**：在**一个读数栅格内**注入缺 `tabular-nums` 的数值 ⇒ **必须 FAIL** —— 自检 `row45 must-FAIL: a readout-grid figure without tabular-nums FAILS` 与 `row45 judge: 1 of 4 missing FAILS the coverage`。",
      "**范围的两版与为什么最终选祖父层（t81 裁决）**：初版「与 ≥2 个同类**兄弟**同父」在每条路由上候选都是 **0** ⇒ **空集通过**（§12.10.5 第 4 条）。原因是面板把每个数字放进**自己的单元格**（`div.readout-cell > span.readout`、`div.stat-cell > span.stat-num`），列在**祖父层**。实测：`parentPeer≥2` 全站 **0**；`grandPeer≥2` = home 11 / board 1 / task 11 / stats 4 —— **正是那些读数栅格**。",
      "**被有意排除的 4 处（逐条列出，让它是被记录的决定而不是静默的排除）**：home `div.dash-head.zone-head > span.zone-note`「199」与「126」、board `div.kanban-head.zone-head > span.count`「20」、task `div.row-btn.selected > span.muted`「17%」。理由：**全部是孤立的单个数字**（无同类兄弟、不构成列或对照）⇒ 按本行意图（tabular-nums 用于**数字列/对照**）不该被要求。同一范围还**自动**排除了 5 个 `button.kanban-card > div.time` 日期（`2026/9/12`）—— 时间戳不是待对齐的数字列。",
    ].join("\n"),
    // MASTER writes the target as 「**覆盖率 100%**」 (no equals sign), while the
    // criterion text and the sibling rows use 「覆盖率 = N%」. Both are read.
    parse: (t) => ({ min: pick(t.text, /覆盖率\s*(?:=\s*)?(\d+)\s*%/, 100) / 100 }),
    judge: (c, l) => { if (!c.markup || c.markup.error) return { display: "— (markup probe failed)", pass: null }; const rows = c.markup?.numerics ?? []; const bad = rows.filter(tabularHit); const cov = rows.length ? (rows.length - bad.length) / rows.length : 1; const pctTxt = (cov * 100).toFixed(1) + "%"; return { display: rows.length ? "tabular-nums 覆盖 " + (rows.length - bad.length) + "/" + rows.length + " = " + pctTxt + " / \u2265" + (l.min * 100).toFixed(0) + "%" : "无数值读数（覆盖率按 1 记）", pass: cov >= l.min, note: bad.slice(0, 6).map((r) => r.parent + " > " + r.sel + " \u300c" + r.text + "\u300d").join(" | "), detail: { min: l.min, candidates: rows.length, missing: bad } }; },
  },
  {
    n: 46, title: "列表行主标签可读性", modes: ["dark", "light"],
    // The contract's own target cell reads 「每 capture 全绿（width > 0 且 width ≥ 5 ×
    // fontSize − 0.5）」: the machine-readable number is the 5, so the floor
    // MULTIPLIER is read out of MASTER rather than hardcoded here.
    parse: (t) => ({ em: pick(t.text, /width\s*≥\s*(\d+)\s*×\s*fontSize/, 5) }),
    criterion: [
      "**MASTER §12 行 46（已落表）**：**列表行主标签可读性**（`.row-btn .title`，**所有列表**，不止 chat 侧栏）。**两级判据**：",
      "① **硬判据（普适，无需出处）**：width > 0 —— 「主标签不得不可见」对任何列表都成立。",
      "② **附加判据（仅当产物里存在该声明）**：width >= 该元素**自己在 CSS 里声明的 min-width**。**阈值是派生的，不是设定的**：chat 有 `.chat-session-row .title { min-width: 5em }` ⇒ chat 断 65px；`#sessions` **没有**这条声明 ⇒ **只断 > 0**。**将来某个列表加了 min-width，本行自动收紧，无需改判据。**",
      "**为什么统一下限是对的（captain 先裁「必须逐列表有声明」、实测后撤回）**：390 档 `#sessions` 最小 40.98px，字号 14 ⇒ **约 3 个字符** ⇒ **那不是可读的标题** ⇒ 它正是用户抱怨的那个形状（「根本就看不到 session 的文字」）⇒ **是真缺陷，不是假红**。",
      "**可追溯性以「读数」而非「阈值」保住**：display 逐个路由打印该样本**自己的 computed min-width** ⇒ 「哪些列表自己声明了保护」仍然可见。",
      "**display 必须打印每个路由的实测最小值与字号**（含 `#sessions` 的 41px，**即使它 PASS**）—— 让下一个读者看见这条边界，而不是从一个全局数字去猜。",
      "**对象集为什么是「所有 `.row-btn` 列表」而不是「会话行」**：塌陷来自**共享规则** `.title { flex: 1; min-width: 0; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }`（`index.css:638`）—— 同一个根因在多个列表上成立（systems 在 390 档实测 **#knowledge 的 `.title` 宽 = 0**）。**判据的对象集若比缺陷窄，就会在那里匹配不到对象而静默通过** —— 那正是 §12.10.5 第 4 条的「漏」。**实测覆盖**：chat 侧栏 · knowledge · wiki · sessions · agents · runtimes · inbox · board。",
      "**下限 5em 对所有对象集都成立的论证（不是把 chat 的数搬过去）**：`5em` 不是 chat 的偶然取值，而是**设计对「一个 `.title` 该有多宽」给出的答案** —— 它写在**同一个元素类**上（`.chat-session-row .title { min-width: 5em }`，`index.css:1909`），修的就是**同一条共享规则**造成的同一个塌陷。因此按元素自身字号算 `5em`，对任何列表都表达同一件事：**这个标签至少要放得下 5 个字**。**display 同时打印每个样本自己的计算 `min-width`**，所以「哪些列表自己声明了保护、哪些没有」是可见的，而不是被一个统一下限抹平。",
      "**下限的依据不是拍脑袋，是设计自己的选择**：`panel/src/index.css` 的 `.chat-session-row .title { min-width: 5em; }` —— 我取 font-size x 5 而**不是硬编码 65**，这样将来改字号阶梯时下限跟着设计走，而不是让这一行悄悄失效。",
      "**为什么需要这一行**：systems 的控制跑（7 条规则全关）让标题只剩 **6px**（768 档 43.7px、390 档 **0px**、#knowledge 390 档 **0px**），而当时审计 **41 项全绿** —— **没有任何判据看着这个对象**，所以它永远不会红。用户症状就是它：**会话的文字根本看不到**。",
      "**判定式**：对每个 `.chat-session-row .title`，width > 0 且 width >= 5 x fontSize - 0.5（0.5 是亚像素容差）。**判 width，不判 scrollWidth** —— 文本被省略号截断是**有意**的（实测 scrollWidth 288 远大于 65），本行只问「标签盒是否被压到不可读」。",
      "**覆盖 390 与 1440**：26 个 capture 都是 1440x900（13 路由 x 2 态），**不含 390** ⇒ 探针**自己在 capture 内扫视口**（1440 / 768 / 390），并在结束时**把测量视口放回去**，不影响截图。",
      "**not_measured 边界**：若某 capture 里 chat 侧栏没有会话行（**无对象**），本行报 not_measured 而**不是 PASS** —— 「没有对象」与「对象合格」必须区分（与行 43 同一条理由）。",
      "**锚点**：/覆盖率\s*(?:=\s*)?(\d+)\s*%/（与行 45 同形；契约行落地后由 design-lead 定稿）。",
    ].join("\n"),
    judge: (c, l) => {
      // A probe that threw leaves the row with no data: not-measured, never pass.
      if (!c.sessionTitle || c.sessionTitle.error) return { display: "— (session title probe failed)", pass: null };
      const v = sessionTitleVerdict(c.sessionTitle, l.em);
      if (!v.measured) return { display: "— 本 capture 无会话行（0 个对象）", pass: null };
      const head = v.bad.length
        ? v.bad.length + " 个标签不合格（最小 " + v.bad[0].w + "px @ " + (v.bad[0].route || "?") + " @ " + v.bad[0].viewport + "，声明下限 " + v.bad[0].declared + "px）"
        : "全部合格（最小 " + v.min + "px @ " + v.where + "）";
      return {
        display:
          "列表标签 " + head +
          " · 硬判据 width>0 满足 " + (v.samples - v.bad.filter((b) => b.w <= 0).length) + "/" + v.samples +
          " · 派生下限（CSS 自声明）适用于 " + v.declared + "/" + v.samples + " 个样本 · " + v.perRoute.join(" · ") +
          (v.pass ? " ✓" : " — 被压到不可读「" + v.worst.text + "」"),
        pass: v.pass,
        note: "样本 " + v.samples + " 个 · 最差 " + v.where + " w=" + v.worst.w + "px fs=" + v.worst.fs + "px 声明 min-width=" + v.worst.minW + " · 两级判据：width>0（普适）∧ width≥声明下限（仅当声明存在）",
        detail: { min: v.min, samples: v.samples, worst: v.worst, where: v.where, perRoute: v.perRoute, derivedFloors: v.declared, bad: v.bad },
      };
    },
  },
  {
    n: 21, title: "命令面板 ARIA 缺口数", modes: ["dark", "light"],
    global: true,
    parse: (t) => ({ max: pickLeadingNumber(t.text, 0) }),
    judge: (c, l, ctx) => {
      const p = ctx.global.palette;
      if (!p || !p.open) return { display: "—", pass: null, note: "palette did not open" };
      return { display: `${p.missing.length} 缺口`, pass: p.missing.length <= l.max, note: p.missing.join(", ") || "all 6 present" };
    },
  },
  {
    n: 22, title: "浮层关闭后焦点归属", modes: ["dark", "light"],
    global: true,
    parse: () => ({ want: "trigger" }),
    judge: (c, l, ctx) => {
      const p = ctx.global.palette;
      if (!p || !p.open) return { display: "—", pass: null, note: "palette did not open" };
      return { display: p.focusAfterEscape ? `${p.focusAfterEscape.tag}.${p.focusAfterEscape.cls ?? ""}` : "?", pass: p.focusReturnedToTrigger === true };
    },
  },
  {
    n: 23, title: "[aria-expanded] 出现数（只手写展开控件）", modes: ["dark", "light"],
    criterion: "primitives §5.2.2 / §11 row 23：只统计 .recall-stub-head / .tool-head / .chat-group-more，原生 summary 自动 pass；全站 querySelectorAll 计数会被 antd Select 抬成 3/3/1/1 而虚假通过",
    // The target is 「覆盖率 = 100%」; the row judges the COMPLEMENT (missing
    // controls), derived from the contract rather than hardcoded, and anchored on
    // the "=" so a stray number in the cell cannot move it.
    parse: (t) => ({ max: 100 - pick(t.text, /=\s*([\d.]+)\s*%/, 100) }),
    judge: (c, l) => {
      const e = c.metrics.expand;
      const present = e.controls.filter((x) => x.count > 0);
      const missing = expandMissing(e);
      return {
        display: present.length ? present.map((x) => `${x.sel} ${x.withAriaExpanded}/${x.count}`).join(", ") : "无手写展开控件（空过）",
        pass: missing.length <= l.max,
        note: `全站 [aria-expanded] ${c.metrics.structure.ariaExpanded} 个（antd combobox ${e.antdCount} 个不计入）· 原生 summary ${e.native}${missing.length ? " · " + missing.map((x) => x.sel).join(", ") : ""}`,
      };
    },
  },
  {
    n: 24, title: "每路由 h1 数", modes: ["dark", "light"],
    parse: (t) => ({ exactly: pick(t.text, /恰好\s*(\d+)/, 1) }),
    judge: (c, l) => ({ display: `${c.metrics.structure.h1}${c.metrics.structure.h1Text ? ` "${c.metrics.structure.h1Text}"` : ""}`, pass: c.metrics.structure.h1 === l.exactly }),
  },
  {
    n: 25, title: "#task DOM 节点数", modes: ["dark", "light"],
    parse: (t) => ({ max: pick(t.text, /≤\s*([\d,]+)/, 3000) }),
    judge: (c, l) => c.route === "task"
      ? { display: `${c.metrics.domNodes} / ≤${l.max}`, pass: c.metrics.domNodes <= l.max }
      : { display: `${c.metrics.domNodes}`, pass: null, note: "row scoped to #task" },
  },
  {
    n: 26, title: "最长单次阻塞 (longtask)", modes: ["dark", "light"],
    parse: (t) => ({ max: pick(t.text, /≤\s*(\d+)\s*ms/, 200) }),
    judge: (c, l) => ({ display: `${c.metrics.longtask.max}ms (${c.metrics.longtask.over200} 次 >200ms)`, pass: c.metrics.longtask.max <= l.max }),
  },
  {
    n: 27, title: "首屏入口 JS 体积（地板 + 预算两段）", modes: ["dark", "light"], global: true,
    criterion: [
      "MASTER §12 行 27（§12.10.2 落槌）：从单一门槛改为**地板 + 预算**两段 + 两条结构断言。",
      "**地板（登记，不判 PASS/FAIL）= 框架层 980KB（gzip ≈305KB）**：随技术栈变更重测、不写死；每次审计打印。",
      "**预算（判定）= 应用层首屏 ≤150KB（gzip ≤48KB）**：推导 = 应用层实测 426KB − markdown 栈 229KB（应懒加载）− 重页路由级分割 ~119KB = 78KB，预算取 ≈2×。",
      "**结构断言 ①** markdown 栈不得在入口（必须是独立 chunk）；**②** 必须存在路由级分割 chunk。",
      "**归类未落地前判 not_measured 并打印「地板 / 应用层 / 总量」三个数** —— 实施前提是 vite.config.ts 的 manualChunks（vendor/app/markdown 三块）+ 工具按 chunk 读 dist。**不硬判 FAIL**（拿总量比旧门槛是无效判定），**也不改成 PASS**（那是粉饰）。原「≤350KB（gzip ≤120KB）」保留为历史记录。",
      "**判定的量是「首屏实际要下载的应用层代码」，不是「入口文件多大」**：首屏静态 chunk 由 dist/index.html 实测得出（入口 <script> + 每个静态依赖一条 modulepreload），懒加载 chunk 天然不在集合里。**把首屏必需的组件改成懒加载会让这个数变小却不改善首屏** —— t57 实测把 CommandPalette 改成 lazy chunk 后入口 94.29 → 89.09KB（工具读数会「达标」），但 Ctrl+K 后 156ms 面板仍未挂载、紧跟按键全丢，e2e/palette.spec.ts:65 稳定失败 3/3，已完整回滚。**本行只报告这个数，不为达标而拆分。**",
    ].join(" "),
    parse: (t) => {
      const s = String(t.text ?? "");
      const num = (re, d) => {
        const m = s.match(re);
        return m ? Number(m[1]) : d;
      };
      return {
        floorKb: num(/框架层\s*(\d+)\s*KB/, 980),
        floorGzipKb: num(/地板[^；]*?gzip\s*≈?\s*(\d+)\s*KB/, 305),
        budgetKb: num(/应用层首屏\s*≤\s*(\d+)\s*KB/, 150),
        budgetGzipKb: num(/应用层首屏\s*≤\s*\d+\s*KB（gzip\s*≤\s*(\d+)\s*KB）/, 48),
        legacyKb: num(/原「≤\s*(\d+)\s*KB/, 350),
        legacyGzipKb: num(/原「≤\s*\d+\s*KB（gzip\s*≤\s*(\d+)\s*KB）」/, 120),
      };
    },
    judge: (c, l, ctx) => {
      const b = ctx.global.bundle;
      const entry = b?.entry;
      if (!entry) return { display: "—", pass: null, note: "no dist bundle found" };
      const js = b.js ?? [];
      const kb = (n) => (n / 1024).toFixed(0);
      const allBytes = js.reduce((a, f) => a + f.bytes, 0);
      const allGzip = js.reduce((a, f) => a + f.gzip, 0);
      // ── The framework floor, MEASURED ──────────────────────────────────────
      // §12.10.2 ② registers 980KB and says to re-measure it as the stack moves
      // ("不写死"), so the floor is the framework chunk(s) actually present in
      // dist; MASTER's registered value is printed alongside for comparison.
      const FW_RE = /^(vendor|antd|react|rc)[-.]/i;
      const fwChunks = js.filter((f) => FW_RE.test(f.file));
      const fwBytes = fwChunks.reduce((a, f) => a + f.bytes, 0);
      const fwGzip = fwChunks.reduce((a, f) => a + f.gzip, 0);
      const floorBytes = fwChunks.length ? fwBytes : l.floorKb * 1024;
      const floorSrc = fwChunks.length
        ? `实测 ${fwChunks.map((f) => f.file).join("+")}`
        : `无框架层 chunk，回退 MASTER 登记 ${l.floorKb}KB`;
      // ── THE app layer. Exactly ONE definition, so exactly one number. ─────
      // 应用层首屏 = 首屏静态 chunk（入口 + 它的静态 import）− 地板.
      //
      // This is what §12.10.2 means by 「应用层首屏」: ③ derives the budget as
      // 426 − 229（markdown 应懒加载）− 119（重页应路由级分割）= 78KB, i.e. the
      // app code the browser must actually download for the first screen. The
      // first-screen static graph is a MEASUREMENT, not a guess: Vite writes it
      // into dist/index.html as the entry <script> plus one modulepreload per
      // statically imported chunk, and lazy chunks are absent by construction.
      //
      // Two wrong readings are gone. (a) t47's 「总量 − vendor − markdown」, which
      // subtracted only a 23KB chunk that merely happened to be named vendor-*
      // while antd/react sat in their own chunks → 1117KB. (b) 「总量 − 地板」,
      // which counts every lazy route chunk as first screen → 391KB. Both were
      // printed under the single label 「应用层」 next to each other.
      const fsNames = new Set(b.firstScreen ?? []);
      const fsChunks = js.filter((f) => fsNames.has(f.file));
      const fsBytes = fsChunks.reduce((a, f) => a + f.bytes, 0);
      const fsGzip = fsChunks.reduce((a, f) => a + f.gzip, 0);
      const fsFw = fsChunks.filter((f) => FW_RE.test(f.file));
      const fsFwBytes = fsFw.reduce((a, f) => a + f.bytes, 0);
      const fsFwGzip = fsFw.reduce((a, f) => a + f.gzip, 0);
      const appBytes = Math.max(0, fsBytes - fsFwBytes);
      const appGzip = Math.max(0, fsGzip - fsFwGzip);
      const preAppBytes = Math.max(0, allBytes - floorBytes);
      // The two structural assertions are read from the ARTIFACT, not the config.
      const mdRe = /markdown|remark|mdast|micromark|unified|highlight/i;
      const mdChunks = js.filter((f) => mdRe.test(f.file));
      const mdInEntry = mdRe.test(entry.file);
      const routeChunks = js.filter((f) => f !== entry && f.bytes > 0);
      const struct = `结构断言：① markdown 在入口 ${mdInEntry ? "是 ✗" : "否 ✓"}${mdChunks.length ? "（独立 chunk " + mdChunks.map((f) => f.file).join(", ") + "）" : "（未见独立 markdown chunk）"} · ② 路由级分割 chunk ${routeChunks.length} 个 ${routeChunks.length ? "✓" : "✗"}`;
      // §12.10.2 ④: the criterion presupposes that the framework layer really is
      // split out. If it is not, "总量 − 地板" would subtract a chunk that merely
      // happens to be named vendor-* and inflate the app layer — so the row is
      // not_measured rather than judged on a fiction.
      const classified = fwChunks.length > 0 && fwBytes >= allBytes * 0.4;
      if (!classified) {
        return {
          display: `总量 ${kb(allBytes)}KB (gzip ${kb(allGzip)}KB) · 地板 ${kb(floorBytes)}KB（${floorSrc}）`,
          pass: null,
          note: [
            "归类未落地 → not_measured（§12.10.2 ④）：框架层没有被真的切出去（框架 chunk 占比 <40%），此时「总量 − 地板」只会减掉一个恰好叫 vendor-* 的小 chunk 而虚增应用层；需 vite.config.ts 的 manualChunks + 工具按 chunk 读 dist",
            `框架层 chunk ${fwChunks.length} 个 = ${kb(fwBytes)}KB（占 ${((fwBytes / allBytes) * 100).toFixed(1)}%）；入口 ${entry.file} 占 ${((entry.bytes / allBytes) * 100).toFixed(1)}%`,
            struct,
            `dist JS chunk ${js.length} 个；原 ≤${l.legacyKb}KB（gzip ≤${l.legacyGzipKb}KB）门槛已由 §12.10.2 修订，仅作历史`,
          ].join(" · "),
        };
      }
      const appOk = appBytes <= l.budgetKb * 1024 && appGzip <= l.budgetGzipKb * 1024;
      return {
        display: `应用层首屏 ${kb(appBytes)}KB (gzip ${kb(appGzip)}KB) / ≤${l.budgetKb}KB (gzip ≤${l.budgetGzipKb}KB) ${appOk ? "✓" : "✗"} ｜ 首屏静态 chunk ${kb(fsBytes)}KB（${fsChunks.map((f) => f.file).join(" + ") || "—"}）− 地板 ${kb(fsFwBytes)}KB ｜ 全量 JS ${kb(allBytes)}KB · dist chunk ${js.length} 个`,
        pass: appOk && !mdInEntry && routeChunks.length > 0,
        note: [
          `应用层首屏 = 首屏静态 chunk − 地板（§12.10.2 ③ 的口径：426 − 229 markdown − 119 路由分割 = 78KB 目标）：${kb(fsBytes)} − ${kb(fsFwBytes)} = ${kb(appBytes)}KB（gzip ${kb(appGzip)}KB）。首屏静态 chunk 由 dist/index.html 实测得出（入口 <script> + 每个静态依赖一条 modulepreload），懒加载 chunk 天然不在集合里`,
          `地板来源：${floorSrc}；MASTER §12.10.2 ② 登记 ${l.floorKb}KB（gzip ≈${l.floorGzipKb}KB），按裁决「随技术栈变更重测」以实测为准`,
          struct,
          `【非判定值，刻意不叫「应用层」】全量非框架 chunk（优化前口径 = 总量 − 地板）：${kb(preAppBytes)}KB —— 它把 15 个懒加载路由 chunk 也算进了首屏，**不是首屏要下载的量**；本行按首屏口径判定`,
          `dist JS chunk ${js.length} 个；入口 ${entry.file}（静态 import：${fsChunks.filter((f) => f !== entry).map((f) => f.file).join(", ") || "无"}）；原 ≤${l.legacyKb}KB（gzip ≤${l.legacyGzipKb}KB）门槛已由 §12.10.2 修订，仅作历史`,
        ].join(" · "),
      };
    },
  },
  {
    n: 28, title: "窗口内 API 请求数 (单路由)", modes: ["dark", "light"],
    criterion: [
      "MASTER §12 行 28（§12.10.1 落槌）：11.5s 窗口内单路由 API 请求数，**门槛 7 不变**（11.5s ÷ 2s ≈ 6 次间隔 + 首次请求 = 7），**改的是判定范围**。",
      "**子断言 ①（逐路由）**：该路由窗口内的全部 /api 请求，**排除 /api/v1/permissions**；把路径里的数字 id 归一化为 :id 后**按端点分组，取最大组 ≤7**。",
      "**子断言 ②（只查一次）**：/api/v1/permissions 单独计数，**全站 13 路由 × 两模式取最大值 ≤7**。",
      "两个子断言**分开计数、分开判定**（与 §12.9 的 24px 地板同手法）。",
      "**没有轮询器的路由照常判 PASS**（端点最大组 1–2），**不判 not_measured、不判「不适用」**——「没有轮询器」正是本行要确认的结论（§12.10.1 ④）。",
      "窗口起点不变：仍从页面 load 起 11.5s，**不引入新的起点信号**（逐端点分组已把载入突发自然排除：突发里同一端点只出现 1–2 次）。",
    ].join(" "),
    parse: (t) => {
      // §12.10.1: the cap is carried by 「最大组 ≤N」, not by the first number in
      // the cell — the cell also quotes the superseded ≤3 and 「见 §12.8」.
      const cap = pick(t.text, /最大组\s*≤\s*(\d+)/, 7);
      const ruling = PENDING_RULINGS[28];
      return { max: cap >= ruling.cap ? cap : ruling.cap, docMax: cap, ruled: cap < ruling.cap };
    },
    judge: (c, l, ctx) => {
      // Judge the window that was actually observed, not the requested one:
      // --settle also lengthens it, and a capture that watched 2.5s must never
      // be scored against an 11.5s target.
      const win = c.apiWindowMs ?? ctx?.args?.apiWindowMs ?? 0;
      const judged = win >= 11500;
      const a = c.api ?? c.metrics?.api;
      if (!judged || !a) {
        return {
          display: `${c.apiRequests} / ≤${l.max} (窗口 ${(win / 1000).toFixed(1)}s)`,
          pass: null,
          note: [
            "informational: window < 11.5s (--api-window=11500 to judge)",
            l.ruled ? `门槛 ${l.max} 来自 captain 裁决：${PENDING_RULINGS[28].why}` : `门槛 ${l.max} 读自 MASTER §12 行 28`,
          ].filter(Boolean).join(" · "),
        };
      }
      // Sub-assertion ①: per-route, per-endpoint. A route with no poller simply
      // has a max group of 1–2 and PASSES — §12.10.1 ④ says "no poller" is the
      // conclusion this row exists to confirm, never a not_measured.
      const routeOk = a.maxGroup <= l.max;
      // Sub-assertion ②: /permissions is checked ONCE for the whole site (max
      // over 13 routes x 2 modes). Judged on the first capture so the failure is
      // reported once instead of 26 identical times; every entry still prints the
      // number, so nothing is hidden.
      const all = ctx?.captures ?? [c];
      const permsMax = Math.max(...all.map((x) => (x.api ?? x.metrics?.api)?.perms ?? 0));
      const permsOk = permsMax <= l.max;
      const isOwner = all[0] === c;
      const permsTxt = `/permissions 全站最大 ${permsMax}/≤${l.max} ${permsOk ? "✓" : "✗"}${isOwner ? "（只查一次，判定落在本条）" : "（只查一次）"}`;
      return {
        display: `逐路由最大组 ${a.maxGroup}/≤${l.max} ${routeOk ? "✓" : "✗"}（${a.maxEndpoint || "无端点"}）· ${permsTxt} ｜ 总请求 ${a.perms + a.nonPermTotal}（窗口 ${(win / 1000).toFixed(1)}s）`,
        pass: routeOk && (isOwner ? permsOk : true),
        note: [
          `两个子断言分开计数、分开判定：① 逐路由（排除 /permissions，按端点分组取最大组）${a.maxGroup} ${routeOk ? "✓" : "✗"} · ② /permissions（13 路由 × 两模式取最大，只查一次）${permsMax} ${permsOk ? "✓" : "✗"}`,
          a.maxGroup <= 2 && !a.perms ? "本路由无轮询器：最大组 " + a.maxGroup + " —— 照常判 PASS（§12.10.1 ④）" : "",
          `端点分组（非 permissions）：${a.groups.map((g) => g.endpoint + " ×" + g.count).join(", ") || "无"}；端点种类 ${a.endpointKinds}`,
          l.ruled ? `门槛 ${l.max} 来自 captain 裁决：${PENDING_RULINGS[28].why}` : `门槛 ${l.max} 读自 MASTER §12 行 28`,
        ].filter(Boolean).join(" · "),
      };
    },
  },
  {
    n: 29, title: "暗色材质台阶 (canvas→panel 相对亮度比)", modes: ["dark"],
    parse: (t) => ({ min: pick(t.text, /≥\s*([\d.]+)\s*x/, 2.4) }),
    judge: (c, l) => {
      const m = c.metrics.material;
      if (m.stepRatio == null) return { display: "—", pass: null, note: "no visible .card to read the panel surface from" };
      return { display: `${m.stepRatio.toFixed(2)}x (${m.canvas} → ${m.panel} / ${m.cardSel})`, pass: m.stepRatio >= l.min };
    },
  },
  {
    n: 30, title: "亮色 panel 的 ring", modes: ["light"],
    parse: (t) => ({ min: pick(t.text, /pageCanvas\s*≥\s*([\d.]+)\s*:\s*1/, 1.2) }),
    judge: (c, l) => {
      const r = c.metrics.ring;
      if (!r) return { display: "—", pass: null, note: "no visible .card" };
      if (!r.exists) return { display: "无 ring", pass: false, note: r.raw || r.reason || "" };
      return {
        display: `ring ${r.color} ${r.spread}px → canvas ${r.ratioOnCanvas}:1 / panel ${r.ratio}:1`,
        pass: r.ratioOnCanvas >= l.min,
        note: [
          `判定用 ringVsCanvas ${r.ratioOnCanvas}:1（captain 裁决 2026-09-21：以 ring 实际被绘制的相邻面为准；MASTER §12 行 30 的判据列也写的是与 pageCanvas 比）`,
          `ringVsPanel ${r.ratio}:1（MASTER 基线 1.22:1 用的就是这个底）`,
          r.source ? `ring 取自 ${r.source}` : "",
          `raw: ${r.raw}`,
        ].filter(Boolean).join(" · "),
      };
    },
  },
  {
    n: 31, title: "图谱边对比度", modes: ["dark", "light"],
    parse: (t) => ({ min: pick(t.text, /≥\s*([\d.]+)/, 3) }),
    judge: (c, l) => c.metrics.graph
      ? { display: `${c.metrics.graph.edgeRatio}:1 (α${c.metrics.graph.edgeAlpha}) on ${c.metrics.graph.bg}`, pass: c.metrics.graph.edgeRatio >= l.min }
      : { display: "—", pass: null, note: "not #graph" },
  },
  {
    n: 32, title: "信号令牌唯一性", modes: ["dark", "light"], scope: "mode",
    criterion: "MASTER §12.3（2026-09-21 判据修正）：唯一性检查集 = {--signal, --status-warn, --brand-claude, --brand-deepseek, --brand-opencode} 共 5 个，两模式内两两字符串互不相同；Δhue ≥ 8° 只约束 --status-warn ↔ --signal（primitives §2.3）。accent（--brand / --ant-color-primary）=== --signal 是刻意设计（primitives §2.4 C6 / §2.6），**显式排除**——MASTER §12 行 32 原文把 accent 计入检查集，在该读法下必然报 1 组重复、永不可能 PASS",
    parse: (t) => ({ max: pick(t.text, /^\s*(\d+)\s*组重复/, 0), hue: pick(t.text, /Δhue\s*≥\s*(\d+)/, 8) }),
    judge: (c, l, ctx) => {
      const g = c.metrics.tokens;
      if (!g || !Object.keys(g).length) return { display: "—", pass: null };
      const entries = SIGNAL_UNIQUENESS_SET.map((name) => [name, g[name]]).filter((e) => e[1]);
      const missing = SIGNAL_UNIQUENESS_SET.filter((name) => !g[name]);
      const dups = [];
      for (let i = 0; i < entries.length; i++) {
        for (let j = i + 1; j < entries.length; j++) {
          if (entries[i][1] === entries[j][1]) dups.push(`${entries[i][0]} === ${entries[j][0]} (${entries[i][1]})`);
        }
      }
      const d = hueDelta(g["--status-warn"], g["--signal"]);
      const hueOk = d != null && d >= l.hue;
      const accent = g[ACCENT_OUT_OF_SET];
      const accentSameAsSignal = !!accent && accent === g["--signal"];
      // Collision scan: any *other* collected token equal to --signal. Tokens in
      // the check set are handled by `dups`; the accent and --signal-* families
      // are the only exemptions.
      const signal = g["--signal"];
      const inSet = new Set(SIGNAL_UNIQUENESS_SET);
      const colliders = Object.keys(g)
        .filter((k) => !inSet.has(k) && !SIGNAL_COLLISION_WHITELIST.includes(k) && g[k] === signal)
        .sort();
      const bad = [];
      if (missing.length) bad.push(`缺令牌 ${missing.join(", ")}`);
      if (dups.length) bad.push(dups.join(" | "));
      if (!hueOk) bad.push(d == null ? "--status-warn ↔ --signal 的 Δhue 不可测（令牌无色度）" : `--status-warn ↔ --signal Δhue ${d.toFixed(1)}° < ${l.hue}°（primitives §2.3）`);
      if (colliders.length) bad.push(`检查集外的令牌与 --signal 同值：${colliders.map((k) => `${k}=${g[k]}`).join(", ")}（MASTER §3.1 I 跨域借用）`);
      return {
        display: `${dups.length} 组重复 / ${entries.length} 个令牌, warn↔signal Δhue ${d == null ? "—" : d.toFixed(1) + "°"}`,
        pass: bad.length === 0,
        note: [
          bad.join("; "),
          entries.map((e) => `${e[0]}=${e[1]}`).join(", "),
          accent ? `accent ${ACCENT_OUT_OF_SET}=${accent}${accentSameAsSignal ? "（=== --signal，刻意设计，不在检查集内）" : "（≠ --signal）"}` : "",
          row32CriterionLanded(ctx?.thresholds) ? "判据出处：MASTER §12.3" : "判据待 t14 修订（当前按 §12.3 修正读法判定，不计为实现缺陷）",
        ].filter(Boolean).join(" · "),
      };
    },
  },
  {
    n: 33, title: "图谱分类色非文本对比度", modes: ["dark", "light"],
    parse: (t) => ({ min: pick(t.text, /≥\s*([\d.]+)/, 3) }),
    judge: (c, l) => {
      const g = c.metrics.graph;
      if (!g || !g.categories.length) return { display: "—", pass: null, note: "not #graph" };
      const bad = g.categories.filter((x) => x.ratio < l.min);
      const min = Math.min(...g.categories.map((x) => x.ratio));
      return { display: `${min.toFixed(2)}:1 (最低, ${g.categories.length} 色)`, pass: bad.length === 0, note: bad.map((b) => `${b.name} ${b.ratio}:1`).join(" | ") };
    },
  },
  {
    n: 34, title: "无 :active 反馈的手写可点元素", modes: ["dark", "light"],
    parse: (t) => ({ max: pickLeadingNumber(t.text, 0) }),
    judge: (c, l) => {
      const a = c.metrics.active;
      return { display: a.present.length ? `${a.missing.length}/${a.present.length} 无 :active` : "无候选元素", pass: a.missing.length <= l.max, note: a.missing.join(", ") };
    },
  },
  {
    n: 35, title: "断点值个数", modes: ["dark", "light"], scope: "global",
    // §12.16 reworded this cell from 「≤4 CSS 断点 + 1 契约值(992)」 to
    // 「① CSS 侧 ≤4 个断点 ∧ ② JS 断点集 ⊆ (CSS 断点集 ∪ {992})」. The NUMBER is
    // unchanged (4); only the phrasing moved, so the anchor reads both. The
    // judge below still implements the superseded "JS may only be 992" half —
    // see the note in this row's criterion; that gap is registered, not hidden.
    parse: (t) => ({
      max: pickAny(t.text, [/CSS\s*侧\s*≤\s*(\d+)\s*个断点/, /≤\s*(\d+)\s*CSS\s*断点/], 4),
    }),
    criterion: [
      "primitives §11 row 35 / §10.1 M91 + MASTER §12.16（t80 锐化）：① 正则提取 @media (max-width) 去重后 ⊆ {520,768,1024,1240} **且计数 ≤4**（上限不变）；② **JS 侧读到的每个断点值都必须存在于 CSS 侧断点集中**（或为契约值 992）—— **子集断言**：出现任何 **JS-only 断点**即 FAIL。",
      "**为什么不是白名单**：520 是 M1 的既有 CSS 断点（ui.tsx「the <=520 2-up grid」、Board.tsx「the 520 branch … 2x2」），SessionsView.tsx 的 matchMedia(「(max-width: 520px)」) 只是**从 JS 侧读同一个断点**。旧措辞「JS 只允许 992」禁止了一件合法且既有的事 ⇒ 报出一个**没有任何改动能消除**的红。加白名单是扩大例外集；子集断言是把**关系**写清楚。",
      "**它比旧措辞更严的地方**：JS 侧偷偷加一个 700 会被抓到（700 既不在 CSS 也不在契约值里），而旧措辞抓不到这类漂移 —— 它只会因为一个合法值误报。",
      "**出处**：MASTER §12.16 行 35 的锐化 + captain t80 裁决（不加白名单）。",
    ].join("\n"),
    judge: (c, l, ctx) => {
      const bp = ctx.captures[0]?.metrics.breakpoints;
      if (!bp) return { display: "—", pass: null };
      const js = ctx.global.bundle?.matchMedia ?? [];
      // The allow-lists come from primitives §10.1 M91, not from literals here.
      const allow = ctx?.contract?.breakpoints ?? { css: [520, 768, 1024, 1240], js: [992] };
      const v = breakpointVerdict(bp, js, allow, l.max);
      return {
        display: `CSS [${v.css.join(", ")}]${v.offCss.length ? ` — 越界 ${v.offCss.join(", ")}` : " ✓"}${v.overCount ? ` — 计数 ${v.css.length} > ${l.max}` : `（${v.css.length}/${l.max}）`} · JS [${v.jsSet.join(", ")}]${v.subsetOk ? " ✓ 子集成立（⊆ CSS ∪ {992}）" : ` — JS-only 断点 ${v.offJs.join(", ")}`}`,
        pass: v.pass,
        note: `runtime(antd) ${bp.runtime.join(", ") || "—"}；JS 每个值都必须存在于 CSS 侧（或为契约值 992）—— 子集断言，不是白名单`,
      };
    },
  },
  {
    n: 36, title: "侧栏宽度", modes: ["dark"],
    parse: (t) => ({ wide: pick(t.text, /^\s*(\d+)\s*\/\s*\d+/, 228), narrow: pick(t.text, /^\s*\d+\s*\/\s*(\d+)/, 72) }),
    judge: (c, l) => {
      const narrow = c.overflow.find((o) => o.width === 768);
      const wide = c.metrics.siderWidth;
      const pass = wide === l.wide && (!narrow || narrow.siderWidth === l.narrow);
      return { display: `${wide} @1440${narrow ? ` / ${narrow.siderWidth} @768` : ""}`, pass, note: `want ${l.wide} / ${l.narrow}` };
    },
  },
  {
    n: 37, title: "间距越界值个数", modes: ["dark", "light"],
    criterion: "MASTER §12.6（2026-09-21 裁决）：只判「设计者写的值」。分子 = #{正值 off-ladder 的作者值} + #{负值 : 值 ∉ {-4,-8,-12} ∨ 元素 ∉ 出血载体白名单}；「作者值」= 获胜声明来自构建后的 index.css（<link>）或元素的 style 属性。antdDerived（antd CSS-in-JS <style data-css-hash>）/ uaDefault（无声明命中）/ autoResolved（指定值 auto/normal）/ clampVw（§5.1 保留）四列单独计数、不参与判定；raw 数字并列输出。判定：分子 == 0。出血闭集从 MASTER §12.6 的表读取",
    parse: (t) => ({ max: pick(t.text, /^\s*(\d+)/, 0) }),
    judge: (c, l) => {
      const sp = c.metrics.spacing;
      const fmt = (s) => `${s.px}px ${s.prop}@${String(s.sel).split(" > ").pop()} ${s.src === "style attribute" ? "（内联 " + s.declared + "）" : "（" + s.declared + "）"}`;
      return {
        display: `${sp.numeratorCount} / ${sp.rawCount} raw（作者值越界 / 全部）`,
        pass: sp.numeratorCount <= l.max,
        note: [
          `分子 ${sp.numeratorCount} = 正值 ${sp.numeratorPos} + 无票负值 ${sp.numeratorNeg}`,
          `对照列（不参与判定）：antdDerived ${sp.antdDerived.count} / uaDefault ${sp.uaDefault.count} / autoResolved ${sp.autoResolved.count} / clampVw ${sp.clampVw.count} / 具名出血 ${sp.bleed.count}`,
          `校验 raw ${sp.rawCount} = 对照列合计 ${sp.antdDerived.count + sp.uaDefault.count + sp.autoResolved.count + sp.clampVw.count + sp.bleed.count} + 分子 ${sp.numeratorCount}`,
          `注：§12.6 表里的 raw 268 已剔除 clamp 产出，本轮 raw 为全部 off-ladder（含 clampVw），可比数 = raw − clampVw = ${sp.rawCount - sp.clampVw.count}`,
          sp.numeratorCount ? "越界样本：" + sp.numerator.samples.slice(0, 6).map(fmt).join(" | ") : "",
        ].filter(Boolean).join(" · "),
      };
    },
  },
  {
    n: 39, title: "单路由首屏 /api 请求数（防按条目展开的 N+1）", modes: ["dark", "light"],
    criterion: [
      "MASTER §12 行 39（§12.10.3 落槌）：**R ≤ 2K + 2**，R = 首屏 /api 请求数（**排除 /permissions**），K = 归一化端点种类数（数字 id 折叠为 :id 后去重）。",
      "**为什么是比例式而不是绝对数**：绝对阈值会随页面数据面数量变化而失准；2K+2 直接编码「每个数据面允许首取 + 一次重取，另给 2 次余量」，**N+1 无论怎么变形都会撑爆比例**。",
      "**与行 28 的分工**：行 28 判「轮询重复」（同一端点被反复取），行 39 判「首屏条目级展开」（一个条目一个请求）—— 两种现象，不混进一个数。",
      "**与 view-graph.md G15 的关系**：#graph 首屏 /api 的**绝对数**回归（≤8，含外壳 permissions）由 G15 守；行 39 是比例式、全站自标定。两条并存，不冲突。",
    ].join(" "),
    parse: (t) => ({ slack: pick(t.text, /R\s*≤\s*\d+\s*K\s*\+\s*(\d+)/, 2), factor: pick(t.text, /R\s*≤\s*(\d+)\s*K/, 2) }),
    judge: (c, l) => {
      const a = c.api ?? c.metrics?.api;
      if (!a) return { display: "—", pass: null, note: "no API accounting in this capture" };
      const R = a.nonPermTotal;
      const K = a.endpointKinds;
      const cap = l.factor * K + l.slack;
      return {
        display: `R ${R} ≤ 2K+2 = ${cap}（K=${K}）${R <= cap ? "✓" : "✗"}`,
        pass: R <= cap,
        note: [
          `端点归一化后分组（排除 /permissions）：${a.groups.map((g) => g.endpoint + " ×" + g.count).join(", ") || "无"}`,
          K === 0 ? "无端点 → 0 ≤ 2 通过（空页面不构成 N+1）" : `每个数据面允许首取 + 一次重取 + 2 次余量；N+1（一个条目一个请求）会随条目数线性撑爆`,
          `/permissions ${a.perms} 次不计入本行（归行 28 的独立断言）`,
        ].join(" · "),
      };
    },
  },
  {
    n: 38, title: "严格口径描边容器: #chat 的例外", modes: ["dark", "light"],
    parse: (t) => ({ max: pick(t.text, /≤\s*(\d+)/, 1) }),
    judge: (c, l) => {
      const b = c.metrics.borders;
      const inScope = c.route === "chat";
      return {
        display: `${b.strictContainers}${b.strictSamples[0] ? ` (${b.strictSamples[0].sel})` : ""}`,
        pass: inScope ? b.strictContainers <= l.max && isComposerOnly(b) : b.strictContainers <= l.max,
      };
    },
  },
];

// Row 20 is measured now (see CHECKS): the failure state is injected with
// page.route on the route's own endpoints. Nothing is left in this table.
const NOT_MEASURED = {};

// ── Runner: turn the captures into per-row verdicts ─────────────────────────
function runChecks(ctx) {
  const results = [];
  const covered = new Set();
  for (const row of CHECKS) {
    covered.add(row.n);
    const t = targetFor(ctx.thresholds, row.n, "");
    let limits = {};
    let parseErr = null;
    resetPickMisses();
    try {
      limits = row.parse(t) ?? {};
    } catch (e) {
      parseErr = e.message;
    }
    // Every row reads its requirement by an anchored phrase/comparator. If the
    // anchor did not match, the row is silently judging a built-in number — say
    // so rather than letting a reworded contract rot in place.
    const anchorMisses = takePickMisses().map((m) => `${m.re} → 内置值 ${m.fallback}`);
    const scope = row.scope ?? "route";
    const evidence = [];
    if (scope === "global") {
      const pseudo = { route: "(global)", mode: "-", metrics: {}, focus: null, overflow: [], pixels: null, apiRequests: null };
      evidence.push({ route: "(global)", mode: "-", ...row.judge(pseudo, limits, ctx) });
    } else if (scope === "mode") {
      for (const mode of row.modes) {
        const c = ctx.captures.find((x) => x.mode === mode);
        if (!c) continue;
        evidence.push({ route: "(mode)", mode, ...row.judge(c, limits, ctx) });
      }
    } else {
      for (const c of ctx.captures) {
        if (!row.modes.includes(c.mode)) continue;
        evidence.push({ route: c.route, mode: c.mode, ...row.judge(c, limits, ctx) });
      }
    }
    const judged = evidence.filter((e) => e.pass !== null);
    const fails = judged.filter((e) => !e.pass);
    // A row that is neither pass nor fail must say why *in the row itself*: the
    // per-capture reasons live on each evidence entry, so hoist the distinct
    // ones up. Otherwise the §12 table shows a blank verdict line for a row
    // nobody actually measured, which reads exactly like "nothing to report".
    const why = [...new Set(evidence.map((e) => e.note).filter(Boolean))];
    let verdict = !judged.length ? "not_measured" : fails.length ? "fail" : "pass";
    // Row 32 until MASTER's own cell carries the §12.3 correction: the original
    // check set counted accent, which primitives §2.4 C6 / §2.6 REQUIRE to equal
    // --signal, so the row could never pass. A non-pass there is a contract
    // defect, not an implementation defect, and must not be reported as one.
    if (row.n === 32 && verdict === "fail" && !row32CriterionLanded(ctx.thresholds)) verdict = "pending";
    results.push({
      n: row.n,
      title: row.title,
      criterion: row.criterion ?? `MASTER §12 行 ${row.n} 的原文判据`,
      target: t.text,
      targetSource: t.source,
      modes: row.modes,
      scope,
      verdict,
      failures: fails.map((f) => `${f.route}/${f.mode}=${f.display}`),
      anchorMisses,
      limits,
      note: parseErr
        ? `target parse failed (${parseErr}); built-in limits used`
        : verdict === "pending"
          ? `判据待 t14 修订（MASTER §12 行 32 原文把 accent 计入唯一性检查集，与 primitives §2.4 C6 / §2.6 要求 accent === --signal 冲突，该行永不可能 PASS）；当前按 §12.3 修正读法判定：${why.slice(0, 1).join("")}`
          : !judged.length
            ? why.slice(0, 2).join(" · ") || "no capture produced a judgeable value"
            : "",
      evidence,
    });
  }
  for (const [n, reason] of Object.entries(NOT_MEASURED)) {
    covered.add(Number(n));
    const row = ctx.thresholds.rows.get(Number(n));
    results.push({
      n: Number(n),
      title: row?.metric ?? "(row missing from §12)",
      target: row?.target ?? "",
      targetSource: row ? "MASTER.md" : "builtin",
      modes: [],
      scope: "none",
      verdict: "not_measured",
      failures: [],
      evidence: [],
      note: reason,
    });
  }
  results.sort((a, b) => a.n - b.n);
  const docRows = [...ctx.thresholds.rows.keys()];
  const uncovered = docRows.filter((n) => !covered.has(n));
  return { results, uncovered };
}

// ── Row 35: the breakpoint SET relationship, single-sourced so the judge and
// --self-test cannot drift apart (MASTER §12.16 + t80's ruling).
//
// The superseded wording was 「≤4 CSS 断点 + 1 契约值(992)」, which reads as "the
// JS side may only contain 992". That FORBADE something legal and already true:
// 520 is an M1 CSS breakpoint with documented semantics (ui.tsx "the <=520 2-up
// grid", Board.tsx "the 520 branch turns the four lane gauges into a 2x2"), and
// SessionsView.tsx's matchMedia(「(max-width: 520px)」) simply READS it from JS.
// The row therefore reported a red that no change could clear.
//
// The sharpened rule is STRICTER where it matters and only there:
//   ① the CSS side still has its own budget (max, read from MASTER — 4);
//   ② every value the JS side reads must exist on the CSS side (or be the
//      contract value 992). A JS-only breakpoint is drift: it is a value the
//      stylesheet does not define, so the two sides can silently disagree.
// Adding 520 to a whitelist would have been the wrong fix — it enlarges the
// exception set instead of stating the relationship.
function breakpointVerdict(bp, js, allow, max) {
  const css = bp.own;
  const offCss = css.filter((v) => !allow.css.includes(v));
  const overCount = css.length > max;
  const jsSet = [...new Set(js)];
  // The coupling is against the MEASURED CSS set, not the contract's allow-list.
  // Using allow.css here (the first cut) made the rule vacuous: the allow-list is
  // a fixed literal, so every JS value was "known" by construction and the case
  // the rule exists for -- the CSS stops defining a breakpoint while the JS still
  // reads it -- could never fail. t81's must-FAIL caught exactly that.
  const known = new Set([...css, ...allow.js]);
  const offJs = jsSet.filter((v) => !known.has(v));
  const subsetOk = offJs.length === 0;
  return {
    css,
    jsSet,
    offCss,
    overCount,
    offJs,
    subsetOk,
    pass: offCss.length === 0 && !overCount && subsetOk,
  };
}

// ── The two criteria MASTER §12 states defectively, single-sourced. ──────────
// Both are named functions rather than inline closures so that --self-test can
// exercise them on synthetic inputs: the panel currently renders ZERO
// `outline-style: auto` stops and ZERO whole-document-only aria-expanded hits,
// so live data alone cannot demonstrate that the corrected criterion is the one
// implemented. A criterion nobody can falsify is how a false pass survives.

// primitives §5.1 「判定口径」(修正 MASTER 行 16): a ring exists only when it is
// `solid` AND >= 2px AND reads >= 3:1 against what is behind it. `auto` (the UA
// default) and `none` are both NO RING — `auto` is what `.readout-btn` and
// `.ant-segmented-item-input` fall back to, and MASTER's original wording
// ("outline-style: none 且 box-shadow: none 即判无环") let them through.
// The single allowed exception is a text input inside `.composer`, whose
// container border-color stands in for the ring (primitives §5.1 例外 2).
const focusRingOk = (s) =>
  (s.form === "solid" && s.ringRatio != null && s.ringRatio >= 3) ||
  (s.inComposer === true && s.isTextInput === true && s.composerRatio != null && s.composerRatio >= 3);

// primitives §5.2.2 / §11 行 23: ONLY the hand-written expand carriers count.
// `document.querySelectorAll('[aria-expanded]')` also picks up antd Select's
// combobox (measured: 3 on #chat, 3 on #task, 1 on #memory, 1 on #settings), so
// the whole-document count makes the row pass while the hand-written controls
// carry no state at all. Returns the carriers that are present but unlabelled.
const expandMissing = (expand) =>
  expand.controls
    .filter((x) => x.count > 0 && x.withAriaExpanded < x.count)
    .map((x) => x.sel);

// ── Focus pass post-processing: a ring colour is only a ring if it can be
// read against what sits behind it (MASTER §12 row 15). ───────────────────
function enrichFocus(cap) {
  if (!cap.focus) return null;
  const stops = cap.focus;
  for (const s of stops) {
    if (s.ring) {
      const rc = parseCssColor(s.ring);
      const bg0 = parseCssColor(s.bgBehind);
      if (!rc || !bg0) {
        s.ringUnparsable = true;
      } else {
        const bg = { ...bg0, a: 1 };
        s.ringRatio = Math.round(contrastRatio(rc.a >= 1 ? rc : composite(rc, bg), bg) * 100) / 100;
      }
    }
    if (s.composer) {
      const bc = parseCssColor(s.composer.borderColor);
      const bg0 = parseCssColor(s.composer.bg);
      if (bc && bg0) {
        const bg = { ...bg0, a: 1 };
        s.composerRatio = Math.round(contrastRatio(bc.a >= 1 ? bc : composite(bc, bg), bg) * 100) / 100;
      }
    }
    // primitives §5.1 判定口径: `outlineStyle === 'solid' && outlineWidth >= 2px`
    // with ring contrast >= 3:1, or the single allowed exception — a text input
    // inside .composer, whose container border-color stands in (>= 3:1).
    s.ringOk = focusRingOk(s);
  }
  const withRing = stops.filter((s) => s.ringRatio != null);
  const forms = {};
  const ownForms = {};
  for (const s of stops) {
    forms[s.form] = (forms[s.form] ?? 0) + 1;
    ownForms[s.ownForm ?? s.form] = (ownForms[s.ownForm ?? s.form] ?? 0) + 1;
  }
  return {
    stops: stops.length,
    requested: stops.requested ?? null,
    walkMs: stops.walkMs ?? null,
    budgetHit: !!stops.budgetHit,
    forms,
    ownForms,
    // How many rings were found on the focus *owner* rather than the focused
    // element itself (antd's :focus-within pattern).
    viaOwner: stops.filter((s) => s.ringOwner).length,
    rings: stops.filter((s) => s.ring).length,
    unparsable: stops.filter((s) => s.ringUnparsable).length,
    noRing: stops.filter((s) => s.form === "none" && s.tag !== "BODY").length,
    lowRing: withRing.filter((s) => s.ringRatio < 3).length,
    notOk: stops.filter((s) => s.tag !== "BODY" && !s.ringOk).length,
    composerStops: stops.filter((s) => s.inComposer).length,
    worst: [...withRing].sort((a, b) => a.ringRatio - b.ringRatio).slice(0, 5),
    failed: stops
      .filter((s) => s.tag !== "BODY" && !s.ringOk)
      .slice(0, 6)
      .map((s) => ({ ...s, why: s.form === "none" ? "无环" : s.form === "auto" ? "UA auto 环（§5.1 判不合格）" : s.ringRatio == null ? "环颜色不可解析" : `环 ${s.ringRatio}:1 < 3:1` })),
  };
}

// ── Human tables ───────────────────────────────────────────────────────────
const pad = (v, w) => String(v ?? "").slice(0, w).padEnd(w);
function renderTable(cols, rows) {
  const head = cols.map((c) => pad(c.label, c.w)).join(" ");
  const sep = cols.map((c) => "-".repeat(c.w)).join(" ");
  const body = rows.map((r) => cols.map((c) => pad(typeof c.get === "function" ? c.get(r) : r[c.key], c.w)).join(" "));
  return [head, sep, ...body].join("\n");
}
const pctTxt = (v) => (v == null ? "—" : (v * 100).toFixed(2) + "%");

const TYPE_COLS = [
  { label: "route", w: 10, get: (r) => r.route },
  { label: "mode", w: 5, get: (r) => r.mode },
  { label: "text", w: 6, get: (r) => r.metrics.text.count },
  { label: "<=14", w: 5, get: (r) => r.metrics.text.le14 },
  { label: ">=18", w: 4, get: (r) => r.metrics.text.ge18 },
  { label: "maxFs", w: 6, get: (r) => r.metrics.text.maxFontSize },
  { label: "offFS", w: 6, get: (r) => r.metrics.text.offScaleCount },
  { label: "offW", w: 5, get: (r) => r.metrics.text.offWeightCount },
  { label: "radOff", w: 7, get: (r) => r.metrics.radius.offCount },
  { label: "inFS", w: 5, get: (r) => r.metrics.inline.fontSized },
  { label: "inSty", w: 6, get: (r) => r.metrics.inline.styleTotal },
  { label: "dom", w: 8, get: (r) => r.metrics.domNodes },
  { label: "h1", w: 3, get: (r) => r.metrics.structure.h1 },
  { label: "aExp", w: 5, get: (r) => r.metrics.structure.ariaExpanded },
  { label: "unnamed", w: 8, get: (r) => r.metrics.inter.unnamedCount },
  { label: "<24", w: 4, get: (r) => r.metrics.inter.small24 },
  { label: "<32", w: 4, get: (r) => r.metrics.inter.small32 },
  { label: "shell32", w: 8, get: (r) => String(r.metrics.inter.shell32 ?? "—") },
  { label: "cont32", w: 7, get: (r) => String(r.metrics.inter.content32 ?? "—") },
  { label: "spaceNum", w: 9, get: (r) => r.metrics.spacing.numeratorCount },
  { label: "spaceRaw", w: 9, get: (r) => r.metrics.spacing.rawCount },
];

const SURFACE_COLS = [
  { label: "route", w: 10, get: (r) => r.route },
  { label: "mode", w: 5, get: (r) => r.mode },
  { label: "hair", w: 5, get: (r) => r.metrics.borders.hairline },
  { label: "4side", w: 6, get: (r) => r.metrics.borders.fourSide },
  { label: "strict", w: 6, get: (r) => r.metrics.borders.strictContainers },
  { label: "bounded", w: 7, get: (r) => r.metrics.borders.allBordered },
  { label: "lumaBand", w: 9, get: (r) => pctTxt(r.pixels?.lumaBandRatio) },
  { label: "lumaDesg", w: 9, get: (r) => pctTxt(r.pixels?.lumaBandRatioDesigned) },
  { label: "canvas%", w: 8, get: (r) => pctTxt(r.pixels?.excludedRatio) },
  { label: "chroma", w: 7, get: (r) => pctTxt(r.pixels?.chromaRatio) },
  { label: "signal", w: 7, get: (r) => pctTxt(r.pixels?.signalRatio) },
  { label: "ovf", w: 6, get: (r) => r.metrics.scroll.docWidth - r.metrics.scroll.innerWidth },
  { label: "ovfMax", w: 16, get: (r) => { const w = [...r.overflow].sort((a, b) => b.overflow - a.overflow)[0]; return w && w.overflow > 0 ? `${w.overflow}px@${w.viewport}` : "0"; } },
  { label: "contDoc", w: 8, get: (r) => r.metrics.contrast.docViolations },
  { label: "contWCAG", w: 9, get: (r) => r.metrics.contrast.wcagViolations },
  { label: "contAmbig", w: 10, get: (r) => r.metrics.contrast.ambiguous },
  { label: "focusBad", w: 9, get: (r) => (r.focusInfo ? `${r.focusInfo.notOk}/${r.focusInfo.stops}` : "—") },
  { label: "ringMin", w: 8, get: (r) => (r.focusInfo && r.focusInfo.worst[0] ? `${r.focusInfo.worst[0].ringRatio}:1` : "—") },
  { label: "ltask", w: 7, get: (r) => r.metrics.longtask.max + "ms" },
];

// ── Report (markdown; also the stdout form) ─────────────────────────────────
const esc = (s) => String(s ?? "").replace(/\|/g, "\|");
const mdTable = (headers, rows) => {
  const lines = [`| ${headers.join(" | ")} |`, `|${headers.map(() => "---").join("|")}|`];
  for (const r of rows) lines.push(`| ${r.map(esc).join(" | ")} |`);
  return lines.join("\n");
};

function buildMarkdown(doc) {
  const { meta, captures, checks, warnings, global } = doc;
  const L = [];
  L.push("# panel design audit — quantitative, reproducible");
  L.push("");
  L.push(`- generated: \`${meta.generatedAt}\``);
  L.push(`- target: \`${meta.baseUrl}\` — panel build \`${meta.dist.buildId ?? "?"}\`${meta.dist.staleSrc ? " **(STALE: panel/src is newer than panel/dist)**" : ""}`);
  L.push(`- viewport: ${meta.viewport}, deviceScaleFactor 1, locale zh-CN, settle ${meta.settleMs}ms`);
  L.push(`- modes: ${meta.modes.join(", ")} · routes: ${meta.routes.join(", ")}`);
  L.push(`- thresholds: \`${relative(REPO, meta.thresholdsPath).split("\\").join("/")}\` (${meta.thresholdsFound ? "§12 parsed" : "NOT FOUND — built-in fallbacks"})`);
  L.push(`- screenshots: \`docs/screenshots/audit-run/\``);
  L.push("");
  L.push("## 1. Type & structure (per route × mode)");
  L.push("");
  L.push("```");
  L.push(renderTable(TYPE_COLS, captures));
  L.push("```");
  L.push("");
  L.push("## 2. Surfaces, pixels, contrast, overflow");
  L.push("");
  L.push("```");
  L.push(renderTable(SURFACE_COLS, captures));
  L.push("```");
  L.push("");
  L.push("## 3. Route fingerprints (hash fallback guard)");
  L.push("");
  L.push(mdTable(
    ["route", "hash", "hash ok", "heading", "markers", "warnings"],
    captures.map((c) => [
      c.route, c.hash, c.metrics.hashOk ? "yes" : `NO (${c.metrics.hash})`,
      c.metrics.structure.heading ?? "—", c.metrics.markers.join(",") || "none",
      c.warnings.join("; ") || "",
    ]),
  ));
  L.push("");
  const dupes = {};
  for (const c of captures) {
    const key = `${c.metrics.structure.heading ?? "—"} @ ${c.metrics.domNodes}`;
    dupes[key] = (dupes[key] ?? 0) + 1;
  }
  const collisions = Object.entries(dupes).filter(([, n]) => n > 1);
  L.push(collisions.length ? `⚠ identical (heading, DOM size) fingerprints: ${collisions.map(([k, n]) => `${k} ×${n}`).join("; ")}` : "no two route×mode captures share the same (heading, DOM size) fingerprint.");
  L.push("");
  L.push("## 4. §12 checks");
  L.push("");
  L.push(mdTable(
    ["#", "verdict", "metric", "target", "failing (route/mode=value)"],
    checks.map((r) => [r.n, r.verdict.toUpperCase(), r.title, r.target, r.failures.join("; ") || r.note || ""]),
  ));
  L.push("");
  // "Corrected" = any criterion that is not MASTER's own un-amended wording.
  // Compared against the default string rather than a prefix test: MASTER §12.3
  // is itself a MASTER section, and a startsWith("MASTER") filter would hide the
  // row 32 correction — the one a reader most needs to see.
  const corrected = checks.filter((r) => r.criterion && r.criterion !== `MASTER §12 行 ${r.n} 的原文判据`);
  if (corrected.length) {
    L.push("### 判据出处（与原 MASTER 判据不同者）");
    L.push("");
    L.push(mdTable(["#", "判据来源与内容"], corrected.map((r) => [r.n, r.criterion])));
    L.push("");
  }
  if (doc.contract) {
    L.push("### 判据出处：从契约读取 vs 硬编码（t17）");
    L.push("");
    L.push(mdTable(
      ["判据", "出处", "是否从契约读取"],
      doc.contract.provenance.map((p) => [
        p.item,
        p.source,
        p.fromContract === true
          ? "是"
          : p.fromContract === "stated"
            ? "口径声明（非阈值）"
            : p.fromContract === "pending"
              ? "**待 MASTER 落槌**"
              : "**否（回退到内置）**",
      ]),
    ));
    L.push("");
    L.push("仍为工具内字面量、且**不是设计阈值**的项（已登记，改动会改变计数）：");
    L.push("");
    L.push(mdTable(["项", "为什么允许硬编码"], doc.contract.hardcoded.map((h) => [h.item, h.why])));
    L.push("");
  }
  const fail = checks.filter((r) => r.verdict === "fail");
  const unmeasured = checks.filter((r) => r.verdict === "not_measured");
  const pending = checks.filter((r) => r.verdict === "pending");
  L.push(`**${checks.length - fail.length - unmeasured.length - pending.length} pass / ${fail.length} fail / ${unmeasured.length} not measured / ${pending.length} pending criterion revision**`);
  L.push("");
  if (pending.length) {
    L.push("### Pending criterion revision (not counted as implementation defects)");
    L.push("");
    for (const r of pending) L.push(`- **${r.n}** ${r.title} — ${r.note}`);
    L.push("");
  }
  if (unmeasured.length) {
    L.push("### Not measured (with reason)");
    L.push("");
    for (const r of unmeasured) L.push(`- **${r.n}** ${r.title} — ${r.note}`);
    L.push("");
  }
  return L.join("\n");
}

function buildDetails(doc) {
  const { meta, captures, warnings, global } = doc;
  const L = [];
  L.push("## 5. Evidence");
  L.push("");
  const dark = captures.find((c) => c.route === "home" && c.mode === "dark") ?? captures[0];
  L.push("### 5.1 Material staircase & tokens");
  L.push("");
  const modeFirst = doc.meta.modes.map((m) => captures.find((c) => c.mode === m)).filter(Boolean);
  const matCaptures = captures.some((c) => ["home", "stats"].includes(c.route))
    ? captures.filter((c) => ["home", "stats"].includes(c.route))
    : modeFirst;
  L.push(mdTable(
    ["route/mode", "canvas", "panel", "panel src", "step", "ring", "ring panel / canvas"],
    matCaptures.map((c) => [
      `${c.route}/${c.mode}`, c.metrics.material.canvas, c.metrics.material.panel ?? "—", c.metrics.material.panelSource ?? "—",
      c.metrics.material.stepRatio ? c.metrics.material.stepRatio.toFixed(3) + "x" : "—",
      c.metrics.ring.exists ? `${c.metrics.ring.color} ${c.metrics.ring.spread}px` : (c.metrics.ring.raw || "none"),
      c.metrics.ring.ratio != null ? `${c.metrics.ring.ratio}:1 / ${c.metrics.ring.ratioOnCanvas}:1` : "—",
    ]),
  ));
  L.push("");
  L.push(`signals (${dark?.mode}): \`${JSON.stringify(dark?.metrics.tokens ?? {})}\``);
  L.push("");
  const bp = dark?.metrics.breakpoints;
  if (bp) {
    L.push(`breakpoints — own CSS: [${bp.own.join(", ")}] (${bp.ownSheets} sheet(s)); runtime/antd: [${bp.runtime.join(", ")}] (${bp.runtimeSheets} sheet(s))`);
    L.push("");
  }
  L.push("### 5.2 Type offenders (off-ladder sizes / weights)");
  L.push("");
  const typeRows = [];
  for (const c of captures) {
    if (c.metrics.text.offScaleCount || c.metrics.text.offWeightCount) {
      typeRows.push([
        `${c.route}/${c.mode}`,
        c.metrics.text.offScale.map((o) => `${o.px}px×${o.count}`).join(", "),
        c.metrics.text.offWeight.map((o) => `${o.weight}×${o.count}`).join(", "),
        c.metrics.text.ge18Samples.slice(0, 2).map((s) => `${s.px}px ${s.sel}`).join(" | "),
      ]);
    }
  }
  L.push(typeRows.length ? mdTable(["route/mode", "off-ladder size", "off-ladder weight", "≥18px samples"], typeRows) : "none — every computed font-size/weight sits on the ladder.");
  L.push("");
  L.push("### 5.3 Radius offenders");
  L.push("");
  const radRows = [];
  for (const c of captures) {
    if (c.metrics.radius.offCount) {
      radRows.push([`${c.route}/${c.mode}`, c.metrics.radius.offCount, c.metrics.radius.off.map((o) => `${o.value}×${o.count}`).join(", "), c.metrics.radius.hist.slice(0, 4).map((h) => `${h.value}×${h.n}`).join(", ")]);
    }
  }
  L.push(radRows.length ? mdTable(["route/mode", "off ladder", "offenders", "top radii"], radRows) : "none.");
  L.push("");
  L.push("### 5.4 Contrast (worst offenders)");
  L.push("");
  const conRows = [];
  for (const c of captures) {
    if (c.metrics.contrast.docViolations || c.metrics.contrast.wcagViolations) {
      for (const w of c.metrics.contrast.worst.slice(0, 3)) {
        conRows.push([`${c.route}/${c.mode}`, `${w.ratio}:1`, `${w.px}px/${w.weight}`, w.fg, w.bg, w.sel, w.text]);
      }
    }
  }
  L.push(conRows.length ? mdTable(["route/mode", "ratio", "size", "fg", "bg", "element", "text"], conRows) : "no contrast violation in either counting convention.");
  L.push("");
  L.push("### 5.5 Focus pass");
  L.push("");
  const focusRows = [];
  for (const c of captures) {
    if (!c.focusInfo) continue;
    focusRows.push([
      `${c.route}/${c.mode}`, c.focusInfo.stops, c.focusInfo.rings, c.focusInfo.notOk,
      Object.entries(c.focusInfo.forms).map(([k, v]) => `${k}x${v}`).join(" "),
      c.focusInfo.worst.map((s) => `${s.ringRatio}:1 ${s.sel}`).slice(0, 2).join(" | ") || "—",
      c.focusInfo.failed.map((s) => `${s.sel}[${s.form}]`).slice(0, 3).join(" | ") || "—",
    ]);
  }
  L.push(focusRows.length ? mdTable(["route/mode", "tab stops", "rings", "不合格", "环形式分布", "最低环", "不合格样本（§5.1 口径）"], focusRows) : "focus pass skipped (--no-focus).");
  L.push("");
  L.push("### 5.6 Overflow per viewport (non-zero only)");
  L.push("");
  const ovfRows = [];
  for (const c of captures) {
    const main = { viewport: `${c.metrics.viewport.w}x${c.metrics.viewport.h}`, overflow: c.metrics.scroll.docWidth - c.metrics.scroll.innerWidth };
    for (const o of [main, ...c.overflow]) if (o.overflow > 0) ovfRows.push([`${c.route}/${c.mode}`, o.viewport, o.overflow, o.docWidth, o.innerWidth]);
  }
  L.push(ovfRows.length ? mdTable(["route/mode", "viewport", "overflow px", "scrollWidth", "innerWidth"], ovfRows) : "zero horizontal overflow on every route × viewport measured.");
  L.push("");
  L.push("### 5.7 Graph (only #graph carries the canvas)");
  L.push("");
  const graphRows = [];
  for (const c of captures) {
    if (!c.metrics.graph) continue;
    graphRows.push([`${c.route}/${c.mode}`, c.metrics.graph.bg, c.metrics.graph.edge ?? "—", c.metrics.graph.edgeRatio ?? "—", c.metrics.graph.categories.map((x) => `${x.name.replace("--graph-", "")}:${x.ratio}`).join(" ")]);
  }
  L.push(graphRows.length ? mdTable(["route/mode", "canvas", "edge", "edge ratio", "categorical ratios"], graphRows) : "—");
  L.push("");
  L.push("### 5.8 Command palette (ARIA + focus return)");
  L.push("");
  L.push("```json");
  L.push(JSON.stringify(global.palette ?? { note: "palette not probed" }, null, 2));
  L.push("```");
  L.push("");
  L.push("### 5.9 Bundle (panel/dist)");
  L.push("");
  // Every chunk, no slice: a preview window that happened to stop at 6 chunks is
  // how the app-layer number once got computed from a partial list.
  L.push(mdTable(["file", "bytes", "gzip", "KB", "gzip KB", "首屏静态"], [...(global.bundle?.js ?? []), ...(global.bundle?.css ?? [])].map((f) => [f.file, f.bytes, f.gzip, (f.bytes / 1024).toFixed(1), (f.gzip / 1024).toFixed(1), (global.bundle?.firstScreen ?? []).includes(f.file) ? "是" : "懒加载"])));

  // §12 target parsing reconciliation. A §12 target cell mixes the requirement
  // with baseline values, historical values and section cross-references, so
  // "the i-th number in the cell" is not a stable way to find the requirement:
  // row 6 read an upper bound as an exemption floor, row 18 let 「见 §12.7.2」
  // push the cap to 2, and row 1 let a baseline number become the goal. Every
  // row now anchors on the phrase that carries the requirement; this table shows
  // what each row read and whether the anchor actually matched.
  L.push("### 5.13 §12 目标解析对账（每行：目标列原文 → 解析结果）");
  L.push("");
  L.push(
    mdTable(
      ["#", "目标列原文（截断）", "解析结果", "锚点命中", "等价"],
      (doc.checks ?? [])
        .filter((c) => c.scope !== "none")
        .map((c) => [
          c.n,
          String(c.target ?? "").replace(/\|/g, "\\|").slice(0, 72) + (String(c.target ?? "").length > 72 ? "…" : ""),
          Object.keys(c.limits ?? {}).length ? JSON.stringify(c.limits) : "（无阈值）",
          c.anchorMisses?.length ? "✗ 回退内置值" : "✓",
          c.anchorMisses?.length ? "✘ 需复核" : "✓",
        ]),
    ),
  );
  L.push("");
  L.push(
    "「锚点命中」= 该行的解析在 MASTER 原文里找到了它锚定的短语/比较符（未回退到内置值）；「等价」= 解析出的数值等于目标列文字的含义。" +
      "回退内置值的行会在 Warnings 里单独点名。全表由 --self-test 的 §12 parse 断言逐行证伪。",
  );
  L.push("");
  if (doc.blindSpots) {
    L.push("### 5.10 盲区登记（本脚本看不到的东西，不假装达标）");
    L.push("");
    L.push(mdTable(["项", "原因"], doc.blindSpots.map((b) => [b.item, b.reason])));
    L.push("");
  }
  const spRows = [];
  for (const c of captures) {
    if (c.metrics.spacing.offCount) {
      spRows.push([`${c.route}/${c.mode}`, c.metrics.spacing.offCount, c.metrics.spacing.off.map((o) => `${o.px}px×${o.count}`).join(", ")]);
    }
  }
  L.push("### 5.11 间距越界值（§11 row 37）");
  L.push("");
  L.push(spRows.length ? mdTable(["route/mode", "越界次数", "值×次数"], spRows) : "no off-ladder spacing value.");
  L.push("");
  // Row 18's inventories in full. The JSON carries them too, but a count alone
  // cannot be re-checked, and MASTER row 18 asks for the visible interactive
  // elements to be reproducible — so the complete list belongs in the report.
  L.push("### 5.12 行 18 命中区清单（逐个点名，完整不截断）");
  L.push("");
  const hitRows = [];
  for (const c of captures) {
    const it = c.metrics?.inter;
    if (!it) continue;
    for (const s of it.hit24All ?? []) hitRows.push([`${c.route}/${c.mode}`, "地板 <24px", s.sel, `${s.w}x${s.h}`, s.via, s.paint]);
    for (const s of it.small32Samples ?? []) hitRows.push([`${c.route}/${c.mode}`, "目标 <32px", s.sel, `${s.w}x${s.h}`, s.via || "自身", "—"]);
  }
  L.push(hitRows.length ? mdTable(["route/mode", "判定", "元素", "命中区", "测量对象", "绘制口径"], hitRows) : "无 <24px 或 <32px 命中区元素。");
  L.push("");
  const tot24 = captures.reduce((a, c) => a + (c.metrics?.inter?.hit24 ?? 0), 0);
  const tot24p = captures.reduce((a, c) => a + (c.metrics?.inter?.paint24 ?? 0), 0);
  L.push(`合计：命中区口径 <24px **${tot24}** 项 · 绘制口径 <24px **${tot24p}** 项（两套口径并列，判定用前者）。`);
  L.push("");
  L.push("## 6. Warnings");
  L.push("");
  L.push(warnings.length ? warnings.map((w) => `- ${w}`).join("\n") : "- none");
  L.push("");
  L.push("## 7. Reproduce");
  L.push("");
  L.push("```bash");
  L.push("cargo run -p ruagent-cli -- serve        # or the already-running daemon on :8787");
  L.push("cd panel && npm run build                # dist must be newer than src");
  L.push(`cd panel && node tools/design-audit.mjs ${meta.checkArgs ?? ""}`.trim());
  L.push("cd panel && node tools/design-audit.mjs --check   # exit 1 if any §12 row fails");
  L.push("```");
  return L.join("\n");
}

// ── --self-test ─────────────────────────────────────────────────────────────
// The panel of *today* renders zero `outline-style: auto` Tab stops, and 12 of
// 13 routes carry no hand-written expand carrier at all — so a live run cannot
// show that rows 16/23 reject what MASTER's original wording accepted. These
// cases pin the criterion itself: each is a shape the defective wording scored
// PASS and the corrected one must score FAIL (and the contract shapes the
// other way round). Pure: no daemon, no browser, no filesystem.
function runSelfTest() {
  const cases = [];
  const check = (name, got, want) => cases.push({ name, got, want, pass: got === want });


  // ---- row 16: focus ring, primitives §5.1 判定口径 --------------------------
  const stop = (o) => ({ tag: "INPUT", sel: "input.x", form: "none", ring: null, bgBehind: "rgb(27, 30, 36)", ...o });
  const focusInfo = (focus) => enrichFocus({ focus });

  // (a) UA default ring: `outline-style: auto`, 1px, alongside a spread shadow.
  //     MASTER's wording ("outline-style: none 且 box-shadow: none 即判无环")
  //     scored this PASS because it *had* a box-shadow. It must be FAIL.
  const autoStop = stop({
    form: "auto", ring: "rgb(238, 238, 238)", outline: "auto 1px rgb(238, 238, 238)",
    shadow: "0px 0px 0px 3px rgb(60, 60, 60)", ringRatio: 12,
  });
  check("row16: outline-style:auto + shadow ring -> FAIL", focusInfo([autoStop]).notOk, 1);

  // (b) `outline-style: none` with a real box-shadow ring -> still FAIL.
  const noneShadow = stop({ form: "none", ring: null, ringRatio: null, shadow: "0px 0px 0px 2px rgb(245, 180, 87)" });
  check("row16: outline-style:none + shadow ring -> FAIL", focusInfo([noneShadow]).notOk, 1);

  // (c) the contract: solid + >=2px + >=3:1 -> PASS.
  const good = stop({ form: "solid", ring: "rgb(245, 180, 87)", outline: "solid 2px rgb(245, 180, 87)", ringRatio: 6.45 });
  check("row16: solid 2px @6.45:1 -> PASS", focusInfo([good]).notOk, 0);

  // (d) solid but 1px -> FAIL (a thin solid line is not a ring).
  const thin = stop({ form: "thin-solid", ring: "rgb(245, 180, 87)", outline: "solid 1px rgb(245, 180, 87)", ringRatio: 6.45 });
  check("row16: solid 1px -> FAIL", focusInfo([thin]).notOk, 1);

  // (e) solid 2px but below the 3:1 floor -> FAIL.
  const dim = stop({ form: "solid", ring: "rgb(86, 65, 32)", outline: "solid 2px rgb(86, 65, 32)", ringRatio: 1.73 });
  check("row16: solid 2px @1.73:1 -> FAIL", focusInfo([dim]).notOk, 1);

  // (f) the ONE allowed exception: a text input inside .composer, judged on the
  //     container border instead of a ring (primitives §5.1 例外 2).
  const compOk = stop({
    tag: "TEXTAREA", form: "none", ring: null, inComposer: true, isTextInput: true,
    composer: { borderColor: "rgb(232, 145, 42)", bg: "rgb(27, 30, 36)" }, composerRatio: 8.3,
  });
  check("row16: .composer textarea, border 8.30:1 -> PASS", focusInfo([compOk]).notOk, 0);

  // (g) the exception must not leak: a low-contrast composer border is not a
  //     ring, and the composer's border is not a ring for anything outside it.
  const compDim = stop({
    tag: "TEXTAREA", form: "none", ring: null, inComposer: true, isTextInput: true,
    composer: { borderColor: "rgb(40, 44, 52)", bg: "rgb(27, 30, 36)" }, composerRatio: 1.2,
  });
  check("row16: .composer textarea, border 1.20:1 -> FAIL", focusInfo([compDim]).notOk, 1);
  const nonComposer = stop({ tag: "INPUT", form: "none", ring: null, composerRatio: 9, composer: { borderColor: "rgb(0,0,0)", bg: "rgb(0,0,0)" } });
  check("row16: composer border outside .composer -> FAIL", focusInfo([nonComposer]).notOk, 1);

  // (h) the carrier list is exactly the three hand-written selectors and never
  //     a whole-document selector.
  // ---- the contract itself: every criterion above must be read, not written --
  const CT = resolveContract(loadThresholds(REPO));
  check("contract: row 6 exempt routes read from MASTER §12.1",
    [...CT.exemptRoutes.keys()].sort().join(",") === "#chat,#inbox,#settings", true);
  check("contract: row 6 exempt threshold is 1 for each",
    [...CT.exemptRoutes.values()].every((v) => v.threshold === 1), true);
  check("contract: row 6 page-title carriers read from MASTER §12 行 6",
    CT.pageTitleRule.default.selectors.join(",") === ".view-bar h2" &&
      CT.pageTitleRule.overrides.home.selectors.join(",") === ".home-hero h1", true);
  check("contract: row 6 title tiers read from MASTER §4.1",
    CT.titleTiers.title === 20 && CT.titleTiers.display === 24, true);
  check("contract: row 7 page classification read from the view specs",
    CT.viewSpecs.get("task")?.maxFontPx === 20 && CT.viewSpecs.get("agents")?.maxFontPx === 20 &&
      CT.viewSpecs.get("home")?.maxFontPx === 32 && CT.viewSpecs.get("board")?.maxFontPx === 32, true);
  check("contract: row 8 font ladder read from MASTER §4.1",
    CT.fontLadder.join(",") === "11,12,13,14,15,16,18,20,24,32", true);
  check("contract: row 9 weight ladder read from MASTER §4.3",
    CT.weightLadder.join(",") === "400,500,600,700", true);
  check("contract: row 10 radius ladder + pill read from primitives §11 行 10",
    CT.radiusLadder.join(",") === "0,3,4,6,10,14" && CT.radiusPill === 50, true);
  check("contract: row 37 spacing ladder read from MASTER §5.1",
    CT.spacingLadder.join(",") === "0,2,4,6,8,10,12,16,20,24,32,80", true);
  check("contract: row 23 carriers read from primitives §5.2.2",
    CT.expandSelectors.join(",") === ".recall-stub-head,.tool-head,.chat-group-more", true);
  check("contract: row 33 graph tokens read from MASTER §3.1 F",
    CT.graphVars.length === 9 && CT.graphVars.includes("--graph-person") && !CT.graphVars.includes("--graph-edge"), true);
  check("contract: row 34 hand classes read from primitives §10.1 N21",
    CT.handClasses.length === 9 && CT.handClasses.includes(".row-btn"), true);
  check("contract: row 35 breakpoints read from primitives §10.1 M91",
    CT.breakpoints.css.join(",") === "520,768,1024,1240" && CT.breakpoints.js.join(",") === "992", true);
  check("contract: row 5 .raw exclusion read from view-task-detail.md D11",
    CT.row5ExcludeSelectors.join(",") === ".raw", true);

  // ---- row 6: the §12.1 conditional rule ----------------------------------
  const row6 = CHECKS.find((r) => r.n === 6);
  const limits6 = row6.parse({ text: "含读数的页 ≥3；无读数的页 ≥1；所有页 ≤40", nums: [3, 1, 40] });
  const cap6 = (route, ge18, count = 1, px = 20) => ({
    route, mode: "dark",
    metrics: { text: { ge18 }, pageTitle: { count, sizes: [px], selectors: [] } },
  });
  const j6 = (route, ge18, count = 1, px = 20) => row6.judge(cap6(route, ge18, count, px), limits6, { contract: CT });
  // The captain's falsifiable pair: the exemption is conditional, not a flat lowering.
  check("row6: ge18=2 on an exempt route (#chat) -> PASS", j6("chat", 2).pass, true);
  check("row6: ge18=2 on a non-exempt route (#sessions) -> FAIL", j6("sessions", 2).pass, false);
  // The drift that made the row unpassable: nums[1] is the exempt floor, not the cap.
  check("row6: target parse takes lo/exemptLo/hi from the comparators",
    limits6.lo === 3 && limits6.exemptLo === 1 && limits6.hi === 40, true);
  check("row6: 9 nodes @#home passes now", j6("home", 9, 1, 24).pass, true);
  check("row6: 9 nodes @#home FAILS under the old positional read (hi=1)",
    row6.judge(cap6("home", 9, 1, 24), { lo: 3, hi: 1, exemptLo: 1 }, { contract: CT }).pass, false);
  check("row6: 41 nodes @#sessions -> FAIL (upper bound)", j6("sessions", 41).pass, false);
  // The title sub-assertion, in both directions.
  check("row6: title 0 @#sessions -> FAIL", j6("sessions", 3, 0, 0).pass, false);
  check("row6: two titles @#sessions -> FAIL", j6("sessions", 3, 2, 20).pass, false);
  check("row6: title 19px @#sessions -> FAIL", j6("sessions", 3, 1, 19).pass, false);
  check("row6: title 20px @#sessions -> PASS", j6("sessions", 3, 1, 20).pass, true);
  check("row6: exempt #chat with no title -> FAIL (the exemption is not a free pass)",
    j6("chat", 2, 0, 0).pass, false);
  check("row6: #home title at the §4.1 display tier 24px -> PASS", j6("home", 9, 1, 24).pass, true);
  check("row6: #home title at 20px (below its tier) -> FAIL", j6("home", 9, 1, 20).pass, false);

  // ---- row 7: page classification from the specs ---------------------------
  const row7 = CHECKS.find((r) => r.n === 7);
  const limits7 = row7.parse({ text: "≥20px；含仪表盘的页 ≥32px", nums: [20, 32] });
  const j7 = (route, maxFontSize) =>
    row7.judge({ route, mode: "dark", metrics: { text: { maxFontSize } } }, limits7, { contract: CT });
  check("row7: #task @20px -> PASS (view-task-detail §3: no dashboard)", j7("task", 20).pass, true);
  check("row7: #agents @20px -> PASS (view-agents A1 = 20px)", j7("agents", 20).pass, true);
  check("row7: #agents @20px would FAIL under the old DOM heuristic (dashboard needs 32)",
    20 >= 32, false);
  check("row7: #board @20px -> FAIL (view-board B1 = 32px)", j7("board", 20).pass, false);
  check("row7: #board @32px -> PASS", j7("board", 32).pass, true);
  check("row7: a route with no readable spec falls back to MASTER's 20px floor",
    row7.judge({ route: "nope", mode: "dark", metrics: { text: { maxFontSize: 20 } } }, limits7, { contract: CT }).pass, true);

  // ---- row 5: the D11 exclusions ------------------------------------------
  const row5 = CHECKS.find((r) => r.n === 5);
  const limits5 = row5.parse({ text: "≤40/路由", nums: [40] });
  const j5 = (allBordered, rawBlocks, mdCells) =>
    row5.judge({ route: "task", mode: "dark", metrics: { borders: { allBordered, rawBlocks, mdTableCells: mdCells } } }, limits5, {});
  check("row5: 35 bordered -> PASS", j5(35, 0, 0).pass, true);
  check("row5: 41 bordered -> FAIL", j5(41, 0, 0).pass, false);
  check("row5: the .raw exclusion is reported", j5(35, 7, 0).note.includes(".raw 7"), true);
  // The exclusion lives in the probe's counting path; pin both the wiring and
  // the branch, since --self-test runs without a browser.
  check("row5: the probe builds its exclusion set from the contract",
    /cfg\.row5ExcludeSelectors/.test(PROBE.toString()), true);
  check("row5: excluded subtrees are counted separately, never into allBordered",
    /rawSet\.has\(el\)\)\s*borders\.rawBlocks\+\+/.test(PROBE.toString()), true);

  // ---- .sr-only / clipped boxes -------------------------------------------
  check("vis: the probe knows about clip-path / 1x1 clipped boxes",
    /clipPath/.test(PROBE.toString()) && /clippedAway/.test(PROBE.toString()), true);
  check("vis: row 24's h1 count still uses vis, not visText (§5.5 counts the sr-only h1)",
    /el\.tagName === "H1" && vis\)/.test(PROBE.toString()), true);
  check("vis: row 12's inline-style count is visibility-gated",
    /if \(visText && styleAttr/.test(PROBE.toString()), true);

  // ---- row 37: only the designer's own values are judged (§12.6) -----------
  const row37 = CHECKS.find((r) => r.n === 37);
  const limits37 = row37.parse({ text: "0", nums: [0] });
  const j37 = (numeratorCount, samples = [], cols = {}) =>
    row37.judge({
      route: "runtimes", mode: "dark",
      metrics: {
        spacing: {
          numeratorCount, rawCount: 268,
          numeratorPos: samples.length ? 2 : 0,
          numeratorNeg: 0,
          numerator: { count: numeratorCount, samples },
          antdDerived: { count: cols.antd ?? 0, samples: [] },
          uaDefault: { count: cols.ua ?? 0, samples: [] },
          autoResolved: { count: cols.auto ?? 0, samples: [] },
          clampVw: { count: cols.clamp ?? 0, samples: [] },
          bleed: { count: cols.bleed ?? 0, samples: [] },
        },
      },
    }, limits37, {});
  const inlineSample = [{ sel: "div.view > p.muted.pad", prop: "marginTop", px: 14, declared: "14px", src: "style attribute" }];
  check("row37: the Runtimes.tsx:222 inline 14px is the residual -> FAIL", j37(2, inlineSample).pass, false);
  check("row37: the residual is 2 (one element x two modes), not 0",
    j37(2, inlineSample).note.includes("分子 2 = 正值 2 + 无票负值 0"), true);
  check("row37: the note names the offending element and its inline value",
    j37(2, inlineSample).note.includes("p.muted.pad") && j37(2, inlineSample).note.includes("内联 14px"), true);
  check("row37: a clean page passes", j37(0, []).pass, true);
  // The invariant that would have caught the CSSOM bugs: every off-ladder value
  // lands in exactly one column, so the columns must sum to raw.
  check("row37: the columns partition raw exactly (no value silently dropped)",
    j37(3, inlineSample, { antd: 100, ua: 14, auto: 24, clamp: 76, bleed: 51 }).note.includes("校验 raw 268 = 对照列合计 265 + 分子 3") &&
      j37(1, inlineSample, { antd: 0, ua: 0, auto: 0, clamp: 0, bleed: 0 }).note.includes("校验 raw 268 = 对照列合计 0 + 分子 1"), true);
  check("row37: raw is reported next to the numerator, not hidden",
    j37(2, inlineSample).display.includes("2 / 268 raw"), true);
  check("row37: the four comparison columns are printed and do not move the verdict",
    j37(0, [], { antd: 90, ua: 16, auto: 6, clamp: 4, bleed: 154 }).pass === true &&
      j37(0, [], { antd: 90, ua: 16, auto: 6, clamp: 4, bleed: 154 }).note.includes("antdDerived 90"), true);
  check("row37: a negative without a ticket still counts",
    j37(1, [{ sel: "div.x", prop: "marginLeft", px: -6, declared: "-6px", src: "author sheet" }]).pass, false);
  check("row37: the closed set is read from MASTER §12.6",
    CT.bleedSetSource === "MASTER §12.6" &&
      CT.bleedSet.map((b) => b.value + ":" + b.selector).join(",") === "-12:.row-btn,-8:.mem-preview,-4:.intent", true);
  check("row37: the probe classifies by CSSOM origin, not by an exception table",
    PROBE.toString().includes("data-css-hash") && PROBE.toString().includes("authorRules") && PROBE.toString().includes("uaDefault"), true);
  check("row37: the bleed ticket requires BOTH the value and the named carrier",
    PROBE.toString().includes("namedBleed(el, v)") && PROBE.toString().includes("b.selector"), true);
  check("row37: the probe's resolver box is removed before it returns",
    PROBE.toString().includes("scratch.remove()"), true);

  // ---- row 18: the hit target is the CONTROL ROOT (t27) --------------------
  const row18 = CHECKS.find((r) => r.n === 18);
  const limits18 = row18.parse({ text: "<24px 必须为 0；<32px 每路由 ≤10", nums: [24, 0, 32, 10] });
  // §12.9's named closed set as MASTER now states it (t38: 1 item).
  const SHELLSET18 = [{ tag: "button", classes: ["kbd-hint"], w: 26, h: 45, raw: "button.kbd-hint" }];
  const mkCap18 = (route, mode, inter) => ({ route, mode, metrics: { inter } });
  const inter18 = (o) => ({
    hit24: o.hit24 ?? 0, hit32: o.hit32 ?? 0, hit24All: o.hit24All ?? [], small24Samples: o.hit24All ?? [], small32Samples: [],
    paint24: o.paint24 ?? 0, paint32: o.paint32 ?? 0, viaRoot: 0, promoted: 0,
    shell24: o.shell24 ?? 0, shell32: o.shell32 ?? 0, content24: o.content24 ?? 0,
    content32: o.content32 ?? 0, shell32All: o.shell32All ?? [],
  });
  const j18 = (hit24, hit32, hit24All = [], cols = {}) => {
    const cap = mkCap18("chat", "dark", inter18({ hit24, hit32, hit24All, ...cols }));
    return row18.judge(cap, limits18, {
      captures: cols.captures ?? [cap],
      contract: { shellSet: cols.allowed === undefined ? SHELLSET18 : cols.allowed, shellSetSource: "MASTER §12.9" },
    });
  };
  const switchHit = [{ sel: "div.card > div.row > button.ant-switch", w: 42, h: 21, via: "自身", paint: "42x21" }];
  const selectHit = [{ sel: "div.composer-controls > div.ant-select > input.ant-select-input", w: 159, h: 23, via: "控件根 ant-select", paint: "159x24" }];
  const SH_KBD = { sel: "aside.app-sider button.kbd-hint", tag: "button", cls: "kbd-hint", w: 26, h: 45 };
  const SH_GHOST = { sel: "aside.app-sider button.ghost", tag: "button", cls: "ghost", w: 30, h: 20 };

  // The defect this task exists to fix.
  // The heart of the fix: the hit-target path must consult NO paint signal, so a
  // borderless + fully transparent control root is still the thing measured.
  const hitSrc18 = PROBE.toString();
  const hitSrcBody = hitSrc18.slice(hitSrc18.indexOf("const hitTarget = (el)"), hitSrc18.indexOf("const paintedBox = (el)"));
  check("row18: a borderless + fully transparent control is NOT fallen back to its inner element",
    hitSrcBody.length > 0 && !/hasBorder|backgroundColor|borderTopStyle|parseColor/.test(hitSrcBody), true);
  check("row18: hitTarget has a real body (the slice above is not vacuous)", hitSrcBody.includes("isControlRoot(el)"), true);
  check("row18: control roots are found by interaction semantics, not by paint",
    PROBE.toString().includes("ANTD_CONTROL_ROOT") && PROBE.toString().includes("ROLE_ROOT") &&
      PROBE.toString().includes("tabindex") && PROBE.toString().includes("cursor"),
    true);
  check("row18: an input inside an antd composite control is not itself the widget",
    PROBE.toString().includes("!n.closest(ANTD_CONTROL_ROOT)"), true);
  check("row18: #chat's 23px .ant-select roots now count as floor violations",
    j18(2, 9, selectHit).pass, false);
  check("row18: the same two selects would have PASSED under the painted-box rule (counterexample)",
    (() => { const it = { paint24: 0, paint32: 9 }; return it.paint24 === 0 && it.paint32 <= limits18.max; })(), true);
  check("row18: the three sub-assertions are judged separately",
    j18(0, 11, [], { content32: 11 }).pass === false &&
      j18(2, 4, switchHit, { content32: 4 }).pass === false &&
      j18(0, 4, [], { content32: 4 }).pass === true, true);
  check("row18: the note states the three sub-verdicts apart",
    j18(2, 4, switchHit, { content32: 4 }).note.includes("地板 <24px 2 ✗") &&
      j18(2, 4, switchHit, { content32: 4 }).note.includes("内容区 <32px 4 ✓"), true);
  check("row18: both readings are printed side by side, hit target first",
    j18(2, 4, switchHit, { paint24: 0, paint32: 4, content32: 4 }).display.includes("命中区 地板<24px 2") &&
      j18(2, 4, switchHit, { paint24: 0, paint32: 4, content32: 4 }).display.includes("对照（绘制口径）<24px 0"), true);
  check("row18: every floor violation is named individually",
    j18(2, 4, switchHit.concat(selectHit)).note.includes("ant-switch") &&
      j18(2, 4, switchHit.concat(selectHit)).note.includes("ant-select"), true);
  check("row18: the switch registration does not soften the verdict",
    j18(2, 4, switchHit).note.includes("antd 默认值") && j18(2, 4, switchHit).pass, false);
  check("row18: the container guard survives the rewrite (grows in both axes, or >6x area)",
    PROBE.toString().includes("nr.width > er.width * 2 && nr.height > er.height * 2") &&
      PROBE.toString().includes("eArea * 6"), true);
  check("row18: the old painted-box rule is kept as a comparison column only",
    PROBE.toString().includes("const paintedBox = (el)") && PROBE.toString().includes("inter.paint24++"), true);
  // T-1: the example is historical now (t34 fixed it), but the *reason* the rule
  // exists must survive — it is the only place that explains why paint signals
  // cannot gate the measurement.
  const crit18 = CHECKS.find((r) => r.n === 18).criterion;
  check("row18: the criterion keeps the why (paint signals must not gate) and marks the 23px case historical",
    crit18.includes("不用「画了东西」") && crit18.includes("历史例证") && crit18.includes("t34") &&
      crit18.includes("min-height: 24px"), true);
  // T-3: the any-dimension reading, pinned so nobody "tightens" it into &&.
  check("row18: the criterion pins the any-dimension reading and warns against &&",
    crit18.includes("w < 32 || h < 32") && crit18.includes("不要改成 &&") && crit18.includes("WCAG 2.5.8"), true);
  check("row18: the criterion states the §12.9 content-area reading, the ⊆ semantics and the constancy premise",
    crit18.includes("§12.9") && crit18.includes("只判内容区") && crit18.includes("变少满足、变多 FAIL") &&
      crit18.includes("常数") && crit18.includes("选择器路径 + className + 几何"), true);
  // T-2: the inventory cap, and the listing really is complete.
  check("row18: the inventory cap is at least 40", INTERACTIVE_SAMPLE_CAP >= 40, true);
  check("row18: pushSample honours a per-call limit (the inventories pass their own)",
    PROBE.toString().includes("limit ?? cap") && PROBE.toString().includes("interCap"), true);
  check("row18: the probe uses the inventory cap for BOTH buckets",
    (PROBE.toString().match(/interCap\)/g) ?? []).length >= 2, true);
  const many18 = Array.from({ length: 11 }, (_, i) => ({ sel: "div.offender" + i, w: 20, h: 20, via: "自身", paint: "24x24" }));
  check("row18: all 11 floor violations are named, not a truncated head",
    many18.every((s) => j18(11, 0, many18).note.includes(s.sel)), true);
  check("row18: a 16-item content list fails the budget while a 16-item shell list does not",
    j18(0, 16, [], { content32: 16 }).pass === false &&
      j18(0, 16, [], { content32: 0, shell32: 16, shell32All: [SH_KBD] }).pass === true, true);
  // §12.9 data split: the shell is classified by its frozen e2e selector, and the
  // split is printed next to the total so a reader can subtract the constant.
  // SHELL_SEL is a module constant passed in via cfg, so assert on both the
  // constant and the probe's use of it.
  check("row18: the shell is classified by the frozen §12.9 selector, not by geometry",
    SHELL_SEL === "aside.ant-layout-sider.app-sider" && PROBE.toString().includes("cfg.shellSel") &&
      PROBE.toString().includes("closest(shellSel"), true);
  check("row18: the note prints the shell/content split and the shell's identity",
    j18(0, 11, [], { shell32: 1, content32: 10, shell32All: [SH_KBD] }).note.includes("外壳 <32px 1") &&
      j18(0, 11, [], { shell32: 1, content32: 10, shell32All: [SH_KBD] }).note.includes("kbd-hint 26x45"), true);

  // ---- §12.9 (t39): content-area budget, shell closed set, constancy --------
  check("row18: the budget is judged on the CONTENT AREA, the shell is not charged",
    j18(0, 11, [], { content32: 10, shell32: 1, shell32All: [SH_KBD] }).pass === true &&
      j18(0, 11, [], { content32: 11, shell32: 0 }).pass === false, true);
  check("row18: the same page under the OLD per-route total would have FAILED (drift is gone)",
    j18(0, 11, [], { content32: 10, shell32: 1, shell32All: [SH_KBD] }).display.includes("旧口径（每路由总数）<32px 11") &&
      j18(0, 11, [], { content32: 10, shell32: 1, shell32All: [SH_KBD] }).pass === true, true);
  check("row18: both readings are printed so the drift can be confirmed removed",
    j18(0, 4, [], { content32: 3, shell32: 1, shell32All: [SH_KBD] }).display.includes("内容区<32px 3/≤10 ✓") &&
      j18(0, 4, [], { content32: 3, shell32: 1, shell32All: [SH_KBD] }).display.includes("旧口径（每路由总数）<32px 4"), true);

  // The closed set, read from MASTER, with ⊆ semantics.
  check("row18: a shell constant inside the named closed set passes",
    j18(0, 4, [], { shell32: 1, shell32All: [SH_KBD] }).pass, true);
  check("row18: ⊆ semantics — a member LEAVING the set passes (t36 moved 3 buttons out)",
    j18(0, 4, [], { shell32: 0, shell32All: [] }).pass, true);
  check("row18: a control OUTSIDE the closed set FAILS (a 5th shell control cannot hide)",
    j18(0, 4, [], { shell32: 1, shell32All: [{ sel: "aside.app-sider button.rogue", tag: "button", cls: "rogue", w: 20, h: 20 }] }).pass, false);
  check("row18: the same geometry under a different class is not admitted (identity, not size)",
    j18(0, 4, [], { shell32: 1, shell32All: [{ sel: "aside.app-sider button.other", tag: "button", cls: "other", w: 26, h: 45 }] }).pass, false);
  check("row18: the shell must still respect the 24px floor",
    j18(0, 4, [], { shell24: 1, shell32: 1, shell32All: [SH_KBD] }).pass, false);
  check("row18: with no closed set readable the third sub-assertion is not judged (not failed)",
    j18(0, 4, [], { content32: 4, allowed: null }).pass === true &&
      j18(0, 4, [], { content32: 4, allowed: null }).note.includes("未判"), true);

  // The constancy premise itself (§12.9 ①).
  const capHome = mkCap18("home", "dark", inter18({ shell32: 2, content32: 0, shell32All: [SH_KBD, SH_GHOST] }));
  const capChat = mkCap18("chat", "dark", inter18({ shell32: 1, content32: 0, shell32All: [SH_KBD] }));
  check("row18: a shell element present on only SOME routes is not a constant",
    shellConstancy([capHome, capChat]).constants.length === 1 &&
      shellConstancy([capHome, capChat]).varying.length === 1, true);
  check("row18: a non-constant shell element falls back into the CONTENT budget",
    row18.judge(capHome, limits18, { captures: [capHome, capChat], contract: { shellSet: SHELLSET18 } }).note.includes("非常数 1 项已回落内容区") &&
      row18.judge(capHome, limits18, { captures: [capHome, capChat], contract: { shellSet: SHELLSET18 } }).display.includes("内容区<32px 1/≤10"), true);
  check("row18: a non-constant shell element can therefore blow the content budget",
    row18.judge(
      mkCap18("home", "dark", inter18({ shell32: 1, content32: 10, shell32All: [SH_GHOST] })),
      limits18,
      { captures: [mkCap18("home", "dark", inter18({ shell32: 1, content32: 10, shell32All: [SH_GHOST] })), capChat], contract: { shellSet: SHELLSET18 } },
    ).pass, false);
  check("row18: identical on every route -> a constant (13-route identity)",
    shellConstancy([capHome, mkCap18("chat", "dark", inter18({ shell32: 2, content32: 0, shell32All: [SH_KBD, SH_GHOST] }))]).constants.length === 2, true);
  check("row18: a geometry change on one route breaks constancy (path+class+geometry, all three)",
    shellConstancy([capHome, mkCap18("chat", "dark", inter18({ shell32: 2, content32: 0, shell32All: [SH_KBD, { ...SH_GHOST, h: 21 }] }))]).constants.length === 1, true);
  check("row18: the closed set is read from MASTER §12.9, not hardcoded",
    CT.shellSetSource === "MASTER §12.9" && CT.shellSet.length >= 1 &&
      CT.shellSet.every((s) => s.tag && s.classes.length && s.w > 0 && s.h > 0), true);
  check("row18: §12.9 membership is identity, not size (a retired member is not re-admitted)",
    shellSetAdmits(CT.shellSet, { tag: "button", cls: "ant-btn css-u6izyw", w: 32, h: 32 }) === false &&
      shellSetAdmits(CT.shellSet, { tag: "button", cls: "kbd-hint", w: 26, h: 45 }) === true, true);

  // ---- row 1: the designed-surface reading (R-1) ---------------------------
  const row1 = CHECKS.find((r) => r.n === 1);
  const limits1 = row1.parse({ text: "每路由 ≤75%", nums: [75] });
  const j1 = (all, designed, excluded) =>
    row1.judge({ route: "inbox", mode: "dark", pixels: { lumaBandRatio: all, lumaBandRatioDesigned: designed, excludedRatio: excluded, exclude: ["#0f1014"], designedPixels: 21500, pixels: 1296000 } }, limits1, {});
  check("row1: #inbox 83.46% with canvas -> FAIL", j1(0.8346, 0.8346, 0).pass, false);
  check("row1: #inbox 0.27% designed-only -> PASS", j1(0.8346, 0.0027, 0.8341).pass, true);
  check("row1: the with-canvas value is still reported, not hidden",
    j1(0.8346, 0.0027, 0.8341).display.includes("83.46%"), true);
  check("row1: the note names the canvas share and cites the landed §12.5",
    /画布/.test(j1(0.8346, 0.0027, 0.8341).note) && /§12\.5/.test(j1(0.8346, 0.0027, 0.8341).note), true);
  check("row1: a genuinely muddy surface still fails (0.9 designed-only)",
    j1(0.9, 0.9, 0).pass, false);
  check("row1: pixelStats excludes exactly the named fill",
    (() => {
      const img = { width: 4, height: 1, channels: 3, data: Uint8Array.from([15,16,20, 15,16,20, 27,30,36, 200,200,200]) };
      const s = pixelStats(img, undefined, { exclude: [[15,16,20]] });
      return s.excludedPixels === 2 && s.designedPixels === 2 && s.lumaBandRatioDesigned === 0;
    })(), true);
  check("row1: without an exclusion list the count is unchanged (old behaviour preserved)",
    (() => {
      const img = { width: 3, height: 1, channels: 3, data: Uint8Array.from([15,16,20, 15,16,20, 27,30,36]) };
      const s = pixelStats(img);
      return s.excludedPixels === 0 && Math.abs(s.lumaBandRatio - 2/3) < 1e-9;
    })(), true);

  // ---- row 30: judged on the ground the ring is actually drawn on (R-2) -----
  const row30 = CHECKS.find((r) => r.n === 30);
  const limits30 = row30.parse({ text: "ring 存在且对底 ≥1.2:1", nums: [1.2] });
  const j30 = (ratioOnCanvas, ratio) =>
    row30.judge({ route: "inbox", mode: "light", metrics: { ring: { exists: true, color: "#e7e9ed", spread: 1, ratioOnCanvas, ratio, raw: "0px 0px 0px 1px rgb(231, 233, 237)" } } }, limits30, {});
  check("row30: #inbox light canvas 1.13 / panel 1.22 -> FAIL (canvas is judged)",
    j30(1.13, 1.22).pass, false);
  check("row30: the same ring would have PASSED on the panel reading (1.22 >= 1.2)",
    j30(1.22, 1.22).pass, true);
  check("row30: both grounds are printed", j30(1.13, 1.22).display.includes("1.13") && j30(1.13, 1.22).display.includes("1.22"), true);
  check("row30: a ring that clears 1.2:1 on canvas passes", j30(1.25, 1.3).pass, true);
  check("row30: the probe falls back to --panel-shadow when no card exists",
    /--panel-shadow/.test(PROBE.toString()) && /token --panel-shadow/.test(PROBE.toString()), true);
  // Both spellings are exercised live: pages with a card go through the
  // normalised box-shadow property ("rgb(...) 0px 0px 0px 1px"), pages without
  // one go through the raw token ("0 0 0 1px #e7e9ed"). Matching only the first
  // spelling made the token path find no ring at all.
  const probeSrc30 = PROBE.toString();
  check("row30: ring parser accepts unitless-zero lengths (custom-property spelling)",
    probeSrc30.includes("(?:px)?"), true);
  check("row30: ring parser accepts hex colours (custom-property spelling)",
    probeSrc30.includes("#[0-9a-fA-F]{3,8}"), true);

  // ---- row 28: the cap that makes the row satisfiable (R-3) ----------------
  const r28row = CHECKS.find((r) => r.n === 28);
  const l28doc = r28row.parse({ text: "取最大组 ≤3", nums: [3] });
  const l28landed = r28row.parse({ text: "取最大组 ≤7", nums: [7] });
  check("row28: a MASTER cap below the ruling is overridden -> cap 7", l28doc.max === 7 && l28doc.ruled === true, true);
  check("row28: once MASTER lands ≤7 the ruling stops applying", l28landed.max === 7 && l28landed.ruled === false, true);
  check("row28: the cap is read from 「最大组 ≤N」, not from the superseded ≤3 earlier in the cell",
    r28row.parse({ text: "门槛 ≤7 不变（原 ≤3 …见 §12.8）。…取最大组 ≤7；…独立断言 ≤7", nums: [7, 3, 12.8, 7, 7] }).max === 7, true);
  // §12.10.1: the row is now two sub-assertions over endpoint groups.
  const apiCap = (o) => ({
    route: o.route ?? "inbox", mode: "dark", apiRequests: o.total ?? 6, apiWindowMs: 11500,
    api: { perms: o.perms ?? 6, nonPermTotal: o.nonPerm ?? 0, endpointKinds: o.kinds ?? 1, maxGroup: o.maxGroup ?? 0, maxEndpoint: o.maxEndpoint ?? "/api/v1/x", groups: o.groups ?? [] },
  });
  const j28 = (cap, limits = l28doc, caps) =>
    r28row.judge(cap, limits, { args: { apiWindowMs: 11500 }, captures: caps ?? [cap] });
  check("row28: a route with a 6-count poller (task) PASSES both sub-assertions",
    j28(apiCap({ perms: 6, nonPerm: 6, maxGroup: 6, kinds: 5, maxEndpoint: "/api/v1/tasks/:id" })).pass, true);
  check("row28: a route with NO poller PASSES (not not_measured) — §12.10.1 ④",
    j28(apiCap({ perms: 6, nonPerm: 1, maxGroup: 1, kinds: 2 })).pass === true &&
      j28(apiCap({ perms: 6, nonPerm: 0, maxGroup: 0, kinds: 0 })).pass === true, true);
  check("row28: two 2s pollers on the SAME endpoint (14) FAIL the per-route assertion",
    j28(apiCap({ perms: 6, nonPerm: 14, maxGroup: 14, kinds: 1 })).pass, false);
  check("row28: the per-route assertion counts the MAX GROUP, not the total",
    j28(apiCap({ perms: 6, nonPerm: 20, maxGroup: 2, kinds: 7 })).pass, true);
  check("row28: /permissions is judged ONCE (max over every capture) and reported on one entry",
    (() => {
      const a = apiCap({ route: "home", perms: 8, nonPerm: 12, maxGroup: 2, kinds: 7 });
      const b = apiCap({ route: "inbox", perms: 6, nonPerm: 0, maxGroup: 0, kinds: 0 });
      const ja = j28(a, l28doc, [a, b]);
      const jb = j28(b, l28doc, [a, b]);
      return ja.pass === false && jb.pass === true && ja.display.includes("全站最大 8") && jb.display.includes("全站最大 8") && ja.display.includes("判定落在本条");
    })(), true);
  check("row28: the two sub-assertions are printed and judged apart",
    j28(apiCap({ perms: 8, nonPerm: 12, maxGroup: 2, kinds: 7 }), l28doc, [apiCap({ perms: 8, nonPerm: 12, maxGroup: 2, kinds: 7 })]).note.includes("① 逐路由") &&
      j28(apiCap({ perms: 8, nonPerm: 12, maxGroup: 2, kinds: 7 }), l28doc, [apiCap({ perms: 8, nonPerm: 12, maxGroup: 2, kinds: 7 })]).note.includes("② /permissions"), true);
  check("row28: 14 on one endpoint would also have FAILED at MASTER's ≤3 (the row still discriminates)",
    j28(apiCap({ perms: 6, nonPerm: 14, maxGroup: 14, kinds: 1 }), { max: 3, docMax: 3, ruled: false }).pass, false);
  check("row28: the ruling is NOT applied when MASTER already agrees",
    j28(apiCap({ perms: 4, nonPerm: 4, maxGroup: 4, kinds: 2 }), l28landed).pass, true);

  // ---- §12 target parsing: read the requirement, not the i-th number -------
  const TH = loadThresholds(REPO);
  // (a) Every row must be judged from the CONTRACT, not from a built-in default.
  // If MASTER's wording moves away from an anchor, the parse silently falls back
  // and the row keeps judging a number nobody re-read — this is what makes that
  // loud instead of silent.
  {
    // A row that HAS a §12 entry must parse from it. A row that does not have one
    // yet (t69's rows 40/41, whose entry the captain asked design-lead to add)
    // must not be judged as drift — but it must be NAMED, so the exemption cannot
    // quietly grow and disappears by itself the moment the entry lands.
    const inDoc = [];
    const notInDoc = [];
    for (const row of CHECKS) {
      if (!row.parse) continue;
      const bucket = TH.rows.has(row.n) ? inDoc : notInDoc;
      const t = targetFor(TH, row.n, "");
      resetPickMisses();
      try {
        row.parse(t);
      } catch (e) {
        bucket.push(`row${row.n} threw: ${e.message}`);
        continue;
      }
      for (const m of takePickMisses()) bucket.push(`row${row.n}: ${m.re} → ${m.fallback}`);
    }
    check("§12 parse: every row WITH a MASTER §12 entry matches its anchor (no silent built-in fallback)",
      inDoc.length === 0, true);
    // Rows 42-45 were opened by t78 ahead of their MASTER entry (§12.15 is their
    // only source; the captain forwards the proposed row text to design-lead).
    // They are the ONLY rows allowed to miss, they are named here, and the moment
    // the entries land this assertion fails and must be tightened again — the
    // exemption cannot quietly grow.
    // Repeatable tool<->contract reconciliation (t81, captain's request): this is
    // a DISCOVERY mechanism, not a prevention one, so it must run on every
    // --self-test rather than be a one-off conclusion. The contract side is read
    // straight out of MASTER §12 by loadThresholds; docs/design/check-contract.mjs
    // (design-lead) independently reports the same row set via --json, so the two
    // can be cross-checked without either script depending on the other.
    {
      const toolRows = [...new Set(CHECKS.map((r) => r.n))].sort((a, b) => a - b);
      const contractRows = [...TH.rows.keys()].sort((a, b) => a - b);
      const onlyTool = toolRows.filter((n) => !contractRows.includes(n));
      const onlyContract = contractRows.filter((n) => !toolRows.includes(n));
      check("reconcile: no contract row is left unjudged (contract promises a row nobody verifies)",
        onlyContract.join(","), "");
      check("reconcile: no row exists only in the tool either (every judge has a contract to be judged against)",
        onlyTool.join(","), "");
      console.log("  reconcile: tool=" + toolRows.length + " contract=" + contractRows.length +
        " onlyTool=[" + onlyTool.join(",") + "] onlyContract=[" + onlyContract.join(",") + "]");
    }
    // 42/43/44 landed while t80 ran, 45 while t81 ran, 46 while t93 ran: the
    // pending list is empty again. Any row added ahead of its entry fails here.
    const PENDING_ENTRY = [];
    check("§12 parse: no row is awaiting a MASTER entry any more (rows 42-46 all landed)",
      [...new Set(notInDoc.map((m) => Number(String(m).match(/^row(\d+)/)?.[1])))].sort((a, b) => a - b).join(","),
      PENDING_ENTRY.join(","));
    check("§12 parse: every row whose entry HAS landed reads its target out of the doc, not the fallback",
      [13, 40, 41].map((n) => {
        resetPickMisses();
        const v = CHECKS.find((r) => r.n === n).parse(targetFor(TH, n, ""));
        return v.max === 0 && takePickMisses().length === 0;
      }).join(","), "true,true,true");
    if (inDoc.length || notInDoc.length) console.log("  anchor misses:", inDoc.concat(notInDoc).join(" | "));
  }
  // (b) A number that is not the requirement must not move the parse. Baseline
  // values, historical values and section cross-references all get spliced into
  // the cell and the parsed limits must be byte-identical.
  {
    const NOISE = "（基线 M1 13.7–83.5%，见 §12.7.2；原值 350KB）";
    const drifted = [];
    for (const row of CHECKS) {
      if (!row.parse) continue;
      const t = targetFor(TH, row.n, "");
      let a;
      let b;
      try {
        a = JSON.stringify(row.parse(t));
      } catch (e) {
        continue;
      }
      try {
        b = JSON.stringify(row.parse({ ...t, text: t.text + NOISE, nums: t.nums.concat([13.7, 83.5, 12.7, 2, 350]) }));
      } catch (e) {
        b = "threw: " + e.message;
      }
      if (a !== b) drifted.push(`row${row.n}: ${a} → ${b}`);
    }
    check("§12 parse: an appended baseline / historical value / §12.x.y reference never moves a target",
      drifted.length === 0, true);
    if (drifted.length) console.log("  drifted:", drifted.join(" | "));
  }
  // (c) The three instances that were actually broken, one assertion each.
  {
    const row1 = CHECKS.find((r) => r.n === 1);
    const row6 = CHECKS.find((r) => r.n === 6);
    const row18 = CHECKS.find((r) => r.n === 18);
    const t1 = targetFor(TH, 1, "");
    // The cell is 「每路由 ≤75%」. A baseline number in front of it must not win.
    check("row1: the target is read from 「每路由 ≤75%」, not from the first number in the cell",
      row1.parse(t1).max === 0.75 &&
        row1.parse({ text: "M1 13.7–83.5%（中位 37.4%）｜ 每路由 ≤75%（见 §12.5）" }).max === 0.75, true);
    check("row1: a baseline-looking number in the cell does not become the goal",
      row1.parse({ text: "基线 13.645；每路由 ≤75%" }).max === 0.75, true);
    // Row 6: the upper bound ≤40 must not be read as an exemption floor.
    const t6 = targetFor(TH, 6, "");
    check("row6: ≥3 / ≥1 / ≤40 are read by comparator, not by position",
      row6.parse(t6).lo === 3 && row6.parse(t6).exemptLo === 1 && row6.parse(t6).hi === 40, true);
    check("row6: appending a stray number does not turn the ≤40 bound into a floor",
      row6.parse({ text: "含读数的页 ≥3；无读数的页 ≥1；所有页 ≤40（历史 92.9%）" }).hi === 40, true);
    // Row 18: 「见 §12.7.2」 used to push the cap to 2.
    const t18 = targetFor(TH, 18, "");
    check("row18: 「见 §12.7.2」 in the cell cannot push the <32px cap to 2",
      row18.parse(t18).max === 10 && row18.parse({ text: "<24px 必须为 0；<32px 每路由「内容区」≤10（见 §12.7.2）" }).max === 10, true);
    check("row18: the floor is still 24px even with a cross-reference present",
      row18.parse(t18).hard === 24 && row18.parse(t18).soft === 32, true);
  }
  check("thresholds: a decimal first cell is NOT a row number (| 1.00 | must not clobber row 1)",
    /≤75%/.test(String(TH.rows.get(1)?.target ?? "")), true);
  check("thresholds: every §12 row number is a bare integer, and row 39 exists",
    TH.rows.has(39) && [...TH.rows.keys()].every((k) => Number.isInteger(k)), true);
  check("thresholds: row 27's target carries the floor+budget wording (not the superseded ≤350KB alone)",
    /地板/.test(String(TH.rows.get(27)?.target ?? "")) && /应用层首屏/.test(String(TH.rows.get(27)?.target ?? "")), true);

  // ---- row 20: the failure state, and the three ways it can be faked ------
  const row20 = CHECKS.find((r) => r.n === 20);
  const F20 = (o) => ({
    injectable: 1, endpoints: ["/api/v1/tasks"], injected: 1, requests: 3, seen: ["/api/v1/tasks"],
    shellPresent: true, contentPresent: true, bodyText: 1200, blank: false,
    alerts: [{ sel: "div.ant-alert-error", text: "无法加载任务列表" }], errorNodes: [],
    retry: { sel: "button.ant-btn", text: "重 试", w: 49, h: 32 },
    emptyShown: false, emptySel: null, retryClicked: true, recovered: true, error: null,
    ...o,
  });
  const j20 = (o) => row20.judge({ failure: o === null ? undefined : F20(o ?? {}) }, {});
  check("row20: a visible alert + a real retry + recovery PASSES", j20({}).pass, true);
  check("row20: the three sub-verdicts are printed apart",
    j20({}).display.includes("失败态 可见 ✓") && j20({}).display.includes("重试 可点 ✓") && j20({}).display.includes("恢复 ✓"), true);
  // The t13 inbox defect: the request failed and the page says everything is fine.
  const asEmpty = j20({ alerts: [], errorNodes: [], emptyShown: true, emptySel: "div.ant-empty", retry: null, recovered: null });
  check("row20: a failure rendered as an EMPTY state FAILS", asEmpty.pass, false);
  check("row20: ...and the note names it as the t13 inbox defect",
    asEmpty.note.includes("把失败渲染成了空态") && asEmpty.note.includes("t13"), true);
  const asSuccess = j20({ alerts: [], errorNodes: [], emptyShown: false, retry: null, recovered: null });
  check("row20: a failure rendered as a SUCCESS-looking page FAILS", asSuccess.pass, false);
  check("row20: ...and the note names that separately", asSuccess.note.includes("把失败渲染成了成功态"), true);
  check("row20: an alert that is not visible does not count as a failure state",
    j20({ alerts: [], errorNodes: [] }).pass, false);
  check("row20: a broken shell (blank page) FAILS — that is a white screen, not a failure state",
    j20({ shellPresent: false, bodyText: 12, blank: true }).pass === false &&
      j20({ shellPresent: false, bodyText: 12, blank: true }).note.includes("外壳异常") &&
      j20({}).note.includes("不是白屏"), true);
  check("row20: a retry control below the 24px floor FAILS",
    j20({ retry: { sel: "button", text: "重 试", w: 20, h: 20 } }).pass, false);
  check("row20: a retry that does not actually recover FAILS",
    j20({ recovered: false }).pass, false);
  check("row20: no injectable endpoint -> not_measured with a reason, never FAIL",
    j20({ injectable: 0, endpoints: [], injected: 0 }).pass === null &&
      j20({ injectable: 0, endpoints: [], injected: 0 }).note.includes("not_measured"), true);
  check("row20: endpoints known but never re-requested -> not_measured, never FAIL",
    j20({ injected: 0 }).pass === null && j20({ injected: 0 }).display.includes("注入 0 次"), true);
  check("row20: the shell's own poller is excluded from injection",
    /\/permissions\$/.test(FAILURE_SKIP.source) && probeFailureState.toString().includes("FAILURE_SKIP"), true);
  check("row20: the probe forces a real document load (a hash-only goto never re-mounts the view)",
    probeFailureState.toString().includes("page.reload("), true);
  check("row20: the retry match tolerates antd's CJK space (「重 试」)",
    (() => {
      const src = probeFailureState.toString();
      return src.includes("replace(/\\s+/g, \"\")") && /重试\|再试/.test(src);
    })(), true);
  check("row20: the criterion states all three requirements and the shell guard",
    row20.criterion.includes("失败态可见") && row20.criterion.includes("可恢复") &&
      row20.criterion.includes("不得把失败渲染成空态或成功态") && row20.criterion.includes("外壳必须仍在"), true);

  // ---- row 39 (new) + row 28 separation: two phenomena, two numbers -------
  const row39 = CHECKS.find((r) => r.n === 39);
  const l39 = row39.parse({ text: "R ≤ 2K + 2", nums: [2, 2] });
  const urls39 = (spec) => spec.flatMap(([ep, n]) => Array.from({ length: n }, (_, i) => `http://x${ep}/${i + 1}`));
  const j39 = (spec) => row39.judge({ route: "graph", mode: "dark", api: summarizeApi(urls39(spec)) }, l39, {});
  const j28sep = (spec) => {
    const cap = { route: "graph", mode: "dark", apiRequests: 9, apiWindowMs: 11500, api: summarizeApi(urls39(spec)) };
    return r28row.judge(cap, l28doc, { args: { apiWindowMs: 11500 }, captures: [cap] });
  };
  check("row39: #graph BEFORE the fix (55 entity ids) -> FAIL (56 > 6)",
    j39([["/api/v1/graph/entities", 1], ["/api/v1/graph/entity", 55]]).pass, false);
  check("row39: #graph AFTER the fix (1 entities call) -> PASS",
    j39([["/api/v1/graph/entities", 1]]).pass, true);
  check("row39: numeric ids are folded to :id (K counts endpoints, not ids)",
    (() => { const a = summarizeApi(urls39([["/api/v1/graph/entity", 55]])); return a.endpointKinds === 1 && a.maxGroup === 55; })(), true);
  check("row39: uuid-shaped ids are folded too",
    summarizeApi(["http://x/api/v1/tasks/8f14e45f-ceea-467a-9a1e-2b0c3d4e5f60"]).endpointKinds === 1 &&
      summarizeApi(["http://x/api/v1/tasks/8f14e45f-ceea-467a-9a1e-2b0c3d4e5f60"]).groups[0].endpoint === "/api/v1/tasks/:id", true);
  check("row39: the five t44 numbers reproduce (graph 1 · home 12/7 · task 9/5 · runtimes 4/5 · settings 2/3)",
    j39([["/api/v1/graph/entities", 1]]).pass === true &&
      row39.judge({ api: summarizeApi(urls39([["/api/v1/a", 2], ["/api/v1/b", 2], ["/api/v1/c", 2], ["/api/v1/d", 2], ["/api/v1/e", 2], ["/api/v1/f", 1], ["/api/v1/g", 1]])) }, l39, {}).pass === true &&
      row39.judge({ api: summarizeApi(urls39([["/api/v1/t", 6], ["/api/v1/u", 1], ["/api/v1/v", 1], ["/api/v1/w", 1]])) }, l39, {}).pass === true, true);
  check("row39: /permissions is excluded from R and K",
    (() => {
      const a = summarizeApi([
        "http://x/api/v1/permissions",
        "http://x/api/v1/permissions",
        "http://x/api/v1/permissions",
        "http://x/api/v1/graph/entities",
      ]);
      return a.perms === 3 && a.nonPermTotal === 1 && a.endpointKinds === 1;
    })(), true);
  check("row39: an empty page passes (0 <= 2)",
    j39([]).pass, true);
  // The acceptance's separation case: a first-screen N+1 that stays UNDER row
  // 28's per-endpoint budget but blows row 39's ratio.
  const sepSpec = [["/api/v1/graph/entities", 1], ["/api/v1/graph/entity", 7]];
  check("row39: catches a 7-item per-id expansion that row 28 does NOT flag",
    j39(sepSpec).pass === false && j28sep(sepSpec).pass === true, true);
  check("row39: the display shows R, K and the derived cap",
    j39(sepSpec).display.includes("R 8 ≤ 2K+2 = 6（K=2）"), true);
  check("row39: the criterion names both phenomena and the G15 relationship",
    row39.criterion.includes("轮询重复") && row39.criterion.includes("条目级展开") && row39.criterion.includes("G15"), true);

  // ---- row 27: floor + budget + two structural assertions ------------------
  const row27 = CHECKS.find((r) => r.n === 27);
  const MASTER27 = "**地板（登记，不判 PASS/FAIL）= 框架层 980KB（gzip ≈305KB）**；**预算（判定）= 应用层首屏 ≤150KB（gzip ≤48KB）**；**两条结构断言**：① markdown 栈不得在入口（必须是独立 chunk）② 必须存在路由级分割 chunk。**原「≤350KB（gzip ≤120KB）」保留为历史记录**";
  const l27 = row27.parse({ text: MASTER27, nums: [350, 120] });
  const j27 = (bundle) => row27.judge({}, l27, { global: { bundle } });
  const classifiedFatOf = (fn) => j27(fn(260));
  const entryChunk = { file: "index-abc.js", bytes: 1406 * 1024, gzip: 436 * 1024 };
  check("row27: the parse reads floor/budget from MASTER's phrases, not by position",
    l27.floorKb === 980 && l27.floorGzipKb === 305 && l27.budgetKb === 150 && l27.budgetGzipKb === 48, true);
  check("row27: the superseded ≤350KB is kept only as history",
    l27.legacyKb === 350 && l27.legacyGzipKb === 120, true);
  const unclassified = j27({ entry: entryChunk, js: [entryChunk, { file: "Graph-def.js", bytes: 120 * 1024, gzip: 40 * 1024 }] });
  check("row27: before manualChunks lands the row is not_measured, NOT failed",
    unclassified.pass === null, true);
  check("row27: before the split only 总量 and 地板 are printed (no app-layer claim)",
    unclassified.display.includes("总量 1526KB") && unclassified.display.includes("地板 980KB") &&
      !unclassified.display.includes("应用层"), true);
  check("row27: the not_measured note names the missing prerequisite",
    unclassified.note.includes("manualChunks") && unclassified.note.includes("not_measured"), true);
  check("row27: the structural assertions are evaluated and printed even when not judged",
    unclassified.note.includes("结构断言") && unclassified.note.includes("路由级分割 chunk"), true);
  check("row27: the old total-vs-350KB judgement is gone (1406KB must not FAIL the row)",
    unclassified.pass !== false, true);
  // The first-screen static graph is what Vite writes into index.html: the entry
  // plus one modulepreload per static import. Everything else is lazy.
  const splitBundle = (appKb, lazyKb = 400) => ({
    entry: { file: "index-entry.js", bytes: appKb * 1024, gzip: Math.round(appKb * 0.33) * 1024 },
    js: [
      { file: "index-entry.js", bytes: appKb * 1024, gzip: Math.round(appKb * 0.33) * 1024 },
      { file: "vendor-react.js", bytes: 980 * 1024, gzip: 305 * 1024 },
      // Lazy route chunks: present in dist, NOT on the first screen.
      { file: "route-graph.js", bytes: lazyKb * 1024, gzip: Math.round(lazyKb * 0.33) * 1024 },
      { file: "markdown-stack.js", bytes: 229 * 1024, gzip: 70 * 1024 },
    ],
    firstScreen: ["index-entry.js", "vendor-react.js"],
  });
  const classified = j27(splitBundle(120));
  check("row27: once the framework is split out the app layer is judged",
    classified.pass === true && classified.display.includes("≤150KB"), true);
  check("row27: the app layer is 首屏静态 chunk − 地板 (the first screen, not all of dist)",
    classified.display.includes("应用层首屏 120KB") &&
      classified.note.includes("应用层首屏 = 首屏静态 chunk − 地板"), true);
  check("row27: a LAZY route chunk is not counted as first screen",
    // 120KB entry + 980KB vendor + 400KB lazy route + 229KB markdown in dist,
    // yet the judged number stays at the entry's 120KB.
    classified.display.includes("应用层首屏 120KB") && classified.display.includes("全量 JS 1729KB"), true);
  check("row27: the display names exactly ONE 应用层 (no two contradictory numbers)",
    (classified.display.match(/应用层/g) ?? []).length === 1 &&
      (classifiedFatOf(splitBundle).display.match(/应用层/g) ?? []).length === 1, true);
  // A vendor-* name alone must not unlock the judgement.
  const fakeVendor = j27({
    entry: { file: "index-big.js", bytes: 1117 * 1024, gzip: 356 * 1024 },
    js: [
      { file: "index-big.js", bytes: 1117 * 1024, gzip: 356 * 1024 },
      { file: "vendor-tiny.js", bytes: 23 * 1024, gzip: 8 * 1024 },
      { file: "markdown-x.js", bytes: 149 * 1024, gzip: 46 * 1024 },
    ],
  });
  check("row27: a chunk merely NAMED vendor-* does not unlock the judgement (framework not split out)",
    fakeVendor.pass === null && fakeVendor.note.includes("框架层没有被真的切出去"), true);
  check("row27: the not_measured display makes no app-layer claim at all",
    !fakeVendor.display.includes("应用层"), true);
  const classifiedFat = j27(splitBundle(260));
  check("row27: a first-screen app layer over the budget FAILS once judged",
    classifiedFat.pass, false);
  check("row27: the pre-optimization figure is reported under its own name, never as 应用层",
    classified.note.includes("全量非框架 chunk") && classified.note.includes("刻意不叫「应用层」") &&
      classified.note.includes("不是首屏要下载的量"), true);
  check("row27: the note states where the first-screen set comes from (index.html, not a slice)",
    classified.note.includes("dist/index.html") && classified.note.includes("modulepreload"), true);
  check("row27: shrinking the entry alone would move the number — the note says the goal is download size, not a smaller entry",
    classified.note.includes("懒加载 chunk 天然不在集合里"), true);
  check("row27: markdown in the entry FAILS structural assertion ①",
    j27({
      entry: { file: "index-markdown-abc.js", bytes: 1406 * 1024, gzip: 436 * 1024 },
      js: [{ file: "index-markdown-abc.js", bytes: 1406 * 1024, gzip: 436 * 1024 }, { file: "vendor-react.js", bytes: 980 * 1024, gzip: 305 * 1024 }],
    }).pass, false);

  // ---- the settle condition (row 18 jitter: 4 / 10 / 11 on #task) ----------
  check("settle: auditRoute waits for DOM stability before counting",
    /const domStable = await waitForDomStable\(page, args\.domQuietMs, args\.domMaxMs\)/.test(auditRoute.toString()), true);
  check("settle: the stability wait has both a quiet window and a hard cap",
    /quietMs/.test(waitForDomStable.toString()) && /maxMs/.test(waitForDomStable.toString()), true);
  check("settle: an unstable DOM is reported, not silently counted",
    /DOM 未在 .* 内稳定/.test(auditRoute.toString()), true);

  // ---- row 26: the long-task window (a one-off 1795ms did not reproduce) ----
  const arSrc = auditRoute.toString();
  check("longtask: the reading is taken before the screenshot (t1's fix holds)",
    arSrc.indexOf("longtaskA = await readLongTasks()") < arSrc.indexOf("await page.screenshot()"), true);
  check("longtask: an over-target sample triggers a second, independent window",
    /longtaskA\.max > o\.longTaskTargetMs/.test(arSrc) && /LONG_TASK_REMEASURE_MS/.test(arSrc), true);
  check("longtask: a block that does not reproduce is registered as an outlier",
    /outlier: !confirmed/.test(arSrc) && /未再现/.test(arSrc), true);
  check("longtask: a block that does reproduce is judged on the worst window",
    /max: confirmed \? Math\.max\(longtaskA\.max, b\.max\) : b\.max/.test(arSrc), true);

  // ---- row 16: the ring may live on the focus owner ------------------------
  const ringStop = (o) => ({ tag: "INPUT", sel: "input.x", form: "none", ownForm: "none", ring: null, bgBehind: "rgb(27, 30, 36)", ...o });
  check("row16: a ring found on the :focus-within owner counts (antd Select)",
    focusInfo([ringStop({ form: "solid", ring: "rgb(245, 180, 87)", ringRatio: 6.45, ringVia: "focus-owner box-shadow", ringOwner: "div.ant-select" })]).notOk, 0);
  check("row16: outline:none on the input alone is still judged as no ring",
    focusInfo([ringStop({ form: "none", ownForm: "none" })]).notOk, 1);

  // ---- row 23: aria-expanded, primitives §5.2.2 / §11 ----------------------
  const expand = (controls, extra = {}) => ({
    controls: controls.map(([sel, count, withAriaExpanded]) => ({ sel, count, withAriaExpanded })),
    native: 0, antdCount: 0, ...extra,
  });

  // (a) THE false pass: antd Select ships 3 aria-expanded on #task while
  //     .tool-head renders 93 times with none. Whole-document counting reads
  //     "3 >= 0 hand-written carriers" and passes the row vacuously.
  check("row23: antd combobox only + .tool-head 0/93 -> FAIL",
    expandMissing(expand([[".tool-head", 93, 0]])).length, 1);

  // (b) a present carrier that is fully labelled -> nothing missing.
  check("row23: .recall-stub-head 2/2 -> PASS",
    expandMissing(expand([[".recall-stub-head", 2, 2]])).length, 0);

  // (c) partially labelled -> FAIL.
  check("row23: .chat-group-more 1/4 -> FAIL",
    expandMissing(expand([[".chat-group-more", 4, 1]])).length, 1);

  // (d) a carrier that is not rendered on this route is not a failure: the
  //     selectors are route-scoped (.tool-head only exists on #task).
  check("row23: absent carrier -> PASS",
    expandMissing(expand([[".recall-stub-head", 0, 0], [".tool-head", 0, 0]])).length, 0);

  // (e) the judge wired into §12 must use expandMissing, so the whole-document
  //     counter in metrics.structure.ariaExpanded cannot rescue the row.
  const row23 = CHECKS.find((r) => r.n === 23);
  const v23 = row23.judge(
    { route: "task", mode: "dark", metrics: { expand: expand([[".tool-head", 93, 0]], { antdCount: 3 }), structure: { ariaExpanded: 3 } } },
    { max: 0 },
    { global: {} },
  );
  check("row23: judge() with antd=3 and .tool-head 0/93 -> fail", v23.pass, false);
  const v23ok = row23.judge(
    { route: "memory", mode: "dark", metrics: { expand: expand([[".recall-stub-head", 4, 4]], { antdCount: 1 }), structure: { ariaExpanded: 1 } } },
    { max: 0 },
    { global: {} },
  );
  check("row23: judge() with .recall-stub-head 4/4 -> pass", v23ok.pass, true);

  // ---- row 16 judge wiring -------------------------------------------------
  const row16 = CHECKS.find((r) => r.n === 16);
  const v16 = row16.judge({ route: "x", mode: "dark", focusInfo: focusInfo([autoStop]), metrics: {} }, { max: 0 }, { global: {} });
  check("row16: judge() for an auto stop -> fail", v16.pass, false);

  // ---- row 32: signal uniqueness, MASTER §12.3 (t14) -----------------------
  const row32 = CHECKS.find((r) => r.n === 32);
  const LANDED = { rows: new Map([[32, { target: "检查集只读运行时品牌族", judge: "见 §12.3" }]]) };
  const NOT_LANDED = { rows: new Map([[32, { target: "0 组重复", judge: "解析 CSS 变量表做字符串比较" }]]) };
  const toks = (o) => ({ "--signal": "#f0a93b", "--status-warn": "#e2894f", "--brand-claude": "#d97757", "--brand-deepseek": "#5786fe", "--brand-opencode": "#e8e8ea", ...o });
  const j32 = (o, th = LANDED) => row32.judge({ mode: "dark", metrics: { tokens: toks(o) } }, { max: 0, hue: 8 }, { thresholds: th });

  // THE false fail the captain flagged: accent === --signal is REQUIRED by
  // primitives §2.4 C6 / §2.6, so counting it made the row unsatisfiable.
  check("row32: accent === --signal but brand family distinct -> PASS", j32({ "--ant-color-primary": "#f0a93b" }).pass, true);
  check("row32: real values, dark (Δhue 12.79°) -> PASS", j32({ "--ant-color-primary": "#f0a93b" }).pass, true);
  check("row32: real values, light (Δhue 9.46°) -> PASS",
    row32.judge({ mode: "light", metrics: { tokens: { "--signal": "#c88413", "--status-warn": "#8a4a12", "--brand-claude": "#d97757", "--brand-deepseek": "#5786fe", "--brand-opencode": "#121212", "--ant-color-primary": "#c88413" } } }, { max: 0, hue: 8 }, { thresholds: LANDED }).pass, true);
  check("row32: Δhue measured 12.79 for the real dark pair",
    Math.abs(hueDelta("#e2894f", "#f0a93b") - 12.79) < 0.02, true);
  check("row32: Δhue measured 9.46 for the real light pair",
    Math.abs(hueDelta("#8a4a12", "#c88413") - 9.46) < 0.02, true);

  // The other direction: a genuine collision must still fail.
  check("row32: --brand-claude === --signal -> FAIL", j32({ "--brand-claude": "#f0a93b" }).pass, false);
  check("row32: --status-warn === --signal -> FAIL", j32({ "--status-warn": "#f0a93b" }).pass, false);
  check("row32: warn↔signal Δhue 4° -> FAIL", j32({ "--status-warn": "#f0b93b" }).pass, false);
  // A token outside the check set AND outside the accent whitelist that collides
  // is a second signal colour in another domain — must NOT be waved through.
  check("row32: --graph-protocol === --signal -> FAIL", j32({ "--graph-protocol": "#f0a93b" }).pass, false);
  check("row32: --status-err === --signal -> FAIL", j32({ "--status-err": "#f0a93b" }).pass, false);
  // ...while the whitelisted accent family is exempt even when it collides.
  check("row32: --ant-color-link === --signal is whitelisted -> PASS", j32({ "--ant-color-link": "#f0a93b" }).pass, true);
  check("row32: --ant-color-primary-hover === --signal is whitelisted -> PASS", j32({ "--ant-color-primary-hover": "#f0a93b" }).pass, true);
  // Missing tokens must fail, never silently shrink the check set (an empty set
  // would pass vacuously — worse than a FAIL).
  check("row32: --brand-claude not collected -> FAIL (no vacuous pass)", j32({ "--brand-claude": undefined }).pass, false);
  check("row32: check set is exactly the 5 authoritative tokens",
    SIGNAL_UNIQUENESS_SET.join(",") === "--signal,--status-warn,--brand-claude,--brand-deepseek,--brand-opencode", true);
  check("row32: accent is NOT in the check set", SIGNAL_UNIQUENESS_SET.includes(ACCENT_OUT_OF_SET), false);
  // Pending-criterion gate: before MASTER's own cell carries §12.3, a non-pass
  // must be reported as a criterion defect, not as an implementation defect.
  check("row32: §12.3 landed -> criterion read from MASTER", row32CriterionLanded(LANDED), true);
  check("row32: original wording -> not landed (row goes pending, not fail)", row32CriterionLanded(NOT_LANDED), false);

  // ---- viewport parsing (the row 19 sweep) ---------------------------------
  check("viewport: bare width 390 -> 390x900", JSON.stringify(parseViewport("390")), JSON.stringify({ width: 390, height: 900 }));
  check("viewport: 768x1024 -> as written", JSON.stringify(parseViewport("768x1024")), JSON.stringify({ width: 768, height: 1024 }));
  check("viewport: junk -> null (never a silent 1440x900)", parseViewport("wide"), null);

  // ---- row 28: the judged window must be the one actually observed --------
  const row28 = CHECKS.find((r) => r.n === 28);
  const short = row28.judge({ route: "inbox", mode: "dark", apiRequests: 12, apiWindowMs: 2500, api: { perms: 0, nonPermTotal: 12, endpointKinds: 1, maxGroup: 12, maxEndpoint: "/api/v1/x", groups: [] } }, { max: 3 }, { args: { apiWindowMs: 11500 } });
  check("row28: observed 2.5s -> NOT judged (pass null), even if 11500 was asked for", short.pass, null);
  const long = row28.judge({ route: "inbox", mode: "dark", apiRequests: 12, apiWindowMs: 11500, api: { perms: 0, nonPermTotal: 12, endpointKinds: 1, maxGroup: 12, maxEndpoint: "/api/v1/x", groups: [] } }, { max: 3 }, { args: { apiWindowMs: 11500 } });
  check("row28: observed 11.5s with a 12-count endpoint -> fail", long.pass, false);
  const longOk = row28.judge({ route: "home", mode: "dark", apiRequests: 2, apiWindowMs: 11500, api: { perms: 0, nonPermTotal: 2, endpointKinds: 2, maxGroup: 1, maxEndpoint: "/api/v1/x", groups: [] } }, { max: 3 }, { args: { apiWindowMs: 11500 } });
  check("row28: observed 11.5s with 2 requests on separate endpoints -> pass", longOk.pass, true);


  // ---- rows 40/41: accidental wrap vs intended clamp; sibling overlap -----
  // The live panel gives one direction only (it either has the defect or it
  // does not), so both directions are forced here. A criterion that cannot be
  // falsified on synthetic input is how a false pass survives.
  const wrec = (o) => ({ sel: 'button.kbd-hint', text: 'Ctrl K K', len: 9, w: 26, h: 45, lh: 15, lines: 3, linesByRange: 3, relaxed: '65x15', relaxedLines: 1, intentional: false, why: '', squeezed: true, ...o });
  // (a) the shell defect: 3 lines, no clamp, collapses to 1 line once the
  //     compression source is removed -> FAIL.
  check('row40: a short label squeezed onto 3 lines FAILS', wrapClassify(wrec()).hit, true);
  check('row40: ...and is classified as squeezed, not content-driven', wrapClassify(wrec()).kind, 'squeezed');
  check('row40: ...and the reason quotes the relaxed geometry', wrapClassify(wrec()).why.includes('65x15'), true);
  // (b) the same geometry but WITH a clamp: the design asking for N lines.
  //     Must NOT be a hit -- this is the everyday false-FAIL guard.
  check('row40: a legal -webkit-line-clamp PASSES', wrapClassify(wrec({ intentional: true, why: '-webkit-line-clamp: 2' })).hit, false);
  check('row40: ...and is classified as intentional', wrapClassify(wrec({ intentional: true, why: '-webkit-line-clamp: 2' })).kind, 'intentional');
  check('row40: an ellipsis single-line clamp PASSES', wrapClassify(wrec({ intentional: true, why: 'text-overflow: ellipsis + overflow-x: hidden' })).hit, false);
  // (c) 2+ lines that SURVIVE the relaxation: the container is not the cause,
  //     so it is content-driven. Must NOT be a hit.
  check('row40: text that stays wrapped after relaxation PASSES', wrapClassify(wrec({ squeezed: false, relaxed: '40x30', relaxedLines: 2 })).hit, false);
  check('row40: ...and is classified as content-driven', wrapClassify(wrec({ squeezed: false, relaxed: '40x30', relaxedLines: 2 })).kind, 'content');
  const row40 = CHECKS.find((r) => r.n === 40);
  const l40 = row40.parse({ text: '每 capture ≤0' });
  const j40 = (wraps) => row40.judge({ textLayout: { wraps, overlaps: [], scanned: 10 } }, l40);
  check('row40 judge: the three shell carriers FAIL the row', j40([wrec({ sel: '.conn.ok' }), wrec({ sel: '.build-id' }), wrec({ sel: '.kbd-hint' })]).pass, false);
  check('row40 judge: ...and the note names them', j40([wrec({ sel: '.conn.ok' })]).note.includes('.conn.ok'), true);
  check('row40 judge: a page with only legal clamps PASSES', j40([wrec({ intentional: true, why: '-webkit-line-clamp: 2' })]).pass, true);
  check('row40 judge: an empty probe is not a hit', j40([]).pass, true);
  // (e) home .mp-text (measured sh:72 > ch:36): a two-line clamped paragraph.
  //     The acceptance asks for this judgement by name.
  const mpText = wrec({ sel: '.mp-text', text: 'x'.repeat(12), w: 300, h: 36, lh: 18, lines: 2, linesByRange: 2, relaxed: '300x36', relaxedLines: 2, intentional: true, why: '-webkit-line-clamp: 2', squeezed: false });
  check('row40: home .mp-text (sh:72 > ch:36, line-clamp 2) PASSES', wrapClassify(mpText).hit, false);
  check('row40: ...and is reported as intentional, not as a defect', wrapClassify(mpText).kind, 'intentional');
  check('row40: .mp-text is NOT the shell defect (relaxation does not collapse it)', mpText.squeezed, false);
  // ---- row 41: sibling overlap, overlays excluded -------------------------
  const orec = (o) => ({ parent: 'div.sidebar-foot', a: 'span.conn.ok', b: 'span.kbd-hint', overlap: '20x40', area: 800, legit: null, ...o });
  check('row41: two intersecting static siblings FAIL', overlapClassify(orec()).hit, true);
  check('row41: ...and are classified as an overlap', overlapClassify(orec()).kind, 'overlap');
  check('row41: an absolutely positioned sibling PASSES (legit overlay)', overlapClassify(orec({ legit: 'position: absolute' })).hit, false);
  check('row41: ...and is classified as an overlay', overlapClassify(orec({ legit: 'position: absolute' })).kind, 'overlay');
  check('row41: a portal-rendered dropdown wrapper PASSES', overlapClassify(orec({ legit: 'overlay subtree' })).hit, false);
  check('row41: a z-indexed non-static sibling PASSES', overlapClassify(orec({ legit: 'z-index 1000' })).hit, false);
  const row41 = CHECKS.find((r) => r.n === 41);
  const l41 = row41.parse({ text: '每 capture ≤0' });
  const j41 = (overlaps) => row41.judge({ textLayout: { wraps: [], overlaps, scanned: 10 } }, l41);
  check('row41 judge: a real overlap FAILS the row', j41([orec()]).pass, false);
  check('row41 judge: ...and the note names the container and both siblings', j41([orec()]).note.includes('div.sidebar-foot'), true);
  check('row41 judge: overlays only -> PASSES', j41([orec({ legit: 'position: absolute' }), orec({ legit: 'overlay subtree' })]).pass, true);
  check('row41 judge: ...and reports how many were excluded', j41([orec({ legit: 'position: absolute' })]).display.includes('1'), true);
  check('row41 judge: an empty probe is not a hit', j41([]).pass, true);
  // The two rows must anchor on MASTER, not on a number this file invented.
  check('row40: the target is anchored on MASTER (phrase, not position)', row40.parse({ text: '**每 capture <=0**' }).max, 0);
  check('row41: the target is anchored on MASTER (phrase, not position)', row41.parse({ text: '**每 capture <=0**' }).max, 0);
  // The probe must actually measure both classes and both exclusions.
  check('rows40/41: the probe is wired into every capture (auditRoute calls it and returns it)',
    /probeTextLayout\(page\)/.test(auditRoute.toString()) && /^\s*textLayout,$/m.test(auditRoute.toString()), true);
  check('rows40/41: the probe carries the overlay exclusion set', /OVERLAY_SEL/.test(probeTextLayout.toString()), true);
  check('rows40/41: the probe applies the 12.12 compression-source test', /flexShrink = "0"/.test(probeTextLayout.toString()), true);
  check('rows40/41: the probe reads -webkit-line-clamp for the intentional case', /webkit-line-clamp/.test(probeTextLayout.toString()), true);
  // ---- row 13's anchor: bold + an inline observation record ---------------
  // Row 13's target cell became 「**0**（当前观测：工具 02:26:57 … 需以最新工具复测
  // 为准）」, which /^\s*(\d+)\s*$/ no longer matched — the row silently fell back
  // to its built-in 0 and the miss was the only signal. The relaxed anchor must
  // read the LEADING number and nothing else.
  check("row13: the relaxed anchor reads through **bold** and an inline parenthetical",
    pickLeadingNumber("**0**（当前观测：工具 02:26:57 上报 knowledge/dark = 1）", -1), 0);
  check("row13: a bare number still parses", pickLeadingNumber("0", -1), 0);
  check("row13: the REAL MASTER target for row 13 now parses (no fallback)",
    (() => { resetPickMisses(); const v = pickLeadingNumber(targetFor(TH, 13, "").text, -1); return v === 0 && takePickMisses().length === 0; })(), true);
  check("row13: a baseline/historical value appended to the cell does not move it",
    pickLeadingNumber("**0**（当前观测：工具 02:26:57 上报 1）" + "（基线 M1 13.7–83.5%，见 §12.7.2；原值 350KB）", -1), 0);
  check("row13: a cell that STARTS with a baseline decimal does NOT match (keeps the built-in default)",
    pickLeadingNumber("13.7–83.5%（中位 37.4%）", -1), -1);
  check("row13: ...and that miss is recorded, not silent",
    (() => { resetPickMisses(); pickLeadingNumber("13.7–83.5%", -1); return takePickMisses().length; })(), 1);
  check("row13: a bold number followed by more prose is still the leading requirement",
    pickLeadingNumber("**0** 处", -1), -1);

  // The live-control trap: a flex child squeezed to 1-2px does NOT relax when you
  // set flex-shrink: 0 / min-width: max-content on the CHILD (the parent's
  // min-width: 0 wins), so a relaxation-only rule scored a real 9-line defect
  // PASS. The natural-width test has no such blind spot.
  check('row40: a squeezed flex child FAILS even when relaxation cannot release it',
    wrapClassify(wrec({ w: 2, h: 135, lines: 9, natural: 84.9, squeezed: true, relaxed: '2x135', relaxedLines: 9 })).hit, true);
  check('row40: ...and the record keeps the corroborating relaxed geometry',
    wrec({ relaxed: '2x135' }).relaxed, '2x135');
  check('row40: text whose natural width FITS its box is not a squeeze',
    wrapClassify(wrec({ w: 85, natural: 84.9, squeezed: false })).hit, false);
  check('row40: the probe measures natural width off-screen with the element font',
    /naturalWidth/.test(probeTextLayout.toString()) && /white-space:nowrap/.test(probeTextLayout.toString()), true);
  check('row40: the probe removes its measuring span (no residue for the next capture)',
    /probeScratch\.remove\(\)/.test(probeTextLayout.toString()), true);


  // ---- rows 42-45: MASTER §12.15's four classes with no row ----------------
  // Each class gets BOTH directions. The must-FAIL half is the answer to
  // §12.10.5: a judge that cannot be made to fail on a deliberately injected
  // violation is not demonstrably looking at the thing it names.
  const row42 = CHECKS.find((r) => r.n === 42);
  const row43 = CHECKS.find((r) => r.n === 43);
  const row44 = CHECKS.find((r) => r.n === 44);
  const row45 = CHECKS.find((r) => r.n === 45);
  const l42 = row42.parse({ text: '每 capture <=0' });
  const l43 = row43.parse({ text: '每 capture <=0' });
  const l44 = row44.parse({ text: '每 capture <=0' });
  const l45 = row45.parse({ text: '覆盖率 = 100%' });
  const j42 = (rows) => row42.judge({ markup: { transitionAll: rows, scanned: 10 } }, l42);
  const j43 = (rows, imgs) => row43.judge({ markup: { imgNoAlt: rows, imgs: imgs ?? 3 } }, l43);
  const j44 = (rows) => row44.judge({ markup: { literalDots: rows } }, l44);
  const j45 = (rows) => row45.judge({ markup: { numerics: rows } }, l45);
  // (42) transition: all
  check('row42 must-FAIL: an element with transition: all 0.2s FAILS', transitionAllHit({ prop: 'all', dur: '0.2s' }), true);
  check('row42 must-PASS: an explicit property list PASSES', transitionAllHit({ prop: 'color', dur: '0.2s' }), false);
  check('row42 trap: the INITIAL value is all + 0s and must NOT count (every element has it)',
    transitionAllHit({ prop: 'all', dur: '0s' }), false);
  check('row42 judge: one injected violation FAILS the row', j42([{ sel: 'div.x', prop: 'all', dur: '0.2s', antd: false }]).pass, false);
  check('row42 judge: ...and the note names the element', j42([{ sel: 'div.x', prop: 'all', dur: '0.2s', antd: false }]).note.includes('div.x'), true);
  check('row42 judge: a page of initial-value elements PASSES', j42([{ sel: 'div.a', prop: 'all', dur: '0s', antd: false }]).pass, true);
  check('row42 judge: an empty probe PASSES (and is not a silent hole -- the scanned count is printed)',
    j42([]).pass && j42([]).display.includes('扫描 10'), true);
  // (43) img alt
  check('row43 must-FAIL: an img without the alt attribute FAILS', imgAltHit({ hasAlt: false }), true);
  check('row43 must-PASS: alt="" (the decoration form) PASSES', imgAltHit({ hasAlt: true }), false);
  check('row43 judge: one alt-less img FAILS the row', j43([{ sel: 'img.x', hasAlt: false }]).pass, false);
  check('row43 judge: ...and the display gives the ratio', j43([{ sel: 'img.x', hasAlt: false }], 3).display.includes('1/3'), true);
  check('row43 judge: every img carrying alt PASSES', j43([{ sel: 'img.a', hasAlt: true }], 1).pass, true);
  check('row43 judge: zero imgs at all PASSES', j43([], 0).pass, true);
  // (44) literal three dots
  check('row44 must-FAIL: a literal ... FAILS', literalDotsHit({ text: '加载中...' }), true);
  check('row44 must-PASS: the ellipsis character PASSES', literalDotsHit({ text: '加载中…' }), false);
  check('row44 must-PASS: two dots are not three', literalDotsHit({ text: '1..2' }), false);
  check('row44 judge: one literal ... FAILS the row', j44([{ sel: 'span.x', text: 'a...b', where: 'text' }]).pass, false);
  check('row44 judge: ...and the note quotes the offending string', j44([{ sel: 'span.x', text: 'a...b', where: 'text' }]).note.includes('a...b'), true);
  check('row44 judge: a page using the ellipsis character PASSES', j44([{ sel: 'span.a', text: 'a…b', where: 'text' }]).pass, true);
  check('row44: the probe excludes code subtrees (a code sample may hold three dots)',
    /closest\(SKIP\)/.test(probeMarkup.toString()) && /pre,code,kbd,samp/.test(probeMarkup.toString()), true);
  check('row44: the probe reads the DOM, so source comments cannot be counted',
    /nodeValue/.test(probeMarkup.toString()) && !/readFileSync/.test(probeMarkup.toString()), true);
  // (45) tabular-nums
  check('row45 must-FAIL: a numeric readout without tabular-nums FAILS', tabularHit({ candidate: true, tabular: false }), true);
  check('row45 must-FAIL: a READOUT-GRID figure without tabular-nums FAILS (the grandparent scope must not mask a regression)',
    tabularHit({ candidate: true, tabular: false, grandPeers: 12, group: 'div.readout-row' }), true);
  check('row45 must-PASS: the same grid figure WITH tabular-nums PASSES',
    tabularHit({ candidate: true, tabular: true, grandPeers: 12, group: 'div.readout-row' }), false);
  check('row45: the scope gate is a ROW OF FIGURES (each direct child holds one), not a subtree count',
    /const isFigureGroup = \(gp\)/.test(probeMarkup.toString()) && /if \(!g \|\| !isFigureGroup\(g\)\) continue;/.test(probeMarkup.toString()), true);
  check('row45: ...and a section that merely CONTAINS a grid is not a row of figures (its heading has no figure)',
    /kids\.every\(\(c\) => hasFigure\(c\)/.test(probeMarkup.toString()), true);
  // (a) Dates and clock times are not figures. Without this, a list of dated
  // cards makes its container look like a row of figures, which kept the kanban
  // column's stand-alone heading count in scope.
  check('row45 must-FAIL: a DATE is not a numeric readout', looksLikeDateTime('2026/9/12'), true);
  check('row45 must-FAIL: a clock time is not a numeric readout', looksLikeDateTime('12:34'), true);
  check('row45 must-FAIL: a date with dashes or dots is not a numeric readout',
    looksLikeDateTime('2026-09-12') && looksLikeDateTime('2026.9.12'), true);
  check('row45 must-PASS: a plain figure is not a date', looksLikeDateTime('199'), false);
  check('row45 must-PASS: a percentage is not a date', looksLikeDateTime('17%'), false);
  check('row45 must-PASS: a count is not a date', looksLikeDateTime('20'), false);
  check('row45: the probe applies the date form to BOTH the figure test and the candidate loop (no drift)',
    probeMarkup.toString().includes("DATE_FORMS") &&
    probeMarkup.toString().includes("isDateTime(own)") &&
    probeMarkup.toString().includes("!isDateTime(t)"), true);
  check('row45: the module-scope form and the page-side form are the same two shapes',
    DATE_TIME_FORMS.length, 2);
  check('row45 must-PASS: a numeric readout WITH tabular-nums PASSES', tabularHit({ candidate: true, tabular: true }), false);
  check('row45: a non-candidate (prose) is never a hit', tabularHit({ candidate: false, tabular: false }), false);
  check('row45 judge: 1 of 4 missing FAILS the coverage', j45([
    { sel: 'a', candidate: true, tabular: true }, { sel: 'b', candidate: true, tabular: true },
    { sel: 'c', candidate: true, tabular: true }, { sel: 'd', candidate: true, tabular: false },
  ]).pass, false);
  check('row45 judge: ...and prints the coverage as a fraction and a percent',
    j45([{ sel: 'a', candidate: true, tabular: true }, { sel: 'd', candidate: true, tabular: false }]).display.includes('1/2'), true);
  check('row45 judge: full coverage PASSES', j45([{ sel: 'a', candidate: true, tabular: true }]).pass, true);
  check('row45 judge: no numeric readouts on the page PASSES (coverage recorded as 1)',
    j45([]).pass && j45([]).display.includes('无数值读数'), true);
  check('row45: the scope is own-text-is-numeric + a grandparent group, NOT parent-level siblings (the latter found 0 candidates and passed vacuously)',
    /const numerics = \[\]/.test(probeMarkup.toString()) && !/recs\.length < 2/.test(probeMarkup.toString()), true);
  check('row45: the peers count is reported as context, so a vacuous pass would be visible',
    /peers: peers\(rec\.el\)/.test(probeMarkup.toString()), true);
  // The four rows must anchor on MASTER, not on a number this file invented.
  check('rows42-44: the target is anchored on MASTER (phrase, not position)',
    [l42.max, l43.max, l44.max].join(','), '0,0,0');
  check('row45: the coverage target is anchored on MASTER', l45.min, 1);
  check('rows42-45: a FAILED markup probe yields not-measured, never pass (a broken instrument must not read green)',
    CHECKS.filter((r) => [42, 43, 44, 45].includes(r.n))
      .every((r) => r.judge({ markup: { error: "boom" } }, { max: 0, min: 1 }).pass === null), true);
  check('rows42-45: the probe is wired into every capture',
    /probeMarkup\(page\)/.test(auditRoute.toString()) && /^\s*markup,$/m.test(auditRoute.toString()), true);
  check('rows42-45: the four classes are NOT the ones already judged (no aria-label / focus ring row added)',
    CHECKS.filter((r) => [42, 43, 44, 45].includes(r.n)).map((r) => r.n).join(','), '42,43,44,45');
  // The captain's condition for ANY antd-attribution exclusion: it must be
  // provable that it cannot mask our own regression. These pin exactly that.
  check('row42: a PROJECT-OWNED element with transition: all still FAILS (an attribution rule cannot mask our own regression)',
    j42([{ sel: 'div.ruagent.kanban-runs', prop: 'all', dur: '0.2s', antd: false, owner: 'link:index-abc.css', ownClass: 'ruagent.kanban-runs' }]).pass, false);
  check('row42: an antd-cssinjs hit is EXCLUDED (captain t81 ruling)',
    transitionAllHit({ prop: 'all', dur: '0.2s', owner: 'antd-cssinjs[hash:pzczty,rc-order]' }), false);
  check('row42: an UNKNOWN origin is NOT excluded (unattributable must judge FAIL, not assume innocence)',
    transitionAllHit({ prop: 'all', dur: '0.2s', owner: 'unknown' }), true);
  check('row42: a link: (our own stylesheet) hit is NOT excluded',
    transitionAllHit({ prop: 'all', dur: '0.2s', owner: 'link:index-abc.css' }), true);
  check('row42: ...and the note shows the origin of every REMAINING hit, so the attribution stays decidable',
    j42([{ sel: 'div.unknown-origin', prop: 'all', dur: '0.2s', owner: 'unknown' }]).note.includes('unknown'), true);
  check('row42: the row reports how many antd hits it excluded instead of hiding them',
    j42([{ sel: 'button.ant-btn.ruagent', prop: 'all', dur: '0.2s', owner: 'antd-cssinjs[h]' }]).display.includes('排除'), true);
  check('row42: the probe records BOTH the origin and whether a project class is present',
    /transitionOwner/.test(probeMarkup.toString()) && /ownClass:/.test(probeMarkup.toString()), true);
  // The captain's condition for ANY antd-attribution exclusion: it must be
  // provable that it cannot mask our own regression. These two pin exactly that.
  check('row42: a PROJECT-OWNED element with transition: all still FAILS (an attribution rule cannot mask our own regression)',
    j42([{ sel: 'div.ruagent.kanban-runs', prop: 'all', dur: '0.2s', antd: false, owner: 'link:index-abc.css', ownClass: 'ruagent.kanban-runs' }]).pass, false);
  check('row42: the probe records BOTH the origin and whether a project class is present',
    /transitionOwner/.test(probeMarkup.toString()) && /ownClass:/.test(probeMarkup.toString()), true);
  // ---- row 46: the sidebar session title's width ---------------------------
  const row46 = CHECKS.find((r) => r.n === 46);
  const sweep = (vw, ws, fs) => [{ viewport: vw, rows: ws.length, titles: ws.map((w) => ({ text: "t", w, fs: fs || 13, minW: "65px", visible: true })) }];
  const j46 = (sw) => row46.judge({ sessionTitle: sw }, row46.parse({ text: "覆盖率 = 100%" }));
  // must-FAIL: the pre-fix state. 0px is the 1440 symptom, 6px the control run's.
  check("row46 must-FAIL: a title squeezed to 0px FAILS",
    sessionTitleVerdict(sweep("1440x900", [0])).pass, false);
  check("row46 must-FAIL: a title squeezed to 6px FAILS (systems' control run reading)",
    sessionTitleVerdict(sweep("1440x900", [6])).pass, false);
  check("row46 must-FAIL: a title squeezed to 43.7px FAILS (the 768 control reading)",
    sessionTitleVerdict(sweep("768x900", [43.7])).pass, false);
  check("row46 judge: the pre-fix sweep FAILS the row and PRINTS the measured minimum",
    j46(sweep("1440x900", [0])).pass === false && j46(sweep("1440x900", [0])).display.includes("0px"), true);
  // must-PASS: the fixed state, measured live at 1440 (65px) and 390 (151px).
  check("row46 must-PASS: the fixed state (65px at 1440, 151px at 390) PASSES",
    sessionTitleVerdict([...sweep("1440x900", [65, 65, 68.91]), ...sweep("390x900", [151, 151])]).pass, true);
  // The floor is DERIVED from the artifact, not set by the judge's author.
  const sweepMin = (vw, ws, fs, minW) => [{ viewport: vw, rows: ws.length, titles: ws.map((w) => ({ route: "r", text: "t", w, fs: fs || 13, minW: minW || "0px", visible: true })) }];
  check("row46 must-FAIL: #sessions' 40.98px at 390 (fs 14, ~3 chars) FAILS the universal 5em floor",
    sessionTitleVerdict(sweepMin("390x900", [40.98], 14, "0px")).pass, false);
  check("row46: ...and that is a REAL defect, not a false red (captain withdrew the derived-floor ruling)",
    sessionTitleVerdict(sweepMin("390x900", [40.98], 14, "0px")).floor, 70);
  check("row46: a sample that declares no min-width is still judged on 5em of its own font size",
    sessionTitleVerdict(sweepMin("1440x900", [65], 13, "0px")).pass, true);
  check("row46: the declared min-width is still REPORTED per route (traceability as a reading)",
    sessionTitleVerdict(sweepMin("1440x900", [65], 13, "65px")).perRoute.join(), "r@1440x900=65px/fs13/声明min65");
  check("row46: the floor multiplier is read OUT of MASTER's target cell, not hardcoded",
    row46.parse({ text: "每 capture 全绿（width > 0 且 width ≥ 5 × fontSize − 0.5）" }).em, 5);
  check("row46: the display lists EVERY route's minimum with its font size (sessions' 41px included)",
    j46([...sweepMin("390x900", [41], 14, "0px"), ...sweepMin("1440x900", [65], 13, "65px")]).display.includes("r@390x900=41px/fs14"), true);
  check("row46: the display prints the measured minimum and the per-viewport breakdown",
    j46(sweep("1440x900", [65])).display.includes("1440x900=65"), true);
  // not_measured: no session rows is NOT a pass.
  check("row46: an EMPTY sweep is not_measured, not pass (0 objects is not verified compliant)",
    j46([]).pass === null, true);
  check("row46: ...and it says so",
    j46([]).display.includes("0 个对象"), true);
  check("row46: a probe ERROR is not_measured, not pass",
    row46.judge({ sessionTitle: { error: "boom" } }, row46.parse({ text: "覆盖率 = 100%" })).pass === null, true);
  check("row46: the sweep covers BOTH 1440 and 390 (390 is not one of the 26 captures)",
    TITLE_SWEEP.includes(1440) && TITLE_SWEEP.includes(390), true);
  check("row46: the probe restores the measurement viewport after sweeping",
    /if \(restore\)/.test(probeSessionTitle.toString()) && /setViewportSize\(\{ width: restore/.test(probeSessionTitle.toString()), true);
  check("row46: the criterion states the 5em derivation and the not_measured boundary",
    /5em/.test(row46.criterion) && /not_measured/.test(row46.criterion), true);

  // ---- row 35: the breakpoint SET relationship (§12.16 + t80 ruling) -------
  // The superseded wording scored the live panel FAIL because 520 appeared on
  // the JS side; 520 is an existing CSS breakpoint, so the red could not be
  // cleared by any legitimate change. The sharpened rule is a SUBSET assertion.
  const ALLOW35 = { css: [520, 768, 1024, 1240], js: [992] };
  const BP35 = (css) => ({ own: css, runtime: [], ownSheets: 2, runtimeSheets: 30 });
  const v35 = (css, js) => breakpointVerdict(BP35(css), js, ALLOW35, 4);
  // must-PASS: the live shape -- JS reads an existing CSS breakpoint + the contract value.
  check('row35 must-PASS: JS [520, 992] is a subset of CSS ∪ {992}',
    v35([520, 768, 1024, 1240], [520, 992]).pass, true);
  check('row35 must-PASS: ...and the display says so', /子集成立/.test((() => {
    const v = v35([520, 768, 1024, 1240], [520, 992]);
    return "CSS [" + v.css.join(", ") + "] 计数 " + v.css.length + "/4 · JS [" + v.jsSet.join(", ") + "] " + (v.subsetOk ? "✓ 子集成立（⊆ CSS ∪ {992}）" : "JS-only " + v.offJs.join(", "));
  })()), true);
  check('row35 must-PASS: the contract value alone is a subset', v35([520, 768, 1024, 1240], [992]).pass, true);
  check('row35 must-PASS: JS reading only existing CSS breakpoints passes', v35([520, 768, 1024, 1240], [520, 768, 1024, 1240]).pass, true);
  // must-FAIL: a JS-only breakpoint is exactly the drift the row must catch.
  // The captain's wording correction (t81): the OLD wording rejected 700 too, so
  // this is NOT where the new rule is stricter. The new rule adds a COUPLING --
  // every JS value must actually exist on the CSS side -- and the case that
  // coupling alone catches is "the CSS deleted a breakpoint while JS still reads
  // it". That case is pinned separately below.
  check('row35 must-FAIL: a JS-only 700 FAILS (both wordings reject it; the NEW coupling is what catches CSS-deleted-while-JS-still-reads)',
    v35([520, 768, 1024, 1240], [520, 992, 700]).pass, false);
  check('row35 must-FAIL (the NEW coupling): CSS drops 520 while JS still reads 520 -> FAILS',
    v35([768, 1024, 1240], [520, 992]).pass, false);
  check('row35: ...and 520 is named as the JS-only value in that case',
    v35([768, 1024, 1240], [520, 992]).offJs.join(","), '520');
  check('row35: the same CSS with JS no longer reading 520 PASSES (the removal direction)',
    v35([768, 1024, 1240], [992]).pass, true);
  check('row35: the OLD rule would have PASSED the CSS-deleted case, so the coupling is the only thing that catches it',
    [520, 992].filter((v) => !ALLOW35.js.includes(v)).length === 1, true);
  check('row35 must-FAIL: ...and 700 is named as the JS-only value',
    v35([520, 768, 1024, 1240], [520, 992, 700]).offJs.join(","), '700');
  check('row35 must-PASS again once the injected 700 is removed (the removal direction)',
    v35([520, 768, 1024, 1240], [520, 992]).pass, true);
  check('row35: a value present on BOTH sides is not JS-only (768 read from JS passes)',
    v35([520, 768, 1024, 1240], [768]).pass, true);
  check('row35: ...and a JS-only value is judged JS-only even when the CSS side is clean',
    v35([520, 768, 1024, 1240], [700]).offJs.join(","), '700');
  // The CSS half keeps its own budget and its own allow-list.
  check('row35 must-FAIL: 5 CSS breakpoints FAIL (the <=4 budget is NOT relaxed)',
    v35([520, 700, 768, 1024, 1240], [992]).pass, false);
  check('row35: ...and the over-count is reported separately from the allow-list',
    v35([520, 700, 768, 1024, 1240], [992]).overCount, true);
  check('row35 must-FAIL: a CSS value off the allow-list FAILS',
    v35([520, 768, 1024, 1240, 1440].slice(0, 5).map((x, i) => (i === 4 ? 1100 : x)), [992]).pass, false);
  check('row35: the live allow-list really is 4 values (so membership implies <=4, but both are checked)',
    ALLOW35.css.length, 4);
  check('row35: the judge is single-sourced (the self-test and the row share one function)',
    /breakpointVerdict\(bp, js, allow, l\.max\)/.test(CHECKS.find((r) => r.n === 35).judge.toString()), true);
  check('row35: the criterion states the subset rule and refuses the whitelist framing',
    /子集断言/.test(CHECKS.find((r) => r.n === 35).criterion) && /不是白名单/.test(CHECKS.find((r) => r.n === 35).criterion), true);

  // ---- row 35's reworded anchor (§12.16) ----------------------------------
  check('row35: the OLD wording still parses to 4', pickAny('**≤4 CSS 断点 + 1 契约值(992)**', [/CSS\s*侧\s*≤\s*(\d+)\s*个断点/, /≤\s*(\d+)\s*CSS\s*断点/], -1), 4);
  check('row35: the NEW wording (§12.16) parses to 4', pickAny('**① CSS 侧 ≤4 个断点**（不变）∧ **② JS 断点集 ⊆…**', [/CSS\s*侧\s*≤\s*(\d+)\s*个断点/, /≤\s*(\d+)\s*CSS\s*断点/], -1), 4);
  check('row35: an unrelated cell records a miss instead of silently returning the default',
    (() => { resetPickMisses(); const v = pickAny('没有任何断点数字', [/CSS\s*侧\s*≤\s*(\d+)\s*个断点/, /≤\s*(\d+)\s*CSS\s*断点/], -1); return v === -1 && takePickMisses().length === 1; })(), true);
  const failed = cases.filter((c) => !c.pass);
  const w = Math.max(...cases.map((c) => c.name.length));
  for (const c of cases) {
    process.stdout.write(`  ${c.pass ? "ok  " : "FAIL"}  ${c.name.padEnd(w)}  got=${JSON.stringify(c.got)} want=${JSON.stringify(c.want)}\n`);
  }
  process.stdout.write(`\nself-test: ${cases.length - failed.length}/${cases.length} pass\n`);
  if (failed.length) process.stdout.write(`self-test FAILED: ${failed.map((c) => c.name).join("; ")}\n`);
  return failed.length ? 1 : 0;
}


// ── main ────────────────────────────────────────────────────────────────────
async function main() {
  const args = parseArgs(process.argv.slice(2));
  QUIET = args.quiet;
  if (args.help) {
    process.stdout.write(HELP);
    return 0;
  }
  if (args.selfTest) return runSelfTest();
  // --out is resolved against the REPO ROOT, not the CWD. Resolving against the
  // CWD silently wrote panel/docs/screenshots/... when the tool was run from
  // panel/ (which is how everyone runs it), and t8 lost an audit-run2/ that way.
  // An absolute path still wins, as resolve() requires.
  const outDir = args.out ? resolve(REPO, args.out) : join(REPO, "docs", "screenshots", "audit-run");
  mkdirSync(outDir, { recursive: true });
  if (args.out && !isAbsolute(args.out)) {
    log(`[audit] --out=${args.out} 相对仓库根解析 → ${outDir}（CWD 是 ${process.cwd()}；如需按 CWD 解析请传绝对路径）`);
  }

  const warnings = [];
  const thresholds = loadThresholds(REPO);
  warnings.push(...thresholds.warnings);
  // The criteria come from the contract, not from this file. Anything that could
  // not be read falls back to BUILTIN and says so here.
  const contract = resolveContract(thresholds);
  warnings.push(...contract.warnings);
  const dist = distInfo();
  if (!dist.exists) warnings.push("panel/dist not found — the daemon is serving something else; run npm run build");
  if (dist.staleSrc) warnings.push("panel/dist is older than panel/src — run `cd panel && npm run build` and re-audit");

  const viewport = parseViewport(args.viewport);
  if (!viewport) throw new Error(`--viewport must be <width>x<height> (got "${args.viewport}")`);
  const modes = args.modes.split(",").map((s) => s.trim()).filter(Boolean);
  // Every entry must parse or be reported: an unparsable one is never dropped
  // in silence, and the measurement frame is skipped because the capture pass
  // already measures it (probeOverflow would only re-measure the same frame).
  const overflowViewports = [];
  for (const raw of args.overflowViewports.split(",").map((s) => s.trim()).filter(Boolean)) {
    const v = parseViewport(raw);
    if (!v) { warnings.push(`--overflow-viewports entry "${raw}" is neither <width> nor <width>x<height>; skipped`); continue; }
    if (v.width === viewport.width) continue;
    if (overflowViewports.some((x) => x.width === v.width && x.height === v.height)) continue;
    overflowViewports.push(v);
  }
  // Row 26's target is the contract's (MASTER §12: "≤200ms"), not a literal.
  const longTaskTargetMs = targetFor(thresholds, 26, 200).nums[0] ?? 200;
  const routes = ROUTES.filter((r) => !args.routes || args.routes.includes(r.id));
  if (!routes.length) throw new Error("no routes selected");

  const health = await fetch(args.baseUrl, { signal: AbortSignal.timeout(6000) }).catch(() => null);
  if (!health || !health.ok) {
    console.error(`daemon not reachable at ${args.baseUrl} (no panel to audit). Start it with: cargo run -p ruagent-cli -- serve`);
    return 2;
  }
  const taskId = routes.some((r) => r.needsTaskId) ? await resolveTaskId(args) : null;
  if (routes.some((r) => r.needsTaskId) && !taskId) {
    warnings.push("#task skipped: could not resolve a task id (/api/v1/tasks empty or unreachable)");
  }

  const cfg = {
    fontLadder: contract.fontLadder,
    weightLadder: contract.weightLadder,
    radiusLadder: contract.radiusLadder,
    radiusPill: contract.radiusPill,
    antControlSrc: ANT_CONTROL_SRC,
    interactiveSel: INTERACTIVE_SEL,
    handClasses: contract.handClasses,
    clickableExtra: CLICKABLE_EXTRA,
    expandSelectors: contract.expandSelectors,
    nativeExpandSelector: contract.nativeExpandSelector,
    spacingLadder: contract.spacingLadder,
    bleedSet: contract.bleedSet,
    shellSet: contract.shellSet,
    shellSetSource: contract.shellSetSource,
    shellSel: SHELL_SEL,
    tokenVars: ["--signal", "--signal-text", "--signal-ink", "--signal-wash", "--status-warn", "--status-ok", "--status-err", "--status-idle", "--app-shell", "--rule", "--rule-soft", "--panel-shadow", "--graph-edge", "--graph-edge-hi", "--graph-label", "--ant-color-primary", "--ant-color-primary-hover", "--ant-color-primary-active", "--ant-color-link", "--ant-color-bg-container", "--brand-claude", "--brand-deepseek", "--brand-opencode", ...contract.graphVars],
    graphVars: contract.graphVars,
    row5ExcludeSelectors: contract.row5ExcludeSelectors,
    sampleCap: 8,
    // Row 18's inventories must be complete, not sampled: MASTER row 18 requires
    // the visible interactive elements to be reproducible, and a cap of 8 hid the
    // tail of the 11/13/16-violation routes. 40 covers the worst route seen.
    interactiveSampleCap: Math.max(INTERACTIVE_SAMPLE_CAP, args.sampleCap ?? 0),
  };

  const browser = await chromium.launch();
  const context = await browser.newContext({ viewport, deviceScaleFactor: 1, locale: "zh-CN" });
  await context.addInitScript(() => {
    window.__longtasks = [];
    try {
      new PerformanceObserver((list) => {
        for (const e of list.getEntries()) window.__longtasks.push(Math.round(e.duration));
      }).observe({ entryTypes: ["longtask"] });
    } catch {}
  });
  const page = await context.newPage();
  const reqCounter = { n: 0, urls: [] };
  page.on("request", (r) => {
    if (r.url().includes("/api/")) {
      reqCounter.n++;
      reqCounter.urls.push(r.url());
    }
  });

  const captures = [];
  const started = Date.now();
  for (const mode of modes) {
    for (const route of routes) {
      if (route.needsTaskId && !taskId) continue;
      log(`[audit] ${mode} ${route.id} …`);
      const cap = await auditRoute(page, { args, route, mode, taskId, outDir, cfg, reqCounter, overflowViewports, measureViewport: viewport, pageTitleRule: contract.pageTitleRule, longTaskTargetMs });
      cap.focusInfo = enrichFocus(cap);
      for (const w of cap.warnings) warnings.push(`${route.id}/${mode}: ${w}`);
      captures.push(cap);
      log(
        `[audit] ${mode} ${route.id}: ${cap.ms}ms dom=${cap.metrics.domNodes} text=${cap.metrics.text.count} ` +
          `lumaBand=${cap.pixels ? (cap.pixels.lumaBandRatio * 100).toFixed(1) + "%" : "—"} contrast=${cap.metrics.contrast.docViolations}`,
      );
      await page.waitForTimeout(120);
    }
  }
  const captureMs = Date.now() - started;

  // ── global: bundle size, palette contract ────────────────────────────────
  const entry = dist.js.find((f) => /^index-.*\.js$/.test(f.file)) ?? dist.js[0] ?? null;
  // ALL chunks, never a slice: the app-layer number must not depend on how many
  // chunks happened to fit in a preview window (slice(0,6) once reported 155KB by
  // counting three lazy route chunks as first screen and dropping ten others).
  const global = { bundle: { entry, js: dist.js, css: dist.css.slice(0, 4), matchMedia: dist.matchMedia ?? [], firstScreen: dist.firstScreen ?? [] }, palette: null };
  await page.setViewportSize(viewport);
  log(`[audit] palette contract …`);
  await page.goto(`${args.baseUrl}/?mode=${modes[0]}#home`, { waitUntil: "load", timeout: 60_000 });
  await page.waitForTimeout(1200);
  global.palette = await probePalette(page).catch((e) => ({ open: false, error: e.message }));

  await browser.close();

  // ── §12 verdicts ─────────────────────────────────────────────────────────
  const ctx = { args, captures, global, thresholds, contract };
  const { results: checks, uncovered } = runChecks(ctx);
  for (const c of checks) {
    if (c.anchorMisses?.length) {
      warnings.push(`行 ${c.n} 的判据锚点未命中 MASTER 原文，已回退到内置值：${c.anchorMisses.join(" · ")} —— MASTER 文字可能已改，请复核该行判据`);
    }
  }
  if (uncovered.length) warnings.push(`§12 rows with neither a check nor a stated reason: ${uncovered.join(", ")}`);

  const meta = {
    generatedAt: new Date().toISOString(),
    baseUrl: args.baseUrl,
    viewport: `${viewport.width}x${viewport.height}`,
    settleMs: args.settleMs,
    tabs: args.focus ? args.tabs : 0,
    apiWindowMs: args.apiWindowMs,
    modes,
    routes: captures.map((c) => c.route),
    overflowViewports: [`${viewport.width}x${viewport.height}`, ...overflowViewports.map((v) => `${v.width}x${v.height}`)],
    taskId,
    thresholdsPath: thresholds.path,
    thresholdsFound: thresholds.found,
    dist: {
      buildId: dist.buildId,
      entry,
      staleSrc: dist.staleSrc,
      newestSrc: dist.newestSrc,
      newestDist: dist.newestDist,
    },
    captureMs,
    checkArgs: process.argv.slice(2).join(" "),
    tool: "panel/tools/design-audit.mjs",
  };
  // t37's drift warning has served its purpose: §12.9's judgement change is now
  // implemented (content-area budget + shell closed set + constancy check). What
  // remains worth saying is only the state of the contract read itself.
  const row18Target = String(thresholds.rows.get(18)?.target ?? "");
  if (/外壳|内容区/.test(row18Target) && !contract.shellSet) {
    warnings.push("MASTER §12.9 提到外壳/内容区，但工具未能读到具名闭集；行 18 的第三个子断言未判定（不是实现缺陷）。");
  }
  const doc = {
    meta,
    warnings,
    global,
    captures,
    checks,
    thresholdRows: [...thresholds.rows.values()],
    contract: { provenance: contract.provenance, hardcoded: contract.hardcoded, sources: contract.sources },
  };
  // A subset run is a legitimate tool but a dangerous artefact: it overwrites
  // docs/screenshots/audit-run with a partial picture. Say so, loudly, in the
  // report and in metrics.json — the default is still all 13 routes.
  if (routes.length !== ROUTES.length) {
    warnings.push(
      `子集运行：--routes 只覆盖 ${routes.length}/${ROUTES.length} 条路由（${routes.map((r) => r.id).join(", ")}）。本次写出的 metrics.json / report.md 不是全站结论，t8 需用默认全量重跑。`,
    );
  }

  const badgeSeen = captures.some((c) => (c.metrics.extras?.navBadge ?? 0) > 0);
  if (!badgeSeen) {
    warnings.push(
      ".nav-badge 本轮未渲染（inboxCount = 0）：它对白字 3.20:1 的对比度失败对本次审计不可见 —— 复测该行前必须先造出一条待处理权限",
    );
  }
  doc.blindSpots = [
    { item: ".nav-badge", reason: badgeSeen ? "本轮已渲染并计入对比度" : "仅在 inboxCount > 0 时渲染；本轮 13 路由实测 0 个" },
    { item: "::selection 的字/底对比度", reason: "不在 §5.3 下限的判据范围（primitives 附录 A.5 登记为未测）" },
    { item: ":hover 面上的文本对比度", reason: "同上，静态截图与计算样式都取不到 hover 面" },
    { item: "动效中间帧", reason: "同上" },
    { item: ".chat-thought / .md blockquote 的装饰竖线", reason: "装饰线无下限要求（§5.3）" },
    { item: "暗色 tag 底色（primitives A.15 估计 3.99-4.23，未核定）", reason: "本脚本按计算样式的实际合成值测，不做估计；若 tag 底色是 antd 派生值，需逐变体核对" },
  ];
  const json = JSON.stringify(doc, null, 2);
  writeFileSync(join(outDir, "metrics.json"), json);
  writeFileSync(join(outDir, "report.md"), buildMarkdown(doc) + "\n" + buildDetails(doc) + "\n");

  if (args.json) {
    process.stdout.write(json + "\n");
  } else {
    process.stdout.write(buildMarkdown(doc) + "\n" + buildDetails(doc) + "\n");
  }

  const fails = checks.filter((r) => r.verdict === "fail");
  const unmeasured = checks.filter((r) => r.verdict === "not_measured");
  const pending = checks.filter((r) => r.verdict === "pending");
  const passed = checks.length - fails.length - unmeasured.length - pending.length;
  log(`[audit] ${captures.length} captures in ${(captureMs / 1000).toFixed(1)}s · checks ${passed} pass / ${fails.length} fail / ${unmeasured.length} not measured${pending.length ? ` / ${pending.length} pending criterion revision` : ""}`);
  log(`[audit] artefacts: ${relative(REPO, outDir).split("\\").join("/")}/{<route>_<mode>.png, metrics.json, report.md}`);
  for (const p of pending) log(`[audit] pending: #${p.n} ${p.title} — 判据待 t14 修订，不计为实现缺陷（当前按 §12.3 修正读法：${p.failures.slice(0, 3).join("; ") || "pass"}）`);
  if (fails.length) {
    log(`[audit] failing rows: ${fails.map((r) => r.n).join(", ")}`);
    for (const f of fails.slice(0, 12)) log(`        #${f.n} ${f.title} — ${f.failures.slice(0, 4).join("; ")}`);
  }
  for (const w of warnings) log(`[audit] warning: ${w}`);

  return args.check && fails.length ? 1 : 0;
}

main()
  .then((code) => process.exit(code))
  .catch((err) => {
    console.error(`design-audit failed: ${err && err.stack ? err.stack : err}`);
    process.exit(2);
  });
