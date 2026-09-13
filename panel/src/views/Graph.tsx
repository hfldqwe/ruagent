// Graph explorer: entities, bi-temporal facts, as-of queries, neighbors.

import { useEffect, useState } from "react";
import { Button, Input, Table } from "antd";
import {
  UserOutlined,
  BankOutlined,
  AppstoreOutlined,
  CodeSandboxOutlined,
  ToolOutlined,
  TagOutlined,
} from "@ant-design/icons";
import { api, type GraphEdge, type GraphEntity } from "../api";
import { Empty, Markdown, Modal, Spinner, useToast } from "../ui";
import { dateOf, useI18n } from "../i18n";

function kindIcon(kind: string | null) {
  switch (kind) {
    case "person":
      return <UserOutlined />;
    case "org":
    case "organization":
      return <BankOutlined />;
    case "project":
      return <AppstoreOutlined />;
    case "repo":
      return <CodeSandboxOutlined />;
    case "tool":
      return <ToolOutlined />;
    default:
      return <TagOutlined />;
  }
}

export function Graph() {
  const { t } = useI18n();
  const [entities, setEntities] = useState<[GraphEntity, number][] | null>(null);
  const [query, setQuery] = useState("");
  const [selected, setSelected] = useState<GraphEntity | null>(null);
  const [creating, setCreating] = useState(false);
  const toast = useToast();

  const refresh = () =>
    api
      .graphEntities()
      .then(setEntities)
      .catch((e) => {
        toast("err", String(e));
        setEntities([]);
      });

  useEffect(() => {
    refresh();
  }, []);

  const search = async () => {
    if (!query.trim()) {
      refresh();
      return;
    }
    try {
      const hits = await api.graphSearch(query.trim());
      setEntities(hits.map((h) => [h, 0]));
    } catch (e) {
      toast("err", String(e));
    }
  };

  return (
    <div>
      <div className="view-bar">
        <h2>{t("graph.title")}</h2>
        <span className="muted">{t("graph.subtitle")}</span>
        <span className="grow" />
        <Button type="primary" onClick={() => setCreating(true)}>
          + {t("graph.newEntity")}
        </Button>
      </div>

      <div className="search-bar">
        <Input
          className="grow"
          placeholder="Search entities (name, summary)…"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onPressEnter={search}
        />
      </div>

      {selected ? (
        <EntityDetail entity={selected} onBack={() => setSelected(null)} />
      ) : entities === null ? (
        <Spinner label={`${t("graph.title")}…`} />
      ) : entities.length === 0 ? (
        <Empty
          icon="graph"
          title={t("graph.empty.title")}
          hint={t("graph.empty.hint")}
        />
      ) : (
        <div className="card">
          {entities.map(([e, factCount]) => (
            <button key={e.id} className="row-btn" onClick={() => setSelected(e)}>
              <span className="doc-icon">{kindIcon(e.kind)}</span>
              <strong>{e.name}</strong>
              {e.kind ? <span className="tag">{e.kind}</span> : null}
              <span className="muted">{factCount} facts</span>
              <span className="grow" />
              {e.summary ? <span className="muted truncated">{e.summary}</span> : null}
            </button>
          ))}
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

function EntityDetail({ entity, onBack }: { entity: GraphEntity; onBack: () => void }) {
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
    <div>
      <div className="view-bar">
        <Button type="link" onClick={onBack} style={{ paddingLeft: 0 }}>
          ← graph
        </Button>
        <h2>
          {kindIcon(entity.kind)} {entity.name}
        </h2>
        {entity.kind ? <span className="tag">{entity.kind}</span> : null}
        <span className="grow" />
        <Button type="primary" onClick={() => setAddingFact(true)}>
          + {t("graph.addFact")}
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
            placeholder="As of date (e.g. 2026-01-15)…"
            value={at}
            onChange={(e) => setAt(e.target.value)}
            onPressEnter={() => loadFacts(at)}
            onBlur={() => loadFacts(at)}
            style={{ width: 220 }}
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
              <button key={n.id} className="neighbor" onClick={onBack}>
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
        <Input autoFocus value={name} onChange={(e) => setName(e.target.value)} placeholder="Alice" />
      </label>
      <label className="field">
        <span>{t("graph.newEntity.kind")}</span>
        <Input value={kind} onChange={(e) => setKind(e.target.value)} placeholder="person / project / tool…" />
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
          placeholder="Acme"
        />
      </label>
      <label className="field">
        <span>{t("graph.addFact.relation")}</span>
        <Input value={relation} onChange={(e) => setRelation(e.target.value)} placeholder="works_at" />
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
        <Input className="mono" value={validAt} onChange={(e) => setValidAt(e.target.value)} placeholder="2026-01-15" />
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
