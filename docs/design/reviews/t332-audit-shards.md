# t332 审计闸门分段/续跑（F-t306-03）—— 结论 **pass**

任务：把 `node tools/design-audit.mjs --check` 的全量（26 captures / 702.2s，超过单次调用 600s 上限）拆成**可独立完成的分段**并支持**从断点续跑**，让任何人都能拿到闸门读数。inScope：`panel/tools/`。

## 交付物

**`panel/tools/audit-shards.mjs`**（新，包装器；不改 design-audit.mjs 的 CLI ⇒ `--help` 等既有用法一字未动）

```bash
node tools/audit-shards.mjs                    # 全部 13 段，默认续跑（跳过已完成段）
node tools/audit-shards.mjs --segments=routes  # 13 段（每段 1 路由 × 2 模式）★默认
node tools/audit-shards.mjs --segments=modes   # 2 段（dark / light）
node tools/audit-shards.mjs --only=chat        # 只跑某一段
node tools/audit-shards.mjs --reset            # 丢弃状态重来
node tools/audit-shards.mjs --detach           # 用 WMI 分离启动（见下）
node tools/audit-shards.mjs -- <额外的 design-audit 参数>
```

* **状态文件在仓库外**：`~/.ruagent/audit-shards-state.json`（`--state=PATH` 可改）。**刻意不落在 `panel/tools/`** —— 它是运行期续跑状态、不是产物，放仓里会被 `git add panel/tools/` 一起带走（队长 22:5x 提醒过这一点；我先前那一版默认路径确实写进了仓，已改掉并删除）。
* **聚合规则（关键，写在文件头与输出里）**：design-audit 自己的计数是**按行**的（一行在所有它被判的 capture 上通过才算 PASS）⇒ **各段的计数绝不能相加**。可比的量是：**失败行号的并集**、**未测行数的逐段清单**、**captures 总数**。「某行 FAIL」= 任一段 FAIL 它 —— 与一次跑完的语义一致。
* **半途而废的段永不等于通过**：`exit` 非 0/1 或没有汇总行 ⇒ 记为 `interrupted`/`failed`（`why` 写明原因），并在汇总里以 `INCOMPLETE (… NEVER a pass)` 与 `PENDING` 列出。
* **F-331b 已纳入**：连续 `AUDIT_EMPTY_CAPTURE_LIMIT`（默认 3）条 capture 报 `text=0` ⇒ 判定**仪器失效**、杀掉该段并记为 `instrument-failed`，而不是跑完再给一份空报告。
* **F-331c 已纳入**：包装器只杀**自己的子进程句柄**（`child.kill`）或**自己记下的 PID**；注释里写明禁止按命令行匹配清理 —— 本次实测就撞上了这条：我按 `design-audit` 匹配查孤儿，返回的 4 个 PID **全是我自己当次调用的 shell/powershell/node**（因为那条命令行里也含这个词）✗。

## 分段边界与每段实测耗时（`--segments=routes`）

边界 = **13 个路由 × 2 个模式 = 26 captures**，每段 1 个路由（两个模式一起），因为单路由 ≈ 2×27s。实测：

| 段 | 秒 | 段 | 秒 | 段 | 秒 |
| --- | --- | --- | --- | --- | --- |
| home | 54 | task | 64 | runtimes | 54 |
| chat | 62 | memory | 60 | stats | 55 |
| sessions | 61 | knowledge | 61 | settings | 61 |
| board | 60 | graph | 61 | inbox | 52 |

**最长 64s，远低于 600s 上限** ⇒ 每段都能在单次调用内完成。粗粒度 `--segments=modes`（dark/light，各 13 captures）实测 ≈ **471s**，也在上限内但余量小（本次就是被 470s 的 timeout 打断的，因此默认取 routes）。

## 分段跑完 vs 一次跑完（读数对比）

| | captures | 失败行 | 未测行 |
| --- | --- | --- | --- |
| 一次跑完（t306 attempt 3 的全量） | **26** | **{58}** | 2（工具汇总行） |
| 分段跑完（13/13，本单） | **26** | **{58}** | 逐段 4-9（`home` 7 · `chat` 5 · … · `inbox` 9；**逐段可见、不相加**） |

⇒ **结论相同**：captures 26 对 26，失败行 `[58]` 对 `[58]`（`chat` 与 `settings` 两段 exit=1，正是行 58 在这两条路由上的形态）。未测项**没有**被静默算成通过：每一段的 `N not measured` 都逐段打印并留在状态文件里。

## 反向：中途打断 + 从断点续跑

```bash
timeout 12 node tools/audit-shards.mjs --segments=routes --only=graph --reset
#  [shards] INCOMPLETE (ran but did not finish -- counts unknown, NEVER a pass): graph:interrupted(exit=143 and no summary line: counts unknown)
#  [shards] PENDING (not pass, not measured): graph:interrupted, home:todo, … , inbox:todo
#  状态文件: {"status":"interrupted","why":"exit=143 and no summary line: counts unknown","exitCode":143,"ms":12659,"pass":null,...}
node tools/audit-shards.mjs --segments=routes --only=graph        # 续跑该段
#  [shards] segments done 1/13 captures 2 NOT-MEASURED rows reported by segments: 4 (per-segment, do not sum)
```

⇒ 被打断的段**没有**变成通过（`pass: null`、`interrupted`、在 PENDING 里可见），续跑后该段正常完成，未测项（4）仍然可见 ✓。

## WMI 分离启动写成可复用参数（`--detach`）

```
=== 分离启动（--detach，WMI + ShowWindow=0）===
[shards] detached (ShowWindow=0, WMI):  · log=C:\Users\19410\Documents\ai\ruagent\panel\tools\.audit-shards-detached.log
[shards] the child outlives this shell; poll the log, then re-run without --detach to read the aggregate.
=== 分离运行的日志尾 ===
=== 状态文件里 home 的记录 ===
null
=== 该 PID 是否已退出（只看我记下的那个）===
plan: 13 done: ['graph']
```

窗口约束（写在文件头注释里，也在这里）：`Start-Process`/裸 `&` 的分离子进程会随启动它的 shell 一起死（实测：后台化的 `--check` 只跑完第一条路由就没了）；能活下来的是 **WMI 启动 + `ShowWindow = 0`**：
```powershell
\$cmd = 'cmd.exe /c cd /d <panel> && node tools\audit-shards.mjs … > <log> 2>&1'
\$s = ([wmiclass]'Win32_ProcessStartup').CreateInstance(); \$s.ShowWindow = 0
([wmiclass]'Win32_Process').Create(\$cmd, <panel>, \$s)
```
`ShowWindow = 0` **不是装饰**：不设它，WMI 子进程会在用户屏幕上摆一个控制台窗口直到扫描结束（这正是本仓库 AGENTS.md 里记过的同一条约束）。

## 结论

**pass** —— 分段（13 段，最长 64s）、续跑、半途段的可见性、分段与全量结论一致（captures 26 · 失败行 [58]）、分离启动的可复用路径，全部有实测读数。findings：

| id | severity | problem | requiredFix |
| --- | --- | --- | --- |
| F-t332-01 | low | 行 58 的失败在分段读数里落在 `chat`/`settings` 两段（exit=1）—— 与 t331 的 F-331a（探针点了已选中的 option ⇒ 状态不变 ⇒ hash 不变）吻合，属**判据侧探针 bug**，不是视图缺陷 | 按 t331 的修法（选 option 时跳过 `aria-selected=true`）修 design-audit 的 URL 状态探针 |
| F-t332-02 | low | `--segments=modes` 的 dark 段实测 ≈471s，离 600s 上限只剩 ~130s 余量；机器一慢就会被截断 | 默认保持 `routes`；若要用 `modes`，建议把 `--no-shots --no-pixels` 之类减负参数一并传入（包装器已支持 `-- <extra>` 透传） |

## 纪律

改动文件：`panel/tools/audit-shards.mjs`（新）+ 本报告（`docs/design/reviews/` 由任务 inScope 覆盖）· **未改 design-audit.mjs 的 CLI**（`--help` 原样）· 未碰 `panel/src/` · 状态文件在仓库外（仓内那份已删，`panel/tools/` 无 state 文件）· 提交用**显式路径**（不 `git add panel/tools/`）· 未 push · 未启停 daemon · 未调 `/api/v1/recall`。
