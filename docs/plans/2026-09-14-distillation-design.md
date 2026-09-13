# Session Distillation — Design (2026-09-14)

Borrows patterns from OpenViking's session-memory extraction flow
(docs/design-study/openviking-extraction-flow.md, AGPL — patterns only,
no code) and the dsh-import-agents session sync plugin (formats archived
in docs/design-study/). The user's requirement, verbatim:

> 会话不仅仅是因为需要会话，更是需要提取记忆，提取关系和知识图谱
> 蒸馏成知识图谱或者向量化知识库，能够进行有效召回（有效而非误召回）
> 召回两种策略：激进（分数阈值 + top-n 全文）；保守（只返回实体/关系
> 或简短描述，具体内容由 agent 通过 MCP 主动调用获取）

## Pipeline

```
sessions (auto-indexed)
   └─ distill(session)                  # daemon: distill.rs
        ├─ render transcript → prompt   # see EXTRACTION_PROMPT
        ├─ one-shot ACP agent run       # any enabled runtime (dsh locally)
        ├─ parse JSON result
        ├─ write memories               # 4 stores × namespaces, dedup by
        │                               #   similarity vs existing rows
        └─ write graph                  # find-or-create entities, add facts
```

Extraction targets (structured JSON from the distillation agent):

- `memories[]`: { store, namespace, content } — profile (user facts),
  observation, procedure (how-to), lesson
- `entities[]`: { name, kind, summary } — real-world objects (person,
  project, tool, org…), OpenViking's "entity identity" rule: merge only
  explicit aliases, distinct objects stay separate
- `relations[]`: { src, dst, relation, fact } — durable facts between
  entities

Dedup policy (OpenViking lesson): a candidate memory is skipped when an
existing row in the same store+namespace is near-duplicate (cosine ≥
0.90 on the shared embedder); otherwise inserted.

## Recall — two strategies

`memory_recall` (MCP tool) / `GET /api/v1/recall`:

- **aggressive** `?strategy=aggressive&min_score=S&top_n=N`:
  hybrid search (semantic cosine + FTS, RRF-fused) across memories and
  knowledge chunks; only results above the score threshold are returned
  WITH FULL CONTENT. For when the caller wants ready-to-use context.
- **conservative** `?strategy=conservative`:
  returns stubs only — memory titles/ids, graph entity names + relation
  one-liners, knowledge chunk ids. The agent then pulls what it actually
  needs via `memory_get(id)` / `graph_entity(id)` /
  `knowledge_chunk(id)`. Cheap context, no over-injection, agent-driven
  precision — the user's "只返回部分实体和关系，或者简短描述".

## Memory embeddings

The memories table grows `embedding BLOB` + `embedder TEXT` (migration
0008); writes embed content through the same embedder the knowledge base
uses (fastembed bge-small when available, hash offline). Semantic
search is brute-force cosine in Rust — memory counts are hundreds, not
millions; ANN is premature.

## Auto vs manual

`ruagent distill --session <key>` distills one; `--recent <n>` the n
newest undistilled. The panel exposes a 蒸馏 button per session. Auto
distill on session end is a policy flag for later (P2) — the manual
path ships first so the quality of extraction can be judged before it
runs unattended.
