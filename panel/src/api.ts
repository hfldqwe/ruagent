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
}

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

export interface Run {
  id: string;
  task_id: string;
  status: string;
  params: { agent: string; model: string | null };
  result: string | null;
  acp_session_id: string | null;
  workspace: string | null;
  context_usage: { used: number; size: number; cost_usd: number | null } | null;
  cost_usd: number | null;
  error: string | null;
  stop_reason: string | null;
  created_at: string;
  updated_at: string;
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
  source: string; // claude-code | dsh | ruagent
  title: string | null;
  project: string | null;
  ref_path: string;
  started_at: number;
  updated_at: number;
  message_count: number;
  preview: string | null;
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
  }[];
  entities: {
    id: number;
    name: string;
    entity_kind?: string;
    summary?: string;
    hint?: string;
  }[];
}

export interface McpRegistry {
  servers: { name: string; command: string | null; url: string | null; inject_for: string[] | null }[];
  profiles: { name: string; servers: string[] }[];
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

// ---------------------------------------------------------------------------
// API
// ---------------------------------------------------------------------------

export const api = {
  // agents + stats
  agents: () => get<{ agents: AgentInfo[] }>("/api/v1/agents").then((r) => r.agents),
  agentOptions: (name: string) =>
    get<AgentOptions>(`/api/v1/agents/${name}/options`),
  stats: () => get<{ agents: AgentStats[] }>("/api/v1/stats").then((r) => r.agents),
  mcp: () => get<McpRegistry>("/api/v1/mcp"),

  // tasks
  tasks: (status?: string) =>
    get<{ tasks: Task[] }>(`/api/v1/tasks${status ? `?status=${status}` : ""}`).then(
      (r) => r.tasks,
    ),
  task: (id: string) =>
    get<{ task: Task; runs: Run[]; selected_run_id: string | null }>(`/api/v1/tasks/${id}`),
  createTask: (title: string, intent: string, project?: string) =>
    post("/api/v1/tasks", { title, intent, project }).then((r) => r.json() as Promise<Task>),
  updateTaskStatus: (id: string, status: string) => send("PATCH", `/api/v1/tasks/${id}`, { status }),
  deleteTask: (id: string) => send("DELETE", `/api/v1/tasks/${id}`),

  // runs
  startRun: (taskId: string, agent: string | null, prompt: string, repo?: string) =>
    post(`/api/v1/tasks/${taskId}/runs`, { agent, prompt, repo }).then(
      (r) => r.json() as Promise<Run>,
    ),
  fanout: (taskId: string, agents: string[], prompt: string, repo?: string) =>
    post(`/api/v1/tasks/${taskId}/fanout`, { agents, prompt, repo }).then(
      (r) => r.json() as Promise<{ runs: Run[] }>,
    ),
  pipeline: (taskId: string, steps: { agent: string; prompt?: string }[]) =>
    post(`/api/v1/tasks/${taskId}/pipeline`, { steps }).then(
      (r) => r.json() as Promise<{ tasks: string[] }>,
    ),
  selectRun: (runId: string) => post(`/api/v1/runs/${runId}/select`),
  cancelRun: (runId: string) => post(`/api/v1/runs/${runId}/cancel`),

  // permissions
  pendingPermissions: () =>
    get<{ pending: PendingPermission[] }>("/api/v1/permissions").then((r) => r.pending),
  resolvePermission: (runId: string, toolCallId: string, action: string) =>
    post(`/api/v1/permissions/${runId}:${toolCallId}`, { action }),

  // memory
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

  // graph
  graphEntities: () =>
    get<{ entities: [GraphEntity, number][] }>("/api/v1/graph/entities").then((r) => r.entities),
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
  chatStart: (agent: string, model: string | null) =>
    post("/api/v1/chat", { agent, model }).then(
      (r) => r.json() as Promise<{ id: string; agent: string; model: string | null }>,
    ),
  chatList: () =>
    get<{ chats: { id: string; agent: string; model: string | null; created_at: string }[] }>(
      "/api/v1/chat",
    ).then((r) => r.chats),
  chatMessage: (id: string, text: string) =>
    post(`/api/v1/chat/${id}/messages`, { text }),
  chatModel: (id: string, model: string | null) =>
    send("PATCH", `/api/v1/chat/${id}`, { model }).then(
      (r) =>
        r.json() as Promise<{
          id: string;
          agent: string;
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
