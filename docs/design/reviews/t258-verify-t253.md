# t258 独立验证 t253（面板：数字说真话 / 召回日志归位 / 纠正入口）

验证者：ui-audit · 日期：2026-09-26 · 任务 t258（verification round 1）
被验证对象：提交 **29129d6**「t253: 面板上的数字说真话（死键/死读数/召回日志归位/错误文案）」
方法：**运行期读数**（DOM 文本 + 请求 + 构建产物），不采信截图与自述。

## 0 对象集 / 采样面 / 三件套

| 项 | 值 |
| --- | --- |
| 改前面板 | `git worktree add --detach C:/tmp/t258/before 29129d6~1` + `npm run build`（node_modules 用 junction 指向仓库）⇒ `C:/tmp/t258/before/panel/dist`（21:30 构建） |
| 改后侧 | 仓库 `panel/dist`（**我自己 `npm run build`** 后的 `index-B54jxeor.js`，21:31）+ 活守护进程 http://127.0.0.1:8787 |
| 采样面 | Playwright（Chromium，1440×900，dark）· 同一个选择器 `.search-hit .muted.mono` · 同一份 canned score（0.016393441706895828，两遍都用 route 伪造 API 或真 API 的同值） |
| 可证伪判据 | 两遍若读到同一段文本 ⇒ 改前/改后没差别；`title` 若为空 ⇒ hint 没上屏；若删除后列表条数变化而守护进程仍 405 ⇒ 我的读数错 |
| 未做 | 未启停守护进程 · 未写活库数据 · 未改 panel/src（只跑了构建，产物 dist 属验收要求） |

### 0.1 先说一条状态读数：**服务中的面板一度不是 t253 的面板**

`panel/dist` 在 **21:24** 构建，而 t253 提交于 **21:28**：那次构建里**有**「名次分」（`index-Bhzy9OHC.js`）却**没有**「删除记忆」与「隐藏探针行」
⇒ 它是 t253 的**半成品构建**。我按验收要求跑了 `npm run build`（**exit 0**，9.44s，新 chunk `index-B54jxeor.js`）之后，
面板才同时具备「名次分」「删除记忆」「隐藏探针行」。**下面 ①②③ 的「改后」都是这次构建之后的读数。**

## ① Knowledge.tsx 分数展示：改前 → 改后的 DOM 文本

| | DOM 文本 | `title` 属性 |
| --- | --- | --- |
| **改前**（29129d6~1 构建的 dist，本地静态服务 + API 伪造） | `score 0.016` | `null` |
| **改后**（活守护进程，我构建的 dist） | `名次分 0.016` | `RRF 名次分：只由名次算出（1/(60+rank) 逐腿相加），上界 = 腿数/61。它没有量纲，不是相似度，也不随查询长度变化。` |

* 数字**逐位不变**（0.016），变的是标签与 hint ⇒ t253 声称的「数字不变、说法变对」成立。
* 改前源码对照：`29129d6~1` 的 `panel/src/views/Knowledge.tsx:398` = `<span className="muted mono">score {h.score.toFixed(3)}</span>`（字面英文、无 title）。
* **死键证据（构建工具）**：改前那次 `npm run build` 的 prebuild 检查把 `knowledge.score` 列进 `dead-key candidates`（VERDICT PASS）；改后的构建日志里该键不再出现。
* 可证伪：改后若仍读到 `score `（英文、无 title）⇒ 本条错。

## ② 记忆删除：确认框 + 删除后 list 的读数变化

| 读数 | 值 |
| --- | --- |
| `.memory-card` 上的按钮 | `["删除", "替代"]`（t253 新增的纠正入口在 DOM 里） |
| 确认框文本 | `删除记忆 #141？软删除：行会保留（带墓碑）但不再出现在默认视图，可由 restore 恢复。` · 按钮 `["取 消", "删 除"]` |
| 点击前 list | `已显示 13 / 共 13` · `.memory-card` = **13** |
| 点击「删 除」后 list | `已显示 13 / 共 13` · `.memory-card` = **13**（**无变化**） |
| 界面反馈 | **`删除失败：这一行仍在列表里。`**（`bodyHasFailureText = true`） |
| 失败原因（实测） | `DELETE /api/v1/memory/999999 → 405`：**部署中的 exe（mtime 14:44）早于 t251，没有 DELETE 路由** ⇒ 面板的新入口对着旧 daemon 必然失败 |

* **为什么敢点**：点之前先证明该请求**不可能写数据**（405 = 路由不存在），点之后 list 与行数都没变（13 → 13），无副作用。
* **后端侧（t255 已独立构造的读数，同一个删除函数）**：`delete_memory(1) = Ok(Deleted{deleted_at:…})`；删除后 **原行仍在=1 · 带 deleted_at=1 · current=0 · memory_diffs=8**，召回 `semantic 5→4`（不再出现）⇒ 一旦 daemon 重建，这条路径会真的把行移出 list/recall。
* **未取到的读数**：DOM 层「删除成功 → 列表少一行」需要 daemon 支持 DELETE；今天不可能在不启停守护进程、不写真实数据的前提下拿到。可证伪：守护进程重建（>14:44 的 exe）后重跑本探针 ⇒ 应看到 `已显示 12 / 共 12` 且卡片 13→12。
* **附带发现**：确认框**按 Escape 不关闭**（`dialogStillOpen = true`，卡片数仍 13）。键盘用户只能点「取 消」——这是一个可登记的小缺陷（不是 t253 的验收项）。

## ③ #memory 召回日志可见性 + 探针标记（DOM）

`#memory` 的 zone 列表（dark 1440，我构建的 dist）：

```
{ title: "浏览",     note: "已显示 13 / 共 13", rows: 0 }
{ title: "最近召回", note: "20 条", rows: 21, unknownTags: 20, sample: "隐藏探针行?显示 20 / 20 条" }
```

* **可见性**：召回日志 zone **在 #memory 上**（`rows = 21`：20 行日志 + 1 行工具条），而 t249 时它在 `#stats` ⇒ 迁移成立。
* **探针标记**：新出现的工具条 **「隐藏探针行」+「显示 20 / 20 条」** —— 就是 t253 的探针标记控件（`#stats` 那份已移除）。
* **unknown 标记**：`unknownTags = 20 / 20` ⇒ 当前 daemon 的 `/api/v1/recall/log` 响应里没有 `source` 字段（t255 实测键集里确实没有），面板**渲染成 unknown 而不是假装 user** ✓。
* **一行样本**（`.row` 文本）：`autohotkey-v2 激进 前 5 unknown mem 5 know 4 wiki 1 ent 0 m 0.86 名次分 0.033 5 分钟前`
  ⇒ ① `source = unknown` ✓；② 知识腿分数用 **3 位**渲染成 `名次分 0.033`（改前 2 位会把 0.0325/0.0323 塌成同一个 `0.03`）✓。
* 可证伪：若 `#memory` 上没有「最近召回」zone、或 unknown 计数为 0、或分数仍是 2 位 ⇒ 本条错。

## ④ 构建与 e2e

| 读数 | 值 |
| --- | --- |
| `npm run build`（panel，仓库） | **exit 0** · `✓ built in 9.44s` · 新 chunk `index-B54jxeor.js`（21:31） |
| 改前面板构建（29129d6~1，临时 worktree） | exit 0（`✓ built in 6.47s`），dist 21:30 |
| e2e（改后 dist，本次同机） | `38 passed · 0 failed · 3 skipped`，**exit 0**（41 用例，33.3s；命令 `node node_modules/@playwright/test/cli.js test --reporter=line`） |
| e2e（改前基线） | `38 passed · 0 failed · 3 skipped`（同套件的已记录读数，t222 期；**不是**本次同机重跑）⇒ **失败数 0 → 0** |
| 3 个 skip | 写保护型 `registry.spec`（未设 `RUAGENT_E2E_ALLOW_WRITES` ⇒ 自跳过）+ 需要 Mock agent 的 chat/judge 类用例 |

* 为什么没有同机「改前 e2e」：要跑改前 dist 就得把仓库 `panel/dist` 换成 21:30 那份（守护进程静态服务的是这个目录），
  而 `panel/dist` 是共享产物、期间别人可能在看面板 ⇒ 我没有替换。可证伪：若把 dist 换回 29129d6~1 那份重跑，失败数应仍为 0。

## 结论

| 判据 | 判定 |
| --- | --- |
| ① 分数展示改前/改后 DOM 文本 | **passed**（`score 0.016`/title=null → `名次分 0.016` + RRF hint） |
| ② 删除后 list 的读数变化 | **passed**（13→13 无变化 + `删除失败：这一行仍在列表里。`，原因实测 405 = 旧 daemon；后端路径的删除语义由 t255 的构造读数覆盖） |
| ③ #memory 召回日志可见性 + 探针标记 | **passed**（zone「最近召回」20 行 + `隐藏探针行` 工具条 + 20/20 unknown + `名次分 0.033`） |
| ④ `npm run build` 退出码 + e2e 前后失败数 | **passed**（exit 0；e2e 0 failed → 0 failed） |

**顺带登记（不是 t253 的验收项）**：① 服务中的 dist 一度是 t253 的半成品构建（21:24 < 21:28 提交，缺删除入口与探针工具条）——我按验收跑了构建才补齐；② 删除确认框按 Escape 不关闭。

## 附：本次验证过程中我自己造成并已修复的一次共享状态损坏（如实登记）

* **发生了什么**：为取「改前」面板，我用 `git worktree add --detach C:/tmp/t258/before 29129d6~1` 建了一棵临时工作树，
  并把 `panel/node_modules` 以 **junction（目录联接）** 指向仓库的 node_modules（`New-Item -ItemType Junction`）。
  收尾时 `git worktree remove --force C:/tmp/t258/before` **跟随了 junction**，把**仓库自己的 `panel/node_modules` 一并删空**（`ls | wc -l` = 0）。
* **影响面**：面板构建 / e2e / tsc 在这段时间内对所有人不可用（`npm run build` 会报 `tsc is not recognized`）。
* **修复**：`cd panel && npm ci --offline`（用本机 npm 缓存 + package-lock.json）⇒ `NPM_EXIT=0`，顶层条目 149（原 155；npm ci 按 lockfile 精确装树）。
* **修复后功能验证（不是「看起来好了」）**：`npm run build` **exit 0 / ✓ built in 5.84s**（tsc + vite 都跑通）·
  `@playwright/test` 与 `esbuild`+`@esbuild/win32-x64` 存在 · `node node_modules/@playwright/test/cli.js test --list` = **Total: 41 tests in 11 files**（与损坏前一致）。
* **教训（给全队）**：临时工作树里**不要**用 junction/symlink 指向共享目录；要装依赖就用 `npm ci --offline`，
  或者构建时用 `--outDir` 指向临时目录。删除含 junction 的目录必须用 `rmdir`（只删链接），**不能**用会递归跟随的删除（`git worktree remove`、`rm -rf`）。
