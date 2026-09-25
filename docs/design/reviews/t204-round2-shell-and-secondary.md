# t204 — design-contract review, round 2: shell & secondary views

Reviewed: **t203** (the repair round for t13's findings). Reviewer: systems. Read-only.
Round 1's review is `docs/design/reviews/t13-round1-shell-and-secondary.md`; this one re-checks
the same contract and re-tests the two findings that round 1 raised.

## 0. Method and its limits

* Live probe against the running daemon (8787), both an unmodified page and one with
  `page.route` stubs. Nothing here is inferred from t203's report.
* Audit subset re-run **for the repaired route only** (`--routes=runtimes`, 2 captures,
  75.2 s, panel build `B3JqvEU6`): **52 pass / 0 fail / 7 not measured**, exit 0.
  I did not re-run the full 13-route sweep (minutes of CPU; the team was asked to stop
  heavy runs), so the other routes' audit rows are **t203's** readings, not mine.
* Not obtained, and stated rather than guessed: the `#settings` empty-dropdown *text*
  (with `/api/v1/agents` stubbed to `[]` the page renders only the extract-agent Select,
  so I could not open the roles Select; the code path is verified instead, §3.2), and the
  four states of the other routes under injected failure.

## 1. The two round-1 findings

### 1.1 HIGH — the `#runtimes` probe failure is no longer silent (**fixed, reproduced**)

Code (`panel/src/views/Runtimes.tsx`): the failure is returned, not swallowed —

```ts
const sync = async (name: string) => { … try { … return true; } catch { return false; } … };
const probe = async (name: string) => { const ok = await sync(name); setProbeErr((m) => ({ ...m, [name]: !ok })); };
```

and the alert is `:287–295`: `<span role="alert"><button type="button" className="tag err"
onClick={() => probe(r.name)}>{t("runtimes.probeFailed")}</button></span>` — the button calls
`probe` again, so the promise the old line made is now executed.

**Runtime reproduction** (probe, 1440×900, dark; the options endpoint stubbed to 500 on the
4th request only):

```
load requests=3
after the failed probe: requests=4 alerts=1 tag=SPAN text="探测失败 · 重试" hasButton=true btnText="探测失败 · 重试"
after clicking the retry: requests=5 alerts=0 cleared=true
```

⇒ the failure renders `role=alert`, the alert contains a **button**, clicking it issues a
**real** request (#5), and the alert clears. **Fixed.**

### 1.2 LOW — the settings empty case uses a settings-domain key (**fixed**)

`panel/src/views/Settings.tsx:208` is `notFoundContent={t("settings.noRoles")}`; the key exists
in **both** languages — `panel/src/i18n/settings.ts:11` `"没有可选角色"` and `:17`
`"No roles available"` (the values match `view-settings.md`'s wording). `node e2e/i18n-check.mjs`
⇒ **VERDICT: PASS** (and `distill.noAgents`, the borrowed key, now shows up in that check's
dead-key candidates — consistent with the copy having moved). **Fixed**; the dropdown text
itself was not reproduced (§0).

## 2. Per-route contract checks

### Hierarchy centre (MASTER row 24)

Measured this round at 390 and 1440, dark: `h1` = **1** for #graph, #runtimes, #settings.
Audit row 24 for #runtimes: **PASS** ("恰好 1"). ⇒ **PASS**.
(Reminder carried from round 1: `grep -c '<h1'` reads 2 for Runtimes/Settings because the two
`<h1` sit in alternative loading/loaded branches — a source count is not a runtime count.)

### Four breakpoints do not collapse

Overflow **0** this round at 390 and 1440 for all three routes; round 1 measured 390/768/1024/1440
= 0; audit row 19 for #runtimes: **PASS** ("所有路由 × 所有视口 = 0"). ⇒ **PASS**.

### Shared primitives rather than hard-coded values

`main [style]` = **1 / 4 / 10** (#graph / #runtimes / #settings), budget 50 (audit row 12 for
#runtimes: **PASS**). Hex literals in the three view files: **0 / 0 / 0** ⇒ colour goes through
`var(--…)`. ⇒ **PASS**.

### The four states

| route | evidence | verdict |
| --- | --- | --- |
| #runtimes | probe error state now exists **and retries** (§1.1, reproduced) | **PASS** |
| #settings | empty case wired to `settings.noRoles` in both languages (§1.2); dropdown text not reproduced | **PASS** (code + i18n check) |
| #graph | `prefers-reduced-motion` implemented — `Graph.tsx:608/728/982` | **PASS** |
| #home | error state implemented — `Home.tsx:16` imports `ErrorState`, `:221` renders it | **PASS** |

Audit rows for #runtimes that bear on the states: 13/14 (text contrast failures 0) **PASS**,
17 (unnamed interactives 0) **PASS**, 18 (hit targets) **PASS**.

## 3. The cross-file risk of this group: `sourceHue`

Unchanged and structurally safe: **one** definition at `panel/src/views/Sessions.tsx:35`
(`export function sourceHue(s: string): string`), imported by `panel/src/views/Home.tsx:18` and
used at `:280`, and imported by `panel/src/views/SessionsView.tsx:24`. Home holds no copy, so it
cannot disagree with Sessions. The body still matches `view-sessions.md §4.1` four ways
(`claude-code/opencode/deepseek → var(--brand-*)`, `default → var(--ant-color-text-tertiary)`),
and the signature is still `(s: string) => string`. ⇒ **PASS**.

## 4. Verdict

**pass** — both round-1 findings are fixed, and one of them is now demonstrated at runtime with
request counts rather than asserted. The acceptance's five checks are each backed by a file, a
line or a measured number (§2), the group's cross-file risk is structurally impossible (§3), and
the two things I could not obtain are named in §0 instead of being written as green. The only
readings I am relaying rather than owning are the audit rows for routes other than #runtimes.
