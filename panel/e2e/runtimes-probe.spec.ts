// t71: the runtime card's two DIAGNOSTIC states have no other coverage.
//
// WHAT THEY ARE (t66). The card answers "can this command actually START here?"
// with three states -- resolved / notFound / unknown (panel/src/views/Runtimes.tsx
// `commandState`, because an absent `command_resolved` is NOT "true") -- and next
// to it a "测试连接" button asks the DAEMON (real spawn + ACP handshake) via
// POST /api/v1/runtimes/{name}/probe, whose answer the card renders as
// ok + stage (a closed set: not-found / spawn-failed / handshake-failed / ok)
// + detail (the daemon's own sentence). Both states were added without any e2e:
// the panel has no Rust gate behind it, so "written, unverified" was the honest
// description until this file.
//
// WHAT THIS FILE IS: READ-ONLY. It never creates, edits or deletes a runtime, so it
// takes no writeAccess() gate and leaves nothing to clean up (contrast
// e2e/registry.spec.ts, which writes agents.toml and therefore must). Two
// techniques are used, and each test says which one it uses:
//   * a SYNTHETIC /api/v1/agents answer (real body fetched, agents array replaced)
//     makes the three command states deterministic on any machine -- a daemon's
//     seed may resolve everything, or nothing, depending on what is installed;
//   * the LIVE daemon's own entries, for the tests whose point is that the REAL
//     probe route answers: they pick a runtime by the polarity the daemon reports
//     and SKIP with a reason when it reports none, rather than inventing one.
//     ONE display flag is flipped for the real-probe test, and only in the fetched
//     RESPONSE: the grid renders only `enabled` runtimes, and the seeded runtime
//     whose command does not resolve is disabled by default -- so the card is made
//     reachable by answering `enabled: true` for that one row, while the PROBE
//     POST is left untouched and reaches the daemon. Enabling it on disk would be a
//     write to agents.toml, which is a different spec's job (registry.spec.ts, with
//     writeAccess() and a cleanup); this file changes no config.
//
// NO REAL AGENT IS STARTED. The probe is only ever clicked on a runtime whose
// command does NOT resolve, so the daemon stops at not-found before spawning
// anything; a resolvable runtime is checked by RENDERING only. That is why no
// assertion here claims a reachable runtime is reachable -- proving ok would mean
// running a real harness, which this suite must not do.
//
// THE DOM CARRIES THE FACTS, so the assertions do not depend on prose:
//   [data-command-state=resolved|notFound|unknown]   the card's command reading
//   [data-probe-state=running|answer|error]          what the probe is doing
//   [data-probe-ok=true|false] [data-probe-stage=…] [data-probe-detail]
//   [data-fleet-verdict=none|partial|unknown|ok]     the PAGE-level verdict banner
//   [data-fleet-total=N] [data-fleet-unresolved=N]   the counts it was computed from
// The stage token is rendered VERBATIM (untranslated, untruncated) -- the property
// the second test protects: the front end must not soften the daemon's word.

import { expect, test, type Page } from "@playwright/test";

const AGENTS = "**/api/v1/agents**";
const PROBE = "**/api/v1/runtimes/*/probe";

/** One row shaped like GET /api/v1/agents returns it. Only the fields the card
 *  reads are meaningful here; the rest keep the page's other widgets quiet. */
function runtime(over: Record<string, unknown>): Record<string, unknown> {
  return {
    id: "e2e-t71-0000-0000-0000-000000000000",
    kind: "runtime",
    harness: "Mock",
    description: "e2e fixture (never written to any config)",
    models: [],
    reasoning_effort: null,
    model: null,
    enabled: true,
    primary: false,
    runtime: null,
    runtimes: [],
    options: {},
    prompt: null,
    command: null,
    command_path: null,
    ...over,
  };
}

/** Serve a SYNTHETIC agents list: the real response is fetched first (so every
 *  other field and every other route stays the daemon's), then the array is
 *  replaced. Pathname-checked, because the page also talks to /agents/... */
async function serveAgents(
  page: Page,
  agents: Record<string, unknown>[] | (() => Record<string, unknown>[]),
): Promise<void> {
  await page.route(AGENTS, async (route) => {
    const req = route.request();
    if (req.method() !== "GET" || new URL(req.url()).pathname !== "/api/v1/agents") {
      await route.continue();
      return;
    }
    const res = await route.fetch();
    const body = (await res.json()) as Record<string, unknown>;
    // A thunk lets one test serve a DIFFERENT grid to each load (reload) without
    // re-registering the route -- which is how the three page-level verdicts are
    // driven from the same page.
    const list = typeof agents === "function" ? agents() : agents;
    await route.fulfill({ response: res, json: { ...body, agents: list } });
  });
}

/** Open the runtimes page (the suite's baseURL is the daemon under test). */
async function openRuntimes(page: Page): Promise<void> {
  await page.goto("/#runtimes");
  await expect(page.locator(".agent-grid")).toBeVisible();
}

/** The card of one runtime, found by the alias the card prints under the harness. */
function cardOf(page: Page, name: string) {
  return page.locator(".agent-card").filter({ hasText: name }).first();
}

test("a runtime whose command does not resolve says so -- not 0 models, not blank", async ({
  page,
}) => {
  // SYNTHETIC: this is a RENDERING contract, and a daemon may resolve everything on
  // a machine that has the tools installed. The reading must hold either way.
  const name = "e2e-unresolved";
  await serveAgents(page, [
    runtime({
      name,
      command: "definitely-not-installed-xyz --acp",
      command_resolved: false,
      command_path: null,
    }),
  ]);
  await openRuntimes(page);

  const card = cardOf(page, name);
  await expect(card).toBeVisible();

  // 1. the state is its own, on a hook rather than on a phrase
  await expect(card.locator("[data-command-state]")).toHaveAttribute(
    "data-command-state",
    "notFound",
  );
  // 2. and it is ON SCREEN (zh locale, see playwright.config.ts)
  await expect(card).toContainText("找不到命令");

  // 3. NOT a model count. A card reading "0 个模型" says the opposite of the truth:
  //    nobody asked this runtime anything.
  await expect(card).not.toContainText("0 个模型");

  // 4. and not blank: the card keeps its identity and the button that asks the
  //    daemon for the real reason.
  const text = (await card.innerText()).trim();
  expect(text.length).toBeGreaterThan(20);
  await expect(card).toContainText("测试连接");
});

test("测试连接 shows the daemon stage and detail VERBATIM, not a friendlier word", async ({
  page,
}) => {
  // SYNTHETIC answer in the daemon's exact shape (RuntimeProbeResult): the point of
  // this test is how a stage token and a detail sentence are RENDERED, so the answer
  // is pinned instead of depending on which binary this machine happens to have.
  const name = "e2e-probe";
  const detail = "PATH searched for definitely-not-installed-xyz: no such file (e2e marker)";
  await serveAgents(page, [
    runtime({
      name,
      command: "definitely-not-installed-xyz --acp",
      command_resolved: false,
    }),
  ]);
  await page.route(PROBE, async (route) => {
    if (route.request().method() !== "POST") {
      await route.continue();
      return;
    }
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({ ok: false, command_path: null, stage: "not-found", detail }),
    });
  });
  await openRuntimes(page);
  const card = cardOf(page, name);

  // The probe is a click on the card's own button, not a page-level action.
  await card.getByRole("button", { name: "测试连接" }).click();

  // The answer arrived (not the request-failed branch, not a stuck spinner).
  const answer = card.locator("[data-probe-state]");
  await expect(answer).toHaveAttribute("data-probe-state", "answer");
  // ok:false is the DAEMON saying the runtime cannot start -- it arrives with HTTP
  // 200 and must not be dressed up as a successful answer.
  await expect(card.locator("[data-probe-ok]")).toHaveAttribute("data-probe-ok", "false");
  await expect(card).toContainText("接不通");

  // THE PROPERTY: the stage token is the daemon's, verbatim. A front end that mapped
  // it to a friendlier word would fail right here, by design.
  await expect(card.locator("[data-probe-stage]")).toHaveAttribute(
    "data-probe-stage",
    "not-found",
  );
  await expect(card.locator("[data-probe-stage]")).toHaveText("not-found");

  // ... and so is the detail, the only part that says WHY.
  await expect(card.locator("[data-probe-detail]")).toContainText(detail);
});

test("a resolvable runtime is NOT marked as a missing command", async ({ page }) => {
  // SYNTHETIC pair, side by side: the false alarm this guards against is a card that
  // shows the not-found badge for a command the daemon DID resolve.
  await serveAgents(page, [
    runtime({
      name: "e2e-resolved",
      command: "npx some-agent --acp",
      command_resolved: true,
      command_path: "C:/tools/npx.cmd",
    }),
    runtime({
      name: "e2e-unresolved-2",
      command: "definitely-not-installed-xyz --acp",
      command_resolved: false,
    }),
  ]);
  await openRuntimes(page);

  const resolved = cardOf(page, "e2e-resolved");
  await expect(resolved.locator("[data-command-state]")).toHaveAttribute(
    "data-command-state",
    "resolved",
  );
  await expect(resolved).toContainText("命令可解析");
  await expect(resolved).not.toContainText("找不到命令");

  // The neighbouring card still reports its own truth: the states are per card.
  await expect(cardOf(page, "e2e-unresolved-2").locator("[data-command-state]")).toHaveAttribute(
    "data-command-state",
    "notFound",
  );
});

test("the page-level verdict tells all-unavailable, partial and all-clear apart", async ({
  page,
}) => {
  // SYNTHETIC, and the point is the DIFFERENCE between the three states: several red
  // badges and no summary was the report this banner answers, so the banner must say
  // "none of them" differently from "some of them" -- computed, not guessed.
  const unresolved = (name: string) =>
    runtime({ name, command: "definitely-not-installed-xyz --acp", command_resolved: false });
  const ok = (name: string) =>
    runtime({
      name,
      command: "npx some-agent --acp",
      command_resolved: true,
      command_path: "C:/tools/npx.cmd",
    });
  let list: Record<string, unknown>[] = [unresolved("e2e-f1"), unresolved("e2e-f2")];
  await serveAgents(page, () => list);
  await openRuntimes(page);
  const banner = page.locator("[data-fleet-verdict]");

  // (1) NOTHING can start -- error tone, and it says what to DO next.
  await expect(banner).toHaveAttribute("data-fleet-verdict", "none");
  await expect(banner).toHaveAttribute("data-fleet-total", "2");
  await expect(banner).toHaveAttribute("data-fleet-unresolved", "2");
  await expect(banner).toContainText("没有任何运行时能启动");
  await expect(banner).toContainText("安装对应的命令行工具");
  await expect(banner).toContainText("command 改成二进制全路径");

  // (2) SOME can start -- warning tone, and the count is the daemon's.
  list = [unresolved("e2e-f1"), ok("e2e-f2"), ok("e2e-f3")];
  await page.reload();
  await expect(banner).toHaveAttribute("data-fleet-verdict", "partial");
  await expect(banner).toHaveAttribute("data-fleet-total", "3");
  await expect(banner).toHaveAttribute("data-fleet-unresolved", "1");
  await expect(banner).toContainText("找不到命令");

  // (3) ALL can start -- and the banner says so instead of staying silent (a silent
  // page cannot be told apart from one whose check did not run).
  list = [ok("e2e-f1"), ok("e2e-f2"), ok("e2e-f3")];
  await page.reload();
  await expect(banner).toHaveAttribute("data-fleet-verdict", "ok");
  await expect(banner).toHaveAttribute("data-fleet-unresolved", "0");
  await expect(banner).toContainText("全部可启动");
});

test("an empty grid is guidance, not a blank page", async ({ page }) => {
  // SYNTHETIC empty registry: what a brand-new install looks like before anything is
  // configured. The card grid is gone here, so this test navigates on its own rather
  // than through openRuntimes (which waits for the grid).
  await serveAgents(page, []);
  await page.goto("/#runtimes");

  const empty = page.locator(".empty-state");
  await expect(empty).toBeVisible();
  await expect(empty.locator(".empty-title")).toHaveText("没有注册的运行时");
  // ... with the "where do I define one" guidance and a way to act on it.
  await expect(empty.locator(".empty-hint")).toContainText("agents.toml");
  await expect(empty).toContainText("新建运行时");
  // ... and NO verdict banner: there is nothing to judge, and two overlapping
  // "you have nothing" messages would be worse than one.
  await expect(page.locator("[data-fleet-verdict]")).toHaveCount(0);
});

test("LIVE daemon: the real probe on an unresolvable runtime stops at not-found", async ({
  page,
}) => {
  // NO INTERCEPTION. This is the end-to-end half: the daemon's real seed, its real
  // command_resolved, and its real POST /probe answer. Zero cost by construction --
  // a command that does not resolve is never spawned.
  const res = await page.request.get("/api/v1/agents");
  expect(res.ok()).toBeTruthy();
  const body = (await res.json()) as { agents?: Array<Record<string, unknown>> };
  const listed = body.agents ?? [];
  const unresolved = listed.find((a) => a.kind === "runtime" && a.command_resolved === false);
  test.skip(
    unresolved == null,
    "this daemon reports no runtime whose command does not resolve (every runtime it knows resolves here), so the real not-found path has nothing to answer; the rendering half is covered synthetically above",
  );
  const name = String(unresolved?.name);
  expect(name.length).toBeGreaterThan(0);

  // Make the card REACHABLE without touching any config: the grid renders only
  // enabled runtimes, and an unresolvable seed is typically disabled. Only this one
  // row's `enabled` is answered differently; the probe POST is NOT intercepted.
  await page.route(AGENTS, async (route) => {
    const req = route.request();
    if (req.method() !== "GET" || new URL(req.url()).pathname !== "/api/v1/agents") {
      await route.continue();
      return;
    }
    const got = await route.fetch();
    const live = (await got.json()) as { agents?: Array<Record<string, unknown>> };
    const agents = (live.agents ?? []).map((a) => (a.name === name ? { ...a, enabled: true } : a));
    await route.fulfill({ response: got, json: { ...live, agents } });
  });

  await openRuntimes(page);
  const card = cardOf(page, name);
  await expect(card).toBeVisible();
  await expect(card.locator("[data-command-state]")).toHaveAttribute(
    "data-command-state",
    "notFound",
  );

  await card.getByRole("button", { name: "测试连接" }).click();
  // Bounded by the suite's expect timeout (8 s): resolving a missing command is an
  // offline PATH lookup, so a slow round trip is the machine, not this assertion.
  await expect(card.locator("[data-probe-state]")).toHaveAttribute("data-probe-state", "answer");
  // THE READING: for a command that does not resolve the daemon's own token IS
  // not-found, and the card shows it unmodified.
  await expect(card.locator("[data-probe-stage]")).toHaveText("not-found");
  await expect(card.locator("[data-probe-ok]")).toHaveAttribute("data-probe-ok", "false");
  // ... with the daemon's own explanation, not an empty line.
  const detail = (await card.locator("[data-probe-detail]").innerText()).trim();
  expect(detail.replace("原因：", "").trim().length).toBeGreaterThan(5);
});

test("LIVE daemon: a resolvable runtime is not marked as a missing command", async ({ page }) => {
  // NO INTERCEPTION, and NO PROBE: proving a runtime is reachable would spawn a real
  // agent (a real harness, real cost), which this suite must not do. The claim
  // checked here is the narrower one the card makes: a command the daemon RESOLVED is
  // not reported as missing.
  const res = await page.request.get("/api/v1/agents");
  const body = (await res.json()) as { agents?: Array<Record<string, unknown>> };
  // Only ENABLED runtimes get a card (the grid filters them), so only those can be
  // checked here; a disabled runtime is not a candidate to start at all.
  const resolved = (body.agents ?? []).filter(
    (a) => a.kind === "runtime" && a.command_resolved === true && a.enabled === true,
  );
  test.skip(
    resolved.length === 0,
    "this daemon resolves no enabled runtime command at all, so there is nothing that could be falsely reported as missing",
  );

  await openRuntimes(page);
  for (const r of resolved) {
    const card = cardOf(page, String(r.name));
    await expect(card).toBeVisible();
    await expect(card.locator("[data-command-state]")).toHaveAttribute(
      "data-command-state",
      "resolved",
    );
    await expect(card).not.toContainText("找不到命令");
  }
});
