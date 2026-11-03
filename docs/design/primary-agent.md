# The primary agent

**Goal.** One agent the user talks to. It answers what it can, routes the rest to the other agents, and
operates the platform on the user's behalf (choose the conversational default, enable/disable agents, point
routing at one of them). Today the user must pick an agent in the panel before typing, and no tool can
change that pick.

This document is the contract the backend, the MCP tools and the panel are built against.

## What exists (facts, not assumptions)

* `agents.toml` has two layers: `[runtime.X]` (how an engine is spawned) and `[agent.Y]` (a portable role:
  prompt + preferred runtimes). `crates/daemon/src/config.rs:117`.
* `AgentCard` (`crates/core/src/agent.rs:56`) is the registry view: id, name, harness, model, `models`, `prompt`,
  `description`, `mcp_profile`, ... It has **no** notion of a default or primary.
* `default_agent()` was **deleted** (`crates/daemon/src/config.rs:105`) because the config-file view mints a
  fresh `AgentId` on every load, so recording its id produced a run whose `routed` event named an agent that
  existed in no registry and no table. The replacement is `api.rs::default_route_agent` -- "first enabled card
  of the REGISTRY, excluding the permission approver".
* Routing cascade (`crates/orchestrator/src/lib.rs:182`): explicit pin > first matching rule > `routing.toml`
  `default`. Only the last-resort fallback consults the approver guard, because a `routing.toml` `default` is
  the operator's deliberate choice (`api.rs:2673`).
* A default-level decision must SAY which agent it chose and why (`api.rs:2690`, t41): `rationale` is required
  on that path.
* Agent CRUD already exists over HTTP: `POST/PUT/DELETE /api/v1/agents`, `POST /api/v1/runtimes`
  (`api.rs:35-41`), writing through `registry().update_agent(...)`.
* **No** write path for `routing.toml`, and **no** MCP tool touching agents or routing at all: the 21 tools are
  memory_*, knowledge_*, graph_*, wiki_*, capabilities_*, distill_*, list_tasks.

## Decision

A **primary agent** is a role agent marked `primary = true` in `agents.toml`. Exactly one may exist.

It is `primary` and not `default` on purpose. `default` already means "the last-resort fallback" in the routing
cascade, and t43's lesson was that an implicit default records an id nobody can resolve. The primary is
explicit, resolvable by NAME, and always sayable: every decision that reaches it carries a rationale naming it.

### Resolution order (one list, in this order)

```
1. task.pinned_agent            explicit pin on the task
2. first matching rule          routing.toml [rule]
3. routing.toml default         the operator's named fallback
4. THE PRIMARY AGENT            the front door this document introduces
5. first enabled card           today's behaviour, approver excluded (unchanged)
```

Steps 1-3 and 5 are unchanged. Step 4 is new and sits directly above the old fallback, so a config that
predates this document behaves exactly as before.

### One answer to "who will answer me"

Steps 3 and 4 can disagree, and a shipped install already has step 3 set: `DEFAULT_ROUTING_TOML`
(`crates/daemon/src/config.rs:1173`) seeds `default = "claude"`. So a user who marks a different agent
primary would see nothing change, because the routing default still wins. That is the shape of feature that
looks broken while working as designed.

Therefore the platform exposes exactly ONE answer, and everything the user can see reads that answer: the
**effective front door** = step 3 if `routing.toml` names an enabled agent, else step 4, else nothing.
`GET /api/v1/primary` returns the effective front door plus a `why` that names its source, and when both
exist and differ it says so in as many words:

```
routing.toml default `claude` overrides the primary flag `builder` -- edit routing.toml, or use
agent_set_primary, which writes both
```

When the two agree, `source` names the cascade step that actually produced the answer (`routing-default`)
and `why` says the flag names the same agent -- agreement is worth stating, because it is what a fresh
install has.

Anything that sets the front door writes BOTH places, so they cannot drift by accident: `agent_set_primary`
(the tool) marks the flag and sets `routing.toml`'s `default` in the same operation, and its reply reports
both writes. A deliberate hand edit of `routing.toml` remains the operator's override -- it just cannot be
mistaken for the flag.

## Failure modes (each must be sayable, not silent)

* **Two agents marked primary** -- `agents.toml` load fails, naming both. A tie is a configuration error;
  picking one by file order would be the t43 mistake again.
* **Primary is disabled or missing from the registry** -- WARN at load, and the resolution falls through to
  step 5 with a rationale that names the primary it could not use and why. Never silently.
* **Primary is the permission approver** -- allowed (the approver is a legitimate role) but the routing
  rationale says so, because a run whose agent is the approver is an unusual shape worth seeing.

## API contract

* `GET /api/v1/agents` rows gain `primary: bool`.
* `GET /api/v1/primary` -> the EFFECTIVE front door: `{"name": ..., "id": ..., "source": "routing-default"|"primary-flag", "why": ...}`
  or `{"name": null, "why": "..."}`. `why` names the source, and names the disagreement when the other
  source holds a different agent.
  The `why` is not decoration: it is what the panel shows when there is no primary, and what an agent tool
  reads back after setting one.
* `PUT /api/v1/primary` `{"name": "<agent>" | "none"}` -- **the operation clients should call.** It writes
  BOTH places in one step (the flag and `routing.toml`'s `default`) and answers with the resulting effective
  front door. Keeping the two-write rule inside the daemon is the point: if each client had to remember it,
  one of them would eventually write only the flag, and the symptom -- "I set the primary and nothing
  changed" -- is the hardest kind to diagnose.
* `PUT /api/v1/agents/{name}` accepts `primary: true` as the low-level primitive (flag only). Setting it
  clears the flag from every other agent in the same write, and clearing it from the current primary is
  allowed (the platform then has none).
* `GET /api/v1/routing` -> the parsed `{default, routes}` view, as the cascade reads it.
* `PUT /api/v1/routing` is a **patch, not a replacement** (t58). An **omitted** key keeps whatever is
  there, so `{"default": "dsh"}` changes the default and leaves `[[routes]]` untouched -- the
  replacement semantics made every client responsible for content it cannot see, and the first tool
  author hit exactly that (a default-only write silently emptied the rules, so the tool had to GET
  first and echo the rules back; a rule duplicated into every client is a rule that will one day be
  half-applied). An **explicit empty** value is a statement that means clear: `"routes": []` empties
  the rules, `"default": ""` (or `null`) removes the default. Omitted and empty are therefore
  distinguishable, which is the whole point -- `routes: []` must not be readable as "no opinion".
  An unknown key is a 400 (a typo must not look like a successful write). The reply is the resulting
  `{default, routes}` view plus `takes_effect`, and it applies to the next run.

## MCP tools (v1)

The primary can only operate the platform if it has hands. Four tools, deliberately few:

| tool | kind | what it does |
| --- | --- | --- |
| `agents_list` | read | every agent: name, id, kind, enabled, `primary`, model, description |
| `agent_set_primary` | write | mark one agent primary (atomic swap), or clear with `none` |
| `routing_show` | read | the `default` and the rules, as the cascade sees them |
| `routing_set_default` | write | set/clear `routing.toml`'s `default` |

Rules these tools follow, matching `capability_set`'s precedent: the reply is the full post-change state
(so the caller reads one answer instead of calling again), a refusal explains WHY, and anything that spends
the user's tokens or changes who answers them states that out loud before the write.

## Panel

* A new chat starts on the primary, with no picker step. `?agent=` still overrides, and an existing session
  keeps the agent it was created with -- switching what an open conversation is talking to would rewrite
  history under the user.
* When there is no primary, the picker appears exactly as it does today, and says what to set to remove the
  step.

## Non-goals for this version

Creating agents, editing prompts and setting models per agent from a tool; a natural-language router tier;
multi-agent conversations. The four tools above are the foundation those need -- each of them is a thin
wrapper over an endpoint that already exists or is created here.
## Measurements (what was actually observed, not designed)

Run on 2026-11-01 against a private root and a privately built binary, so the resident daemon's own config was
never touched.

**A fresh install already has a front door.** `GET /api/v1/primary` before any edit:

```
{"name":"claude","id":"01a19668-...","source":"routing-default",
 "why":"routing.toml's `default` and the `primary = true` flag name the SAME agent (`claude`), so both
  carriers agree and level 3 decides"}
```

**One write moves both carriers.** `PUT /api/v1/primary {"name":"opencode"}` returned the opencode card with
the same shape, and on disk `agents.toml` carried `primary = true` while `routing.toml` carried
`default = "opencode"`. An unknown name answered `400` and left both files unchanged.

**Level 4 routes real runs, and it is provably level 4.** With `routing.toml` given no `default` and two
enabled agents -- `aaa-first-enabled` (which the level-5 fallback would have taken, alphabetically first) and
`mock` (marked primary) -- a run started with **no agent named** got:

```
params.agent = 01a19669-c1b5-74ba-b0f8-90268460f249     # the id /api/v1/primary reported for `mock`
rationale     = PRIMARY agent `mock` (agents.toml marks it `primary = true`)
source.level  = default
```

The id in the run is the registry's own (the t43 rule), so a run and the API cannot disagree about which
agent answered. `aaa-first-enabled` appears nowhere in the transcript, which is what makes this level 4 rather
than level 5 -- a unit test on the pure function could not have told the two apart.

**A defect found this way, not by review:** `RoutingEditor` replaced the `default` item together with its
decor, so the first `PUT` dropped `routing.toml`'s header comment block (the file began with
`default = "opencode"` instead of `# ruagent routing rules ...`). Filed as its own repair rather than fixed on
the way past, because the design says the comments survive and a green suite would otherwise have implied they
did.