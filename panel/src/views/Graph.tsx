// Graph explorer: an interactive force-directed canvas over entities and
// their currently-valid facts, with an inspector panel (bi-temporal facts,
// as-of queries, neighbors). A plain list mode stays one toggle away.

import { useEffect, useRef, useState } from "react";
import { Button, Input, Segmented, Table } from "antd";
import { Icon, type IconName } from "../icons";
import { api, type GraphEdge, type GraphEntity } from "../api";
import { Empty, Markdown, Modal, Spinner, useToast } from "../ui";
import { dateOf, useI18n } from "../i18n";
import { useThemeMode } from "../theme";

/** Over this the canvas only renders the most-connected nodes. */
const MAX_NODES = 150;

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

export function Graph() {
  const { t } = useI18n();
  const { mode: theme } = useThemeMode();
  const [mode, setMode] = useState<"graph" | "list">("graph");
  const [entities, setEntities] = useState<[GraphEntity, number][] | null>(null);
  const [edges, setEdges] = useState<GraphEdge[] | null>(null);
  const [query, setQuery] = useState("");
  const [hitIds, setHitIds] = useState<Set<number> | null>(null);
  const [selected, setSelected] = useState<GraphEntity | null>(null);
  const [creating, setCreating] = useState(false);
  const toast = useToast();

  const refresh = () => {
    api
      .graphEntitiesAll()
      .then(async (ents) => {
        setEntities(ents);
        setEdges(null);
        // Edges come from each rendered entity's current facts (the
        // endpoint already excludes invalidated ones); dedupe by id.
        const rendered =
          ents.length > MAX_NODES
            ? [...ents].sort((a, b) => b[1] - a[1]).slice(0, MAX_NODES)
            : ents;
        const perEntity = await Promise.all(
          rendered.map(([e]) => api.graphEntity(e.id).catch(() => [] as GraphEdge[])),
        );
        const ids = new Set(rendered.map(([e]) => e.id));
        const seen = new Set<number>();
        const list: GraphEdge[] = [];
        for (const facts of perEntity) {
          for (const f of facts) {
            if (!seen.has(f.id) && ids.has(f.src) && ids.has(f.dst)) {
              seen.add(f.id);
              list.push(f);
            }
          }
        }
        setEdges(list);
      })
      .catch((e) => {
        toast("err", String(e));
        setEntities([]);
        setEdges([]);
      });
  };

  useEffect(() => {
    refresh();
  }, []);

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
    }
  };

  const capped = (entities?.length ?? 0) > MAX_NODES;
  const loading = entities === null || (mode === "graph" && edges === null);

  return (
    <div>
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

      <div className="search-bar">
        <Input
          className="grow"
          placeholder={t("graph.searchPh")}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onPressEnter={search}
        />
      </div>

      {loading ? (
        <Spinner label={`${t("graph.title")}…`} />
      ) : entities.length === 0 ? (
        <Empty
          icon="graph"
          title={t("graph.empty.title")}
          hint={t("graph.empty.hint")}
        />
      ) : (
        <div className="graph-layout">
          <div className="graph-main">
            {mode === "graph" ? (
              <>
                <GraphCanvas
                  entities={entities}
                  edges={edges ?? []}
                  selectedId={selected?.id ?? null}
                  hitIds={hitIds}
                  theme={theme}
                  onSelect={setSelected}
                />
                <div className="graph-hint">
                  {t("graph.hint")}
                  {capped ? ` — ${t("graph.tooMany", { n: MAX_NODES })}` : ""}
                </div>
              </>
            ) : (
              <div className="card">
                {entities.map(([e, factCount]) => (
                  <button key={e.id} className="row-btn" onClick={() => setSelected(e)}>
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

          {selected && (
            <EntityDetail
              key={selected.id}
              entity={selected}
              onClose={() => setSelected(null)}
              onSelectEntity={setSelected}
              onGraphChanged={refresh}
            />
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

function readPalette(el: HTMLElement): Record<string, string> {
  const root = getComputedStyle(document.documentElement);
  const local = getComputedStyle(el); // inside antd's .ruagent var scope
  const v = (cs: CSSStyleDeclaration, name: string) => cs.getPropertyValue(name).trim();
  const kinds = [
    "person",
    "org",
    "project",
    "repo",
    "tool",
    "concept",
    "product",
    "protocol",
    "default",
  ];
  const out: Record<string, string> = {};
  for (const k of kinds) out[k] = v(root, `--graph-${k}`);
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
  selectedId,
  hitIds,
  theme,
  onSelect,
}: {
  entities: [GraphEntity, number][];
  edges: GraphEdge[];
  selectedId: number | null;
  hitIds: Set<number> | null;
  theme: string;
  onSelect: (e: GraphEntity | null) => void;
}) {
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
  });
  // live props for the event handlers (no listener churn)
  const propsRef = useRef({ entities, edges, selectedId, hitIds, onSelect });
  propsRef.current = { entities, edges, selectedId, hitIds, onSelect };

  // (re)build the simulation when the data changes
  useEffect(() => {
    const canvas = canvasRef.current;
    const wrap = wrapRef.current;
    if (!canvas || !wrap) return;
    const s = simRef.current;

    const rect = wrap.getBoundingClientRect();
    s.w = Math.max(rect.width, 200);
    s.h = 520;

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

      ctx.clearRect(0, 0, s.w, s.h);

      // edges under nodes
      ctx.lineWidth = 1;
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
        ctx.fillStyle = pal[normalizeKind(n.kind)] ?? pal.default;
        ctx.beginPath();
        ctx.arc(n.x, n.y, rr, 0, Math.PI * 2);
        ctx.fill();
        if (n.fixed) {
          ctx.strokeStyle = pal.label;
          ctx.lineWidth = 1;
          ctx.stroke();
        }
        if (n.id === sel || hit) {
          ctx.strokeStyle = pal.ring;
          ctx.lineWidth = 2;
          ctx.beginPath();
          ctx.arc(n.x, n.y, rr + 3, 0, Math.PI * 2);
          ctx.stroke();
        }
        const labeled =
          n.facts > 0 ||
          hit ||
          n.id === sel ||
          (focus != null && n.id === focus) ||
          (near?.has(n.id) ?? false);
        if (labeled) {
          ctx.fillStyle = pal.label;
          let label = n.name;
          if (ctx.measureText(label).width > 120) {
            while (label.length > 3 && ctx.measureText(label + "…").width > 120) {
              label = label.slice(0, -1);
            }
            label += "…";
          }
          ctx.fillText(label, n.x, n.y + rr + 13);
        }
      }
      ctx.globalAlpha = 1;
    };

    const tick = () => {
      const moving = step();
      draw();
      if (moving || performance.now() < s.pulseUntil) {
        s.raf = requestAnimationFrame(tick);
      } else {
        s.running = false;
      }
    };
    const kick = () => {
      if (!s.running) {
        s.running = true;
        s.raf = requestAnimationFrame(tick);
      }
    };

    s.draw = draw;
    s.kick = kick;

    const size = () => {
      const dpr = window.devicePixelRatio || 1;
      const r = wrap.getBoundingClientRect();
      s.w = Math.max(r.width, 200);
      s.h = 520;
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

    const toCanvas = (ev: PointerEvent | MouseEvent) => {
      const r = canvas.getBoundingClientRect();
      return { x: ev.clientX - r.left, y: ev.clientY - r.top };
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
      }
    };
    const onMove = (ev: PointerEvent) => {
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
      }
    };
    const onLeave = () => {
      s.hoverId = null;
      draw();
    };

    size();
    kick();
    const ro = new ResizeObserver(size);
    ro.observe(wrap);
    canvas.addEventListener("pointerdown", onDown);
    canvas.addEventListener("pointermove", onMove);
    canvas.addEventListener("pointerup", onUp);
    canvas.addEventListener("dblclick", onDbl);
    canvas.addEventListener("pointerleave", onLeave);

    const debug = {
      nodeCount: s.nodes.length,
      edgeCount: s.edges.length,
      get selectedId() {
        return propsRef.current.selectedId;
      },
      pinned: () => s.nodes.filter((n) => n.fixed).map((n) => n.id),
      positions: () =>
        s.nodes.map((n) => ({ id: n.id, name: n.name, x: Math.round(n.x), y: Math.round(n.y), r: Math.round(n.r) })),
      kinds: s.nodes.reduce<Record<string, number>>((acc, n) => {
        const k = normalizeKind(n.kind);
        acc[k] = (acc[k] ?? 0) + 1;
        return acc;
      }, {}),
    };
    (window as unknown as Record<string, unknown>).__graphDebug = debug;

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
      canvas.removeEventListener("pointerleave", onLeave);
      delete (window as unknown as Record<string, unknown>).__graphDebug;
    };
  }, [entities, edges]);

  // selection / search-pulse / theme redraws without rebuilding physics
  useEffect(() => {
    const s = simRef.current;
    if (hitIds && hitIds.size > 0) s.pulseUntil = performance.now() + 2400;
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
      <canvas ref={canvasRef} className="graph-canvas" />
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
}: {
  entity: GraphEntity;
  onClose: () => void;
  onSelectEntity: (e: GraphEntity) => void;
  onGraphChanged: () => void;
}) {
  const { t } = useI18n();
  const [facts, setFacts] = useState<GraphEdge[] | null>(null);
  const [neighbors, setNeighbors] = useState<[GraphEntity, number][] | null>(null);
  const [at, setAt] = useState("");
  const [addingFact, setAddingFact] = useState(false);
  const toast = useToast();

  const loadFacts = (asOf?: string) =>
    api
      .graphFacts(entity.id, asOf || undefined)
      .then(setFacts)
      .catch((e) => toast("err", String(e)));

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
        <Button size="small" type="primary" onClick={() => setAddingFact(true)}>
          + {t("graph.addFact")}
        </Button>
        <Button size="small" type="text" onClick={onClose} aria-label="close">
          <Icon name="x" size={14} />
        </Button>
      </div>
      {entity.summary ? <p className="muted intent">{entity.summary}</p> : null}

      <div className="card">
        <div className="row tight">
          <strong>{t("graph.facts")}</strong>
          <span className="muted">{t("graph.asOf")}</span>
          <Input
            className="mono"
            size="small"
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
              size="small"
              onClick={() => {
                setAt("");
                loadFacts();
              }}
            >
              {t("graph.backToNow")}
            </Button>
          ) : null}
        </div>
        {facts === null ? (
          <Spinner />
        ) : facts.length === 0 ? (
          <p className="muted pad">{t("graph.noFacts")}</p>
        ) : (
          <Table
            className="facts"
            size="small"
            dataSource={facts}
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
      </div>

      <div className="card">
        <strong>{t("graph.neighbors")}</strong>
        {neighbors === null ? (
          <Spinner />
        ) : neighbors.length === 0 ? (
          <p className="muted pad">{t("graph.noNeighbors")}</p>
        ) : (
          <div className="row wrap">
            {neighbors.map(([n, d]) => (
              <button key={n.id} className="neighbor" onClick={() => onSelectEntity(n)}>
                {kindIcon(n.kind)} {n.name} <span className="muted">·{d}</span>
              </button>
            ))}
          </div>
        )}
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
      <div className="row end">
        <Button
          type="primary"
          disabled={!name.trim()}
          onClick={async () => {
            try {
              await api.graphCreateEntity(name.trim(), kind.trim() || undefined, summary.trim() || undefined);
              toast("ok", t("toast.entityCreated"));
              onCreated();
            } catch (e) {
              toast("err", String(e));
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
      <div className="row end">
        <Button
          type="primary"
          loading={busy}
          disabled={!targetName.trim() || !relation.trim() || !factText.trim()}
          onClick={async () => {
            setBusy(true);
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
              toast("err", String(e));
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
