# Research: Multica & LoopX (AI agent orchestration)

Date: 2026-09-10. Sources: GitHub API (repos, code search, releases, contributors, commit counts), raw READMEs/docs fetched from both repos, the LoopX hosted docs/blog, and the multica-cli skill repo. All stats verified live on 2026-09-10. (Note: the WebSearch tool returned no usable results in this environment and WebFetch was domain-blocked; everything below comes from direct API/raw fetches, which is higher-trust anyway.)

---

## 1. Multica

### 1.1 Official repo / site

- **Repo:** https://github.com/multica-ai/multica (org `multica-ai`, 6 public repos)
- **Site / cloud:** https://multica.ai (docs at multica.ai/docs), X: @MulticaAI, Discord
- **Ecosystem repos:** `multica-ai/multica-cli` (agent skill for operating Multica via CLI), `multica-ai/dsh-multica-runtime`, `multica-ai/homebrew-tap`, `multica-ai/andrej-karpathy-skills`, plus community `it235/multica-best-practices` (bilingual Agent · Skill · Squad templates, 114 stars)

**Disambiguation:** A GitHub search for "multica" is dominated by unrelated near-names (Solidity `multicall` contracts, multi-camera calibration `multical`, multicast-dns, etc.). Exactly one result is an AI-agent project: `multica-ai/multica`, "Make humans and AI agents work as one team — open-source and self-hostable," with 49.5k stars — three orders of magnitude above every other "multica" hit and clearly the agent-engineering project. Confirmed by its ecosystem (multica-cli skill repo, best-practices templates repo referencing "Agent · Skill · Squad" concepts).

### 1.2 One-line positioning

An open-source, self-hostable team workspace — a Linear/Jira-style issue board where **AI coding agents are first-class teammates**: you assign an issue to an agent the way you'd assign it to a colleague, it picks it up on your machine, comments as it goes, and hands it back for review. Tagline: "Agents that show up on the board" / "Your next 10 hires won't be human."

### 1.3 Architecture & key features

**Core execution model** (from `how-multica-works.mdx`): issue assigned to agent → Multica creates a **run** → queue → a **runtime** (daemon on a connected computer) claims the run → daemon spawns the local AI coding tool → progress streams back to the issue's timeline/execution log → result written back. Clean split: *Multica records and coordinates the work; connected computers execute it.* If no runtime is online, the run waits in the queue.

**Multi-agent orchestration / collaboration:**
- **Agents as board citizens.** Each agent = name + provider + runtime; appears in the assignee picker, activity timeline, comments. Work is never implicitly started: only four triggers — issue assignment, @-mention in a comment, direct chat, or an Autopilot.
- **Squads** (leader-routing, not fan-out): a squad = one leader agent + members (agents *or humans*). Assigning an issue to a squad wakes only the leader; it reads context, posts exactly **one delegation comment** @-mentioning the chosen member(s) — and the mention itself triggers those agents' runs. The leader also records its routing evaluation via CLI into the activity timeline. Role descriptions are context-only (no permissions, no auto-triggering). Explicitly documented as "doesn't merge agents into a new agent, doesn't auto-raise concurrency."
- **Autopilots:** cron (5-field, IANA timezones) or webhook triggers; each has a Runbook, assignee (agent or squad), and an execution mode — *create issue* (run goes through the normal queue, survives offline runtimes) vs *run only* (direct background run).
- **Chat + Inbox:** ask the workspace a question; Inbox pings you only when an agent "needs a call," not for every step — deliberate signal/noise design.

**Agent-harness integration (the deepest part of the codebase):**
- **26 agent CLIs supported** — Claude Code, Codex, Cursor, Copilot, Kimi, OpenCode, OpenClaw, Hermes, Pi, Antigravity, CodeBuddy, DevEco Code, Grok, Kiro CLI, Qoder (incl. CN), Qwen Code, QwenPaw, Reasonix, Trae CLI, DeepSeek Harness, Oh-My-Pi, MiniMax Code, Dim, Huawei CodeArts, and more. Switching providers is "a dropdown, not a migration" — Multica drives CLIs you already have installed/authenticated; it ships no model.
- **Per-CLI backends** in `server/pkg/agent/*.go` (~25+ backend files, heavily unit-tested per-CLI: cancel semantics on unix/windows, interrupt latency, compaction, cleanup). The Claude Code backend spawns `claude` with `--output-format stream-json` and injects `--mcp-config <tempfile>` so the agent uses a controlled MCP set instead of inheriting the outer session's; graceful SIGTERM→SIGKILL process-group teardown with WaitDelay backstop.
- **ACP support:** a shared **Agent Client Protocol (JSON-RPC 2.0 over stdio) client** transport drives Hermes, Kimi, Kiro, TraeCLI, QwenPaw, Dim, Grok, Zeroclaw, Reasonix, etc. Code-level detail is impressive: `session/new` requires absolute cwd; Dim's ACP server hardcodes a read-only permission preset at session creation, so Multica issues `session/set_config_option` (full-access, agent mode) right after; `session/load` resume across processes with documented per-version lock-release timing; model override via `session/set_model`; hardcoded `acp` subcommand protected from user arg overrides.
- **MCP support:** workspace-level and per-agent MCP server configuration (`runtime_mcp.go`, `cmd_agent_mcp.go`, `workspace_mcp(_api).go`; dedicated UI tabs for agent MCP / MCP config / settings), a `browser_mcp_config.go` (browser automation), and Cursor-MCP exec-env integration. Managed MCP config is materialized per-run as a temp file.
- **Multica-cli skill:** a portable skill (Claude Code plugin marketplace, Codex skill installer, Cursor rules) that teaches any local agent to *operate Multica itself* via the authenticated `multica` CLI — triage issues, safe comment writes with `--content-file`, mention/assignment/rerun side effects, `--no-start` suppression, run/token inspection, PR linking. Permissions come only from the user's local CLI login. I.e., the orchestration platform is itself agent-operable — symmetric human/agent API surface.

**Memory & knowledge management:**
- **Skills** = reusable playbooks (`SKILL.md` + scripts/templates/references), attached to multiple agents, enable/disable per binding. Import from local archive, **GitHub, ClawHub, Skills.sh**, or snapshot-scan from a runtime; skills imported from a hosted source keep a reference and can be re-pulled (`multica skill refresh`) preserving identity/assignments. Clean separation: agent *instructions* = long-lived identity/boundaries; *skills* = reusable methods shared across agents. Repo-level skills (`.claude/skills/`, `.agents/skills/`) stay owned by the repo and are not auto-registered.
- **Projects** group issues and carry shared execution context: the project's name+description is injected into every agent run inside the project; **resources** link GitHub repos (checkoutable, with ref) or local directories on specific machines; a **lead** (member or agent) marks the coordinator (explicitly not a permission).
- **Execution log** = full replayable history: every tool call, command, and error, timestamped; **token usage per agent and per issue**; intent/run/decisions/diff stay connected to the same issue so "the next agent does not start from zero."
- Per-agent memory dirs are handled per-runtime (e.g., `execenv/codex_memory.go`, `hermes_memory.go`), i.e., agent-state isolation rather than a built-in long-term memory/RAG product.

**Security model** (unusually honest doc): the boundary is the **daemon OS user** — Multica makes no filesystem-sandbox claim and says so loudly ("a run can read your SSH keys… put the boundary around it"); recommends dedicated Unix user → container → VM. What it *does* isolate: per-run working directory under `~/multica_workspaces/`, run-scoped agent state (e.g., per-run `CODEX_HOME`), and **run-scoped `MULTICA_TOKEN` bound to that agent and that run** — a run cannot act as you or as another agent through the Multica API. Approval prompts are auto-answered since agents run unattended.

**Other:** workspaces (per-team isolation), roles owner/admin/member + agent access scopes, VCS integrations (GitHub, GitLab, Gitea, Forgejo — self-hosted included), chat channels (Slack, Lark, DingTalk, WeCom, Telegram — some community-maintained), CLI + API for every surface.

### 1.4 Tech stack, stats, maturity, license

| | |
|---|---|
| Stars / forks | **49,500** / 6,387 (as of 2026-09-10) |
| Age / growth | Created 2026-01-13 → 49.5k stars in ~8 months; 1,530 open issues; ~300 contributors |
| Activity | Pushed 2026-09-10 (today); **100 commits in the last 7 days**; releases most weekdays (v0.4.42 on 2026-09-09, v0.4.41 on 09-07, v0.4.40 on 09-04) |
| Backend | **Go** (Chi router, sqlc, gorilla/websocket), ~24.3M bytes Go |
| Frontend | **TypeScript / Next.js 16** (App Router) ~16.5M bytes; Electron desktop sharing web UI packages; iOS via **Expo/React Native** (build from source) |
| DB | PostgreSQL 17 (pgcrypto + pg_trgm) |
| Dev setup | pnpm monorepo, `make dev` one-command; docs in MDX (fumadocs), localized EN / 简体中文 / 日本語 / 한국어 |
| Deployment | Multica Cloud, Desktop app, or self-host via Docker Compose / Helm (GHCR images) |
| License | **"Multica License"** — full Apache-2.0 text **plus additional conditions** on hosted services, commercial embedding, and branding (GitHub reports NOASSERTION; see LICENSE/NOTICE) |
| Maturity | Pre-1.0 (v0.4.x) but very polished: 4-language docs, security model doc, desktop+mobile clients, cloud + self-host paths. Fast-moving (`main` "moves quickly — pull often") |

### 1.5 Worth borrowing (for a multi-agent orchestration platform)

1. **Issue-board-native multi-agent coordination.** Making agents assignees/mentioners on a normal issue tracker gives humans a zero-new-UI mental model and gives agents a durable, auditable coordination medium. The **squad leader pattern** — leader posts exactly one delegation comment, and *the mention is the trigger* — is a beautifully minimal orchestration primitive: routing is legible, sequenced, and reviewable, with no hidden fan-out.
2. **Transport-layer pragmatism over framework-ism.** 26 CLIs driven by a small set of shared transports (stream-JSON CLI spawn; one shared ACP JSON-RPC client reused across ~10 CLIs), with per-CLI quirks (permission presets, session resume, blocked args) encoded and unit-tested in backend adapters. Provider-neutrality achieved by *integration engineering*, not by abstracting the agent away.
3. **The runtime daemon / data-execution boundary.** Server records and schedules; your machine executes next to your code; runs queue while runtimes are offline. Per-run workdirs, run-scoped agent state (`CODEX_HOME`), and **run-scoped tokens bound to agent+run** are a clean, cheap API-level blast-radius control worth copying verbatim.
4. **Agents operating the orchestrator itself.** The `multica-cli` skill makes the platform self-managing: agents triage, rerun, link PRs through the same authenticated CLI humans use, inheriting the local login's permissions. Orchestration platforms usually only expose humans-as-operators; this symmetry is rare.
5. **Skills as governed, shareable workspace assets** — import/update from GitHub/ClawHub/Skills.sh with identity-preserving refresh, snapshot-from-runtime, and the instructions (identity) vs skills (method) split. Plus **cost observability baked into the run model** (token usage per agent *and per issue*) and an **Inbox** that only interrupts on "needs a human call."
6. **Honest security docs as a feature.** "Treat every run as unsandboxed; the boundary is the daemon user; put one around it" — with concrete escalation ladders. Builds trust for a product that runs agents on your laptop.

---

## 2. LoopX

### 2.1 Official repo / site

- **Repo:** https://github.com/huangruiteng/loopx
- **Site:** https://huangruiteng.github.io/loopx/ (product page, blog, hosted docs, bilingual EN/中文 "Developer Book")
- **Community:** Discord; Feishu/Lark user manual (public wiki); Trendshift badge; active GitHub Discussions
- **Author:** Ruiteng Huang (`huangruiteng`) — Beijing, ByteDance AML engineer, THU EE. Personal open-source project (not a ByteDance product); the author is also a contributor to volcengine/OpenViking, which shows up as LoopX's first reward-memory provider.

**Disambiguation:** Several small repos named "loopx" exist, mostly *around the same concept* — `munesoft/loopx` ("universal loop controller for AI agents… prevent infinite iterations"), `calvingit/loopx` (tmux-based long-running task runner for Codex CLI), `rye567/loopx` ("LoopX cross-project quality gate kit for Codex and Claude Code"), `lcmax/Loopx` ("轻量级 Agent Loop Skills"), `xielixing/loopx-console` ("packaging the official LoopX control plane (huangruiteng/loopx)"). The canonical project is unambiguous: `huangruiteng/loopx` (5.8k stars; topics `agent-control-plane`, `loop-engineering`, `long-horizon-agents`; description "Long-horizon agent control plane for durable, governed work across Codex, Claude Code, and other harnesses"). The others are 0–3-star derivatives/echoes.

### 2.2 One-line positioning

An open, provider-neutral, **deterministic (no-LLM) state kernel and local-first control plane for "loop engineering"** — it sits *on top of* agent harnesses (Codex, Claude Code, Cursor, …) and preserves objective, gates, todos, evidence, quota, and handoffs across turns, sessions, and harnesses, while the harness executes bounded work. "Keep the loop moving. Keep the judgment human."

### 2.3 Architecture & key features

**The state kernel.** One compact durable layer holds, per goal: `objective + gates + todos + scope + evidence + quota`. Six durable control-plane layers: (1) Registry, (2) Goal state, (3) Run log (JSON+Markdown per goal), (4) compact Run history for agents/heartbeats/UI, (5) Status/attention queue ("who needs to act next"), (6) Compute quota — plus an optional read-only **probe surface**. The README's loop:

```
objective → LoopX state → human judgment needed? → ask a concrete question and wait
                                ↓ no
                     safe fallback? → run ONE bounded agent slice (Codex/Claude Code/Cursor/shell)
                                ↓
                     write evidence + handoff + next todo → quota decides the next tick
```

**The tick protocol** (deliberately tiny, CLI-first):

```
loopx quota should-run   # may this registered agent act now?
loopx todo claim         # who owns this slice?
loopx todo update        # what changed?
loopx refresh-state      # what should the next turn see?
loopx quota spend-slot   # account for a completed, validated slice
```

Quota spends **only after validated writeback**; quiet skips, preflight failures, and dry-runs don't spend. Scheduler cadence follows `quota should-run.scheduler_hint`, and installed host automations must ACK the hint (`ack_hint.cli_args`) — a documented contract between control plane and host scheduler.

**Conceptual framing** (from the architecture doc and the blog post "From one-shot agents to long-horizon control"):
- A four-layer recovery stack: 01 execution substrate (processes/containers/sessions) → 02 durable workflow (steps, retries) → 03 work coordination (todos, claims, leases, handoffs) → 04 **semantic control plane** (goals, evidence, authority, acceptance, replanning). LoopX owns layer 04 and treats harnesses as replaceable workers.
- Precise vocabulary: **Turn** = bounded execution window; **Result** = artifact/observation; **Transition** = validated, permitted state change; **Progress** = durable transition relevant to acceptance. "A convincing answer is a result; it becomes progress only when validation and state transition are accepted."
- "Control plane as **effect interpreter**": `model → effect request → harness interprets effect → observation → model`, implemented with a TypeScript **Effect** core; typed turn contracts (`LoopXTurnRoute` before execution in `driver.py`; `TurnResultKind` after, in `settlement.ts`). Explicitly positions itself against LangGraph checkpoints (graph state) and Temporal (activity idempotency): LoopX adds project-semantic authority on top.
- "Guided autonomy, not recommendation lock-in": hard boundaries (claims, scope, gates, quota, public/private policy) are machine-enforced; suggestions/priorities are advisory and *must say so in their schema* — if a controller wants a whitelist, it belongs in a typed authority contract, not list position or prompt wording.
- **Agent-native Kanban** mental model: cards carry identity, authority, evidence, continuation; moves are validated operators (claim, gate, monitor, writeback); the board is a *projection*, kernel state is the source of truth.

**Multi-agent orchestration:** registered agents are **peers — no durable leader**. Claims, leases, task boundaries, capabilities, and typed continuation decide who acts next. Peer protocol: `todo claim` before delivery, `todo update` after validation. Showcase demos: Auto Research (proposer / executor / evaluator-promoter roles iterating in parallel with quota + evidence visible), and a cross-runtime implementation review (Claude implements, Codex reviews, LoopX keeps ownership/evidence/quota/handoff explicit).

**Host/harness integrations** (each with an explicit "loop driver" contract):
- **Claude Code:** opt-in adapter — the run loop is **Claude Code's native `/loop`** executing a project `.claude/loop.md`; LoopX provides the control plane via **MCP tools `should_run` / `claim_task` / `complete_task`**; `/loopx <task>` sets up the goal and writes loop.md. Optional `--harden`: a project-scoped **PreToolUse hook** that allows read-only tools unconditionally, gates Edit/Write by `write_scope`, Bash by a destructive-command denylist, and fails **closed** if the probe is unreachable — documented honestly as "a policy layer, not a sandbox."
- **Codex App:** heartbeat automation refreshed from `quota should-run.scheduler_hint`; **Codex CLI:** visible `/goal` driver, no hidden headless execution by default; **KunlunCode:** native Goal Pro through app-server (completion written only after strict verification); **OpenCode:** static command facade + opt-in goal bridge; **Pi / ZCode / Antigravity (agy) / Kiro CLI:** skill facades re-entering through `quota should-run`; **DSH (DeepSeek Harness):** native plugin + GoalBar; **Cursor / shell / custom runners:** minimal custom-runtime example + full custom-runner integration guide + worker-bridge install contract.
- **ACP:** `loopx/chat_acp.py` is an **ACP v1 stdio adapter** for owner-configured LoopX Chat Agents — LoopX as ACP *client* with startup/idle/hard timeouts (30s/180s/900s), graceful-exit windows, and thread-id session resume. So LoopX speaks MCP *server-side* (hosts call LoopX) and ACP *client-side* (LoopX drives chat agents).
- **MCP:** a shared FastMCP-based MCP control plane (`goal_mode_mcp.py`) exposes deterministic lifecycle ops to host adapters; includes boundary-typing discipline (strict pydantic `Annotated[int, Strict()]` so lax pydantic can't coerce JSON `true` → `1` before loopx code runs).

**Memory & knowledge management:**
- **Reward Memory v0** (experimental, default-off, provider-neutral): separates **feedback evidence / policy content / action authority** — "the distinction between inferring what the contributor wants and inventing what the contributor is authorized to permit." Five memory classes, guarded precedence, corpus registry + health read model, stateless candidate/review seam, explicit recall/application stages. Config declares per-corpus scopes, module-owned surfaces, and `recall_profile`s; LoopX "never discovers routes by scanning all corpora"; `automatic_recall`/`automatic_ingest` default false; fail-open. OpenViking is the first provider (an option, not a dependency).
- **Evidence** as a first-class state element: compact run history, validation, blockers, accepted writeback, review packets (`loopx review-packet` = owner-facing view of decisions/evidence/validation/unresolved gates). Capabilities like **Issue Fix** keep "rolling repository context, revision-stamped fix knowledge, and reviewer-facing preferences" separate, with linked PRs + source + tests remaining authoritative.
- **Project-level reward model:** conservative value signals across output quantity, quality, token cost, and *user attention cost*.
- A documented user case adopted an external `codebase-memory-mcp` tool — memory is pluggable, not owned.

**Capability system** (the extension model): **Kernel** (durable truth) / **Capability** (stable provider-neutral contract for one bounded, verifiable outcome) / **Provider** (calls external systems, returns observations + effect results + readback) / **Extension** (packaged optional provider with explicit install → readiness → enable → upgrade → disable → rollback lifecycle). Execution path `Agent → Capability → Provider`; control path `Provider readback → Capability transition → Kernel`. Shipped capability lanes: issue-fix, change-quality (final-diff qualification), integration-branch (reconciling stacks of reviewed branches), explore (research with hypotheses+findings), decision-context (rebase decisions on current evidence), periodic-report, content-ops, value-connectors, ml-experiment, benchmark.

**Operator surfaces:** `loopx dashboard` (PWA) + native desktop previews (signed macOS App updates; Windows preview installers) — the **Personal Agent Workspace 1.0**: see what needs you / is running / watched / scheduled / stopped; configure Goal capabilities with machine-default vs Goal-override distinction and preview before applying; steer a live turn or queue a message; async inbox from Lark with Goal/Agent/session routing; review protected changes via typed preview + explicit confirmation + receipts ("LoopX state — not the browser — remains authoritative"). Plus `loopx dash` (fleet session view) and a **Lark Kanban projection** (`loopx lark-kanban`).

**Evidence discipline** (notable): every showcase carries explicit evidence-strength labels and reproducibility boundaries — "200+ hours of *elapsed* loop lifetime… wall-clock project time, not continuous model execution"; the Auto ML showcase is "owner-run, redacted, not sufficient to reproduce independently"; SWE-Marathon benchmark states "one trial per task and mode… neither establishes a general performance gain." Three independent-user cases (13h C++ run, 4-day unattended run, 7 merged PRs / reported 1B+ tokens) are labeled user reports.

**Current strategic programs** (explicitly "direction signals, not delivery promises"): long-horizon benchmarks & evidence; operator surface + IM integration; shared goal authority & cross-host coordination (NoKV as an unpromoted provider candidate); architecture/research incubator (Effect Program hardening, TypeScript parity migration, hierarchical stride, memory utility).

### 2.4 Tech stack, stats, maturity, license

| | |
|---|---|
| Stars / forks | **5,776** / 526 (as of 2026-09-10) |
| Age / growth | Created 2026-05-31 → 5.8k stars in ~3.5 months; 90 open issues; ~72 contributors |
| Activity | Pushed 2026-09-10 (today); **586 commits in the last 7 days** (extreme cadence); v1.0.2 released 2026-09-09; desktop-main builds shipped daily |
| Stack | **Python 3.11+** (PyPI package `loopx`) + a managed, idle-exiting **TypeScript Effect core** on Node 22.6+ (24 LTS recommended); ~30.6M bytes Python, ~4.2M TypeScript, some Rust; PWA dashboard + desktop app (Apple Silicon signed-not-notarized; Windows preview with manual updates); native Windows PowerShell 7 support; local-first state in `.loopx/`, `.codex/goals/` (git-ignored) |
| Install | `pip install loopx` → `loopx workflow-skills --install` → `loopx doctor`; or clone + `install-local.sh` for contributors; `loopx update plan/apply` preserves install channel |
| License | **Apache-2.0** |
| Maturity | v1.0.x shipped ("LoopX 1.0.2 — single-owner Todo authority, automatic recovery, faster workspaces"); heavy documentation culture (bilingual developer book, RFCs, versioned protocol contracts, release-readiness gates); single primary author with growing contributor base; honest maturity labels everywhere |

### 2.5 Worth borrowing

1. **The five-question state kernel.** Objective / what's next / what needs human judgment / what evidence changed / may the loop continue — a minimal, inspectable, harness-agnostic durable state. And the discipline that the control plane is **deterministic (no LLM)**: routing and gating are code, not prompts, which makes long-horizon behavior debuggable.
2. **Quota-and-tick protocol.** `should-run → claim → update → refresh-state → spend-slot`, with spend only after validated writeback and scheduler-hint ACK contracts. It's the cleanest published answer to "when should an agent act *at all*" — including "stay quiet when facts haven't changed" as a first-class outcome (deliver / wait / ask / replan / repair / stay quiet).
3. **Peer coordination without a leader.** Claims + leases + task boundaries + typed continuation instead of a central orchestrator LLM; and the **Kanban-as-projection** rule — the board is a view, the kernel is truth — which prevents UI state drift, the classic failure of agent dashboards.
4. **Turn/Result/Transition/Progress vocabulary + typed transitions.** Distinguishing "a convincing answer" from "accepted progress" (result vs validated transition) kills the "agent says done" problem at the type level. The execution-path/control-path split (`Agent→Capability→Provider` vs `readback→transition→Kernel`) is a solid seam for any orchestrator.
5. **Human gates as concrete state.** "A concrete question and wait" instead of a vague waiting-for-owner flag; audited safe fallbacks that explicitly *cannot* bypass a blocked gate; fail-closed PreToolUse hardening with write_scope enforcement. Also: advisory-vs-authoritative schema discipline (advisory fields must say so).
6. **Reward Memory's authority separation.** Feedback evidence / policy content / action authority as distinct things, with per-corpus scopes derived from *verified* contributor identity, default-off automation, and no cross-corpus route scanning. The best governance design for agent memory I've seen in an open project.
7. **Evidence-strength labeling as a cultural practice.** Every claim in the README carries its evidentiary boundary (elapsed-vs-compute, user-report-vs-reproduced, redacted caveats). For a platform making autonomy claims, this is worth copying wholesale.
8. **The host-integration ladder.** From heartbeat automation → native host loops (`/loop` on Claude Code, `/goal` on Codex/Kiro) → command/skill facades → custom runners — all re-entering through the same `quota should-run` gate. Meet each harness where it is; gate everything through one chokepoint.

---

## 3. Comparison & synthesis for a multi-agent orchestration platform

| Axis | Multica | LoopX |
|---|---|---|
| Layer | Product/workspace layer (team-facing) | Control-plane/state layer (owner-facing) |
| Unit of work | Issue on a board | Goal with durable state kernel |
| Multi-agent model | Leader-routed squads (mention-triggered delegation) | Peer agents with claims/leases/quota (no leader) |
| Human role | Reviewer/assignee in a team UI | Owner holding gates, authority, and acceptance |
| State authority | Multica server (Postgres) | Local-first kernel state (`.loopx/`) |
| Harness integration | Drives 26 CLIs (stream-JSON spawn + ACP client) | Sits under ~12 host surfaces (MCP server + ACP client + facades) |
| Memory/knowledge | Skills, project context, execution log, token accounting | Evidence, gates, reward memory (authority-scoped), revision-stamped fix knowledge |
| Concurrency stance | Sequential, legible routing | Parallel peers under compute quota |
| Scale | 49.5k stars, ~300 contributors, company-backed cloud | 5.8k stars, ~72 contributors, single primary author (ByteDance AML engineer) |
| License | Apache-2.0 + conditions (hosting/embedding/branding) | Apache-2.0 |

Both projects share one big bet worth internalizing: **neither builds "another agent framework."** Both treat existing coding-agent CLIs as replaceable execution workers and compete on the coordination/governance/legibility layer around them — Multica by making that layer look like a team workspace humans already understand, LoopX by making it a deterministic state kernel that outlives any single session or harness. They are complementary more than competitive: Multica organizes *who does what and who reviews it*; LoopX governs *whether a long-running goal may continue, on what evidence, and at what cost*. A new orchestration platform could do far worse than combining Multica's board/squad/mention semantics and runtime-daemon boundary with LoopX's kernel/quota/gate semantics and evidence discipline.

### Practical borrow-list (merged, prioritized)

1. Deterministic control plane with a five-question state kernel (LoopX) as the core; issue-board + mention-trigger + leader-routing as the human-legible projection on top (Multica).
2. Quota/tick protocol with spend-after-validated-writeback and scheduler-hint ACKs (LoopX) as the universal "should this agent act" chokepoint for every harness integration.
3. Run-scoped tokens + per-run workdir/agent-state + honest security docs (Multica) for the execution boundary; fail-closed tool gating with write_scope (LoopX) for in-harness enforcement.
4. Typed transitions and result-vs-progress distinction (LoopX) to replace "the agent said it's done."
5. Skills as refreshable workspace assets importable from GitHub/ClawHub/Skills.sh, split from agent identity instructions (Multica); authority-scoped reward memory, default-off (LoopX) for learning.
6. Agent-operable platform surface: one CLI + skill so agents can triage/rerun/link work themselves (Multica), and per-agent/per-issue token cost accounting with attention-cost signals (both).
7. Evidence-strength labels on every internal claim and demo (LoopX) as a project-culture default.
