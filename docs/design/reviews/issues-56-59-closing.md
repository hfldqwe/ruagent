# ruagent-issues-56-59 — rounds 2 and 3 closing record

Written by the captain on 2026-10-19/20. Round 1 (issues #45-#55) is recorded separately in
`issues-45-55-closing.md`. This file covers everything after it: the discoveries that became
issues #56-#68, the two delivery rounds that closed #56-#65, the CI repair, and one regression
these words are obligated to describe (SS5) because my first reading of it was wrong.

## 1. What was delivered

| issue | fix | task | the reading that closed it |
| --- | --- | --- | --- |
| #56 memory/write refused a pair with 200 + outcome | 409 + JSON outcome/refused/allowed_namespaces | t1 (round 1-2) | 12 branches measured; the outcome string is byte-identical to a python reconstruction of the Debug arm + write_vocabulary(); memories 0->0, one `op='reject'` audit row per refusal |
| #57 the panel's batch distill silently defaulted to ACP (a paid run) | tier is part of the type system (`DistillTier`, `extractor` required) | t6 | POST bodies rules/acp/rules/acp-real/rules, no `{}` anywhere; receipt `data-run-tier` matches, `data-run-calls=none` |
| #58 the distill coverage range had no ledger columns | migration 0030 (3 nullable columns), read back additively | t3/t4 | a copy of the real v29 database upgraded: 42 rows preserved row-by-row, NULL stays NULL; 19 pre-existing keys compared key-by-key by VALUE against the pre-change build -- 0 differences, which also proves the tail append |
| #59 a pure-assistant session was distilled into a user preference | the attribution rule lives in the prompt, pinned by a test that shares no constant with it | t8/t11 | same fixture + model, only the prompt changed: memories_written 1 -> 0, and the ledger row's `prompt_hash` is exactly the reconstruction with the rule present (441844cc vs 4f408ad0 without it) |
| #60 a surviving agent child left no reading | `Reclaim{killed, already_gone, failed}` on every exit path | t16 | 3 runs / 18 real mock children, 0 left behind; two tests document how to red them |
| #61 supersede caller errors answered 500; empty content was accepted | typed `WriteError` variants -> 422/409; empty content -> 400 naming the field | t15 | 12 branches with SQLite before/after; refusals write no row but one audit row each |
| #62 a governance refusal was 200 + memories_skipped | 409 carrying refused[{store,namespace,allowed_namespaces}] + memories_refused, with the measurement not swallowed | t13 | the verifier hit this 409 over real HTTP with the repo's own mock agent (zero model calls) and confirmed ledger `ok`+reason vs `empty`+NULL |
| #63 the prompt published a flat namespace list | per-store NAMESPACE CONTRACT; `default_namespace()` asks the vocabulary | t14 | observation downgrade really wrote (4,'observation','user'), a plain agent plan wrote nothing |
| #65 the free rules tier could never write procedure/lesson | same change: candidates are stamped with a legal namespace | t14 | free-tier lesson really wrote (5,'lesson','global'); no reject rows left on that path |
| #64 a daemon death left no evidence | `<root>/data/daemon.exit`: only a death that can speak overwrites its boot line | t20 | hard kill -> `DIED WITHOUT A TRACE`; graceful stop -> `exit reason=stop-request`; real panic -> `exit reason=panic` with the site |
| CI: the E2E doctor step had been red since 2026-10-04 | `--url` became a global flag (one file, one line) | t12 | `doctor --url <base>` exits 1 through the daemon-unreachable path instead of 2; `--url <base> doctor` byte-identical; `--urll` still exits 2 |
| the consumer side of #62 | CLI reads error->message->reason; MCP reads both refusal shapes; the panel shows a refusal card + batch refused counts | t18/t19 | a real 409 (mock agent, illegal lesson x user) rendered as `data-refused-pairs=1`, `data-batch-refused=8` with `data-batch-sum=8/8`, and `not_recorded` against an older daemon |
| #67/#68 a refused pass stayed `status=ok`, so the CLI answered `[ok]` + exit 0 and nothing could list the refused sessions | migration 0031 adds a nullable `memories_refused` column; the read side gains `?refused=only|none|not_recorded` and `totals.refused*`; the CLI exits non-zero with `[refused]` | t21 | a real v30 database upgraded in place (48 rows unchanged, NULL preserved), facets 1/2/0 whose union is `total`, `?refused=maybe` -> 400; refused -> exit 1 + `[refused]`, wrote -> exit 0 `[ok]`, empty -> exit 0 `[empty]` -- this closes the item round 3's verifier had to leave open |
| #66 a self-contradictory ledger payload blanked `#settings` | the card checks shape AND coupling before rendering, and reports the defect instead of throwing | t23/t26 | pre-fix, 4 of 7 mutations white-screened (including t10's exact `TypeError ... reading 'ok'`); post-fix, 7/7 no pageerror with `[data-ledger-inconsistent=<defect>]` + detail; the permanent spec's own red control: 4 failed / 2 passed with both clean controls green |
| the regression in SS5 | child attribution now requires positive evidence and has no fallback | t24 | baseline 4/4 green, pre-repair 3 of 6 red, post-repair 15/15 green; snapshotting 88x cheaper; t16's two tests re-reddened in an isolated worktree |

## 2. Gate window and readings

Frozen tree for the round-3 verification (the verifier's own hash pass; every blob equals its
task's declaration, and none moved while the gates ran):

| file | git blob | md5 |
| --- | --- | --- |
| crates/daemon/src/distill.rs | dc7f2cd6f5a5e34bf5d1c5609abeb3c327ab4e92 | b187c53e33e208d98fe1a2580b306bfa |
| crates/daemon/src/api.rs | 524220e2a44d6734d9c2851c4fe42e647c148641 | 0b654b530c71ec62c3031ea89de1ea29 |
| crates/extract/src/lib.rs | 0dcfc2f61d3244cbaa167e15f621abd181e34e23 | -- |
| crates/extract/src/memory.rs | 46700da86872a5f1cddaa072e1cd4b0d84690d2b | 97b13ee28ac71aae7fa7e0447592c669 |
| crates/extract/src/rules.rs | f230fbba48cd73c2aa4a48316dfe284354d0bbc5 | -- |
| crates/extract/tests/memory-gold.rs | ef98f9d3a1e3439f4f873279dbb1804a6a4d3525 | -- |
| crates/memory/src/write.rs | 425ee6dd893c5d934e0ee9c3eef272f8b31f3a62 | -- |
| crates/memory/src/lib.rs | d9358d43714c22cd66e3e8b1693fa67133268d39 | -- |
| crates/memory/tests/write_governance.rs | c2641b8c2e34432840aaf8054b173f080020d5a0 | -- |
| cli/src/main.rs | 9a4553cb40af761fd4317f913cc6a5649aa70380 | 3ac8b158596a9b15d2d50635a58d01e7 |
| crates/daemon/src/orphans.rs / chat.rs / runs.rs / lib.rs | 187842e4 / 2e131cdb / 7f405afc / 2869a286 | -- |

Commands and readings on those bytes: `cargo fmt --all --check` exit 0;
`cargo clippy --workspace --all-targets -- -D warnings` exit 0 (25s); `cd panel && npm run build`
exit 0 (i18n zh=913/en=913 + two tsc projects + vite); `NO_PROXY=127.0.0.1,localhost,::1`
`cargo test --workspace` -- see SS5, this is where the honest story is.

## 3. What the independent verifier did NOT take on trust

* It rebuilt every reading on its own private daemon (port 8801, throwaway root, its own binary
  sha256 `5247eed9b1f2ac53ee3fe0e5eec713aca11d50706c23089e2b77e31d0c818d6e`), and never posted a
  single request that could change 8787 -- health was read, nothing else.
* For #62 it registered the repo's own mock agent in its throwaway root and made the model-less
  path produce an illegal `lesson x user` pair: real HTTP 409, keys checked, and the measurement
  (`coverage{0,2,2}`, windows=1) confirmed NOT swallowed by the error.
* It read the ledger with its own SQLite queries rather than trusting the API, and compared
  `status` AND `failure_reason` to establish that a refused pass cannot be mistaken for an empty one.
* It declined to accept "the test comment explains the re-pin": the byte-identical prompt gate
  (t329's `the_memories_half_of_the_prompt_is_byte_identical`) had to be re-pinned because the JSON
  example moved, and the verifier checked the gate's INTENT against SOURCE instead.
* It reported what it could not do: the second real-ACP re-measurement of #59 is a
  deliberately-authorized cost nobody paid twice, so that conclusion rests on round 2's single real
  run plus the byte-level prompt_hash attribution -- labelled as such, not as its own measurement.

## 4. Consumption

Round 2 and round 3 together spent **3 real prompt turns**, all in round 2 and all authorized: the
#59 ACP measurement and its re-measurement (#59's whole point is what a model does with the
prompt), plus bootstrap option-catalog handshakes (`prompt=None`) that spend no tokens. Every
other reading -- 409 contracts, migrations, coverage columns, namespaces, reaping, exit traces,
the console UI -- was taken on `extractor=rules`, mock agents, or pure HTTP+SQLite. The ACP tier
stays behind the panel's explicit choice, and #57's `DistillTier` now makes the cheap tier the
one you must give up on purpose.

## 5. The regression I first misread as a flake (and the discipline that caught it)

`cargo test --workspace` on the frozen round-3 tree exited **101**:
`ruagent-mock-agent --test e2e_daemon :: approver_agent_answers_ask_unattended` failed at
`e2e_daemon.rs:971` with `worker proceeds after approval`.

My first two full-workspace runs had shown only passing targets -- because I had piped them through
`tail`, which cut the failing target out of view. Then the failing test **slid**:

| run | failing tests (default concurrency) |
| --- | --- |
| 1 | `approver_agent_answers_ask_unattended` |
| 2 | `chat_runs_in_its_project_cwd`, `chat_stop_cancels_inflight_reply` |
| 3 | (all 24 passed) |
| 4 | `approver_agent_answers_ask_unattended`, `chat_runs_in_its_project_cwd` |

A sliding failing set is the classic signature of a load flake, and that is what I called it --
while noting in the same breath that round 3 had touched exactly this machinery (t16 rewrote child
reclaim in `chat.rs`/`runs.rs`/`orphans.rs`). Isolation was 5/5 green and `--test-threads=4` was
24/24 green, which fits "too much concurrency for this machine" equally well as "this machine is
busy". Both readings were available and only one of them was true.

AGENTS.md's rule is to reproduce a red on an untouched baseline before believing it, so I built one:
`git worktree add D:\wt_flakebase e4bbb58` with **its own** `CARGO_TARGET_DIR`
(`D:\rust_cache_wt\flakebase`, `BUILD_EXIT=0`), and then ran the two trees **interleaved** so both
saw the same machine load:

| tree | default-concurrency runs | result |
| --- | --- | --- |
| baseline HEAD (round 3 absent) | 4 + 3 interleaved | **7/7 green** |
| current tree (round 3 present) | 3 + 3 interleaved | **6/6 FAILED** |

That is not an environment flake. What it was took a second pass, and the second pass moved the
diagnosis twice: the repair's own owner re-derived it from the diff rather than from my summary,
and the captain's first guess ("the reclaim work sits on the close path and slows it") turned out
to be half right -- the half it missed was the serious one.

**The cost.** The old `matching_image` took a full-machine process snapshot *per candidate*:
measured **avg 68 ms** against the replacement `attribute_child`'s single snapshot, **avg 5 ms**.
Per 3 s reap window that is ~8760 ms -> ~100 ms, an **88x** reduction. `e2e_daemon` runs 24 daemons
inside ONE process, so every one of them was snapshotting the whole machine while the others
spawned.

**The defect, and this is the one that matters.** The committed round-3 chat path claimed a child
with NO image evidence whenever `children_since(&window).len() == 1`. When session A's spawn window
opens before session B's agent starts and A's own child has not appeared yet, the single "new"
child IS B's agent -- so A's close or cancel **killed a live agent belonging to another session**,
which then reported `Process exited with exit code: 1` while still being driven. That is not a test
artifact: the exposure is the whole 3 s window at 25 ms polling, so any two concurrent sessions can
interleave that way, and the 24-daemon test suite simply meets it every time. The repair replaces
the fallback with positive evidence only -- the image must match AND be unique among our live
children -- and accepts the measured cost of declining to attribute (an unread reading, never a
stranger's agent). What actually prevents leaks is the fallback chain it verified: the ACP SDK
kills the child process group when the connection drops (`acp/src/chat.rs:197`), the daemon's
kill-on-death job (`daemon/src/lib.rs:351`), and the next boot's orphan sweep.

Readings after the repair -- same window, interleaved, default concurrency on this 16-core machine:

| tree | runs | result |
| --- | --- | --- |
| baseline HEAD `e4bbb58` (round 3 absent) | 4 | 4/4 green |
| `ce1600b` (round 3, pre-repair) | 6 | **3 red** (`chat_runs_in_its_project_cwd`, `e2e_daemon.rs:2614` HTTP 400) |
| current tree (repair) | 15 | **15/15 green**, including 5 consecutive final checks |

t16's guarantee survived the repair: all three outcomes (`killed`/`already_gone`/`failed`) are
still emitted by all three exit paths, and both of its tests were **re-reddened in an isolated
worktree** (removing the reclaim call -> two panics at `chat.rs:2211`; forcing attribution to fail
-> panics at `chat.rs:2578`/`2690`), with the triggers written into the tests' own docs. Zero
assertions were loosened, checked mechanically rather than promised: 0 assert-line changes in
`runs.rs` and `e2e_daemon.rs`, `chat.rs`'s four changes are doc comments, and `orphans.rs`'s nine
are unit-test adaptations whose expectations are unchanged (they swap an injected fake for a
`ChildRef` that carries a start time). After 15 runs plus the chat suite, mock-agent child
processes: **0**.

What this cost and what it bought: the wrong reading ("pre-existing load flake") was written into
my first draft of this round's commit message and removed before the push, because the baseline had
been run. Had I trusted the sliding-failure-set heuristic, a real regression would have been
committed as an environment problem -- the failure mode AGENTS.md records for the proxy and ONNX
cases, arriving from the opposite direction. It cost a second lesson too: my first repair brief
carried my own half-diagnosis into the acceptance criteria, and the repairer was right to override
it by reading the diff. The brief for **t24** therefore carried two non-negotiable items -- confirm
the mechanism yourself from the diff, and state the NEW rule's production cost honestly instead of
letting a test-only artifact (each test now gets its own mock-binary name, which production does
not have) make the suite look clean for the wrong reason.

## 6. Disclosed, not hidden

* **t14 changed six files while its task record declared one.** The captain had folded #65 into a
  running task by message, which cannot edit a locked contract; the extra files (three in
  `crates/extract`, one gold test, plus `api.rs` for two t13 fixtures that this fix had made
  unreachable) were disclosed by the implementer with per-file blob hashes and checked by the
  verifier against that list. A task whose record under-declares its footprint is a real defect of
  the record; the honest handling is the disclosure plus a reviewer's check, which is what happened.
* **An unauthorized probe, self-reported**: t18's first stub failed to bind its own port while a
  member's daemon was listening on 8897, so its CLI and curl each sent one distill request there.
  Both answered 400 (unknown session) before any write; the captain confirmed the database's mtime
  predates the probe. It was reported by the member that made it, unprompted.
* **A free-tier defect found while measuring something else**: #65 existed only because t13
  noticed, in passing, that the rules tier stamped `namespace=user` on procedure/lesson candidates
  and its own vocabulary then refused them. It is fixed in this round; the issue is closed with it.
* **The evidence the captain himself destroyed**: cleaning probe leftovers out of the user's home
  removed the `~/.dsh/sessions/...t8-acp...` directory that round 2's report cites for its per-turn
  token counts. The verifier could still confirm the ledger side (`tokens_used=13224`) but had to
  report those numbers as unreproducible from their cited source. The rule is now in AGENTS.md.
* **Accepted trade-offs, so a later reader does not re-litigate them**: the namespace contract lives
  inside `EXTRACTION_PROMPT` (so the published `builtin_prompt` equals the bytes actually sent, at
  the cost of an override replacing the table too); and a refused pass KEEPS `status=ok` +
  `failure_reason` even after round 4, because t13 refused to add a fourth status word to a closed
  vocabulary that the panel counts by -- round 4 settled it with an ADDITIVE column instead
  (`memories_refused`, migration 0031), so every existing reader's behaviour is unchanged, and the
  panel reads it through one helper rather than three copies of the same arithmetic. The repair added
  a third: when two sessions run the same CLI image name, attribution declines -- a reading of 0
  rather than a guess -- because the guess measured out as killing a live agent of another session.

## 7. Still open when this was written

| issue | why it is not closed here |
| --- | --- |
| #69 | t16's two tests swallow a failed `remove_dir_all` (the db handle is still open), so `%TEMP%\ruagent-t16-*` has grown to 56 directories; the fix must make the failure VISIBLE as well as successful |
| exact child ownership | the repair's honest cost -- sessions that share a CLI image name go unattributed. Per-session job objects, or reading the pid back from the SDK, would restore exact attribution; both need the spawn ownership, so it is a design task rather than a drive-by |
| round 2's one code-read acceptance item | t9 declared an acceptance item as a code reading rather than a measurement; it is recorded here rather than papered over |

## 8. How to reproduce any single row of SS1

Every row names a task; each task's `output` carries the exact commands, ports, private roots and
readings (`.agent-teams/ruagent-issues-56-59/`), and every member kept its evidence under
`%TEMP%\<task>\`. The private daemons used for measurement are stopped; the fixtures that reports
cite were deliberately left in place (AGENTS.md: never delete an artifact a report cites).
## 9. What happened to the machine after the round closed

* **The resident daemon was upgraded through the scheduled task, not from a session.** `stop -Force`
  was needed because the running binary PREDATED the graceful-stop request file -- that file is one of
  this round's own additions -- so "forced after 10 s" was the expected, explainable outcome rather
  than a failure. The binary was rebuilt from the pushed tree and started with `Start-ScheduledTask`,
  i.e. from a process whose parent is the Task Scheduler rather than a session of mine. It came up
  healthy on `127.0.0.1:8787`, applied migration 0031 to the real database (`max=31`, `distill_log`
  now ending in `memories_refused`), and serves the new read side (`totals.refused` /
  `refused_not_recorded`, `?refused=only|none|not_recorded`, 400 on anything else). Its log immediately
  shows the new instrumentation in production, including the accepted cost stated out loud:
  `no child could be attributed to this chat ... its close will reclaim nothing` with
  `children=0 killed=0 already_gone=0 failed=0`.
* **The measurement instruments were removed** after the round closed: the baseline worktree
  `D:\wt_flakebase` with its own target dir, and the round-4 verifier's control worktree
  `D:\wt_t25ctl` with `D:\rust_cache_t25ctl` (~13 GB together). They are rebuildable rather than
  unique -- SS5 and the task outputs carry the exact commands -- so their removal is RECORDED here
  instead of being silent. The evidence the reports cite was not touched: the `%TEMP%\<task>\`
  directories (including t9's, which the #56/#58 reads come from, and t20's exit-trace root) and the
  fixture databases under `%TEMP%\ruagent-t5-panel\` stay where the reports left them.
* **The private daemons were reaped by their recorded pids, never by a name or port sweep**: 8895 and
  8897 honoured the graceful stop request -- an independent confirmation, on a real running daemon,
  that the mechanism t20 added works -- while 8896 predated it and had to be terminated. The resident
  daemon on 8787 was never touched.
* **The E2E job now runs 68 tests where the reference floor is 62**: the six new cases are the
  ledger-inconsistency spec, and the run that measured them passed 68/68 with 0 skipped.
