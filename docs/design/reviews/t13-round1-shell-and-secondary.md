# t13 — design-contract review: shell & secondary views (round 1, attempt 2)

Reviewed: **t7** (App / CommandPalette / Graph / Runtimes / Settings).
Reviewer: systems. Method: read-only. Every claim below carries a file, a line or a
measured number; nothing here rests on impression.

## 0. What I could and could not verify

* **Verified by measurement** — h1 per route, overflow at four widths, inline-style counts
  (live probe against the running daemon, 8787, both modes).
* **Verified by reading** — the shared-primitive usage, the `sourceHue` contract, the state
  vocabulary, and the specific gaps the specs name.
* **NOT verified (stated, not guessed)** — the audit's own rows for these three routes: I did
  not re-run `design-audit.mjs` (its 13-route sweep is minutes of CPU and the user has asked
  the team to stop heavy builds/runs). t7's numbers are therefore *its* readings, not mine.
* **NOT verified** — the four states under **injected failure** for these routes; I measured
  the ready state only. The Runtimes gap in §3 is a *code* finding, which needs no injection.

## 1. Per-route acceptance items

### Hierarchy centre (one h1 per route)

Measured, live, both modes (probe, 1440×900, `?mode=<m>#<route>`):

| route | dark | light | h1 text |
| --- | --- | --- | --- |
| #graph | 1 | 1 | 实体图谱 |
| #runtimes | 1 | 1 | 运行时 |
| #settings | 1 | 1 | 设置 |

⇒ **PASS** (MASTER row 24). Note for future reviewers: `grep -c '<h1'` reports **2** for
Runtimes and Settings, because each file holds two `<h1` elements in *alternative* branches
(loading vs loaded). The source count is not the runtime count — this is exactly the kind of
"looks like a reading" that a review must not report as a violation.

### Four breakpoints do not collapse

Measured overflow (`scrollWidth - clientWidth`), dark:

| route | 390 | 768 | 1024 | 1440 |
| --- | --- | --- | --- | --- |
| #graph | 0 | 0 | 0 | 0 |
| #runtimes | 0 | 0 | 0 | 0 |
| #settings | 0 | 0 | 0 | 0 |

⇒ **PASS**.

### Shared primitives rather than hard-coded values

Measured `main [style]` (row 12 budget 50) and hex literals in the three view files:

| route | inline style nodes (dark/light) | `style={{` in source | hex literals in source |
| --- | --- | --- | --- |
| #graph | 1 / 1 | 2 | 0 |
| #runtimes | 4 / 4 | 8 | 0 |
| #settings | 10 / 10 | 10 | 0 |

⇒ **PASS**. **Zero hex literals in all three files**: colour goes through the token layer
(`var(--…)`), which is the contract's point. The remaining inline styles are geometry
(pixel offsets), not colour.

### The four states

| route | evidence | verdict |
| --- | --- | --- |
| #graph | `prefers-reduced-motion` implemented — `Graph.tsx:608` (comment naming G11), `:728` `matchMedia("(prefers-reduced-motion: reduce)")`, `:982` (redraw path). `view-graph.md:108` named this **缺失（必须补）** | **fixed** |
| #runtimes | `Runtimes.tsx:36` is still `.catch(() => {});` — see §3 | **VIOLATION** |
| #settings | empty case present: `Settings.tsx:206` `notFoundContent={t("distill.noAgents")}` (spec asked for 「没有可选角色」) | present, with a naming smell (§3.2) |
| #home | error state present: `Home.tsx:16` imports `ErrorState`, `:221` renders it — `view-home.md:95` named this **现状违反** | **fixed** (for the primary readout path) |

## 2. The cross-file risk of this group: `sourceHue` semantics

The acceptance names this specifically. There is **exactly one definition**:

* `panel/src/views/Sessions.tsx:35` — `export function sourceHue(s: string): string`
* `panel/src/views/Home.tsx:18` — `import { SOURCE_LABEL, msToIso, sourceHue } from "./Sessions"`,
  used at `Home.tsx:280`
* `panel/src/views/SessionsView.tsx:24` — same import; `Chat`/`CommandPalette` go through `App.tsx:85`'s note

⇒ **PASS, and structurally so**: Home cannot disagree with Sessions because it does not own a
copy — a second definition would be the only way to diverge, and there is none.

The body matches `view-sessions.md §4.1`: `claude-code → var(--brand-claude)`,
`opencode → var(--brand-opencode)`, `deepseek → var(--brand-deepseek)`,
`default → var(--ant-color-text-tertiary)` — i.e. the four-family ban in that section's
constraint 4 (no `--signal` / `--status-*` / `--graph-*` / `--ws-*`) holds by inspection, and
the signature stayed `(s: string) => string` as required.

## 3. Findings

### 3.1 HIGH — `#runtimes` probe failure is still silent (spec-named violation)

`panel/src/views/Runtimes.tsx:36`:

```ts
        .catch(() => {});
```

`docs/design/views/view-runtimes.md:99` names this exact call site and calls it a violation:
「`api.agentOptions(name, true)` 的 `.catch(() => {})`（`Runtimes.tsx:36`）**静默** → 同步失败
看起来像"没反应"；规格要求按钮旁 `role=alert` 一行「探测失败 · 重试」」. The line is unchanged and
no `role=alert` exists on the route (measured: `role=alert` count = 0 in the ready state, and
the code has no error branch for this call).

Required fix: catch into state and render a `role=alert` line beside the probe button, with a
retry affordance — i.e. the error state the contract asks for, not a swallowed promise.

### 3.2 LOW — the settings empty case is keyed to the wrong domain

`Settings.tsx:206` renders `t("distill.noAgents")` as the Select's `notFoundContent`. The
behaviour the spec asks for is present, but the copy is borrowed from the distill domain; a
settings-owned key would make the intent legible and keep the i18n split honest
(`panel/src/i18n/settings.ts` exists now).

## 4. Verdict

**needs_revision** — one spec-named violation stands (§3.1), plus one low-severity naming
issue (§3.2). Everything else the acceptance lists is implemented and measured: the hierarchy
centre (row 24), the four breakpoints, the shared-primitive layer (zero hex literals), the
`sourceHue` cross-file contract, and three of the four spec-named state gaps.

**Not obtained, stated as such** (and deliberately not written as green): the audit's own rows
for these routes, and the four states under injected failure. t7's report lists a site-wide
row 27 FAIL (first-paint JS 1405 KB / gzip 438 KB against 350/120); I did not re-measure it,
so I neither confirm nor dispute the number — it is out of this review's reach.
