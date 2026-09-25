// Fan-out judge (design §5.2/§5.3): the full loop in the panel — fan out
// to two mock members, let a judge agent pick, see the verdict card, the
// AI provenance on the winner badge, and the rationale. Only runs when
// the e2e mock agents are registered (the workflow does; a personal
// daemon with real agents skips — the Rust integration test covers the
// flow there, without spending real agent runs).

import { expect, test } from "@playwright/test";

test("fan-out judge picks a winner with provenance", async ({ page, request }) => {
  // This test owns three assertion waits (20s cards + 20s judge-ready + 20s
  // verdict), so its own budget must exceed their sum: otherwise a failure
  // inside one of them is reported as an opaque 30s test timeout instead of
  // the specific precondition that broke. The happy path is fast (measured
  // 4.8s end to end with working mock agents), so this is a diagnosability
  // bound, not a workaround -- raising it alone does NOT make the test pass
  // (a 180s run still failed on the disabled judge button).
  test.setTimeout(60_000);
  const list = await request.get("/api/v1/agents");
  test.skip(!list.ok(), "agents API unavailable");
  const { agents } = (await list.json()) as {
    agents: { name: string; harness: string; enabled: boolean }[];
  };
  const mocks = agents.filter((a) => a.enabled && a.harness === "Mock");
  const judge = mocks.find((a) => a.name === "judge");
  const members = mocks.filter((a) => a.name !== "judge");
  test.skip(!judge || members.length < 2, "needs the e2e mock agents (2 members + judge)");
  // t190: test.skip() is a runtime decision, not a type guard, so TypeScript still
  // sees judge as possibly undefined below. This narrows it AND names the
  // precondition, instead of letting a missing judge surface as "cannot read
  // properties of undefined" somewhere later.
  if (!judge) return; // unreachable: test.skip above already ended the test.
  // Using return rather than throw keeps the outcome a SKIP: a throw would
  // turn a missing mock agent into a suite FAILURE, which is worse than a skip.

  // Scratch task; deleted in finally.
  const made = await request.post("/api/v1/tasks", {
    data: {
      title: "e2e judge fanout",
      // "FANOUT MARKER" keys the members' scripted replies.
      intent: "FANOUT MARKER: design the widget",
    },
  });
  test.skip(!made.ok(), "task creation unavailable");
  const task = (await made.json()) as { id: string };

  try {
    const fan = await request.post(`/api/v1/tasks/${task.id}/fanout`, {
      data: { agents: [members[0].name, members[1].name] },
    });
    test.skip(!fan.ok(), "fan-out unavailable");

    await page.goto(`/#task/${task.id}`);
    // Comparison cards appear once both members completed (2s polling).
    await expect(page.locator(".compare-card")).toHaveCount(2, { timeout: 20_000 });

    // The judge bar: pick the judge agent, start the review. (antd 6
    // Select: no .ant-select-selector — open via the combobox input.)
    await expect(page.locator(".judge-bar")).toBeVisible();
    await page.locator(".judge-bar .ant-select-input").click();
    await page.locator(".ant-select-item-option").filter({ hasText: judge.name }).click();

    // Wait for the judge button's OWN precondition, not a proxy for it.
    // canJudge (TaskDetail.tsx) turns true only once >=2 runs are completed
    // AND carry a non-empty result; the comparison cards above appear much
    // earlier (measured with working mock agents: cards at +455ms, button
    // enabled about 2s later). Clicking straight after the cards therefore
    // raced the panel -- Playwright retried a disabled button until the whole
    // 30s test budget was gone (element is not enabled -> Test timeout of
    // 30000ms exceeded). The flow itself is fast (4.8s end to end), so the
    // fix is to wait for the real precondition, not to widen the timeout.
    const judgeGo = page
      .locator(".judge-bar button")
      .filter({ hasText: /^开始评审$|^Judge$/ });
    await expect(judgeGo).toBeEnabled({ timeout: 20_000 });
    await judgeGo.click();

    // Verdict: ok tag with the AI pick + the mock's rationale.
    await expect(
      page.locator(".judge-bar .tag.ok").filter({ hasText: /AI 选优|AI pick/ }),
    ).toBeVisible({ timeout: 20_000 });
    await expect(page.locator(".judge-bar")).toContainText(
      "mock judge prefers the first candidate",
    );

    // The winner card: badge names the judge agent as the selector.
    const winnerCard = page.locator(".compare-card.selected");
    await expect(winnerCard).toBeVisible();
    await expect(winnerCard.locator(".tag.ok")).toContainText(judge.name);
  } finally {
    await request.delete(`/api/v1/tasks/${task.id}`);
    // The judge sub-task is its own task (design §5.3 everything-is-a-run);
    // deleting the parent removes the edge but not the judge task.
    const list = await request.get("/api/v1/tasks");
    if (list.ok()) {
      const { tasks } = (await list.json()) as { tasks: { id: string; title: string }[] };
      const judgeTask = tasks.find((t) => t.title === `judge: e2e judge fanout`);
      if (judgeTask) await request.delete(`/api/v1/tasks/${judgeTask.id}`);
    }
  }
});
