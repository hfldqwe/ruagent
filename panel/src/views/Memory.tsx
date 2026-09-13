// Memory browser: stores × namespaces, supersession chains, write dialog,
// audit trail (OpenViking parity).

import { Button, Input, Segmented, Select } from "antd";
import { useEffect, useState } from "react";
import { api, type MemoryDiff, type MemoryRow } from "../api";
import { Empty, Modal, RelTime, Spinner, useToast } from "../ui";
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
        <h2>{t("memory.title")}</h2>
        <span className="muted">{t("memory.subtitle")}</span>
        <span className="grow" />
        <Segmented
          value={tab}
          onChange={(v) => setTab(v as "browse" | "audit")}
          options={[
            { value: "browse", label: t("memory.browse") },
            { value: "audit", label: t("memory.audit") },
          ]}
        />
        {tab === "browse" && (
          <Button type="primary" onClick={() => setWriting(true)}>
            + {t("memory.write")}
          </Button>
        )}
      </div>

      {tab === "audit" ? (
        <AuditView />
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
