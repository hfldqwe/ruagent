// Typed client for the ruagent daemon API (design §3: the panel is a
// thin client over the same local API the CLI uses).

export interface AgentInfo {
  id: string;
  name: string;
  harness: string;
  description: string;
  model: string | null;
  enabled: boolean;
}

export interface Task {
  id: string;
  title: string;
  intent: string;
  status: string;
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
  context_usage: { used: number; size: number; cost_usd: number | null } | null;
  cost_usd: number | null;
  error: string | null;
  stop_reason: string | null;
  created_at: string;
}

export interface PendingPermission {
  run_id: string;
  tool_call_id: string;
  title: string;
  raw_input: unknown;
  choices: { option_id: string; name: string; kind: string }[];
}

const BASE = "";

async function get<T>(path: string): Promise<T> {
  const resp = await fetch(`${BASE}${path}`);
  if (!resp.ok) throw new Error(`${resp.status}: ${await resp.text()}`);
  return resp.json() as Promise<T>;
}

async function post(path: string, body: unknown): Promise<Response> {
  const resp = await fetch(`${BASE}${path}`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body),
  });
  if (!resp.ok) throw new Error(`${resp.status}: ${await resp.text()}`);
  return resp;
}

export interface AgentStats {
  agent: string;
  runs: number;
  completed: number;
  failed: number;
  total_cost_usd: number;
  last_run_at: string | null;
}

export interface McpRegistry {
  servers: { name: string; command: string | null; url: string | null; inject_for: string[] | null }[];
  profiles: { name: string; servers: string[] }[];
}

export const api = {
  agents: () => get<{ agents: AgentInfo[] }>("/api/v1/agents").then((r) => r.agents),
  tasks: () => get<{ tasks: Task[] }>("/api/v1/tasks").then((r) => r.tasks),
  task: (id: string) => get<{ task: Task; runs: Run[] }>(`/api/v1/tasks/${id}`),
  createTask: (title: string, intent: string) =>
    post("/api/v1/tasks", { title, intent }).then((r) => r.json() as Promise<Task>),
  startRun: (taskId: string, agent: string | null, prompt: string) =>
    post(`/api/v1/tasks/${taskId}/runs`, { agent, prompt }).then(
      (r) => r.json() as Promise<Run>,
    ),
  pendingPermissions: () =>
    get<{ pending: PendingPermission[] }>("/api/v1/permissions").then((r) => r.pending),
  resolvePermission: (runId: string, toolCallId: string, action: string) =>
    post(`/api/v1/permissions/${runId}:${toolCallId}`, { action }),
  stats: () => get<{ agents: AgentStats[] }>("/api/v1/stats").then((r) => r.agents),
  mcp: () => get<McpRegistry>("/api/v1/mcp"),
  fanout: (taskId: string, agents: string[], prompt: string) =>
    post(`/api/v1/tasks/${taskId}/fanout`, { agents, prompt }).then(
      (r) => r.json() as Promise<{ runs: Run[] }>,
    ),
  selectRun: (runId: string) => post(`/api/v1/runs/${runId}/select`, {}),
};
