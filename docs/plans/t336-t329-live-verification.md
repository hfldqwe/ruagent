# t336 — third-level live verification of t329's write-time near-duplicate check

Status: verified on a throwaway daemon (127.0.0.1:8793, temp root), 2026-09-27 00:10-00:20.
Commit of the code under test: e3101ee (t329). Binary: D:/rust_cache/debug/ruagent.exe, mtime 23:50
— AFTER e3101ee (23:38:44) — so the daemon under test carries the change.

## Why this task exists

t329's rule is in the tree and its unit tests are green, but the LIVE evidence was missing: t334 and
t335 both stopped at the seed format. t335 fed source='dsh' with TEXT, and parse_file_messages
dispatches by source (claude-code -> parse_claude_code(text) · dsh -> parse_dsh(bytes) ·
ruagent -> parse_ruagent(text) · anything else -> an EMPTY Vec), so the session parsed to zero
messages and the distill answered "session has no messages to distill" — with zero rows in the DB,
so the criterion was never exercised.

## The seed recipe that works (the one fix over t335)

- transcript <root>/transcripts/seed.jsonl, one JSON object per line:
    {"ts":"2026-09-26T10:00:00Z","seq":1,"event":{"type":"user_message","text":"记住：<MARKER>"}}
    {"ts":"2026-09-26T10:00:01Z","seq":2,"event":{"type":"agent_message_chunk","content":"好的"}}
    {"ts":"2026-09-26T10:00:02Z","seq":3,"event":{"type":"stopped"}}
- sessions row: key='ruagent:t336', source='ruagent' (NOT dsh), ref_path=<that file>,
  started_at/updated_at/mtime_ms/size_bytes/message_count all set (the column list is NOT NULL heavy).
- config/agents.toml: [runtime.mock] command = "<mock exe> --behavior scripted --replies <root>/replies.json"
  and [agent.dsh] runtime = "mock".
- replies.json is a Vec of {marker, reply}; the reply is a STRING holding the inner JSON
  ({"memories":[{store,namespace,content,confidence}],"entities":[],"relations":[]}), and the marker
  is matched by substring against the prompt. Changing the marker in the transcript therefore changes
  what the distiller writes, which is how one session exercises three phrasings.
- Two-phase seeding: start once to create the schema, stop, write the sessions row with NO daemon
  running (so there is never a second writer), start again.

## Readings (read-only SQL on the temp DB)

Distill #1, marker M1_WORDING_A, reply 「用户偏好使用简体中文交流。」:
    {"distilled":{"session_key":"ruagent:t336","memories_written":1,"memories_skipped":0,"agent":"dsh"}}
    ALL rows: 1 · LIVE rows: 1 · row 1 supersedes=NULL, source_episode=1

Distill #2, marker M2_WORDING_B, reply 「用户使用简体中文进行交流。」 (synonym):
    {"distilled":{...,"memories_written":1,"memories_skipped":0,...}}
    ALL rows: 2 · LIVE rows: 1   <- did NOT increase
    row 1: live=0 supersedes=NULL source_episode=1
    row 2: live=1 supersedes=1    source_episode=2   <- the new row points at the old one

Distill #3, marker M3_POLARITY, reply 「用户不使用简体中文。」 (polarity):
    ALL rows: 3 · LIVE rows: 2   <- +1, as the criterion requires
    row 3: live=1 supersedes=NULL source_episode=3

source_episode, BOTH scopes:
    ALL rows with source_episode: 3 / 3
    LIVE rows with source_episode: 2 / 2

## The verify command, and why this pass is NOT the vacuous one

RUAGENT_DB=<temp>/data/ruagent.db node scripts/converge-near-duplicates.mjs --verify  (exit 0):
    live rows: total=2 profile=2
    live rows with source_episode: 2
    === PLAN: 0 duplicate group(s) among live rows ===
    live duplicate groups: 0 (must be 0)
    live profile rows: 2 · superseded rows: 1
    OK: no live row group is a near-duplicate of another.
    chain rows (supersedes / superseded_at): 2
      #1 supersedes=NULL superseded=1 [distilled] 用户偏好使用简体中文交流
      #2 supersedes=1 superseded=0 [distilled] 用户使用简体中文进行交流
    (counter-evidence section also lists the refused polarity pair #2 vs #3, overlap 87.2%,
     verdict=Refused(a polarity token is in the difference))

The SAME script against an empty DB with the same schema, for contrast:
    live duplicate groups: 0 (must be 0)  ·  live profile rows: 0 · superseded rows: 0
    OK: no live row group is a near-duplicate of another.        exit 0
=> identical exit code and the identical "0" line, distinguishable ONLY by the live row count.
   t335's F-t335-03 stands: an empty-set pass is not evidence. This run's evidence is the
   non-vacuous one: live rows total=2, and the chain shows the supersede that t329 produced.

## Two pitfalls that cost time here (both reproducible)

1. A heredoc eats backslash escapes: the exe path written as D:\rust_cache\... arrived as
   D:ust_cachedebuguagent.exe, so cmd.exe reported "not recognized as an internal or external
   command" and the daemon never started — while the log file existed, which looks like a daemon
   failure rather than a launch failure. Build such paths from [char]92 in PowerShell.
2. A kill filter of "command line mentions my root" also matches MY OWN shell (the root is passed
   as an argument to the helper scripts), and the first version therefore killed the bash process
   running the cleanup. Filter by Name as well (ruagent.exe / cmd.exe).

## No residue

- temp daemon stopped by pid, matched on Name+root: killed 22764 cmd.exe + 70052 ruagent.exe.
- 8793: no LISTENING socket (only TIME_WAIT from the just-closed connections).
- no ruagent.exe mentions the temp root any more (remaining matches are this shell and the query).
- temp root deleted (/c/tmp/t336/root gone; only the three helper scripts remain outside the repo).
- the real daemon was never touched: pid file still "42804 1790437830767", /api/v1/health on 8787 = 200.
- no repository file was modified by this task.

## What this does NOT prove

- It does not re-run the rule's own edge cases: it exercises three phrasings through the DISTILL
  path only (the unit tests in crates/memory/src/dedupe.rs own the judgement itself).
- It says nothing about how the rule behaves with a real embedder: this daemon had no embedding
  model, so the semantic leg was absent and the decision came from the character rule alone.
- One session, one store+namespace, three phrasings: it is evidence that the rule is WIRED INTO the
  write path, not a measurement of its precision across the live corpus (that is t323/t325's work).
