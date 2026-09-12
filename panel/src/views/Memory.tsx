// Memory browser: stores × namespaces, supersession chains, write dialog,
// audit trail (OpenViking parity).

import { useEffect, useState } from "react";
import { api, type MemoryDiff, type MemoryRow } from "../api";
import { Empty, Modal, Spinner, dateOf, relTime, useToast } from "../ui";

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
  const [counts, setCounts] = useState<[string, string, number][] | null>(null);
  const [store, setStore] = useState<Store>("observation");
  const [namespace, setNamespace] = useState("user");
  const [memories, setMemories] = useState<MemoryRow[] | null>(null);
  const [writing, setWriting] = useState(false);
  const [tab, setTab] = useState<"browse" | "audit">("browse");
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
        <h2>Memory</h2>
        <span className="muted">
          what the platform remembers — every agent reads this via MCP
        </span>
        <span className="grow" />
        <div className="seg">
          <button className={tab === "browse" ? "on" : ""} onClick={() => setTab("browse")}>
            Browse
          </button>
          <button className={tab === "audit" ? "on" : ""} onClick={() => setTab("audit")}>
            Audit log
          </button>
        </div>
        {tab === "browse" && (
          <button className="primary" onClick={() => setWriting(true)}>
            + Write Memory
          </button>
        )}
      </div>

      {tab === "audit" ? (
        <AuditView />
      ) : (
        <>
          <div className="filter-bar">
            <div className="seg">
              {STORES.map((s) => (
                <button
                  key={s}
                  className={store === s ? "on" : ""}
                  onClick={() => {
                    setStore(s);
                    setNamespace(NAMESPACES[s][0]);
                  }}
                >
                  {s}
                </button>
              ))}
            </div>
            <select value={namespace} onChange={(e) => setNamespace(e.target.value)}>
              {nsOptions.map((ns) => (
                <option key={ns} value={ns}>
                  {ns}
                </option>
              ))}
            </select>
            <NewNamespaceInput
              store={store}
              onCreated={(ns) => {
                setNamespace(ns);
              }}
            />
          </div>

          {memories === null ? (
            <Spinner label="Loading memories…" />
          ) : memories.length === 0 ? (
            <Empty
              icon="🧠"
              title={`No ${store} memories in ${namespace}`}
              hint="Agents write here via the memory_write MCP tool; you can also write manually."
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
  const [value, setValue] = useState("");
  const [open, setOpen] = useState(false);
  const prefix = store === "profile" ? "" : store === "observation" ? "project:" : "global";
  return (
    <span className="muted">
      {prefix ? (
        open ? (
          <span className="row tight">
            <input
              autoFocus
              className="mono sm"
              placeholder={`${prefix}…`}
              value={value}
              onChange={(e) => setValue(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter" && value.trim()) {
                  onCreated(`${prefix}${value.trim()}`);
                  setValue("");
                  setOpen(false);
                }
                if (e.key === "Escape") setOpen(false);
              }}
              style={{ width: 140 }}
            />
          </span>
        ) : (
          <button className="link" onClick={() => setOpen(true)}>
            + namespace
          </button>
        )
      ) : null}
    </span>
  );
}

function MemoryCard({ memory, onChanged }: { memory: MemoryRow; onChanged: () => void }) {
  const [editing, setEditing] = useState(false);
  const superseded = memory.superseded_at != null;

  return (
    <div className={superseded ? "card memory-card superseded" : "card memory-card"}>
      <div className="row tight">
        <span className="tag">{memory.store}</span>
        <span className="tag">{memory.namespace}</span>
        {superseded ? (
          <span className="tag warn">superseded {dateOf(memory.superseded_at!)}</span>
        ) : (
          <span className="tag ok">current</span>
        )}
        {memory.supersedes != null ? (
          <span className="muted mono">replaces #{memory.supersedes}</span>
        ) : null}
        <span className="grow" />
        <span className="muted time" title={memory.updated_at}>
          {relTime(memory.updated_at)} · #{memory.id}
        </span>
      </div>
      <p className={superseded ? "memory-content old" : "memory-content"}>{memory.content}</p>
      {!superseded && (
        <div className="row tight">
          <button className="link" onClick={() => setEditing(true)}>
            supersede
          </button>
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
  const toast = useToast();
  return (
    <Modal title={`Supersede memory #${memory.id}`} onClose={onClose}>
      <p className="muted">
        The old content stays in history (audit log keeps the trail); the new content becomes
        current in <code>{memory.store}/{memory.namespace}</code>.
      </p>
      <textarea rows={4} value={content} onChange={(e) => setContent(e.target.value)} />
      <div className="row end">
        <button
          className="primary"
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
          Supersede
        </button>
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
  const toast = useToast();
  return (
    <Modal title="Write memory" onClose={onClose}>
      <p className="muted">
        One concise fact per write. This memory will be injected into future runs (bounded) and
        is searchable by every agent via <code>memory_search</code>.
      </p>
      <label className="field">
        <span>
          Store / namespace (governed: profile=user-only, procedure/lesson=project/global)
        </span>
        <div className="row tight mono">
          <span className="tag">{store}</span>
          <span className="tag">{namespace}</span>
        </div>
      </label>
      <label className="field">
        <span>Content</span>
        <textarea
          autoFocus
          rows={4}
          value={content}
          onChange={(e) => setContent(e.target.value)}
          placeholder="the deploy script lives in scripts/release.sh"
        />
      </label>
      <div className="row end">
        <button
          className="primary"
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
          Write
        </button>
      </div>
    </Modal>
  );
}

function AuditView() {
  const [diffs, setDiffs] = useState<MemoryDiff[] | null>(null);
  useEffect(() => {
    api.memoryDiffs(200).then(setDiffs).catch(() => setDiffs([]));
  }, []);
  if (!diffs) return <Spinner label="Loading audit log…" />;
  if (diffs.length === 0)
    return <Empty icon="📜" title="No memory writes yet" hint="Every write decision lands here." />;
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
