// Typed client for the ruagent daemon API (design §3: the panel is a
// thin client over the same local API the CLI uses).

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface AgentInfo {
  id: string;
  name: string;
  harness: string;
  description: string;
  model: string | null;
  enabled: boolean;
  models?: string[];
  runtime?: string | null;
  runtimes?: string[];
  prompt?: string | null;
  /** Canonical session-option defaults ([agent.X.options]): `mode`
   *  (permission) and `effort` (thinking), applied at chat start. */
  options?: Record<string, string>;
  /** Two-layer model: a card with a prompt or runtime refs is a "role";
   *  a bare harness instance (legacy entry or [runtime.*] card) is a
   *  "runtime" — claude-code/dsh/opencode, the execution backends. */
  kind?: "role" | "runtime";
  command?: string | null;
}

/** Two-layer model (user ruling 2026-09-17): a card with a role prompt or
 *  runtime references is a ROLE; a bare harness instance (legacy entry or
 *  [runtime.*] card) IS a runtime. `kind` comes from the API; the shape
 *  fallback covers older daemons. */
export const isRoleAgent = (a: AgentInfo) =>
  a.kind === "role" ||
  (a.kind == null && (!!a.prompt || !!a.runtime || (a.runtimes?.length ?? 0) > 0));

export interface OptionChoice {
  value: string;
  name: string;
  description?: string;
  group?: string;
}

/** One session config option an agent advertises (model, reasoning
 * effort, permission mode, …). */
export interface SessionOptionInfo {
  id: string;
  name: string;
  category?: string;
  choices: OptionChoice[];
  current?: string | null;
}

export interface AgentOptions {
  agent: string;
  options: SessionOptionInfo[];
  /** true = served from the persisted catalog (no probe); false = a
   *  fresh probe just ran. */
  cached?: boolean;
  /** Epoch ms of the persisted catalog copy. */
  updated_at?: number;
}

/** One recorded conversation (chats table) — the history drawer. */
export interface ChatHistoryEntry {
  id: string;
  agent: string;
  runtime: string | null;
  model: string | null;
  title: string | null;
  created_at: number;
  updated_at: number;
  /** The daemon is still holding this chat (streams live). */
  active: boolean;
  /** Project working directory the chat runs in (workspace grouping). */
  cwd?: string | null;
  message_count: number | null;
  preview: string | null;
  /** Sessions-index key of the transcript (the shared viewer). */
  session_key?: string;
}

export interface AgentStats {
  agent: string;
  runs: number;
  completed: number;
  failed: number;
  total_cost_usd: number;
  last_run_at: string | null;
}

export interface Task {
  id: string;
  title: string;
  intent: string;
  status: string;
  project: string | null;
  created_at: string;
  updated_at: string;
}

/** The judge view over a task's fan-out (design §5.2/§5.3): the newest
 *  judge run, its verdict re-parsed from the reply, and its rationale.
 *  The authoritative selection lives on the task itself. */
export interface Judgement {
  judge_run_id: string;
  judge_run_status: string;
  winner_run_id: string | null;
  rationale: string | null;
}

export interface TaskDetail {
  task: Task;
  runs: Run[];
  selected_run_id: string | null;
  /** "human" | "agent:<name>" | null (no selection yet). */
  selected_by: string | null;
  judgement: Judgement | null;
  /** The winner's worktree still exists — its branch can be landed
   *  (merged back into the main repo). */
  landable: boolean;
}

export interface Run {
  id: string;
  task_id: string;
  status: string;
  params: {
    agent: string;
    model: string | null;
    /** Canonical session-option defaults (mode / effort). */
    options?: Record<string, string>;
    /** The original launch prompt (None on pre-field rows). */
    prompt?: string | null;
  };
  result: string | null;
  acp_session_id: string | null;
  workspace: string | null;
  context_usage: { used: number; size: number; cost_usd: number | null } | null;
  cost_usd: number | null;
  error: string | null;
  stop_reason: string | null;
  created_at: string;
  updated_at: string;
  /** Parked on the permission inbox — the run is waiting on a human
   *  (issue #34). Null when not waiting. */
  waiting_permission?: { tool_call_id: string; title: string } | null;
}

export interface PendingPermission {
  run_id: string;
  tool_call_id: string;
  title: string;
  raw_input: unknown;
  choices: { option_id: string; name: string; kind: string }[];
}
export interface SessionRecord {
  key: string;
  source: string; // claude-code | dsh | ruagent | opencode | codex
  title: string | null;
  project: string | null;
  ref_path: string;
  started_at: number;
  updated_at: number;
  message_count: number;
  preview: string | null;
  /** ruagent chats: the agent (role/runtime) the conversation was with. */
  agent?: string;
}

export interface RecallResult {
  strategy: string;
  memories: {
    kind: string;
    id: number;
    store: string;
    namespace: string;
    content?: string;
    title?: string;
    score?: number;
    hint?: string;
  }[];
  knowledge: {
    kind: string;
    chunk_id: number;
    document: string;
    content?: string;
    excerpt?: string;
    score?: number;
    hint?: string;
  }[];
  /** §13-2: generated wiki pages — a separate section, never mixed
   * into knowledge, always conservative stubs. */
  wiki: {
    kind: string;
    slug: string;
    chunk_id: number;
    document: string;
    title: string;
    summary: string;
    excerpt?: string;
    stale: boolean;
    hint?: string;
  }[];
  entities: {
    id: number;
    name: string;
    entity_kind?: string;
    summary?: string;
    hint?: string;
    facts?: { relation: string; with: string; fact: string }[];
    related?: {
      chunks: { chunk_id: number; document: string; excerpt: string }[];
      memories: { id: number; store: string; namespace: string; title: string }[];
      /** §12-2: wiki pages citing this entity (generated — labeled). */
      wiki?: { slug: string; title: string; stale: boolean; hint?: string }[];
    } | null;
  }[];
}

export interface McpRegistry {
  servers: {
    name: string;
    command: string | null;
    url: string | null;
    inject_for: string[] | null;
    /** Live health (issue #24); null = never checked. */
    health?: {
      state: "ok" | "down";
      tools: number | null;
      latency_ms: number;
      error: string | null;
      checked_at: number;
    } | null;
  }[];
  profiles: { name: string; servers: string[] }[];
}

export interface DistillPolicy {
  auto: boolean;
  graph?: boolean | null;
  agent: string | null;
  language: string | null;
  prompt: string | null;
  builtin_prompt: string;
}

export interface MemoryRow {
  id: number;
  store: string;
  namespace: string;
  content: string;
  confidence: number;
  supersedes: number | null;
  superseded_at: string | null;
  created_at: string;
  updated_at: string;
}

export interface MemoryDiff {
  id: number;
  ts: string;
  op: string;
  mem_store: string | null;
  namespace: string | null;
  before: string | null;
  after: string | null;
  reason: string | null;
}

export interface KnowledgeDocument {
  id: number;
  name: string;
  source: string;
  chunk_count: number;
  created_at: string;
}

export interface SearchHit {
  chunk_id: number;
  document: string;
  content: string;
  score: number;
}

/** One recorded chunk edit (old/new pair, newest first). */
export interface KnowledgeRevision {
  id: number;
  chunk_id: number;
  document_name: string;
  old_content: string;
  new_content: string;
  edited_at: string;
}

/** Result of a chunk edit or a revision rollback. */
export interface KnowledgeEditOutcome {
  revision: number;
  document: string;
  chunks: number;
}

/** One recall call in the usage log (M6 tuning dataset): per-section
 * counts plus RAW top scores before the relevance filters. */
export interface RecallLogRow {
  ts: string;
  query: string;
  strategy: string;
  top_n: number;
  memories: number;
  knowledge: number;
  wiki: number;
  entities: number;
  top_memory_score: number | null;
  top_knowledge_score: number | null;
}

/** A hit expanded into its parent section. */
export interface KnowledgeExpansion {
  chunk_id: number;
  document: string;
  section: string;
  chunk: string;
  file: string | null;
}

/** One pass of the file → index rebuild. */
export interface KnowledgeRebuildReport {
  indexed: number;
  unchanged: number;
  removed: number;
  errors: number;
}

// ---------------------------------------------------------------------------
// Wiki (M2 read APIs + the build surface)
// ---------------------------------------------------------------------------

/** One wiki page in the inventory: stale = a cited source's hash
 * drifted; edited = hand-edited since its last build (§13-3). */
export interface WikiPageInfo {
  slug: string;
  title: string;
  summary: string;
  aliases: string[];
  entities: string[];
  sources: string[];
  stale: boolean;
  edited: boolean;
  links_out: number;
  links_in: number;
}

/** The link graph: broken = linked but missing (wanted pages). */
export interface WikiLinks {
  nodes: string[];
  edges: { src: string; dst: string }[];
  broken: string[];
  orphans: string[];
}

export interface WikiBuild {
  id: number;
  scope: string;
  status: string; // planned|running|done|failed
  dry_run: boolean;
  agent: string;
  pages_planned: number;
  pages_written: number;
  pages_failed: number;
  error: string | null;
  started_at: string;
  finished_at: string | null;
}

/** One page of a build plan (create|update|delete|keep). */
export interface WikiPagePlan {
  slug: string;
  title: string;
  summary?: string;
  aliases?: string[];
  entities?: string[];
  sources?: string[];
  action: string;
}

/** POST /wiki/build result — dry runs carry the plan for review. */
export interface WikiBuildStarted {
  build_id: number;
  status: string;
  agent: string;
  pages_planned: number;
  plan?: WikiPagePlan[];
  notes?: string;
}

export interface WikiBuildDetail {
  build: WikiBuild & { plan: WikiPagePlan[] | null };
  pages: { slug: string; action: string; status: string; error: string | null }[];
}

export interface GraphEntity {
  id: number;
  name: string;
  kind: string | null;
  summary: string | null;
}

export interface GraphEdge {
  id: number;
  src: number;
  dst: number;
  relation: string;
  fact_text: string;
  valid_at: string;
  invalid_at: string | null;
  source_episode: number | null;
}

// ---------------------------------------------------------------------------
// HTTP
// ---------------------------------------------------------------------------

const BASE = "";

async function get<T>(path: string): Promise<T> {
  const resp = await fetch(`${BASE}${path}`);
  if (!resp.ok) throw new Error((await resp.text()) || `${resp.status}`);
  return resp.json() as Promise<T>;
}

async function send(method: string, path: string, body?: unknown): Promise<Response> {
  const resp = await fetch(`${BASE}${path}`, {
    method,
    headers: { "content-type": "application/json" },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  if (!resp.ok) throw new Error((await resp.text()) || `${resp.status}`);
  return resp;
}

const post = (path: string, body?: unknown) => send("POST", path, body);
const put = (path: string, body?: unknown) => send("PUT", path, body);
const patch = (path: string, body?: unknown) => send("PATCH", path, body);

/** GET a non-JSON body (the raw markdown endpoint). */
async function getText(path: string): Promise<string> {
  const resp = await fetch(`${BASE}${path}`);
  if (!resp.ok) throw new Error((await resp.text()) || `${resp.status}`);
  return resp.text();
}

// ---------------------------------------------------------------------------
// API
// ---------------------------------------------------------------------------

export const api = {
  // agents + stats
  agents: () => get<{ agents: AgentInfo[] }>("/api/v1/agents").then((r) => r.agents),
  agentOptions: (name: string, refresh?: boolean, runtime?: string) =>
    get<AgentOptions>(
      `/api/v1/agents/${name}/options` +
        `${refresh || runtime ? "?" : ""}${refresh ? "refresh=1" : ""}` +
        `${refresh && runtime ? "&" : ""}${runtime ? `runtime=${encodeURIComponent(runtime)}` : ""}`,
    ),
  stats: () => get<{ agents: AgentStats[] }>("/api/v1/stats").then((r) => r.agents),
  mcp: () => get<McpRegistry>("/api/v1/mcp"),

  // tasks
  tasks: (status?: string) =>
    get<{ tasks: Task[] }>(`/api/v1/tasks${status ? `?status=${status}` : ""}`).then(
      (r) => r.tasks,
    ),
  task: (id: string) => get<TaskDetail>(`/api/v1/tasks/${id}`),
  createTask: (title: string, intent: string, project?: string) =>
    post("/api/v1/tasks", { title, intent, project }).then((r) => r.json() as Promise<Task>),
  updateTaskStatus: (id: string, status: string) => send("PATCH", `/api/v1/tasks/${id}`, { status }),
  deleteTask: (id: string) => send("DELETE", `/api/v1/tasks/${id}`),

  // runs
  startRun: (
    taskId: string,
    agent: string | null,
    prompt: string,
    repo?: string,
    options?: Record<string, string>,
  ) =>
    post(`/api/v1/tasks/${taskId}/runs`, { agent, prompt, repo, options }).then(
      (r) => r.json() as Promise<Run>,
    ),
  fanout: (
    taskId: string,
    agents: string[],
    prompt: string,
    repo?: string,
    options?: Record<string, string>,
  ) =>
    post(`/api/v1/tasks/${taskId}/fanout`, { agents, prompt, repo, options }).then(
      (r) => r.json() as Promise<{ runs: Run[] }>,
    ),
  pipeline: (taskId: string, steps: { agent: string; prompt?: string }[]) =>
    post(`/api/v1/tasks/${taskId}/pipeline`, { steps }).then(
      (r) => r.json() as Promise<{ tasks: string[] }>,
    ),
  // registry editing (runtimes + roles write straight to agents.toml
  // through the daemon, then hot-reload)
  createRuntime: (body: {
    name: string;
    harness?: string;
    command?: string;
    description?: string;
    mcp_profile?: string;
    models?: string[];
  }) => post("/api/v1/runtimes", body).then((r) => r.json() as Promise<AgentInfo>),
  updateRuntime: (name: string, body: Partial<AgentInfo>) =>
    send("PATCH", `/api/v1/runtimes/${encodeURIComponent(name)}`, body),
  deleteRuntime: (name: string) => send("DELETE", `/api/v1/runtimes/${encodeURIComponent(name)}`),
  createAgent: (body: {
    name: string;
    prompt: string;
    description?: string;
    model?: string;
    runtimes?: string[];
    runtime?: string;
    options?: Record<string, string>;
  }) => post("/api/v1/agents", body).then((r) => r.json() as Promise<AgentInfo>),
  updateAgent: (name: string, body: Record<string, unknown>) =>
    send("PATCH", `/api/v1/agents/${encodeURIComponent(name)}`, body),
  deleteAgent: (name: string) => send("DELETE", `/api/v1/agents/${encodeURIComponent(name)}`),
  judgeTask: (taskId: string, agent: string) =>
    post(`/api/v1/tasks/${taskId}/judge`, { agent }).then(
      (r) => r.json() as Promise<{ judge_run: Run }>,
    ),
  selectRun: (runId: string) => post(`/api/v1/runs/${runId}/select`),
  /** Land the task's selected winner: merge its worktree branch into
   *  the main repo. Resolves with the post-merge HEAD short hash. */
  landTask: (taskId: string) =>
    post(`/api/v1/tasks/${taskId}/land`).then((r) => r.json() as Promise<{ landed: string }>),
  cancelRun: (runId: string) => post(`/api/v1/runs/${runId}/cancel`),
  retryRun: (runId: string) =>
    post(`/api/v1/runs/${runId}/retry`).then((r) => r.json() as Promise<Run>),

  // permissions
  pendingPermissions: () =>
    get<{ pending: PendingPermission[] }>("/api/v1/permissions").then((r) => r.pending),
  resolvePermission: (
    runId: string,
    toolCallId: string,
    answer: { action: "allow" | "reject" | "cancel" } | { option_id: string },
  ) => post(`/api/v1/permissions/${runId}:${toolCallId}`, answer),

  // memory
  distillPolicy: () => get<DistillPolicy>("/api/v1/distill"),
  setDistillPolicy: (body: {
    auto: boolean;
    graph?: boolean;
    agent: string;
    language: string;
    prompt: string;
  }) => send("PUT", "/api/v1/distill", body),
  memoryList: (store: string, namespace: string) =>
    get<{ memories: MemoryRow[]; counts: [string, string, number][] }>(
      `/api/v1/memory/list?store=${store}&namespace=${encodeURIComponent(namespace)}`,
    ),
  memorySearch: (q: string) =>
    get<{ hits: MemoryRow[] }>(`/api/v1/memory/search?q=${encodeURIComponent(q)}`).then(
      (r) => r.hits,
    ),
  memoryWrite: (store: string, namespace: string, content: string) =>
    post("/api/v1/memory/write", { store, namespace, content }).then(
      (r) => r.json() as Promise<{ outcome: string }>,
    ),
  memorySupersede: (id: number, newContent: string) =>
    post("/api/v1/memory/supersede", { id, new_content: newContent }).then(
      (r) => r.json() as Promise<{ outcome: string }>,
    ),
  memoryGet: (id: number) =>
    get<{ memory: MemoryRow }>(`/api/v1/memory/${id}`).then((r) => r.memory),
  memoryDiffs: (limit = 100) =>
    get<{ diffs: MemoryDiff[] }>(`/api/v1/memory/diffs?limit=${limit}`).then((r) => r.diffs),

  // knowledge
  knowledgeDocs: () =>
    get<{ documents: KnowledgeDocument[]; embedder: string }>("/api/v1/knowledge/documents"),
  knowledgeChunks: (id: number) =>
    get<{ chunks: [number, string][] }>(`/api/v1/knowledge/documents/${id}`).then(
      (r) => r.chunks,
    ),
  knowledgeDelete: (id: number) => send("DELETE", `/api/v1/knowledge/documents/${id}`),
  knowledgeIngest: (name: string, content: string) =>
    post("/api/v1/knowledge/ingest", { name, content }).then(
      (r) => r.json() as Promise<{ chunks: number }>,
    ),
  knowledgeSearch: (q: string) =>
    get<{ hits: SearchHit[] }>(`/api/v1/knowledge/search?q=${encodeURIComponent(q)}`).then(
      (r) => r.hits,
    ),
  // knowledge — markdown truth-source editing
  knowledgeRaw: (name: string) =>
    getText(`/api/v1/knowledge/raw/${encodeURIComponent(name)}`),
  knowledgeSave: (name: string, content: string) =>
    put(`/api/v1/knowledge/raw/${encodeURIComponent(name)}`, { content }).then(
      (r) => r.json() as Promise<{ chunks: number; file: string }>,
    ),
  knowledgeEditChunk: (id: number, content: string) =>
    patch(`/api/v1/knowledge/chunks/${id}`, { content }).then(
      (r) => r.json() as Promise<KnowledgeEditOutcome>,
    ),
  knowledgeChunkRevisions: (id: number) =>
    get<{ revisions: KnowledgeRevision[] }>(`/api/v1/knowledge/chunks/${id}/revisions`).then(
      (r) => r.revisions,
    ),
  knowledgeRollback: (revisionId: number) =>
    post(`/api/v1/knowledge/revisions/${revisionId}/rollback`).then(
      (r) => r.json() as Promise<KnowledgeEditOutcome>,
    ),
  knowledgeExpand: (chunkId: number) =>
    get<KnowledgeExpansion>(`/api/v1/knowledge/expand/${chunkId}`),
  knowledgeRebuild: () =>
    post("/api/v1/knowledge/rebuild").then(
      (r) => r.json() as Promise<{ rebuild: KnowledgeRebuildReport }>,
    ),

  // wiki — M2 read APIs + builds
  wikiPages: () =>
    get<{ pages: WikiPageInfo[] }>("/api/v1/knowledge/wiki/pages").then((r) => r.pages),
  wikiLinks: () => get<WikiLinks>("/api/v1/knowledge/wiki/links"),
  wikiBuilds: (limit = 20) =>
    get<{ builds: WikiBuild[] }>(`/api/v1/knowledge/wiki/builds?limit=${limit}`).then(
      (r) => r.builds,
    ),
  wikiBuild: (id: number) => get<WikiBuildDetail>(`/api/v1/knowledge/wiki/builds/${id}`),
  wikiBuildStart: (req: { scope?: string; dry_run?: boolean; agent?: string }) =>
    post("/api/v1/knowledge/wiki/build", req).then((r) => r.json() as Promise<WikiBuildStarted>),
  wikiBuildConfirm: (buildId: number, agent?: string) =>
    post("/api/v1/knowledge/wiki/build", { confirm_plan: buildId, agent }).then(
      (r) => r.json() as Promise<WikiBuildStarted>,
    ),

  // graph
  graphEntities: () =>
    get<{ entities: [GraphEntity, number][] }>("/api/v1/graph/entities").then((r) => r.entities),
  /** The whole entity list (graphEntities caps at the daemon default of 50). */
  graphEntitiesAll: (limit = 500) =>
    get<{ entities: [GraphEntity, number][] }>(`/api/v1/graph/entities?limit=${limit}`).then(
      (r) => r.entities,
    ),
  graphSearch: (q: string) =>
    get<{ entities: GraphEntity[] }>(`/api/v1/graph/search?q=${encodeURIComponent(q)}`).then(
      (r) => r.entities,
    ),
  graphEntity: (id: number) =>
    get<{ facts: GraphEdge[] }>(`/api/v1/graph/entity/${id}`).then((r) => r.facts),
  graphNeighbors: (id: number, hops = 2) =>
    get<{ neighbors: [GraphEntity, number][] }>(
      `/api/v1/graph/entity/${id}/neighbors?hops=${hops}`,
    ).then((r) => r.neighbors),
  graphFacts: (id: number, at?: string) =>
    get<{ facts: GraphEdge[] }>(
      `/api/v1/graph/entity/${id}/facts${at ? `?at=${encodeURIComponent(at)}` : ""}`,
    ).then((r) => r.facts),
  graphCreateEntity: (name: string, kind?: string, summary?: string) =>
    post("/api/v1/graph/entity", { name, kind, summary }).then(
      (r) => r.json() as Promise<{ id: number }>,
    ),
  graphAddFact: (fact: {
    src: number;
    dst: number;
    relation: string;
    fact_text: string;
    valid_at?: string;
  }) => post("/api/v1/graph/fact", fact).then((r) => r.json() as Promise<{ id: number }>),

  // chat
  chatStart: (agent: string, model: string | null, cwd?: string) =>
    post("/api/v1/chat", { agent, model, cwd: cwd || undefined }).then(
      (r) =>
        r.json() as Promise<{
          id: string;
          agent: string;
          runtime: string;
          model: string | null;
        }>,
    ),
  chatList: () =>
    get<{ chats: { id: string; agent: string; model: string | null; created_at: string }[] }>(
      "/api/v1/chat",
    ).then((r) => r.chats),
  chatsHistory: (agent?: string, limit = 50) =>
    get<{ chats: ChatHistoryEntry[] }>(
      `/api/v1/chats?${agent ? `agent=${encodeURIComponent(agent)}&` : ""}limit=${limit}`,
    ).then((r) => r.chats),
  chatMessage: (id: string, text: string) =>
    post(`/api/v1/chat/${id}/messages`, { text }),
  chatStop: (id: string) => post(`/api/v1/chat/${id}/stop`),
  chatResume: (id: string) =>
    post(`/api/v1/chat/${id}/resume`).then(
      (r) =>
        r.json() as Promise<{
          id: string;
          agent: string;
          runtime: string;
          model: string | null;
        }>,
    ),
  chatHandoff: (id: string, agent: string) =>
    post(`/api/v1/chat/${id}/handoff`, { agent }).then(
      (r) =>
        r.json() as Promise<{
          id: string;
          agent: string;
          runtime: string;
          model: string | null;
        }>,
    ),
  chatModel: (id: string, model: string | null, runtime?: string) =>
    send("PATCH", `/api/v1/chat/${id}`, { model, runtime }).then(
      (r) =>
        r.json() as Promise<{
          id: string;
          agent: string;
          runtime: string;
          model: string | null;
          switched: "live" | "restarted";
        }>,
    ),
  chatSetOption: (id: string, optionId: string, value: string) =>
    post(`/api/v1/chat/${id}/options`, { id: optionId, value }).then(
      (r) => r.json() as Promise<{ options: SessionOptionInfo[] }>,
    ),
  chatClose: (id: string) => send("DELETE", `/api/v1/chat/${id}`),

  // session history (auto-synced)
  sessions: () =>
    get<{ sessions: SessionRecord[] }>("/api/v1/sessions").then((r) => r.sessions),
  sessionMessages: (key: string) =>
    get<{ messages: { role: string; text: string; ts: number }[] }>(
      `/api/v1/sessions/${key}`,
    ).then((r) => r.messages),

  // distillation + recall
  sessionDistill: (key: string) =>
    post(`/api/v1/sessions/${key}/distill`).then(
      (r) =>
        r.json() as Promise<{
          distilled: {
            memories_written: number;
            memories_skipped: number;
            entities_written: number;
            relations_written: number;
            agent: string;
          };
        }>,
    ),
  recall: (q: string, conservative: boolean, topN = 5) =>
    get<RecallResult>(
      `/api/v1/recall?q=${encodeURIComponent(q)}&strategy=${conservative ? "conservative" : "aggressive"}&top_n=${topN}`,
    ),
  recallLog: (limit = 50) =>
    get<{ log: RecallLogRow[] }>(`/api/v1/recall/log?limit=${limit}`).then(
      (r) => r.log,
    ),

  // skills
  skills: () =>
    get<{ skills: { name: string; description: string; source: string; path: string }[] }>(
      "/api/v1/skills",
    ).then((r) => r.skills),
  skillsSync: () =>
    post("/api/v1/skills/sync").then(
      (r) => r.json() as Promise<{ installed: number; skipped: number }>,
    ),
};
