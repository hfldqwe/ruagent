// t26 (#66): a distillation-ledger response that CONTRADICTS ITSELF must not
// white-screen the panel. This is the resident guard for the defect t10 measured
// and t23 fixed; before t26 the proof only lived in a %TEMP% probe script, and a
// guard that is not in the repo is not a guard: the next change could restore the
// old behaviour and CI would still see everything green.
//
// THE DEFECT, MEASURED. On the t22 revision, a ledger page whose summaries
// describe one set of rows while entries/total describe another unmounted the
// WHOLE #settings route (React removed the tree, so #root's innerText was EMPTY)
// with one of these uncaught errors:
//
//   drop_by_status        TypeError: Cannot read properties of undefined (reading 'ok')
//   null_by_status        TypeError: Cannot read properties of null (reading 'ok')
//   drop_totals           TypeError: ... (reading 'coverage_not_recorded')
//   entries_not_an_array  TypeError: Qe.some is not a function   (antd Table internals)
//
// 'ok' is the FIRST status bucket the card indexes, which is why a missing
// by_status reports that word. Three more variants rendered contradictory numbers
// as if they were data -- those are covered by the detail assertions below.
//
// HOW THE FIXTURES ARE BUILT (and why this file is deterministic): every case
// fetches the daemon's REAL body through page.route and rewrites exactly ONE
// coupling before fulfilling it. So the spec needs the daemon to be UP and to
// answer the ledger route -- it never needs the daemon to BE contradictory, and
// it never writes anything: no config, no rows. writeAccess() is therefore not
// needed here (see e2e/write-guard.ts for the specs that do write).
//
// WHAT EACH DAMAGING CASE ASSERTS:
//   (a) no pageerror was recorded -- the white screen always came with one;
//   (b) #root still carries the app (a white screen leaves it empty);
//   (c) the card printed the diagnostic: [data-ledger-inconsistent=<defect>] with
//       the observed numbers on [data-ledger-detail], and did NOT fabricate the
//       piece the defect made unusable (no buckets, no rows, no zero).
// The clean controls assert the opposite: no notice at all, and the normal
// renderings (the row count the served body declared, and four buckets).
//
// HOW TO MAKE THIS SPEC RED (this was run as t26's negative control): short-circuit
// the consistency check the card performs. In panel/src/views/Settings.tsx replace
//
//     const check = checkDistillLogPage(page);
//
// with a permissive object that hands the raw response straight back:
//
//     const check = { ok: true, defect: null, detail: { total: null, entries: null, bucketSum: null, missing: [] }, entries: page.entries, by_status: page.by_status, totals: page.totals, total: page.total };
//
// rebuild the panel, run this file: the four damaging cases FAIL (no notice,
// pageerror recorded, #root empty) while both clean controls still pass. The
// measured red/green pair is recorded in the t26 report.

import { expect, test, type Page } from "@playwright/test";

const LEDGER_ROUTE = "**/api/v1/distill/log*";

/** The parts of a ledger page this file is about. Untrusted by construction: the
 *  whole point is that a daemon (or a middle layer) can hand the panel a body
 *  whose parts describe different sets. */
type LedgerBody = {
  entries?: unknown;
  total?: unknown;
  by_status?: unknown;
  totals?: unknown;
};

/** One damaging rewrite, the defect it must be reported as, and what the detail
 *  line must say: the number that could NOT be read is a question mark, never a
 *  fabricated zero. */
type Damage = {
  case: string;
  defect: string;
  edit: (b: LedgerBody) => void;
  detail: RegExp;
};

const DAMAGING: Damage[] = [
  {
    case: "drop_by_status",
    defect: "by_status_missing",
    edit: (b) => {
      delete b.by_status;
    },
    detail: /;bucketSum=\?;missing=by_status$/,
  },
  {
    case: "null_by_status",
    defect: "by_status_missing",
    edit: (b) => {
      b.by_status = null;
    },
    detail: /;bucketSum=\?;missing=by_status$/,
  },
  {
    case: "drop_totals",
    defect: "totals_missing",
    edit: (b) => {
      delete b.totals;
    },
    detail: /;bucketSum=\d+;missing=totals$/,
  },
  {
    case: "entries_not_an_array",
    defect: "entries_missing",
    edit: (b) => {
      b.entries = { ok: 16 };
    },
    detail: /;entries=\?;bucketSum=\d+;missing=entries$/,
  },
];

/** A CONSISTENT ledger page this file owns, used only when the daemon's own
 *  ledger has no rows yet (a fresh CI daemon): three entries, four buckets that
 *  add up to the total, coverage and tokens recorded as absent-as-null rather
 *  than as measured zero. It exists so the clean control exercises the ROW and
 *  BUCKET renderings on any machine instead of degenerating into 'the card said
 *  nothing was there'. */
const SYNTHETIC_PAGE = {
  entries: [0, 1, 2].map((i) => ({
    id: 9000 + i,
    session_key: "ruagent:ledger-inconsistency-fixture-" + i,
    distilled_at: "2026-10-19T00:0" + i + ":00Z",
    status: i === 2 ? "empty" : "ok",
    failure_reason: null,
    agent: "fixture",
    memories_written: i === 2 ? 0 : 1,
    entities_written: 0,
    relations_written: 0,
    relations_duplicate: 0,
    relations_refused: 0,
    tokens_used: null,
    context_size: null,
    cost_usd: null,
    retries: 0,
    windows: 1,
    truncated: false,
    prompt_hash: null,
    from_seq: null,
    to_seq: null,
    total_messages: null,
    memories_refused: 0,
  })),
  total: 3,
  limit: 20,
  offset: 0,
  truncated: false,
  by_status: { ok: 2, empty: 1, failed: 0, not_recorded: 0 },
  totals: {
    tokens_used: null,
    cost_usd: null,
    tokens_used_not_recorded: 3,
    cost_usd_not_recorded: 3,
    coverage_not_recorded: 3,
    refused: 0,
    refused_not_recorded: 3,
  },
};

/** The same shape with nothing in it -- what the card's SECOND read (the most
 *  recent failure) answers in the synthetic control, so the failure line stays
 *  honest about there being no failure. */
const EMPTY_PAGE = {
  entries: [],
  total: 0,
  limit: 1,
  offset: 0,
  truncated: false,
  by_status: { ok: 0, empty: 0, failed: 0, not_recorded: 0 },
  totals: {
    tokens_used: null,
    cost_usd: null,
    tokens_used_not_recorded: 0,
    cost_usd_not_recorded: 0,
    coverage_not_recorded: 0,
    refused: 0,
    refused_not_recorded: 0,
  },
};

/** Install the rewrite, collect page errors, navigate, and wait until the card's
 *  own read has landed (its zone note renders whatever the response said). */
async function openDamaged(
  page: Page,
  errors: string[],
  edit: ((b: LedgerBody) => void) | null,
): Promise<void> {
  await page.route(LEDGER_ROUTE, async (route) => {
    const res = await route.fetch();
    let body: LedgerBody;
    try {
      body = (await res.json()) as LedgerBody;
    } catch {
      // Not JSON: hand the response through untouched. This spec is about a
      // body that IS a ledger page.
      await route.fulfill({ response: res });
      return;
    }
    // THE BASE ALWAYS CARRIES ROWS, and that is a determinism requirement, not a
    // convenience: a daemon with an empty ledger legitimately answers total 0, and
    // the card then renders its empty paragraph WITHOUT ever evaluating the parts
    // these fixtures are about -- so the pre-fix crash path would not even be
    // reachable and the guard would silently stop guarding. Live rows are used
    // whenever there are any; otherwise the consistent three-row page above is the
    // base. The defect under test stays the ONLY difference from a real response.
    // A COPY, always: the fixtures MUTATE what they are handed, and the
    // synthetic page above is module state shared by every test in this worker.
    // Damaging it in place leaked one case's defect into the next one's body
    // (measured: a drop_totals run left the shared page without totals, so a
    // later case was served an already-damaged base and reported the wrong
    // defect) -- which is exactly the kind of order-dependence a guard must not
    // have.
    const base =
      Array.isArray(body.entries) && body.entries.length > 0
        ? body
        : (JSON.parse(JSON.stringify(SYNTHETIC_PAGE)) as LedgerBody);
    if (edit) edit(base);
    await route.fulfill({ response: res, json: base });
  });
  page.on("pageerror", (e) => errors.push(String(e)));
  await page.goto("/#settings");
  // Wait for the card's own read to land, TOLERANTLY. On a white screen there is
  // nothing to wait for, and this helper must not be what decides the verdict:
  // the assertions in the test body are, in an order where the uncaught error is
  // reported FIRST (it names the defect), then the missing mount, then the
  // missing diagnostic. A synchronous `expect(...).toBeVisible()` here would
  // turn every red into "the card is not there" and hide the reason.
  await page
    .locator(".distill-ledger .zone-note")
    .waitFor({ state: "visible", timeout: 12_000 })
    .catch(() => undefined);
}

for (const d of DAMAGING) {
  test("ledger " + d.case + ": the card reports it instead of white-screening #settings", async ({
    page,
  }) => {
    const errors: string[] = [];
    await openDamaged(page, errors, d.edit);

    // (a) THE WHITE SCREEN ALWAYS CAME WITH AN UNCAUGHT ERROR. This is the
    // assertion that fails hardest on a regression: it is the defect itself.
    expect(errors, "a contradictory ledger response must not throw").toEqual([]);

    // (b) the app is still mounted. A white screen leaves #root with no text at
    // all (measured: length 0 on the pre-fix revision).
    const rootText = await page.locator("#root").innerText();
    expect(rootText.length).toBeGreaterThan(100);

    // (c) the diagnostic, with the numbers it actually observed.
    const notice = page.locator("[data-ledger-inconsistent]");
    await expect(notice).toBeVisible();
    await expect(notice).toHaveAttribute("data-ledger-inconsistent", d.defect);
    const detail = (await notice.getAttribute("data-ledger-detail")) ?? "";
    expect(detail).toMatch(d.detail);

    // ... and the piece the defect made unusable is NOT fabricated: no bucket
    // chips for a bucket map nobody can read, no rows for a page that is not an
    // array. A 0 there would be a reading this response never contained.
    if (d.defect === "by_status_missing") {
      await expect(page.locator("[data-bucket]")).toHaveCount(0);
    }
    if (d.defect === "entries_missing") {
      // An EMPTY antd table still renders a placeholder row, so the data rows
      // are counted by their own class: this asserts "no ROW was fabricated",
      // not "the table element is gone".
      await expect(page.locator(".distill-ledger table tbody tr.ant-table-row")).toHaveCount(0);
    }
  });
}

test("ledger clean control: the daemon's own response renders with no notice", async ({ page }) => {
  const errors: string[] = [];
  let served: LedgerBody | null = null;
  await page.route(LEDGER_ROUTE, async (route) => {
    const res = await route.fetch();
    // A fresh daemon (CI boots one) may have an empty ledger: then the row and
    // bucket renderings cannot be exercised from live data, so the page served
    // to the card is the consistent fixture above. Either way the assertions
    // below are made against the body that was actually served.
    let body: LedgerBody;
    try {
      body = (await res.json()) as LedgerBody;
    } catch {
      body = SYNTHETIC_PAGE;
    }
    const hasRows = Array.isArray(body.entries) && body.entries.length > 0;
    const out = hasRows ? body : SYNTHETIC_PAGE;
    // The SECOND read asks for the most recent failure; answer it with the empty
    // page so the failure line cannot contradict the page above it.
    const onlyFailed = new URL(route.request().url()).searchParams.get("status") === "failed";
    const json = onlyFailed ? EMPTY_PAGE : out;
    if (!onlyFailed) served = out;
    await route.fulfill({ response: res, json });
  });
  page.on("pageerror", (e) => errors.push(String(e)));
  await page.goto("/#settings");
  await expect(page.locator(".distill-ledger")).toBeVisible();
  await expect(page.locator(".distill-ledger .zone-note")).toBeVisible();

  expect(errors, "a consistent response must render without throwing").toEqual([]);
  await expect(page.locator("[data-ledger-inconsistent]")).toHaveCount(0);

  const body = served as LedgerBody | null;
  expect(body, "the spec must know which body it served").not.toBeNull();
  const entries = Array.isArray(body?.entries) ? body.entries.length : 0;
  expect(entries).toBeGreaterThan(0);
  await expect(page.locator(".distill-ledger table tbody tr.ant-table-row")).toHaveCount(
    Math.min(entries, 20),
  );
  await expect(page.locator("[data-bucket]")).toHaveCount(4);
});

test("ledger clean control: a consistent three-row page renders three rows and four buckets", async ({
  page,
}) => {
  const errors: string[] = [];
  await page.route(LEDGER_ROUTE, async (route) => {
    const onlyFailed = new URL(route.request().url()).searchParams.get("status") === "failed";
    const json = onlyFailed ? EMPTY_PAGE : SYNTHETIC_PAGE;
    await route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify(json) });
  });
  page.on("pageerror", (e) => errors.push(String(e)));
  await page.goto("/#settings");
  await expect(page.locator(".distill-ledger .zone-note")).toBeVisible();

  expect(errors).toEqual([]);
  await expect(page.locator("[data-ledger-inconsistent]")).toHaveCount(0);
  await expect(page.locator(".distill-ledger table tbody tr.ant-table-row")).toHaveCount(3);
  await expect(page.locator("[data-bucket]")).toHaveCount(4);
  // The invariant the card prints beside its refusal readings: the buckets count
  // the same rows total does. A card that rendered 4 chips from another set
  // would still show 4 chips, so the numbers are read, not the count.
  const buckets = await page.locator("[data-bucket]").allInnerTexts();
  const sum = buckets.reduce((a, t) => a + (Number(t.replace(/\D+/g, "")) || 0), 0);
  expect(sum).toBe(3);
});
