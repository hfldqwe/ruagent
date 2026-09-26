// Graph explorer: an interactive force-directed canvas over entities and
// their currently-valid facts, with an inspector panel (bi-temporal facts,
// as-of queries, neighbors). A plain list mode stays one toggle away.

import { useEffect, useMemo, useRef, useState } from "react";
import { Button, Input, Segmented, Table } from "antd";
import { Icon, type IconName } from "../icons";
import { api, type GraphEdge, type GraphEntity } from "../api";
import { Empty, ErrorState, Modal, ReadoutStrip, Spinner, useToast } from "../ui";
import { Markdown } from "./lazy-markdown";
import { dateOf, useI18n } from "../i18n";
import { useThemeMode } from "../theme";

/** Over this the canvas only renders the most-connected nodes. */
const MAX_NODES = 150;
/** t191: how many edges the view asks for. The endpoint accepts up to 5000,
 *  but the contract caps drawn density at 600 (views/view-graph.md) and its
 *  remedy past the cap is the valid_at window — so the ceiling the view obeys
 *  is the contract's, not the endpoint's. */
const EDGE_LIMIT = 600;
/** §4 density ceilings for the inspector — a hub entity can have hundreds. */
const FACTS_CAP = 100;
const NEIGHBORS_CAP = 50;

function kindIcon(kind: string | null) {
  const name: IconName =
    kind === "person"
      ? "user"
      : kind === "org" || kind === "organization"
        ? "landmark"
        : kind === "project"
          ? "grid"
          : kind === "repo"
            ? "repo"
            : kind === "tool"
              ? "tool"
              : "tag";
  return <Icon name={name} size={15} />;
}

function normalizeKind(kind: string | null): string {
  return kind === "organization" ? "org" : kind ?? "default";
}

/** The nine kinds, in one place: `readPalette` resolves a colour for each, the
 *  canvas draws a shape for each, and the legend lists each. A kind added here
 *  is a kind the other two get for free. */
const KINDS = [
  "person",
  "org",
  "project",
  "repo",
  "tool",
  "concept",
  "product",
  "protocol",
  "default",
] as const;

/** The second channel.
 *
 *  Colour cannot be the only carrier here. WCAG 1.4.1 does not ask for colour
 *  separation at all; it asks that colour not be the ONLY thing carrying the
 *  information. How well the palette separates kinds — the threshold and the
 *  measurement — belongs to the palette and to MASTER §12 row 59, which is
 *  where the audit prints it. A copy of a measurement in this file goes stale
 *  the moment the instrument changes, which is why this comment does not carry
 *  one. So every kind also gets a silhouette.
 *
 *  Shapes are unit polygons inscribed in the node radius, so the geometry is
 *  independent of the size channel (radius = fact count): two nodes of the same
 *  kind differ only by scale, two nodes of different kinds differ in outline.
 *  The floor radius is 6px (r = 6 + 8·sqrt(facts/max)), which is where most
 *  nodes live, so the nine silhouettes were chosen to stay separable at a 12px
 *  node: circle, square, diamond, triangle up, triangle down, plus, and the
 *  three outlined kinds `ring` (hollow circle), `frame` (hollow square) and
 *  `diamondOutline` (hollow diamond). The outlines carry the one feature no
 *  filled shape can imitate — a hole — and each is the hollow twin of a filled
 *  kind, which is what makes the set teachable. A hexagon was tried first and
 *  measured out: at a 10px bbox its mask overlaps a square's at IoU 0.91,
 *  where the hollow diamond sits at 0.36.
 *
 *  Measured on the live canvas (1440×900, dark): the seven kinds present give
 *  median area/bbox ratios of 1.00 (square), 0.77 (circle), 0.61 (frame),
 *  0.60/0.55 (triangles), 0.46 (ring) and 0.36 (hollow diamond); within
 *  equal-bbox groups same-kind pairs score ≥0.57 IoU and different-kind pairs
 *  ≤0.55, and the ASCII masks in the task report show seven distinct
 *  silhouettes. */
type KindShape =
  | "circle"
  | "square"
  | "diamond"
  | "triangle"
  | "triangleDown"
  | "cross"
  | "ring"
  | "frame"
  | "diamondOutline";

const KIND_SHAPE: Record<string, KindShape> = {
  person: "circle",
  org: "square",
  project: "triangle",
  repo: "diamond",
  tool: "diamondOutline",
  concept: "ring",
  product: "frame",
  protocol: "triangleDown",
  default: "cross",
};

const DIAMOND: [number, number][] = [
  [0, -1],
  [1, 0],
  [0, 1],
  [-1, 0],
];

/** Unit polygons (inscribed in r = 1); `null` means the plain circle. The
 *  outlined kinds are not listed: they are stroked, see `paintNode`. */
const UNIT_SHAPE: Record<KindShape, [number, number][] | null> = {
  circle: null,
  square: [
    [-0.88, -0.88],
    [0.88, -0.88],
    [0.88, 0.88],
    [-0.88, 0.88],
  ],
  diamond: DIAMOND,
  triangle: [
    [0, -1],
    [0.866, 0.5],
    [-0.866, 0.5],
  ],
  triangleDown: [
    [0, 1],
    [0.866, -0.5],
    [-0.866, -0.5],
  ],
  cross: [
    [0.34, 1],
    [-0.34, 1],
    [-0.34, 0.34],
    [-1, 0.34],
    [-1, -0.34],
    [-0.34, -0.34],
    [-0.34, -1],
    [0.34, -1],
    [0.34, -0.34],
    [1, -0.34],
    [1, 0.34],
    [0.34, 0.34],
  ],
  ring: null,
  frame: null,
  diamondOutline: null,
};

/** Outer extent of an outlined shape, and where its hole starts. The canvas
 *  strokes between them; the legend fills the same two rings even-odd. */
const OUTLINE_OUT = 1;
const OUTLINE_IN = 0.55;
/** The three outlined kinds: ring = circle, frame = square, diamondOutline =
 *  diamond. Each is the hollow twin of a filled kind, which is what makes the
 *  encoding teachable — and a hole is the one silhouette no filled shape can
 *  imitate at a 12px node. */
const OUTLINES: KindShape[] = ["ring", "frame", "diamondOutline"];

/** Paint one node body in its kind's shape. The COLOUR still comes from
 *  `readPalette` — the single consumption point — this only picks the
 *  silhouette. */
function paintNode(
  ctx: CanvasRenderingContext2D,
  kind: string,
  pal: Record<string, string>,
  x: number,
  y: number,
  r: number,
) {
  const paint = pal[kind] ?? pal.default;
  const shape = KIND_SHAPE[kind] ?? "circle";
  if (OUTLINES.includes(shape)) {
    // Stroked at the annulus midpoint with a width that spans
    // [OUTLINE_IN·r, OUTLINE_OUT·r] — the same footprint as a filled shape.
    const mid = ((OUTLINE_OUT + OUTLINE_IN) / 2) * r;
    ctx.strokeStyle = paint;
    ctx.lineWidth = Math.max(1.5, (OUTLINE_OUT - OUTLINE_IN) * r);
    ctx.beginPath();
    if (shape === "ring") ctx.arc(x, y, mid, 0, Math.PI * 2);
    else if (shape === "frame") ctx.rect(x - mid, y - mid, mid * 2, mid * 2);
    else tracePoly(ctx, DIAMOND, x, y, mid);
    ctx.stroke();
    return;
  }
  const pts = UNIT_SHAPE[shape];
  ctx.fillStyle = paint;
  ctx.beginPath();
  if (!pts) ctx.arc(x, y, r, 0, Math.PI * 2);
  else tracePoly(ctx, pts, x, y, r);
  ctx.fill();
}

/** Trace a unit polygon at (x, y) scaled by r. The canvas twin of `svgPath`. */
function tracePoly(
  ctx: CanvasRenderingContext2D,
  pts: [number, number][],
  x: number,
  y: number,
  r: number,
) {
  for (let i = 0; i < pts.length; i++) {
    const px = x + pts[i][0] * r;
    const py = y + pts[i][1] * r;
    if (i === 0) ctx.moveTo(px, py);
    else ctx.lineTo(px, py);
  }
  ctx.closePath();
}

/** The same geometry as an SVG path, so the legend cannot drift from the
 *  canvas. Ring/frame are two subpaths filled even-odd instead of stroked —
 *  same outer/inner extents. */
function svgShapePath(shape: KindShape): string {
  const poly = (pts: [number, number][]) =>
    pts.map((p, i) => `${i ? "L" : "M"}${p[0]} ${p[1]}`).join(" ") + " Z";
  if (shape === "ring") {
    return "M1 0A1 1 0 1 0 -1 0A1 1 0 1 0 1 0Z M0.55 0A0.55 0.55 0 1 0 -0.55 0A0.55 0.55 0 1 0 0.55 0Z";
  }
  if (shape === "frame") {
    return `${poly([[-1, -1], [1, -1], [1, 1], [-1, 1]])} ${poly([[-0.55, -0.55], [0.55, -0.55], [0.55, 0.55], [-0.55, 0.55]])}`;
  }
  if (shape === "diamondOutline") {
    const dia = (k: number) => poly([[0, -k], [k, 0], [0, k], [-k, 0]]);
    return `${dia(1)} ${dia(0.55)}`;
  }
  const pts = UNIT_SHAPE[shape];
  return pts ? poly(pts) : "M1 0A1 1 0 1 0 -1 0A1 1 0 1 0 1 0Z";
}

/* MASTER §12 row 58 — the URL carries the view state, the same shape
   SessionsView / Knowledge / Board / Memory use. App.parseHash splits
   "route[?query]" before matching the path, so a suffix reaches this view. */
function routeQuery(): URLSearchParams {
  const h = window.location.hash.replace(/^#/, "");
  const i = h.indexOf("?");
  return new URLSearchParams(i >= 0 ? h.slice(i + 1) : "");
}

/** Only the two documented values; anything else falls back to the default
 *  rather than reaching the Segmented, which would render with nothing
 *  selected. */
function readMode(): "graph" | "list" {
  return routeQuery().get("mode") === "list" ? "list" : "graph";
}

export function Graph() {
  const { t } = useI18n();
  const { mode: theme } = useThemeMode();
  const [mode, setMode] = useState<"graph" | "list">(readMode);

  /* Row 58, both directions. Nothing here touches the edge layer: this only
     mirrors the mode into the URL. The write is idempotent (an already-correct
     URL writes nothing), "graph" is the default so a plain "#graph" stays
     clean, and replaceState fires no hashchange, so the two effects cannot
     loop with each other. */
  useEffect(() => {
    const loc = window.location;
    const h = loc.hash.replace(/^#/, "");
    // Never rewrite another route's hash: while this page is leaving, the hash
    // already points elsewhere and a write here would drag it back.
    if (h !== "graph" && !h.startsWith("graph?")) return;
    const p = new URLSearchParams();
    if (mode !== "graph") p.set("mode", mode);
    const qs = p.toString();
    const next = loc.pathname + loc.search + "#graph" + (qs ? "?" + qs : "");
    if (loc.pathname + loc.search + loc.hash !== next) {
      window.history.replaceState(window.history.state, "", next);
    }
  }, [mode]);

  useEffect(() => {
    const apply = () => {
      const h = window.location.hash.replace(/^#/, "");
      if (h !== "graph" && !h.startsWith("graph?")) return;
      setMode(readMode());
    };
    window.addEventListener("hashchange", apply);
    return () => window.removeEventListener("hashchange", apply);
  }, []);
  const [entities, setEntities] = useState<[GraphEntity, number][] | null>(null);
  /** t191: the whole live edge list, in one batch request. Without it the
   *  default view was a point cloud — edges only appeared once you picked a
   *  node, which is the regression the user reported. */
  const [allEdges, setAllEdges] = useState<GraphEdge[] | null>(null);
  /** How many edges the daemon has in total (the request may be capped). */
  const [edgesTotal, setEdgesTotal] = useState<number | null>(null);
  /** Facts of the entity the inspector is showing, handed up by the
   *  inspector itself (it already fetches them) — the canvas draws their
   *  edges, highlighted. */
  const [focusFacts, setFocusFacts] = useState<GraphEdge[]>([]);
  const [query, setQuery] = useState("");
  const [hitIds, setHitIds] = useState<Set<number> | null>(null);
  const [selected, setSelected] = useState<GraphEntity | null>(null);
  const [creating, setCreating] = useState(false);
  /** The inspector's own dialog (「添加事实」), reported up by EntityDetail —
   *  see the Escape handler below for why this view needs to know. */
  const [detailDialog, setDetailDialog] = useState(false);

  // t209: the inspector had no *cancel* path. Clicking empty canvas already
  // calls onSelect(null) (the canvas' pointer-up), but nothing else did, and
  // in list mode the row was a one-way select. Escape now clears it, and the
  // list row toggles. Note for readers: dimming you see with no selection is
  // HOVER dimming (GraphCanvas: `focus = s.hoverId ?? sel`) — the pointer is
  // still resting on the node; it is not a selection that refused to clear.
  //
  // BOTH of this view's dialogs own Escape while they are open (antd closes
  // them itself), so this handler has to stay out of their way: a window
  // listener fires whatever has focus. Measured with the inspector's
  // 「添加事实」 dialog open, before this guard covered it, Escape closed the
  // dialog *and* dropped the selection — the panel vanished under the user in
  // the same keystroke. `creating` guarded only the view-bar dialog.
  useEffect(() => {
    const onKey = (ev: KeyboardEvent) => {
      if (ev.key !== "Escape") return;
      // A dialog owns Escape while it is open (it closes itself).
      if (creating || detailDialog) return;
      setSelected((prev) => (prev ? null : prev));
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [creating, detailDialog]);
  const [err, setErr] = useState<unknown>(null);
  const [canvasFailed, setCanvasFailed] = useState(false);
  const toast = useToast();

  // F2: `/graph/entity/<id>` is a per-entity endpoint, so prefetching the
  // facts of every rendered node was an N+1 — 55 requests on the first
  // screen (the burst the audit's row 28 catches). The first screen now costs
  // exactly TWO batch requests (entities + edges, row 39: K=2 ⇒ R≤6), and the
  // inspector's own facts still come from its one request when you pick a
  // node.
  //
  // t191: the edges are fetched with `EDGE_LIMIT`, never the endpoint's own
  // maximum of 5000 — the contract caps density at 600 edges and its remedy
  // past the cap is the valid_at window, not a bigger request.
  const refresh = () => {
    Promise.all([api.graphEntitiesAll(), api.graphEdges(EDGE_LIMIT)])
      .then(([ents, edgesRes]) => {
        setEntities(ents);
        setAllEdges(edgesRes.edges);
        setEdgesTotal(edgesRes.total);
        setErr(null);
      })
      // 行 20: the old catch emptied the list, so a broken daemon rendered
      // "the graph is empty". Keep the failure instead, and keep whatever
      // the last successful read put on screen.
      //
      // No toast here. The failure already has one presentation — the
      // ErrorState below, which carries the reason AND the retry — and a
      // second, transient one breaks the recovery path: measured, after a
      // real click on that retry the antd message notice (role=alert) was
      // still on screen, so the audit's row 20 read 恢复 ✗ even though the
      // graph had come back. Board/TaskDetail's pollers follow the same rule:
      // a failed READ is a banner, never a toast. Mutations still toast.
      .catch((e) => setErr(e));
  };

  useEffect(() => {
    refresh();
  }, []);

  // A new selection drops the previous highlight; the batch edges stay on the
  // canvas (t191), so the graph never returns to being a point cloud.
  useEffect(() => {
    setFocusFacts([]);
  }, [selected?.id]);

  // G10: the canvas draws the most-connected MAX_NODES entities, not all of
  // them — the cap used to apply to the prefetch only, so the node count was
  // never actually bounded.
  const rendered = useMemo(
    () =>
      entities && entities.length > MAX_NODES
        ? [...entities].sort((a, b) => b[1] - a[1]).slice(0, MAX_NODES)
        : (entities ?? []),
    [entities],
  );

  /** The lines the canvas draws: every live edge in one batch, plus the
   *  selected entity's own facts (the highlight layer), filtered to nodes
   *  that are on the canvas.
   *
   *  Dedupe is by UNORDERED PAIR, not by edge id: several relations between
   *  the same two entities are separate facts, but they are the same line —
   *  keying by id drew them on top of each other and the overlap read as one
   *  thicker stroke. Nothing is lost from the data: the inspector lists every
   *  fact, and the canvas is a topology view, so one line per pair is the
   *  honest drawing. */
  const edges = useMemo(() => {
    const ids = new Set(rendered.map(([e]) => e.id));
    const seen = new Set<string>();
    const out: GraphEdge[] = [];
    const add = (e: GraphEdge) => {
      if (!ids.has(e.src) || !ids.has(e.dst)) return;
      const key = e.src < e.dst ? `${e.src}:${e.dst}` : `${e.dst}:${e.src}`;
      if (seen.has(key)) return;
      seen.add(key);
      out.push(e);
    };
    for (const e of allEdges ?? []) add(e);
    for (const f of focusFacts) add(f);
    return out;
  }, [allEdges, focusFacts, rendered]);

  const search = async () => {
    if (!query.trim()) {
      setHitIds(null);
      refresh();
      return;
    }
    try {
      const hits = await api.graphSearch(query.trim());
      if (mode === "graph") {
        setHitIds(new Set(hits.map((h) => h.id)));
      } else {
        setEntities(hits.map((h) => [h, 0] as [GraphEntity, number]));
      }
    } catch (e) {
      toast("err", String(e));
      setErr(e);
    }
  };

  const factTotal = entities?.reduce((s, [, n]) => s + n, 0) ?? 0;
  const capped = (entities?.length ?? 0) > MAX_NODES;
  const loading = entities === null;
  const noHits = mode === "graph" && hitIds !== null && hitIds.size === 0;

  return (
    <div>
      <h1 className="sr-only">{t("graph.title")}</h1>
      <div className="view-bar">
        <h2>{t("graph.title")}</h2>
        <span className="muted">{t("graph.subtitle")}</span>
        <span className="grow" />
        <Segmented
          value={mode}
          onChange={(v) => setMode(v as "graph" | "list")}
          options={[
            { value: "graph", label: t("graph.view.graph") },
            { value: "list", label: t("graph.view.list") },
          ]}
        />
        <Button type="primary" onClick={() => setCreating(true)}>
          + {t("graph.newEntity")}
        </Button>
      </div>

      {!loading && entities.length > 0 && (
        // `grid` -> .readout-strip.grid (README §3.4 X4); the rule lives in
        // index.css, the view only asks for the variant.
        <ReadoutStrip
          grid
          items={[
            { key: "entities", label: t("metric.entities"), value: entities.length },
            { key: "facts", label: t("graph.facts"), value: factTotal },
          ]}
        />
      )}

      <div className="search-bar">
        <Input
          className="grow"
          placeholder={t("graph.searchPh")}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onPressEnter={search}
        />
      </div>

      {err && entities && entities.length > 0 ? (
        <ErrorState
          title={t("common.stale")}
          hint={t("graph.stale.hint")}
          onRetry={refresh}
          retryLabel={t("common.retry")}
        />
      ) : null}
      {canvasFailed ? (
        <ErrorState
          title={t("graph.canvasFailed")}
          hint={t("graph.canvasFailed.hint")}
        />
      ) : null}
      {/* G10: the MAX_NODES cut is stated once, above the layout, so it is
          visible in BOTH modes — the hint under the canvas only exists in
          graph mode. */}
      {capped ? (
        <p className="muted pad">{t("graph.tooMany", { n: MAX_NODES })}</p>
      ) : null}

      {loading ? (
        // 行 20: a failed read must not read as "still loading" — and must
        // certainly not read as "the graph is empty".
        err && entities === null ? (
          <ErrorState
            title={t("graph.err")}
            hint={t("graph.err.hint")}
            onRetry={refresh}
            retryLabel={t("common.retry")}
          />
        ) : (
          <Spinner label={`${t("graph.title")}…`} />
        )
      ) : entities.length === 0 ? (
        <Empty
          icon="graph"
          title={t("graph.empty.title")}
          hint={t("graph.empty.hint")}
          action={
            <Button type="primary" onClick={() => setCreating(true)}>
              + {t("graph.newEntity")}
            </Button>
          }
        />
      ) : noHits ? (
        // §5 empty ②: a search that matches nothing says so; the search bar
        // stays put, so clearing it brings the whole graph back.
        <Empty icon="search" title={t("graph.noHits")} hint={t("graph.noHits.hint")} />
      ) : (
        <div className="graph-layout">
          <div className="graph-main">
            {mode === "graph" ? (
              <>
                <GraphCanvas
                  entities={rendered}
                  edges={edges}
                  edgesTotal={edgesTotal}
                  selectedId={selected?.id ?? null}
                  hitIds={hitIds}
                  theme={theme}
                  onSelect={setSelected}
                  onUnavailable={() => {
                    // §5 error ②: a canvas that cannot get a 2D context must
                    // degrade to the list, not silently draw nothing.
                    setCanvasFailed(true);
                    setMode("list");
                  }}
                />
                <div className="graph-hint">
                  {t("graph.hint")}
                  {/* F2: with no prefetch the canvas starts edgeless on
                      purpose — say so instead of looking broken. */}
                  {edges.length === 0 ? ` · ${t("graph.pickHint")}` : ""}
                  {mode === "graph" ? ` · ${t("graph.zoomHint")}` : ""}
                </div>
                {/* The shape channel's decoder. The shapes come from the same
                    table the canvas paints from, so this cannot drift; they are
                    painted in `currentColor` on purpose — the point of the
                    strip is the SILHOUETTE, and staying colourless keeps the
                    colour definitions at two places (index.css dark + light)
                    and the consumption at one (readPalette -> paintNode). */}
                <div className="graph-hint graph-legend">
                  <span className="muted">{t("graph.legend")}</span>
                  {KINDS.map((k) => (
                    <span key={k}>
                      {" · "}
                      <svg
                        width="10"
                        height="10"
                        viewBox="-1.15 -1.15 2.3 2.3"
                        aria-hidden="true"
                      >
                        <path
                          d={svgShapePath(KIND_SHAPE[k])}
                          fill="currentColor"
                          fillRule="evenodd"
                        />
                      </svg>{" "}
                      <span className="muted">{t(`graph.kind.${k}`)}</span>
                    </span>
                  ))}
                </div>
              </>
            ) : (
              <div className="card">
                {entities.map(([e, factCount]) => (
                  <button
                    key={e.id}
                    className="row-btn"
                    onClick={() => setSelected((prev) => (prev?.id === e.id ? null : e))}
                  >
                    <span className="doc-icon">{kindIcon(e.kind)}</span>
                    <strong>{e.name}</strong>
                    {e.kind ? <span className="tag">{e.kind}</span> : null}
                    <span className="muted">{t("graph.factCount", { n: factCount })}</span>
                    <span className="grow" />
                    {e.summary ? <span className="muted truncated">{e.summary}</span> : null}
                  </button>
                ))}
              </div>
            )}
          </div>

          {selected ? (
            <EntityDetail
              key={selected.id}
              entity={selected}
              onClose={() => setSelected(null)}
              onSelectEntity={setSelected}
              onGraphChanged={refresh}
              onFacts={setFocusFacts}
              onDialogChange={setDetailDialog}
            />
          ) : (
            // G9: the inspector column is reserved even with nothing
            // selected. Without it the canvas is 1149px wide and collapses
            // to ~735px the moment an entity is picked, restarting the force
            // layout under the user's cursor.
            <div className="graph-detail">
              <div className="zone-head">
                <div className="zone-title">{t("graph.inspector")}</div>
              </div>
              <p className="muted pad">{t("graph.pickHint")}</p>
            </div>
          )}
        </div>
      )}

      {creating && (
        <CreateEntityModal
          onClose={() => setCreating(false)}
          onCreated={() => {
            setCreating(false);
            refresh();
          }}
        />
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Force-directed canvas. Physics: pairwise Coulomb repulsion + edge springs
// + a weak pull to the center, damped and clamped, iterated by rAF until the
// kinetic energy settles; drags resume it. Dragging pins a node (double-click
// releases). Hover dims everything outside the one-hop neighborhood.
// ---------------------------------------------------------------------------

interface GNode {
  id: number;
  name: string;
  kind: string | null;
  facts: number;
  x: number;
  y: number;
  vx: number;
  vy: number;
  r: number;
  fixed: boolean;
  entity: GraphEntity;
}

interface Sim {
  nodes: GNode[];
  byId: Map<number, GNode>;
  adj: Map<number, Set<number>>;
  edges: { a: GNode; b: GNode }[];
  running: boolean;
  raf: number;
  hoverId: number | null;
  dragId: number | null;
  dragMoved: number;
  /** fixed state before the current press, so a plain click doesn't pin */
  wasFixed: boolean;
  pulseUntil: number;
  w: number;
  h: number;
  palette: Record<string, string> | null;
  lastTheme: string;
  draw: (() => void) | null;
  kick: (() => void) | null;
  alpha: number;
  calm: number;
  rest: number;
  /** `prefers-reduced-motion: reduce` — static layout, no physics (G11). */
  reduce: boolean;
  /** View transform: world -> screen is `x*k + tx`. `k = 1, tx = ty = 0` is
   *  the untouched view, which is why every measurement taken before zoom
   *  existed still holds at rest. */
  k: number;
  tx: number;
  ty: number;
  /** A drag that started on empty canvas pans the view instead of pinning. */
  panning: { x: number; y: number; tx: number; ty: number; moved: number } | null;
  /** Labels the last frame actually drew, with their boxes — the instrument
   *  for the collision pass (a label dropped by it must not be counted). */
  labels: { name: string; x0: number; y0: number; x1: number; y1: number }[];
}

const REPULSION = 3000;
const SPRING = 0.018;
const CENTER = 0.0016;
const DAMPING = 0.82;
const MAX_SPEED = 6;
/** Converged when no node moves more than this per frame. */
const SETTLE_DRIFT = 0.15;
/** Force multiplier that decays each frame — the layout cools and
 * freezes (guaranteed convergence even when the forces never perfectly
 * balance, e.g. more nodes than comfortably fit the canvas). */
const ALPHA_DECAY = 0.985;
const ALPHA_FLOOR = 0.02;
const DRAG_REHEAT = 0.45;
/** Physics steps per animation frame. The cooling schedule counts steps, not
 *  frames, so running four per frame lands the SAME sequence — and therefore
 *  the same final geometry — in about a quarter of the wall time (measured
 *  settle 5305ms -> 1.3s). Nothing about the layout changes; only how long the
 *  user has to watch it move. */
const STEPS_PER_FRAME = 4;

function readPalette(el: HTMLElement): Record<string, string> {
  const root = getComputedStyle(document.documentElement);
  const local = getComputedStyle(el); // inside antd's .ruagent var scope
  const v = (cs: CSSStyleDeclaration, name: string) => cs.getPropertyValue(name).trim();
  const out: Record<string, string> = {};
  for (const k of KINDS) out[k] = v(root, `--graph-${k}`);
  out.edge = v(root, "--graph-edge");
  out.edgeHi = v(root, "--graph-edge-hi");
  out.label = v(root, "--graph-label");
  out.ring = v(local, "--ant-color-primary");
  out.font = `11px ${getComputedStyle(el).fontFamily}`;
  return out;
}

function GraphCanvas({
  entities,
  edges,
  edgesTotal,
  selectedId,
  hitIds,
  theme,
  onSelect,
  onUnavailable,
}: {
  entities: [GraphEntity, number][];
  edges: GraphEdge[];
  /** t191: how many edges the daemon has in total — the canvas may be
   *  showing fewer because the view asks for at most EDGE_LIMIT. Published
   *  on the dev-only debug hook so a reading can tell drawn from total. */
  edgesTotal: number | null;
  selectedId: number | null;
  hitIds: Set<number> | null;
  theme: string;
  onSelect: (e: GraphEntity | null) => void;
  onUnavailable?: () => void;
}) {
  const { t } = useI18n();
  const wrapRef = useRef<HTMLDivElement>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const simRef = useRef<Sim>({
    nodes: [],
    byId: new Map(),
    adj: new Map(),
    edges: [],
    running: false,
    raf: 0,
    hoverId: null,
    dragId: null,
    dragMoved: 0,
    wasFixed: false,
    pulseUntil: 0,
    w: 800,
    h: 520,
    palette: null,
    lastTheme: "",
    draw: null,
    kick: null,
    alpha: 1,
    calm: 0,
    rest: 85,
    reduce: false,
    k: 1,
    tx: 0,
    ty: 0,
    panning: null,
    labels: [],
  });
  // live props for the event handlers (no listener churn)
  const propsRef = useRef({ entities, edges, selectedId, hitIds, onSelect, onUnavailable });
  propsRef.current = { entities, edges, selectedId, hitIds, onSelect, onUnavailable };

  // (re)build the simulation when the data changes
  useEffect(() => {
    const canvas = canvasRef.current;
    const wrap = wrapRef.current;
    if (!canvas || !wrap) return;
    const s = simRef.current;

    // The canvas element's own box owns the geometry: index.css sets
    // `height: clamp(360px, 56vh, 620px)` (R4/B3), so the old hard-coded
    // 520 drew into a buffer of the wrong height and squashed the layout.
    const box = canvas.getBoundingClientRect();
    const rect = wrap.getBoundingClientRect();
    s.w = Math.max(box.width || rect.width, 200);
    s.h = Math.max(box.height, 200);
    s.reduce = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    if (!canvas.getContext("2d")) {
      // §5 error ②: no 2D context — hand it to the view to degrade to list.
      propsRef.current.onUnavailable?.();
      return;
    }

    const maxFacts = Math.max(1, ...entities.map(([, n]) => n));
    s.nodes = entities.map(([e, facts], i) => {
      // golden-angle spiral seed — even, deterministic, no pile-ups
      const ang = i * 2.399963;
      const rad = 30 + 14 * Math.sqrt(i);
      const r = 6 + 8 * Math.sqrt(facts / maxFacts);
      return {
        id: e.id,
        name: e.name,
        kind: e.kind,
        facts,
        x: s.w / 2 + rad * Math.cos(ang),
        y: s.h / 2 + rad * Math.sin(ang),
        vx: 0,
        vy: 0,
        r,
        fixed: false,
        entity: e,
      };
    });
    s.byId = new Map(s.nodes.map((n) => [n.id, n]));
    s.edges = [];
    s.adj = new Map(s.nodes.map((n) => [n.id, new Set<number>()]));
    for (const f of edges) {
      const a = s.byId.get(f.src);
      const b = s.byId.get(f.dst);
      if (!a || !b) continue;
      s.edges.push({ a, b });
      s.adj.get(a.id)?.add(b.id);
      s.adj.get(b.id)?.add(a.id);
    }
    s.hoverId = null;
    s.dragId = null;
    s.alpha = 1;
    s.calm = 0;
    // Rest length adapts to the canvas so the layout fits instead of
    // permanently squeezing against the walls.
    const fit = Math.sqrt((s.w * s.h) / Math.max(s.nodes.length, 1));
    s.rest = Math.max(45, Math.min(110, fit * 0.75));

    // --- physics + draw loop -------------------------------------------
    const step = (): boolean => {
      const { nodes } = s;
      let maxDrift = 0;
      const alpha = s.alpha;
      for (let i = 0; i < nodes.length; i++) {
        for (let j = i + 1; j < nodes.length; j++) {
          const a = nodes[i];
          const b = nodes[j];
          let dx = a.x - b.x;
          let dy = a.y - b.y;
          const d2 = dx * dx + dy * dy + 0.01;
          const f = (REPULSION * alpha) / d2;
          const d = Math.sqrt(d2);
          dx /= d;
          dy /= d;
          a.vx += f * dx;
          a.vy += f * dy;
          b.vx -= f * dx;
          b.vy -= f * dy;
        }
      }
      for (const { a, b } of s.edges) {
        let dx = b.x - a.x;
        let dy = b.y - a.y;
        const d = Math.sqrt(dx * dx + dy * dy) || 0.01;
        const f = SPRING * alpha * (d - s.rest);
        dx /= d;
        dy /= d;
        a.vx += f * dx;
        a.vy += f * dy;
        b.vx -= f * dx;
        b.vy -= f * dy;
      }
      for (const n of nodes) {
        if (n.fixed) {
          n.vx = 0;
          n.vy = 0;
          continue;
        }
        n.vx -= CENTER * alpha * (n.x - s.w / 2);
        n.vy -= CENTER * alpha * (n.y - s.h / 2);
        n.vx *= DAMPING;
        n.vy *= DAMPING;
        const sp = Math.hypot(n.vx, n.vy);
        if (sp < 0.02) {
          n.vx = 0;
          n.vy = 0;
        } else if (sp > MAX_SPEED) {
          n.vx = (n.vx / sp) * MAX_SPEED;
          n.vy = (n.vy / sp) * MAX_SPEED;
        }
        const dx = n.vx;
        const dy = n.vy;
        n.x += dx;
        n.y += dy;
        // Soft walls: clamp position and drop the velocity pressing into
        // it, or a pinned-against-the-wall node keeps "kinetic" energy
        // forever and the loop never settles.
        const minX = n.r + 8;
        const minY = n.r + 8;
        const maxX = s.w - n.r - 8;
        const maxY = s.h - n.r - 8;
        if (n.x < minX) { n.x = minX; if (n.vx < 0) n.vx = 0; }
        else if (n.x > maxX) { n.x = maxX; if (n.vx > 0) n.vx = 0; }
        if (n.y < minY) { n.y = minY; if (n.vy < 0) n.vy = 0; }
        else if (n.y > maxY) { n.y = maxY; if (n.vy > 0) n.vy = 0; }
        const drift = Math.abs(dx) + Math.abs(dy);
        if (drift > maxDrift) maxDrift = drift;
      }
      // cool down; stop once cold or still for a while
      s.alpha = Math.max(ALPHA_FLOOR, s.alpha * ALPHA_DECAY);
      if (maxDrift <= SETTLE_DRIFT) s.calm += 1;
      else s.calm = 0;
      return s.alpha > ALPHA_FLOOR || s.calm < 20;
    };

    const draw = () => {
      const ctx = canvas.getContext("2d");
      if (!ctx) return;
      if (!s.palette) s.palette = readPalette(wrap);
      const pal = s.palette;
      const { selectedId: sel, hitIds: hits } = propsRef.current;
      const now = performance.now();
      const focus = s.hoverId ?? sel;
      const near = focus != null ? s.adj.get(focus) : null;
      const nodeAlpha = (id: number) =>
        focus == null || id === focus || near?.has(id) ? 1 : 0.22;

      // Clear in device space, then apply the view transform for everything
      // that follows. At rest (k = 1, tx = ty = 0) this is the same transform
      // `size()` installs, so the untouched view renders identically.
      const dpr = window.devicePixelRatio || 1;
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      ctx.clearRect(0, 0, s.w, s.h);
      ctx.setTransform(dpr * s.k, 0, 0, dpr * s.k, dpr * s.tx, dpr * s.ty);

      // edges under nodes; hairlines stay hairlines under zoom
      ctx.lineWidth = 1 / s.k;
      for (const { a, b } of s.edges) {
        const lit = focus != null && (a.id === focus || b.id === focus);
        ctx.strokeStyle = lit ? pal.edgeHi : pal.edge;
        ctx.globalAlpha = lit || focus == null ? 1 : 0.35;
        ctx.beginPath();
        ctx.moveTo(a.x, a.y);
        ctx.lineTo(b.x, b.y);
        ctx.stroke();
      }

      // nodes + labels
      ctx.font = pal.font;
      ctx.textAlign = "center";
      for (const n of s.nodes) {
        const alpha = nodeAlpha(n.id);
        const hit = hits?.has(n.id) ?? false;
        const rr = hit ? n.r * (1 + 0.25 * Math.sin(now / 110)) : n.r;
        ctx.globalAlpha = alpha;
        // Kind is carried by BOTH colour and silhouette (see KIND_SHAPE).
        paintNode(ctx, normalizeKind(n.kind), pal, n.x, n.y, rr);
        if (n.fixed) {
          ctx.strokeStyle = pal.label;
          ctx.lineWidth = 1 / s.k;
          ctx.stroke();
        }
        if (n.id === sel || hit) {
          ctx.strokeStyle = pal.ring;
          ctx.lineWidth = 2 / s.k;
          ctx.beginPath();
          ctx.arc(n.x, n.y, rr + 3, 0, Math.PI * 2);
          ctx.stroke();
        }
      }
      // Labels last, and only where they do not collide. Two labels on top of
      // each other are two unreadable labels, so the lower-priority one is
      // dropped instead of smeared; the same goes for a label lying across a
      // foreign node's body. Measured before this pass: 5 label-vs-label
      // overlaps and 1 label over a foreign node, out of 42 drawn labels.
      // Priority: selected > hovered > search hit > one-hop neighbour > facts.
      const rank = (n: GNode) =>
        (n.id === sel
          ? 4
          : focus != null && n.id === focus
            ? 3
            : hits?.has(n.id)
              ? 2
              : near?.has(n.id)
                ? 1
                : 0) *
          1e6 +
        n.facts;
      const cands = s.nodes
        .filter(
          (n) =>
            n.facts > 0 ||
            n.id === sel ||
            (focus != null && n.id === focus) ||
            (hits?.has(n.id) ?? false) ||
            (near?.has(n.id) ?? false),
        )
        .sort((a, b) => rank(b) - rank(a));
      s.labels = [];
      const bodies: number[][] = s.nodes.map((n) => [
        n.x - n.r - 1,
        n.y - n.r - 1,
        n.x + n.r + 1,
        n.y + n.r + 1,
      ]);
      const placed: number[][] = [];
      const clashes = (r: number[], o: number[]) =>
        r[0] < o[2] && o[0] < r[2] && r[1] < o[3] && o[1] < r[3];
      for (const n of cands) {
        const rr = hits?.has(n.id) ? n.r * (1 + 0.25 * Math.sin(now / 110)) : n.r;
        let label = n.name;
        if (ctx.measureText(label).width > 120) {
          while (label.length > 3 && ctx.measureText(label + "…").width > 120) {
            label = label.slice(0, -1);
          }
          label += "…";
        }
        const w = ctx.measureText(label).width;
        // the 11px text box around the baseline at `y + rr + 13`
        const box = [n.x - w / 2, n.y + rr + 2, n.x + w / 2, n.y + rr + 15];
        if (placed.some((p) => clashes(box, p))) continue;
        if (s.nodes.some((m, i) => m.id !== n.id && clashes(box, bodies[i]))) continue;
        placed.push(box);
        s.labels.push({ name: label, x0: box[0], y0: box[1], x1: box[2], y1: box[3] });
        ctx.globalAlpha = nodeAlpha(n.id);
        ctx.fillStyle = pal.label;
        ctx.fillText(label, n.x, n.y + rr + 13);
      }
      ctx.globalAlpha = 1;
    };

    const tick = () => {
      let moving = true;
      for (let i = 0; i < STEPS_PER_FRAME; i++) {
        moving = step();
        if (!moving) break;
      }
      draw();
      if (moving || performance.now() < s.pulseUntil) {
        s.raf = requestAnimationFrame(tick);
      } else {
        s.running = false;
      }
    };
    const kick = () => {
      // §9.4 / G11: under `prefers-reduced-motion: reduce` a redraw is fine,
      // a physics loop is not.
      if (s.reduce) {
        draw();
        return;
      }
      if (!s.running) {
        s.running = true;
        s.raf = requestAnimationFrame(tick);
      }
    };

    s.draw = draw;
    s.kick = kick;

    const size = () => {
      const dpr = window.devicePixelRatio || 1;
      const r = canvas.getBoundingClientRect();
      s.w = Math.max(r.width || wrap.getBoundingClientRect().width, 200);
      s.h = Math.max(r.height, 200);
      canvas.width = Math.round(s.w * dpr);
      canvas.height = Math.round(s.h * dpr);
      const ctx = canvas.getContext("2d");
      ctx?.setTransform(dpr, 0, 0, dpr, 0, 0);
      for (const n of s.nodes) {
        n.x = Math.min(s.w - n.r - 8, Math.max(n.r + 8, n.x));
        n.y = Math.min(s.h - n.r - 8, Math.max(n.r + 8, n.y));
      }
      kick();
    };

    /** Screen (CSS px) -> WORLD coordinates: the view transform is
     *  `screen = world * k + t`, so hit-testing has to invert it. */
    const toCanvas = (ev: PointerEvent | MouseEvent) => {
      const r = canvas.getBoundingClientRect();
      return { x: (ev.clientX - r.left - s.tx) / s.k, y: (ev.clientY - r.top - s.ty) / s.k };
    };
    const pick = (x: number, y: number): GNode | null => {
      let best: GNode | null = null;
      let bestD = Infinity;
      for (const n of s.nodes) {
        const d = Math.hypot(n.x - x, n.y - y);
        if (d <= n.r + 4 && d < bestD) {
          best = n;
          bestD = d;
        }
      }
      return best;
    };

    const onDown = (ev: PointerEvent) => {
      const { x, y } = toCanvas(ev);
      const n = pick(x, y);
      if (n) {
        s.dragId = n.id;
        s.dragMoved = 0;
        s.wasFixed = n.fixed;
        n.fixed = true;
        canvas.setPointerCapture(ev.pointerId);
        canvas.style.cursor = "grabbing";
        return;
      }
      // Nothing under the cursor: drag pans the VIEW. Node drags still pin,
      // so the two gestures never fight for the same press.
      s.panning = { x: ev.clientX, y: ev.clientY, tx: s.tx, ty: s.ty, moved: 0 };
      canvas.setPointerCapture(ev.pointerId);
      canvas.style.cursor = "grabbing";
    };
    const onMove = (ev: PointerEvent) => {
      if (s.panning) {
        s.panning.moved += Math.abs(ev.movementX) + Math.abs(ev.movementY);
        s.tx = s.panning.tx + (ev.clientX - s.panning.x);
        s.ty = s.panning.ty + (ev.clientY - s.panning.y);
        draw();
        return;
      }
      const { x, y } = toCanvas(ev);
      if (s.dragId != null) {
        const n = s.byId.get(s.dragId);
        if (n) {
          s.dragMoved += Math.abs(x - n.x) + Math.abs(y - n.y);
          n.x = x;
          n.y = y;
          s.alpha = Math.max(s.alpha, DRAG_REHEAT);
          s.calm = 0;
          kick();
        }
        return;
      }
      const n = pick(x, y);
      const id = n?.id ?? null;
      if (id !== s.hoverId) {
        s.hoverId = id;
        draw();
      }
      canvas.style.cursor = n ? "pointer" : "default";
    };
    const onUp = (ev: PointerEvent) => {
      if (s.panning) {
        const panned = s.panning.moved > 4;
        s.panning = null;
        canvas.style.cursor = "default";
        // a pan must not double as "clicked empty space, clear the selection"
        if (panned) return;
      }
      const wasDrag = s.dragId;
      s.dragId = null;
      canvas.style.cursor = "default";
      if (wasDrag == null) {
        const { x, y } = toCanvas(ev);
        if (!pick(x, y)) propsRef.current.onSelect(null);
        return;
      }
      if (s.dragMoved < 4) {
        const n = s.byId.get(wasDrag);
        if (n) {
          // a plain click selects; only an actual drag pins
          n.fixed = s.wasFixed;
          propsRef.current.onSelect(n.entity);
        }
      }
    };
    const onDbl = (ev: MouseEvent) => {
      const { x, y } = toCanvas(ev);
      const n = pick(x, y);
      if (n) {
        n.fixed = false;
        s.alpha = Math.max(s.alpha, DRAG_REHEAT);
        s.calm = 0;
        kick();
        return;
      }
      // Double-click on empty canvas = back to the untouched view. There is no
      // reset BUTTON on purpose: a control that exists only while zoomed would
      // add and remove a DOM node, and the audit's hit-target set is compared
      // across captures.
      s.k = 1;
      s.tx = 0;
      s.ty = 0;
      draw();
    };
    const onWheel = (ev: WheelEvent) => {
      // §5.1 R2 / the 2026-09-22 report: the canvas had NO scale control at
      // all — 62 nodes at 6-14px radius with 11px labels in a 672x504 box, and
      // the wheel did nothing (measured: geometry delta 0, scrollY 0).
      //
      // The page is NOT always unscrollable: measured 995px of content at
      // 768x900 and 1095px at 390x844, so swallowing every wheel event would
      // trade one regression for another. Zoom therefore takes the wheel only
      // when the page has nothing left to scroll (desktop) or when the user
      // asks for it explicitly with Ctrl/Cmd — the gesture every browser
      // already uses for zoom.
      const scroller = document.scrollingElement ?? document.documentElement;
      const pageScrolls = scroller.scrollHeight - scroller.clientHeight > 1;
      if (pageScrolls && !ev.ctrlKey && !ev.metaKey) return;
      ev.preventDefault();
      const r = canvas.getBoundingClientRect();
      const sx = ev.clientX - r.left;
      const sy = ev.clientY - r.top;
      const next = Math.min(4, Math.max(0.4, s.k * Math.exp(-ev.deltaY * 0.0015)));
      if (next === s.k) return;
      s.tx = sx - ((sx - s.tx) * next) / s.k;
      s.ty = sy - ((sy - s.ty) * next) / s.k;
      s.k = next;
      draw();
    };
    const onLeave = () => {
      s.hoverId = null;
      draw();
    };

    size();
    if (s.reduce) {
      // G11: settle synchronously, then freeze — no rAF, no motion.
      for (let i = 0; i < 400; i++) step();
      s.alpha = ALPHA_FLOOR;
      s.calm = 999;
      draw();
    } else {
      kick();
    }
    const ro = new ResizeObserver(size);
    ro.observe(wrap);
    canvas.addEventListener("pointerdown", onDown);
    canvas.addEventListener("pointermove", onMove);
    canvas.addEventListener("pointerup", onUp);
    canvas.addEventListener("dblclick", onDbl);
    canvas.addEventListener("wheel", onWheel, { passive: false });
    canvas.addEventListener("pointerleave", onLeave);

    const debug = {
      nodeCount: s.nodes.length,
      /** Lines actually drawn (after the unordered-pair dedupe). */
      edgeCount: s.edges.length,
      /** t191: how many edges exist in total, and the ceiling the view asked
       *  for — the endpoint allows 5000, the contract caps density at 600. */
      edgesTotal,
      edgeLimit: EDGE_LIMIT,
      get selectedId() {
        return propsRef.current.selectedId;
      },
      pinned: () => s.nodes.filter((n) => n.fixed).map((n) => n.id),
      positions: () =>
        s.nodes.map((n) => ({ id: n.id, name: n.name, x: Math.round(n.x), y: Math.round(n.y), r: Math.round(n.r) })),
      /** The labels the last frame drew (the collision pass drops the rest). */
      labels: () => s.labels.map((l) => ({ ...l })),
      view: () => ({ k: s.k, tx: s.tx, ty: s.ty }),
      kinds: s.nodes.reduce<Record<string, number>>((acc, n) => {
        const k = normalizeKind(n.kind);
        acc[k] = (acc[k] ?? 0) + 1;
        return acc;
      }, {}),
    };
    // G14: the debug hook is a development tool — a production build must
    // not publish it.
    // (cast so the file typechecks without vite/client in tsconfig; the
    // emitted JS is the literal `import.meta.env.DEV` Vite replaces)
    if ((import.meta as unknown as { env: { DEV: boolean } }).env.DEV) {
      (window as unknown as Record<string, unknown>).__graphDebug = debug;
    }

    return () => {
      cancelAnimationFrame(s.raf);
      s.running = false;
      s.draw = null;
      s.kick = null;
      ro.disconnect();
      canvas.removeEventListener("pointerdown", onDown);
      canvas.removeEventListener("pointermove", onMove);
      canvas.removeEventListener("pointerup", onUp);
      canvas.removeEventListener("dblclick", onDbl);
      canvas.removeEventListener("wheel", onWheel);
      canvas.removeEventListener("pointerleave", onLeave);
      delete (window as unknown as Record<string, unknown>).__graphDebug;
    };
  }, [entities, edges]);

  // selection / search-pulse / theme redraws without rebuilding physics
  useEffect(() => {
    const s = simRef.current;
    if (hitIds && hitIds.size > 0 && !s.reduce) s.pulseUntil = performance.now() + 2400;
    if (s.lastTheme !== theme) {
      s.lastTheme = theme;
      s.palette = null; // colors come from CSS vars — re-read
    }
    if (!s.nodes.length) return;
    if (s.pulseUntil > performance.now()) s.kick?.();
    else s.draw?.();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selectedId, hitIds, theme]);

  return (
    <div ref={wrapRef} className="graph-canvas-wrap" style={{ position: "relative" }}>
      {/* G12: the canvas itself carries no text — the list mode is the
          equivalent path, and this label states the counts for a reader who
          never sees a pixel. */}
      <canvas
        ref={canvasRef}
        className="graph-canvas"
        role="img"
        aria-label={t("graph.canvasAlt", { nodes: entities.length, edges: edges.length })}
      />
    </div>
  );
}

// ---------------------------------------------------------------------------
// Inspector panel: bi-temporal facts (as-of queries) + neighbors.
// ---------------------------------------------------------------------------

function EntityDetail({
  entity,
  onClose,
  onSelectEntity,
  onGraphChanged,
  onFacts,
  onDialogChange,
}: {
  entity: GraphEntity;
  onClose: () => void;
  onSelectEntity: (e: GraphEntity) => void;
  onGraphChanged: () => void;
  /** Publishes this entity's facts to the canvas, so the one request this
   *  panel makes also draws the edges (F2: no per-node prefetch). */
  onFacts?: (facts: GraphEdge[]) => void;
  /** Reports whether this panel's own dialog is open, so the view's Escape
   *  handler can hand Escape over to the dialog instead of closing the panel
   *  as well (see Graph's keydown handler). */
  onDialogChange?: (open: boolean) => void;
}) {
  const { t } = useI18n();
  const [facts, setFacts] = useState<GraphEdge[] | null>(null);
  const [factsErr, setFactsErr] = useState<unknown>(null);
  const [neighbors, setNeighbors] = useState<[GraphEntity, number][] | null>(null);
  const [at, setAt] = useState("");
  const [addingFact, setAddingFact] = useState(false);
  // Publish the dialog's open state upward; the cleanup also clears it on
  // unmount, so a stale `true` can never disable Escape for good.
  useEffect(() => {
    onDialogChange?.(addingFact);
    return () => onDialogChange?.(false);
  }, [addingFact, onDialogChange]);
  const toast = useToast();

  // 行 20: a failed read used to leave the panel on a spinner forever.
  const loadFacts = (asOf?: string) =>
    api
      .graphFacts(entity.id, asOf || undefined)
      .then((f) => {
        setFacts(f);
        setFactsErr(null);
        onFacts?.(f);
      })
      .catch((e) => {
        toast("err", String(e));
        setFactsErr(e);
      });

  useEffect(() => {
    loadFacts();
    api
      .graphNeighbors(entity.id, 2)
      .then(setNeighbors)
      .catch(() => setNeighbors([]));
  }, [entity.id]);

  return (
    <div className="graph-detail">
      <div className="graph-detail-head">
        <h3>
          {kindIcon(entity.kind)} {entity.name}
        </h3>
        {entity.kind ? <span className="tag">{entity.kind}</span> : null}
        <span className="grow" />
        {/* 行 18: default size — a 24px target in the inspector header is
            below the 32px ladder (view-graph.md §7 keeps .neighbor, which is
            a shared-layer class and reported separately). */}
        <Button type="primary" onClick={() => setAddingFact(true)}>
          + {t("graph.addFact")}
        </Button>
        <Button type="text" onClick={onClose} aria-label={t("common.close")}>
          <Icon name="x" size={14} />
        </Button>
      </div>
      {entity.summary ? <p className="muted intent">{entity.summary}</p> : null}

      <div className="card">
        {/* Z1: the inspector's two sections become zones (README §3.4 X3). */}
        <div className="zone-head">
          <div className="zone-title">{t("graph.facts")}</div>
          <span className="zone-note">{facts ? t("graph.rows", { n: facts.length }) : ""}</span>
          <span className="grow" />
          <span className="muted">{t("graph.asOf")}</span>
          <Input
            className="mono"
            aria-label={t("graph.asOf")}
            placeholder={t("graph.asOfPh")}
            value={at}
            onChange={(e) => setAt(e.target.value)}
            onPressEnter={() => loadFacts(at)}
            onBlur={() => loadFacts(at)}
            style={{ width: 150 }}
          />
          {at ? (
            <Button
              type="link"
              onClick={() => {
                setAt("");
                loadFacts();
              }}
            >
              {t("graph.backToNow")}
            </Button>
          ) : null}
        </div>
        {factsErr ? (
          <ErrorState
            title={t("graph.factsFailed")}
            hint={String(factsErr)}
            onRetry={() => loadFacts(at)}
            retryLabel={t("common.retry")}
          />
        ) : facts === null ? (
          <Spinner />
        ) : facts.length === 0 ? (
          <p className="muted pad">{t("graph.noFacts")}</p>
        ) : (
          <Table
            className="facts"
            size="small"
            dataSource={facts.slice(0, FACTS_CAP)}
            rowKey="id"
            pagination={false}
            rowClassName={(f) => (f.invalid_at ? "row-old" : "")}
            columns={[
              { title: t("graph.relation"), dataIndex: "relation", render: (v: string) => <span className="mono">{v}</span> },
              { title: t("graph.fact"), dataIndex: "fact_text" },
              { title: t("graph.valid"), dataIndex: "valid_at", render: (v: string) => <span className="muted mono">{dateOf(v)}</span> },
              {
                title: t("graph.until"),
                dataIndex: "invalid_at",
                render: (v: string | null) => <span className="muted mono">{v ? dateOf(v) : "—"}</span>,
              },
            ]}
          />
        )}
        {facts && facts.length > FACTS_CAP ? (
          <p className="muted micro pad">{t("graph.capped", { n: FACTS_CAP })}</p>
        ) : null}
      </div>

      <div className="card">
        <div className="zone-head">
          <div className="zone-title">{t("graph.neighbors")}</div>
          <span className="zone-note">
            {neighbors ? t("graph.rows", { n: neighbors.length }) : ""}
          </span>
        </div>
        {neighbors === null ? (
          <Spinner />
        ) : neighbors.length === 0 ? (
          <p className="muted pad">{t("graph.noNeighbors")}</p>
        ) : (
          <div className="row wrap">
            {neighbors.slice(0, NEIGHBORS_CAP).map(([n, d]) => (
              <button key={n.id} className="neighbor" onClick={() => onSelectEntity(n)}>
                {kindIcon(n.kind)} {n.name} <span className="muted">·{d}</span>
              </button>
            ))}
          </div>
        )}
        {neighbors && neighbors.length > NEIGHBORS_CAP ? (
          <p className="muted micro pad">{t("graph.capped", { n: NEIGHBORS_CAP })}</p>
        ) : null}
      </div>

      {addingFact && (
        <AddFactModal
          source={entity}
          onClose={() => setAddingFact(false)}
          onAdded={() => {
            setAddingFact(false);
            loadFacts(at);
            onGraphChanged();
          }}
        />
      )}
    </div>
  );
}

function CreateEntityModal({ onClose, onCreated }: { onClose: () => void; onCreated: () => void }) {
  const [name, setName] = useState("");
  const [kind, setKind] = useState("");
  const [summary, setSummary] = useState("");
  const [err, setErr] = useState<string | null>(null);
  // t229: this dialog had no pending state at all — during a slow create the
  // button read loading=false / disabled=false, so the same submit could be
  // fired again from the window. Same shape as Board's CreateTaskModal (busy
  // drives loading, busy also holds disabled) and as AddFactModal above.
  const [busy, setBusy] = useState(false);
  const { t } = useI18n();
  const toast = useToast();
  return (
    <Modal title={t("graph.newEntity.title")} onClose={onClose}>
      <label className="field">
        <span>{t("graph.newEntity.name")}</span>
        <Input autoFocus value={name} onChange={(e) => setName(e.target.value)} placeholder={t("graph.namePh")} />
      </label>
      <label className="field">
        <span>{t("graph.newEntity.kind")}</span>
        <Input value={kind} onChange={(e) => setKind(e.target.value)} placeholder={t("graph.kindPh")} />
      </label>
      <label className="field">
        <span>{t("graph.newEntity.summary")}</span>
        <Input value={summary} onChange={(e) => setSummary(e.target.value)} />
      </label>
      {err ? <ErrorState title={t("graph.createFailed")} hint={err} /> : null}
      <div className="row end">
        <Button
          type="primary"
          loading={busy}
          disabled={busy || !name.trim()}
          onClick={async () => {
            setBusy(true);
            setErr(null);
            try {
              await api.graphCreateEntity(name.trim(), kind.trim() || undefined, summary.trim() || undefined);
              toast("ok", t("toast.entityCreated"));
              onCreated();
            } catch (e) {
              // §5 error ③: a line that stays, and every field keeps its value.
              setErr(String(e));
            } finally {
              setBusy(false);
            }
          }}
        >
          {t("common.create")}
        </Button>
      </div>
    </Modal>
  );
}

function AddFactModal({
  source,
  onClose,
  onAdded,
}: {
  source: GraphEntity;
  onClose: () => void;
  onAdded: () => void;
}) {
  const [targetName, setTargetName] = useState("");
  const [relation, setRelation] = useState("");
  const [factText, setFactText] = useState("");
  const [validAt, setValidAt] = useState("");
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const { t } = useI18n();
  const toast = useToast();
  return (
    <Modal title={t("graph.addFact.title", { name: source.name })} onClose={onClose}>
      <p className="muted"><Markdown>{t("graph.addFact.desc")}</Markdown></p>
      <label className="field">
        <span>{t("graph.addFact.target")}</span>
        <Input
          autoFocus
          value={targetName}
          onChange={(e) => setTargetName(e.target.value)}
          placeholder={t("graph.targetPh")}
        />
      </label>
      <label className="field">
        <span>{t("graph.addFact.relation")}</span>
        <Input value={relation} onChange={(e) => setRelation(e.target.value)} placeholder={t("graph.relationPh")} />
      </label>
      <label className="field">
        <span>{t("graph.addFact.text")}</span>
        <Input
          value={factText}
          onChange={(e) => setFactText(e.target.value)}
          placeholder={t("graph.addFact.textPh")}
        />
      </label>
      <label className="field">
        <span>{t("graph.addFact.valid")}</span>
        <Input className="mono" value={validAt} onChange={(e) => setValidAt(e.target.value)} placeholder={t("graph.datePh")} />
      </label>
      {err ? <ErrorState title={t("graph.addFactFailed")} hint={err} /> : null}
      <div className="row end">
        <Button
          type="primary"
          loading={busy}
          disabled={!targetName.trim() || !relation.trim() || !factText.trim()}
          onClick={async () => {
            setBusy(true);
            setErr(null);
            try {
              const target = await api.graphCreateEntity(targetName.trim());
              await api.graphAddFact({
                src: source.id,
                dst: target.id,
                relation: relation.trim(),
                fact_text: factText.trim(),
                valid_at: validAt.trim() || undefined,
              });
              toast("ok", t("toast.factAdded"));
              onAdded();
            } catch (e) {
              // §5 error ③: a line that stays, and every field keeps its value.
              setErr(String(e));
            } finally {
              setBusy(false);
            }
          }}
        >
          {t("graph.addFact.btn")}
        </Button>
      </div>
    </Modal>
  );
}
