# Unified Memory Layer — Design (2026-09-14)

Borrows: Karpathy's simplicity-first methodology (nanochat LOG.md is a
masterclass in "measure, then add complexity only when the simple thing
fails"; his documented position that at personal scale plain text +
model-curated notes beat vector-DB ceremony), OpenViking's layered
extraction (AGPL — patterns only), and the user's own two-strategy
recall spec.

## The layer model

```
L0  episodic   session transcripts   verbatim, BOTH sides, zero judgment
               (cheap to keep, never filtered — the raw ground truth)
L1  semantic   distilled memories    judged at distill time (see below)
L2  structural knowledge graph      entities + relations, supersession
L3  digest     injected context      compact, per-session, nanochat-style
```

## Answer to "怎么判断 agent 的回答是否符合要求"

**无脑先记录，蒸馏时判断 — 这就是正确架构** (user's own suggestion):

- L0 records everything: user AND agent messages, verbatim. Storage is
  cheap (KBs); judgment is expensive and context-dependent. Deferring
  judgment loses nothing and gains the full conversation as evidence.
- Judgment happens at L1 (distill), where the extractor sees the WHOLE
  conversation — including outcome signals that are invisible turn-by-turn:
  1. **User correction**: agent answer → user replies "不对/应该是/no,
     actually…" → the corrected fact is extracted from the USER's message;
     the agent's wrong answer is NOT distilled.
  2. **Acceptance**: answer followed by the user moving on / positive
     wording → normal confidence.
  3. **Explicit confirmation** ("对，就是这样") → high confidence.
  4. **Agent hedging** ("可能/不确定/I think") → lower confidence, or skip.
- The extraction prompt now instructs exactly this, and extracted
  memories carry a confidence value (0.5–1.0) fed into the store's
  existing confidence column.

This is the episodic/semantic split from cognitive memory research:
episodes are lossless, semantics are curated — and curation can be
re-run (re-distill) when the algorithm improves, because L0 is never
destroyed.

## Token economy (the user's "既不浪费 token，又跨会话记忆")

| Layer | Cost | When paid |
|---|---|---|
| L0 transcripts | disk only | never enters context |
| L1 memories | — | only via recall (score-gated) |
| L2 graph | — | only via recall (stubs in conservative mode) |
| L3 digest | ~0.5–1.5k tokens | once per session start |

L3 digest (this increment): chat sessions start with a compact memory
block — profile facts + recent durable observations — injected as a
separate ContextInjected event (visible in the transcript, stripped
from distillation input → no feedback loop). This is the cross-session
memory the user asked for: a fresh chat "remembers" the user without
any recall call.

## Recall (two strategies, refined)

- aggressive: full content, score-thresholded — semantic memories
  (embedding cosine, this increment) + knowledge hybrid + graph
  entities with summaries
- conservative: stubs only — entity names + relation one-liners +
  memory titles; the agent pulls details via memory_get/graph_entity
  on demand. Graph-guided: entity hit → its currently-valid facts
  (neighborhood), not the whole graph.
