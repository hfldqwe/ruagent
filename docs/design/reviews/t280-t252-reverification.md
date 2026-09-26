# t280 — re-verification of t252 on the REBUILT binary

Verifier: member `tools`, attempt 1. Supersedes the blocked review at
`t257-t252-independent-verification.md` (whose blocker — the change was not in
any executable — the switch removed).

Switch under test (from t279): exe md5 `3763f910…` → `4b66312a…`,
`schema_migrations` 17 → 18.

## Verdict: needs_revision

Three of t252's four claims verify. The fourth is not reproducible, and the
probe-hygiene claim is only half true — the doctor says so itself, in a message
that t276 has since made stale.

## 1. Ingest closure — VERIFIED

Object set: the knowledge corpus in `~/.ruagent/data/ruagent.db`.
Sampling plane: a probe-prefixed temp corpus at `C:/tmp/t280corpus`
(`__probe__/t280/alpha.md` 56 B, `__probe__/t280/nested/beta.md` 50 B) —
the user's corpus is never written by this review.
Falsifiable criterion: if a probe document survives the DELETE below, or the
counts do not return to their before values, this reading is wrong.

```
$ ruagent knowledge --help        -> lists "ingest"  (acceptance item 1a)
$ ruagent knowledge ingest /c/tmp/t280corpus --prefix __probe__/t280
  ok    __probe__/t280/alpha          56 B -> 1 chunks
  ok    __probe__/t280/nested/beta    50 B -> 1 chunks
  walked 2 md file(s) · ingested 2 file(s) · bytes 106 · chunks 2
  documents  4 -> 6
  chunk rows 11 -> 13

DELETE /api/v1/knowledge/documents/19 -> 204
DELETE /api/v1/knowledge/documents/20 -> 204
after cleanup: documents=4 chunks=11   <- returned
```

The CLI prints bytes, chunks, and a before/after total, exactly as claimed.
**Verified.**

## 2. Doctor twice: production +0 — VERIFIED

Object set: five tables; "production" excludes the probe prefixes
(`__probe__%` documents/entities, `agent:__probe__%` memories).
Sampling plane: read-only SQLite (`file:…?mode=ro`).

```
                     all                              production
before doctor 1:  doc=4 chunks=11 ent=65 mem=165 wb=5   doc=4 chunks=11 ent=64 mem=163
after  doctor 1:  doc=4 chunks=11 ent=65 mem=165 wb=5   doc=4 chunks=11 ent=64 mem=163
after  doctor 2:  doc=4 chunks=11 ent=65 mem=165 wb=5   doc=4 chunks=11 ent=64 mem=163
```

**Production +0 across both runs. Verified.** The probe document the doctor
creates is removed inside the same run — its own report line says so:
`removed probe document __probe__/doctor-probe (#19)`.

## 3. The two residue rows — ANSWERED: still there, and --cleanup does not remove them

```
before --cleanup:  entities=1  memories=2
  entity (65, '__probe__doctor-node')
  memory (164, 'agent:__probe__',      'doctor probe: the kettle is chrome')
  memory (165, 'agent:__probe__t263',  't263 probe row (delete round-trip)')
after  --cleanup:  entities=1  memories=2      <- identical
```

**Answer: after a plain doctor run they are UNCHANGED (still present), and
`--cleanup` does not delete them either.** The doctor states the reason itself:

```
[PASS] probe hygiene   removed probe document __probe__/doctor-probe (#19);
                       isolated (no delete route yet): entity __probe__doctor-node, memory agent:__probe__
```

**That message is now stale** — see finding F1.

Also note the memory residue is **not** all doctor's: row 165 is
`agent:__probe__t263`, a probe row left by a different task. t257 measured
`memories=1`; it is 2 now, and the increase is that row.

## 4. A NEW wiki dry-run — NOT REPRODUCIBLE

```
POST /api/v1/wiki/build?dry_run=true   -> 405
wiki_builds unchanged: (5,0,'done',…) (4,1,'planned',None) (3,0,'done',…) (2,0,'done',…)
```

The two `dry_run=1` rows are still the ones t252 did not write, so the
before/after comparison the acceptance asks for cannot be produced from this
endpoint. **Item 3 of t252's acceptance is not independently reproducible as
specified** — see finding F3.

## Findings

| id | severity | problem | requiredFix |
| --- | --- | --- | --- |
| F1 | medium | `doctor --cleanup` leaves the entity and memory probe rows, and the doctor's own report line still says `no delete route yet` — but t276 shipped `delete_entity` and `purge_memory` with their routes. The message contradicts the shipped capability, and the capability is not used. | Wire the probe cleanup to t276's routes; the report line must either delete the rows or stop claiming the route is missing. |
| F2 | low | Probe-space residue is not confined to one task: row 165 is `agent:__probe__t263`, left by t263. t257 measured memories=1; it is 2 now. | Probe-space cleanup must be prefix-wide (`agent:__probe__%`), not doctor-specific. |
| F3 | medium | `POST /api/v1/wiki/build?dry_run=true` returns 405, so t252's acceptance item 3 (a new dry-run's `status`/`finished_at`) cannot be produced as specified. | Publish the dry-run route's actual method/path, or correct the acceptance. |

## What I did NOT do (rule 5)

| not done | reason |
| --- | --- |
| write the user's corpus | only a probe-prefixed temp corpus was ingested, and both documents were deleted; counts verified back at 4/11 |
| produce a wiki dry-run | 405 on the documented shape; no other shape was guessed, because guessing a write endpoint is not verification |
| touch `crates/`, `cli/`, `panel/` | out of scope for this task |
