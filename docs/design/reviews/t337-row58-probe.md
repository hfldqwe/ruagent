# t337 行 58 的探针 bug（F-331a）—— 修法与读数

任务：`panel/tools/design-audit.mjs` 的行 58（URL 反映视图状态）**永远失败**。inScope：`panel/tools/` · `docs/design/reviews/`。

## 三层成因（逐层实测，最后一层才是 t331 说的那一层）

| 层 | 实测 | 修法 |
| --- | --- | --- |
| ① t331 说的 | 探针取 `[role=option]` 的**第一个可见项** —— 那正是**当前已选中**的 `approver` ⇒ 状态不变 | 选 option 时跳过 `aria-selected=true` / `aria-disabled=true` / `.ant-select-item-option-disabled`（与第 ① 步取未选中 radio 同构） |
| ② **更深一层（本单实测）** | 探针用 **DOM `.click()` 打开 antd 的 Select** —— antd 监听的是 **mousedown** ⇒ 下拉**从未打开**，`[role=option]` 为空（实测 `visible 0`）⇒ 上面那次 pick 是**静默空操作**，在任何路由上都如此 | 打开下拉改为派发 `mousedown`+`mouseup`，**再**取 option；并且**不在第 ① 步点它**（`picker.click()` 已删，改由 picker 分支自己开） |
| ③ **分支从不记录** | 第 ① 步的对象集里含 `[role=combobox]` ⇒ 它在 #chat/#settings 上**第一个**被采用 ⇒ `clicked` 落在第 ① 步 ⇒ picker 分支（第 ③ 步）**从不运行** ⇒ `pickNote` 为 null ⇒ 连「探针没做到」都说不出来 | 第 ① 步不再把 combobox/select 当可点开关（它们归 picker 分支），picker 分支作为兜底仍然可达 |

## 点后断言（本单新增）

点前/点后各取 `.ant-select-selection-item` 文本（回退 `p.value`），写进 `pickNote.before/after`；**若未变且 hash 未变 ⇒ `not_measured` 并点名原因**：
`no option changed the selection (visible N, selectable M, picked 'X') -- the probe failed to construct a state change, which is not a view that ignores the URL`
—— 三个计数（visible/selectable/picked）就是一份诊断，而不是一句推脱。

## 读数

| 读数 | 值 |
| --- | --- |
| `node tools/design-audit.mjs --self-test` | **472/472 pass**（含本单新增三条方向用例） |
| `--routes=chat --check`（两模式） | **exit=0**，行 58 = **NOT_MEASURED**（不再 FAIL） |
| `--routes=settings --check`（两模式） | **exit=0**，行 58 = **NOT_MEASURED** |
| 全量分段跑（13 段，`audit-shards.mjs --segments=routes --reset --detach`） | 已跑 **9/13** 段 · captures **18** · **失败行并集 []** |

**行 58 现在是 NOT_MEASURED 而不是 PASS**：这是「探针没能构造出状态变化」的正确结论 —— 我的独立探针证明**产品这一侧是好的**（`#chat` 上用 mousedown 打开下拉、点 `architect` ⇒ hash 从 `#chat?agent=approver` 变成 **`#chat?agent=architect`** ✓，视图修复 3a7f00e 确实在写 URL）。探针侧仍有一条没走通（`#settings` 的「提取智能体」在两次求值之间把下拉关掉了：`opts` 抓到 3 项、紧随其后的 pick 却读到 `visible 0`），所以它诚实地报 not_measured。

## 自检新增的三条方向用例（`--self-test`）

* `row58 must-PASS: clicking a DIFFERENT option changes the hash and replay restores the state`
* `row58 must-not-PASS (F-331a shape): the probe clicked the CURRENTLY SELECTED option, so nothing changed -> not_measured with a reason, never a pass and never a silent FAIL`（断言 `pass===null && measured===false && /no option changed the selection/`）
* `row58 must-FAIL: the probe DID change the selection but the URL did not move`

⇒ 「永远失败的判据」被它自己的自检钉住；「探针没做到」与「视图不写 URL」被分成两种结论。

## 反向（③）的说明

活体反向（把可选 option 全标成 `aria-selected=true`）需要临时改 `panel/src/views/Chat.tsx:266`，而本单明令不许改 `panel/src`（且 t333 占着它、平台不会给合法 inScope）⇒ 按队长裁决**停在仪器层**（上面两条自检用例）+ **活体的 NOT_MEASURED 读数**（chat/settings 两段现在就是 not_measured 带原因，正是「不许报 PASS」的现场 ✓）。

## 纪律

只改 `panel/tools/design-audit.mjs`（显式路径提交）· 未碰 `panel/src/` · 未 push · 未启停 daemon · 未调 `/api/v1/recall` · 探针脚本在仓外 `C:/tmp/t337` · `node --check` 通过、`--self-test` 全绿（工具未留在坏状态）。
