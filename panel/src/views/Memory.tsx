// Memory browser: stores × namespaces, supersession chains, write dialog,
// audit trail (OpenViking parity).

import { Button, Input, Segmented, Select, Spin } from "antd";
import { useEffect, useState } from "react";
import {
  api,
  type GraphEdge,
  type KnowledgeExpansion,
  type MemoryDiff,
  type MemoryRow,
  type RecallResult,
} from "../api";
import { Empty, Markdown, Modal, RelTime, Spinner, useToast } from "../ui";
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

export function Memory() {
  const { t } = useI18n();
  const [counts, setCounts] = useState<[string, string, number][] | null>(null);
  const [store, setStore] = useState<Store>("observation");
  const [namespace, setNamespace] = useState("user");
  const [memories, setMemories] = useState<MemoryRow[] | null>(null);
  const [writing, setWriting] = useState(false);
  const [tab, setTab] = useState<"browse" | "recall" | "audit">("browse");
  const toast = useToast();

  const refresh = () =>
    api
      .memoryList(store, namespace)
      .then((r) => {
        setMemories(r.memories);
        setCounts(r.counts);
      })
      .catch((e) => {
        toast("err", String(e));
        setMemories([]);
      });

  useEffect(() => {
    refresh();
  }, [store, namespace]);

  const projectNamespaces = (counts ?? [])
    .filter(([s, ns]) => s === store && ns.startsWith("project:"))
    .map(([, ns]) => ns);
  const agentNamespaces = (counts ?? [])
    .filter(([s, ns]) => s === store && ns.startsWith("agent:"))
    .map(([, ns]) => ns);
  const nsOptions = [
    ...new Set([...NAMESPACES[store], ...projectNamespaces, ...agentNamespaces]),
  ];

  return (
    <div>
      <div className="view-bar">
        <h2>{t("memory.title")}</h2>
        <span className="muted">{t("memory.subtitle")}</span>
        <span className="grow" />
        <Segmented
          value={tab}
          onChange={(v) => setTab(v as "browse" | "audit")}
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
        {tab === "recall" && <RecallPlayground />}
      </div>

      {tab === "audit" ? (
        <AuditView />
      ) : tab === "recall" ? (
        <div />
      ) : (
        <>
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

          {memories === null ? (
            <Spinner label={`${t("memory.title")}…`} />
          ) : memories.length === 0 ? (
            <Empty
              icon="brain"
              title={t("memory.empty.title", { store: t(`memory.store.${store}`), namespace })}
              hint={t("memory.empty.hint")}
            />
          ) : (
            <div className="memory-list">
              {memories.map((m) => (
                <MemoryCard key={m.id} memory={m} onChanged={refresh} />
              ))}
            </div>
          )}
        </>
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
          <Button type="link" size="small" style={{ paddingLeft: 0 }} onClick={() => setOpen(true)}>
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
  const superseded = memory.superseded_at != null;

  return (
    <div className={superseded ? "card memory-card superseded" : "card memory-card"}>
      <div className="row tight">
        <span className="tag">{memory.store}</span>
        <span className="tag">{memory.namespace}</span>
        {superseded ? (
          <span className="tag warn">{t("memory.superseded")} {dateOf(memory.superseded_at!)}</span>
        ) : (
          <span className="tag ok">{t("memory.current")}</span>
        )}
        {memory.supersedes != null ? (
          <span className="muted mono">{t("memory.replaces", { id: memory.supersedes })}</span>
        ) : null}
        <span className="grow" />
        <span className="time" title={memory.updated_at}>
          <RelTime iso={memory.updated_at} /> · #{memory.id}
        </span>
      </div>
      <p className={superseded ? "memory-content old" : "memory-content"}>{memory.content}</p>
      {!superseded && (
        <div className="row tight">
          <Button type="link" size="small" style={{ paddingLeft: 0 }} onClick={() => setEditing(true)}>
            {t("memory.supersede")}
          </Button>
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
  const { t } = useI18n();
  const toast = useToast();
  return (
    <Modal title={t("memory.supersede.title", { id: memory.id })} onClose={onClose}>
      <p className="muted">{t("memory.supersede.desc")}</p>
      <Input.TextArea rows={4} value={content} onChange={(e) => setContent(e.target.value)} />
      <div className="row end">
        <Button
          type="primary"
          onClick={async () => {
            try {
              await api.memorySupersede(memory.id, content);
              toast("ok", "superseded");
              onSaved();
            } catch (e) {
              toast("err", String(e));
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
  const { t } = useI18n();
  const toast = useToast();
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
      <div className="row end">
        <Button
          type="primary"
          disabled={!content.trim()}
          onClick={async () => {
            try {
              const r = await api.memoryWrite(store, namespace, content.trim());
              toast("ok", `memory ${r.outcome.toLowerCase()}`);
              onWritten();
            } catch (e) {
              toast("err", String(e));
            }
          }}
        >
          {t("memory.write.btn")}
        </Button>
      </div>
    </Modal>
  );
}

function AuditView() {
  const { t } = useI18n();
  const [diffs, setDiffs] = useState<MemoryDiff[] | null>(null);
  useEffect(() => {
    api.memoryDiffs(200).then(setDiffs).catch(() => setDiffs([]));
  }, []);
  if (!diffs) return <Spinner label={`${t("memory.audit")}…`} />;
  if (diffs.length === 0)
    return <Empty icon="scroll" title="No memory writes yet" hint="Every write decision lands here." />;
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
          <span className="diff-detail">
            {d.before ? <code>{d.before}</code> : null}
            {d.before && d.after ? " → " : null}
            {d.after ? <code>{d.after}</code> : null}
            {d.reason ? <span className="muted"> ({d.reason})</span> : null}
          </span>
        </div>
      ))}
    </div>
  );
}

function RecallPlayground() {
  const { t } = useI18n();
  const [q, setQ] = useState("");
  const [conservative, setConservative] = useState(false);
  const [result, setResult] = useState<RecallResult | null>(null);
  const [busy, setBusy] = useState(false);
  const toast = useToast();

  const run = async () => {
    if (!q.trim()) return;
    setBusy(true);
    try {
      setResult(await api.recall(q.trim(), conservative));
    } catch (e) {
      toast("err", String(e));
    } finally {
      setBusy(false);
    }
  };

  const empty =
    result != null &&
    result.memories.length + result.knowledge.length + result.entities.length === 0;

  return (
    <div>
      <div className="search-bar">
        <Input
          className="grow"
          autoFocus
          placeholder={t("memory.recallPh")}
          value={q}
          onChange={(e) => setQ(e.target.value)}
          onPressEnter={run}
        />
        <Segmented
          value={conservative ? "conservative" : "aggressive"}
          onChange={(v) => setConservative(v === "conservative")}
          options={[
            { value: "aggressive", label: t("memory.recallAggressive") },
            { value: "conservative", label: t("memory.recallConservative") },
          ]}
        />
        <Button type="primary" loading={busy} onClick={run}>
          {t("memory.recallGo")}
        </Button>
      </div>
      {result &&
        (empty ? (
          <Empty icon="search" title={t("memory.recallEmpty")} />
        ) : (
          <div className="card">
            <div className="row tight">
              <span className="tag">{result.strategy}</span>
              <span className="muted">
                {result.memories.length} mem · {result.knowledge.length} know ·{" "}
                {(result.wiki ?? []).length} wiki · {result.entities.length} ent
              </span>
            </div>
            {result.memories.map((m) =>
              m.hint ? (
                <MemoryStub key={`m${m.id}`} stub={m} />
              ) : (
                <div key={`m${m.id}`} className="search-hit">
                  <div className="row tight">
                    <span className="tag">{m.store}</span>
                    <span className="tag">{m.namespace}</span>
                    {m.score != null && <span className="muted mono">{m.score.toFixed(2)}</span>}
                  </div>
                  <p className="hit-content">{m.content ?? m.title}</p>
                </div>
              ),
            )}
            {result.knowledge.map((k) =>
              k.hint ? (
                <KnowledgeStub key={`k${k.chunk_id}`} stub={k} />
              ) : (
                <div key={`k${k.chunk_id}`} className="search-hit">
                  <span className="tag">{k.document}</span>
                  <p className="hit-content">{k.content ?? k.excerpt}</p>
                </div>
              ),
            )}
            {(result.wiki ?? []).map((w) => (
              <WikiStub key={`w${w.chunk_id}`} stub={w} />
            ))}
            {result.entities.map((e) =>
              e.hint ? (
                <EntityStub key={`e${e.id}`} stub={e} />
              ) : (
                <div key={`e${e.id}`} className="search-hit">
                  <span className="tag">{e.name}</span>
                  {e.entity_kind && <span className="tag">{e.entity_kind}</span>}
                  {e.summary && <p className="hit-content">{e.summary}</p>}
                  {(e.facts ?? []).map((f, i) => (
                    <p key={i} className="hit-content mono" style={{ fontSize: 12 }}>
                      —{f.relation}→ {f.with}: {f.fact}
                    </p>
                  ))}
                  {(e.related?.chunks ?? []).map((c, i) => (
                    <p key={`c${i}`} className="hit-content mono" style={{ fontSize: 12 }}>
                      [doc] {c.document} · {c.excerpt}
                    </p>
                  ))}
                  {(e.related?.memories ?? []).map((m, i) => (
                    <p key={`m${i}`} className="hit-content mono" style={{ fontSize: 12 }}>
                      [mem] #{m.id} {m.title}
                    </p>
                  ))}
                </div>
              ),
            )}
          </div>
        ))}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Conservative recall returns stubs — the panel mirrors the agent's
// progressive disclosure: each stub expands inline with an on-demand
// fetch (memory_get / knowledge_expand / graph facts), instead of a dead
// truncated line.
// ---------------------------------------------------------------------------

function useStubFetch<T>(fetcher: () => Promise<T>) {
  const [open, setOpen] = useState(false);
  const [data, setData] = useState<T | null>(null);
  const [loading, setLoading] = useState(false);
  const toast = useToast();
  const toggle = async () => {
    if (open) {
      setOpen(false);
      return;
    }
    setOpen(true);
    if (data) return;
    setLoading(true);
    try {
      setData(await fetcher());
    } catch (e) {
      toast("err", String(e));
      setOpen(false);
    } finally {
      setLoading(false);
    }
  };
  return { open, data, loading, toggle };
}

function StubChevron({ open }: { open: boolean }) {
  return <span className="chev">{open ? "▾" : "▸"}</span>;
}

function RecallLoading() {
  return (
    <div className="recall-loading">
      <Spin size="small" />
    </div>
  );
}

function MemoryStub({ stub }: { stub: RecallResult["memories"][number] }) {
  const { t } = useI18n();
  const { open, data, loading, toggle } = useStubFetch(() => api.memoryGet(stub.id));
  return (
    <div className="recall-hit">
      <button className="recall-stub-head" onClick={toggle}>
        <span className="tag">#{stub.id}</span>
        <span className="tag">{stub.store}</span>
        {stub.score != null && <span className="muted mono">{stub.score.toFixed(2)}</span>}
        <span className="stub-text">{stub.title}</span>
        {!open && <span className="stub-afford">{t("memory.recallExpand")}</span>}
        <StubChevron open={open} />
      </button>
      <div className={open ? "recall-expand open" : "recall-expand"}>
        <div className="recall-inner">
          {loading ? (
            <RecallLoading />
          ) : data ? (
            <div className="recall-body md">
              <Markdown>{data.content}</Markdown>
            </div>
          ) : null}
        </div>
      </div>
    </div>
  );
}

/** §13-2: a generated wiki page hit — always a stub, labeled so the
 * reader can tell compiled content from sources. */
function WikiStub({ stub }: { stub: NonNullable<RecallResult["wiki"]>[number] }) {
  const { t } = useI18n();
  const { open, data, loading, toggle } = useStubFetch(() =>
    api.knowledgeExpand(stub.chunk_id),
  );
  const x = data as KnowledgeExpansion | null;
  return (
    <div className="recall-hit">
      <button className="recall-stub-head" onClick={toggle}>
        <span className="tag">wiki</span>
        <strong>{stub.title}</strong>
        {stub.stale && <span className="tag warn">{t("wiki.staleTag")}</span>}
        {!open && <span className="stub-text">{stub.summary || stub.excerpt}</span>}
        {!open && <span className="stub-afford">{t("memory.recallExpand")}</span>}
        <StubChevron open={open} />
      </button>
      <div className={open ? "recall-expand open" : "recall-expand"}>
        <div className="recall-inner">
          {loading ? (
            <RecallLoading />
          ) : x ? (
            <div className="recall-body">
              <div className="row tight" style={{ marginBottom: 6 }}>
                <span className="tag">{stub.document}</span>
                <span className="tag warn">{t("memory.recallWikiGenerated")}</span>
              </div>
              <Markdown>{x.section}</Markdown>
            </div>
          ) : null}
        </div>
      </div>
    </div>
  );
}

function KnowledgeStub({ stub }: { stub: RecallResult["knowledge"][number] }) {
  const { t } = useI18n();
  const { open, data, loading, toggle } = useStubFetch(() =>
    api.knowledgeExpand(stub.chunk_id),
  );
  const x = data as KnowledgeExpansion | null;
  const i = x ? x.section.indexOf(x.chunk) : -1;
  return (
    <div className="recall-hit">
      <button className="recall-stub-head" onClick={toggle}>
        <span className="tag">{stub.document}</span>
        {stub.score != null && <span className="muted mono">{stub.score.toFixed(2)}</span>}
        <span className="stub-text">{stub.excerpt}</span>
        {!open && <span className="stub-afford">{t("memory.recallExpand")}</span>}
        <StubChevron open={open} />
      </button>
      <div className={open ? "recall-expand open" : "recall-expand"}>
        <div className="recall-inner">
          {loading ? (
            <RecallLoading />
          ) : x ? (
            <div className="recall-body">
              {x.file ? (
                <div className="row tight" style={{ marginBottom: 6 }}>
                  <span className="tag">{x.file}</span>
                </div>
              ) : null}
              {i >= 0 ? (
                <>
                  {x.section.slice(0, i).trim() ? (
                    <Markdown>{x.section.slice(0, i)}</Markdown>
                  ) : null}
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
        </div>
      </div>
    </div>
  );
}

function EntityStub({ stub }: { stub: RecallResult["entities"][number] }) {
  const { t } = useI18n();
  const { open, data, loading, toggle } = useStubFetch(async () => {
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
      <button className="recall-stub-head" onClick={toggle}>
        <span className="tag">&{stub.id}</span>
        <strong>{stub.name}</strong>
        {stub.entity_kind && <span className="tag">{stub.entity_kind}</span>}
        {stub.summary && <span className="stub-text">{stub.summary}</span>}
        {!open && <span className="stub-afford">{t("memory.recallExpand")}</span>}
        <StubChevron open={open} />
      </button>
      <div className={open ? "recall-expand open" : "recall-expand"}>
        <div className="recall-inner">
          {loading ? (
            <RecallLoading />
          ) : data ? (
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
            </div>
          ) : null}
        </div>
      </div>
    </div>
  );
}
