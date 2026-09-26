# t304 — do t112 / t120 still hold? (clean environment, judged today)

Verifier: member `tools`, attempt 1. Conclusion only; no product file touched.

## Verdict

| record | today | basis |
| --- | --- | --- |
| **t112** (composer's model field is empty on a clean daemon; expected `mock-pro`) | **STILL HOLDS** | the agent card reads `"model": null` |
| **t120** (root cause: the ACP option probe returns an empty directory in a clean env) | **REFUTED** | the probe returns a **full** directory, `mock-pro`/`mock-max`, `current: mock-pro` |

**They are two faces of one thing, and the face that was named as the cause is the
one that is healthy.** The probe has the answer; the **card does not receive it**.
So the user-visible symptom in t112 is real, its stated root cause in t120 is not,
and the fix belongs where a probe result should be written into the card — not in
the probe.

## The clean environment

Object set: a daemon whose root contains nothing but the e2e mock registration.
Sampling plane: HTTP against my own instance; no user data, no 8787.
Falsifiable criterion: if the daemon's root were the real one, the cards would
carry the real agents' names, not `alpha`/`beta`/`judge`.

```
root : C:/tmp/t304clean          port: 127.0.0.1:8792       (independent of 8787)
shape: scripts/ruagent-canary.ps1  -> WMI Win32_Process.Create, ShowWindow = 0
       daemon pid read from <root>/data/daemon.pid  (the script's own comment: the
       WMI wrapper pid is not the daemon's; that mistake cost an orphan before)
copied from .github/workflows/e2e.yml, step "Register e2e mock agents" (lines 33-59):
  line 41-42  the two printf reply JSONs (e2e-alpha.json, e2e-beta.json)
  line 43-58  the agents.toml heredoc: [agent.alpha] / [agent.beta] / [agent.judge],
              harness = "mock", command = "<bin> --behavior scripted|judge ..."
  ONLY CHANGE: the workflow writes to $HOME/.ruagent; I wrote the same bytes to
              C:/tmp/t304clean/config/agents.toml (the acceptance requires a temp root)
```

## t112 — the agent cards on a clean daemon

```
GET http://127.0.0.1:8792/api/v1/agents
{"agents":[{"name":"alpha","harness":"Mock","models":[],"model":null,"options":{},"enabled":true,…},
           {"name":"beta", …same shape…},
           {"name":"judge", …same shape…}]}
```

**`"model": null`, `"models": []`, `"options": {}`** — the card's model field is
**empty**, and it is not `mock-pro`.

**t112 holds.** One correction of detail, not of substance: the empty value is
`null`, not the empty string `""` that the record quotes. A UI that tests
`model === ""` and a UI that tests `!model` behave differently here, so the
literal is worth having right in the fix.

## t120 — the ACP option probe on the same clean daemon

```
GET http://127.0.0.1:8792/api/v1/agents/alpha/options          -> 200
{"agent":"alpha","options":[
   {"id":"model","category":"model",
    "choices":[{"value":"mock-pro","name":"mock-pro"},{"value":"mock-max","name":"mock-max"}],
    "current":"mock-pro"},
   {"id":"mode", …"ask"/"auto", "current":"ask"},
   {"id":"reasoning_effort", …"off"/"high"/"max", "current":"off"}],
 "cached":true,"updated_at":1790433140869}
```

**The probe returns a full directory, not an empty one.** `crates/acp/src/run.rs:263`
is reached and answers. **t120 as stated is refuted.**

The two readings together say something the two records separately do not: the
card endpoint (`api.rs:38` router, the card field around `api.rs:3468`) reports
`models: []` while the options endpoint on the *same* agent reports two model
choices. The probe is not the broken half.

## Minimal reproduction (for the fix task)

1. Temp root + the e2e mock block above; start the daemon on an independent port.
2. `GET /api/v1/agents` → every card has `model: null`, `models: []`, `options: {}`.
3. `GET /api/v1/agents/alpha/options` → the same agent has `model` choices
   `mock-pro`/`mock-max` with `current: mock-pro`.
4. The gap is between 2 and 3: a probe result that exists is not reflected in the
   card the composer reads.

## A second finding, about the tooling (not about t112/t120)

**`scripts/ruagent-canary.ps1 -Stop` did not stop my daemon, and said so in a way
that reads like success**:

```
$ … -Stop
canary: healthy (200) on 127.0.0.1:8792, daemon pid unknown, root C:\tmp\t304clean
$ curl -o /dev/null -w '%{http_code}' …/api/v1/health      -> 200     (still running)
$ rm -rf /c/tmp/t304clean
rm: cannot remove '…/data/ruagent.db': Device or resource busy
```

It printed "healthy" (true), "daemon pid unknown" (true), and stopped nothing. The
pid file in that root contains **two** fields —
`12468 1790433119510` — so a reader that expects one integer finds none.
I stopped my instance the way the discipline requires instead: verify the recorded
pid's command line contains my root, then `taskkill /PID 12468 /F`
(`SUCCESS`; 8792 then answered nothing, 8787 stayed 200, and the temp directory
deleted cleanly, which is itself the proof that nothing held it).

This is the same family the team has recorded twice today — a self-report that
does not track reality: the message says "pid unknown" where it should say "I
could not find the pid, so I stopped nothing", and a caller reading only the
"healthy" word would walk away with a live daemon. Recorded here because it is a
reading; whether it is worth a task is the captain's call.

## What I did NOT do (rule 5)

| not done | reason |
| --- | --- |
| touch 8787 | excluded; verified untouched (200) before and after |
| kill by name or port | excluded; only my recorded pid 12468, after checking its command line |
| leave the temp root | deleted; the successful `rm` is the proof nothing held it |
| judge t112/t120 from the live daemon | the whole point is that the live root is not clean |
