# Knowledge-Graph Agent Memory + Karpathy Methodology — Research Report

**Date:** 2026-09-11
**Researcher:** research-kg-karpathy agent (tasking: team-lead)
**Scope:** (1) Andrej Karpathy's methodology — nanochat, context engineering, LLM OS, eval-driven iteration, 2026 output. (2) Knowledge-graph-based agent memory — Zep/Graphiti, Microsoft GraphRAG, LightRAG, cognee, Letta, VikingMem. (3) Synthesis: what to borrow for an embedded Rust implementation over SQLite + LanceDB.

**Verification key:** **[V]** = live-verified this session (GitHub REST API, raw.githubusercontent.com, arXiv API/ar5iv, simonwillison.net, docs sites — all via `curl`; WebSearch returned empty stubs and WebFetch was domain-blocked throughout, same degradation the OpenViking researcher hit). **[K]** = training knowledge (~Jan 2026 cutoff), could not be live-verified because x.com and mirrors are unreachable from this environment.

**Two headline corrections to the task premises:**

1. **nanochat has no LanceDB personal-memory system, no skills system, no web search, no RAG over notes.** Verified against the original announcement (discussion #1, Oct 13 2025, 27,399 chars — zero mentions of any of these), the Nov 28 2025 tree, the current master tree, and `pyproject.toml`. nanochat is a *training* harness (tokenizer → pretrain → midtrain → SFT → RL → eval → inference). What the task described is Karpathy's *agent workflow around* the repo (Claude Code + a `read-arxiv-paper` skill + a `knowledge/` notes directory) — which is real, live-verified, and arguably a better pattern to borrow. Details in §1.1–1.3.
2. **VikingMem is covered in depth in the sibling report `2026-09-10-openviking.md`**; this report covers the paper's memory model itself and positions it against the others (§2.6).

---

# PART 1 — Andrej Karpathy

## 1.1 nanochat — what it actually is [V]

**Repo:** [github.com/karpathy/nanochat](https://github.com/karpathy/nanochat) · created 2025-10-13 · MIT · Python (+Rust tokenizer) · **57,926 stars** as of 2026-09-11 · description: *"The best ChatGPT that $100 can buy."*

**Launch state (Oct 13 2025, discussion #1 "Introducing nanochat"):**

- Full-stack LLM pipeline in "a single, clean, minimal, hackable, dependency-lite codebase" — at launch ~8,000 lines, mostly Python, plus Rust for a custom BPE tokenizer (`rustbpe`, compiled with maturin/pyo3; "the Python version in my earlier minbpe project is way too slow and the huggingface tokenizers is too bloated"). Dependencies at launch: `datasets, fastapi, files-to-prompt, psutil, regex, tiktoken, tokenizers, torch, uvicorn, wandb` — **no lancedb, no vector store, no search SDK**. [V]
- Stages, all on one 8×H100 node (~$24/hr): train tokenizer (65,536 vocab, GPT-4-style BPE, ~1 min) → pretrain on FineWeb-EDU-100B (repackaged into shuffled parquet shards; ~3 hr for a d20, ~$72) → **midtraining** on SmolTalk 460K + MMLU auxiliary_train 100K + GSM8K 8K (teaches chat special tokens, multiple-choice quiz format, and the Python-interpreter tool via `<|python_start|>…<|python_end|>` interleave; ~8 min) → **SFT** on ~21.4K cherry-picked examples (ARC-Easy/Challenge, GSM8K, SmolTalk; ~7 min; pads examples to mimic test-time format, "another little tightening-the-screws boost") → **RL** on GSM8K (commented out by default) → evaluation → inference via `chat_cli` / `chat_web` (FastAPI + vanilla-JS UI, data-parallel worker pool per GPU).
- Chat schema: `<|user_start|>…<|user_end|><|assistant_start|>…<|assistant_end|>`, "loosely following the OpenAI Harmony chat format". The `Engine` class implements KV-cache prefill/decode and "supports tool use (of Python interpreter)". [V]
- **RL loop (the "local RL" the task asked about):** a deliberately simplified GRPO — "we don't use trust regions (throw away reference model and KL regularization), we are on policy (throw away the PPO ratios+clip), we use GAPO style normalization (token-level…), and the advantage is simple reward shift by mean… So we're left with something that looks quite a bit more like REINFORCE, but keeping the GR ('group relative') part." Iterates GSM8K: sample completions → reward correct answers → train on high-reward rollouts, interleaving sampling and training. ~1.5 h; "RL is sucking supervision bits through a straw"; creates "a GSM-specific model, not a general chat model." [V]
- Report card: every run emits a `report.md` with a metrics table across BASE/MID/SFT/RL (CORE, ARC, MMLU, GSM8K, HumanEval, ChatCORE — his own baseline-subtracted ensemble metric) and a cost accounting ("$8 left for ice cream"). [V]

**2026 pivot — pretraining speedrun focus [V].** Current master has dropped midtrain/SFT-centric framing; the repo is now organized around a **"Time-to-GPT-2" leaderboard**: wall-clock time on 8×H100 to beat GPT-2's DCLM CORE score of 0.256525. Progression: 168 h (OpenAI 2019, ~$43K) → 3.04 h (Jan 29 2026, d24) → 2.91 h (+fp8) → 2.76 h (1M-token batch) → 2.02 h (NVIDIA ClimbMix dataset, Mar 2026) → 1.80 h (**"autoresearch round 1"**, Mar 9 2026) → 1.65 h (autoresearch round 2, Mar 14 2026). "Autoresearch" = Karpathy running an LLM-driven research loop to propose/implement speedrun improvements (README links the X threads; X unreachable from here, so beyond the leaderboard rows this is **[K]**). The Jan 31 2026 framing, quoted by Simon Willison [V]: a **600× cost reduction over 7 years, ≈2.5×/year**. File tree now: `nanochat/{gpt,optim,engine,tokenizer,fp8,flash_attention,core_eval,…}.py`, `scripts/{base_train,base_eval,chat_sft,chat_rl,chat_eval,chat_cli,chat_web}.py`, `tasks/`, `runs/`, plus `dev/LOG.md` (experiment log) and `dev/LEADERBOARD.md`.

## 1.2 The memory system that actually exists: agent + skills + notes, not LanceDB [V]

What the task brief described ("personal memory with LanceDB, skills, RAG over notes, web search") does not exist in nanochat's code. What exists instead — and what almost certainly got conflated — is **Karpathy's Claude Code configuration inside the repo**, which is live-verified and is a complete, minimal, borrowable memory design:

- **`.claude/skills/read-arxiv-paper/SKILL.md`** — a 40-line skill that instructs the agent, given an arXiv URL: (1) normalize to the `/src/` TeX-source URL; (2) download to `~/.cache/nanochat/knowledge/{arxiv_id}.tar.gz` (skip if cached); (3) unpack; (4) find the LaTeX entrypoint; (5) read and recurse through source files; (6) "produce a summary of the paper into a markdown file at `./knowledge/summary_{tag}.md`… generate some reasonable tag like `conditional_memory`… you're processing this paper within the context of the nanochat repository, so most often we will be interested in how to apply the paper and its lessons to the nanochat project… explicitly make the connection of how this paper might relate to nanochat."
- **A local `knowledge/` directory** of those agent-written, topically-tagged paper summaries. It is referenced by `dev/LOG.md` ("Cached notes are in `knowledge/parameter_golf.md`", "saved to `knowledge/muonh.md`") but absent from the git tree — i.e. **gitignored personal notes**. `.gitignore` confirms: `dev-ignore/`, `report.md`, `eval_bundle/`, `.env`, **`CLAUDE.md`** (his private agent instructions), `wandb/`. [V]

So Karpathy's de-facto memory lifecycle is:

| Phase | Mechanism |
|---|---|
| **Write** | Event-driven, agent-performed: when he asks the agent to read a paper, the skill mandates writing `knowledge/summary_{tag}.md`, connected to project code. No automatic capture of conversations. |
| **Index** | None. Retrieval is by filename tag and by the agent re-reading files on demand; the skill explicitly tells the agent to "remind yourself" of related project code when summarizing. |
| **Retrieve/inject** | The agent (or Karpathy) opens the relevant note; `CLAUDE.md` is auto-injected every session. Human- and agent-readable Markdown, version-able, no embeddings. |

This is the same design family the industry converged on in 2026: Letta Code's git-backed **MemFS** (§2.5), OpenViking's `viking://` filesystem (sibling report), and Claude Code's own skills/memory. The borrowable insight: **at single-user scale, "filesystem + agent-written tagged Markdown + a private instruction file" beats an embedding pipeline for trust, debuggability, and cost — semantic search is an accelerator layered on top, not the substrate.**

## 1.3 "Context engineering > prompt engineering" (June 2025) [V]

Origin: Shopify CEO Tobi Lütke's tweet adopting the term; Karpathy's amplifying reply (late June 2025), full text as preserved by Simon Willison (2025-06-27) [V]:

> "+1 for 'context engineering' over 'prompt engineering'. People associate prompts with short task descriptions you'd give an LLM in your day-to-day use. When in every industrial-strength LLM app, context engineering is the delicate art and science of **filling the context window with just the right information for the next step**. Science because doing this right involves task descriptions and explanations, few shot examples, RAG, related (possibly multimodal) data, tools, state and history, compacting. […] Doing this well is highly non-trivial. And art because of the guiding intuition around LLM psychology of people spirits. […] Unfortunately, most people's inferred definition [of prompt engineering] is that it's a laughably pretentious term for typing things into a chatbot! It turns out that inferred definitions are the ones that stick."

Design consequences he draws: context assembly is a *systems* problem (RAG, tools, state, history, compaction), the unit of optimization is "the next step" not the single prompt, and compaction is a first-class context operation. Follow-on 2025–2026 statements in the same vein, all live-verified via Simon Willison's archive:

- **Oct 2025, Dwarkesh Podcast ("AGI is still a decade away", 2h25m)** [V]: agents should be thought of as "an employee or an intern that you would hire to work with you"; they fail today because "they just don't work… they don't have continual learning. You can't just tell them something and they'll remember it. They're cognitively lacking." His blog post **"Animals vs Ghosts"** frames LLMs as "ethereal spirit entities… fully digital and mimicking humans," not animals — "we're not building animals… it's also possible to make them a bit more animal-like over time." On harnesses: asked about his tweet saying Claude Code and Codex CLI were "didn't work well enough at all and net unhelpful" for his nanochat project, he explained agents excel at "boilerplate code that's just copy-paste… very good at stuff that occurs very often on the Internet," while nanochat is "a fairly unique repository" with little training-set precedent. (The "Animals vs Ghosts" post itself was not fetchable — his bearblog returned 404 — so its text is **[K]**.)
- **Feb 26 2026** [V]: "It is hard to communicate how much programming has changed due to AI in the last 2 months… imo coding agents basically didn't work before December and basically work since — the models have significantly higher quality, long-term coherence and tenacity and they can power through large and long tasks."
- **Jun 9 2026** (on Claude Fable 5) [V]: "I feel a lot of things changing as working software increasingly comes out on a tap. The Jevon's paradox kicks in and I feel my own demand for software growing substantially… 'Free your mind'."

## 1.4 The LLM OS framing (Oct 2023) [K]

The tweet/thread (Oct 15–16, 2023; X unreachable from this environment, so **[K]** from training knowledge — it is among the most-quoted artifacts in the field): Karpathy sketched the LLM as the **kernel of an emerging operating system**: the **context window is RAM**, tools (browser, Python, DALL-E, APIs) are **peripherals**, the file system / internet is **disk**, and vector databases provide the **memory hierarchy beyond RAM**; applications/agents are processes the kernel multiplexes; fine-tuning ~ installing software. The diagram famously put the LLM in the center with "Context Window (RAM)" above it and peripherals around it.

Influence on agent-memory design — the framing is the direct ancestor of every tiered-memory system in Part 2:

- **MemGPT/Letta** (Oct 2023, weeks later) explicitly re-cast it as OS memory management: main context = RAM, external stores = disk, the agent pages memory in/out itself via tool calls ("self-editing memory" = processes managing their own pages).
- **Zep/Graphiti** answers "what is the disk?" with a temporal knowledge-graph service; retrieval = page-in, invalidation = cache coherence.
- **OpenViking** (sibling report) literally names the product a "context database" and tiers content L0/L1/L2 like an L1/L2/L3 cache hierarchy for tokens.
- For our platform the mapping is direct: context window = RAM (scare, must be curated per step), SQLite+LanceDB = disk (durable, queryable), recall = demand paging, compaction = eviction, embeddings = an index over disk, and "context engineering" = the scheduler.

## 1.5 Methodology: eval-driven iteration, transcripts, simplicity-first

Live-verified artifacts [V]:

- **`dev/LOG.md` — a dated experiment log, mostly negative results.** Entries like "2026-05-05: DyT for d12 pretraining (negative)", "2026-02-19: Mixture of Experts (negative)", "2026-02-05: SwiGLU Activation (Negative Result)", "2026-01-15: Olmo pretraining mix (Negative result)", each with Rationale / Ideas Tried / Result. This is the eval-driven iteration loop made visible: change one thing, re-run a d12 (~6 min), watch `val_bpb`/`core_metric`/MFU on wandb, record the verdict even when it's "no". The README: "I like to change something in the code, re-run a d12 (or a d16 etc) and see if it helped, in an iteration loop."
- **Honest-metric discipline.** He rejected validation-loss comparisons as gameable ("using very long sequences and batch size 1… the resulting 'improvement' is not real"; "Only actual metrics are real and comparable") and rebuilt DCLM's CORE metric as a single dependency-free file. The leaderboard requires reporting wall-clock training time *excluding* eval/logging. [V]
- **Simplicity as an architectural constraint.** "nanochat is not an exhaustively configurable LLM 'framework'; there are no giant configuration objects, model factories, or if-then-else monsters… a single, cohesive, minimal, readable, hackable, maximally-forkable 'strong baseline'." One dial — `--depth` — derives every other hyperparameter so the whole miniseries stays compute-optimal; any change "must be principled enough that it works for all settings of depth." Merge bar includes aesthetics: rejected if "it is gnarly or it significantly bloats the code." PRs must declare LLM contribution ("Current AI policy: disclosure"). [V]
- **Synthetic-data design craft** (discussion #164, "counting r in strawberry") [V]: randomize user-prompt phrasings (incl. other languages) so the ability is "triggered" robustly; design the reasoning trace (manual attempt → Python double-check) in SFT as a *prior*, then "RL to take over and actually find a way to string it all together"; tokenizer-aware formatting (surrounding the word in quotes to force token boundaries). "In practice, you usually want a bit of both!" (SFT prior + RL practice).
- **Transcript-reading as the core debugging practice [K].** His repeated advice across talks (e.g., "Deep Dive into LLMs like ChatGPT", Feb 2025 video [K]; YC AI Startup School "Software 3.0" talk, June 2025 [K]) is to read the model's/agent's actual transcripts, build tiny personal evals, and iterate on failure cases rather than trust aggregate benchmarks. The Dwarkesh interview [V] extends this to agents: judge them like interns, on concrete work samples; boilerplate-heavy work succeeds, unprecedented work fails.
- **2026 output worth noting:** the autoresearch speedrun rounds (Mar 2026) [V-README]; community models built *with* nanochat (e.g., "Mr. Chatterbox", Mar 2026 — a 340M model trained by Trip Venturella on 28K Victorian-era British Library texts via nanochat, explicitly *not* a Karpathy project) [V]; the "Beating GPT-2 for <<$100" writeup (discussion #481, Jan 31 2026) with the full architecture deep-dive (RoPE, parameter-free RMSNorm, QK-norm, untied embeddings, ReLU², logit softcapping, SSSL sliding-window pattern, value embeddings, per-layer residual scalars, Muon+AdamW split optimizer) [V].

**Borrow-list from Karpathy (for an agent platform):** one obvious dial per concept; dated experiment logs with negative results; honest metrics with explicit anti-gaming rules; a public leaderboard for anything speed/runnable; SFT-prior-then-RL-practice for tool loops; agent-written tagged Markdown notes in a repo-local `knowledge/` dir; skills as tiny procedural `SKILL.md` files; private `CLAUDE.md`; disclosure norms for LLM-written code; simplicity enforced at merge time.

---

# PART 2 — Knowledge-graph-based agent memory

## 2.1 Zep / Graphiti — temporal knowledge graph memory [V]

**Repo:** [github.com/getzep/graphiti](https://github.com/getzep/graphiti) · Apache-2.0 · Python · 30,767 stars · created 2024-08. Paper: **"Zep: A Temporal Knowledge Graph Architecture for Agent Memory"** (arXiv:2501.13956, Jan 2025). Usable as a **library** (`pip install graphiti-core`), as an **MCP server**, or as a FastAPI REST service. Graph backends: Neo4j 5.26, FalkorDB 1.1.2 (incl. embedded `falkordblite`), Amazon Neptune + OpenSearch; Kuzu deprecated (upstream unmaintained). LLM defaults to OpenAI; Anthropic/Gemini/Groq and any OpenAI-compatible/local endpoint (Ollama, vLLM, llama.cpp) supported. Commercial **Zep** = managed platform on a proprietary "Context Graph Engine" (sub-200 ms retrieval at scale); Graphiti is the OSS core you self-assemble.

**The model — a "context graph" with four node kinds:**

| Component | Stores |
|---|---|
| Entities (nodes) | People/products/policies/concepts, with summaries that evolve over time |
| Facts / Relationships (edges) | Triplets with **temporal validity windows** |
| Episodes (provenance) | Raw data as ingested — the non-lossy ground-truth stream; every derived fact traces back here |
| Custom types | Prescribed ontology via Pydantic models, or learned/emergent structure |

**Bi-temporal model (the signature idea).** Two timelines: **T** = chronological order of events (when facts were true in the world), **T′** = transactional order of ingestion (database audit). Every edge carries **four timestamps**: `t_valid, t_invalid ∈ T` (when the fact held true) and `t′_created, t′_expired ∈ T′` (when the system created/invalidated the record). Extraction resolves relative dates ("I started my new job two weeks ago") against a reference timestamp `t_ref` on each message. **Edge invalidation:** when a new edge arrives, an LLM compares it against semantically related existing edges; on a temporally overlapping contradiction, the old edge's `t_invalid` is set to the new edge's `t_valid` — old facts are *invalidated, not deleted*, so you can query "what was true at any point in time." On the T′ timeline, new information consistently wins.

**Entity resolution.** Entity names embedded in 1024-d space; cosine similarity over existing nodes **plus** full-text search over names/summaries yields candidates; candidates + episode context go through an LLM resolution prompt; duplicates get merged name/summary. Writes use **predefined Cypher queries, not LLM-generated queries**, "to ensure consistent schema formats and reduce the potential for hallucinations." A Reflexion-inspired reflection pass after initial extraction reduces hallucination and improves coverage. Edge dedup hybrid search is **constrained to edges between the same entity pair** — prevents cross-entity confusion and cuts the search space; the same fact extracted between multiple entity pairs yields hyper-edges.

**Communities.** Clusters of strongly connected entities with LLM-generated summaries — but built with **label propagation, not Leiden**, explicitly because label propagation has "a straightforward dynamic extension" that keeps communities accurate incrementally, "delaying the need for complete community refreshes" (a direct jab at GraphRAG's batch recomputation). Community names (key terms extracted from summaries) are embedded for cosine search.

**Retrieval — three-stage pipeline:**
1. **Search (φ):** cosine semantic similarity + **Okapi BM25** full-text + **breadth-first search** (n-hop neighborhood expansion around hits; the paper notes this parallels LightRAG's keyword-search methodology). Returns (semantic edges, entity nodes, community nodes).
2. **Rerank (ρ):** RRF or MMR; plus graph-native rerankers — an **episode-mentions reranker** (frequency of entity/fact mentions in the conversation) and a **node-distance reranker** (graph distance from a designated centroid node).
3. **Constructor (χ):** formats the selected nodes/edges into text context.

Hybrid retrieval is explicitly designed "without reliance on LLM summarization" at query time — the LLM cost lives in the *write* path.

**Results:** DMR benchmark 94.8% vs MemGPT's 93.4%; LongMemEval accuracy +18.5% with **90% lower response latency** vs full-context baselines. **Cost profile:** every ingested episode costs multiple LLM calls (entity extraction, reflection, resolution, edge dedup, temporal extraction, invalidation checks); default concurrency is capped (`SEMAPHORE_LIMIT=10`) to avoid provider 429s; the README warns it "works best with LLM services that support Structured Output… particularly problematic… using smaller models" — extraction failures on small local models are the #1 operational risk.

## 2.2 Microsoft GraphRAG — hierarchical community summaries [V]

**Repo:** [github.com/microsoft/graphrag](https://github.com/microsoft/graphrag) · MIT · Python · 35,925 stars · created 2024-03. **Status: maintenance mode** — README warning: "This project is largely in maintenance mode, and won't be accepting new PRs or implementing new features" (bug fixes/CVEs only). Also: "GraphRAG indexing can be an expensive operation." Paper: "From Local to Global: A Graph RAG Approach to Query-Focused Summarization" (arXiv:2404.16130).

**Indexing (write path):**
- **Standard method:** LLM does everything — entity extraction (named entities + descriptions per text unit), relationship extraction (per entity pair), entity/relationship summarization (merge descriptions across occurrences), optional claim extraction, and **community report generation**. Communities come from **hierarchical Leiden clustering** (multi-level community hierarchy; Leiden per the Zep paper's characterization [V], details of the hierarchy from the original paper [K]).
- **FastGraphRAG:** replaces LLM reasoning with NLP — noun-phrase entities via NLTK/spaCy, relationships = text-unit co-occurrence, no descriptions/summaries; community reports from raw text. "A faster and cheaper indexing alternative."

**Query (read path) — four modes:**
- **Local search:** entity-anchored. Query → semantically related entities → their connected entities, relationships, covariates, source text units, and community reports; rank + filter each candidate set; combine into context. Good for "specific entities mentioned in the documents."
- **Global search:** **map-reduce over all community reports** at a chosen level of the community hierarchy — each report chunk produces a rated intermediate response, then intermediate responses are aggregated. "Resource-intensive"; per Zep's comparison table, query latency is "seconds to tens of seconds" (vs Graphiti's sub-second). Good for whole-corpus sensemaking ("What are the main themes in the dataset?") where vector RAG "performs terribly because there is nothing in the query to direct it to the correct information."
- **DRIFT search** (Dynamic Reasoning and Inference with Flexible Traversal): **primer** on the top-K semantically relevant community reports (broad initial answer + follow-up questions) → **follow-up** local searches refining into more intermediate answers/questions, with per-node confidence on continuing expansion → **hierarchical output** of Q&As ranked by relevance. Explicitly "balances computational costs with quality."
- **Basic search:** plain vector RAG baseline for comparison.

**When it's worth it:** static corpora, offline indexing budgets, and questions that are *query-focused summarization over the whole corpus*. Not worth it as a live agent memory: batch-oriented, expensive indexing, high-latency global queries, low adaptability (all per Graphiti's own comparison table and the maintenance-mode status). The durable ideas to steal: **community reports as a precomputed corpus-level digest**, and **DRIFT's primer-then-refine query pattern** as an *optional* deep-research mode.

## 2.3 LightRAG — dual-level KG+vector hybrid [V]

**Repo:** [github.com/HKUDS/LightRAG](https://github.com/HKUDS/LightRAG) · MIT · Python · **39,544 stars (largest of the group)** · created 2024-10 · EMNLP 2025. Paper: arXiv:2410.05779.

**Indexing:** LLM extracts entities + relationships from chunks (entities get descriptions; high-degree entities double as "global" keys); everything embedded. **Incremental update algorithm** integrates new documents without rebuilding the index; selective deletion reuses the indexing-time LLM cache to rebuild affected entities/relations.

**Retrieval (the fusion pattern):**
1. **Query keyword extraction** — an LLM extracts **local keywords** k⁽ˡ⁾ and **global keywords** k⁽ᵍ⁾ from the query (in the product this is the `KEYWORD` role, which "must use a non-thinking model to keep query latency low").
2. **Keyword matching** — vector search matches local keywords to candidate *entities*, global keywords to *relations* linked to global keys.
3. **High-order relatedness** — gather the **one-hop neighbors** 𝒩ᵥ ∪ 𝒩ₑ of retrieved nodes and edges.
4. Context = concatenated entity/relation descriptions (the "profiling function"), fed to the answering LLM.

So: low-level = specific entities/relations; high-level = topics/themes via global keys; the fusion is *keyword-typed vector lookup + one-hop graph expansion*, deliberately avoiding "inefficient community reports or multi-hop reasoning" — fewer LLM calls at index and query time than GraphRAG. **Query modes:** `naive` (chunk vector RAG), `local`, `global`, `hybrid` (local+global), `mix` (all three — **default**); a cross-encoder **reranker is default since Aug 2025**. **Role-specific LLMs** (EXTRACT / QUERY / KEYWORDS / VLM) with independent settings since May 2026 — cheap fast models for latency-critical roles. **Storage:** defaults are in-memory with file persistence (JsonKVStorage, NanoVectorDBStorage, NetworkXStorage, JsonDocStatusStorage) — "only for small-scale testing"; **PostgreSQL is the recommended production backend and can serve all four storage types alone**; Neo4j/Memgraph (graph), Milvus/Qdrant (vector), MongoDB, OpenSearch as alternatives. Embedding model is locked at index time (changing it requires re-embedding everything).

## 2.4 cognee — memory platform with sessions + auto-routed recall [V]

**Repo:** [github.com/topoteretes/cognee](https://github.com/topoteretes/cognee) · Apache-2.0 · Python · 30,622 stars · created 2023-08. Positions itself as "the open-source AI memory platform for agents… self-hosted knowledge graph engine." Paper: "Optimizing the Interface Between Knowledge Graphs and LLMs for Complex Reasoning" (arXiv:2505.24478).

**Model:** four operations — `remember` / `recall` / `improve` / `forget`. Text becomes entities + relationships + searchable chunks; **code becomes a graph of symbols and dependencies**; **session distillation curates accepted lessons into permanent memory**. `remember(..., session_id=…)` writes to a **session memory (fast cache) that syncs to the graph in background**; `recall` uses **auto-routing** ("picks best search strategy automatically") and checks session memory first, falling through to the graph. Ontologies via custom data models (Pydantic-style). Ships a **COGX exchange format** to import memory from Mem0/Letta/Zep/Graphiti; Claude Code + Codex plugins, MCP server, Python/TS SDKs, REST, and **`cognee-rs` (a Rust client)**. **cognee 1.0: run the entire memory layer on a single Postgres instance** (relational metadata + PGVector + graph together) — currently a demo feature, production variant licensed. BEAM self-evaluation: 0.79 @100K tokens, 0.67 @10M (with published methodology caveats). The paper's finding: systematic hyperparameter tuning of chunking/graph-construction/retrieval/prompting yields consistent but non-uniform gains on multi-hop QA (HotPotQA, 2WikiMultiHop, MuSiQue) — i.e., **the KG-LLM interface is tunable engineering, not a fixed recipe.**

## 2.5 Letta (ex-MemGPT) — from memory blocks to a git-backed memory filesystem [V]

**Repos:** `letta-ai/letta` (24,692 stars) is now a **landing page**; the retired Python V1 server lives on the `archive` branch; active development is **`letta-ai/letta-code`** — TypeScript, Apache-2.0, npm `@letta-ai/letta-code`, CLI + desktop app + Agent SDK, with **Letta Cloud** holding agent state across machines. Research lineage: MemGPT (arXiv:2310.08560) and **sleep-time compute** (arXiv:2504.13171 — "let models think offline about contexts before queries are presented"; ~5× test-time compute reduction, up to +13–18% accuracy, 2.5× cheaper per query when amortized across related queries).

**Classic V1/MemGPT memory model [K + V remnants]:** three tiers — **core memory blocks** (persona + human blocks, always in-context, *self-edited by the agent* via tools like `core_memory_append`/`core_memory_replace`), **recall memory** (searchable conversation history), **archival memory** (long-term vector store). The new SDK's `createAgent({ human, persona })` still exposes exactly this interface [V].

**Letta Code's current design [V] — a full pivot to the filesystem pattern:**
- **MemFS: a git-backed memory filesystem** the agent inspects and edits; "all context (including memory blocks) is tracked via git" and can sync to a GitHub repo (`/memory-repository set git@…`). Memory is versioned, diffable, and inspectable.
- Memory lifecycle commands: **`/init`** (bootstrap memory for a project — agent inspects the repo, asks about working style, reviews prior sessions via subagents), **`/remember`** ("the agent decides where the lesson belongs and commits the update to MemFS"), **`/doctor`** (audit placement, duplication, system-prompt token usage), **`/palace`** (memory viewer), and **reorganize** (backs up, then splits large files / merges duplicates / restructures).
- **Dreaming** (productized sleep-time compute): **background subagents review recent conversations, consolidate useful lessons, and update memory without interrupting active work** — triggered after N agent steps or on context compaction; optional "agent reviews before applying" pass. Tagline: agents "learn and evolve over long horizons through rewriting their own memory, skills, prompts, and even the harness itself (through mods)."
- Skills at three scopes — global (`~/.letta`), project (`.agents/skills`), agent-scoped (stored in MemFS); installable from GitHub/ClawHub; subagents (general-purpose, forked, recall, history-analyzer); hooks, permissions, crons/heartbeats, channels (Slack/Telegram/Discord/WhatsApp/Signal).

## 2.6 VikingMem (arXiv:2605.29640, VLDB 2026) — the Memory Base paradigm [V]

Paper: "VikingMem: A Memory Base Management System for Stateful LLM-based Applications" (Fu et al., published 2026-05-28; implemented on ByteDance's VikingDB vector engine; open-sourced as OpenViking — see the sibling report `2026-09-10-openviking.md` for the full system walkthrough).

**The paradigm — "Memory Base," three principles [V, from the abstract]:**
1. **Selective extraction of high-value memories** from raw information streams (vs "simplistic extraction methods that lead to incomplete memories" or "rigid, single-purpose memory extraction prompts tailored to a single use case, such as chatbots").
2. **Inherent statefulness and evolution** — memory content is *progressively summarized, corrected, and temporally weighted to prioritize recent interactions*.
3. **A generalizable abstraction paradigm** for transfer across applications (education, recommendation, agent memory).

**Mechanics:** interconnected **event** and **entity** abstractions — **event-centric extraction** selectively handles complex information streams, while **entities are dynamically updated by events** to achieve stateful evolution. **Temporal compression via a topic-wise timeline** and **time-weighted recall** progressively produce high-level summary memories, prioritize recent items, and "compress and fade" older ones. Claimed: up to **30% better memory-retrieval effectiveness** on long-term-memory benchmarks at interactive latencies (OpenViking's LoCoMo runs: 80–83% with memory vs 24–57% without, with 34–91% fewer input tokens).

**Positioning vs the others:**
- vs **Graphiti/Zep**: VikingMem is *event-centric* — events are first-class and mutate entity state over a topic timeline; Graphiti is *fact/edge-centric* — edges carry validity windows and get logically invalidated. Graphiti's bi-temporal model is more precise for point-in-time queries; VikingMem's decay/compression is more aggressive about token economics (old memories literally fade).
- vs **GraphRAG**: operational conversational memory with recency, not corpus sensemaking; no community reports.
- vs **Letta**: evolution happens in the database (server-side consolidation), not by the agent self-editing in-context blocks.
- vs **LightRAG**: the innovation is the *write/lifecycle* side (extraction selectivity, summarization, fading) rather than the retrieval-fusion side.
- The OpenViking implementation adds the parts the paper doesn't specify: `viking://` URI filesystem, L0/L1/L2 tiered sidecars, schema-driven memory types with per-field merge ops, `memory_diff.json` audit, two-phase session commit, hierarchical recursive retrieval (all detailed in the sibling report — borrow-list there overlaps heavily with §3 below).

---

# PART 3 — SYNTHESIS: what to build in embedded Rust over SQLite + LanceDB

Target: single-user, local-first, no server process, property graph as SQLite adjacency tables + vectors in LanceDB. Filter every pattern above through that lens.

## 3.1 Borrow — high value, low LLM cost

1. **Bi-temporal validity columns on edges (Graphiti).** Pure schema, zero marginal cost: `valid_at`, `invalid_at` (event time T) plus `created_at`, `expired_at` (ingestion time T′) on every fact row. Enables "what was true as of X" queries and clean supersession. SQLite does this trivially; index `(invalid_at)` for "current facts" scans. This is the single highest-value idea in the whole survey for a durable memory store.
2. **Episodes as the non-lossy base layer (Graphiti).** Store raw turns/documents first (`episodes(id, kind, content, content_hash, ref_time, ingested_at)`); derived entities/edges reference their source episodes via join tables. Content-hash dedup makes ingestion idempotent; the graph is *rebuildable* from episodes if extraction logic changes (same principle as OpenViking's "FS is source of truth, index is derived").
3. **Hybrid retrieval without an LLM in the loop (Graphiti's φ + LightRAG's fusion).** The mapping to our stack is exact: cosine search → **LanceDB** ANN; BM25 → **SQLite FTS5** (built in); graph walk → **recursive CTEs over adjacency tables** (1–2 hops). Fuse with **RRF** (trivial in Rust), then the temporal filter `valid_at ≤ now AND (invalid_at IS NULL OR invalid_at > now)` plus a recency decay. LightRAG adds the cheap trick of *typed keywords* (local→entities, global→relations) — with a local model this can even be skipped in favor of embedding-the-query-against-both-tables. Zep's node-distance and episode-mentions rerankers are pure SQL/arithmetic.
4. **Time-weighted recall (VikingMem).** `score *= f(age)` with topic-timeline compression of old events into summaries. No LLM calls needed for the weighting itself; run the summarization lazily/nightly.
5. **Tiered summaries L0/L1/L2 (OpenViking) = LightRAG's dual-level, generalized.** Per-entity and per-topic: a ~256-char abstract (embedded, the retrieval unit), a ~4K-char overview (the rerank unit), full content on demand. Generate on write, refresh with stable sampling; this is the token-economics engine.
6. **Session memory + async background consolidation (cognee sessions, Letta dreaming, OpenViking two-phase commit).** The write path must never block the agent: append raw turn to SQLite synchronously (microseconds), enqueue extraction/consolidation on a background task, persist the queue so it survives crashes, make extraction idempotent by content hash. "Dreaming" (re-reading recent sessions to distill lessons) is a great nightly cron — it's sleep-time compute applied to memory.
7. **Schema-driven memory types with per-type merge semantics (OpenViking).** Each memory type (preference, entity profile, event, project note) declares its fields and `merge_op`s (immutable identity fields vs patchable content), which *constrains* the LLM's dedup/merge decisions into predictable, auditable operations. Pair with a **memory diff log** (before/after per change, reason-coded skips) — nearly free, enormous trust dividend.
8. **Entity resolution, cheap version (Graphiti).** Embed entity names + FTS5 over names/summaries → candidates → confirm via LLM only when the merge is consequential; constrain edge-dedup search to the same entity pair (Graphiti's own complexity cut). At single-user scale, a deterministic pass (exact/normalized name match, content hash) resolves most duplicates with zero LLM cost.
9. **Memory as files, git-versioned (Letta MemFS, Karpathy's `knowledge/`, `viking://`).** Even with SQLite+LanceDB underneath, expose memory as a browsable, exportable file tree (Markdown with frontmatter) — humans can audit and edit, agents can `ls`/`read`, git gives versioning and diff for free, and it degrades gracefully if the index is lost. SQLite remains the index/cache, not the only representation.
10. **Skills as tiny SKILL.md procedures (Karpathy, Letta, Claude Code).** Not part of the graph, but part of the memory platform: procedural memory is files-on-disk with a trigger description; the read-arxiv-paper skill is the canonical minimal example (trigger → procedure → write-to-knowledge-dir → connect-to-project).
11. **Explicit `/remember` + selective auto-capture (Letta + VikingMem's principle 1).** Two write paths: explicit, high-trust, user/agent-invoked writes (always kept, verbatim), and automatic extraction (cheap model, hash-deduped, schema-constrained, marked lower-confidence until consolidated). VikingMem's core finding is that *selectivity beats completeness*.
12. **DRIFT's primer-then-refine as an optional deep mode (GraphRAG).** For research-style tasks: start from top-K topic/community summaries, generate follow-up questions, refine with local graph search — but as an explicitly-invoked mode with a step budget, never the default recall path.

## 3.2 LLM-cost traps to avoid

1. **GraphRAG global search as a default recall path** — map-reduce over community reports *per query*; seconds-to-tens-of-seconds latency and per-query token spend. Reserve corpus-level digesting for offline jobs (and note the project itself is in maintenance mode — the field moved to incremental designs).
2. **Batch full-reindex on every change** (GraphRAG Standard's assumption). All four newer systems converged on incremental updates; design for append-only episodes + delta extraction from day one.
3. **Community (re)summarization on the write path.** Graphiti chose label propagation over Leiden specifically to defer community refreshes; OpenViking documents parent-summary "bubbling" write amplification as an unsolved TODO. If you want communities at all: compute them **offline** (Leiden is fine in a nightly job over SQLite adjacency — `graphrb`/`petgraph` have Louvain/Leiden implementations), cache the summaries, and treat them as derived data with a freshness counter.
4. **LLM in the retrieval loop.** Query expansion, DRIFT follow-ups, LLM reranking — every one adds latency and cost per *query*, which is the hot path. Keep query-time LLM usage at zero (RRF + arithmetic rerankers), and put the LLM budget in the *write* path where it amortizes across future queries.
5. **Unbounded per-episode extraction fan-out.** Graphiti's pipeline is extract → reflect → resolve → dedup → temporal-extract → invalidate-check per episode. At single-user volume that's fine, but: batch episodes, cache by content hash, skip reflection for trivial turns, and make every stage independently disableable with a latency fuse (OpenViking's 5s/30s fuses are the pattern).
6. **Embedding-model lock-in.** LightRAG documents that changing the embedding model requires re-embedding everything. Store the embedding model id + dims per LanceDB table and version tables on migration.
7. **Small-model structured-output fragility.** Graphiti's #1 operational warning: extraction with small local models fails schema validation. Mitigate with strict JSON schemas, retry-with-repair, and a deterministic fallback (store the raw episode even when extraction fails — episodes are non-lossy by design).
8. **Claims/covariate extraction and other optional enrichments** (GraphRAG's optional claim extraction) — nice-to-have tiers, off by default.

## 3.3 A concrete minimal schema (starting point)

```sql
-- non-lossy base layer
CREATE TABLE episodes (
  id INTEGER PRIMARY KEY, kind TEXT, content TEXT, content_hash TEXT UNIQUE,
  ref_time TEXT, ingested_at TEXT, session_id TEXT
);
-- property graph as adjacency tables
CREATE TABLE entities (
  id INTEGER PRIMARY KEY, name TEXT, norm_name TEXT, type TEXT, summary TEXT,
  l0_abstract TEXT, created_at TEXT, updated_at TEXT
);
CREATE TABLE edges (
  id INTEGER PRIMARY KEY, src INTEGER REFERENCES entities(id),
  dst INTEGER REFERENCES entities(id), relation TEXT, fact_text TEXT,
  l0_abstract TEXT,
  valid_at TEXT, invalid_at TEXT,          -- event time (T)
  created_at TEXT, expired_at TEXT,        -- ingestion time (T′)
  confidence REAL, source_episode_ids TEXT -- JSON array (provenance)
);
CREATE TABLE edge_episodes (edge_id INTEGER, episode_id INTEGER);  -- join
CREATE VIRTUAL TABLE entities_fts USING fts5(name, summary, l0_abstract);
CREATE VIRTUAL TABLE edges_fts USING fts5(fact_text, relation);
-- memory types (OpenViking-style, schema-driven)
CREATE TABLE memories (
  id INTEGER PRIMARY KEY, type TEXT,  -- preference|profile|event|lesson|note
  topic TEXT, content TEXT, content_hash TEXT,
  confidence REAL, created_at TEXT, updated_at TEXT,
  supersedes INTEGER REFERENCES memories(id)
);
CREATE TABLE memory_diffs (            -- audit log
  id INTEGER PRIMARY KEY, ts TEXT, op TEXT, mem_type TEXT,
  before TEXT, after TEXT, reason TEXT
);
```

LanceDB tables (derived, rebuildable): `entity_vecs(entity_id, vec, text_embedded)`, `fact_vecs(edge_id, vec, fact_text)`, `episode_vecs(episode_id, vec)`, `memory_vecs(memory_id, vec)`. Query path: embed query → ANN over `fact_vecs`∪`entity_vecs` ∪ FTS5 on both → RRF → 1–2-hop expansion via `edges` joins → temporal filter + recency decay → optional rerank → context constructor. Write path: append episode → hash-dedup → background extraction (entities/edges/memories) with schema-constrained JSON → resolution/dedup constrained to same entity pair → invalidation via `UPDATE edges SET invalid_at = ?` → embed → done.

---

## Sources

**Karpathy / nanochat [V unless noted]:**
- nanochat repo + README (master, 2026-09): https://github.com/karpathy/nanochat (via GitHub API / raw)
- Original announcement discussion #1 (Oct 13 2025), #164 abilities guide, #420 miniseries, #481 journey writeup: https://github.com/karpathy/nanochat/discussions/1 (via GitHub API)
- `dev/LOG.md`, `dev/LEADERBOARD.md`, `.gitignore`, `.claude/skills/read-arxiv-paper/SKILL.md` (master, via raw.githubusercontent.com)
- Simon Willison archive: /2025/Jun/27/context-engineering/, /2025/Oct/13/nanochat/, /2025/Oct/18/agi-is-still-a-decade-away/, /2026/Jan/31/andrej-karpathy/, /2026/Feb/26/andrej-karpathy/, /2026/Jun/9/andrej-karpathy/, /2026/Mar/30/mr-chatterbox/ (https://simonwillison.net/)
- LLM OS tweet (Oct 2023) [K — x.com unreachable]; "Animals vs Ghosts" essay [K — blog 404]; "Deep Dive into LLMs" video (Feb 2025) [K]; YC AI Startup School "Software 3.0" talk (Jun 2025) [K]; autoresearch X threads (Mar 2026) [V-README rows, K details]

**KG memory [V unless noted]:**
- Graphiti README + repo metadata: https://github.com/getzep/graphiti · Zep paper: arXiv:2501.13956 (abstract via arXiv API; §2.2/§3 via ar5iv HTML)
- GraphRAG README, docs (query/overview, local_search, global_search, drift_search, index/methods): https://github.com/microsoft/graphrag · https://microsoft.github.io/graphrag/ · paper arXiv:2404.16130 (abstract)
- LightRAG README + repo: https://github.com/HKUDS/LightRAG · paper arXiv:2410.05779 (abstract + §3.2 via ar5iv)
- cognee README + repo: https://github.com/topoteretes/cognee · paper arXiv:2505.24478 (abstract)
- Letta: https://github.com/letta-ai/letta (landing) · https://github.com/letta-ai/letta-code (README, repo metadata) · https://docs.letta.com/letta-code/memory (Memory & dreaming) · sleep-time compute arXiv:2504.13171 (abstract) · MemGPT paper arXiv:2310.08560 [K for V1 details]
- VikingMem: arXiv:2605.29640 (abstract via arXiv API) · implementation via sibling report `docs/research/2026-09-10-openviking.md`
