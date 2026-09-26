# t257 — independent verification of t252 (ingest closure + doctor probe isolation)

Verifier: member `tools` (attempt 1). Date: session of 2026-09-26.
Scope of this review: `docs/design/reviews/` only. No product file was touched.

## Verdict

**NOT VERIFIED — blocked, and one finding is already positive.**

I could not run either of the two commands the acceptance asks for, and I am
recording why rather than approximating them. **Both blockers are the same
blocker**, and it is a real one:

> The installed binary predates the change under test.

```
D:/rust_cache/debug/ruagent.exe   mtime 2026-09-26 14:44   (321,812,480 bytes)
$ ruagent.exe knowledge --help
error: unrecognized subcommand 'knowledge'
```

t252 added `ruagent knowledge ingest <DIR>` (its acceptance item 1). That
subcommand does not exist in this binary, so **the ingest path cannot be
exercised without a rebuild**, and a rebuild is a >10 minute full-workspace
build that does not fit one verification call.

**The second blocker is more serious than the first.** This binary also
predates t252's probe isolation. Running it twice, as the acceptance asks,
would therefore exercise the OLD doctor — the one that leaves probe rows
behind — against the user's real database. That is a destructive action on
production data, so **I did not run it.** Refusing was the point: an
independent verifier that runs the pre-fix binary and then reports the
resulting residue as a t252 defect would be wrong, and one that runs it and
reports nothing would be worse.

## Reading 1 — database state BEFORE (acceptance items 2 and 3, before half)

Object set: the five tables the acceptance names, in `~/.ruagent/data/ruagent.db`.
Sampling plane: a **read-only** SQLite connection, `file:...?mode=ro`, `uri=True`
(rule 6 — the user's database is never opened for writing by this review).
Falsifiable criterion: any count below that changes without a corresponding
command in this report means the reading is wrong.

```
documents    4
chunks       11
entities     65
memories     164
wiki_builds  5
```

`wiki_builds` rows, newest first (`id, dry_run, status, finished_at`):

```
(5, 0, 'done',    '2026-09-15T00:48:15.003495300+00:00')
(4, 1, 'planned', None)          <- the dry_run=1 rows the acceptance names
(3, 0, 'done',    '2026-09-14T19:12:10.981998200+00:00')
(2, 0, 'done',    '2026-09-14T19:09:37.308167800+00:00')
(1, 1, 'planned', None)          <- and here
```

**BEFORE status value for `dry_run=1`: `planned`, `finished_at = None`.**

t252 claims two changes here: the stored status becomes the terminal constant
`planned_only`, and a new `finish_dry_run` writes `finished_at`. **Neither is
visible in these two rows, and that is expected, not a defect**: both rows were
written before t252. The "after" half of this comparison requires a NEW dry-run
produced by the rebuilt binary. I could not produce one, so this item is
**half-measured**: the before value is real, the after value does not exist.

## Reading 2 — probe residue in production, right now (finding)

Same object set and sampling plane as Reading 1. The criterion is t252's own
claim: a doctor run must add **zero** permanent rows to production corpus,
memory or graph. Residue that is already there is a different question, and the
acceptance does not ask it — but it is the question an independent verifier can
answer without running anything:

```
documents   LIKE '__probe__%'              0
entities    LIKE '__probe__%'              1
memories    namespace LIKE 'agent:__probe__%'  1
```

**There is 1 probe-named entity and 1 probe-namespaced memory in the live
database.** Two readings follow from this and they must not be conflated:

1. It is **consistent with t252's own report**, which says `--cleanup` exists to
   sweep historical residue under the OLD naming. Residue predating the fix is
   exactly what a fix cannot remove retroactively.
2. It is **not yet evidence that t252 works.** The claim under test is that a
   doctor run adds zero. Residue that was already present says nothing about
   that, in either direction.

What it does establish is that the **acceptance's second item is testable on
this machine today, and that its "before" is not zero.** Whoever rebuilds
should therefore state whether `--cleanup` removes these two rows, and a
plain `doctor` run must not be expected to.

## What is needed to finish this verification

1. **A rebuilt binary** (`cargo build -p ruagent`, `CARGO_TARGET_DIR=D:/rust_cache`),
   so that `ruagent knowledge ingest` exists and `doctor` carries the probe
   isolation. This is the only blocker for acceptance items 1 and 2.
2. Then: run the ingest against a **temp corpus** (never the user's corpus),
   record `documents`/`chunks` before and after, delete the ingested probe
   documents, and confirm the counts return.
3. Then: run `ruagent doctor` **twice**, recording the five counts above before
   and after each run, with the probe prefixes excluded from the "production"
   object set — and state explicitly whether the 1 entity and 1 memory found
   here are still present, removed, or unchanged.
4. For acceptance item 3: run one **new** wiki dry-run and read its
   `dry_run=1` row. The before value is recorded above; without a new row there
   is no after value.

## What I did NOT do, and why (rule 5 — no silent skips)

| not done | reason |
| --- | --- |
| run `ruagent doctor` twice | the installed binary predates t252's probe isolation; running it writes residue into the user's real database. Destructive, so refused. |
| run `ruagent knowledge ingest` | the subcommand does not exist in the installed binary; it needs a rebuild. |
| produce a new wiki dry-run | same reason — the rebuild is the gate. |
| touch `crates/`, `cli/`, `panel/src/` | out of scope for this task. |
