# t322 — panel cold-start loading: navigation → first screen → interactive

Measured with a real browser on the live 8787. No `panel/src/` change; no write
access armed; the browser is created and closed by the probe itself.

## Verdict: **NOT a real problem** — and the objective's own mechanism is confirmed

Every route reaches its controls in **~90–130 ms** on a cold context, and the
warm run is **not faster**. The 1 MB vendor chunk transfers uncompressed exactly
as the objective said, and it is **not** what costs: it decodes and parses in
**16 ms**, while a **46 KB font** takes **92 ms**.

## Object set, sampling plane, falsifiable criterion

* **Object set**: the panel as served by the live daemon at `http://127.0.0.1:8787`,
  three routes (`#/home`, `#/memory`, `#/knowledge`).
* **Sampling plane**: a real Chromium via Playwright, driven from a temp script
  inside `panel/` (so `playwright` resolves), `headless: true`. **cold** = a fresh
  browser context per run (no cache); **warm** = one shared context reused. 3 runs
  per route per mode.
* **Falsifiable criterion**: if cache mattered, warm would be materially faster
  than cold; if transfer mattered, the vendor chunk's `transferSize` would be
  well below its `decodedBodySize`; if the problem were real, a route's median
  would exceed the threshold below.

## Readings (ms) — cold = fresh context, warm = shared context

```
route      mode  loadMs (3 runs)        median   dcl median   interactive (3 runs)
home       cold  185 / 108 / 135         135        132        100 / 103 / 89
home       warm  159 / 115 / 111         115        108        96 / 95 / 100
memory     cold  104 / 100 / 100         100         98        95 / 88 / 88
memory     warm  100 / 118 / 130         118        115        91 / 81 / 107
knowledge  cold  110 / 129 / 120         120        107        89 / 104 / 97
knowledge  warm  117 / 101 / 118         117        111        104 / 118 / 127
```

* Cold median load: **135 / 100 / 120 ms** (home / memory / knowledge); warm:
  **115 / 118 / 117 ms**. **Warm is not faster — it is the same.** With no cache
  benefit visible, transfer is not the constraint.
* Interactive (first enabled form control visible, measured after the load event):
  **88–127 ms** across every sample. `inputs: 23` on every route, so the controls
  are present when the load event fires.
* Range across all 18 navigations: **100–185 ms load**, **81–127 ms interactive**.

## Resource readings (home; identical chunks on the other two routes)

```
name                                 transfer   decoded    dur
vendor-DnTfy3f4.js                  1,044,705  1,044,405   16   <- the 1 MB chunk
index-Bq1SRq2N.js                     120,054    119,754    6
index-BBh3ZOue.css                     45,656     45,356   21
vendor-DW1jimNH.css                     7,928      7,628    6
Home-seJtVnr6.js                        9,216      8,916    2   <- route-level chunk EXISTS
CreateTaskModal-Qc2S3SHn.js             1,625      1,325    2
ibm-plex-sans-latin-wght-normal.woff2  46,012     45,712   79   <- slowest thing on the page
ibm-plex-mono-latin-600-normal.woff2   15,920     15,620   79
ibm-plex-mono-latin-500-normal.woff2   15,188     14,888   35
ibm-plex-mono-latin-400-normal.woff2   15,008     14,708   80
```

**How the objective's mechanism shows up in the numbers**: `transfer ≈ decoded`
for every asset (vendor: 1,044,705 vs 1,044,405 — a 300-byte difference, i.e.
**uncompressed**), so the loopback moves the full 1 MB. And yet the vendor chunk's
`dur` is **16 ms** — the same order as a 7 KB CSS file. **Parsing and executing
1 MB of JS is not what costs here.** The slowest resources on the page are four
**fonts** (79 / 79 / 35 / 80 ms), and their total is 92 KB against the vendor's
1 MB.

## The threshold, and why this is not a problem

Stated before the reading, as the acceptance requires: **a first screen beyond
1 s, or interactivity beyond 2 s, would be a real problem.** Measured: worst
median **135 ms** load, worst interactive **127 ms** — one order of magnitude
inside both. **Conclusion: loading speed is not a real problem on this machine.**

## What I could NOT measure (rule 5 — no silent zeros)

* **`first-contentful-paint` is 0 in every sample.** The paint entry is not
  emitted by this headless Chromium. That is a **missing measurement, not a
  0 ms first paint**, and I am not reporting it as a reading. The acceptance asked
  for FCP; I could not get it here.
* **"First interactive" is a proxy.** It is the first visible, enabled form
  control — measured **after** the `load` event, so it is "controls present", not
  a true time-to-interactive. It is named as a proxy rather than presented as TTI.

## Minimal falsifiable suggestions (only because the acceptance asks)

Since the verdict is "not a problem", these are **not** required fixes; they are
the cheap checks that would matter if a slower machine changed the answer:

1. **Route-level splitting is already in effect** — `Home-seJtVnr6.js` exists as a
   separate 9 KB chunk. Worth checking why it is fetched on `#/memory` and
   `#/knowledge` too (it appears in all three resource lists).
2. **antd is not fully inlined into the route chunks** — `vendor-*` at 1 MB is the
   antd + React core. Measuring whether a slower device changes the 16 ms parse
   would need a CPU-throttled profile, which this reading does not include.
3. **The fonts are the slowest asset**, not the JS. If anything here is worth
   deferring, it is them — but at 79 ms they are not a user-visible problem either.

## What I did NOT do

| not done | reason |
| --- | --- |
| change `panel/src/` | out of scope |
| arm write access / run the e2e suite | not needed; the probe drives the live daemon read-only |
| leave the probe behind | `panel/.t322probe.mjs` deleted; the browser was closed by the probe itself |
| CPU/network throttling | not requested; recorded above as the one thing that could change the verdict |
