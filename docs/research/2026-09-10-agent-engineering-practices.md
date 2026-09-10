# Agent Engineering Practices — Landscape Research Report

**Date:** 2026-09-10/11 · **Intended location:** `docs/research/2026-09-10-agent-engineering-practices.md`

**Methodology / confidence legend.** Live web verification was heavily degraded during this session: GitHub and the GitHub API were entirely blocked to fetch, and the search backend succeeded on only roughly half of queries (some responses were stale or partially model-synthesized). Claims are marked **[V]** = corroborated by live search today, **[K]** = from model knowledge (training through ~Jan 2026), not independently re-verified. Star counts are approximate and drift; spot-check before quoting in anything public.

---

## 1. Coding-agent harnesses / CLI agents

| Project | Positioning (one line) | Approx. stars | Standout engineering ideas |
|---|---|---|---|
| **Claude Code** (anthropics/claude-code) | The reference harness: agentic terminal coding tool that spawned the whole category's conventions | ~30k **[K]** | (1) Harness-as-product — permissioning, hooks, subagents, skills, checkpoints, auto-compaction are the moat, not the model; (2) progressive disclosure as a pervasive design principle (CLAUDE.md → skills → tool results) |
| **Gemini CLI** (google-gemini/gemini-cli) | Google's Apache-2.0 terminal agent; the distribution powerhouse | 60k+ **[V]** (20k in first 24h, June 2025) | (1) Open-core at Google scale with a genuinely free tier (60 req/min, 1,000/day) **[V]**; (2) it became the *base* other CLIs fork (Qwen Code et al.), and added first-class Agent Skills support (`.agents/skills/`, `/skills`, `google/skills`, `npx skills`) **[V]** |
| **OpenHands** (All-Hands-AI/OpenHands, ex-OpenDevin) | Autonomous "AI software engineer" — the open platform play | 50k+ **[V]** | (1) Docker-sandbox-first design; (2) event-stream architecture (actions/observations as replayable events) **[K]**; runs a cloud + SDK platform on top **[K]** |
| **Codex CLI** (openai/codex) | OpenAI's open-source agent; the security-orthodox one | ~40k+ **[K]** ("tens of thousands, fastest-growing" **[V]**) | (1) Rust rewrite for native-binary distribution and memory safety **[V]**; (2) sandbox-by-default approval ladder (suggest → auto → full-auto) with OS-level sandboxing, plus origin of AGENTS.md **[V/K]** |
| **opencode** (sst/opencode) | Provider-agnostic, open terminal agent — the anti-lock-in choice | 20k+ by mid-2025 **[V]** (~25–30k est. now **[K]**) | (1) Client/server split (TUI over a local server; shareable, resumable sessions) **[K]**; (2) deliberate compatibility with Claude Code artifacts — reads `.claude/skills/` and AGENTS.md **[V]** |
| **Cline** (cline/cline) | Open-source VS Code autonomous agent (the IDE-native lineage) | ~45k **[K]** | Plan/Act mode split, in-editor checkpoints, MCP marketplace in-IDE **[K]**. Fork ecosystem: **Roo Code**, **Kilo Code** continue the pattern **[K]** |
| **Aider** (Aider-AI/aider) | The OG chat-based AI pair programmer in the terminal | ~35k **[K]** | (1) Repo map via tree-sitter + graph ranking — the classic answer to context limits **[V]**; (2) benchmark-driven development (its polyglot benchmark drove edit-format/model co-design); auto git commits per change **[V]** |
| **Goose** (block/goose) | Block's Rust agent; extensibility-via-MCP as the core bet | ~10–14k **[V/K]** | (1) MCP-first extensions (every extension is an MCP server) + shareable "recipes" **[K]**; (2) enterprise-friendly local agent with provider freedom **[K]** |
| **SWE-agent** (SWE-agent/SWE-agent, Princeton NLP) | The research harness that defined agent-computer interface design | ~16k **[V]** | (1) The "Agent-Computer Interface" (ACI) concept — interfaces tuned for agents, not humans **[V]**; (2) co-evolved with SWE-bench, the field's shared metric |
| **Qwen Code** (QwenLM/qwen-code) | Alibaba's CLI wrapping the Gemini-CLI framework for Qwen3-Coder | ~5k **[K]** | Vertical integration: CLI + open 480B-MoE/35B-active model (256K→1M ctx) trained with agentic RL; free tier via Qwen OAuth **[V]** |
| **Cursor CLI** (getcursor/cursor-cli) | Cursor's agent in the terminal, open-sourced 2025 | thousands, growing **[V]** | IDE-quality context (codebase indexing) brought to terminal/CI/headless use; TypeScript/ink **[V]** |
| **Amp** (Sourcegraph, proprietary) | Issue→PR autonomous agent with subagents | n/a | Parallel subagents, code-search-derived context, MCP support, multi-model **[V]** |

**Also notable:** Amazon Q Developer CLI (open source, AWS) **[K]**; Droid (Factory) **[K]**; Avo and Wingman (smaller agents surfaced as Vibe Kanban-supported targets) **[V-mention]**; Continue (~28k, open-source IDE assistant) and Roo Code (~20k) **[K]**. Devin and Windsurf remain closed-source.

**Momentum read:** the terminal-harness category has consolidated around Claude Code as the *convention-setter* and Gemini CLI as the *distribution+openness* play; differentiation has moved from the loop itself to context management, sandboxing, permission UX, and ecosystem artifacts (AGENTS.md, skills, MCP).

---

## 2. Orchestration frameworks

- **LangGraph** — graph/state-machine orchestration with durable execution (checkpoints, human-in-the-loop interrupts); v1.0 shipped Oct 2025; the de facto Python standard for controllable production agents **[K]**.
- **CrewAI** — role-based "crews" (role/goal/backstory agents), sequential/hierarchical processes plus deterministic Flows layer; Enterprise product on top **[V]**.
- **AutoGen / AG2** — the conversational multi-agent research lineage; the community fork AG2 (ag2ai/ag2) carries it forward **[K]**.
- **Microsoft Agent Framework** — Oct 2025 consolidation of AutoGen + Semantic Kernel's agent stacks into one framework; AutoGen and SK agents moved to maintenance mode **[K — could not verify live this session]**.
- **Mastra** — TypeScript-first "everything framework": agents, tools, RAG, memory, evals, and XState-based durable workflows; dev server + playground; Cloudflare-Workers-friendly deploys **[V]**.
- **OpenAI Agents SDK** — deliberately minimal primitives (agents, handoffs, guardrails, tracing), Swarm's production successor; the philosophical counterpoint to graph frameworks **[V]**.
- **Google ADK** — multi-agent framework (Python + Java) with A2A as a first-class feature and Vertex AI Agent Engine deployment **[V]**.
- **Strands Agents SDK (AWS)** — model-driven (no explicit graphs), MCP-native, Apache 2.0, hardened across 100+ internal Amazon workloads **[V]**.
- **Pydantic AI** — type-safe agent framework ("the FastAPI of AI agents"); 1.0 in late 2025 with Logfire observability **[V]**.
- **Letta** (ex-MemGPT) — stateful agents with an OS-style memory hierarchy (core memory blocks, recall, archival), self-editing memory, sleep-time compute, Agent Server **[V]**.
- **smolagents** (Hugging Face) — CodeAct minimalism: agents write code instead of JSON tool calls **[K]**.
- Adjacent: **LlamaIndex Workflows**, **DSPy** (declarative prompt pipelines), **Agno** (high star velocity; covered by another teammate), **n8n** (workflow automation with AI nodes) **[K]**.

**Trend:** the pendulum swung from heavy orchestration (graphs, crews, hierarchical managers) toward thin agent loops + open protocols (MCP, A2A, ACP, skills). Durable/resumable execution became table stakes. TypeScript (Mastra) and Python (LangGraph/Pydantic AI) now form two parallel ecosystems rather than one dominating.

---

## 3. "Agent engineering" as a discipline

**Origins.** Armin Ronacher's 2025 writing is the community's usual citation for naming the discipline — pragmatic essays on using Claude Code-class agents day-to-day, with recurring theses: *code becomes cheap, trust/verification becomes the scarce resource; the engineer's job shifts to orchestration and review; typed interfaces, tests, and docs matter more because they are context for agents* **[V — themes corroborated; exact essay titles not independently confirmed this session; pieces commonly cited include "How I Use LLMs to Help Me Write Code" and "AI in 2025: Where We Are and What's Missing" / "Agent Engineering"]**. Successors that shaped the canon: Anthropic's engineering essays — "Building effective agents" (workflows vs agents, composability), "How we built our multi-agent research system," "Effective context engineering for AI agents" **[V]**, "Equipping agents for the real world with Agent Skills" **[V]**; Karpathy popularizing "context engineering > prompt engineering" (June 2025) **[K]**; Cognition's "Don't Build Multi-Agents" (context-sharing pitfalls) **[K]**; Hamel Husain's eval-driven development **[K]**; Geoffrey Huntley's "Ralph runs" (long-horizon agent loops as stress tests) **[K]**.

**Converged-pattern inventory — what's actually standardized vs fragmented:**

| Pattern | Status | Notes |
|---|---|---|
| **AGENTS.md** | **Standardized** | OpenAI-proposed, vendor-neutral instructions file; supported by Codex, Cursor, GitHub Copilot, Claude Code (alongside CLAUDE.md), Gemini CLI, Jules, Factory, Amp, Zed, Roo Code; adopted by TypeScript, React, Next.js, Electron, Rust repos **[V]**. The single biggest standardization win. |
| **MCP as tool layer** | **Standardized** | Every major CLI consumes MCP servers; the `mcpServers` JSON schema is the de facto config format **[V/K]**. |
| **Agent Skills (SKILL.md)** | **Rapidly converging** | Claude Code, Gemini CLI, opencode, Amazon Q CLI **[K for Q]**; Codex not yet native **[V]**. See §4. |
| **OS-level sandboxing** | **Converged concept, per-CLI implementations** | Claude Code (Seatbelt/bubblewrap) **[V]**, Codex (Landlock/Seatbelt + approval ladder) **[V]**, OpenHands (Docker) **[K]**; cloud sandboxes E2B/Daytona/Modal **[K]**. |
| **Context engineering** | **Converged concept, fragmented implementation** | Auto-compaction, structured note-taking ("Notes to self"), just-in-time retrieval, sub-agent context quarantine are now shared vocabulary (Anthropic's essay) **[V]** — but each harness implements its own. |
| **Sub-agents** | **Converged concept, divergent mechanics** | Claude Code `/agents` + `.claude/agents/` **[V]**, Amp subagents **[V]**, opencode agent configs **[K]**. |
| **Checkpointing / resume** | **Common but non-interoperable** | Claude Code `/rewind` + file-history background process **[V]**; Aider's git-commit-as-checkpoint **[V]**; session resume in opencode/Codex **[K]**. |
| **Hooks / lifecycle events** | **Fragmented** | Claude Code's PreToolUse/PostToolUse/Notification hooks in settings.json are the reference **[V]**; opencode has an event system **[K]**; no cross-CLI standard. |
| **Memory** | **Fragmented beyond repo files** | Repo-level files standardized (AGENTS.md); session memory via MEMORY.md-style files and Claude Code's memory tool **[K]**; long-term memory products (Letta **[V]**, mem0, Zep **[K]**) remain a product category, not a standard. |
| **Evals** | **Benchmarks standardized, infra fragmented** | SWE-bench Verified, Terminal-Bench, tau2-bench as shared yardsticks **[K]**; eval tooling split across LangSmith, Braintrust, Langfuse, promptfoo, `claude plugin eval` **[K]**. |
| **Token/cost tracking** | **Converging on OpenTelemetry** | Claude Code emits OTel metrics **[K]**; ccusage for local JSONL analysis **[K]**; `/context`-style introspection commands spreading **[K]**. |
| **Multi-agent interop protocols** | **Standardization in progress** | A2A (Linux Foundation, 150+ partners) **[V]**, Agent Client Protocol (Zed) **[K]**, agentgateway (CNCF sandbox) **[V]**. |

---

## 4. Agent Skills standard and MCP management

### Agent Skills (agentskills.io / Anthropic, Oct 2025)

**Definition [V]:** a skill is a folder whose centerpiece is `SKILL.md` — YAML frontmatter plus a markdown body. Frontmatter: `name` (matches folder name, ≤64 chars), `description` (≤1024 chars — *the only thing loaded at startup, so it's the trigger surface*), optional `license`, `allowed-tools` (e.g. `Bash(git log:*)`), `metadata`, `version`. Optional siblings: `scripts/`, `references/`, `assets/`.

**Mechanism [V]:** progressive disclosure — ~100 words per skill loaded at startup; the full body enters context only when the request matches the description; bundled scripts are executed by the agent when the skill fires.

**Discovery/invocation [V]:** filesystem locations (`.claude/skills/`, `~/.claude/skills/`), API upload as ZIPs (`/v1/skills` with code execution), claude.ai capabilities; invoked either model-side (description match) or explicitly (slash command).

**Cross-CLI adoption:**
- **Claude Code** — native, plus plugin-bundled skills and enterprise managed deployment **[V/K]**.
- **Gemini CLI** — supports the open spec: project `.agents/skills/`, user `~/.gemini/skills/`, `/skills` command, `google/skills` collection, `npx skills` installer **[V]**.
- **opencode** — reads the SKILL.md format from `.opencode/skills/` and `.claude/skills/`, making skills portable with Claude Code **[V]**.
- **Amazon Q Developer CLI** — skills support reported **[K, moderate confidence]**.
- **Codex CLI** — **not native** as of latest verified info: its extension points remain AGENTS.md, `~/.codex/prompts/*.md` custom prompts, and `config.toml` MCP servers; skills support is an open community request **[V]**.
- **Marketplace:** skills.sh — "npm for agent skills" (`npx skills add owner/skill`; installs into `.claude/skills/`, `.cursor/skills/`, `.codex/skills/`; agents can self-install skills on demand) **[V]**. Ecosystem collections: `anthropics/skills` (docx/pdf/pptx/xlsx document skills) **[K]**, `obra/superpowers** (Jesse Vincent's popular community set) **[K]**.

**Assessment:** the fastest-standardizing artifact in the space — it's just markdown + frontmatter + folders, so every CLI can adopt it cheaply. The open question is trust: script-executing skills need the same permission/sandbox treatment as any tool call.

### MCP management

- **Config formats [K/V]:** the `mcpServers` JSON key is the de facto schema — VS Code `.vscode/mcp.json`, Cursor `.cursor/mcp.json`, Claude Code `.mcp.json` + `claude mcp add`, opencode `opencode.json`; Codex uses `config.toml` `[mcp_servers]` **[V-adjacent]**. Unified in schema, fragmented in file location.
- **Registries:** Docker MCP Toolkit — 100+ curated, containerized servers behind `docker mcp` **[V]**; Smithery / Glama / PulseMCP directories **[K]**; Microsoft's open-source TypeSpec-based MCP Registry with verified publishers **[K — not verifiable live this session]**; Cloudflare's OAuth'd remote MCP servers **[K]**.
- **Libraries/gateways:** `lastmile-ai/mcp-agent` — composable, MCP-first framework implementing Anthropic's canonical patterns (chain, parallel, router, orchestrator-workers, evaluator-optimizer) **[V]**; FastMCP **[K]**; **MetaMCP** — aggregate all MCP servers behind one endpoint with a management UI **[K — not verifiable live]**; **agentgateway** — CNCF sandbox, Rust, routes/translates A2A + MCP with policy and observability ("Envoy for AI agents") **[V]**; **Obot** (Acorn Labs) — self-hosted agent platform whose MCP gateway also *exposes agents as MCP servers* **[V]**.

---

## 5. Unified control plane / hub over multiple coding-agent CLIs

No dominant vendor-neutral hub exists yet. The space is stratifying into layers, each with an early leader:

**Execution layer (run many agents in parallel):**
- **Vibe Kanban** (BloopAI; Tom Blomfield) — kanban board orchestrating Claude Code, Codex, Gemini CLI, Cursor, Amp, Avo, Wingman and more; git-native cards (branch per task), worktree isolation, chat/diff review, merge flow; Rust + TypeScript; self-hosted and cloud; the closest thing to a category leader **[V]**.
- **Claude Squad** (smtg-ai/claude-squad) — Go TUI managing multiple agent sessions (Claude Code, Codex, Aider…) via tmux + worktrees **[V]**.
- **Conductor** (conductor.build) — macOS-native GUI for parallel Claude Code worktrees **[V]**.
- **Sculptor** (sculptor-dev/sculptor) — Rust TUI for orchestrating parallel Claude Codes **[V]**.
- **Claude Code agent teams / fleets** — the first-party answer: a lead agent spawning teammates with a shared task system and messaging — the harness vendor absorbing the hub role **[K]**.

**Shared-knowledge layer (one brain across CLIs):**
- **Backlog.md** — "project manager for coding agents": markdown tasks in `/backlog/`, npm CLI (`backlog init/create/list/board`), agents create/claim/complete tasks, per-branch organization, durable version-controlled memory that any agent (Claude Code, Cursor, Codex…) can read **[V]**. Siblings: OpenSpec, Task Master (claude-task-master) **[V-mention]**.

**Tool/capability layer:** MetaMCP (central MCP config) **[K]**; skills.sh (shared skill installs across `.claude/`, `.cursor/`, `.codex/`) **[V]**; AGENTS.md (shared instructions) **[V]**.

**Protocol/infra layer:** agentgateway (CNCF; A2A+MCP routing, policy, observability) **[V]**; Obot (self-hosted platform + MCP gateway) **[V]**; A2A (Linux Foundation) **[V]**; ACP (Zed's Agent Client Protocol for editor↔agent interop — covered by a teammate) **[K]**; kagent (Kubernetes-native agents) **[K]**; sandbox infra: E2B, Daytona, Morph **[K]**.

**Gap analysis (the opportunity):** nobody unifies all four layers. Vibe Kanban owns execution, Backlog.md owns shared knowledge, MetaMCP/skills.sh own tools, agentgateway owns protocol — but there is no vendor-neutral hub giving multiple CLIs one shared AGENTS.md + skills + MCP config + memory + identity/session model. The seams that still don't interoperate: per-tool MCP config files, no cross-CLI memory standard, no shared session/identity model, and skills' script-execution trust model.

---

## Bottom line

1. **AGENTS.md and MCP are the two settled standards**; Agent Skills (SKILL.md) is the third, converging fastest.
2. **The harness is the product** — engineering effort has moved from the agent loop to context management, sandboxing, permission UX, and ecosystem artifacts.
3. **Context engineering is the discipline's core skill** (compaction, JIT loading, notes-to-self, sub-agent quarantine), converged in vocabulary, fragmented in implementation.
4. **Orchestration frameworks commoditized** toward thin loops + protocols, with durable execution as the surviving differentiator.
5. **The control-plane/hub layer is the open frontier** — stratified into execution / knowledge / tools / protocol layers with no unifier yet.

## Sources (primary ones surfaced during live verification)

- [Anthropic — Effective context engineering for AI agents](https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents)
- [Anthropic — Building effective agents](https://www.anthropic.com/research/building-effective-agents)
- [Anthropic Agent Skills docs](https://docs.anthropic.com/en/docs/agents-and-tools/agent-skills) · [Equipping agents for the real world with Agent Skills](https://www.anthropic.com/engineering/equipping-agents-for-the-real-world-with-agent-skills) · [anthropics/skills](https://github.com/anthropics/skills)
- [agentskills.io](https://agentskills.io) · [agents.md](https://agents.md) · [skills.sh](https://skills.sh)
- [google-gemini/gemini-cli](https://github.com/google-gemini/gemini-cli) · [All-Hands-AI/OpenHands](https://github.com/All-Hands-AI/OpenHands) · [openai/codex](https://github.com/openai/codex) · [sst/opencode](https://github.com/sst/opencode) · [SWE-agent/SWE-agent](https://github.com/SWE-agent/SWE-agent) · [block/goose](https://github.com/block/goose) · [getcursor/cursor-cli](https://github.com/getcursor/cursor-cli) · [letta-ai/letta](https://github.com/letta-ai/letta) · [lastmile-ai/mcp-agent](https://github.com/lastmile-ai/mcp-agent) · [agentgateway](https://github.com/agentgateway) · [obot-platform/obot](https://github.com/obot-platform/obot) · [smtg-ai/claude-squad](https://github.com/smtg-ai/claude-squad)
- [Backlog.md](https://backlog.md) · [conductor.build](https://conductor.build) · [aider.chat](https://aider.chat) · [swe-agent.com](https://swe-agent.com)
- Docker MCP Toolkit (`docker mcp` catalog) · Vibe Kanban (BloopAI) · Armin Ronacher's 2025 writing at [lucumr.pocoo.org](https://lucumr.pocoo.org)
