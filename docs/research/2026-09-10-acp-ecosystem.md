# ACP Ecosystem Research — Agent Client Protocol & Coding-Agent Integration

**Date:** 2026-09-10
**Scope:** Zed's Agent Client Protocol (ACP) spec + ecosystem, ACP support matrix for coding-agent CLIs, "DeepSeek Harness" identification, multi-agent ACP orchestrators, and disambiguation vs. IBM BeeAI ACP / Google A2A.
**Method:** Live fetches of agentclientprotocol.com (spec, agents/clients/registry pages, announcements), the ACP registry JSON (`cdn.agentclientprotocol.com`), GitHub API (repo metadata, READMEs, file trees), npm registry, and web search.

---

## Executive summary

- **ACP (Agent Client Protocol, agentclientprotocol.com)** is the de-facto standard for a **client (editor/UI) driving a coding agent**: JSON-RPC 2.0 over stdio, agent runs as a subprocess. Started by Zed (Sept 2025), now jointly governed by **Zed + JetBrains** under the `agentclientprotocol` GitHub org. **v1 is stable; v2 is in draft (July 20, 2026).**
- The ecosystem is large: **40 agents in the official curated registry**, plus ~150+ clients (editors, TUIs, desktop/mobile apps, messaging bridges, frameworks).
- **Claude Code** connects via the adapter `npx @agentclientprotocol/claude-agent-acp` (renamed from `@zed-industries/claude-code-acp`, now deprecated). **Codex** via `npx @agentclientprotocol/codex-acp`. **Gemini CLI, OpenCode, Qwen Code, Cursor, GitHub Copilot CLI, Cline, Goose, Junie, Kimi, Grok** etc. all speak ACP natively (exact commands below). **Crush (Charmbracelet) does NOT support ACP** as of today.
- **"DeepSeek Harness" is real and is called `dsh`**: DeepSeek AI's official open-source agent harness (github.com/deepseek-ai/deepseek-harness, ~219k stars, released Aug 2026, developer preview). It has **native ACP support** as an automation-only server (`dsh --profile acp`), **and** ships an ACP *client* (`@deepseek-ai/dsh-subagent-acp`) that can drive any other ACP agent as an isolated subagent.
- **Multi-agent ACP orchestrators already exist** at every altitude: headless CLI building blocks (acpx, 3.2k stars), desktop multi-agent workbenches (Codeg 3.4k, CompozyOS 2.7k), team-chat orchestrators (AgentConnect 1.4k), and framework-level composition (Mastra `@mastra/acp`, DSH's own subagent-acp).
- **Name-collision warning:** "ACP" also refers to IBM BeeAI's *Agent Communication Protocol* (agent-to-agent, REST). That project **has been merged into Google's A2A under the Linux Foundation**. For a client driving coding agents, **Zed's Agent Client Protocol is the one that matters**.

---

## 1. The Agent Client Protocol (Zed) — spec summary

Repo: `github.com/agentclientprotocol/agent-client-protocol` (formerly `zed-industries/agent-client-protocol`, which now redirects; 4,205 stars, actively developed as of 2026-09-10). Spec site: **agentclientprotocol.com**. Tagline: *"A protocol for connecting any editor to any agent."* It is explicitly modeled on LSP: standardize the editor↔agent boundary so any ACP client can drive any ACP agent.

### Transport

- **Local (the norm):** agent runs as a **subprocess of the client**, speaking **JSON-RPC 2.0 over stdio** (newline-delimited JSON). All file paths MUST be absolute; line numbers are 1-based.
- **Remote (work in progress):** cloud-hosted agents over HTTP or WebSocket. A **Transports working group** (announced Apr 22, 2026) is drafting an RFD for WebSocket + HTTP transports.
- Reuses **MCP's JSON representations** where possible, adds agentic-coding UX types (diffs, tool-call progress, plans). User-readable text is Markdown by default.
- Extensibility: `_meta` fields, custom methods prefixed with `_`, custom capabilities advertised at initialize. v2 additionally allows unknown enum variants with a `_` prefix.

### Lifecycle (v1)

```
client spawns agent subprocess (stdio)
  → initialize            (version + capability negotiation)
  → [authenticate]        (if agent advertises authMethods)
  → session/new           (cwd + mcpServers [+ additionalDirectories])  → sessionId
  → session/prompt        (ContentBlock[]: text, resource_link, image, …)
      ← session/update notifications (streaming):
            user_message_chunk / agent_message_chunk / thought (with messageId)
            plan (entries with priority + status)
            tool_call / tool_call_update (kind, status, content, locations)
            usage_update (tokens used/size, cost)
            available_commands_update, mode_change
      ← session/request_permission   (agent→client method; see below)
      → session/cancel       (client→agent notification; agent must answer
                              the prompt with stopReason "cancelled")
  ← prompt response { stopReason: end_turn | max_tokens | max_requests |
                      refusal | cancelled }
  → session/load (replay history via session/update; requires loadSession cap)
  → session/resume (reconnect without replay; requires sessionCapabilities.resume)
  → session/close (cancel + free resources; requires sessionCapabilities.close)
  → session/list, session/delete (optional capabilities)
```

- **Version negotiation:** `protocolVersion` is a single integer MAJOR version. Client sends the latest it supports; agent echoes it or replies with the latest it supports; client closes if unsupported.
- **Capabilities:** all optional; omitted = unsupported. Client caps: `fs.readTextFile`, `fs.writeTextFile`, `terminal`, elicitation modes, boolean config options, terminal-auth. Agent caps: `loadSession`, `promptCapabilities` (image/audio/embeddedContext), `mcpCapabilities` (http/sse), `auth.logout`, session caps (resume/close/delete/additionalDirectories).
- **Session config options** (stabilized): agents advertise config options (e.g. model, reasoning effort); client updates via `session/set_config_option`.
- **Elicitation** (stabilized Jul 24, 2026): agent can request structured info from the user (`elicitation/create` with `form` or `url` modes).

### Client-side tools (what the *client* must implement)

| Method | Capability gate | Purpose |
|---|---|---|
| `fs/read_text_file` | `fs.readTextFile` | Read file contents incl. **unsaved editor state** (line/limit params) |
| `fs/write_text_file` | `fs.writeTextFile` | Write/create files; client tracks modifications |
| `terminal/create` | `terminal` | Start command in a new terminal (returns terminalId immediately; `outputByteLimit`) |
| `terminal/output` | `terminal` | Poll output + exit status |
| `terminal/wait_for_exit` | `terminal` | Await exit code/signal |
| `terminal/kill` | `terminal` | Kill without releasing |
| `terminal/release` | `terminal` | Kill + release resources |
| `session/request_permission` | baseline | Permission/approval flow (below) |
| `elicitation/create` | elicitation modes | Structured user input (forms/URL) |

**Note on "applyEdit":** ACP has **no `applyEdit` method** (that's LSP's `workspace/applyEdit`). In ACP v1, file edits are surfaced as **`diff` content blocks** (`{type: "diff", path, oldText, newText}`) attached to `tool_call` updates for display, while actual writes go through `fs/write_text_file` if the client advertises it (otherwise the agent writes files itself). **v2 replaces `oldText/newText` with structured file changes** (add/delete/modify/move/copy, binary/non-text) plus an optional `git_patch` for rendering.

### Permission / approval flow

- Agent **MAY** call `session/request_permission` (a request from agent→client) before executing a tool call, with a `toolCall` update and a list of `options`, each with `optionId`, human-readable `name`, and `kind`: `allow_once` / `allow_always` / `reject_once` / `reject_always`.
- Client responds `{outcome: {outcome: "selected", optionId}}` — or `cancelled` if the turn was cancelled. Clients **MAY auto-allow/reject** based on user settings (important for headless orchestrators).
- v2 makes permission prompts more flexible: own `title`/`description`, extensible `subject` instead of a hard-wired tool call.

### MCP relationship

ACP is **not** a competitor to MCP: MCP servers are passed **into** `session/new`/`session/load`/`session/resume` params (`mcpServers[]` with stdio command/env, or HTTP/SSE URLs if the agent advertises those capabilities). All agents MUST support stdio MCP; HTTP is recommended for new agents. The agent then connects to those MCP servers for tools/data.

### Version & governance

- **Current stable: v1** (`protocolVersion: 1`). **v2 first draft published July 20, 2026** — themes: beyond-the-turn updates (session updates no longer owned by the prompt request; agent "idle" signaling), patch-by-ID semantics for messages/tool calls/terminals, diff overhaul (structured file changes + git_patch), flexible permissions, forward-compat by default. SDKs publish v2 JSON schemas as `v2.0.0-alphaX`. Implementers are told to support v1 and v2 side by side.
- **Governance (interim):** jointly governed by **Zed and JetBrains**, working toward an independent foundation. Two **lead maintainers (BDFLs): Ben Brandt (Zed Industries) and Sergey Ignatov (JetBrains** — joined Feb 18, 2026; Agus Zubiaga remains core maintainer). Hierarchy: contributors → maintainers (per component: SDKs, docs, …) → core maintainers → lead maintainers (the "ACP steering group"). Core maintainers meet biweekly; **RFD process** for proposals; public **Zulip**; decisions must be public. License: **Apache 2.0**, no CLA. Security triage by Zed (security@zed.dev).
- **Registry:** a curated registry of 40 agents (must support authentication): `https://cdn.agentclientprotocol.com/registry/v1/latest/registry.json`, repo `github.com/agentclientprotocol/registry` (submit via PR with `agent.json` per schema). Clients can auto-install agents from it.
- **SDKs (all reached 1.0 on June 25, 2026):**
  - TypeScript: `npm install @agentclientprotocol/sdk` (fluent `agent()` / `client()` APIs; the older `AgentSideConnection`/`ClientSideConnection` classes are deprecated)
  - Rust: `cargo add agent-client-protocol` (powers Zed's own integration)
  - Python: `pip install agent-client-protocol`
  - Java: `agentclientprotocol/java-sdk` (incl. Spring AI examples)
  - Kotlin: `com.agentclientprotocol:acp` (JVM)
- **Timeline:** Sept 2025 Zed announces ACP (with Claude Code + Gemini CLI adapters) → Oct 24, 2025 implementation-info spec work → v1 stabilized → Feb 18, 2026 JetBrains co-leadership → Mar 9, 2026 registry stabilized → Apr 22, 2026 transports WG → Jun 25, 2026 SDKs 1.0 → Jul 20, 2026 v2 draft → Jul 24, 2026 elicitation stabilized.

---

## 2. ACP support matrix — coding agents (exact packages/commands)

All commands below are from the **official registry JSON** (verified 2026-09-10) — these are the exact invocations an ACP client uses to spawn each agent over stdio.

### The agents the team asked about

| Agent | ACP support | Exact command | Notes |
|---|---|---|---|
| **Claude Code / Claude Agent** | Adapter (official) | `npx @agentclientprotocol/claude-agent-acp` (v0.76.0) | Wraps the Claude Agent SDK. Repo `agentclientprotocol/claude-agent-acp` (2.5k stars, active). **Renamed**: the old `@zed-industries/claude-code-acp` npm package is **deprecated** (frozen at 0.16.2, "renamed to @agentclientprotocol/claude-agent-acp"). |
| **Codex CLI (OpenAI)** | Adapter (official) | `npx @agentclientprotocol/codex-acp` (v1.11.0) | Repo `agentclientprotocol/codex-acp` (active). The older `zed-industries/codex-acp` (877 stars) still exists but is stale (last push Jul 2026). |
| **Gemini CLI (Google)** | **Native** | `npx @google/gemini-cli --acp` (v0.59.0) | `--acp` flag; ships in `packages/cli/src/acp`. Cited by the ACP docs as the reference production ACP implementation. |
| **OpenCode** | **Native** | `opencode acp` (v1.18.30) | Repo moved: `github.com/anomalyco/opencode` (formerly `sst/opencode`; the old name redirects). Binary distributed per-OS. |
| **Qwen Code (Alibaba)** | **Native** | `npx @qwen-code/qwen-code --acp --experimental-skills` (v0.23.2) | Repo `QwenLM/qwen-code`. |
| **Crush (Charmbracelet)** | **NOT supported** | — | Not on the official ACP agents list; no ACP code in the repo tree (verified via GitHub tree search; the only "acp" hits are `llamacpp`). Crush is a Go TUI agent with its own LSP integration but no ACP as of 2026-09-10. |
| **Cursor** | Native | `cursor-agent acp` (v2026.09.02) | Docs: cursor.com/docs/cli/acp; per-OS binary downloads from cursor.com. |
| **GitHub Copilot CLI** | Native (preview) | `npx @github/copilot --acp` (v1.0.83) | Public preview since 2026-01-28 (GitHub changelog). |

### Other notable ACP agents (registry versions, 2026-09-10)

- **Cline** — `npx cline --acp` (3.0.61)
- **Goose (Block)** — `goose acp` (1.50.0)
- **Junie (JetBrains)** — `junie --acp=true` (3123.7.0)
- **Kimi CLI (Moonshot)** — `kimi acp` (1.50.0)
- **Kilo** — `npx @kilocode/cli acp` (7.6.0)
- **Grok Build (xAI)** — `npx @xai-official/grok agent stdio` (1.0.27)
- **Factory Droid** — `droid exec --output-format acp-daemon` (0.216.0)
- **Auggie (Augment Code)** — `npx @augmentcode/auggie --acp` (0.36.0)
- **Amp** — community adapter `amp-acp` binaries (0.9.0)
- **Mistral Vibe** — `vibe-acp` binary (2.24.1)
- **Devin CLI (Cognition)** — `devin acp` (3000.10.21)
- **Google Antigravity** — `agy_acp_server` binaries (1.1.1)
- **GLM Agent (Zhipu)** — `npx glm-acp-agent` (1.8.0; GLM coding-plan models)
- **Codebuddy Code (Tencent)** — `npx @tencent-ai/codebuddy-code --acp` (2.148.0)
- **Cortex Code (Snowflake)** — `cortex acp serve` (1.0.73)
- **pi** — `npx pi-acp` adapter (0.0.33)
- **DeepAgents (LangChain)** — `npx deepagents-acp` (0.1.7)
- **fast-agent** — `uvx fast-agent-acp` (0.10.1)
- **Harn (burin-labs)** — `harn serve acp` (0.10.134; runs `.harn` agent pipelines — unrelated to DeepSeek Harness)
- Plus: Amp, Autohand, Corust, crow-cli, DimCode, Dirac, Nova (Compass), Poolside, Qoder, siGit, Stakpak, VT Code, Agoragentic — full list at agentclientprotocol.com/get-started/agents (registry: 40 agents).

### ACP clients (editors & surfaces)

Editors/IDEs: **Zed**, **JetBrains AI Assistant**, **Neovim**, **VS Code** (multiple extensions: ACP Client, ACP Patchbay, ACP Pro, Multicoder, Poolside Assistant), Emacs (agent-shell.el), Obsidian (Agent Client, Agent Console, Copilot, Obsidian Harness plugins), Qt Creator (official plugin), Sublime, Pulsar, Unity (two clients), web IDEs. Also CLI/TUI clients (acpx, Martty, Hydra, Nori, pool), desktop apps (AionUi, DeepChat, Devin Desktop, Agent Studio, …), mobile (Happy, Agmente, Runmote, Shellular, Mobvibe, VACP voice), messaging bridges (Telegram, Discord, Slack, WeChat, QQ, Lark/飞书, Matrix), notebook tools (Jupyter agent-client-kernel, marimo, DuckDB extension).

---

## 3. "DeepSeek Harness" — what it is and how to integrate it

**It exists, it's official, and it already speaks ACP.**

### Identity

- **Repo:** `github.com/deepseek-ai/deepseek-harness` — "DeepSeek Harness: Everything is a Plugin."
- **Stats (2026-09-10):** ~218,900 stars; created **2026-08-13** (≈4 weeks old); MIT; **developer preview** ("THERE WILL BE COMPATIBILITY-BREAKING CHANGES"); homepage deepseek.com/harness; docs deepseek-harness.github.io/deepseek-harness/.
- **Install/run:** `npx @deepseek-ai/dsh web` → Web UI at `http://127.0.0.1:3080`. npm package `@deepseek-ai/dsh` (latest 0.1.5-rc.1, published daily; `next` 0.1.5-rc.2). Binary name / command: **`dsh`**.
- **Architecture:** "everything-is-a-plugin" harness built on **Cordis** (cordiverse/cordis; design paper: arXiv 2608.25512, "A Programming Paradigm for Spatiotemporal Composability"). The monorepo has ~50 packages: `acp`, `mcp`, `lsp`, `subagent`, `workflow`, `schedule`, `jobs`, `goal`, `skill`, `persona`, `plan`, `sandbox`, `e2b`, `webhook`, `terminal`, `fs`, `shell`, plus tools (fs/bash/web/goal/jobs) and app frontends (web, headless, cmdline, acp-app, sdk-app). Providers include `deepseek-official` (e.g. model `deepseek-v4-pro`).

### ACP support — agent side (this is what a client integrates against)

Package: `@deepseek-ai/dsh-acp` ("The dsh ACP profile bundle: automation-only JSON-RPC stdio and process lifecycle over dsh-base"); source `packages/acp/acp`.

- **Start command:** `dsh --profile acp` (equivalently `npx @deepseek-ai/dsh --profile acp`). Stdout carries only protocol traffic.
- It is an **"automation-only" ACP v1 server**: create/resume/close/list sessions, attach MCP servers (stdio + **Streamable HTTP**), select model + reasoning effort, send text/image prompts, receive semantic updates, answer permissions, cancel — no human in the loop. Design record: ".agents/notes/implemented/simplification/2026-07-23-acp-automation-only-protocol.md".
- **Multiplexes several concurrent sessions over one connection** (per-session isolation, ownership, teardown).
- **Supported:** `initialize` (ACP v1 + `session/list`, `session/resume`, `session/close`, HTTP MCP), `authenticate` (no-op, always succeeds), `session/new` (validates absolute workspace + MCP entries), `session/list` (paged, newest-first, optional cwd filter), `session/resume` (restores log without replaying updates), `session/close` (quiescent cancellation + persistence flush), `session/set_config_option` (`model`, `reasoning_effort`), `session/prompt` (text, resource links, images; one in-flight prompt per session), `session/cancel` + `$/cancel_request`, `session/update` (messages, thoughts, tool lifecycle, config changes, usage), `session/request_permission` (one-shot allow/reject options).
- **Not supported (rejected/omitted):** `session/load` transcript replay, session delete, fork, `additionalDirectories`, SSE MCP, modes, slash commands, plans, terminals, client fs operations (`fs/read_text_file`, `fs/write_text_file`), elicitation. DSH-specific presentation data stays off the wire.
- Config (provider/model per session):

```yaml
- name: '@deepseek-ai/dsh-acp'
  config:
    provider: deepseek-official
    model: deepseek-v4-pro
```

- **Not yet in the official ACP registry** (no DeepSeek entry among the 40) — spawn it directly by command rather than expecting registry auto-install.

### ACP support — client side (DSH can orchestrate other ACP agents)

`@deepseek-ai/dsh-subagent-acp` ("the out-of-process ACP subagent backend"): DSH can **delegate a task to any ACP-compatible agent** running in a fresh subprocess with its own runtime/session/model/tools. It shares only the working directory, sends the task over ACP, returns the child's final answer; **permission prompts are auto-answered by policy** (`reject`, or `allow` = first allow_once/allow_always option). Config:

```yaml
- id: subagent-acp
  name: '@deepseek-ai/dsh-subagent-acp'
  config:
    providerName: acp
    command: dsh                 # or any other ACP agent binary
    args: ['--profile', 'acp']
    permission: reject
    env:
      DSH_HOME: /absolute/path/to/isolated-child-home
```

### Companion client: Martty

`openma-ai/Martty` (npm `martty`, 71 stars) — "Unified Harness TUI (ACP client). Self-Improvement TUI plugin of DeepSeek Harness." A DSH-first terminal UI built on the same Cordis plugin system; connects to DSH over ACP by default and can attach other ACP agents. Demonstrates that the intended way to drive DSH programmatically is ACP.

### Integration recipe for an orchestrator

1. Spawn `dsh --profile acp` (or `npx @deepseek-ai/dsh --profile acp`) as a subprocess.
2. `initialize` (client need not advertise fs/terminal — DSH doesn't use them).
3. `session/new` with `cwd` (absolute) and optional MCP servers (stdio or HTTP).
4. Optionally `session/set_config_option` for model/reasoning effort.
5. `session/prompt` with text (images if enabled); consume `session/update`; auto-answer `session/request_permission` per your policy.
6. Persist `sessionId`s; use `session/list` / `session/resume` across restarts; `session/close` to free.

---

## 4. Multi-agent ACP orchestrators (open source)

Yes — several exist, at every layer. Grouped by what they do (stars verified 2026-09-10 via GitHub API):

### Purpose-built multi-agent orchestrators

| Project | Stars | What it does |
|---|---|---|
| **Codeg** (`xintaofei/codeg`) | 3,364 | Collaborative multi-agent coding workbench; aggregates sessions from Claude Code, Codex, OpenCode, Pi, Grok and more; desktop app, self-hosted server, or Docker. |
| **CompozyOS** (`compozy/compozy`) | 2,734 | "An operating system for AI agents. Plug in the agent CLIs you already use (Claude Code, Codex, Gemini CLI, Crush…)." Runs ACP agents **as a team on loops and schedules**, with shared memory and approvals. Web + CLI. |
| **AgentConnect** (`agentconnect-md/agentconnect`) | 1,351 | "The open-source, multi-agent alternative to Claude Tag." Brings ACP agents into team workflows across Slack, Telegram, Discord, Lark, GitHub, GitLab, managed from a web console; @-mention any agent. |
| **Claw Orchestrator** (`Enderfga/claw-orchestrator`) | 570 | "Run Claude Code, Codex, Antigravity, Cursor Agent and OpenCode as one runtime — persistent sessions, multi-agent…" (Also listed on the ACP *agents* page, i.e. it can itself be driven over ACP.) |
| **Gold Band** (`diodeme/Gold-Band`) | 82 | Local-first ACP desktop client with **DSL-based workflow orchestration** + AI-generated dynamic workflows + unified context management. |
| **Jockey** (`recailai/jockey`) | 24 | Multi-agent collaboration platform (Tauri + Rust + SolidJS) coordinating Claude Code, Gemini CLI, Codex CLI via ACP. |
| **Kronos** (`Reqeique/Kronos`) | 4 | Self-hosted scheduler/orchestration dashboard; runs ACP agent tasks on schedules via a bridge CLI, delivers to Slack, real-time dashboard. |

### Parallel-session clients (run several agents side by side, less "orchestration logic")

- **Exo for VS Code** — parallel sessions isolated in git worktrees, native Diff Editor approvals.
- **Braide** (braide.dev) — parallel sessions, worktrees, personas, interactive agent responses.
- **Kepler** (GitKraken, commercial) — "agentic development environment" for parallel agents at scale.
- **Superlite** (superlite.dev) — Rust-native desktop client, parallel sessions, browser remote.
- **Obsidian Agent Console** — tabbed multi-session workspace: several ACP agents in parallel, restorable/searchable sessions. (Related: **Obsidian Harness** plugin — every agent session is a first-class `.session` vault file.)
- **tlbx** (`tlbx-ai/tlbx`) — self-hosted browser control station for persistent ACP agent + terminal sessions, supervised from desktop/tablet/phone.

### Programmatic / headless building blocks (most relevant for building your own)

- **acpx** (`openclaw/acpx`, 3,236 stars) — "Headless CLI client for **stateful ACP sessions**." Scriptable, non-interactive; the closest thing to `kubectl` for ACP agents. Strong prior art for an orchestrator's execution layer.
- **Mastra `@mastra/acp`** (npm 0.4.1) — wraps external ACP agents **as tools or subagents** inside the Mastra TS framework; compose ACP agents into workflows programmatically.
- **ACP Kit** (`vcoderun/acpkit`) — adapters exposing Pydantic AI / LangChain agent runtimes through ACP, plus `ACP Remote` (WebSocket transport) and **ACP Router** (`vcoderun/acprouter`, Telegram surface with rich diffs + approvals).
- **stdio Bus** (`stdiobus/stdiobus`) — deterministic stdio kernel: agentic **process supervisor and message router** for NDJSON-framed JSON-RPC (ACP/MCP-style) with session routing.
- **Remote Agent Server** (`ma-pony/remote-agent-server`) — self-hosted ACP execution gateway: async Task API over Codex/Claude Code, isolated workspaces, persistent sessions, SSE, signed webhooks.
- **acp_rpc_bridge** (`Intellexie/acp_rpc_bridge`) — bridges stdio ACP agents to HTTP (ACP Streamable HTTP + OpenCode REST endpoints).
- **ACP → AG-UI** (`namanrajpal/acp-to-agui`) — bridges any ACP agent to web frontends via AG-UI events over SSE.
- **DeepSeek Harness itself** — `dsh-subagent-acp` turns any ACP agent into an isolated subagent of DSH (see §3), i.e. DSH is already a multi-ACP-agent orchestrator.
- **acpdbg** — fun niche one: pipes LLDB crash dumps to any ACP coding agent for root-cause analysis.
- Framework-level: **fast-agent**, **LLMling-Agent**, **AgentPool**, **LangChain Deep Agents ACP**, **LlamaIndex workflows-acp**, **Koog** (`agents-features-acp`) all speak ACP for agent-to-framework composition.

**Gap observation:** there is no dominant, generic "ACP router that load-balances one logical task across N agents" — existing projects either (a) multiplex *sessions* of many agents behind one UI, (b) schedule/loop agents, or (c) wrap agents as subagents of one harness (DSH, Mastra). A client that fans a task out to several ACP agents, compares results, and routes follow-ups is still open territory; the building blocks (acpx, `@agentclientprotocol/sdk`, DSH's subagent-acp permission auto-answer pattern) are all available.

---

## 5. Disambiguation: three protocols, two acronyms

**Zed's Agent Client Protocol (ACP — agentclientprotocol.com).** A **client↔agent** protocol: an editor, IDE, TUI, or program drives a coding agent (Claude Code, Gemini CLI, DSH, …) running as a local subprocess over JSON-RPC/stdio. The client owns UI, permissions, terminals, and the filesystem view; the agent owns the model loop. **This is the one relevant for a client driving coding agents** — it is what "ACP support" means for every CLI in §2/§3.

**IBM BeeAI's Agent Communication Protocol (ACP — agentcommunicationprotocol.dev).** An **agent↔agent** protocol (same acronym, unrelated): RESTful HTTP API for agents built on different frameworks (BeeAI, LangChain, CrewAI, custom) to talk to each other — multimodal messages, sync/async, streaming, stateful/stateless, discovery, long-running tasks. Developed by IBM Research/BeeAI under the **Linux Foundation**, with the BeeAI platform as reference implementation. **Status: merged into A2A.** The site now leads with "ACP is now part of A2A under the Linux Foundation," the platform was renamed **Agent Stack** (`i-am-bee/agentstack`), and its migration guide moves agents from `acp_sdk` to an A2A-based `agentstack-sdk`. Treat BeeAI-ACP as historical/superseded.

**Google A2A (Agent2Agent Protocol — a2a-protocol.org, `a2aproject` GitHub org).** The surviving **agent↔agent** standard: announced by Google (April 2025, 50+ partners), donated to the **Linux Foundation** (June 2025), where IBM's ACP was folded into it. Complements MCP by design ("MCP is agent-to-tool; A2A is agent-to-agent"): agents expose AgentCards/skills, delegate tasks, exchange artifacts, without sharing memory or tools. Official SDKs: Python, JS, Java, .NET, Go, Rust. **Relevant to you only if** your orchestrating client should itself be callable as an agent by other agents, or if you want network-service-style delegation between orchestrator instances — not needed for locally driving coding-agent CLIs.

**Rule of thumb:** driving an agent from a UI/harness → **Zed ACP**. Giving an agent tools → **MCP**. Agents calling agents → **A2A** (formerly also BeeAI ACP).

---

## Sources

- ACP spec & site (fetched 2026-09-10): [agentclientprotocol.com](https://agentclientprotocol.com/) — get-started/introduction, agents, clients, registry; protocol/v1 overview, initialization, session-setup, prompt-turn, file-system, terminals, tool-calls; community/governance; announcements (v2 draft, SDK 1.0, Ignatov, transports WG, registry/elicitation stabilization).
- ACP registry JSON: `https://cdn.agentclientprotocol.com/registry/v1/latest/registry.json` (40 agents, distribution commands).
- GitHub API / raw (2026-09-10): `agentclientprotocol/agent-client-protocol` (4,205★), `agentclientprotocol/claude-agent-acp` (2,509★), `agentclientprotocol/codex-acp` (366★), `zed-industries/codex-acp` (877★, stale), `charmbracelet/crush` (27,995★; no ACP code), orchestrator repos (Codeg, CompozyOS, acpx, AgentConnect, Claw, Jockey, Gold Band, Kronos, stdiobus, acpkit, Martty).
- npm registry: `@zed-industries/claude-code-acp` (deprecated → `@agentclientprotocol/claude-agent-acp`), `@deepseek-ai/dsh` (0.1.5-rc.1), `@deepseek-ai/dsh-acp-app`, `@mastra/acp` (0.4.1), `martty`.
- DeepSeek Harness: [github.com/deepseek-ai/deepseek-harness](https://github.com/deepseek-ai/deepseek-harness) README + `packages/acp/acp/README.md` + `packages/subagent/subagent-acp/README.md` (218,877★, created 2026-08-13).
- IBM BeeAI ACP: [agentcommunicationprotocol.dev](https://agentcommunicationprotocol.dev/) ("ACP is now part of A2A under the Linux Foundation") + `i-am-bee/agentstack` `docs/stable/community/acp-a2a-migration-guide.mdx`.
- Google A2A: [a2a-protocol.org/latest](https://a2a-protocol.org/latest/) + web search on the June 2025 Linux Foundation unification of ACP into A2A.
