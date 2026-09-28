# 面板失败可见性（第 2 轮）：汇总第三态 + 探针闩锁 —— t103

- 单号：**t103**（work）· attempt 1 · 来源：`t102`（verify2）的 **F1 + F2**，两者已随 `0da0cb6` 进入 main
- inScope 实改：**`panel/src/views/Runtimes.tsx`** + 复用/扩展 **`panel/e2e/failure-visibility.spec.ts`**（t102 的产物，本单验收明确允许扩）
- 参照读数：`docs/design/reviews/gen3-panel-failure-visibility-verify.md` §4/§5（改前 `0a39e5b` 上同一 spec = **2 failed / 2 passed / exit 1**）

## 1 改了什么（三条，全部落在失败态分支）

| # | 位置（最终字节） | 改动 |
| --- | --- | --- |
| F1 | `Runtimes.tsx` 汇总条 `models` 的 `value` | 旧：`Object.entries(counts).filter(…).reduce(…, 0)` —— **空集合求和仍是 0**，于是「全部探测失败」与「真的 0 个模型」印同一个数字。新：**先判断「至少一个失败且没有一个成功」⇒ 给第三态文案 `t("runtimes.probeFailed")`**，否则只对**答复过**的探测求和（全成功时与旧表达式**逐字等价**——没有失败就什么都没被过滤掉） |
| F2 | `useModelCounts().sync` 成功分支 | 旧：只 `setCounts` + `return true`，**从不清 `probeFailed`** ⇒ 挂载探测失败后，用户点同步成功也永远显示「探测失败」（挂载探测只在 `key` 变化时重跑 ⇒ 无自愈）。新：成功时 `setProbeFailed((f) => (f[name] ? { ...f, [name]: false } : f))` |
| F2b | 同上，catch 分支 | 同步**失败**也记标记（`setProbeFailed(… true)`），使 chip 对失败的 retry 同样诚实 |
| F2c | chip 渲染（失败态） | 文案「探测失败 · **重试**」原是不可点的 `<span>`（承诺了它不提供的动作，该文件 `:300-302` 自己论证过这条），改为**可点 `<button type="button" className="tag" onClick={() => probe(r.name)}>`**；**不加 `role="alert"`、不加 `err` 类**（那两个属于 sync 路径，t102 的 spec 断言挂载探测**不**走那条路 ⇒ 计数仍为 0） |

## 2 读数（`panel/e2e/failure-visibility.spec.ts`，走仓库入口 `npm run test:e2e`）

**聚焦运行（最终字节）**：**6 passed / 0 failed / exit 0**（7.9–10.1s）

| 断言 | DOM 读数 |
| --- | --- |
| **F1 全失败**（所有 `agentOptions` 返 500） | `DOM READING F1-models-readout-all-failed: 探测失败 · 重试` ⇒ **汇总条没有数字**（断言 `not.toMatch(/\d/)` 通过）；同一情形的卡片也是 `探测失败 · 重试` |
| **F2 挂载失败 → 重试成功**（`page.unroute` 后改返 `THREE_MODELS`，点卡上的同步按钮） | 改前卡：`…探测失败 · 重试…` → 改后卡：`…3 个模型…`；**汇总 = `DOM READING F2-after-retry models readout: 3`**（断言 `toMatch(/3/)` 通过），且**不再出现**失败文案 |
| 部分失败 / 全成功 | 汇总 = 只对答复过的探测求和（全成功与旧表达式逐字等价；部分失败排除失败项） |
| **真 0 models**（P3 成功控制） | `DOM READING P3-success error-text count: 0`、`P3 models readout` 仍为可测的数字/「未探测」态，**与「全失败」可区分** |
| t84 既有断言未回归 | P2 500 ⇒ `DOM READING P2 error zone: 最近召回召回失败召回失败`；P2 200+`[]` ⇒ 错误文案计数 **0**；P3 失败 ⇒ in-card `role="alert"` 节点 **0**（挂载探测仍未走 sync 路径） |

## 3 负控（**能红**，且是重建过的字节）

- **第一次尝试无效（如实登记）**：我只改了源码**没有重建 `dist`** 就重跑 spec ⇒ 服务的是**旧的已修复 bundle** ⇒ 控制组「6 passed」。**这是第 15 条（陈旧产物）的实例**：验证面板行为必须 `npm run build` 之后再跑。
- **第二次（有效）**：**删掉那一行 `setProbeFailed((f) => (f[name] ? { ...f, [name]: false } : f));`**（回到旧行为）→ `npm run build` → 聚焦运行：
  - `Error: expect(locator).not.toHaveText(expected) failed` · `Expected pattern: not /探测失败|probe failed/` · 定位到 **`failure-visibility.spec.ts:253`**
  - **`1 failed`**（`F2 (t103): a successful retry after a failed mount probe CLEARS the failure`）· `5 passed` · **exit 1**
- 恢复：`READING restored_sha=34597D7322583040`（与改前一致）→ **重建** → 聚焦运行 **6 passed / exit 0**。
- **窗口**：`2026-09-29T04:53:53 → 04:54:30`（约 37s，含两次构建），发生在 `panel/src/views/Runtimes.tsx`，只影响 F2 那条 spec；窗口内未触碰 Rust 侧任何文件。

## 4 成功态零改变

- `git diff --numstat`：**`panel/src/views/Runtimes.tsx` = `34 5`**、`panel/e2e/failure-visibility.spec.ts` = `35 0`（后者是**新增**的两条 t103 测试，不动既有断言）。
- 三处改动都在**失败态分支**或**对全成功等价**的表达式上：全成功时 `filter` 不删任何项 ⇒ 求和与旧表达式**逐字相同**；chip 只在 `probeFailed` 为真时换元素；`sync` 的两处只在成功/失败时各清/记一个标记。
- `cd panel && npm run build` = **exit 0**（后一次 5.11s；`dist updated (55 assets)`）。

## 5 不覆盖什么（第 19 条）

- **不覆盖 Rust 门禁**：本单只改 `panel/`，`cargo fmt/clippy/test` 对它是瞎的（AGENTS.md 已写明）。本单没有跑 Rust 门禁，也**不声称** workspace 状态。
- **不覆盖全量 e2e 绿**：仓库入口的**整条** e2e = `48 passed / 1 failed / 2 skipped / exit 1`，唯一红点在 **`consumption.spec.ts` 的 Wiki 覆盖率用例**（`consumption-Wiki-page-…-shows-coverage-unknown`，产物 `test-results\run-74076-…\test-failed-1.png`）—— **不是本单文件**（我只改 `Runtimes.tsx` 与该 spec 新增段），按第 10 条只报坐标不改；它可能是他人在途改动，需 captain 转 wiki 侧确认。
- **不覆盖**：模型计数「部分失败」的逐样本读数（本单只给求和语义与全失败/全成功两端）；`ReadoutStrip` 对字符串 `value` 的渲染细节（靠 panel 构建的 tsc + 聚焦 e2e 的 DOM 读数间接覆盖）；IPv6/多运行时并发的探测时序。
- **未取到**：无（三条判据的读数与负控都在上面；唯一「未取到」的是整条 e2e 的全绿，原因与归属已写明）。
