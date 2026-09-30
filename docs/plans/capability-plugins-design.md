# The capability plane: configurable, individually switchable memory / recall / distillation / knowledge pipelines

Status: **approved design for implementation** (t1, attempt 1).
Audience: t2–t9 implementers. This document is the contract; where it names a type,
a default, an error or a file, type that, not something similar.
Authority: every statement about *current* behaviour carries a `file:line` from this tree.
Nothing in this document is implemented yet.

Deliverable of this increment: a capability registry + `[capabilities]` config table, a
zero-token extractor crate, knowledge-base → knowledge-graph ingestion (off by default),
per-leg recall switches, and 4 new MCP tools + operator-skill sections — **with the default
configuration behaving exactly as today.**

---

## 1. The four laws (read these before any code)

| id | law | how it is proven |
| --- | --- | --- |
| **L1** | *Legacy equivalence.* With **no** `[capabilities]` table in `policy.toml`, every gate answers with today's behaviour, for every capability. | `crates/daemon/tests/capability_defaults.rs`: exhaustive over the registry, `gate(id, legacy) == legacy` for both `legacy` values; `plane.table_present() == false` on a fresh root. |
| **L2** | *Narrowing only.* A capability can only **suppress** work the legacy flags already asked for. It can never start work the legacy flags did not request. | One expression, `gate = legacy && (!table_present || enabled(id))` (§4.4). No call site may use any other combination. |
| **L3** | *No silent default.* An unknown capability **id**, an unknown option **key**, or an out-of-range **value** is a hard error naming the id/key/value. No gate, no option lookup and no config load may fall back to a default on a name it does not recognise. | `CapabilityError` (§4.3), raised in `CapabilityPlane::from_policy` (config load / `PUT`) — and in no other place, because runtime accessors take the closed `CapabilityId` enum and are therefore total. |
| **L4** | *Cost is opt-in.* Every `llm`-tier capability defaults **disabled**; every capability **new in this increment** defaults **disabled**; no new always-on background job is introduced. | Registry test asserting `tier == Llm ⇒ !default_enabled` and `new_in_this_increment ⇒ !default_enabled`. |

**A warning that must be repeated in three places (code comment, config comment, API doc):**
presence of the `[capabilities]` table is what activates the plane. The shipped
`DEFAULT_POLICY_TOML` (crates/daemon/src/config.rs:595-624) must therefore carry the
`[capabilities]` block **fully commented out**, exactly like `[distill]` is today
(config.rs:597-604). An uncommented empty `[capabilities]` header is a *present* table and
switches every llm-tier default to off.

---

## 2. Ground truth: what the tree actually does today

### 2.1 The six claims, verified

**1. "`crates/daemon/src/config.rs` has a `[distill]` table and a round-trip `DistillEditor`" — CONFIRMED, with a correction about *where* the table lives.**
`[distill]` is a section of **`policy.toml`**, not of `agents.toml`/`mcp.toml`. It is typed in
the policy crate (`DistillConfig`, crates/policy/src/lib.rs:166-187), hangs off
`PolicyConfig` (crates/policy/src/lib.rs:136-144), is parsed at boot by
`PolicyConfig::parse` (crates/daemon/src/config.rs:54-56) and is written back by
`DistillEditor` (crates/daemon/src/config.rs:775-815), whose discipline is
toml_edit + temp-file + `rename` + a mutex (config.rs:790-813) and whose round-trip
property is pinned by `distill_editor_round_trips_preserving_comments`
(config.rs:629-699). The `[capabilities]` table goes in the **same file**, edited by the
**same discipline** (§5).

**2. "`crates/daemon/src/distill.rs` has exactly one extraction path (ACP agent + `EXTRACTION_PROMPT`)" — CONFIRMED for automatic extraction, with two named exceptions.**
One path: `distill_once` renders the transcript (`render_transcript`, distill.rs:520-536),
composes prompt + service-language clause + `"TRANSCRIPT:\n"` (distill.rs:890-902), runs
**one ACP chat turn** (`ask_agent`, distill.rs:541-625; call at distill.rs:262-265), parses
one JSON object (`parse_extraction`, distill.rs:862-876) and writes memories then graph
(distill.rs:268-277). The prompt is `EXTRACTION_PROMPT` (distill.rs:16-36).
Exceptions, both outside the automatic path: (a) the wiki builder reuses
`Distiller::ask_agent` directly (crates/daemon/src/wiki.rs:2373) and never calls
`distill()` — so `graph: true` in that call site's `Distiller` literal
(crates/daemon/src/api.rs:945-956, the flag at 955) is inert, as its own comment says (950-952); (b) the graph has
two *manual* write entry points (`graph_create_entity` api.rs:1165-1189,
`graph_add_fact` api.rs:1190-1206) served by `POST /api/v1/graph/entity` and
`POST /api/v1/graph/fact` (api.rs:53-54).

**3. "`knowledge_ingest` writes the markdown file and chunks but never touches `crates/graph`" — CONFIRMED, and it is stronger than it looks.**
The handler is 15 lines (api.rs:4557-4571): `state.knowledge.save(&name, &content)` then a
`{"chunks", "file"}` JSON reply. `Knowledge::save` writes the `.md` atomically and reindexes
(crates/knowledge/src/files.rs:218-236); the same index step is the only thing the 60 s
scanner does (files.rs:269-311, loop at crates/daemon/src/lib.rs:230-248). The **only
production caller of `ruagent_graph::apply_extraction` in the whole tree is
crates/daemon/src/distill.rs:817** (verified by grep: `apply_extraction` appears in
distill.rs:817, in graph/src/lib.rs itself and in graph's own tests only). So no document,
and no knowledge chunk, has ever written an entity or a relation.
Nuance not to be confused with this: the wiki has *its own* graph, a link graph between
generated pages (`link_graph` wiki.rs:2787, `record_graph_reading` wiki.rs:3589, table
`wiki_graph_readings`). That is not `crates/graph` and must not be counted as KB→graph
ingestion.

**4. "`memembed.rs` + `api.rs recall` implement aggressive/conservative strategies with legs always on" — CONFIRMED in substance, three corrections.**
(a) The strategy does **not** live in `memembed`: `recall_memories`
(crates/daemon/src/memembed.rs:565-638) takes no strategy argument. The strategy is the
endpoint's cosine floor (`if conservative { 0.30 } else { 0.25 }`, api.rs:2792 + 2811), the
stub-vs-content rendering (api.rs:2967, 3106) and the `strategy` label (api.rs:3131-3134,
echoed at api.rs:3256). (b) Memory legs fuse with the **unweighted** `rrf`
(memembed.rs:581, `RRF_K = 60` at memembed.rs:187), while the knowledge legs fuse with a
**weighted** 2:1 RRF (`FUSION`, crates/knowledge/src/store.rs:174-178, applied in
`fuse`, store.rs:942-947). (c) Six legs are always queried: memory semantic
(memembed.rs:572 → `semantic_search`, memembed.rs:43), memory FTS (memembed.rs:573 →
`ruagent_memory::query::search_fts_scored`), knowledge ANN + FTS (both inside
`Knowledge::search_page`, store.rs:1052-1054 → `compute_legs`, store.rs:760-798, which
**unconditionally embeds the query** at store.rs:766), graph (`resolve_seeds` +
`retrieve`, api.rs:2889-2897) and wiki (the `wiki/` partition of the knowledge hits,
api.rs:2870-2875 → `wiki::recall_stubs`).

**5. "`crates/mcp/src/lib.rs` exposes 15 tools" — CONTRADICTED. There are 14.**
The `#[tool]` functions are at lib.rs:51, 74, 105, 138, 168, 191, 220, 250, 325, 348, 372,
402, 421, 442 — fourteen. This is not a guess: `crates/mcp/tests/roundtrip.rs:132-152`
asserts **set equality** between the live tool list and a 14-name `expected` vector, with
`assert_eq!(expected.len(), 14, "the declared consumption surface is 14 tools")`
(roundtrip.rs:148-152). The "15" is almost certainly the OpenViking research note's "MCP as
universal fallback with 15 well-chosen tools"
(docs/research/2026-09-10-openviking.md:238). **The correct baseline is 14**, and after t6
it is **18** (§14).

**6. "`crates/daemon/src/skills.rs` manages SKILL.md distribution" — CONFIRMED, and it is thinner than the name suggests.**
`harness_skill_dirs` maps a harness to its skills directory (skills.rs:20-27),
`parse_skill_md` reads `name`/`description` out of YAML frontmatter (skills.rs:30-52),
`discover` merges platform-then-project with project winning on name collision
(skills.rs:56-87), `install_skill` skips on identical `SKILL.md` and otherwise
**wipes and re-copies** the directory (skills.rs:91-109), `sync` fans that out over the
harnesses (skills.rs:128-144). There is no registry, no version, no enable/disable and no
lifecycle: skills are files that get copied. The bundled operator skill
(`skills/ruagent-operator/SKILL.md`) is installed into `<root>/skills` at boot
(crates/daemon/src/lib.rs:138-147) and exposed over `GET /api/v1/skills` +
`POST /api/v1/skills/sync` (api.rs:47-48, handlers api.rs:1513-1540).

### 2.2 The gaps this increment closes (each with its evidence)

| # | gap | evidence (current tree) |
| --- | --- | --- |
| G1 | **No capability/plugin mechanism exists at all.** No registry, no enable/disable, no `[capabilities]`, no plugin crate. | `grep -i capabilit` over the tree returns 25 hits, all of them prose comments, docs, `AgentCapabilities` in the mock agent (crates/mock-agent/src/main.rs:43) or the *agent card* comment (crates/daemon/src/config.rs:530). `grep -i plugin` returns 12 hits, all unrelated (test comments, an example `[agent.plugin-dev]` at config.rs:903). `crates/` holds exactly the 11 crates of Cargo.toml:3's `crates/*` glob; there is no `crates/extract`. |
| G2 | **Knowledge ingest never touches the graph.** | api.rs:4557-4571 (handler), and `apply_extraction`'s only production caller is distill.rs:817. |
| G3 | **Auto-distill gating is a single boolean with no per-capability control.** | `auto_distiller` returns `None` when `policy.auto` is false (chat.rs:1479-1493, the check at 1481); the spawn is `maybe_auto_distill` (chat.rs:1052-1070) from `close` (chat.rs:997) and the idle reaper (chat.rs:1515-1530, `IDLE_TIMEOUT = 60 min` at chat.rs:438); the background half is `auto_distill_now` (chat.rs:397-427). |
| G4 | **Memory digest injection is unconditional.** | The first prompt always builds the context (chat.rs:161-237; the once-only flag `memory_injected` at chat.rs:141, swapped at 169-171), through `ChatManager::injection_context` (chat.rs:521-629), whose memory block comes from `memembed::select_injection_memories` (chat.rs:540-550), knowledge from `k.search_page` (chat.rs:562-602) and graph evidence from `ruagent_graph::retrieve` (chat.rs:604-616). The runs path is the same contract with no gate (`render_run_injection`, runs.rs:1856-1930). Note: no symbol named `digest` exists anywhere in the tree (`grep -i digest` = 0 hits) — "memory digest injection" is the requester's name for *this* block. |
| G5 | **Recall legs are always on and are not individually addressable.** | §2.1 claim 4: six always-on legs; `recall_memories` fuses unweighted (memembed.rs:581) and `compute_legs` embeds the query unconditionally (store.rs:766). |
| G6 | **There is no zero-token extractor.** Every extraction is an ACP chat turn (distill.rs:262) that costs model tokens; the only deterministic machinery in the tree is the graph's *resolution/normalisation* layer (`variants` graph/src/lib.rs:909-926, `acronym` 930-944, `judge_against` 994-1024, `normalize_fact`/`fact_hash` 539-562), which consumes candidates, never produces them. |
| G7 | **The router's deps understate the real order.** t2…t7 each depend on *this* document only, yet t4/t5 need t2's registry and t3's crate, and t2/t4/t5 all edit `crates/daemon/src/api.rs`. §17 lays the real order out; a task that starts early must stop and report instead of inventing a private interface. |

---

## 3. Non-goals of this increment (do not widen into these)

1. **No default behaviour change.** No new memory is written, no new token is spent, no
   recall result moves, and `DEFAULT_POLICY_TOML`/`DEFAULT_AGENTS_TOML`/`DEFAULT_MCP_TOML`
   (config.rs:528-624) gain no active table. The `[capabilities]` block ships commented out.
2. **No new always-on background job.** KB→graph ingestion rides the *existing* knowledge
   scan loop (lib.rs:230-248); it does not spawn a task of its own. The only new loop-like
   thing permitted is `tokio::spawn` inside an already-existing loop.
3. **No per-leg switches on the injection paths.** The six recall toggles govern
   `/api/v1/recall` (and therefore MCP `memory_recall`) only. `ChatManager::injection_context`
   (chat.rs:521) and `render_run_injection` (runs.rs:1856) call the knowledge search with
   `LegConfig::default()`; they are governed by the on/off capabilities `memory_inject_chat`
   and `memory_inject_runs`. Wiring per-leg config into injection is a *named follow-up*, not
   part of this increment.
4. **No injection-budget capability.** `InjectionBudget`, `CHAT_SELECTION`,
   `KNOWLEDGE_SOURCES`/`WIKI_PAGES`/`GRAPH_PATHS` (crates/memory/src/inject.rs:8, 244, 339-349)
   are untouched.
5. **No LLM-tier KB→graph ingestion.** `knowledge_ingest_graph` is zero-token by
   construction (§13). An LLM variant is deliberately not registered, so the cost story has
   no hole: nothing in this increment spends a token on document ingestion.
6. **`[distill].graph` is unchanged.** It keeps its meaning (memories only vs memories +
   graph for a *session* distillation, distill.rs:129/175, `unwrap_or(true)` at
   api.rs:2720). `knowledge_ingest_graph` is about a *different producer* (the knowledge
   base). Do not merge the two flags.
7. **Manual distillation is not gated.** `POST /api/v1/sessions/{key}/distill`
   (api.rs:178, handler 2725-2750) is an explicit human/agent instruction and keeps
   working in the default configuration, exactly as today. `distill_session` gates the
   *unattended* path only (§6, §10.3). Recorded here because it is the one place where
   "disable every llm surface" would otherwise have changed a default behaviour.
8. **No panel redesign.** t7 adds one settings card. Navigation, the Memory/Knowledge
   views, and `panel/e2e/**` are out of scope (no new e2e spec; the existing suite must stay
   green).
9. **No schema change to `recall_log`, `distill_log`, `memories` or `entities`.** One new
   table (the ingest ledger, §13.3) is the only schema addition.
10. **No runtime per-call leg overrides.** Configuration is the only switch: a request
    cannot turn a leg on that the configuration turned off (`L3`). `?strategy=` keeps its
    present meaning (api.rs:2792).
11. **No `async-trait`, no new daemon dependency for dispatch.**
    `grep async-trait` over every `Cargo.toml` = 0 hits; dispatch is an enum (§10.1), like
    `HarnessKind`/`FusionKind`/`IndexOutcome`.
12. **Do not reformat files you did not touch** (AGENTS.md), and run `cargo fmt --all` only
    on your own change set.

---

## 4. The capability registry contract

### 4.1 Where it lives

New module `crates/daemon/src/capability.rs` (registered in `crates/daemon/src/lib.rs`'s
module list at lib.rs:6-17, alphabetically first: `pub mod capability;`).

Why the daemon and not `ruagent-policy`: the registry names *daemon pipelines* (with call
sites), and the MCP bridge is HTTP-only (crates/mcp/src/lib.rs:1-7 — a thin stdio→HTTP
bridge), so no other crate needs crate-level access. `ruagent-policy` stays a leaf holding
only the *file shape* (§5.1).

### 4.2 The types (frozen — copy these signatures)

```rust
// crates/daemon/src/capability.rs

/// Token cost class. `Free` = zero tokens, deterministic, no model call.
/// `Llm` = consumes model tokens (an ACP chat turn at distill.rs:262).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier { Free, Llm }

impl Tier { pub fn as_str(self) -> &'static str { match self { Tier::Free => "free", Tier::Llm => "llm" } } }

/// The CLOSED set of capability ids. Strings appear only at the two boundaries
/// (the config file and the HTTP API); every call site uses a variant, so a
/// typo is a compile error rather than a silent default (L3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CapabilityId {
    MemoryInjectChat,
    MemoryInjectRuns,
    RecallLegMemorySemantic,
    RecallLegMemoryFts,
    RecallLegKnowledgeSemantic,
    RecallLegKnowledgeFts,
    RecallLegWiki,
    RecallLegGraph,
    SessionExtractRules,
    KnowledgeIngestGraph,
    DistillSession,
}

impl CapabilityId {
    pub const ALL: &'static [CapabilityId] = &[ /* the 11 above, in the order written */ ];
    /// The stable config/API id. NEVER rename one: it is a user-facing key in
    /// policy.toml and in `GET /api/v1/capabilities`.
    pub fn as_str(self) -> &'static str {
        match self {
            CapabilityId::MemoryInjectChat => "memory_inject_chat",
            CapabilityId::MemoryInjectRuns => "memory_inject_runs",
            CapabilityId::RecallLegMemorySemantic => "recall_leg_memory_semantic",
            CapabilityId::RecallLegMemoryFts => "recall_leg_memory_fts",
            CapabilityId::RecallLegKnowledgeSemantic => "recall_leg_knowledge_semantic",
            CapabilityId::RecallLegKnowledgeFts => "recall_leg_knowledge_fts",
            CapabilityId::RecallLegWiki => "recall_leg_wiki",
            CapabilityId::RecallLegGraph => "recall_leg_graph",
            CapabilityId::SessionExtractRules => "session_extract_rules",
            CapabilityId::KnowledgeIngestGraph => "knowledge_ingest_graph",
            CapabilityId::DistillSession => "distill_session",
        }
    }
}

/// The option keys a capability may declare. A key present in the file for a
/// capability that does not declare it is a HARD ERROR naming id + key (L3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptionKey { Weight, MinScore, MaxPerInput, MinConfidence, MaxDocsPerPass }

impl OptionKey {
    pub fn as_str(self) -> &'static str {
        match self {
            OptionKey::Weight => "weight",
            OptionKey::MinScore => "min_score",
            OptionKey::MaxPerInput => "max_per_input",
            OptionKey::MinConfidence => "min_confidence",
            OptionKey::MaxDocsPerPass => "max_docs_per_pass",
        }
    }
}

/// One capability's declared options, RESOLVED (every field is what the
/// pipeline must use; `None` means "the pipeline's own argument decides").
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct CapabilityOptions {
    pub weight: Option<f64>,
    pub min_score: Option<f64>,
    pub max_per_input: Option<u32>,
    pub min_confidence: Option<f64>,
    pub max_docs_per_pass: Option<u32>,
}

/// The registry row. `description` is USER-FACING and is returned verbatim by
/// the API; `gates` is a stable human phrase, never a line number.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CapabilitySpec {
    pub id: CapabilityId,
    pub description: &'static str,
    pub tier: Tier,
    /// The registry default, i.e. what `enabled(id)` answers when the file
    /// carries no `enabled` key for this id. Chosen so that L1 holds.
    pub default_enabled: bool,
    /// false = this capability names behaviour that exists today (so its
    /// default MUST be true); true = new in this increment (default MUST be
    /// false). Asserted for every row by the registry test (§18).
    pub new_in_this_increment: bool,
    /// The option keys this capability accepts — and no others.
    pub options: &'static [OptionKey],
    pub defaults: CapabilityOptions,
    pub gates: &'static str,
}

/// The registry: 11 rows, one per CapabilityId::ALL entry, same order.
pub fn specs() -> &'static [CapabilitySpec];
pub fn spec(id: CapabilityId) -> &'static CapabilitySpec;
```

### 4.3 Errors (hard, naming the thing)

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum CapabilityError {
    /// `unknown capability `foo` in [capabilities]: known ids are a, b, c, …`
    UnknownId(String),
    /// `capability `distill_session` does not accept the option `weight` (it accepts: none)`
    UnknownKey { id: String, key: String, accepted: String },
    /// `capability `session_extract_rules`: `max_per_input = 0` is out of range (1..=10000)`
    BadValue { id: String, key: String, value: String, expectation: &'static str },
}
impl std::fmt::Display for CapabilityError { /* exactly the three sentences above */ }
```

`known ids` is `CapabilityId::ALL.map(as_str).join(", ")` — the message must list them, so a
typo is fixable from the error alone (the same discipline as `harness_of`'s
`unknown harness `{other}`` at crates/daemon/src/config.rs:150).

**Range table** (validated in `from_policy`, `BadValue` on violation):

| key | range |
| --- | --- |
| `weight` | finite and `0.0..=100.0` |
| `min_score` | finite and `0.0..=1.0` |
| `max_per_input` | `1..=10_000` |
| `min_confidence` | finite and `0.0..=1.0` |
| `max_docs_per_pass` | `1..=1000` |

### 4.4 The plane

```rust
/// The live plane. Cheap to clone (a handful of entries) and shared by every
/// consumer through one `Arc<RwLock<..>>` (see §4.5).
#[derive(Debug, Clone, PartialEq)]
pub struct CapabilityPlane {
    /// `None` = the `[capabilities]` table is ABSENT = legacy mode (L1).
    /// `Some(map)` = present; `map.is_empty()` is legal and means "every
    /// capability at its registry default" (which, per L4, switches every
    /// llm-tier capability OFF).
    table: Option<std::collections::BTreeMap<String, ruagent_policy::CapabilityFile>>,
}

impl CapabilityPlane {
    /// Legacy mode: gates answer with today's behaviour. This is the value
    /// `ChatManager::new` starts from, so no test harness has to opt in.
    pub fn legacy() -> Self { Self { table: None } }

    /// Build the plane from the parsed policy, validating ids, keys and ranges.
    /// This is the ONLY fallible constructor.
    pub fn from_policy(p: &ruagent_policy::PolicyConfig) -> Result<Self, CapabilityError>;

    /// true when the `[capabilities]` table exists (an EMPTY table counts).
    pub fn table_present(&self) -> bool;

    /// The configured/registry-default enable state. TOTAL: the id is an enum.
    pub fn enabled(&self, id: CapabilityId) -> bool;

    /// THE GATE (L2). `legacy` = what today's code would do.
    /// `gate = legacy && (!self.table_present() || self.enabled(id))`
    pub fn gate(&self, id: CapabilityId, legacy: bool) -> bool;

    /// The resolved options for one capability (defaults merged with the file).
    pub fn options(&self, id: CapabilityId) -> CapabilityOptions;

    /// API/panel rows, one per registry row, in registry order.
    pub fn rows(&self) -> Vec<CapabilityRow>;
}

/// One row of `GET /api/v1/capabilities`.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct CapabilityRow {
    pub id: &'static str,
    pub tier: &'static str,          // "free" | "llm"
    pub description: &'static str,
    pub gates: &'static str,
    pub default_enabled: bool,
    pub enabled: bool,
    /// "legacy" (table absent) | "default" (table present, no key) | "file"
    pub configured: &'static str,
    pub new: bool,
    pub options: CapabilityOptions,
}
```

Two deliberate properties:

* **`gate` is total.** There is no `Result` to ignore and therefore no way for a call site
  to invent a fallback. A string id cannot reach it: the only door from string to
  `CapabilityId` is `from_policy` (config) and the two HTTP handlers (API), both of which
  return the hard error.
* **`gate` can only narrow.** `legacy == false` ⇒ `gate == false`, always. That is the
  formal content of L2 and it is what makes "the default configuration is unchanged"
  provable by exhaustion over 11 ids × 2 legacy values.

### 4.5 Ownership and the live swap (no `AppState` change)

`AppState` is constructed in **12 places across 12 files** (src: lib.rs:285, api.rs:4929;
tests: daemon/tests/smoke.rs:78, daemon/tests/knowledge_api.rs:63,
daemon/tests/injection_e2e.rs:228, mcp/tests/roundtrip.rs:90,
mock-agent/tests/wiki_pipeline.rs:110, mock-agent/tests/registry_api.rs:55,
mock-agent/tests/mcp_health.rs:90, mock-agent/tests/judge.rs:84,
mock-agent/tests/chat_experience.rs:66, mock-agent/tests/e2e_daemon.rs:80).
Adding a required field would edit twelve files before anything compiles, and would make ten
test files shared by every task. **Rejected.**

Instead the plane rides the existing handle pattern of `distill_policy` — the API already
reads that policy out of the chat manager (`state.chats.distill_policy_now()`, api.rs:2643)
and the boot installs it (`lib.rs:254-271`):

```rust
// crates/daemon/src/chat.rs
pub struct ChatManager {
    /* … existing fields … */
    /// The capability plane, shared with RunManager by ONE handle created at
    /// boot (lib.rs), the way the knowledge handle is shared (lib.rs:217-223,
    /// 280-283). Swapped at runtime by `PUT /api/v1/capabilities`; the swap is
    /// a write THROUGH the Arc, so both managers see it at once.
    capabilities: std::sync::Arc<std::sync::RwLock<capability::CapabilityPlane>>,
}

impl ChatManager {
    /// Clone of the live plane. Read it, drop the lock, never hold a guard
    /// across an `.await` (`std::sync::RwLock` — same discipline as
    /// `distill_policy`, read-and-cloned at chat.rs:1058-1062 and 1480).
    pub fn capabilities(&self) -> capability::CapabilityPlane;
    /// Install/swap the plane (write through the shared Arc).
    pub fn set_capabilities(&self, plane: capability::CapabilityPlane);
}
```

* `ChatManager::new` (chat.rs:445) gains **no parameter**: the field is initialised to
  `Arc::new(RwLock::new(CapabilityPlane::legacy()))`, exactly like `drop_asks`
  (`Mutex::new(None)`, chat.rs:345) and `knowledge` (`Mutex::new(None)`, chat.rs:349). Every
  existing `ChatManager::new` call site (15 of them: lib.rs:254, api.rs:4913,
  chat.rs:1679/1804/1950/2046, and the 9 test harnesses) therefore keeps compiling
  and keeps today's behaviour — the constructor default *is* law L1.
* `RunManager` gets the same handle through a `set_capabilities` setter mirroring
  `set_knowledge` (runs.rs:299-309) — that setter pattern exists precisely because widening
  `new()` would edit five files (see the comment at chat.rs:969-976).
* `DaemonConfig` gains one field, `pub capabilities: CapabilityPlane`, filled in
  `DaemonConfig::load` (config.rs:67-73) from `policy.capabilities`; a load error bails the
  boot with the `UnknownId`/`UnknownKey`/`BadValue` message (the file's only construction
  site is config.rs:67).

`PUT` writes the file **and** swaps the live value, in that order, like
`distill_policy_put` does (api.rs:2712-2721).

---

## 5. The config schema

### 5.1 File shape (`ruagent-policy`)

```rust
// crates/policy/src/lib.rs

/// `[capabilities.<id>]` — the typed option set of ONE capability.
/// An unknown KEY inside the table is refused by serde (`deny_unknown_fields`),
/// which names both the key and the table; an unknown ID (the table name) is
/// refused by the registry, which names the id and lists the known ones.
#[derive(Debug, Clone, Default, PartialEq, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CapabilityFile {
    pub enabled: Option<bool>,
    pub weight: Option<f64>,
    pub min_score: Option<f64>,
    pub max_per_input: Option<u32>,
    pub min_confidence: Option<f64>,
    pub max_docs_per_pass: Option<u32>,
}

// in PolicyConfig (crates/policy/src/lib.rs:136-144), next to `distill`:
    /// `[capabilities]`: `None` = the table is ABSENT (legacy mode, L1);
    /// `Some({})` = present and empty (every registry default applies).
    /// The Option is the mechanism that keeps "absent" distinguishable from
    /// "present but empty" — a `BTreeMap` with `#[serde(default)]` cannot.
    #[serde(default)]
    pub capabilities: Option<std::collections::BTreeMap<String, CapabilityFile>>,
```

### 5.2 What it looks like in `policy.toml`

```toml
# Zero-token deterministic extraction of a closed session into memory
# candidates. OFF by default: enabling it adds memories that were not there
# before.
[capabilities.session_extract_rules]
enabled = true
max_per_input = 32
min_confidence = 0.0

# Knowledge base -> knowledge graph ingestion. OFF by default: it costs no
# tokens but it does write entities and relations.
[capabilities.knowledge_ingest_graph]
enabled = false
max_per_input = 96
max_docs_per_pass = 20

# Per-leg recall switches. All ON by default (= today's behaviour).
[capabilities.recall_leg_graph]
enabled = false
[capabilities.recall_leg_memory_semantic]
weight = 1.0
min_score = 0.25

# Unattended ACP distillation when a session closes. LLM tier: OFF by default.
# Manual distillation (POST /api/v1/sessions/{key}/distill) is NOT gated.
[capabilities.distill_session]
enabled = true
```

The **shipped** `DEFAULT_POLICY_TOML` (config.rs:595-624) adds this block, **entirely
commented**, right after the existing `[distill]` comment block:

```toml
# Capability plane (docs/plans/capability-plugins-design.md).
# ABSENT [capabilities] table = today's behaviour, every gate passes through.
# The moment the table EXISTS, the registry defaults apply: llm-tier
# capabilities default OFF (they cost model tokens), free-tier new ones too.
# Unknown capability ids and unknown option keys are startup errors.
#
# [capabilities.session_extract_rules]
# enabled = true                  # zero-token transcript -> memory candidates
# max_per_input = 32
#
# [capabilities.knowledge_ingest_graph]
# enabled = true                  # knowledge base -> knowledge graph, zero tokens
# max_per_input = 96              # candidates per document
# max_docs_per_pass = 20          # documents per sweep (rides the 60s scan)
#
# [capabilities.distill_session]
# enabled = true                  # unattended ACP extraction on session close
#
# [capabilities.recall_leg_wiki]
# enabled = false                 # switch one recall leg off
# [capabilities.recall_leg_memory_semantic]
# weight = 1.0                    # today's value; raise to prefer the leg
```

### 5.3 The round-trip editor (exactly the `DistillEditor` discipline)

```rust
// crates/daemon/src/config.rs, right below DistillEditor (config.rs:775-815)

/// Round-trip editor for the `[capabilities]` table of `policy.toml`.
/// Same discipline as `DistillEditor`: toml_edit keeps comments, key order and
/// every other section; temp file + rename is atomic; the mutex serialises
/// writers (config.rs:790-813).
pub struct CapabilitiesEditor { path: PathBuf, lock: Mutex<()> }

impl CapabilitiesEditor {
    pub fn new(path: impl Into<PathBuf>) -> Self;
    /// REPLACE the whole table with `entries`.
    /// * an entry whose table is empty writes `[capabilities.<id>]` with no keys
    ///   (legal: every declared default applies);
    /// * every key set to `None` is REMOVED from that table, not written empty
    ///   (the `set_or_remove` rule, config.rs:817-839);
    /// * an EMPTY map REMOVES the `[capabilities]` table entirely -> legacy mode.
    pub fn update(&self, entries: &BTreeMap<String, ruagent_policy::CapabilityFile>) -> Result<()>;
}
```

`update` reuses `set_or_remove` / `set_or_remove_bool` (config.rs:817-839). Like
`DistillEditor::update`, it must **not** validate ids — validation belongs to
`CapabilityPlane::from_policy`, called by the HTTP handler *before* the write
(§16), so a refused update leaves the file untouched and the live plane unchanged.

As landed (§21 item 5): the handler validates by **reusing the boot door** — it builds
`ruagent_policy::PolicyConfig { capabilities: table, ..Default::default() }` and calls
`CapabilityPlane::from_policy` (`capability.rs:759-763`). There is therefore exactly one
validation path in the tree, and a body that would fail the next boot fails this request
instead. The editor itself is `CapabilitiesEditor` (`config.rs:1058`, `update` at 1063-1114,
an empty map removing the table at 1080-1088) and its round-trip is pinned by
`capabilities_editor_round_trips_preserving_comments` (`config.rs:751-870`). The module is
`crates/daemon/src/capability.rs` — **singular**; `crates/daemon/tests/capabilities.rs` is
the (plural) integration test file, and the two are different files (§21 item 4).

Required test (mirrors `distill_editor_round_trips_preserving_comments`,
config.rs:629-699): write a `policy.toml` with a comment, `[permissions]`, `[distill]` and
an existing `[capabilities.recall_leg_wiki]`; update with a different set; assert the
comment, the other sections and `[distill]` survive, that a freed key disappeared, and that
the result re-parses through `PolicyConfig::parse` with the same plane. Then update with an
**empty** map and assert the `[capabilities]` table is gone and `table_present() == false`.

### 5.4 Absent table ⇒ today, capability by capability

| capability id | tier | registry default | legacy input (today's switch) | with the table absent | with the table present but no key for it |
| --- | --- | --- | --- | --- | --- |
| `memory_inject_chat` | free | `true` | none — always on (chat.rs:169-203) | on (today) | on |
| `memory_inject_runs` | free | `true` | none — always on (runs.rs:1856) | on (today) | on |
| `recall_leg_memory_semantic` | free | `true` | none — always on (memembed.rs:572) | on (today) | on |
| `recall_leg_memory_fts` | free | `true` | none — always on (memembed.rs:573) | on (today) | on |
| `recall_leg_knowledge_semantic` | free | `true` | none — always on (store.rs:766) | on (today) | on |
| `recall_leg_knowledge_fts` | free | `true` | none — always on (store.rs:796) | on (today) | on |
| `recall_leg_wiki` | free | `true` | none — always on (api.rs:2870-2875) | on (today) | on |
| `recall_leg_graph` | free | `true` | none — always on (api.rs:2889-2897) | on (today) | on |
| `session_extract_rules` | free | `false` | **new** — no legacy behaviour | off (nothing runs; today nothing ran) | off |
| `knowledge_ingest_graph` | free | `false` | **new** — no legacy behaviour | off (today the graph is never touched by ingest) | off |
| `distill_session` | llm | `false` | `[distill].auto`, default false (policy/src/lib.rs:169-173) | gate = `auto` exactly (today) | off ⇒ **narrowed**; the boot WARN + `conflicts` row below fire |

Two consequences to state out loud, because a reviewer will look for them:

* **The default configuration is unchanged.** Nine capabilities name behaviour that is
  unconditional today (`default_enabled = true`, and with the table absent the gate is not
  even consulted). The two new free capabilities default off, which is exactly today
  (nothing new runs). The llm capability defaults off and today's `[distill].auto` is off, so
  no token is spent in the default configuration either way.
* **A non-default configuration can lose unattended distillation.** A user who set
  `[distill] auto = true` and has no `[capabilities]` table keeps it (legacy mode). A user who
  set `auto = true` **and** writes any `[capabilities]` line without a `distill_session`
  entry gets it *disabled* — the fail-safe direction (cost), never the spend direction. That
  must be loud: `DaemonConfig::load` emits exactly one `tracing::warn!` naming both keys,

  ```text
  WARN capability `distill_session` is off while [distill].auto = true: unattended
       distillation will not run. Add [capabilities.distill_session] enabled = true to
       restore it (docs/plans/capability-plugins-design.md §5.4).
  ```

  and `GET /api/v1/capabilities` returns it in `conflicts[]` (§16.2). The conflict list is
  produced by `CapabilityPlane::conflicts(&self, distill_auto: bool) -> Vec<CapabilityConflict>`
  — one entry per (capability, legacy flag) pair whose legacy flag is currently `true` while
  the capability resolves to `false`. With one legacy pair today (`distill_session`,
  `[distill].auto`), the function is a two-line function and not a mechanism.

---

## 6. The inventory (what each capability gates, and its call site)

| id | tier | default | options declared | what it gates (today's code) |
| --- | --- | --- | --- | --- |
| `memory_inject_chat` | free | on | — | The first-prompt context of a chat: `chat.rs:169-203` → `ChatManager::injection_context` (chat.rs:521-629). Off ⇒ `injection_context` returns `None`, so the prompt carries the role/handoff blocks only and no memory block. |
| `memory_inject_runs` | free | on | — | The run prompt's injected block: `render_run_injection` (runs.rs:1856-1930, called at runs.rs:771). Off ⇒ the injected string is empty. |
| `recall_leg_memory_semantic` | free | on | `weight`, `min_score` | The cosine leg of `recall_memories` (memembed.rs:572). `min_score` default `None` ⇒ today's per-strategy floor 0.25/0.30 (api.rs:2811). |
| `recall_leg_memory_fts` | free | on | `weight` | The bm25 leg (memembed.rs:573). |
| `recall_leg_knowledge_semantic` | free | on | `weight` (default 2.0) | The LanceDB ANN leg + the query embedding (store.rs:766-791), fused at `w_semantic` (store.rs:174-178). |
| `recall_leg_knowledge_fts` | free | on | `weight` (default 1.0) | The three-stage keyword leg (store.rs:796 → `keyword_leg`, store.rs:825-865). |
| `recall_leg_wiki` | free | on | — | The `wiki/` partition of the knowledge hits and the stubs (api.rs:2870-2875 → `wiki::recall_stubs`). Off ⇒ `"wiki": []`, and a `wiki/…` document never appears in `knowledge` either (today it is partitioned out at api.rs:2870-2874). |
| `recall_leg_graph` | free | on | — | `resolve_seeds` + `retrieve` + the entity-facts pass (api.rs:2889-2897, 2942). Off ⇒ `graph.entities == 0`, `graph.paths == 0`, `entities: []`, and neither call is made. |
| `session_extract_rules` | free | **off** | `max_per_input` (32), `min_confidence` (0.0) | **NEW**: the deterministic transcript → memory extractor of §8, on the unattended path only. |
| `knowledge_ingest_graph` | free | **off** | `max_per_input` (96), `max_docs_per_pass` (20) | **NEW**: knowledge base → `crates/graph` ingestion of §13. |
| `distill_session` | llm | **off** | — | The unattended ACP extraction: `auto_distiller` (chat.rs:1479-1493, the `!policy.auto` check at 1481) and the spawn in `maybe_auto_distill` (chat.rs:1052-1070). Effective condition: `[distill].auto AND gate(DistillSession, true)`. Manual distillation is not gated (§3.7). |

`description` strings (user-facing, returned verbatim by the API; keep them one sentence and
honest about cost):

| id | `description` |
| --- | --- |
| `memory_inject_chat` | Inject long-term memory, knowledge and graph evidence into a chat's first prompt. |
| `memory_inject_runs` | Inject long-term memory, knowledge and graph evidence into a run's prompt. |
| `recall_leg_memory_semantic` | Recall leg: cosine similarity over memory embeddings. |
| `recall_leg_memory_fts` | Recall leg: bm25 keyword match over memory text. |
| `recall_leg_knowledge_semantic` | Recall leg: vector search over knowledge chunks. |
| `recall_leg_knowledge_fts` | Recall leg: bm25 keyword match over knowledge chunks. |
| `recall_leg_wiki` | Recall leg: agent-generated wiki pages (leads, not ground truth). |
| `recall_leg_graph` | Recall leg: entity seeds and multi-hop relation paths. |
| `session_extract_rules` | Zero-token deterministic extraction of a closed session into memory candidates. |
| `knowledge_ingest_graph` | Zero-token deterministic ingestion of knowledge documents into the entity graph. |
| `distill_session` | ACP-agent distillation when a session closes (spends model tokens). Manual distillation is an explicit request and is never gated. |

---

## 7. `crates/extract`: the pure extraction crate

### 7.1 Layout (t3 owns every file in this list)

```
crates/extract/Cargo.toml        # NEW: name = "ruagent-extract"
crates/extract/src/lib.rs        # NEW: pub types + `pub mod {text, rules, memory, graph};`
crates/extract/src/text.rs       # NEW: sentence splitting, tokenising, normalising
crates/extract/src/rules.rs      # NEW: every marker table, every rule id, every cap
crates/extract/src/memory.rs     # NEW: `memory_candidates`
crates/extract/src/graph.rs      # NEW: `graph_candidates`
crates/extract/tests/memory-gold.rs   # NEW
crates/extract/tests/graph-gold.rs    # NEW
crates/extract/tests/bounded.rs       # NEW: caps, determinism, purity
crates/extract/tests/gold/memory.json # NEW: transcript -> expected candidates
crates/extract/tests/gold/graph.json  # NEW: document text -> expected candidates
Cargo.toml                       # root: one line in [workspace.dependencies]
```

**No `Cargo.toml` member edit is needed**: `members = ["cli", "crates/*"]` (Cargo.toml:3)
picks the directory up. The only root edit is
`ruagent-extract = { path = "crates/extract" }` in `[workspace.dependencies]`
(Cargo.toml:12-24). Consumers add `ruagent-extract.workspace = true` to their own
`[dependencies]` (t4: crates/daemon/Cargo.toml:14-41).

### 7.2 Purity contract (verifiable, not aspirational)

* `crates/extract/Cargo.toml` has **no `[dependencies]` section at all**. Only
  `[dev-dependencies] serde_json = "1"` (for the gold fixtures).
* The crate imports nothing internal — no `ruagent-*`, no `rusqlite`, no `tokio`; and
  nothing from `std::fs`, `std::net`, `std::env`, `std::process`, `std::time`, `std::thread`.
  `crates/extract/tests/bounded.rs` asserts this mechanically by scanning the crate's own
  sources for those paths (a `include_str!` sweep over the five source files is enough and
  needs no dependency).
* Both entry points are **pure functions of their arguments**: no clock, no RNG, no env, no
  I/O, no interior mutability. Same input ⇒ byte-identical output, which the `bounded.rs`
  test proves by running each function twice and comparing.
* Nothing in the crate knows what a *capability* is: limits arrive as an argument
  (crates/extract never reads config), and candidates leave as plain structs. Applying them
  to SQLite lives in the daemon (`crates/daemon/src/extract_plane.rs`, §10) and in
  `ruagent_graph` (`apply_extraction`, graph/src/lib.rs:1427).
* Tokenisation is deliberately local (`src/text.rs`) rather than a dependency on
  `ruagent_store::fts` (crates/store/src/fts.rs), because depending on the store crate would
  drag rusqlite — i.e. I/O — into a crate whose defining property is that it has none. The
  duplication is bounded to one module and is honest: the extractor needs (a) sentence
  boundaries, (b) literal marker matching, (c) identifier/han-run tokens — not an FTS query
  builder. `src/text.rs`'s header must say this in those words.

### 7.3 The types

```rust
// crates/extract/src/lib.rs

/// One transcript turn. The daemon maps `sessions::SessionMessage`
/// (crates/daemon/src/sessions.rs:36-39: `role: String`, `text: String`, `ts: i64`)
/// onto this; `role` is normalised to "user" or "assistant" there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Turn { pub role: Role, pub text: String, pub ts_ms: i64 }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role { User, Assistant }

/// Memory-candidate confidence as EVIDENCE, not as a number: the number is
/// chosen in ONE place already (crates/memory/src/confidence.rs, `confidence()`
/// at line 100 with the four named signals at lines 26-38), and the free
/// extractor must speak the same vocabulary the LLM prompt asks the model for
/// (distill.rs:19-22: correction / confirmation / hedge markers).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CandidateConfidence {
    /// The user explicitly confirmed it  -> `ConfidenceSignals::confirmed()`  (1.0)
    Confirmed,
    /// The user corrected the agent      -> `ConfidenceSignals::corrected()`  (1.0)
    Corrected,
    /// The speaker hedged                -> `ConfidenceSignals::hedged()`     (0.4)
    Hedged,
    /// Nothing was said about it         -> `ConfidenceSignals::unconfirmed()`(0.8)
    Unconfirmed,
    /// The LLM tier's own number, used verbatim (the escape hatch that already
    /// exists: `ConfidenceSignals::explicit`, confidence.rs:86-92).
    Explicit(f64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CandidateStore { Profile, Observation, Procedure, Lesson }

/// Where a candidate came from, for the applier's audit and for the tests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CandidateOrigin {
    /// The turn index in the input slice.
    Turn { index: usize, role: Role },
    /// A document-level candidate; `section` is the 0-based section index when
    /// the caller passed sectioned text, else `None`.
    Document { section: Option<usize> },
}

#[derive(Debug, Clone, PartialEq)]
pub struct MemoryCandidate {
    pub store: CandidateStore,
    /// A canonical namespace string: "user" | "global" | "project:<name>" |
    /// "agent:<name>". Validated by the applier through
    /// `ruagent_memory::namespace::Namespace::parse` (used at distill.rs:683).
    pub namespace: String,
    pub content: String,
    pub confidence: CandidateConfidence,
    /// The rule that produced it, e.g. "user_preference". Stable; used in the
    /// dedup key and named in tests.
    pub rule: &'static str,
    pub origin: CandidateOrigin,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EntityCandidate {
    pub name: String,
    pub kind: Option<&'static str>,   // "person"|"project"|"tool"|"org"|"product"|"concept"
    pub summary: Option<String>,
    pub aliases: Vec<String>,
    /// Ranking score in (0,1]; see §9.3. Used ONLY to cut the candidate set —
    /// the graph has no confidence column (`ExtractEntity`, graph/src/lib.rs:1375-1380).
    pub score: f32,
    pub rule: &'static str,
    pub origin: CandidateOrigin,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RelationCandidate {
    pub src: String,
    pub dst: String,
    /// A snake_case literal from the closed table (`RELATION_PATTERNS`).
    pub relation: &'static str,
    pub fact: String,
    pub score: f32,
    pub rule: &'static str,
    pub origin: CandidateOrigin,
}

/// Input bounds. The daemon builds this from capability options; the crate
/// never reads config. `Default` is the shipped set.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ExtractLimits {
    pub max_per_input: usize,   // 32
    pub min_score: f32,         // 0.0
    pub max_text_bytes: usize,  // 256 * 1024
    pub max_content_bytes: usize, // 400
    pub max_entity_name_chars: usize, // 60
    pub max_turns: usize,       // 2000
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct GraphCandidates {
    pub entities: Vec<EntityCandidate>,
    pub relations: Vec<RelationCandidate>,
    /// true when the input was cut to `max_text_bytes`.
    pub truncated: bool,
    pub bytes_skipped: usize,
}

/// Session transcript -> memory candidates. Deterministic, bounded, pure.
pub fn memory_candidates(turns: &[Turn], limits: &ExtractLimits) -> Vec<MemoryCandidate>;

/// Knowledge document text -> entity/alias/relation candidates.
pub fn graph_candidates(text: &str, limits: &ExtractLimits) -> GraphCandidates;
```

### 7.4 The shared mechanics (both algorithms)

1. **Windowing.** `memory_candidates` keeps the **last** `max_turns` turns (recent material is
   what a session is *about*); `graph_candidates` keeps the **first** `max_text_bytes` bytes
   cut at a paragraph boundary (a document's head states its terms). Both set
   `truncated`/`bytes_skipped` so the caller can report it.
2. **Sentences.** Split on `。！？!?` and on `.`/`;` only when followed by whitespace, plus
   newlines. A sentence is trimmed and its internal whitespace runs collapsed to one space.
   Empty or single-character sentences are dropped. Sentence splitting is in
   `src/text.rs::sentences`.
3. **Verbatim content.** A candidate's `content`/`fact` is the source text (sentence or
   paragraph), trimmed and whitespace-collapsed — **never** rewritten, re-templated or
   completed. A source unit longer than `max_content_bytes` is **dropped**, not truncated:
   half a sentence is invented text, and the repo has already paid for body rewriting
   (t347, distill.rs:671-677).
4. **Normalisation for keys.** `norm(s)` = trim, collapse whitespace, lowercase, strip a
   trailing `.`/`。`. This is only used for identity/keys, never for what gets written.
5. **Ordering.** Candidates are ordered by (occurrence index ascending, rule id ascending),
   which makes every later step — dedup, truncation — deterministic.
6. **Dedup within a run.** A `BTreeSet<String>` keyed by `format!("{rule}\u{1f}{norm}")`;
   the first occurrence wins (so "first mention order" is preserved and a repeated sentence
   produces one candidate).
7. **Caps.** The ordered, deduped list is truncated to `max_per_input`, then candidates with
   `score < min_score` (graph) or a below-evidence signal... **no**: memory candidates are
   never filtered by confidence (see §8.3), only truncated.

---

## 8. Algorithm A — session transcript → memory candidates (free tier)

### 8.1 Input

`&[Turn]`, built by the daemon from the session's own transcript
(`sessions::parse_file_messages`, sessions.rs:593, whose `SessionMessage` is
`{role, text, ts}` at sessions.rs:36-39). Nothing else is read.

### 8.2 The rules

Every marker set is a `pub const &[&str]` in `crates/extract/src/rules.rs`, matched
case-insensitively for ASCII by `to_lowercase()` containment of the whole marker. The zh
markers are exactly the strings the LLM prompt names (distill.rs:19-22) and the ones the
confidence module documents (confidence.rs:14-17, 28-38) — the two tiers must ask for the
same evidence.

| rule id | role filter | marker set (const) | example markers | store | confidence |
| --- | --- | --- | --- | --- | --- |
| `user_preference` | user | `PREF` | 记住 / 以后 / 下次 / 不要再 / 别用 / 我喜欢 / 我不喜欢 / 我偏好 / 统一用 / 我的…是 / remember that / from now on / always use / never use / i prefer / i like / i don't like / don't use / call it | `profile` if the sentence contains a first-person marker (`我`, `我的`, `My `, `I `), else `observation` | `Confirmed` if the same turn also contains `CONFIRM`; else `Unconfirmed` |
| `user_correction` | user | `CORRECT` | 不对 / 不是 / 不这样 / 应该是 / 纠正 / 不是这样 / no, actually / that's wrong / actually, / correction: | `observation` | `Corrected` |
| `user_decision` | user | `DECISION` | 决定 / 就用 / 选 / 定下来 / 以后都用 / let's go with / we'll use / decided / final answer | `procedure` if the sentence names a marker from `TOOLISH`; else `observation` | `Unconfirmed` |
| `procedure_note` | assistant | `PROC` | 步骤 / 首先要 / 然后 / 否则 / 需要先 / 配置 / 安装 / 执行 / step 1 / first, / then, / otherwise, / note that / requires / run the | `procedure` | `Hedged` if the same turn contains `HEDGE`; else `Unconfirmed` |
| `lesson_learned` | assistant | `LESSON` | 坑 / 踩坑 / 注意 / 很容易 / 会导致 / 失败是因为 / 教训 / pitfall / gotcha / lesson / because it fails | `lesson` | `Hedged` if the same turn contains `HEDGE`; else `Unconfirmed` |
| `hedged_statement` | user or assistant | `HEDGE` | 可能 / 我觉得 / 好像 / 不确定 / 大概 / i think / maybe / not sure / probably | `observation` | `Hedged` |

`CONFIRM` = 对 / 就是这样 / 没错 / 正确 / perfect / exactly / yes that's right.
`HEDGE` = 可能 / 好像 / 不确定 / 大概 / i think / maybe / not sure / probably.
(These are the prompt's own words at distill.rs:19-21 and confidence.rs:28-38.)

**The correction rule also suppresses.** Implement the prompt's "extract the corrected fact
from the user's message, NOT the agent's wrong answer" (distill.rs:19) deterministically:
when user turn *i* contains a `CORRECT` marker, the candidates derived from assistant turn
*i−1* are **not emitted** (they are counted, not returned — see the `suppressed` counter
below). One-turn lookback only; a longer window is not a rule, it is a guess.

### 8.3 Confidence: signals now, numbers in one place

The extractor emits `CandidateConfidence` (a signal), and the **applier** turns it into a
number through the codebase's single rule — `ruagent_memory::confidence::confidence()`
(crates/memory/src/confidence.rs:100-114):

```rust
let c = match cand.confidence {
    CandidateConfidence::Confirmed   => confidence(&ConfidenceSignals::confirmed()),
    CandidateConfidence::Corrected   => confidence(&ConfidenceSignals::corrected()),
    CandidateConfidence::Hedged      => confidence(&ConfidenceSignals::hedged()),
    CandidateConfidence::Unconfirmed => confidence(&ConfidenceSignals::unconfirmed()),
    CandidateConfidence::Explicit(v) => confidence(&ConfidenceSignals::explicit(v)),
};
```

Why signals and not numbers in the extractor: (a) the numbers live in exactly one place
today and a second copy would drift; (b) it makes the free tier and the LLM tier produce the
*same shape*, which is the whole point of the seam (§10); (c) the confidence module's header
records that the old `[0.5,1.0]` clamp made the hedge signal unreachable
(confidence.rs:6-22) — a deterministically derived hedge at 0.4 must be able to reach the
data, not be floored.

**A hedged statement is kept, not dropped.** 0.4 < `LOW_CONFIDENCE` (0.5), so the panel
renders it unsure (confidence.rs:24-26) — visible doubt beats a silent omission.

### 8.4 Dedup / idempotence key

Three levels, all deterministic:

1. **Within one call** — `format!("{rule}\u{1f}{norm(content)}")` in a `BTreeSet`; first
   occurrence wins (§7.4.6).
2. **Across calls on the same transcript** — the function is pure, so the output is
   byte-identical; re-running produces the same candidate list. No "already seen" state is
   kept inside the crate.
3. **At the store** — the applier writes through
   `ruagent_memory::write::write_memory` (crates/memory/src/write.rs:117), whose
   `content_hash` (write.rs:79) turns a byte-identical body into
   `WriteOutcome::SkippedDuplicate` (write.rs:42), and the episode is idempotent by content
   hash (`record_episode`, used at distill.rs:641-661 with the idempotence stated in the
   comment at distill.rs:632-635). So a second run of the same pass adds nothing.

### 8.5 Boundedness guarantee (state it in the doc comment)

* output `len() <= limits.max_per_input` — enforced by truncation after ordering;
* every `content` is `<= limits.max_content_bytes`, else the unit is dropped;
* only the last `limits.max_turns` turns are read;
* work is `O(turns × |markers|)` with `|markers|` a compile-time constant; memory is
  `O(max_per_input)` plus one `BTreeSet` of the same size;
* no growth per input beyond those caps, no state between calls, no allocation proportional
  to untruncated input.

### 8.6 Gold fixture

`crates/extract/tests/gold/memory.json` holds hand-written transcripts with the candidates
they must produce, keyed by rule id, in order. Shape:

```json
{
  "cases": [
    {
      "name": "correction_beats_the_agents_wrong_answer",
      "turns": [
        {"role": "user", "text": "Rust 的 async 是怎么调度的？"},
        {"role": "assistant", "text": "可能是用 green thread 实现的，我觉得大概是这样。"},
        {"role": "user", "text": "不对，应该是用 work-stealing 的 executor。"}
      ],
      "expect": [
        {"rule": "user_correction", "store": "observation",
         "content": "不对，应该是用 work-stealing 的 executor。", "confidence": "Corrected"}
      ],
      "expect_suppressed": ["hedged_statement"]
    },
    {
      "name": "confirmed_preference",
      "turns": [
        {"role": "user", "text": "以后统一用简体中文回复我。"},
        {"role": "user", "text": "对，就是这样。"}
      ],
      "expect": [
        {"rule": "user_preference", "store": "profile", "content": "以后统一用简体中文回复我。",
         "confidence": "Unconfirmed"},
        {"rule": "user_preference", "store": "profile", "content": "对，就是这样。",
         "confidence": "Confirmed"}
      ]
    }
  ]
}
```

The test reads the fixture, runs `memory_candidates` with `ExtractLimits::default()`, and
compares rule + store + content + confidence **in order**. Precision is the target
(the requirement is "有效而非误召回"): a rule that fires on a sentence with no content token
(§8.2's guards) or produces a candidate not in the fixture fails the test.

### 8.7 Rule guards (precision, not recall)

A sentence is only considered if: it is `<= max_content_bytes`, it is not a question
(no trailing `?`/`？`), and it contains at least one **content token** — an ASCII
alphanumeric run of length ≥ 3 or a Han run of length ≥ 2 (this is what stops "对，就是这样"
being a *preference* while still allowing it to be a *confirmation*). Marker matching alone
is never sufficient.

---

## 9. Algorithm B — knowledge document text → entities / aliases / relations (free tier)

### 9.1 Input

The **whole document markdown**, as a single `&str`. The applier passes exactly the string
that `Knowledge::index_doc` chunks (crates/knowledge/src/store.rs:614-720; the content string
arrives from `Knowledge::save` at files.rs:222-236 and from `scan`/`rebuild` at
files.rs:269-350). One pass per document, not per chunk: per-chunk extraction multiplies the
candidate count across overlapping chunks (chunk overlap is 100 bytes by default,
crates/knowledge/src/chunk.rs:11-12) and produces the same entity many times, which is
exactly the unbounded growth the acceptance forbids.

**Deviation from the brief, stated deliberately:** the brief says "knowledge *chunk* text →
entities/aliases/relations". `graph_candidates` is unit-agnostic — it takes any text and
returns candidates — but the ingestion applier hands it the **document** text, for the reason
above, and the entity/document-frequency gates of §9.2 assume a document-sized window (a
single 800-byte chunk has almost no repeated terms, so `proper_noun_phrase`'s `df ≥ 2` and
`chinese_term_run`'s `df ≥ 3` would fire on nothing). A future per-chunk caller is a valid
use of the same function with a larger `max_per_input`; today, doing it per chunk would be a
change of behaviour, not a widening of this contract. The section text (a heading plus its
paragraphs, `chunk_sections`, chunk.rs:59-80) is what the *chunk* view expands to for a
reader; the document is the extraction unit.

### 9.2 Entity rules

| rule id | structural source | name rules | kind | aliases |
| --- | --- | --- | --- | --- |
| `heading_entity` | lines matching `^#{1,6}\s+` | strip leading numbering (`1.`, `3.2.`), strip trailing `:`, 2..=`max_entity_name_chars` chars, not in `STOP_HEADINGS` (overview/介绍/背景/notes/summary/参考/附录/目录/todo/see also) | `concept` (or `tool` if it matches the inline-code shape below) | the content of ONE trailing parenthetical group, plus its acronym — the same shape as `ruagent_graph::variants` (graph/src/lib.rs:909-926) and `acronym` (930-944) |
| `inline_code_identifier` | backticked spans `` `x` `` | `^[A-Za-z][A-Za-z0-9_.:/-]{2,60}$`, not in `STOP_CODE` (`true/false/null/let/fn/impl/async/await/if/else/for/while/return`, single letters) | `tool` when the span contains `/` or an extension in `.sh .md .json .toml .rs .ts .tsx .py .exe .dll` or its sentence contains an `EXEC` marker (run/install/execute/执行/安装/启动/配置); else `concept` | the hyphen/underscore twin (`a_b` ↔ `a-b`) |
| `proper_noun_phrase` | Latin text | `[A-Z][A-Za-z0-9]*( [A-Z][A-Za-z0-9]*){0,3}`, **document frequency ≥ 2**, not sentence-initial-only, not in `STOP_WORDS` (the/this/if/when/note/see/…), ≤ `max_entity_name_chars` | `concept` | the acronym of the phrase (`acronym` shape, graph/src/lib.rs:930-944) |
| `chinese_term_run` | Han runs | a Han run of 2..=12 chars, **document frequency ≥ 3**, not in `STOP_HAN` (的/是/我们/这个/可以/如果/因为/所以/以及/一个/没有/就是/不是) | `concept` | none |

Document frequency is counted in one pass over the truncated window. The term table is
capped at `MAX_DISTINCT_TERMS = 4096` entries (first-occurrence order wins); beyond the cap
new terms are not counted — deterministic and bounded, and stated in the doc comment.

`aliases` are **candidates**: the authoritative resolution still happens in the write path
(graph's `judge_against`/`variants`/`add_alias` via `apply_extraction`,
graph/src/lib.rs:1427-1489). The extractor's job is to hand it the spellings, not to decide
identity.

### 9.3 Score (there is no confidence column on the graph side)

`ExtractEntity`/`ExtractFact` carry no confidence field (graph/src/lib.rs:1375-1393), so
graph candidates carry a **ranking score** used only to cut the set:

* entity: `score = df / (df + 3.0)`, raised to `max(score, 0.5)` for `heading_entity`
  (a heading is a deliberate term, not a coincidence). `df = 1 → 0.25`, `df = 3 → 0.5`,
  `df = 9 → 0.75`.
* relation: `score = min(1.0, 0.5 × support)`, where `support` = the number of distinct
  sentences stating the same `(src, relation, dst)`. `support = 1 → 0.5` (only allowed for
  definitional relations), `support ≥ 2 → 1.0`.

The applier drops candidates with `score < min_confidence` (option, default `0.0` = keep
everything the rules produced).

### 9.4 Relation rules (conservative by construction)

Only four patterns, all requiring an explicit copula or verb, and **both endpoints must
already be entities emitted by §9.2 in the same pass** (a relation can never introduce a
name — the graph's own write path skips relations to unlisted entities,
graph/src/lib.rs:1467-1469, and this enforces the same rule before the write):

| rule id | pattern (sentence level) | relation literal | minimum support |
| --- | --- | --- | --- |
| `relation_is_a` | `X 是 Y` / `X is a Y` / `X is an Y` / `X 是一种 Y` | `is_a` | 1 |
| `relation_uses` | `X 使用 Y` / `X uses Y` / `X 依赖 Y` / `X depends on Y` | `uses` | 2 |
| `relation_runs_on` | `X 运行在 Y` / `X runs on Y` / `X 部署在 Y` / `X is deployed on Y` | `runs_on` | 2 |
| `relation_includes` | `X 包含 Y` / `X includes Y` / `X 由 Y 组成` / `X consists of Y` | `includes` | 2 |

`X`/`Y` are matched against the emitted entity names (exact, case-insensitive, or via one of
that entity's aliases). `fact` = the source sentence verbatim. `relation` is a literal from
this table and nothing else — the extractor never invents a relation name, and the graph's
own `relation_verdict` (graph/src/lib.rs:739) remains the final referee.

### 9.5 Dedup / idempotence and boundedness

* Entity dedup key: `norm(name)` (case-insensitive, whitespace-collapsed, one trailing
  parenthetical removed for identity — the `base_name` shape).
* Relation dedup key: `(norm(src), relation, norm(dst))`.
* Cross-run idempotence has **three** guards, and this is the important one:
  1. `graph_candidates` is pure ⇒ same text ⇒ same candidates.
  2. The ingestion is keyed by `(document_id, content_hash)`, and
     `Knowledge::index_doc` only runs at all when the content hash changed
     (store.rs:625-651) ⇒ an unchanged document is never re-extracted (§13.3).
  3. The write path dedups facts by `fact_hash`/`normalize_fact`
     (graph/src/lib.rs:539-562) and refusals/duplicates are counted in the returned
     `ExtractionWrite` (graph/src/lib.rs:1396-1410).
* Boundedness: `entities.len() + relations.len() <= limits.max_per_input` (default 96) after
  ordering by score descending then first-mention ascending; each name
  `<= max_entity_name_chars`; each `fact` `<= max_content_bytes` (else the relation is
  dropped); term tables capped at `MAX_DISTINCT_TERMS`; input windowed to
  `max_text_bytes`.

---

## 10. The extractor seam: one shape, two implementations

### 10.1 The seam (t4 owns `crates/daemon/src/extract_plane.rs`)

```rust
// crates/daemon/src/extract_plane.rs

/// Which implementation produced a bundle. Enum, not a trait: the set is closed
/// (two), `async-trait` is nowhere in the tree, and the repo's own precedent for
/// closed dispatch is an enum (`HarnessKind`, `FusionKind`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtractSource { Rules, Acp }

impl ExtractSource { pub fn as_str(self) -> &'static str { /* "rules" | "acp" */ } }

/// ONE extraction result, whichever tier produced it. This is the seam: the
/// writers below take this shape and cannot tell the two producers apart.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ExtractBundle {
    pub memories: Vec<ruagent_extract::MemoryCandidate>,
    pub entities: Vec<ruagent_extract::EntityCandidate>,
    pub relations: Vec<ruagent_extract::RelationCandidate>,
    pub source: Option<ExtractSource>,
    pub truncated: bool,
}
impl ExtractBundle {
    pub fn is_empty(&self) -> bool;
    /// Byte-level identity guard used when several sources run in one pass:
    /// (rule, normalized content) for memories, (base name) for entities,
    /// (src, relation, dst) for relations. Keeps the FIRST.
    pub fn dedup(mut self) -> Self;
}

/// What an extraction pass may read. Every field is already available at the
/// call site today (transcript rendering: distill.rs:520-536).
pub struct ExtractCtx<'a> {
    pub db: &'a ruagent_store::Db,
    pub session_key: &'a str,
    pub turns: &'a [ruagent_extract::Turn],
    /// The rendered transcript, in the exact form `render_transcript` produces
    /// (distill.rs:520-536) — the ACP prompt appends it verbatim
    /// (distill.rs:263, `extraction_prompt` at 890-902).
    pub transcript: &'a str,
    pub limits: ruagent_extract::ExtractLimits,
    pub registry: &'a crate::distill::AgentRegistry,
    pub language: Option<&'a str>,
    pub prompt_override: Option<&'a str>,
    /// Set by `POST /api/v1/sessions/{key}/distill` when the caller asked for
    /// a specific implementation; `None` on the unattended path.
    pub forced: Option<ExtractSource>,
}

pub enum ExtractError {
    NoSourceEnabled,
    Acp { agent: String, reason: String },
    /// The ACP reply carried no JSON object (`parse_extraction`, distill.rs:862-876).
    Malformed(String),
    Db(String),
}

/// Run every ENABLED source in the fixed order Rules -> Acp and merge the
/// bundles (dedup keeps the first, so a rule-derived candidate wins over an LLM
/// paraphrase of the same sentence).
pub async fn extract(cx: &ExtractCtx<'_>, plane: &CapabilityPlane) -> Result<ExtractBundle, ExtractError>;

/// The free implementation. Pure call + the limits the capability declared.
pub fn extract_rules(cx: &ExtractCtx<'_>, options: CapabilityOptions) -> ExtractBundle;

/// The llm implementation: today's ACP path, mapped onto the seam.
pub async fn extract_acp(cx: &ExtractCtx<'_>) -> Result<ExtractBundle, ExtractError>;
```

**Correction (t14 — the review recorded this as partly this document's own omission).** The sketch
above stops at the seam, and the seam was never the problem: nothing in it needs an agent for the
zero-token tier (`extract_rules` is a pure call over already-loaded turns, `extract_plane.rs:312`).
The **call sites** imposed the agent: both `auto_distill_now` and `session_distill` selected an
agent card BEFORE consulting the plan, so a rules-only pass on a machine with every agent
`enabled = false` — the machine the free tier exists for — failed with `no enabled agent available`
instead of extracting. The landed shape therefore makes the card OPTIONAL and resolves it LAZILY:
`Distiller::distill_plan(.., card: Option<&ruagent_core::AgentCard>, ..)` (`distill.rs:276-279`)
maps a card into `AcpExtractor` only when one was resolved and otherwise passes `None`
(`distill.rs:371`), `auto_distill_now` selects an agent only `if plan.acp` (`chat.rs:440-459`), and
`session_distill` builds the card only for the plans that include the ACP tier (`api.rs:2813-2833`).
A plan that DOES enable the llm tier with no card is still refused **by name** by the seam
(`ExtractError::Acp`, "the plan enables the ACP tier but no extractor was supplied",
`extract_plane.rs:469-478`): laziness is not a silent success. Pinned in both directions by
`chat::t8_tests::the_rules_pass_writes_with_every_agent_disabled` (`chat.rs:2524`) and
`api::tests::the_manual_rules_route_runs_with_every_agent_disabled` (`api.rs:5408`), which also
assert that an ACP-only plan on the same empty registry still fails visibly; both go red under a
mutation control that restores the eager selection (`if plan.acp` → `if true`, run in an isolated
worktree).

### 10.2 How today's ACP distillation becomes one implementation
* `extract_acp` **is** today's code path: `Distiller::ask_agent` (distill.rs:541-625) with
  `extraction_prompt(language, prompt_override)` + the transcript (distill.rs:890-902 and
  the call at 261-265), then `parse_extraction` (distill.rs:862-876).
* Its private wire structs keep their exact names and fields — `Extraction`/`ExtractedMemory`
  /`ExtractedEntity`/`ExtractedRelation` (distill.rs:38-85) and the JSON contract in
  `EXTRACTION_PROMPT` (distill.rs:29-35) are **unchanged**, so no prompt changes and no
  extraction-value drift. The only new code is a mapping:

  | wire field | candidate |
  | --- | --- |
  | `ExtractedMemory { store, namespace, content, confidence: Option<f64> }` | `MemoryCandidate { store: parse_store(&store)?, namespace, content, confidence: match confidence { Some(v) => Explicit(v), None => Unconfirmed }, rule: "acp", origin: CandidateOrigin::Document { section: None } }` |
  | `ExtractedEntity { name, kind, summary, aliases }` | `EntityCandidate { name, kind: kind.as_deref(), summary, aliases, score: 1.0, rule: "acp", .. }` |
  | `ExtractedRelation { src, dst, relation, fact, valid_at }` | `RelationCandidate { src, dst, relation: "", fact, score: 1.0, rule: "acp", .. }` + keep `valid_at` on a daemon-side `AcpFact` wrapper (the write path needs it: `event_time_source` is decided from it at distill.rs:808-812) |

  `None` confidence maps to `Unconfirmed` (0.8) — byte-identical to today, because that is
  already what distill.rs:737-740 does.
  A relation's `relation` literal is the model's own snake_case string; that path must NOT go
  through §9.4's table (it would reject every LLM relation). The candidate type therefore
  carries `relation: String` for ACP facts and a `&'static str` for rule facts — implement it
  as `pub enum RelationName { Rule(&'static str), Model(String) }` on
  `ruagent_extract::RelationCandidate`, with `relation_literal() -> &str` for the writer.
* **Landed narrowings, stated because they are behaviour, not plumbing (§21 item 7):**
  * An ACP `kind` **outside the prompt's vocabulary** (`person|project|tool|org|product|
    concept`, EXTRACTION_PROMPT distill.rs:32) becomes **no kind** (`None`), never an invented
    one — a candidate's kind is a `&'static str` and the graph then stores no kind for that row
    (`extract_plane.rs:358-361`, `393`, `438-440`, pinned by
    `an_unknown_acp_kind_becomes_no_kind`, `extract_plane.rs:772-785`). For a compliant model —
    the only kind the prompt asks for — nothing changes.
  * A **`dry_run` distill writes nothing at all**: it reports what WOULD be written and
    creates no episode, no memory row, no graph row and **no `distill_log` row**
    (`distill.rs:256-277` returns before `log_outcome`; the counts are filled in at
    `distill.rs:356-367`). A price is not a purchase, and `run_turn` episodes are what the
    panel reads as "this session was distilled" (`distill.rs:271-275`).
* `write_memories` (distill.rs:636-770) and `write_graph` (distill.rs:778-835) keep their
  bodies and change only their parameter types (`&[MemoryCandidate]`, `&ExtractBundle`),
  preserving today's merge decision (`merge_target`, distill.rs:388-413), the episode
  creation (distill.rs:641-661) and the single-transaction graph write
  (`ruagent_graph::apply_extraction`, distill.rs:816-819).
* `DistillOutcome` (distill.rs:100-108) gains three additive fields:
  `pub source: &'static str`, `pub truncated: bool`, `pub dry_run: bool` — the panel's
  existing reads (`{session_key, memories_written, …}`, api.rs:2749) are unaffected.

### 10.3 llm-tier inertness (L4, stated as behaviour)

* `distill_session` is the **only** llm capability and it defaults **off**. The unattended
  condition becomes `distill_policy.auto && plane.gate(CapabilityId::DistillSession, true)`
  — i.e. `auto` alone (chat.rs:1481) is no longer sufficient, and the gate can only narrow.
* With the **default configuration** (`[distill]` commented out ⇒ `auto == false`,
  policy/src/lib.rs:169-173) nothing changes: no ACP turn ran before, none runs now. Test:
  the equivalence test of §18 asserts `gate(DistillSession, false) == false` and that a
  close-with-auto-off performs no `ask_agent` call (the counting-embedder pattern cannot see
  a spawn, so assert instead on `distill_log` having **no new row** for that session —
  `distill_log` gets a row on every attempt, distill.rs:303-325).
* **Manual** `POST /api/v1/sessions/{key}/distill` is **not** gated (§3.7). Its optional body
  (§14.3) lets the caller choose `rules` (zero tokens), `acp` (today's behaviour, the default
  when no body is sent) or `both`, plus `dry_run`.

---

## 11. Recall legs: toggles, weights, and the regression bar

### 11.1 The additive-twin rule (why the frozen signatures stay frozen)

The repo's own precedent is explicit: `rrf` was **not** re-parameterised when weights
arrived — a second function was added, and `rrf(x, k)` was pinned bit-identical to
`rrf_weighted(x, w=1)` (crates/knowledge/src/rrf.rs:20-26, pinned by the test at rrf.rs:52-63).
This increment follows it exactly:

| today | new twin | who calls the twin |
| --- | --- | --- |
| `memembed::recall_memories(db, embedder, q, top_n, min_score)` (memembed.rs:825, a one-line delegation with `MemoryLegs::default()`) | `recall_memories_with(db, embedder, q, top_n, min_score, &MemoryLegs)` (memembed.rs:885) | `recall` handler only (api.rs:2950) |
| `Knowledge::compute_legs(q, leg_k)` (store.rs:916, private, delegates) | `compute_legs_with(q, leg_k, &LegConfig)` (store.rs:942) | via `search_page_with` |
| `Knowledge::fuse(ann, fts)` (store.rs:1141, private, delegates) | `fuse_with(ann, fts, &LegConfig)` (store.rs:1153) | via `search_page_with` |
| `Knowledge::search_page(q, limit)` (store.rs:1279) | `Knowledge::search_page_with(q, limit, &LegConfig)` (store.rs:1302) | `recall` handler only (api.rs:2971) |

The old functions become one-line delegations with the default configuration
(`LegConfig::default()`, `MemoryLegs::default()`), and a test asserts the equivalence
(`fuse(x) == fuse_with(x, default)`, `recall_memories == recall_memories_with(.., default)`).
**No existing caller changes**: `chat.rs:563` and `runs.rs:1889` keep calling `search_page`
(they are injection paths, §3.3), `knowledge_search` (api.rs:4617) keeps calling it too, and
`search`/`search_legs` (store.rs:986/1013) are untouched.

```rust
// crates/daemon/src/memembed.rs
/// Which memory legs to run and with what RRF weight.
/// Default = today: both legs on, weight 1.0 each, i.e. `rrf` exactly
/// (pinned by crates/knowledge/src/rrf.rs:52-63).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MemoryLegs { pub semantic: bool, pub keyword: bool, pub w_semantic: f32, pub w_keyword: f32 }
impl Default for MemoryLegs { fn default() -> Self { Self { semantic: true, keyword: true, w_semantic: 1.0, w_keyword: 1.0 } } }

// crates/knowledge/src/store.rs
/// Which knowledge legs to run and with what RRF weight.
/// Default = `FUSION.weights()` (2.0 : 1.0, LEG_WINDOW 60) — today exactly.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LegConfig { pub semantic: bool, pub keyword: bool, pub w_semantic: f32, pub w_keyword: f32 }
impl Default for LegConfig {
    fn default() -> Self { let (_, ws, wk) = FUSION.weights(); Self { semantic: true, keyword: true, w_semantic: ws, w_keyword: wk } }
}
```

Fusion implementation note: `recall_memories_with` must call
`ruagent_knowledge::rrf_weighted(&[(&sem_ids, w_semantic), (&kw_ids, w_keyword)], RRF_K)`
instead of `rrf(...)` (memembed.rs:581) — at 1.0/1.0 that is bit-identical
(rrf.rs:35-45 + 52-63). The memory `RRF_K = 60` (memembed.rs:187) is unchanged.

### 11.2 Leg weights and options

| leg | capability | `weight` default | how it is applied |
| --- | --- | --- | --- |
| memory semantic | `recall_leg_memory_semantic` | 1.0 | `w_semantic` in `rrf_weighted` |
| memory FTS | `recall_leg_memory_fts` | 1.0 | `w_keyword` in `rrf_weighted` |
| knowledge ANN | `recall_leg_knowledge_semantic` | 2.0 (from `FUSION`, store.rs:174-178) | `w_semantic` in `fuse_with` |
| knowledge FTS | `recall_leg_knowledge_fts` | 1.0 (from `FUSION`) | `w_keyword` in `fuse_with` |
| wiki | `recall_leg_wiki` | — | no weight (it is a partition, not a ranked leg) |
| graph | `recall_leg_graph` | — | no weight (`retrieve` has its own budget) |

`recall_leg_memory_semantic.min_score` default `None` ⇒ today's per-strategy floor
(`if conservative { 0.30 } else { 0.25 }`, api.rs:2811) stays the source; when set, it
overrides **both** strategies (documented in the option's API description).

### 11.3 Disabled-leg semantics (the "never queried" bar)

| leg off | what must NOT happen | what the response reports |
| --- | --- | --- |
| `recall_leg_memory_semantic` | no `semantic_search`, no `embed_query` for memories (`recall_memories_with`, memembed.rs:885-910; the semantic call at 902 — the delegating `recall_memories` is at 825) | `memory_legs.semantic == 0`, no hit has `"semantic"` in `legs`, `semantic_score` is `null` |
| `recall_leg_memory_fts` | no `search_fts_scored` (memembed.rs:907) | `memory_legs.keyword == 0`, `keyword_new == 0`, no hit has `"keyword"` in `legs`, `keyword_score` is `null` |
| `recall_leg_knowledge_semantic` | no `embed_query`, no LanceDB ANN query — the whole block sits inside `if legs.semantic` (`compute_legs_with`, store.rs:942-979; the embed at 950) | every knowledge hit has `semantic_rank: null`, `semantic_score: null`; `scoring.leg_window` unchanged |
| `recall_leg_knowledge_fts` | no `keyword_leg` (store.rs:999-1003; `keyword_leg` itself at 1032) | `keyword_rank`/`keyword_score` `null`, and `query_keyword_stage` is **`"disabled"`** — the endpoint composes it from the leg configuration it passed (landed at `api.rs:3723`, `keyword_stage_label(stage, keyword_leg_enabled)`, called from the `recall` handler at `api.rs:2986` with `legs.knowledge.keyword`). An ENABLED leg that ran and matched nothing still reports `"empty"`, so the two states are distinguishable — both are pinned by `crates/daemon/tests/knowledge_api.rs::a_disabled_keyword_leg_reports_disabled_not_empty` (`:1157`). Do **not** add a variant to `KeywordStage` (store.rs:492): that enum is the producer's own state, and `crates/knowledge` never sees a configuration |
| `recall_leg_wiki` | no `wiki::recall_stubs` call (api.rs:3014-3021) | `"wiki": []`; `wiki/…` documents still stay out of `knowledge` (the partition at api.rs:3009-3013 is a filter over hits already retrieved, not a query) |
| `recall_leg_graph` | no `resolve_seeds`, no `retrieve`, no `current_facts` pass (api.rs:3041-3073; the per-entity facts pass at 3101) | `graph.entities == 0`, `graph.paths == 0`, `graph.empty_reason` is `"leg disabled"` (landed at api.rs:3073 — a literal deliberately NOT one of `EmptyReason`'s values), `entities: []` |

All six legs off is **legal** and returns empty sections (never a 500): "the configuration
asked for nothing" is a reading, not an error.

### 11.4 What the endpoint and the log gain (additive, no migration)

* The response gains `"legs_disabled": ["recall_leg_graph", …]` (sorted, config-derived) —
  exactly as specified here (landed at `api.rs:3462`, built from `RecallLegConfig::disabled_legs`
  at `api.rs:2926-2930`).
* `recall_log`'s existing JSON column **`candidates_json`** gains the key
  `"disabled_legs": [...]` (landed at `api.rs:3343-3355`). **Corrected by §21 item 2:** this
  document first said `top_legs_json`, which is a JSON **array** (one entry per ranked hit,
  built at api.rs:3318-3337) — a key there would change the shape every existing reader sees,
  i.e. a default-configuration wire change, which law L1 forbids. `candidates_json` is an
  object that already grows with the fusion (`candidates`, `leg_window`, `fusion`,
  `ranked_page`). No schema change either way: the column already exists, and `recall_log`'s
  retention (`RECALL_LOG_KEEP`, api.rs:3247-3250) is untouched.
* t2 (**increment 3**) adds the memory fusion's EFFECTIVE weights — the response key
  `"memory_fusion"` and the recorder key `candidates_json.memory_fusion`, both
  `"rrf:k=60,w_semantic=<n>,w_keyword=<n>"`, read from the same `MemoryLegs` the fusion
  consumes through `MemoryLegs::effective` (= `normalized`) — so a memory-weight change is
  distinguishable from a corpus change. The EXISTING `scoring.fusion` /
  `candidates_json.fusion` keys keep their bytes: the memory key follows the RESPONSE's
  spelling, not `FusionKind::label()`'s (`rrf(k=..,w_sem=..,w_kw=..)`, `store.rs:151`).
  §20 item 2 is closed by this; see §11.5 for the declared-option surface added on top.

### 11.5 The declared-option schema and the file's own key set (`t6`, additive)

Two defects in the increment-3 panel are one change apart, and one of them writes to the user's
file. Both are fixed by making the API report the two facts the panel is currently inferring.

**Measurements (file:line, taken on `a0f1eae` + increment-3 paths).**

1. **The unreachable declared key is exactly ONE, and it is reachable in NO surface.** The
   registry declares each capability's accepted keys and their defaults at
   `crates/daemon/src/capability.rs:198-330` (`options: &'static [OptionKey]`,
   `defaults: CapabilityOptions`). Reading those rows: `recall_leg_memory_semantic` declares
   `[Weight, MinScore]` with `weight: Some(1.0)` and **no `min_score`** (`:226-230`);
   `recall_leg_memory_fts`, `recall_leg_knowledge_semantic`, `recall_leg_knowledge_fts` declare
   `weight` only, all with a default; `session_extract_rules` declares `max_per_input: Some(32)`
   + `min_confidence: Some(0.0)` (`:298-303`); `knowledge_ingest_graph` declares
   `max_per_input: Some(96)` + `max_docs_per_pass: Some(20)` (`:312-317`); the remaining five
   rows declare `&[]`. **`min_score` on `recall_leg_memory_semantic` is the ONLY declared key
   with a `None` default** — confirmed independently of the registry by reading a fresh
   daemon's `GET /api/v1/capabilities`, which reports `{weight: 1, min_score: null, …}` for
   that row and non-null values for every other declared key. The API reports RESOLVED values
   only (`CapabilityPlane::options`, `capability.rs:472-484`; `options: self.options(s.id)` in
   `rows()`, `:510`), and the panel renders an input per key whose value is non-null
   (`reportedOptions`, `panel/src/capability-options.ts:118-120`), so that row renders
   `["weight"]` and never `min_score` — measured. The panel's own comment discloses the hole
   at `capability-options.ts:113-117`. **The MCP surface has the same hole for the same
   reason**: `capability_set` has the parameter, but `capabilities_list` renders
   `options_to_text` from RESOLVED values (`crates/mcp/src/lib.rs:621-631`), so an agent
   reading the list cannot discover the key either. What is unreachable is not cosmetic: the
   handler reads that key and it changes recall (`api.rs:2957-2961`, `.map(|v| v as f32)
   .unwrap_or(if conservative { 0.30 } else { 0.25 })`, used as `min_score` at
   `api.rs:2967-2975` -> `memembed.rs:949`). Without it the override is dead code learned of
   only from a comment or this document.
2. **A write materializes registry defaults as if the user had typed them** (the reason this is
   worth doing for more than one key). The panel preserves a row's keys by re-emitting
   `row.options` — the RESOLVED values — for every row whose `configured === "file"`
   (`capabilitiesBody`, `panel/src/capability-options.ts:173-187`; `optionValues`, `:130-141`).
   `configured` says the row's ID is in the file, NOT which of its keys are
   (`CapabilityPlane::configured`, `capability.rs:489-495`), and `options()` cannot tell a key
   the file carries from one that is merely defaulted (`:472-484`). So the next write about ANY
   row re-emits `weight = 1.0` for a row whose file body was `enabled = true`. Measured on my
   own daemon, via the file: `[capabilities.recall_leg_memory_semantic]` went
   `enabled = true` (after a Reset) -> `enabled = true` + `weight = 1` on the next write.
   `set_or_remove_f64` WRITES what the body carries (`config.rs:1100-1105`), so the default the
   API reported BECOMES a key in the user's config file — a persisted change, not a display
   artefact, exactly the class of silent rewrite this effort exists to remove. It is in
   `a0f1eae` and it is live.

**The surface: two ADDITIVE fields per row, no existing field redefined.** `options` keeps
meaning "RESOLVED values" (`capability.rs:155-164`, `:472-484`) because three callers already
read it that way: the panel (`panel/src/capability-options.ts:118-141`), the MCP list
(`crates/mcp/src/lib.rs:621-631`) and the MCP write's re-emit (`:744-758`, `:785-805`). Per
`CapabilityRow` (`capability.rs:542-553`):

```json
"options_schema": [
  { "key": "weight", "kind": "float", "min": 0, "max": 100,
    "default": 1.0, "expectation": "finite and 0.0..=100.0" },
  { "key": "min_score", "kind": "float", "min": 0, "max": 1,
    "default": null, "expectation": "finite and 0.0..=1.0" }
],
"options_set": { "enabled": true, "weight": false, "min_score": true }
```

* `options_schema` — one entry per key the capability DECLARES, in registry order, generated
  from `spec(id).options` + `spec(id).defaults`. `kind` is `"float"` (`OptionKey::Weight`,
  `MinScore`, `MinConfidence`) or `"uint"` (`MaxPerInput`, `MaxDocsPerPass`, i.e. the
  `CapabilityFile` fields typed `Option<u32>`, `crates/policy/src/lib.rs:165-179`); `min`/`max`
  are the bounds `validate` already enforces (`capability.rs:608-657`); `default` is
  `null` ONLY for a key the registry leaves unset, which is the one key in (1) — so the panel
  knows a key MAY be unset without learning the count; `expectation` is the daemon's OWN
  phrase from the same table (the string a 400 already carries), so the range text a user reads
  is the daemon's and not a second wording. `options_schema` is `[]` for the five rows that
  declare nothing — their current "no editor at all" behaviour is preserved rather than
  replaced by an empty editor claiming a knob.
* `options_set` — for every declared key (plus `enabled`), whether the FILE carries it now:
  `true` = the row's `[capabilities.<id>]` body has this key, `false` = it does not and the
  resolved value comes from the registry default or the pipeline. Derived from the plane's own
  `CapabilityFile` for the id, the same value `configured` is derived from
  (`capability.rs:489-495`, `535-537`). When the table is absent every entry is `false`,
  `enabled` included. Read the example as ONE ROW's state, not a whole file's: the file names
  this id, carries `min_score = 0.4`, and carries neither `weight` nor `enabled` — so the row
  resolves `weight = 1.0` from the registry and `enabled = true` from `default_enabled`, and a
  write about another row must preserve exactly that. `enabled` is in this object because it has
  the same present-or-defaulted ambiguity as an option value.

**Do NOT redefine `options`, and do not widen `configured`.** `configured` keeps its three
values and its meaning ("legacy" | "default" | "file"); a client that reads only `options` and
`configured` sees a byte-identical payload. **Why additive rather than "just expose the schema
in `options`":** `options` is what three existing callers parse as resolved values (the panel,
the MCP list, the MCP write's re-emit), and the MCP writer's read-modify-write loop is the one
that keeps a single toggle from resetting every other capability — redefining the field it
re-emits through would break the write path of a surface that was just verified, and this
increment's law is additive-only. Two new fields cost a client that ignores them exactly
nothing.

**The panel side, down to the symbols removed** (`panel/src/capability-options.ts`, 187 lines
today). REMOVE: the `OPTION_KEYS` array (`:35-41`), the `OptionRule` interface (`:45-54`), the
`OPTION_RULES` table (`:56-67`) — the copy of `capability.rs:608-657` — and the `OptionKey`
type (`:33`, a `keyof CapabilityOptions`), which becomes `string` derived from the response.
KEEP, changed to read the schema instead of the table: `validateOptionValue` (`:89-101`) takes
the schema entry for the key and applies the same order — the `DECIMAL` shape test (`:82`),
then finiteness, then `kind === "uint"` => integer, then `min`/`max`; `reportedOptions` becomes
`row.options_schema.map(e => e.key)` (every DECLARED key, so the key with a `null` default
renders an input, pre-filled with the resolved value or empty); the refusal text uses the
entry's `expectation` instead of `OPTION_RULES[key].expectation`
(`panel/src/views/Settings.tsx:305-316`). KEEP `RESET_OPTIONS` (`{}`, `:146`), `RowEdit`
(`:149-155`) and `capabilitiesBody` (`:173-187`) — the read-modify-write discipline is
correct — with the non-edited branch re-emitting each row's `options_set` keys (plus `enabled`
when `options_set.enabled`) instead of every resolved value, which is what removes defect (2).
`panel/src/api.ts` gains the two fields on `CapabilityRow` (`:296-307`) and the schema type
next to `CapabilityOptions` (`:281-287`); the shape guard (`:747-768`) does not need them
(absent fields must not fail the guard, or an older daemon becomes a client-side error). The
MCP's own copy of the key list (`crates/mcp/src/lib.rs:613-619`) is a THIRD spelling of the
same fact and should be removed in the same change by iterating the row's `options_schema`;
that is `crates/mcp`, outside this document's file set, so it is named here as the follow-up
rather than folded in.

**The cross-field invariant: resolve it at the WRITE door, not at boot (option 3 of three).**
Measured today: `PUT` accepts `{"<recall leg>": {"enabled": true, "weight": 0.0}}` with 200,
and the NEXT `GET /api/v1/recall` is refused 400 — `` recall leg configuration is invalid: leg
`memory semantic` is enabled with weight 0: an enabled leg must have a weight > 0 (disable the
leg instead of zeroing it) `` (`memembed.rs:654-663` -> `check_weights`,
`crates/knowledge/src/rrf.rs:81-97`, which refuses `enabled && weight <= 0.0`; the negative half
is already refused at the door by the range check, so the runtime-refused case is exactly
`weight == 0.0`). The three candidates and why the third wins:

* *Enforce in `CapabilityPlane::from_policy`* (so `PUT` AND boot both refuse): **rejected.** It
  makes a `policy.toml` that boots today fail to start after the upgrade — a daemon that
  refuses to start cannot show the user which line to fix. Trading a loud refusal at read time
  for a dead daemon is worse than the defect.
* *Stop refusing at read time* (an enabled leg at 0.0 just contributes nothing): **rejected.**
  That refusal is what makes `w_semantic=0` in a recall record mean "off" rather than "weighted
  to zero" — the reading §11.4's `memory_fusion` evidence depends on — and `check_weights` is
  the shared, knowledge-side guard, not a daemon local.
* *Refuse NEW writes that create the state; leave load and read exactly as they are* (the
  `unconfirmed_llm_enable` shape, `capability.rs:724-736`): **chosen.** The `PUT` handler
  already builds `before` and `after` planes for the cost gate; add the mirrored check over the
  memory leg pair on the SAME two values, after `from_policy` and before the write, and refuse
  with a body that reuses the runtime's sentence verbatim (`says "disable the leg instead of
  zeroing it"`, naming the id and the key). Consequences, stated: a `policy.toml` that ALREADY
  contains `enabled = true, weight = 0.0` still boots and still falls back to today's behaviour
  (recall 400s, loudly, naming the leg) — no migration, no dead daemon; the panel can no longer
  CREATE it, and because the rule is now enforced at the write door the panel can mirror it
  FROM DATA (the schema cannot express it, so the refusal comes back as the daemon's own
  message, verbatim, exactly as every other daemon refusal is rendered today,
  `Settings.tsx:311-316`); and an agent has no way to create it in the first place
  (`capability_set` takes `Option<f64>`, so `0.0` is expressed as an omitted key = "keep the
  current value", `crates/mcp/src/lib.rs:1183-1189`). The write-door rule is stated for the
  MEMORY legs, which is where that rule is enforced and where I measured it; whether
  `recall_leg_knowledge_*` gets the same guard is a separate, unverified question and is NOT
  asserted here.

**What must NOT change.** No existing key's value moves and no key is redefined
(`options`, `configured`, `table_present`, `config_file`, `conflicts` and every recall key keep
their bytes; the two new fields are absent-before, present-after). The whole-table
read-modify-write discipline stays (`config.rs:1071-1114`): the body is still the complete
table, a row it omits is still DELETED, and `set_or_remove_*` still removes every key the entry
omits — the change only makes the panel emit the keys the file ACTUALLY carries. That cuts both
ways and the ORIGINAL behaviour is the one preserved here: for a row this write is not about,
the rule is "do not invent keys": for a row this write is not about, an option key is emitted
only when `options_set` says the file already carries it, and the `enabled` key is emitted only
when `options_set.enabled` says the same — so a file body that is `{enabled}` (the shape a Reset
leaves) does not gain a `weight` back on the next unrelated write. `enabled` is still always
emitted for the row the write IS about (it is the switch this card writes). Reset still REMOVES
the row's keys (`RESET_OPTIONS = {}`, `capability-options.ts:146`) rather than writing defaults
in, and an emptied field still means unset rather than a written default.

**The closure test — named.**
`crates/daemon/tests/knowledge_api.rs`, next to the existing capability tests:
`the_panel_sees_every_declared_key_and_a_write_does_not_materialize_defaults`. It seeds a
`policy.toml` whose `[capabilities]` table configures TWO rows — `knowledge_ingest_graph` with
its keys, and `recall_leg_memory_semantic` with `enabled` + `weight` ONLY (the row shape after
a Reset) — then: (a) asserts the second row's `options_schema` contains `min_score` with
`default: null` and its `options_set` says `min_score: false`; (b) PUTs the panel's body with
`min_score = 0.4` added to that row and asserts the key lands in policy.toml and reads back as
`0.4` while the OTHER row's keys are byte-identical in the file; (c) PUTs an unrelated change
(toggling `recall_leg_wiki`) and asserts the file's second row still carries ONLY
`enabled`/`weight`/`min_score` — i.e. `weight = 1.0` was NOT materialized by a write about a
different row (the exact regression of §11.5 (2)); (d) PUTs `enabled: true, weight: 0.0` on a
memory leg and asserts 400 naming the leg, with the file byte-identical and the next recall
still succeeding. Steps (a)-(c) mirror the existing partial-PUT precedent
`crates/daemon/tests/capabilities.rs:239` ("an option key the request did not name keeps its
declared default") and the write-door refusal is a new case beside
`put_refuses_an_undeclared_option_key_and_an_out_of_range_value` (`:294`).

**Cost, and the honest case for not doing it.** One additive struct + one projection in
`capability.rs` (schema entries built from `spec(id).options`/`defaults`, `options_set` from
`file_of`) and the write-door check; `panel/src/capability-options.ts` loses its table and its
`OptionKey` type and gains two readers; `Settings.tsx` loses one import and one
`OPTION_RULES[key].expectation`; `api.ts` gains two fields. No migration, no new route, no
config-surface change beyond the misconfiguration refusal. Against: the ONE unreachable key
would be cheaper to fix by giving `min_score` a declared default — and that is rejected on its
own merits, not on cost: a default IS a behaviour, and inventing one for a key whose whole
point is "the caller's strategy floor decides when unset" (`api.rs:2957-2961`) would change
recall for people who never asked. The change is therefore justified by (2), which is live in
`a0f1eae` and rewrites the user's file on an unrelated write; exposing the unreachable key is
what the same field buys. **'Do nothing' is the wrong call here** for that reason, and it is
recorded as wrong rather than left open.

**Follow-up (not performed here):** a spike that boots the daemon with the two fields, GETs the
rows, renders the editor from `options_schema` alone, and diffs `policy.toml` before/after a
write that touches a different row — the shape of §11.5's closure test, run live.

### 11.6 The regression bar (pinned, testable)

1. `cargo test --workspace` green, including the existing recall assertions:
   crates/daemon/tests/knowledge_api.rs:133/154 (both strategies), 616-660 (wiki stubs are
   always stubs), 721-741 (the `strategy` column of `recall_log`),
   crates/knowledge/tests/retrieval-legs.rs, crates/knowledge/tests/score-kind-wire.rs and
   crates/graph/tests/empty-recall-pattern.rs.
2. New equivalence test (t5): one fixture database, two runs of `GET /api/v1/recall` —
   (a) a `policy.toml` with **no** `[capabilities]` table, (b) a table with all six legs
   `enabled = true` and their default weights. Assert equality of the load-bearing keys:
   `memories` (ids, order, `score`, `legs`), `knowledge` (chunk ids, order, scores),
   `wiki` (slugs), `entities` (ids and order), `memory_legs`, `graph`, `scoring`.
   Landed as `the_default_leg_configuration_reproduces_the_legacy_recall`
   (`crates/daemon/tests/knowledge_api.rs:879`).
   **Scope note (increment 2):** this bar is *"the configuration table is a no-op at its
   defaults"* — both sides run the same code, so it stays green. The *other* bar, "the default
   equals the PRE-capability-plane platform", was increment 1's law L1, and increment 2
   deliberately moved it on the memory side by repairing one legacy defect (§21.1); the daemon's
   memory golden was re-derived there, not here.
3. New leg-off tests (t5): a counting `Embedder` (the repo already has the pattern, e.g. the
   test embedder at distill.rs:1664) asserting `embed_query` calls drop from 2 to 1 when
   `recall_leg_knowledge_semantic` is off and to 0 when both semantic legs are off; plus one
   assertion per row of §11.3. Landed over HTTP as
   `a_disabled_wiki_or_graph_leg_is_reported_and_leaves_its_section_empty` (:921),
   `all_six_legs_off_is_an_empty_success_that_names_every_leg` (:967),
   `both_strategies_answer_over_the_toggleable_legs` (:1032) and
   `a_non_default_leg_weight_reorders_the_recall_page` (:1075), and in the crate as
   `a_disabled_memory_semantic_leg_is_never_queried` (`memembed.rs:1350`). That last pin's
   OBSERVABLE was re-derived by increment 2 (t16): it used to assert the unobservability of the
   memory-FTS leg's on/off state (the leg returned nothing either way because of the bm25 index
   defect), and it now asserts the leg's contribution and the difference the switch makes —
   §21.1.

### 11.7 The id-convergence shape as landed (the engine names no capability id)

§11.1/§11.2 said the recall engines would take the leg configuration from the plane, which left
open *where* a capability id is spelled. The landed shape answers that, and it is better than
what §11 implied — **§21 item 8** records it as the design:

* `crates/daemon/src/memembed.rs` exposes a typed `RecallLeg` enum — six variants
  (`Graph`, `KnowledgeFts`, `KnowledgeSemantic`, `MemoryFts`, `MemorySemantic`, `Wiki`),
  **declaration order = report order** (`memembed.rs:564-589`). `label()` is a HUMAN label for
  a log/reason sentence, never a wire value.
* `crates/knowledge/src/store.rs` exposes `KnowledgeLeg { Semantic, Keyword }`
  (`store.rs:209-222`), and `LegConfig::disabled_legs()` returns those types
  (`store.rs:302-308`).
* `RecallLegConfig { memory: MemoryLegs, knowledge: LegConfig, wiki: bool, graph: bool }`
  (`memembed.rs:708-714`) resolves all six from the plane's `(enabled, weight)` readings in one
  call (`resolve`, `memembed.rs:731-745`), and **`disabled_legs()` is THE ONE PLACE that knows
  all six** — it chains the two engines' typed lists, maps `KnowledgeLeg -> RecallLeg`
  (`memembed.rs:762-787`), and returns types, not strings.
* The **single** id mapping is `crates/daemon/src/api.rs::recall_leg_id(RecallLeg) ->
  &'static str` (`api.rs:2844-2863`), whose strings come from
  `crate::capability::CapabilityId::as_str()`. The endpoint is the right owner because the
  endpoint is what speaks capability ids.
* `crates/knowledge` stays the lower layer and **`memembed` does not depend on
  `crate::capability` at all** (its only occurrences of the word are doc comments:
  memembed.rs:555-562, 675, 705, 759-761, 1536-1538).

The checkable chain a reviewer can follow without re-deriving anything:
`RecallLegConfig::disabled_legs` (6 typed entries when all six are off) →
`recall_leg_id` (6 arms, ids from the registry) → the response's `legs_disabled`
(`api.rs:2926-2930`, `3462`) and `candidates_json.disabled_legs` (`api.rs:3343-3355`).

---

## 12. Injection gates (memory digest)

* `memory_inject_chat` gates `ChatManager::injection_context`'s **call site** (chat.rs:169-203)
  — when off, no context is built and `memory_injected` is not consumed by an injection, so
  the prompt carries the role/handoff blocks (if any) and nothing from the retrieval stores.
  The honest implementation: `let ctx = if plane.gate(MemoryInjectChat, true) { …existing… }
  else { None };` and leave everything else (the sentinel logic at chat.rs:226-229, the
  handoff/role blocks at 178-201) exactly as it is.
* `memory_inject_runs` gates `render_run_injection` (runs.rs:1856) at its call site
  (runs.rs:771): off ⇒ the injected string is empty.
* Both default **on** with the table absent (L1), so no default behaviour changes.

---

## 13. Knowledge base → knowledge graph ingestion (off by default)

### 13.1 Where it hangs

There is no new job. The daemon's existing knowledge scan loop (lib.rs:230-248) calls
`Knowledge::scan()` every 60 s (files.rs:269-311); the ingest sweep runs **inside that same
loop body**, after the scan, only when the capability is on and only for documents the
ledger has not seen. A second, immediate path serves the explicit HTTP/MCP call.

```
                ┌───────────────────────────────────────────────────────────┐
knowledge/…md ─▶│ Knowledge::save / scan / rebuild  (files.rs:222/269/316) │─▶ chunks + vectors
                └───────────────────────────────────────────────────────────┘
                                   │ (one 60s loop, lib.rs:230-248)
                                   ▼
     capability knowledge_ingest_graph ON?  ──no──▶ nothing (ledger untouched)
                                   │yes
                                   ▼
       documents ⋈ NOT IN knowledge_graph_ingest (document_name, content_hash)  [≤ max_docs_per_pass]
                                   ▼
       read_raw(name) ─▶ extract::graph_candidates(text, limits)   (ZERO tokens)
                                   ▼
       ruagent_graph::apply_extraction(db, &entities, &facts, "knowledge", None)  (one tx)
                                   ▼
       — on a CHANGED document, also retract the previous revision (invalidate its facts;
         delete only the entities nothing else claims) — knowledge_graph.rs:39-65
                                   ▼
       UPSERT knowledge_graph_ingest ON CONFLICT(document_name) (counts, entities_json, facts_json)
```

### 13.2 Cost control (four teeth)

1. **Zero tokens by construction** — `graph_candidates` is a pure function (§9) and nothing
   in this path calls a model. There is no llm-tier variant (§3.5).
2. **Off by default** (`default_enabled = false`).
3. **Only changed documents** — `Knowledge::index_doc` already short-circuits on an unchanged
   content hash (store.rs:625-651), and the sweep *additionally* filters on the ledger, so a
   rebuild (`rebuild`, files.rs:316) cannot re-ingest an unchanged document either.
4. **Bounded catch-up** — at most `max_docs_per_pass` (default 20) documents per sweep, so
   turning the capability on with a 6 000-file knowledge tree ingests 20 per minute, never
   all at once. `max_per_input` (default 96) bounds candidates per document.

### 13.3 Schema: one new table

`crates/store/src/migrations/0026_capability_ingest.sql` (registered by one line in
`crates/store/src/migrations.rs`'s `MIGRATIONS` list, migrations.rs:8-34; `SCHEMA_VERSION =
MIGRATIONS.len()` at migrations.rs:37 becomes 26). Follow the 0025 style (a long header
comment that states the defect/decision, then idempotent DDL).

**The key landed as the document NAME, not `(document_id, content_hash)`** (§21 item 1):

```sql
-- 0026: the knowledge → graph ingestion ledger (t4).
-- ONE ROW PER DOCUMENT NAME (the CURRENT revision). NOT keyed on document_id:
-- `Knowledge::index_doc` DELETES the documents row and INSERTs a new one on every
-- content change (crates/knowledge/src/store.rs:652-654 and 672-683), so
-- `documents.id` is reassigned on every revision and cannot anchor "the same
-- document, new revision" — the key the revision rule below must hang on.
-- `documents.name` is stable: it is the upsert key of index_doc (store.rs:638)
-- and the argument of `Knowledge::read_raw` (files.rs:239).
CREATE TABLE IF NOT EXISTS knowledge_graph_ingest (
    id            INTEGER PRIMARY KEY,
    document_name TEXT    NOT NULL UNIQUE,  -- the ledger KEY
    -- Informational: the `documents.id` this revision happens to have. Reassigned
    -- by index_doc on the next revision; never a key, so no index on it.
    document_id   INTEGER NOT NULL,
    -- sha256 (ruagent_knowledge::sha256_hex) of the markdown this revision came from.
    content_hash  TEXT    NOT NULL,
    entities      INTEGER NOT NULL DEFAULT 0,
    relations     INTEGER NOT NULL DEFAULT 0,
    candidates    INTEGER NOT NULL DEFAULT 0,
    -- What THIS revision wrote: entity names / `ruagent_graph::fact_hash` values,
    -- as JSON arrays of strings. They are the only record of what a future
    -- revision must retract, since the graph does not record which document a
    -- fact came from and the new text no longer mentions the old one.
    entities_json TEXT    NOT NULL DEFAULT '[]',
    facts_json    TEXT    NOT NULL DEFAULT '[]',
    ingested_at   TEXT    NOT NULL
);
```

Two things this document did not specify and the code had to add: the revision rule
(a changed document retracts what its previous revision contributed — facts are invalidated,
never deleted; an entity is deleted only when nothing else claims it and every edge it still
has came from the revision being replaced) and the `ON CONFLICT(document_name) DO UPDATE`
upsert. Both are stated once in
`crates/daemon/src/knowledge_graph.rs:32-65` and the migration header
(`0026_capability_ingest.sql:12-40`); the code is the spec for them.

`KnowledgeDocument` (store.rs:389-396) gains `pub content_hash: String`, selected in
`list_documents` (store.rs:1303-1315). This is an **additive JSON field** on
`GET /api/v1/knowledge/documents`; the existing tests index that response by name
(crates/daemon/tests/knowledge_api.rs:205-220, 321-336), so nothing breaks. Supporting
`source`-less legacy rows is out of scope: the sweep skips a document whose `read_raw`
returns `None` and records nothing for it.

### 13.4 The module and its report

```rust
// crates/daemon/src/knowledge_graph.rs
pub struct IngestReport {
    pub documents: u32, pub skipped: u32, pub entities: u32, pub relations: u32,
    pub duplicates: u32, pub refused: u32, pub truncated: u32,
    pub candidates: u32,
    pub ledger_hits: u32,       // documents skipped because the ledger already had them
}
/// Ingest ONE document (idempotent per content_hash): extract → apply → ledger.
pub async fn ingest_document(db: &Db, kb: &Knowledge, name: &str, opts: &IngestOptions) -> Result<IngestReport, anyhow::Error>;
/// Sweep up to `max_docs_per_pass` documents the ledger has not seen, newest-name-first
/// deterministic order (by (created_at DESC, id ASC), the order `list_documents` already uses).
pub async fn sweep(db: &Db, kb: &Knowledge, opts: &IngestOptions) -> Result<IngestReport, anyhow::Error>;
pub struct IngestOptions { pub max_per_input: usize, pub min_confidence: f32, pub max_docs_per_pass: u32, pub dry_run: bool }
```

`dry_run` runs extraction and returns counts without calling `apply_extraction` and without a
ledger row — the way a user can price the ingestion ("what would this add?") before enabling
it.

---

## 14. MCP surface (t6) — exact tools, arguments, descriptions

Four new `#[tool]` functions in `crates/mcp/src/lib.rs` (the file's 14 existing ones are
listed in §2.1 claim 5). Argument structs are `schemars::JsonSchema` like the existing ones
(mcp/src/lib.rs:528-616) and each tool is a thin HTTP proxy — the bridge's whole design
(mcp/src/lib.rs:1-7). **Do not add a fifth write path**: every tool calls an existing or
newly-added daemon route.

| tool | arguments (name: type) | one-line `when to call this` (goes in the `#[tool(description = …)]`) | route |
| --- | --- | --- | --- |
| `capabilities_list` | `tier?: "free"\|"llm"` | "Call this before relying on a platform pipeline (recall legs, distillation, knowledge ingestion): it reports which capabilities are on, their token tier, and what each one gates." | `GET /api/v1/capabilities` |
| `capability_set` | `id: string`, `enabled: boolean`, `weight?: number`, `min_score?: number`, `max_per_input?: integer`, `min_confidence?: number`, `max_docs_per_pass?: integer`, `confirm_cost?: boolean` | "Call this when the user asks to turn a pipeline on or off (e.g. 'stop distilling sessions automatically', 'ingest the knowledge base into the graph'). Enabling an llm-tier capability spends model tokens and requires `confirm_cost: true`." | `PUT /api/v1/capabilities` |
| `distill_session` | `session_key: string`, `extractor?: "acp"\|"rules"\|"both"` (default `"acp"`), `dry_run?: boolean` (default false) | "Call this when the user asks to distill one session now: `extractor: \"rules\"` costs no tokens, `\"acp\"` (the default) runs the extraction agent, `dry_run: true` returns what would be written without writing." | `POST /api/v1/sessions/{key}/distill` |
| `knowledge_graph_ingest` | `document?: string` (a document name; absent = sweep), `dry_run?: boolean` (default false) | "Call this when the user wants the knowledge base's entities and relations in the graph, or asks why a document's entities are missing: without `document` it sweeps up to the configured per-pass limit." | `POST /api/v1/knowledge/graph/ingest` |

As landed (§21 item 10): the four tools exist and the surface is **18 tools**; the shipped
`#[tool(description = …)]` strings are **supersets** of the one-liners above — they add a
`WHEN:` clause and a `COST:` clause to each (e.g. `knowledge_graph_ingest`,
`crates/mcp/src/lib.rs:431-433`), which is the intent of this column, not a divergence from
it. The ingest tool sends `document`/`dry_run` in the **query string on a POST**
(`mcp/src/lib.rs:439-441`, `ingest_query` at 955), matching the daemon handler's
`Query<GraphIngestQuery>` (`api.rs:4770-4781`).

Rules for t6:

* `capability_set` must **echo the resulting state**: the HTTP `PUT` returns the same payload
  as `GET` (§16), and the tool returns it pretty-printed (the file's existing pattern for
  structured replies: `serde_json::to_string_pretty` at mcp/src/lib.rs:369, 399, 418, 437).
* A refusal (unknown id, out-of-range value, missing `confirm_cost` for an llm capability) is
  returned as an `rmcp::ErrorData` **naming the id/key** — never as an empty success (the
  file's own rule about protocol-drift absorbers, mcp/src/lib.rs:272-288 and `facts_of` at
  484-489).
* `get_info().instructions` (mcp/src/lib.rs:518-525) gains one sentence: "## Platform
  capabilities — call `capabilities_list` before depending on recall, distillation or
  knowledge ingestion; the list names each capability's token tier."
* **`crates/mcp/tests/roundtrip.rs:132-152` must be updated in the same change** — the
  `expected` vector gains the four names, and `assert_eq!(expected.len(), 14, …)` becomes 18.
  The test drives a real in-test daemon over HTTP (roundtrip.rs:39-104) and each new tool
  needs one call assertion in the same style as the `memory_write` call at roundtrip.rs:162-169.

### 14.1 `GET /api/v1/capabilities` response (t2)

```json
{
  "table_present": false,
  "config_file": "/home/u/.ruagent/config/policy.toml",
  "capabilities": [
    { "id": "distill_session", "tier": "llm",
      "description": "ACP-agent distillation when a session closes (spends model tokens). Manual distillation is an explicit request and is never gated.",
      "gates": "auto-distill on session close (crates/daemon/src/chat.rs)",
      "default_enabled": false, "enabled": false, "configured": "legacy",
      "new": false,
      "options": { "weight": null, "min_score": null, "max_per_input": null,
                   "min_confidence": null, "max_docs_per_pass": null } }
  ],
  "conflicts": [
    { "id": "distill_session", "legacy_key": "distill.auto",
      "reason": "[distill] auto = true but capability `distill_session` is off: unattended distillation will not run. Add [capabilities.distill_session] enabled = true to restore it." }
  ]
}
```

`configured` reports **where the value came from**: `"legacy"` (no table at all), `"default"`
(the table exists and this id is not in it), `"file"` (the file names this id). The API's own
reading is "whether it differs from the registry default (`configured == \"file\"`)"
(`capability.rs:486-495`, `713-716`) — as landed.

`conflicts` is **what this configuration is narrowing**: one entry per legacy pair whose
LEGACY flag asks for work the gate switches off. Landed shape (§21 item 5):
`conflicts(distill_auto)` pushes the `distill.auto` entry only when
`distill_auto && !gate(DistillSession, true)` (`capability.rs:520-533`) — so in **legacy mode
it always reports nothing**, even with `[distill] auto = true`, because nothing is being
narrowed there. The conflict appears exactly when the user has turned the table on and thereby
lost the unattended path.

### 14.2 `PUT /api/v1/capabilities` (t2)

Request: `{ "confirm_cost": false, "capabilities": { "<id>": { "enabled": true, "weight": 0.5 } } }`.
The map **replaces** the whole table (an empty map ⇒ the table is removed ⇒ legacy mode).

1. Validation reuses `from_policy` **on a synthesized policy**: the handler builds
   `PolicyConfig { capabilities: table, ..Default::default() }` and calls
   `CapabilityPlane::from_policy` (`capability.rs:759-763`), so there is no second validation
   path to drift from the boot path. Unknown id ⇒ `400` naming it and listing the known ids;
   unknown option key ⇒ `400` naming id + key; out-of-range value ⇒ `400` naming id + key +
   the range. **Corrected by §21 items 3 and 5:** the refusals are the daemon's existing
   plain-text `ApiError` body, not a `{"error": …}` JSON envelope (mcp-dev's client reads both,
   `crates/mcp/src/lib.rs:1862-1877`).
2. Cost gate: `unconfirmed_llm_enable(before, after, confirm_cost)` compares **configuration**
   — `spec.tier == Tier::Llm && after.enabled(id) && !before.enabled(id)`
   (`capability.rs:724-736`) — so switching `distill_session` on from off requires
   `confirm_cost: true` even when `[distill].auto` is false. That is deliberately conservative
   (the capability is the thing being enabled, and the legacy flag can be flipped later).
   Without confirmation ⇒ `409` with the plain-text body
   `capability `distill_session` is llm-tier: enabling it spends model tokens. Resend with
   "confirm_cost": true.` (refuse loudly rather than proceed silently — the repo's refusal
   discipline, e.g. config.rs:440-497).
3. `CapabilitiesEditor::update` writes `policy.toml` (round-trip). An EMPTY map writes the
   table away: the handler maps `{}` to `table = None` itself (`capability.rs:750-756`), so
   "no capabilities configured" and "an empty table" cannot be confused by a caller.
4. `state.chats.set_capabilities(after)` swaps the live value for both managers.
5. Respond `200` with the §14.1 payload — the handler re-reads the live plane
   (`capability.rs:776`), so a client renders straight from its own response.

### 14.3 `POST /api/v1/sessions/{key}/distill` (t4)

Body is **optional and read explicitly**:

```rust
#[derive(serde::Deserialize)]
struct DistillBody { #[serde(default)] extractor: Option<String>, #[serde(default)] dry_run: bool }
// axum::body::Bytes, then:
//   * empty body  -> today's behaviour (ExtractSource::Acp, dry_run = false)
//   * non-empty   -> serde_json::from_slice, a parse error is 400 naming the field
```

Why `Bytes` and not `Option<Json<_>>`: `Option<Json<T>>` swallows every rejection, so a
malformed body would silently become "no body" — a silent default, forbidden by L3. Allowed
`extractor` values are `"acp"` (default), `"rules"`, `"both"`; anything else is `400` naming
the value and listing the three. Response: `{"distilled": {"session_key", "memories_written",
"memories_skipped", "entities_written", "relations_written", "agent", "source", "truncated",
"dry_run"}}` — the first six keys are today's exact shape (api.rs:2749).

---

## 15. Operator skill (t6, second half)

`skills/ruagent-operator/SKILL.md` (43 lines today; installed into `<root>/skills` at boot,
lib.rs:138-147) gains a section and updates one bullet:

```markdown
## Platform capabilities (what is switched on)

- Call `capabilities_list` **before** you rely on recall, distillation or
  knowledge ingestion. It reports, per capability: the token tier (`free` =
  zero tokens, deterministic; `llm` = spends model tokens), whether it is
  enabled, and what it gates.
- Turn a pipeline on or off with `capability_set`. Enabling an `llm`
  capability spends the user's tokens: say so and pass `confirm_cost: true`
  only after the user agrees.
- Distill one session with `distill_session`: `extractor: "rules"` is
  zero-token and deterministic; `extractor: "acp"` (the default) runs the
  extraction agent; `dry_run: true` shows what would be written.
- Put a document's entities and relations into the knowledge graph with
  `knowledge_graph_ingest` (`dry_run: true` first).
```

Update the existing "Knowledge (searchable documents)" section (SKILL.md:25-31) to mention
that `knowledge_ingest` also feeds the knowledge graph **when
`knowledge_ingest_graph` is enabled**, and that it is off by default. Keep the file's 4
existing rules (SKILL.md:38-43) — add one: "5. Never enable an `llm`-tier capability without
the user's explicit agreement: it spends their tokens."

---

## 16. HTTP surface: complete list of changes

| method + path | owner | change |
| --- | --- | --- |
| `GET /api/v1/capabilities` | t2 | NEW (route line + handler in `capability.rs`) |
| `PUT /api/v1/capabilities` | t2 | NEW |
| `POST /api/v1/sessions/{key}/distill` | t4 | EXISTING (route api.rs:178): optional body, `source`/`truncated`/`dry_run` in the response |
| `POST /api/v1/knowledge/graph/ingest` | t4 | NEW (sweep or one document, `dry_run`) |
| `GET /api/v1/knowledge/graph/ingest/status` | t4 | NEW (ledger counts + last N rows; the "is my knowledge base in the graph?" answer) |
| `GET /api/v1/recall` | t5 | EXISTING (api.rs:179): leg config from the plane; `legs_disabled` added; leg-off semantics per §11.3 |
| `GET /api/v1/knowledge/documents` | t4 | additive `content_hash` per row (§13.3) |

---

## 17. Task breakdown: exact file sets, order, and shared files

### 17.1 The real dependency graph (and why the scheduler's is not enough)

Tasks t2–t7 declare only t1 as a dependency, but the compile-time truth is: t4/t5/t6 need
t2's `capability` module and t3's crate to exist, t6's roundtrip test needs t2's and t4's
routes, and t7 needs t2's API. **Run this order; if you are dispatched before your inputs
exist on disk, stop and report instead of inventing a private interface.**

```
Wave A (parallel, disjoint):      t2 (capability core)   t3 (crates/extract)
Wave B (needs A):                 t4 (seam + KB→graph)   t5 (recall legs)   t7 (panel, needs t2 only)
Wave C (needs A+B routes):        t6 (MCP + skill)
Wave D (needs all):               t8 (integration) → t9 (verification) → t10 (review)
```

### 17.2 t2 — capability registry, `[capabilities]` config, list/update API (platform-dev)

| file | change |
| --- | --- |
| `crates/daemon/src/capability.rs` | NEW: §4.1–4.4 (ids, tiers, option keys, specs, `CapabilityPlane`, `CapabilityError`, `CapabilityRow`, `conflicts`), the two HTTP handlers, inline unit tests |
| `crates/policy/src/lib.rs` | `CapabilityFile` + `PolicyConfig.capabilities: Option<BTreeMap<String, CapabilityFile>>` (§5.1) |
| `crates/daemon/src/config.rs` | `CapabilitiesEditor` (§5.3), the commented `[capabilities]` block in `DEFAULT_POLICY_TOML`, the round-trip test |
| `crates/daemon/src/lib.rs` | `pub mod capability;`; the `capabilities` field on `DaemonConfig` + its construction in `load`; keep the boot WARN of §5.4 here (t8 will call `from_policy` from the same place) |
| `crates/daemon/src/chat.rs` | the `capabilities` field on `ChatManager` (initialised to `CapabilityPlane::legacy()` in `new`) + `capabilities()`/`set_capabilities()` |
| `crates/daemon/src/api.rs` | 2 route lines only (`GET`/`PUT /api/v1/capabilities`) |
| `crates/daemon/tests/capabilities.rs` | NEW: registry conformance (L4), unknown id/key/value, PUT/GET round-trip, editor round-trip, absence = legacy |

Shared files: `api.rs` (t2, t4, t5 — sequential), `chat.rs` (t2, t8 — sequential),
`lib.rs` (t2, t4, t8 — sequential), `tests/capabilities.rs` (t2, t8 — sequential).

### 17.3 t3 — `crates/extract` (extract-dev)

Every file is in §7.1. **No file outside `crates/extract/**` except the one line in the root
`Cargo.toml`'s `[workspace.dependencies]`.** This task is therefore fully parallel with t2.

### 17.4 t4 — extractor seam + KB→graph ingestion (knowledge-dev)

| file | change |
| --- | --- |
| `crates/daemon/src/extract_plane.rs` | NEW: §10.1 (`ExtractBundle`, `ExtractSource`, `ExtractCtx`, `ExtractError`, `extract`, `extract_rules`, `extract_acp`, `ExtractBundle::dedup`) |
| `crates/daemon/src/distill.rs` | ACP extraction mapped onto the seam; `write_memories`/`write_graph` take candidate types; `DistillOutcome` gains `source`/`truncated`/`dry_run` |
| `crates/daemon/src/knowledge_graph.rs` | NEW: §13.4 (`ingest_document`, `sweep`, `IngestReport`, `IngestOptions`) + the ledger SQL + the two HTTP handlers |
| `crates/daemon/src/api.rs` | 3 route lines (2 new + the distill body) |
| `crates/daemon/src/lib.rs` | `pub mod extract_plane; pub mod knowledge_graph;` (module lines only) |
| `crates/daemon/Cargo.toml` | `ruagent-extract.workspace = true` |
| `crates/knowledge/src/store.rs` | `KnowledgeDocument.content_hash` + the SELECT in `list_documents` |
| `crates/store/src/migrations/0026_capability_ingest.sql` | NEW (§13.3) |
| `crates/store/src/migrations.rs` | one `include_str!` line in `MIGRATIONS` |
| `crates/daemon/tests/graph_ingest.rs` | NEW: off-by-default, ledger idempotence, `max_docs_per_pass`, `dry_run`, entity/relation counts, "an unchanged document is never re-extracted" |

Shared files: `crates/knowledge/src/store.rs` (t4, t5 — sequential), `api.rs` (t2, t4, t5),
`lib.rs` (t2, t4, t8). Owns `crates/store/**` exclusively.

### 17.5 t5 — per-leg recall (recall-dev)

| file | change |
| --- | --- |
| `crates/daemon/src/memembed.rs` | `MemoryLegs` + `recall_memories_with`; `recall_memories` delegates (§11.1) |
| `crates/knowledge/src/store.rs` | `LegConfig` + `compute_legs_with`/`fuse_with`/`search_page_with`; the old three delegate (§11.1) |
| `crates/daemon/src/api.rs` | the `recall` handler: read the plane, build both configs, add `legs_disabled`, skip disabled legs per §11.3, add the `disabled_legs` key to `top_legs_json` |
| `crates/daemon/tests/knowledge_api.rs` | the equivalence test and the leg-off tests (§11.5) |
| `crates/knowledge/tests/retrieval-legs.rs` | leg-off unit cases + `fuse == fuse_with(default)` |

Shared files: `store.rs` (t4, t5 — sequential), `api.rs` (t2, t4, t5).

### 17.6 t6 — MCP + operator skill (mcp-dev)

| file | change |
| --- | --- |
| `crates/mcp/src/lib.rs` | 4 `#[tool]` fns + 4 param structs + the `instructions` sentence (§14) |
| `crates/mcp/tests/roundtrip.rs` | `expected` gains 4 names, count 14 → 18 (roundtrip.rs:132-152), one call test per tool |
| `skills/ruagent-operator/SKILL.md` | the "Platform capabilities" section + the two edits of §15 |

Shared files: none (this is why t6 can be its own wave).

### 17.7 t7 — panel capability card (panel-dev)

| file | change |
| --- | --- |
| `panel/src/api.ts` | `CapabilityRow`/`CapabilitiesResponse` types (next to `DistillPolicy` at api.ts:270-277), `api.capabilities()`, `api.setCapabilities()` (next to api.ts:802-810) |
| `panel/src/views/Settings.tsx` | a `CapabilitiesSettings` card below `DistillSettings` (Settings.tsx:22, card at 30-263): a switch per capability, a tier badge, a `free`/`llm` tooltip, a cost warning before enabling an llm row, and the `conflicts` banner |
| `panel/src/i18n/settings.ts` | zh + en keys under `settings.capabilities.*` (both languages; the file's header says so, settings.ts:1-6) |

Verification for t7 is `cd panel && npm run build` (the wrapper runs the i18n check + both
type checks before vite; AGENTS.md). No e2e spec is added (§3.8).

### 17.8 t8 — integration (platform-dev)

**Half of this row is already done by t4 (§21 item 6).** The KB→graph sweep and the
`ruagent-extract` dependency landed with t4: `crates/daemon/src/lib.rs:252-273` already calls
`knowledge_graph::sweep_if_enabled` **inside the existing 60 s knowledge-scan loop**, reading
the live plane on every pass through `chats.capabilities_handle()` (`lib.rs:258`) — no new job,
no new timer — and `crates/daemon/Cargo.toml` already depends on `ruagent-extract`.

What is **still missing after t4/t2** and is therefore t8's real job (verified by grepping the
tree: the only `set_capabilities` callers today are the PUT handler at `capability.rs:775` and
the tests at `crates/daemon/tests/capabilities.rs:66` / `graph_ingest.rs:626,701,755`):

| file | remaining change |
| --- | --- |
| `crates/daemon/src/lib.rs` | **the live-plane install at boot** — `chats.set_capabilities(config.capabilities.clone())`, so a `[capabilities]` table in `policy.toml` takes effect on a restart. Today it is parsed and validated (`DaemonConfig.capabilities`, config.rs:20-26, 74-96) but never applied to the live manager: after a restart the plane would be legacy and the file would be ignored. Everything else in this file is done. |
| `crates/daemon/src/runs.rs` | `set_capabilities` on the RunManager (`capabilities_handle()`, mirroring `set_knowledge`, runs.rs:299-309) + the injection gate at runs.rs:771 |
| `crates/daemon/src/chat.rs` | the auto-distill gate in `maybe_auto_distill`/`auto_distiller` (chat.rs:1052-1070, 1479-1493) and the injection gate in `send_prompt` (chat.rs:169-203) |
| `crates/daemon/tests/capability_defaults.rs` | NEW: the exhaustive L1/L2/L4 test (§18) |
| `crates/daemon/tests/capabilities.rs` | extend t2's file with the live-swap + restart-install tests |

Shared files: `lib.rs`, `chat.rs`, `tests/capabilities.rs` (all t2-owned first).

### 17.9 The shared-file matrix (sequential execution required)

| file | tasks | rule |
| --- | --- | --- |
| `crates/daemon/src/api.rs` | t2, t4, t5 (+ t8) | **single writer at a time**, in that order; after each, `cargo check -p ruagent-daemon` |
| `crates/daemon/src/lib.rs` | t2, t4, t8 | sequential, same order |
| `crates/daemon/src/chat.rs` | t2, t8 | sequential |
| `crates/knowledge/src/store.rs` | t4, t5 | sequential (different regions, same file) |
| `crates/daemon/tests/capabilities.rs` | t2, t8 | sequential |
| `Cargo.toml` (root) | t3 | t3 only |
| `crates/daemon/Cargo.toml` | t4 | t4 only |
| `crates/mcp/tests/roundtrip.rs` | t6 | t6 only (no `AppState` change, §4.5) |

---

## 18. Verification plan (t9) — what "default-identical" is checked by

| check | where | what it proves |
| --- | --- | --- |
| Registry conformance | `crates/daemon/tests/capability_defaults.rs` | one row per `CapabilityId::ALL`; ids are unique and non-empty; `tier == Llm ⇒ !default_enabled`; `new_in_this_increment ⇒ !default_enabled`; `!new_in_this_increment && tier == Free ⇒ default_enabled` (L4) |
| Exhaustive pass-through | same file | for every id and both legacy values, `CapabilityPlane::legacy().gate(id, legacy) == legacy`; `options(id) == spec(id).defaults` (L1/L2) |
| Fresh-root default | `crates/daemon/tests/capabilities.rs` | `DaemonConfig::load` on an empty root yields `table_present() == false`, and `DEFAULT_POLICY_TOML` still parses (mirroring `config_load_creates_defaults`, config.rs:736-745) |
| Hard errors | same file | unknown id in the file ⇒ boot fails with a message containing the id and the known ids; unknown key ⇒ serde error naming id + key; `max_per_input = 0` ⇒ the range message (L3) |
| Recall equivalence | `crates/daemon/tests/knowledge_api.rs` | absent table vs explicit all-defaults table: identical load-bearing fields (§11.5.2) |
| Legs never queried | `knowledge_api.rs` + `crates/knowledge/tests/retrieval-legs.rs` | counting `Embedder`; per-row assertions of §11.3 |
| llm inertness | `crates/daemon/tests/capability_defaults.rs` + the `distill_log` assertion of §10.3 | with the default configuration, closing a chat adds **no** `distill_log` row and spawns no agent |
| Ingestion off by default | `crates/daemon/tests/graph_ingest.rs` | with `knowledge_ingest_graph` off, ingesting a document adds **zero** rows to `entities`/`entity_edges`/`knowledge_graph_ingest` |
| Ingestion idempotence | same file | run the sweep twice on the same document: `entities` count unchanged, second run reports `ledger_hits == 1`, `documents == 0` |
| Bounded catch-up | same file | 30 documents + `max_docs_per_pass = 20` ⇒ first sweep ingests 20, second 10 |
| Extractor purity | `crates/extract/tests/bounded.rs` | no `std::fs`/`std::net`/`std::env`/`std::process`/`std::time` path in the crate's sources; two runs byte-identical; caps respected; a > `max_content_bytes` unit is dropped, not truncated |
| MCP surface | `crates/mcp/tests/roundtrip.rs` | set equality with 18 names; one call per new tool; a refusal is an error naming the id |
| Gates | AGENTS.md | `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`; plus `cd panel && npm run build` after t7 |

Live probe for t9 (not a substitute for the tests): start the daemon with
`scripts/ruagent-daemon.ps1 start`, then `GET /api/v1/capabilities` (must show
`table_present: false` on a fresh home), `PUT` one free capability on, re-`GET` (must show
`configured: "file"`), `PUT` an empty map (must return to `table_present: false`), and read
`~/.ruagent/config/policy.toml` to confirm the comments and `[distill]` survived.

---

## 19. Risks and how they are contained

| risk | containment |
| --- | --- |
| An implementer "fixes" a frozen signature (`Store::search_page`, `memembed::recall_memories`, `rrf`) | §11.1's additive-twin table is a rule, with the repo's own precedent (rrf.rs:20-26). A reviewer rejects a changed signature. |
| The `[capabilities]` block accidentally ships uncommented, silently switching llm work off | L1's fresh-root test asserts `table_present() == false`; the config comment says it three times. |
| `api.rs` conflicted between three writers | §17.9: single writer, in order, `cargo check` after each; each task's api.rs diff is route lines + one handler body, with the body living in the task's own module. |
| The extractor's tokenizer drifts from `ruagent_store::fts` | §7.2: the duplication is one module, documented, and the extractor's needs (sentences, markers, identifiers) do not overlap the FTS query builder's. Hoisting a shared pure `text` crate is a named follow-up if the two ever need the same behaviour. |
| A rule table quietly grows into a heuristic pile | Each rule needs a gold case (§8.6) and a marker set in `rules.rs`; §8.7's guards (content token, no question, byte cap) are the precision floor. |
| The ingest ledger and the graph disagree (rows written, ledger missing) | The ledger row is written in the same `async fn` after a successful `apply_extraction`; a failure leaves no row, so the next sweep retries — the same "attempt is visible, failure is not a success" discipline as `distill_log` (distill.rs:303-325, 458-493). |
| The store's writer thread outlives the last `Db` handle, so the next opener races its `wal_checkpoint` (measured: `SQLITE_BUSY` on a damaged-ledger reopen once migration 26 made that reopen a write) | `crates/store/src/sqlite.rs` closes the channel and JOINS the writer thread on the last handle's drop (skipping the join on the writer thread itself), so a second opener cannot start before the file is released. Still one writer connection, no retry loop. Pinned by `sqlite::tests::a_reopen_after_the_last_handle_drops_never_races_the_writer`. |
| Someone turns the KB→graph capability on with a big tree | `max_docs_per_pass` bounds the catch-up; `dry_run` prices it first; the status route reports the backlog. |

---

## 20. Open items this document deliberately does not decide

1. Whether the injection paths should eventually take per-leg config (§3.3) — a follow-up
   task with its own measurement, not a widening of this one.
2. ~~Whether `recall_leg_*` weights should be *shared* with the knowledge fusion's calibration
   version (`SCORING_VERSION`, store.rs:338) — a weight change today is not recorded in
   `recall_log`, so a reader cannot tell a different weight from a different corpus.~~ The
   second half is **CLOSED (increment 3)**: the memory half landed as the additive
   `memory_fusion` key in the response and in `candidates_json`, carrying the EFFECTIVE weights
   (design §11.4), and a disabled leg reads `0` rather than its stale configured weight. The
   original recommendation — *"the honest next step is to fold the leg weights into `fusion`'s
   label (the label is built at `api.rs:2974-2982`)"* — is **superseded, not implemented**:
   `scoring.fusion` is an existing wire value, so folding a memory weight into it would have
   been a non-additive change, which is why a NEW key was added instead and why the new key
   follows the response's spelling rather than the knowledge TYPE's (`FusionKind::label()`,
   `store.rs:151`, still a different string and still not emitted). Still open: **sharing** a
   calibration version between the two fusions (the `SCORING_VERSION` half). See §11.5 for the
   declared-option surface added on top of this.
3. Whether skills need a lifecycle (`skills.rs` copies files; §2.1 claim 6). Out of this
   increment's scope; the operator-skill edit is documentation only.

---

## 21. Deviations landed against this document

The implementation deliberately diverged from this document in the places below. In every case
**the code is right and this document was wrong** — grounded in the real schema or in law L1 —
and the sections named above have been corrected in place. Where a promise WAS met, it is not
softened here; only the divergences are listed.

1. **§13.1/§13.3 — the ledger key is the document NAME, not `(document_id, content_hash)`.**
   Reason: `Knowledge::index_doc` DELETES the documents row and INSERTs a new one on every
   content change (`crates/knowledge/src/store.rs:652-654`, `672-683`), so `documents.id` is
   reassigned on every revision and cannot anchor "the same document, new revision" — the key
   the revision rule must hang on. `documents.name` is stable: it is `index_doc`'s upsert key
   (`store.rs:638`) and the argument of `Knowledge::read_raw` (`files.rs:239`). The table is
   `document_name TEXT NOT NULL UNIQUE` with `document_id` kept as an informational column
   (`0026_capability_ingest.sql:42-63`), the upsert is `ON CONFLICT(document_name) DO UPDATE`
   (`knowledge_graph.rs:660-667`), and the ledger also carries `entities_json`/`facts_json` —
   what that revision wrote — which the design had not specified and without which the revision
   rule is not implementable (§13.3; `knowledge_graph.rs:32-65`). **The §13.3 key as written was
   unimplementable.**

2. **§11.4 — `disabled_legs` landed in `candidates_json`, NOT in `top_legs_json`.**
   Reason: `top_legs_json` is a JSON **ARRAY** (one entry per ranked hit, built at
   `api.rs:3318-3337`), so adding a key there changes the shape every existing reader sees —
   a default-configuration wire change, which law L1 forbids. `candidates_json` is an object
   that already grows with the fusion (`candidates`, `leg_window`, `fusion`, `ranked_page`),
   and the key landed there with that reason written next to it (`api.rs:3343-3355`). The
   response's `legs_disabled` **is exactly as specified** (`api.rs:3462`).
   Corrected in §11.4.

3. **§14.2 — the error-body shape is the daemon's plain-text `ApiError`, not a
   `{"error": …}` JSON envelope.** The capability endpoints answer with
   `ApiError::{bad_request, conflict}` like every other route (the illustrative JSON body in
   §14.2 was never implemented); mcp-dev's client reads **both** shapes, with the plain-text
   fallback documented as the common one (`crates/mcp/src/lib.rs:1862-1877`).
   Corrected in §14.2.

4. **The module file is `crates/daemon/src/capability.rs` — SINGULAR.** This document already
   said singular everywhere (§4.1, §17.2, §16); the record is here because an amendment
   message had briefly named it `capabilities.rs`. `crates/daemon/tests/capabilities.rs`
   (plural) is the **integration test** file and is a different file — do not merge or rename
   either one. Corrected in §5.3.

5. **§4.4/§5.3/§14.1/§14.2 — the PUT path and the cost gate, as landed.**
   * PUT **reuses the boot door**: it synthesizes `PolicyConfig { capabilities: table,
     ..Default::default() }` and calls `CapabilityPlane::from_policy`
     (`capability.rs:759-763`), so there is exactly ONE validation path in the tree — no second
     validator to drift. The editor still never validates (`config.rs:1063-1114`).
   * The llm cost gate compares **configuration** — `tier == Llm && after.enabled(id) &&
     !before.enabled(id)` (`capability.rs:724-736`) — so enabling `distill_session` requires
     `confirm_cost` **even when `[distill].auto` is false**. Deliberately conservative: the
     capability is the thing being enabled and the legacy flag can be flipped later.
   * `conflicts[]` fires only when the **GATE** is off (`distill_auto && !gate(..)`,
     `capability.rs:520-533`), so **legacy mode reports nothing** — nothing is being narrowed
     there.
   * `configured` is the API's "differs from the registry default" field in the sense the GET
     handler documents: `"file"` ⇔ the file names this id (`capability.rs:486-495`, `713-716`);
     the other values are `"legacy"` (no table) and `"default"` (table present, id absent).

6. **§17.8 — the sweep and the `ruagent-extract` dependency were landed by t4, not left to
   t8.** The sweep rides the EXISTING 60 s knowledge-scan loop (`lib.rs:252-273`) with the live
   plane read on every pass (`chats.capabilities_handle()`, `lib.rs:258`), so there is still no
   new job — as required. **t8's remaining boot duty is only the live-plane install**
   (`chats.set_capabilities(config.capabilities.clone())`) plus `capabilities_handle()` on the
   RunManager. Verified by grep: the only `set_capabilities` callers today are the PUT handler
   (`capability.rs:775`) and tests (`daemon/tests/capabilities.rs:66`,
   `daemon/tests/graph_ingest.rs:626,701,755`) — i.e. **without t8's install, a `[capabilities]`
   table survives a boot only until the next restart.** Stated so it cannot be missed.

7. **Two behaviour narrowings the design did not state (§10.2).**
   * An ACP entity `kind` **outside the prompt's vocabulary** becomes **no kind** (`None`),
     never an invented string: `extract_plane.rs:358-361`, `438-440`, pinned by
     `an_unknown_acp_kind_becomes_no_kind` (`extract_plane.rs:772-785`). For a compliant model
     nothing changes.
   * A **`dry_run` distill writes nothing at all** — no memory, no graph row, no episode, and
     **no `distill_log` row** (`distill.rs:256-277`, counts at `356-367`). The design only said
     "returns what would be written"; the code also had to guarantee the log stays clean,
     because a `run_turn` episode is what the panel reads as "this session was distilled".

8. **§11.1/§11.2 — the id-convergence shape is better than what §11 implied, and is now the
   design (§11.7).** The recall engines name **no capability id**: `memembed` exposes a typed
   `RecallLeg` (six variants, declaration order = report order, `memembed.rs:564-589`) and
   `crates/knowledge` a typed `KnowledgeLeg` (`store.rs:209-222`). The **single** id mapping is
   `crates/daemon/src/api.rs::recall_leg_id(RecallLeg) -> &'static str`
   (`api.rs:2844-2863`), whose strings come from `CapabilityId::as_str()`. `RecallLegConfig::
   disabled_legs` (`memembed.rs:762-787`) is the one place that knows all six legs and maps
   `KnowledgeLeg -> RecallLeg`. Consequently the registry stays the lower layer and
   **`memembed` does not depend on `crate::capability` at all** (doc-comment mentions only).

9. **§11.3 — `query_keyword_stage` for a DISABLED knowledge FTS leg: FOUND as `"empty"`,
   CLOSED by t12 — the promise stands.** t11 verified the landed code for the review round and
   found that §11.3's promise was not kept: `keyword_stage_label` simply debug-formatted the
   producer's own enum (`api.rs:3705-3707`, called at `2983`), and a disabled leg reports
   `KeywordStage::Empty` (`store.rs:999-1003`), so "you turned this off" and "this ran and
   found nothing" were the same wire value. Repairing it was **chosen over documenting the
   loss** — `legs_disabled` (`api.rs:3462`) and `candidates_json.disabled_legs`
   (`api.rs:3343-3355`) do say *which* legs did not run, but a switch that is not visible on the
   leg's OWN field is exactly the confusion §11.3 exists to prevent. The repaired code composes
   the label from the configuration the endpoint holds:
   `keyword_stage_label(stage, keyword_leg_enabled)` (`api.rs:3723`) returns `"disabled"` when
   the leg is off and the stage's own lower-case name otherwise, and the `recall` handler passes
   `legs.knowledge.keyword` (`api.rs:2986`) — the same config it handed to
   `Knowledge::search_page_with`. No variant was added to `KeywordStage`; the knowledge-search
   endpoint, which runs the default configuration, passes `true` (`api.rs:4926`). Both states
   are pinned by `crates/daemon/tests/knowledge_api.rs::a_disabled_keyword_leg_reports_disabled_not_empty`
   (`:1157`), and the comment in `store.rs:985-996` that always asserted this behaviour is now an
   accurate description of the shipped code. The document no longer records a downgrade here.

10. **§14 — the MCP surface as landed (found while verifying; the four tools and their
    argument names/types match this document).** Beyond the four tools: the shipped
    `#[tool(description)]` strings are WHEN/COST **supersets** of the one-liners in §14 (an
    improvement, recorded so a verifier does not read it as a mismatch); `knowledge_graph_ingest`
    sends its arguments in the **query string of a POST** (`crates/mcp/src/lib.rs:439-441`,
    `ingest_query` at 955) against the daemon's `Query<GraphIngestQuery>`
    (`api.rs:4770-4781`); and the **pre-existing** `memory_recall` tool gained a
    `strategy?: "aggressive"|"conservative"` argument next to `conservative`, with the two
    spellings refused when they disagree (`crates/mcp/src/lib.rs:1080-1094`) — a widening of an
    existing tool, not one of this increment's four, and not covered by §14's table.

11. **§19 — the store's writer thread could outlive the last `Db` handle; it now shuts down IN
    ORDER and the single-writer actor is unchanged.** `Db` was a bare channel sender: dropping the
    last handle closed the channel and returned, while the writer thread's
    `wal_checkpoint(TRUNCATE)` — and with it the connection, the file handle and the WAL lock — was
    still running. The daemon's concurrent writers make that window reachable (the KB→graph sweep
    writes through the same actor while chat/run closes, ingest and distillation are in flight), and
    **migration 26 turned a damaged-ledger reopen from a READ into a WRITE**, which is what made the
    window observable: `cargo test --workspace` went red on
    `crates/store/tests/ledger_reopen.rs::a_lost_maximum_version_row_still_boots` with
    `SQLITE_BUSY … database is locked`, green when run alone, red under load. The fix is
    `Arc<DbInner>` + a `Drop` that closes the channel FIRST and then joins the writer
    (`sqlite.rs:38-90`), skipping the join when it runs ON the writer thread so a `Db` held by an op
    closure cannot deadlock. It is a product fix, not a test accommodation: no second connection, no
    retry loop, no sleep, and the checkpoint still happens — the next opener, in this process,
    simply cannot start before the previous one has let the file go. `ledger_reopen.rs` was not
    touched; the shape is pinned by
    `sqlite::tests::a_reopen_after_the_last_handle_drops_never_races_the_writer` (`sqlite.rs:244`),
    and the isolated-worktree control (unfixed `Db` + migration 26) fails at round 1. Corrected in
    §19.

### 21.1 CLOSED by increment 2 (t16): the `search_fts_scored` bm25 defect, repaired

**Status: REPAIRED.** Increment 1 recorded this as out of scope and the top follow-up; increment 2
repaired it as ONE deliberate, measured default-behaviour change. This item therefore does NOT
claim law-L1 equivalence: the default recall result moved, on purpose, and the delta is below.

THE DEFECT, as it stood: `crates/memory/src/query.rs` read the bm25 score with
`r.get::<_, f64>(10)` while its SELECT lists eleven memory columns — `source_episode` is
**index 10**, `bm25(memories_fts)` is **index 11**. Two branches, and only one of them is what real
data does. (Do not conflate this item with §21 item 9, a different defect — the `keyword_stage`
label.)

* **`source_episode` NON-NULL — the production branch.** An earlier version of this document
  claimed the leg never contributed a row and that default memory recall is semantic-only. **That
  was wrong**: the independent verifier measured real rows and `source_episode` is non-NULL there
  (3 rows, 0 nulls), so the read **succeeded**. The leg ran, contributed rows, and reported each
  row's **EPISODE ID as its "score"** — the live wire showed `keyword_score` **2** and **1** while
  index 11 held the real bm25, `-1.55e-06` and `-8.84e-07`. Not a dead leg: a silently wrong number.
* **`source_episode` NULL — the branch the t5 fixture writes.** The read failed with
  `Invalid column type Null at index: 10, name: source_episode`, and the caller swallowed the error
  into an empty keyword leg (`memembed.rs`). This is how the "dead leg / semantic-only" reading
  arose: a fixture whose rows all carry `source_episode: None` exercises only this branch.

THE REPAIR: the score column is **aliased and read BY NAME**
(`bm25(memories_fts) AS bm25` → `r.get::<_, f64>("bm25")`), the way `row_to_memory` already reads
every other column — an index that was wrong once can be wrong again, a name cannot silently point
at the column beside it. No `ORDER BY`, no weight, no RRF constant and no threshold was touched.

THE OPEN QUESTION, ANSWERED BY MEASUREMENT (increment 1 refused to assert it): the wrong score never
reached the ORDER, and the answer is branch-dependent because only one branch produced rows at all.
Measured on the daemon's fixed fixture, before → after:

* **NON-NULL branch (platform rows).** The leg's ID order was already the bm25 order
  (`ORDER BY rank`), and the memory fusion consumes ID lists, so the fused page was **already**
  `[1, 3, 2, 4]` → unchanged. Before: `leg=[(1, 1.0), (3, 3.0)]` (episode ids; index 11 held
  `-0.4331185173528379` and `-0.29290029231055226`). After: `leg=[(1, -0.4331185173528379),
  (3, -0.29290029231055226)]`. **Only the reported `keyword_score` moved.**
* **NULL branch (this fixture).** Before: the read errored, the leg contributed NOTHING
  (`keyword = 0`), and the page was `[1 (0.016393), 2 (0.016129), 3 (0.015873), 4 (0.015625)]`.
  After: the leg contributes rows 1 and 3 (`keyword = 2`, `keyword_new = 0`) and the page is
  `[1 (0.032787), 3 (0.032002), 2 (0.016129), 4 (0.015625)]`. **The page moved**, because rows that
  no leg had found now have a rank — the expected consequence of the leg reporting real relevance.

THE NEW RECALL BASELINE (re-derived, not hand-edited until green): the daemon's fixed fixture is
`[1, 3, 2, 4]` with fused scores `0.032787 / 0.032002 / 0.016129 / 0.015625`, `semantic = 4`,
`keyword = 2`, `keyword_new = 0`, `top_semantic_score = 1.0`, and the leading hit found by BOTH legs
(`["semantic", "keyword"]`). The old pin was `[1, 2, 3, 4]` with `0.016393 / 0.016129 / 0.015873 /
0.015625` and `keyword = 0`. The strategies test moved with it (aggressive `[1, 3, 2, 4]`,
conservative `[1, 3, 2]`; the cosine floors themselves are unchanged — they are the semantic leg's
own gate).

THE LEG IS NOW OBSERVABLE, which it was not: t5 recorded that the memory-FTS leg's on/off state was
indistinguishable from outside (the leg returned nothing either way). With the read repaired, the
two states differ — semantic-leg-off now answers with the keyword leg's own bm25 order
(`[1, 3]`, `keyword = 2`, `keyword_new = 2`, no row carrying semantic evidence) — and that is what
the re-derived pin asserts instead of the old unobservability.

THE TRIPWIRE BECAME A POSITIVE TEST: t13's
`characterizes_defect_both_branches_of_the_source_episode_index_bug` asserted the BROKEN behaviour on
both branches, so this repair turned it red **by design**. It is replaced by
`crates/daemon/src/memembed.rs::the_source_episode_index_repair_reports_bm25_for_both_shapes`
(both branches, corrected; the defect and its fix are named in the test's doc comment so the history
stays readable), backed by `crates/memory/tests/search_fts_scored.rs`, which pins the read from
outside the crate — a NULL row, a non-NULL row, the mixed case (one NULL row used to fail the whole
statement), and the invariant that the scored and score-less legs agree on rows and order.
