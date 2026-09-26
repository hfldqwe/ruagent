# t294 — t252 acceptance item 3: a NEW wiki dry-run (round 3)

Verifier: member `tools`, attempt 1.

> ## CORRECTION (appended after the captain's refutation)
>
> **G1 is WITHDRAWN. It was produced by the request shape I was given, not by a
> defect in t252.** The task description said to dry-run with
> `?dry_run=true`; the handler reads `Json(req).dry_run` — **the body** — and
> **silently ignores the query string**. I measured the query-string shape
> faithfully and reported what it did; the conclusion I drew from it was wrong.
>
> The captain's controlled A/B on a canary (temp root, one preloaded document):
>
> ```
> A  ?dry_run=true  + body {}   -> 202 {build_id:1, status:"running", pages_planned:0}
> B  body {"dry_run":true}      -> 200 {build_id:2, status:"planned", pages_planned:1, plan:[…]}
> wiki_builds: [[1, 0, "running", null], [2, 1, "planned_only", "2026-09-26T14:11:19…"]]
> ```
>
> **B produces exactly what t252 claims**: `dry_run=1`, status `planned_only`
> (outside the four old values), `finished_at` set. **A produces exactly the row
> I reported.** So **acceptance item 3 PASSES**, with B's row as the evidence,
> and the "status criterion FAILED" line below is void.
>
> I re-ran the body shape against the **live** database and got
> `400 scope selects no source documents` — a legitimate refusal, and a second
> correction to my own "minimal shape": the body must also carry the scope
> (row 1's `scope` is `all`). I did not guess further shapes on the live
> database, because guessing write-endpoint shapes is not verification.

## The corrected answer to "minimal request shape"

```
POST /api/v1/knowledge/wiki/build
Content-Type: application/json
{"dry_run": true, "scope": "all"}
```

* The dry-run flag is **body-only**. `?dry_run=true` is **accepted and ignored**
  — the request still returns 202 and starts a real build.
* The scope is required in the body; without it the endpoint answers
  `400 scope selects no source documents`.

## The two findings, restated after the refutation

| id | severity | status | problem |
| --- | --- | --- | --- |
| G1 | high | **WITHDRAWN** | refuted by the captain's A/B; the defect was in the request shape I was given, not in t252 |
| G2 | low | **WITHDRAWN** | it depended on G1 |

## What survives as a real finding

**An ignored parameter is worse than a rejected one.** `?dry_run=true` is
silently accepted: the request returns **202**, the response body even says
`pages_planned: 0` (the dry-run behaviour really happened), and the stored row
is `dry_run=0` with `status='done'` — indistinguishable from a real build. A
caller who follows the documentation's query-parameter spelling gets a
successful status code and a row that looks like production. Whether that is
worth a task is the captain's call; it is recorded here because it is the
reading, not the inference.

| id | severity | problem | requiredFix |
| --- | --- | --- | --- |
| H1 | low | `?dry_run=true` is accepted and silently ignored (202, a real build starts, the stored row is `dry_run=0`/`done`). Only the body flag works. A caller using the query-parameter form cannot tell the dry-run did not happen. | Either read the flag from the query string as well, or reject an unrecognised `dry_run` query parameter instead of ignoring it. |

## Why this correction is here and not in the task record

The task record is terminal and immutable: t294 stands as `failed` with
`needs_revision` and G1. **This document is the artifact of record**, so the
correction lives here, where the next reader will find it. A verifier's wrong
finding that is not visibly withdrawn is worse than no finding.

## What I did NOT do (rule 5)

| not done | reason |
| --- | --- |
| more live-database shape guesses | guessing write-endpoint shapes is not verification; I recorded the 400 and stopped |
| call /api/v1/recall | excluded by the task |
| start or stop the daemon | excluded by the task |
| touch crates/, cli/, panel/ | out of scope |
