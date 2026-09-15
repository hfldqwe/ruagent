// Knowledge raw editor: open the modal, verify the textarea mirrors
// the markdown truth source. STRICTLY READ-ONLY in the UI — the
// editor's save button is never clicked, so the user's knowledge
// library is untouched.
//
// The fixture doc is created and deleted through the API (PUT raw +
// DELETE, which also removes the .md file) — idempotent on every run.

import { expect, test } from "@playwright/test";

const DOC = "e2e-editor-probe";
const CONTENT = "# E2E editor probe\n\nThe markdown truth source round-trips through the panel editor.";

let docId: number | null = null;

test.beforeAll(async ({ request }) => {
  const put = await request.put(`/api/v1/knowledge/raw/${DOC}`, {
    data: { content: CONTENT },
  });
  test.skip(!put.ok(), "knowledge raw API unavailable");
  const list = await request.get("/api/v1/knowledge/documents");
  const { documents } = (await list.json()) as {
    documents: { id: number; name: string }[];
  };
  docId = documents.find((d) => d.name === DOC)?.id ?? null;
});

test.afterAll(async ({ request }) => {
  if (docId != null) {
    await request.delete(`/api/v1/knowledge/documents/${docId}`);
  }
});

test("the raw editor shows the file's exact markdown", async ({ page }) => {
  test.skip(docId == null, "probe doc was not created");
  await page.goto("/#knowledge");

  const row = page.locator(".row-btn").filter({ hasText: DOC });
  await expect(row).toBeVisible();
  // file-backed: shows its source file and the edit affordance
  await expect(row.locator(".tag").filter({ hasText: `${DOC}.md` })).toBeVisible();

  await row.locator("button").filter({ hasText: /编 辑|Edit/ }).click();

  const textarea = page.locator(".ant-modal textarea.md-editor");
  await expect(textarea).toBeVisible();
  await expect(textarea).toHaveValue(CONTENT);

  // close WITHOUT saving — read-only discipline
  await page.locator(".ant-modal button").filter({ hasText: /取 消|Cancel/ }).click();
  await expect(page.locator(".ant-modal")).toHaveCount(0);

  // and the file is untouched
  const raw = await page.request.get(`/api/v1/knowledge/raw/${DOC}`);
  expect(await raw.text()).toBe(CONTENT);
});
