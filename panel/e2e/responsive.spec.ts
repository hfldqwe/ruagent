// Responsive shell (2026-09-19 UX audit): the sider collapses to an icon
// rail below lg, and no view overflows the viewport sideways. The audit
// measured a 228px sider leaving 162px of content at 390px, and grid
// tracks floored at content min-content blowing 768px viewports out to
// 1087px. These assertions pin both regressions. Data-independent —
// passes on the empty CI daemon and a lived-in one alike.

import { expect, test } from "@playwright/test";

const noOverflow = async (page: import("@playwright/test").Page) => {
  const over = await page.evaluate(
    () =>
      document.documentElement.scrollWidth -
      document.documentElement.clientWidth,
  );
  expect(over).toBeLessThanOrEqual(2);
};

/** t164: the sider's rendered width, as something to POLL. The shell's layout is
 *  the fact these tests are about; how long it takes to settle is exactly what a
 *  fixed `waitForTimeout` before each read was guessing at.
 *
 *  The miss value is `NaN` on purpose, and it has to be chosen against the
 *  PREDICATES: the reads this replaces were `expect(sider?.width).toBeLessThanOrEqual(80)`,
 *  where a missing sider gave `undefined <= 80` == false (a failure, as it should
 *  be). A sentinel of `-1` would have made `-1 <= 80` true and quietly turned that
 *  failure into a PASS -- a weaker test than the one it replaced. `NaN` compares
 *  false against every predicate here, so "no sider yet" keeps polling and its
 *  exhaustion fails loudly. */
const railWidth = (page: import("@playwright/test").Page) =>
  page
    .locator(".ant-layout-sider")
    .first()
    .evaluate((el) => el.getBoundingClientRect().width)
    .catch(() => Number.NaN);

test.describe("phone (390px)", () => {
  test.use({ viewport: { width: 390, height: 844 } });

  for (const route of [
    "home",
    "board",
    "chat",
    "sessions",
    "knowledge",
    "memory",
    "agents",
    "runtimes",
    "graph",
  ]) {
    test(`${route} fits the viewport`, async ({ page }) => {
      await page.goto(`/#${route}`);
      // t164 (was `waitForTimeout(600)`): wait for the FACT -- the rail collapsed.
      await expect
        .poll(() => railWidth(page), {
          timeout: 5_000,
          message: `${route} at 390px: the sider never collapsed to the icon rail (width <= 80px)`,
        })
        .toBeLessThanOrEqual(80);
      await noOverflow(page);
    });
  }

  // The chat history rail is a slide-over here: the view-bar toggle
  // opens it, tapping beside it closes.
  test("chat history rail opens and closes as a slide-over", async ({
    page,
  }) => {
    await page.goto("/#chat");
    // t164 (was `waitForTimeout(600)`): the fact is the toggle existing; the
    // click's own actionability contract (visible/stable/enabled/receives events)
    // covers the rest of what the fixed wait was guessing at.
    await expect(page.locator(".chat-side-toggle")).toBeVisible();
    await page.locator(".chat-side-toggle").click();
    await expect(page.locator(".chat-side")).toHaveClass(/open/);
    await expect(page.locator(".chat-side-backdrop")).toBeVisible();
    await page.mouse.click(360, 400); // right of the 300px rail
    await expect(page.locator(".chat-side")).not.toHaveClass(/open/);
  });
});

test.describe("tablet (768px)", () => {
  test.use({ viewport: { width: 768, height: 1024 } });

  test("home and board fit with the rail collapsed", async ({ page }) => {
    for (const route of ["home", "board"]) {
      await page.goto(`/#${route}`);
      // t164 (was `waitForTimeout(600)`): same fact as the 390px case.
      await expect
        .poll(() => railWidth(page), {
          timeout: 5_000,
          message: `${route} at 768px: the sider never collapsed to the icon rail (width <= 80px)`,
        })
        .toBeLessThanOrEqual(80);
      await noOverflow(page);
    }
  });
});

test.describe("desktop", () => {
  test.use({ viewport: { width: 1280, height: 800 } });

  test("sider expands at lg and the footer toggle collapses it", async ({
    page,
  }) => {
    await page.goto("/#home");
    // t164 (was `waitForTimeout(600)`): poll for the full-width sider itself. The
    // predicate is the same one the fixed wait used to precede (`=== 228`), so the
    // assertion is unchanged -- only the "how long does it take" guess is gone.
    await expect
      .poll(() => railWidth(page), {
        timeout: 5_000,
        message: "desktop #home: the sider never reached its full 228px width",
      })
      .toBe(228);

    // Manual toggle: desktop users get the rail too. Since S1 (t133) the
    // accessible name is state-specific — "收起侧栏" while expanded, "展开侧栏"
    // while collapsed — so the selector accepts either direction.
    await page
      .getByRole("button", { name: /collapse|expand|收起|展开/i })
      .first()
      .click();
    // t164 (was `waitForTimeout(300)`): the collapse is a state change, so poll
    // for the state -- the same `<= 80` predicate as before.
    await expect
      .poll(() => railWidth(page), {
        timeout: 5_000,
        message: "the sider never collapsed to the icon rail after the footer toggle",
      })
      .toBeLessThanOrEqual(80);
    // The brand mark survives the rail; the full name does not.
    await expect(page.locator(".brand-mark")).toBeVisible();
    await expect(page.locator(".brand-name")).toBeHidden();

    // Toggle back — state persists across a route change.
    await page
      .getByRole("button", { name: /collapse|expand|收起|展开/i })
      .first()
      .click();
    // t164 (was `waitForTimeout(300)`): the wait is gone entirely -- the assertion
    // below is already a bounded, retrying read of the fact it was waiting for.
    await expect(page.locator(".brand-name")).toBeVisible();
  });
});
