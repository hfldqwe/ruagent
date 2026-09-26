// Memory browser: stores × namespaces, supersession chains, write dialog,
// audit trail (OpenViking parity).
//
// Three tasks, three tabs — browse / recall / audit — because they are three
// different jobs and side by side they interrupt each other. The store
// readouts are the top-level navigation: each gauge is a jump-link into that
// store (view-memory.md §1).

import { Button, Input, Popconfirm, Segmented, Select } from "antd";
import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import {
  api,
  type GraphEdge,
  type KnowledgeExpansion,
  type MemoryDiff,
  type MemoryRow,
  type RecallResult,
} from "../api";
import {
  Empty,
  ErrorState,
  Modal,
  ReadoutStrip,
  RelTime,
  Spinner,
  Zone,
} from "../ui";
import { Markdown } from "./lazy-markdown";
import { dateOf, useI18n } from "../i18n";

const STORES = ["profile", "observation", "procedure", "lesson"] as const;
type Store = (typeof STORES)[number];

// Store → allowed namespaces (design §6.1).
const NAMESPACES: Record<Store, string[]> = {
  profile: ["user"],
  observation: ["user", "global"],
  procedure: ["global"],
  lesson: ["global"],
};

/** Memory cards per page — memoryList has no server-side paging, so the view
 *  slices and says so (view-memory.md §4 density cap). */
const CARD_CAP = 100;
/** Confidence below this is worth showing: it is the one quality signal the
 *  row never surfaced (M12). */
const LOW_CONFIDENCE = 0.5;

/** The daemon serialises `WriteOutcome` with `{:?}`, so the raw string is
 *  "Inserted(7)" / "Superseded { old: 1, new: 2 }" / "RejectedNamespace".
 *  The view's job is to say what happened, not to echo the enum (M11). */
function outcomeKey(outcome: string): string {
  const variant = outcome.split(/[({\s]/)[0].trim();
  return `memory.outcome.${variant.charAt(0).toLowerCase()}${variant.slice(1)}`;
}

/* MASTER §12 row 58 — the URL carries the view state, the same shape
   SessionsView / Knowledge / Board use. App.parseHash splits "route[?query]"
   before matching the path, so a suffix reaches this view. */
function routeQuery(): URLSearchParams {
  const h = window.location.hash.replace(/^#/, "");
  const i = h.indexOf("?");
  return new URLSearchParams(i >= 0 ? h.slice(i + 1) : "");
}

/** Whitelist over ALL THREE values: an unknown one falls back to the default
 *  rather than reaching the Segmented, which would render with nothing
 *  selected. */
function readTab(): "browse" | "recall" | "audit" {
  const v = routeQuery().get("tab");
  return v === "recall" || v === "audit" ? v : "browse";
}

export function Memory() {
  const { t } = useI18n();
  const [counts, setCounts] = useState<[string, string, number][] | null>(null);
  const [store, setStore] = useState<Store>("observation");
  const [namespace, setNamespace] = useState("user");
  const [memories, setMemories] = useState<MemoryRow[] | null>(null);
  const [memError, setMemError] = useState(false);
  const [recallLog, setRecallLog] = useState<
    Awaited<ReturnType<typeof api.recallLog>>["log"] | null
  >(null);
  /** t297: the WHOLE log's row count. The page holds at most `limit` rows, so
   *  counting them says nothing about the log — the daemon's envelope carries
   *  the real total, and the view must read it instead of implying it. */
  const [logTotal, setLogTotal] = useState<number | null>(null);
  const [logFilter, setLogFilter] = useState<string | null>(null);
  const [writing, setWriting] = useState(false);
  const [tab, setTab] = useState<"browse" | "recall" | "audit">(readTab);

  /* Row 58, both directions (idempotent write; "browse" is the default so the
     plain "#memory" stays clean; replaceState fires no hashchange, so the two
     effects cannot loop). Only the tab is written: the store and namespace
     controls are filters INSIDE browse, and the capture set is ②/③ — a tab or
     mode that changes what you are looking at — not filters. */
  useEffect(() => {
    const loc = window.location;
    const h = loc.hash.replace(/^#/, "");
    if (h !== "memory" && !h.startsWith("memory?")) return;
    const p = new URLSearchParams();
    if (tab !== "browse") p.set("tab", tab);
    const qs = p.toString();
    const next = loc.pathname + loc.search + "#memory" + (qs ? "?" + qs : "");
    if (loc.pathname + loc.search + loc.hash !== next) {
      window.history.replaceState(window.history.state, "", next);
    }
  }, [tab]);

  useEffect(() => {
    const apply = () => {
      const h = window.location.hash.replace(/^#/, "");
      if (h !== "memory" && !h.startsWith("memory?")) return;
      setTab(readTab());
    };
    window.addEventListener("hashchange", apply);
    return () => window.removeEventListener("hashchange", apply);
  }, []);

  const refresh = useCallback(() => {
    api
      .memoryList(store, namespace)
      .then((r) => {
        setMemories(r.memories);
        setCounts(r.counts);
        setMemError(false);
      })
      .catch(() => {
        // Never blank the list on a failed poll: an unreadable store is not
        // an empty store (MASTER §12 row 20).
        setMemError(true);
      });
  }, [store, namespace]);

  useEffect(() => {
    refresh();
  }, [refresh]);

  // t253: the recall log moved here from #stats. It is produced by THIS page
  // (api.recall is called only from this view), so #stats was a second
  // rendering point for data it did not own. Loaded once: it is a log, and a
  // second poll would spend the route's request budget for no new truth.
  // t284: the source filter is SERVER-side. The daemon's WHERE is an exact-match
  // inclusion filter (`?2 IS NULL OR (?2 = 'unknown' AND source IS NULL) OR
  // source = ?2`, api.rs recall_log), so the panel can ask for one source and
  // spend the page's limit on rows that match instead of filtering after the
  // fact. "Hide probe" is therefore a selection, not a post-filter: picking a
  // source other than probe is what keeps probe rows out of the page, and
  // `unknown` selects exactly the rows written before the column existed.
  const [srcFilter, setSrcFilter] = useState<string | undefined>(undefined);

  useEffect(() => {
    void api
      .recallLog(20, srcFilter)
      .then((page) => {
        setRecallLog(page.log);
        setLogTotal(page.retention.rows);
        // The daemon echoes the filter it actually applied (trimmed and
        // lowercased), which is worth showing: it is the server's answer, not
        // the panel's assumption about what it asked for.
        setLogFilter(page.source_filter ?? null);
      })
      .catch(() => {
        setRecallLog([]);
        setLogTotal(null);
        setLogFilter(null);
      });
  }, [srcFilter]);


  const projectNamespaces = (counts ?? [])
    .filter(([s, ns]) => s === store && ns.startsWith("project:"))
    .map(([, ns]) => ns);
  const agentNamespaces = (counts ?? [])
    .filter(([s, ns]) => s === store && ns.startsWith("agent:"))
    .map(([, ns]) => ns);
  const storeTotals = (counts ?? []).reduce<Record<string, number>>((acc, [s, , n]) => {
    acc[s] = (acc[s] ?? 0) + n;
    return acc;
  }, {});
  const nsOptions = [
    ...new Set([...NAMESPACES[store], ...projectNamespaces, ...agentNamespaces]),
  ];

  const shown = memories?.slice(0, CARD_CAP) ?? [];

  return (
    <div>
      <h1 className="sr-only micro">{t("memory.title")}</h1>
      <div className="view-bar">
        <h2>{t("memory.title")}</h2>
        <span className="muted">{t("memory.subtitle")}</span>
        <span className="grow" />
        <Segmented
          value={tab}
          onChange={(v) => setTab(v as "browse" | "recall" | "audit")}
          options={[
            { value: "browse", label: t("memory.browse") },
            { value: "recall", label: t("memory.recall") },
            { value: "audit", label: t("memory.audit") },
          ]}
        />
        {tab === "browse" && (
          <Button type="primary" onClick={() => setWriting(true)}>
            + {t("memory.write")}
          </Button>
        )}
      </div>

      {/* `grid` -> .readout-strip.grid (README §3.4 X4): the store gauges are
          also the store switcher, so they must stay a readable 2-up block on a
          phone instead of a 4-row stack. */}
      <ReadoutStrip
        grid
        items={(Object.keys(NAMESPACES) as Store[]).map((s) => ({
          key: s,
          label: t(`memory.store.${s}`),
          value: storeTotals[s] ?? 0,
          onOpen: () => {
            setTab("browse");
            setStore(s);
            setNamespace(NAMESPACES[s][0]);
          },
        }))}
      />

      {tab === "audit" ? (
        <AuditView />
      ) : tab === "recall" ? (
        <RecallPlayground />
      ) : (
        <>
          <Zone
            title={t("memory.browse")}
            note={
              memories === null
                ? `${t(`memory.store.${store}`)} · ${namespace}`
                : t("memory.shown", { n: shown.length, total: memories.length })
            }
          >
            <div className="filter-bar">
              <Segmented
                value={store}
                onChange={(v) => {
                  setStore(v as Store);
                  setNamespace(NAMESPACES[v as Store][0]);
                }}
                options={STORES.map((s) => ({ value: s, label: t(`memory.store.${s}`) }))}
              />
              <Select
                value={namespace}
                onChange={setNamespace}
                // A name, so the combobox input is not an unnamed control
                // (MASTER §12 row 17).
                aria-label={t("memory.ns")}
                // antd sizes the inner combobox input from its line-height
                // (14 x 1.5 = 21px), which is under the 24px hit floor of
                // row 18. The floor is restored on the input itself; a
                // shared `.ant-select-input { min-height: 24px }` would be
                // the better home for it (reported to the captain).
                styles={{ input: { minHeight: 24 } }}
                style={{ minWidth: 170 }}
                options={nsOptions.map((ns) => ({ value: ns, label: ns }))}
              />
              <NewNamespaceInput
                store={store}
                onCreated={(ns) => {
                  setNamespace(ns);
                }}
              />
            </div>
          </Zone>

          {memError ? (
            <ErrorState
              title={t("memory.err")}
              hint={t("memory.err.hint")}
              onRetry={refresh}
              retryLabel={t("common.retry")}
            />
          ) : memories === null ? (
            <Spinner label={`${t("memory.title")}…`} />
          ) : memories.length === 0 ? (
            <Empty
              icon="brain"
              title={t("memory.empty.title", {
                store: t(`memory.store.${store}`),
                namespace,
              })}
              hint={t("memory.empty.hint")}
            />
          ) : (
            <>
              <div className="memory-list">
                {shown.map((m) => (
                  <MemoryCard key={m.id} memory={m} onChanged={refresh} />
                ))}
              </div>
              {memories.length > shown.length && (
                <p className="muted micro">
                  {t("memory.shown", { n: shown.length, total: memories.length })}
                </p>
              )}
            </>
          )}
        </>
      )}

      {recallLog && recallLog.length > 0 && (
        <Zone
          title={t("stats.recallLog")}
          note={
            logTotal === null
              ? t("stats.rows", { n: recallLog.length })
              : t("memory.recallShown", { n: recallLog.length, total: logTotal }) +
                (logFilter ? " · " + t("memory.recallSource") + ": " + logFilter : "")
          }
        >
          <div className="card">
            {/* t284: a server-side source filter (see the state comment above).
                The request carries `source=`; rows with a NULL source are reached
                by the special value `unknown` and are never dropped implicitly. */}
            <div className="row tight muted micro">
              <span>{t("memory.recallSource")}</span>
              <Select
                size="small"
                aria-label={t("memory.recallSource")}
                value={srcFilter ?? "__all__"}
                onChange={(v) => setSrcFilter(v === "__all__" ? undefined : v)}
                options={[
                  { value: "__all__", label: t("memory.recallSourceAll") },
                  { value: "unknown", label: t("memory.recallSourceUnknown") },
                  ...["probe", "distill", "user"]
                    .filter((s) => (recallLog ?? []).some((r) => (r as { source?: string | null }).source === s) || s === "probe")
                    .map((s) => ({ value: s, label: s })),
                ]}
                style={{ minWidth: 120 }}
              />
              <span title={t("memory.recallProbeNote")} className="tag micro">
                ?
              </span>
              <span>{t("stats.rows", { n: recallLog.length })}</span>
            </div>
            {recallLog.map((r, i) => {
              // t253: who produced the row. Rows written before the column
              // existed carry no source and read as unknown — never as
              // "user", never guessed from the query, never filtered out.
              const src = (r as { source?: string | null }).source;
              return (
                <div key={`${r.ts}-${i}`} className="row">
                  <span className="mono truncated">{r.query}</span>
                  <span className="tag">
                    {r.strategy === "conservative"
                      ? t("memory.recallConservative")
                      : t("memory.recallAggressive")}
                  </span>
                  <span className="micro muted">
                    {t("stats.topN", { n: r.top_n })}
                  </span>
                  <span className="tag micro">
                    {src ?? t("stats.recallSourceUnknown")}
                  </span>
                  <span className="mono muted">
                    mem {r.memories}  know {r.knowledge}  wiki {r.wiki}  ent{" "}
                    {r.entities}
                  </span>
                  {r.top_memory_score != null && (
                    <span className="muted mono micro">
                      m {r.top_memory_score.toFixed(2)}
                    </span>
                  )}
                  {/* t253: 3 decimals, not 2. The live values are 0.0164 /
                      0.0325 / 0.0323 — at 2 decimals two of the three collapse
                      to the same "0.03" and the reading loses its information. */}
                  {r.top_knowledge_score != null && (
                    <span
                      className="muted mono micro"
                      title={t("knowledge.scoreHint")}
                    >
                      {t("knowledge.score", {
                        s: r.top_knowledge_score.toFixed(3),
                      })}
                    </span>
                  )}
                  <span className="time">
                    <RelTime iso={r.ts} />
                  </span>
                </div>
              );
            })}
          </div>
        </Zone>
      )}

      {writing && (
        <WriteModal
          store={store}
          namespace={namespace}
          onClose={() => setWriting(false)}
          onWritten={() => {
            setWriting(false);
            refresh();
          }}
        />
      )}
    </div>
  );
}

function NewNamespaceInput({ store, onCreated }: { store: Store; onCreated: (ns: string) => void }) {
  const { t } = useI18n();
  const [value, setValue] = useState("");
  const [open, setOpen] = useState(false);
  const prefix = store === "profile" ? "" : store === "observation" ? "project:" : "global";
  return (
    <span className="muted">
      {prefix ? (
        open ? (
          <span className="row tight">
            <Input
              autoFocus
              className="mono"
              size="small"
              aria-label={t("memory.ns")}
              placeholder={`${prefix}…`}
              value={value}
              onChange={(e) => setValue(e.target.value)}
              onPressEnter={() => {
                if (value.trim()) {
                  onCreated(`${prefix}${value.trim()}`);
                  setValue("");
                  setOpen(false);
                }
              }}
              onKeyDown={(e) => e.key === "Escape" && setOpen(false)}
              style={{ width: 160 }}
            />
          </span>
        ) : (
          /* Quiet action: a secondary control must not wear the signal
             (MASTER §3.2 V4). Default size also clears the 24px hit floor. */
          <Button type="text" onClick={() => setOpen(true)}>
            + {t("memory.nsNew")}
          </Button>
        )
      ) : null}
    </span>
  );
}

function MemoryCard({ memory, onChanged }: { memory: MemoryRow; onChanged: () => void }) {
  const { t } = useI18n();
  const [editing, setEditing] = useState(false);
  const [busy, setBusy] = useState(false);
  const [failed, setFailed] = useState(false);
  const superseded = memory.superseded_at != null;

  return (
    <div className={superseded ? "card memory-card superseded" : "card memory-card"}>
      <div className="row tight">
        <span className="tag">{memory.store}</span>
        <span className="tag">{memory.namespace}</span>
        {superseded ? (
          <span className="tag warn">
            {t("memory.superseded")} {dateOf(memory.superseded_at!)}
          </span>
        ) : (
          <span className="tag ok">{t("memory.current")}</span>
        )}
        {memory.supersedes != null ? (
          <span className="muted mono">{t("memory.replaces", { id: memory.supersedes })}</span>
        ) : null}
        <span className="grow" />
        {/* t253: this number used to render ONLY when confidence < LOW_CONFIDENCE,
            and no row in the live store is below 0.5 (measured distribution over
            57 rows: 1.0 x19, 0.9 x9, 0.8 x22, 0.95 x3, 0.7 x1, 0.6 x1, 0.5 x2) —
            so a reading the panel promised was never on screen at all. It is shown
            always now, with its provenance: the writer supplies the value (default
            0.9) and it is not a calibrated probability. */}
        <span className="muted mono" title={t("memory.confidenceHint")}>
          {memory.confidence.toFixed(2)}
          {memory.confidence < LOW_CONFIDENCE
            ? ` ${t("memory.confidenceLow")}`
            : ""}
        </span>
        <span className="time" title={memory.updated_at}>
          <RelTime iso={memory.updated_at} /> · #{memory.id}
        </span>
      </div>
      <p className={superseded ? "memory-content old" : "memory-content"}>{memory.content}</p>
      {/* t263: the correction path (supersede) already existed; this adds the
          other half of t251's lifecycle API. Soft delete: the row stays in the
          store with a tombstone and leaves the default view. */}
      {!superseded && (
        <div className="row tight">
          <Popconfirm
            title={t("memory.deleteConfirm.title", { id: memory.id })}
            description={t("memory.deleteConfirm.hint")}
            okText={t("memory.delete")}
            cancelText={t("common.cancel")}
            onConfirm={async () => {
              setBusy(true);
              setFailed(false);
              try {
                await api.memoryDelete(memory.id);
                onChanged();
              } catch {
                setFailed(true);
              } finally {
                setBusy(false);
              }
            }}
          >
            <Button danger type="text" loading={busy}>
              {t("memory.delete")}
            </Button>
          </Popconfirm>
          {failed ? (
            <span className="muted micro">{t("memory.deleteFailed")}</span>
          ) : null}
        </div>
      )}
      {!superseded && (
        <div className="row tight">
          {/* Destructive: confirm first (audit P2). Quiet by colour — V4. */}
          {/* t310: this entry is a TWO-STEP path, and the step order is what makes
              it look like it does not open: clicking the trigger first shows a
              Popconfirm (`.ant-popconfirm`, title `memory.supersede.confirm`, focus
              stays on the trigger) and only its OK mounts the editor (`.ant-modal`,
              title `memory.supersede.title`, a textarea, focus inside the dialog).
              Looking for `.ant-modal` right after the click finds nothing; t284
              measured exactly that and reported MODAL null. Usable selectors:
              `.memory-card button:has-text(替代)` -> `.ant-popconfirm-buttons
              button.ant-btn-primary` -> `.ant-modal textarea` +
              `.ant-modal button.ant-btn-primary`. */}
          <Popconfirm
            title={t("memory.supersede.confirm")}
            okText={t("memory.supersede.btn")}
            cancelText={t("common.cancel")}
            onConfirm={() => setEditing(true)}
          >
            <Button type="text">{t("memory.supersede")}</Button>
          </Popconfirm>
        </div>
      )}
      {editing && (
        <EditModal
          memory={memory}
          onClose={() => setEditing(false)}
          onSaved={() => {
            setEditing(false);
            onChanged();
          }}
        />
      )}
    </div>
  );
}

/** Write / supersede both report through this: a translated outcome, kept on
 *  screen (role=status) instead of a 2-second toast that echoes the backend
 *  enum (M11). */
function OutcomeLine({ result }: { result: { ok: boolean; text: string } | null }) {
  if (!result) return null;
  return (
    <p className="muted" role={result.ok ? "status" : "alert"}>
      {result.text}
    </p>
  );
}

function EditModal({
  memory,
  onClose,
  onSaved,
}: {
  memory: MemoryRow;
  onClose: () => void;
  onSaved: () => void;
}) {
  const [content, setContent] = useState(memory.content);
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<{ ok: boolean; text: string } | null>(null);
  const { t } = useI18n();
  return (
    <Modal title={t("memory.supersede.title", { id: memory.id })} onClose={onClose}>
      <p className="muted">{t("memory.supersede.desc")}</p>
      <Input.TextArea rows={4} value={content} onChange={(e) => setContent(e.target.value)} />
      <OutcomeLine result={result} />
      <div className="row end">
        <Button
          type="primary"
          loading={busy}
          disabled={!content.trim()}
          onClick={async () => {
            setBusy(true);
            try {
              const r = await api.memorySupersede(memory.id, content);
              setResult({ ok: true, text: t(outcomeKey(r.outcome)) });
              onSaved();
            } catch {
              setResult({ ok: false, text: t("memory.supersede.failed") });
            } finally {
              setBusy(false);
            }
          }}
        >
          {t("memory.supersede.btn")}
        </Button>
      </div>
    </Modal>
  );
}

function WriteModal({
  store,
  namespace,
  onClose,
  onWritten,
}: {
  store: string;
  namespace: string;
  onClose: () => void;
  onWritten: () => void;
}) {
  const [content, setContent] = useState("");
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<{ ok: boolean; text: string } | null>(null);
  const { t } = useI18n();
  return (
    <Modal title={t("memory.write.title")} onClose={onClose}>
      <p className="muted">{t("memory.write.desc")}</p>
      <label className="field">
        <span>{t("memory.write.govern")}</span>
        <div className="row tight mono">
          <span className="tag">{store}</span>
          <span className="tag">{namespace}</span>
        </div>
      </label>
      <label className="field">
        <span>{t("memory.write.content")}</span>
        <Input.TextArea
          autoFocus
          rows={4}
          value={content}
          onChange={(e) => setContent(e.target.value)}
          placeholder={t("memory.write.placeholder")}
        />
      </label>
      <OutcomeLine result={result} />
      <div className="row end">
        <Button
          type="primary"
          loading={busy}
          disabled={!content.trim()}
          onClick={async () => {
            setBusy(true);
            try {
              const r = await api.memoryWrite(store, namespace, content.trim());
              setResult({ ok: true, text: t(outcomeKey(r.outcome)) });
              onWritten();
            } catch {
              // The typed content stays in the editor — a failed write must
              // not throw away what the user wrote (view-memory.md §5).
              setResult({ ok: false, text: t("memory.write.failed") });
            } finally {
              setBusy(false);
            }
          }}
        >
          {t("memory.write.btn")}
        </Button>
      </div>
    </Modal>
  );
}

function oneLine(s: string): string {
  const flat = s.replace(/\s+/g, " ").trim();
  return flat.length > 60 ? `${flat.slice(0, 60)}…` : flat;
}

function AuditView() {
  const { t } = useI18n();
  const [diffs, setDiffs] = useState<MemoryDiff[] | null>(null);
  const [failed, setFailed] = useState(false);

  const load = useCallback(() => {
    setFailed(false);
    api
      .memoryDiffs(200)
      .then(setDiffs)
      .catch(() => setFailed(true));
  }, []);

  useEffect(() => {
    load();
  }, [load]);

  if (failed) {
    return (
      <ErrorState
        title={t("memory.audit.err")}
        hint={t("memory.err.hint")}
        onRetry={load}
        retryLabel={t("common.retry")}
      />
    );
  }
  if (!diffs) return <Spinner label={`${t("memory.audit")}…`} />;
  if (diffs.length === 0) {
    return (
      <Empty
        icon="scroll"
        title={t("memory.audit.empty")}
        hint={t("memory.audit.empty.hint")}
      />
    );
  }
  return (
    <div className="card">
      {diffs.map((d) => (
        <div key={d.id} className="diff-row">
          <span className="time mono muted">{dateOf(d.ts)}</span>
          <span
            className={
              d.op === "insert"
                ? "tag ok"
                : d.op === "supersede"
                  ? "tag warn"
                  : d.op === "reject"
                    ? "tag err"
                    : "tag"
            }
          >
            {d.op}
          </span>
          <span className="mono muted">
            {d.mem_store}/{d.namespace}
          </span>
          {/* One line each with the full text in `title`: a long before/after
              must not stretch the audit list (view-memory.md §5 密集). */}
          <span className="diff-detail">
            {d.before ? <code title={d.before}>{oneLine(d.before)}</code> : null}
            {d.before && d.after ? " → " : null}
            {d.after ? <code title={d.after}>{oneLine(d.after)}</code> : null}
            {d.reason ? <span className="muted"> ({d.reason})</span> : null}
          </span>
        </div>
      ))}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Conservative recall returns stubs — the panel mirrors the agent's
// progressive disclosure: each stub expands inline with an on-demand
// fetch (memory_get / knowledge_expand / graph facts), instead of a dead
// truncated line. The OPEN state is owned by the playground so only one
// stub is unfolded at a time.
// ---------------------------------------------------------------------------

function useStubData<T>(open: boolean, fetcher: () => Promise<T>) {
  const [data, setData] = useState<T | null>(null);
  const [loading, setLoading] = useState(false);
  const [err, setErr] = useState(false);
  const fn = useRef(fetcher);
  fn.current = fetcher;
  // Liveness is tied to UNMOUNT, not to the effect: `setLoading(true)`
  // re-runs an effect that lists `loading`, and its cleanup would then
  // discard the very response it just asked for (the stub stayed on the
  // spinner forever). One fetch per open, keyed by a ref.
  const alive = useRef(true);
  const started = useRef(false);
  useEffect(
    () => () => {
      alive.current = false;
    },
    [],
  );
  useEffect(() => {
    if (!open || started.current) return;
    started.current = true;
    setLoading(true);
    fn.current()
      .then((d) => {
        if (alive.current) setData(d);
      })
      .catch(() => {
        if (alive.current) setErr(true);
      })
      .finally(() => {
        if (alive.current) setLoading(false);
      });
  }, [open]);
  return { data, loading, err };
}

function StubChevron({ open }: { open: boolean }) {
  return <span className="chev">{open ? "▾" : "▸"}</span>;
}

function RecallLoading() {
  return (
    <div className="recall-loading">
      <Spinner />
    </div>
  );
}

/** The head of every stub. `aria-expanded` is mandatory: a hand-written
 *  expander without it is a §12 row 23 failure (primitives §11 row 23). */
function StubHead({
  open,
  onToggle,
  children,
}: {
  open: boolean;
  onToggle: () => void;
  children: ReactNode;
}) {
  return (
    <button className="recall-stub-head" aria-expanded={open} onClick={onToggle}>
      {children}
      <StubChevron open={open} />
    </button>
  );
}

function StubBody({
  open,
  loading,
  failed,
  children,
}: {
  open: boolean;
  loading: boolean;
  failed: boolean;
  children: ReactNode;
}) {
  const { t } = useI18n();
  return (
    <div className={open ? "recall-expand open" : "recall-expand"}>
      <div className="recall-inner">
        {loading ? (
          <RecallLoading />
        ) : failed ? (
          <p className="muted micro">{t("memory.recallNoFacts")}</p>
        ) : open ? (
          children
        ) : null}
      </div>
    </div>
  );
}

function MemoryStub({
  stub,
  open,
  onToggle,
}: {
  stub: RecallResult["memories"][number];
  open: boolean;
  onToggle: () => void;
}) {
  const { t } = useI18n();
  const { data, loading, err } = useStubData(open, () => api.memoryGet(stub.id));
  return (
    <div className="recall-hit">
      <StubHead open={open} onToggle={onToggle}>
        <span className="tag">#{stub.id}</span>
        <span className="tag">{stub.store}</span>
        {stub.score != null && (
          <span className="muted mono" title={t("knowledge.scoreHint")}>
            {t("knowledge.score", { s: stub.score.toFixed(3) })}
          </span>
        )}
        {/* t263: the same leg evidence as the knowledge hits. The panel type
            for recall memories is behind the response (t251 added these
            fields), so they are read through a narrow cast — absent on an
            older daemon, in which case nothing renders. */}
        {(() => {
          const m = stub as {
            legs?: string[];
            semantic_score?: number | null;
            keyword_score?: number | null;
          };
          return (
            <>
              {m.legs?.length ? (
                <span className="tag micro">{t("knowledge.legs")} {m.legs.join(" · ")}</span>
              ) : null}
              {m.semantic_score != null ? (
                <span className="muted mono micro" title={t("knowledge.semanticHint")}>
                  {t("knowledge.semanticScore", { s: m.semantic_score.toFixed(4) })}
                </span>
              ) : null}
              {m.keyword_score != null ? (
                <span className="muted mono micro" title={t("knowledge.keywordHint")}>
                  {t("knowledge.keywordScore", { s: m.keyword_score.toFixed(4) })}
                </span>
              ) : null}
            </>
          );
        })()}
        <span className="stub-text">{stub.title}</span>
        {!open && <span className="stub-afford">{t("memory.recallExpand")}</span>}
      </StubHead>
      <StubBody open={open} loading={loading} failed={err}>
        {data ? (
          <div className="recall-body md">
            <Markdown>{data.content}</Markdown>
          </div>
        ) : null}
      </StubBody>
    </div>
  );
}

/** §13-2: a generated wiki page hit — always a stub, labeled so the
 * reader can tell compiled content from sources. */
function WikiStub({
  stub,
  open,
  onToggle,
}: {
  stub: NonNullable<RecallResult["wiki"]>[number];
  open: boolean;
  onToggle: () => void;
}) {
  const { t } = useI18n();
  const { data, loading, err } = useStubData(open, () => api.knowledgeExpand(stub.chunk_id));
  const x = data as KnowledgeExpansion | null;
  return (
    <div className="recall-hit">
      <StubHead open={open} onToggle={onToggle}>
        <span className="tag">wiki</span>
        <strong>{stub.title}</strong>
        <span className="tag warn">{t("memory.recallWikiGenerated")}</span>
        {stub.stale && <span className="tag warn">{t("wiki.staleTag")}</span>}
        {!open && <span className="stub-text">{stub.summary || stub.excerpt}</span>}
        {!open && <span className="stub-afford">{t("memory.recallExpand")}</span>}
      </StubHead>
      <StubBody open={open} loading={loading} failed={err}>
        {x ? (
          <div className="recall-body">
            <div className="row tight">
              <span className="tag">{stub.document}</span>
            </div>
            <Markdown>{x.section}</Markdown>
          </div>
        ) : null}
      </StubBody>
    </div>
  );
}

function KnowledgeStub({
  stub,
  open,
  onToggle,
}: {
  stub: RecallResult["knowledge"][number];
  open: boolean;
  onToggle: () => void;
}) {
  const { t } = useI18n();
  const { data, loading, err } = useStubData(open, () => api.knowledgeExpand(stub.chunk_id));
  const x = data as KnowledgeExpansion | null;
  const i = x ? x.section.indexOf(x.chunk) : -1;
  return (
    <div className="recall-hit">
      <StubHead open={open} onToggle={onToggle}>
        <span className="tag">{stub.document}</span>
        {stub.score != null && (
          <span className="muted mono" title={t("knowledge.scoreHint")}>
            {t("knowledge.score", { s: stub.score.toFixed(3) })}
          </span>
        )}
        {stub.legs?.length ? (
          <span className="tag micro">
            {t("knowledge.legs")} {stub.legs.join(" · ")}
          </span>
        ) : null}
        {stub.semantic_score != null ? (
          <span className="muted mono micro" title={t("knowledge.semanticHint")}>
            {t("knowledge.semanticScore", { s: stub.semantic_score.toFixed(4) })}
          </span>
        ) : null}
        {stub.keyword_score != null ? (
          <span className="muted mono micro" title={t("knowledge.keywordHint")}>
            {t("knowledge.keywordScore", { s: stub.keyword_score.toFixed(4) })}
          </span>
        ) : null}
        <span className="stub-text">{stub.excerpt}</span>
        {!open && <span className="stub-afford">{t("memory.recallExpand")}</span>}
      </StubHead>
      <StubBody open={open} loading={loading} failed={err}>
        {x ? (
          <div className="recall-body">
            {x.file ? (
              <div className="row tight">
                <span className="tag">{x.file}</span>
              </div>
            ) : null}
            {i >= 0 ? (
              <>
                {x.section.slice(0, i).trim() ? <Markdown>{x.section.slice(0, i)}</Markdown> : null}
                <div className="recall-chunk-hl md">
                  <span className="rev-label">{t("memory.recallHitChunk")}</span>
                  <Markdown>{x.chunk}</Markdown>
                </div>
                {x.section.slice(i + x.chunk.length).trim() ? (
                  <Markdown>{x.section.slice(i + x.chunk.length)}</Markdown>
                ) : null}
              </>
            ) : (
              <Markdown>{x.section}</Markdown>
            )}
          </div>
        ) : null}
      </StubBody>
    </div>
  );
}

function EntityStub({
  stub,
  open,
  onToggle,
}: {
  stub: RecallResult["entities"][number];
  open: boolean;
  onToggle: () => void;
}) {
  const { t } = useI18n();
  const { data, loading, err } = useStubData(open, async () => {
    const [facts, neighbors] = await Promise.all([
      api.graphFacts(stub.id),
      api.graphNeighbors(stub.id, 1),
    ]);
    return {
      facts,
      names: new Map(neighbors.map(([e]) => [e.id, e.name] as const)),
    };
  });
  return (
    <div className="recall-hit">
      <StubHead open={open} onToggle={onToggle}>
        <span className="tag">&{stub.id}</span>
        <strong>{stub.name}</strong>
        {stub.entity_kind && <span className="tag">{stub.entity_kind}</span>}
        {stub.summary && <span className="stub-text">{stub.summary}</span>}
        {!open && <span className="stub-afford">{t("memory.recallExpand")}</span>}
      </StubHead>
      <StubBody open={open} loading={loading} failed={err}>
        {data ? (
          <div className="recall-body">
            {data.facts.length === 0 ? (
              <p className="muted">{t("memory.recallNoFacts")}</p>
            ) : (
              data.facts.map((f: GraphEdge) => {
                const other = f.src === stub.id ? f.dst : f.src;
                return (
                  <div key={f.id} className={f.invalid_at ? "recall-fact old" : "recall-fact"}>
                    <span className="tag">{f.relation}</span>
                    <strong>{data.names.get(other) ?? "#" + other}</strong>
                    <span className="muted">{f.fact_text}</span>
                  </div>
                );
              })
            )}
            {(stub.related?.wiki ?? []).length > 0 && (
              <div className="row tight wrap">
                <span className="tag">wiki</span>
                {(stub.related?.wiki ?? []).map((w) => (
                  <span key={w.slug} className="tag" title={w.hint}>
                    {w.title}
                    {w.stale ? " ⚠" : ""} · {t("memory.recallWikiGenerated")}
                  </span>
                ))}
              </div>
            )}
          </div>
        ) : null}
      </StubBody>
    </div>
  );
}

function RecallPlayground() {
  const { t } = useI18n();
  const [q, setQ] = useState("");
  const [conservative, setConservative] = useState(false);
  const [result, setResult] = useState<RecallResult | null>(null);
  const [busy, setBusy] = useState(false);
  const [failed, setFailed] = useState(false);
  // Accordion: one open stub at a time, so four sections cannot all unfold
  // at once and bury the list (M10).
  const [openKey, setOpenKey] = useState<string | null>(null);

  const run = async (useConservative = conservative) => {
    if (!q.trim()) return;
    setBusy(true);
    setFailed(false);
    setOpenKey(null);
    try {
      setResult(await api.recall(q.trim(), useConservative));
    } catch {
      setFailed(true);
    } finally {
      setBusy(false);
    }
  };

  const toggle = (key: string) => setOpenKey((prev) => (prev === key ? null : key));

  const empty =
    result != null &&
    result.memories.length +
      result.knowledge.length +
      (result.wiki ?? []).length +
      result.entities.length ===
      0;

  return (
    <div>
      <div className="search-bar">
        <Input
          className="grow"
          autoFocus
          aria-label={t("memory.recallPh")}
          placeholder={t("memory.recallPh")}
          value={q}
          onChange={(e) => setQ(e.target.value)}
          onPressEnter={() => void run()}
        />
        <Segmented
          value={conservative ? "conservative" : "aggressive"}
          onChange={(v) => setConservative(v === "conservative")}
          options={[
            { value: "aggressive", label: t("memory.recallAggressive") },
            { value: "conservative", label: t("memory.recallConservative") },
          ]}
        />
        <Button type="primary" loading={busy} onClick={() => void run()}>
          {t("memory.recallGo")}
        </Button>
      </div>

      {failed && (
        <ErrorState
          title={t("memory.recall.err")}
          hint={t("memory.err.hint")}
          onRetry={() => void run()}
          retryLabel={t("common.retry")}
        />
      )}

      {result &&
        !failed &&
        (empty ? (
          <Empty
            icon="search"
            title={t("memory.recallEmpty")}
            hint={t("memory.recallAggressiveHint")}
            action={
              <Button
                onClick={() => {
                  setConservative(false);
                  void run(false);
                }}
              >
                {t("memory.recallAggressive")}
              </Button>
            }
          />
        ) : (
          <div className="card">
            <div className="row tight">
              <span className="tag">{result.strategy}</span>
              <span className="muted">
                {result.memories.length} · {result.knowledge.length} ·{" "}
                {(result.wiki ?? []).length} · {result.entities.length}
              </span>
            </div>

            {/* Four segments, never mixed: a generated wiki page is not a
                source document and must say so (view-memory.md §4). */}
            {result.memories.length > 0 && (
              <Zone title={t("nav.memory")} note={result.memories.length}>
                {result.memories.map((m) =>
                  m.hint ? (
                    <MemoryStub
                      key={`m${m.id}`}
                      stub={m}
                      open={openKey === `m${m.id}`}
                      onToggle={() => toggle(`m${m.id}`)}
                    />
                  ) : (
                    <div key={`m${m.id}`} className="search-hit">
                      <div className="row tight">
                        <span className="tag">{m.store}</span>
                        <span className="tag">{m.namespace}</span>
                        {m.score != null && (
                          <span className="muted mono">{m.score.toFixed(2)}</span>
                        )}
                      </div>
                      <p className="hit-content">{m.content ?? m.title}</p>
                    </div>
                  ),
                )}
              </Zone>
            )}

            {result.knowledge.length > 0 && (
              <Zone title={t("nav.knowledge")} note={result.knowledge.length}>
                {result.knowledge.map((k) =>
                  k.hint ? (
                    <KnowledgeStub
                      key={`k${k.chunk_id}`}
                      stub={k}
                      open={openKey === `k${k.chunk_id}`}
                      onToggle={() => toggle(`k${k.chunk_id}`)}
                    />
                  ) : (
                    <div key={`k${k.chunk_id}`} className="search-hit">
                      <span className="tag">{k.document}</span>
                      <p className="hit-content">{k.content ?? k.excerpt}</p>
                    </div>
                  ),
                )}
              </Zone>
            )}

            {(result.wiki ?? []).length > 0 && (
              <Zone title="Wiki" note={(result.wiki ?? []).length}>
                {(result.wiki ?? []).map((w) => (
                  <WikiStub
                    key={`w${w.chunk_id}`}
                    stub={w}
                    open={openKey === `w${w.chunk_id}`}
                    onToggle={() => toggle(`w${w.chunk_id}`)}
                  />
                ))}
              </Zone>
            )}

            {result.entities.length > 0 && (
              <Zone title={t("metric.entities")} note={result.entities.length}>
                {result.entities.map((e) =>
                  e.hint ? (
                    <EntityStub
                      key={`e${e.id}`}
                      stub={e}
                      open={openKey === `e${e.id}`}
                      onToggle={() => toggle(`e${e.id}`)}
                    />
                  ) : (
                    <div key={`e${e.id}`} className="search-hit">
                      <span className="tag">{e.name}</span>
                      {e.entity_kind && <span className="tag">{e.entity_kind}</span>}
                      {e.summary && <p className="hit-content">{e.summary}</p>}
                      {(e.facts ?? []).map((f, i) => (
                        <p key={i} className="hit-content mono">
                          —{f.relation}→ {f.with}: {f.fact}
                        </p>
                      ))}
                      {(e.related?.chunks ?? []).map((c, i) => (
                        <p key={`c${i}`} className="hit-content mono">
                          [doc] {c.document} · {c.excerpt}
                        </p>
                      ))}
                      {(e.related?.memories ?? []).map((m, i) => (
                        <p key={`m${i}`} className="hit-content mono">
                          [mem] #{m.id} {m.title}
                        </p>
                      ))}
                      {(e.related?.wiki ?? []).map((w, i) => (
                        <p key={`w${i}`} className="hit-content mono">
                          [wiki] {w.slug} — {w.title}
                          {w.stale ? " ⚠" : ""} ({t("memory.recallWikiGenerated")})
                        </p>
                      ))}
                    </div>
                  ),
                )}
              </Zone>
            )}
          </div>
        ))}
    </div>
  );
}
