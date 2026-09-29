# E2E 夹具假设：`failure-visibility.spec.ts` 不再依赖操作者的活配置（t109）

- 单号：**t109**（work）· attempt 1 · 文件：`panel/e2e/failure-visibility.spec.ts`（唯一实现改动）
- 触发：`5d3adfd` 的 E2E 第一次完整判定（run `36485138690`）；captain 补充第二次 **`e3a58ea`** 的读数为 **4 failed**（集合不稳定）

## 1 CI 原文（三条/四条同一形状）

```
1) e2e/failure-visibility.spec.ts:158:1 › P3: a failed mount probe says so on the CHIP…
   Error: expect(locator).toBeVisible() failed / Error: element(s) not found
   > 163 |   await expect(cardOf(page, "claude")).toBeVisible({ timeout: 15_000 });
2) :206 › P3 success control: a real 0 models is 'not probed', not an error, and byte-stable
   > 217 |   await expect(cardOf(page, "claude")).toBeVisible({ timeout: 15_000 });
3) :238 › F2 (t103): a successful retry after a failed mount probe CLEARS the failure
   > 244 |   await expect(card).toBeVisible({ timeout: 15_000 });
4) :260 › F1 (t103): with EVERY probe failed the models readout carries no number   ← 第二次才出现
```

## 2 根因：**证实**，且比原假设更糟

1. **`claude` 根本不是一个 runtime 名** —— 本机活守护进程 `GET /api/v1/agents` 的 10 个条目里，runtime 只有 `claude-code` / `dsh` / `opencode`（kinds 实测 `{role, runtime}`）。spec 用的是 `cardOf(page,"claude") = locator(".agent-card").filter({ hasText: name })` ⇒ **按子串命中 `claude-code`**。**所以本地「绿」从一开始就是巧合**，不是「操作者配了 claude」。
2. **CI 的夹具是 mock agents**（captain 扫出的 `e2e.yml:85-102`：`[agent.alpha]`/`[agent.beta]` + `judge`，无 `claude-code`）⇒ 子串也命中不了 ⇒ `element(s) not found`。
3. **失败条数不稳定（3→4）**：`F1` 在第一次通过、第二次失败 ⇒ 这些断言**隐式依赖同一 spec 里前序用例留下的探测状态/夹具差异**，不是超时（CI 步骤耗时正常）。⇒ 判据不能是「把 N 条改绿」，必须是「**整个 spec 与夹具自足**」。

**本地复现（一手，干净守护进程，非 pid 79984）**：同一份**改前** spec 在「带 mock runtime 的干净守护进程」和「无 runtime 的干净守护进程」上都 **`4 failed / 2 passed / exit 1`**，报错逐字 `Error: element(s) not found` ⇒ 与 CI 同形。

## 3 修法：**自带发现**（选项 b 的等价物），不写死名字

- 新增 `beforeEach`：`GET /api/v1/agents` 取 `kind === "runtime"` 的第一个名字 ⇒ `RT`；三条/四条断言全部改成 `RT`（`cardOf/cardText/inCardAlerts/循环列表/200 响应体` 共 17 处）。**没有再出现 `claude` 字面量**（只剩注释与历史说明）。
- **daemon 没有任何 runtime ⇒ `test.skip()` 并带原因**（点名它不再假设操作者配置、并打印它实际看到的 `agents: [name:kind…]`）；**绝不静默通过**。
- **不改其它 spec、不改 `panel/src/**`、不要求 CI 造一个叫 `claude` 的 agent**（captain 2026-09-29 的两条边界消息）。

## 4 成对读数（干净守护进程 + 临时端口 8843 / 8844，私有 `-TargetDir` 构建的二进制）

| 情形 | 读数 |
| --- | --- |
| **改前**，带 mock runtime（`agents=mock:runtime`） | **`4 failed / 2 passed` / exit 1**，`Error: element(s) not found`（= CI 形状） |
| **改前**，无 runtime（`agents=` 空） | **`4 failed / 2 passed` / exit 1**（同上） |
| **改后**，带 mock runtime，**run 1** | **`6 passed` / exit 0**（9.2s） |
| **改后**，带 mock runtime，**run 2** | **`6 passed` / exit 0**（8.1s） ⇒ **连续两次失败集合相同（都为空）** |
| **改后**，无 runtime（负控 1） | **`6 skipped` / exit 0**（带原因，非静默通过） |
| **改后**，无 runtime + `RUAGENT_E2E_ALLOW_WRITES=0`（负控 2） | **`6 skipped` / exit 0**（同上） |

- 夹具端 `GET /api/v1/agents`：A = `mock:runtime`；B = 空。全程 `E2E_BASE_URL` 显式给出（t107 的闸门），**没有一次跑在本机活守护进程上**。
- `git diff --numstat -- panel/e2e/failure-visibility.spec.ts` = **`49 17`**（只此一个实现文件）。

## 5 能红的负控（本单要求的形状）

1. **回到旧行为 ⇒ 必须红**：同一干净守护进程上跑改前 spec ⇒ **4 failed / exit 1**（CI 同形）；改后同环境 ⇒ 6 passed。
2. **没有夹具 ⇒ 不许静默通过**：无 runtime 的守护进程上 ⇒ **6 skipped**，原因字符串写明「this daemon exposes no runtime to probe … must not assume the operator's live config」。
3. **连续两次** ⇒ 失败集合稳定为空（见上表 run 1/run 2）。

## 6 不覆盖什么（第 19 条）

- **只覆盖面板「三态显示」这件事**：模型 chip 的失败/成功/未探测三态、汇总条的第三态、召回日志失败态与 R11 alert 的区别；**不覆盖**探测的真实网络行为、`agentOptions` 的重试语义、模型选项内容、其它视图的失败可见性。
- **不含写型夹具**：本单**没有**创建/删除任何 runtime 或 agent（选择「发现」而非「建夹具」）⇒ **不覆盖**「CI 夹具能把面板三态走通」这件事本身；若 CI 的 daemon 恰好没有任何 runtime，这条 spec 会 **skip**（有原因），此时 e2e 的绿**不包含**本 spec 的覆盖 —— **这是本单的已知代价，写在明处**。
- **不覆盖** `claude-code` 这类**真实 harness** 的探测路径（只发现「第一个 runtime」，不区分 harness 种类）。
- 本单**不动** `panel/src/**`、crates、workflows；`clippy`/workspace 测试与它无关，未跑也未声称。
