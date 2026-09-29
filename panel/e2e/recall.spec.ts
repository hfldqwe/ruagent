// Recall chain (conservative): query → stubs → inline expand → full text
// visible, cross-checked against the memory API.
//
// t113: this spec used to end with an UNCONDITIONAL
//   test.skip(true, "recall matched no memories in this database");
// so on CI (a fresh root with no memories) it never executed at all -- a skip
// is not coverage. It now carries its OWN fixture: it writes one memory through
// the API under writeAccess(), recalls it through the panel, and deletes it
// again through the API in `finally` (cleanup is a WRITE too, so it is inside
// the same gate; the write-guard file explains why cleanup must not go through
// the UI). The old note that "doctor seeds at least one memory in CI" was wrong:
// nothing in the panel or the daemon seeds a memory, and the daemon's root in CI
// is a throwaway one.
//
// Where writes are disarmed the spec SKIPS BY NAME with the guard's reason --
// that is a declared skip, not the silent kind: it is the runner's own gate, and
// t110's evidence step names it.

import { expect, test } from "@playwright/test";
import { writeAccess } from "./write-guard";

/** Unique enough that no other row in any database can satisfy the query. */
const PROBE = "t113recallprobe";
/** A store/namespace pair the write matrix accepts (observation x user). */
const FIXTURE = {
  store: "observation",
  namespace: "user",
  content: `${PROBE} the kettle boils at 100C, and the deploy pipeline runs on Tuesday.`,
};

test("conservative recall stubs expand to the full memory", async ({ page, request }) => {
  const access = writeAccess();
  test.skip(!access.allowed, access.reason);

  // ── fixture: ONE memory, written through the API (not the UI) ────────────
  let id: number | null = null;
  const written = await request.post("/api/v1/memory/write", { data: FIXTURE });
  expect(written.ok(), `fixture write failed: ${written.status()} ${await written.text()}`).toBe(
    true,
  );
  // The write response is only an outcome (`{"outcome":"Inserted(1)"}`, measured
  // on a clean daemon) -- it carries no id. So the id comes from the SEARCH the
  // panel itself would use, which doubles as "the write is already visible"
  // before the UI is touched.
  const found = await request.get(`/api/v1/memory/search?q=${encodeURIComponent(PROBE)}`);
  expect(found.ok(), `fixture lookup failed: ${found.status()}`).toBe(true);
  const hits = ((await found.json()) as { hits?: Array<{ id?: number }> }).hits ?? [];
  id = hits[0]?.id ?? null;
  expect(id, "the fixture memory must be findable right after the write").toBeTruthy();

  try {
    await page.goto("/#memory");
    await page.locator(".ant-segmented-item-label").filter({ hasText: "召回" }).click();
    await page.locator(".ant-segmented-item-label").filter({ hasText: "保守" }).click();

    await page.locator(".search-bar input.ant-input").fill(PROBE);
    // antd auto-inserts a space between two CJK chars in buttons ("召回" → "召 回")
    await page.locator(".search-bar button").filter({ hasText: /召 ?回|Recall/ }).click();

    // wait for the result card (or the empty state) — count() is an instant
    // check and would race the recall API call
    await expect(page.locator(".recall-hit, .ant-empty").first()).toBeVisible({
      timeout: 10_000,
    });

    // memory stubs only (knowledge stubs show a document tag, entity stubs use
    // &id — only memory stubs carry #id). t113: with the fixture written above,
    // ZERO stubs is a FAILURE, not a reason to skip -- that is exactly what the
    // old `test.skip(true, …)` hid.
    const stubs = page.locator(".recall-stub-head").filter({ hasText: /#\d+/ });
    await expect(
      stubs.first(),
      `the fixture memory ${PROBE} must be recalled: ${await page
        .locator(".search-bar, .recall-hit, .ant-empty")
        .first()
        .textContent()}`,
    ).toBeVisible({ timeout: 10_000 });

    const stub = stubs.first();
    await expect(stub.locator(".stub-afford")).toBeVisible();

    // the stub carries #id — remember it for the API cross-check
    const headText = (await stub.textContent()) ?? "";
    const stubId = headText.match(/#(\d+)/)?.[1];
    expect(stubId, "stub exposes its memory id").toBeTruthy();

    // expand inline
    await stub.click();
    const expanded = page.locator(".recall-hit .recall-body").first();
    await expect(expanded).toBeVisible();
    const bodyText = (await expanded.textContent()) ?? "";
    expect(bodyText.length).toBeGreaterThan(10);

    // cross-check with the API — the expansion is memory_get, not a mock
    if (stubId) {
      const res = await request.get(`/api/v1/memory/${stubId}`);
      const { memory } = (await res.json()) as { memory: { content: string } };
      expect(bodyText.trim()).toContain(memory.content.trim().slice(0, 40));
    }

    // toggle collapses again (height animates to 0)
    await stub.click();
    await expect(page.locator(".recall-hit .recall-expand.open").first()).toHaveCount(0);
  } finally {
    // ── cleanup: API, not UI, and inside the very same gate ────────────────
    if (id !== null) {
      const del = await request.delete(`/api/v1/memory/${id}`);
      expect(del.ok(), `fixture cleanup failed: ${del.status()}`).toBe(true);
      // The panel's delete is a SOFT delete (measured: `GET /api/v1/memory/{id}`
      // still answers after it, so demanding a 404 here was my own wrong
      // assumption). The honest post-condition is the one the fixture itself
      // used to find the row: it is no longer LISTED and no longer SEARCHABLE,
      // and the raw status codes are logged for the report.
      const listed = await request.get("/api/v1/memory/list?namespace=user");
      const listedHits = ((await listed.json()) as { hits?: Array<{ id?: number }> }).hits ?? [];
      const search = await request.get(`/api/v1/memory/search?q=${encodeURIComponent(PROBE)}`);
      const searchHits = ((await search.json()) as { hits?: Array<{ id?: number }> }).hits ?? [];
      console.log(
        `DOM READING t113 cleanup: delete=${del.status()} list=${listed.status()} search=${search.status()} listed_ids=[${listedHits
          .map((h) => h.id)
          .join(",")}] search_ids=[${searchHits.map((h) => h.id).join(",")}]`,
      );
      expect(
        listedHits.some((h) => h.id === id),
        "the fixture memory must not be LISTED after cleanup",
      ).toBe(false);
      expect(
        searchHits.some((h) => h.id === id),
        "the fixture memory must not be SEARCHABLE after cleanup",
      ).toBe(false);
    }
  }
});
