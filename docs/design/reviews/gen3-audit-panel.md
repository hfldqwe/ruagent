# gen3 面板审计（只读）— UI 语义 vs API、失败可见性、轮询成本、e2e 覆盖

> 任务：t79。范围：`panel/src/**` + `panel/e2e/**`（**只读**；我的唯一写入 = 本报告）。
> 审计者：verify（独立验证员）。
> 纪律：**没有启动真守护进程**；为取「UI 文案 ↔ API 响应」成对读数的 API 侧，我用了**我自己的临时 root + 临时端口 8815**（`%TEMP%\ruagent-t79`，exe = `%TEMP%\ruagent-team-target\debug\ruagent.exe`，`RUAGENT_EMBEDDER=hash`），收尾按记录的 pid 停、root 已删（见 §6）。
> 归因：本单在并发波次里做（工作区在动）。凡引用**同名文件**我都用全路径；引用的每个行号都在第 6 节列了收尾读数。
> 「我读到的」= 我这一轮的读数；「t20 的 DOM 读数」= 我 3 小时前在**我自己的**临时 root（8811/8813，pid 已记录并已停）上跑 `npm run test:e2e` 的产物 —— 这两类我都标出来源。

## 0 结论（三条最值钱）

1. **P1（high）面板的 `NAMESPACES` 矩阵与守护进程的**权威**写词表不一致，且是**行为级**错**：我在自己的临时 root 上逐个打 `POST /memory/write`，得到
   `profile×user` → `Inserted` · **`observation×global` → `RejectedNamespace`** · `procedure×user` → `RejectedNamespace` · `procedure×global` → `Inserted` · `lesson×global` → `Inserted`。
   对照 `panel/src/views/Memory.tsx:36-39` 的矩阵（`profile:[user]`、**`observation:[user,global]`**、`procedure:[global]`、`lesson:[global]`）⇒ **observation 那一行提供了一个服务器永远拒绝的选项**，而 **procedure/lesson 又漏掉了服务器允许的 `project:<x>`**。守护进程**自己**在 `crates/memory/src/lib.rs:129-140` 写明了这件事：*"Consumers that currently publish a flat list of namespace values … **the panel's `NAMESPACES` table** … should read this instead — see the t52 report for the routed findings."*
2. **P2（high）Memory 的召回日志把「读失败」渲染成「没有数据」**：`Memory.tsx:163-167` 的 `catch(() => { setRecallLog([]); … })` + `:309` 的 `{recallLog && recallLog.length > 0 && (…)}` ⇒ 500/超时/断连时整张卡片**消失**。这与本仓**自己写下的规则**相反——`Wiki.tsx:73-74` 逐字写着 *"an unreadable wiki is not an empty wiki (MASTER §12 row 20)"*，`Home.tsx` 用 `markFailed(domain)`、`SessionsView` 用 `kind:"error"` + "data may be stale · N seconds ago" 都对。⇒ **失败不可判定，被长度 0 掩盖。**
3. **P3（medium）Runtimes 的模型数探针把失败渲染成 `0`**：`Runtimes.tsx:36` 的 `.catch(() => {})` 让 `agentOptions` 失败后 chip 显示 **0 个模型**；同一个文件在 `:42-44` 刚刚为同一类问题写了 *"R11 wants a visible failure, not a silent no-op (this catch used to swallow it)"* ——**修了一个、漏了它的兄弟**。

## 1 方法与可复现仪器

| 仪器 | 命令 | 用途 |
| --- | --- | --- |
| 面板静态清点 | `python %TEMP%\t79-census.py` | 轮询点、错误处理形状、`any`/非空断言、视图↔spec 覆盖矩阵 |
| 成对读数（API 侧） | `powershell -File %TEMP%\t79-run.ps1` → `python %TEMP%\t79-pair.py`（我自己的临时 root，端口 8815） | P1 的 8 个 (store×namespace) 行为读数、`/recall/log` 行键、`/distill`+`/stats` 键 |
| 权威词表（源码侧） | `crates/memory/src/lib.rs:141-150`（`write_vocabulary()`）、`allowed_kinds()` 的 match 表 | P1 的"正确值"从**函数**取，不是我复制常量 |
| 逐行读数 | `Read`/`Select-String`（行号见每条 finding） | 其余 |

## 2 findings（10 条，每条：file:line · 复现 · 证据读数 · 为什么重要 · 可证伪判据 · owner）

### P1（high · UI 语义 vs API）`NAMESPACES` 矩阵两个 store 行错：observation 提供必拒选项、procedure/lesson 漏 project

- **file:line**：`panel/src/views/Memory.tsx:34-40`（矩阵）、`:171-182`（把矩阵与 counts 里的 project/agent 命名空间合并）、`:215-252`（store/namespace 选择器）；权威侧 `crates/memory/src/lib.rs:129-150`、`allowed_kinds()` 的 match（`Profile => [User]`、`Observation => [User, Project, Agent]` **注释逐字写 "NOT global — `global` is the cross-project scope of distilled procedure/lesson"**、`Procedure | Lesson => [Project, Global]`）。
- **复现**：
  ```powershell
  # 我做的（临时 root + 8815；每次调用一个 (store, namespace)）
  POST /api/v1/memory/write {"store":"observation","namespace":"global","content":"…","kind":"fact"}
  POST /api/v1/memory/write {"store":"procedure","namespace":"user","…"}
  ```
- **证据读数**（我这一轮、逐条）：
  ```
  UI matrix (Memory.tsx:36-39): {"profile":["user"],"observation":["user","global"],"procedure":["global"],"lesson":["global"]}
     profile      + user    [UI-allows ] -> 200 {"outcome":"Inserted(1)"}
     profile      + global  [UI-blocks ] -> 200 {"outcome":"RejectedNamespace (this store does not allow that namespace; write vocabulary = profile×{user} | o…
     observation  + user    [UI-allows ] -> 200 {"outcome":"Inserted(2)"}
     observation  + global  [UI-allows ] -> 200 {"outcome":"RejectedNamespace (…)"}      ← UI 提供、服务器必拒
     procedure    + user    [UI-blocks ] -> 200 {"outcome":"RejectedNamespace (…)"}
     procedure    + global  [UI-allows ] -> 200 {"outcome":"Inserted(3)"}
     lesson       + user    [UI-blocks ] -> 200 {"outcome":"RejectedNamespace (…)"}
     lesson       + global  [UI-allows ] -> 200 {"outcome":"Inserted(4)"}
  invalid namespace `nope` -> 400 `invalid namespace \`nope\` (user | global | project:<x> | agent:<x>)`
  权威表: Profile=>[User]; Observation=>[User,Project,Agent]; Procedure|Lesson=>[Project,Global]
  ```
- **为什么重要**：① 用户可以在 UI 里选一个**永远失败**的组合（不是静默——对话框会显示 `RejectedNamespace(...)`——但这是"界面承诺了一个不存在的权限"）；② 反向更隐蔽：**procedure/lesson 的 `project:<x>` 没有出现在基础矩阵里**，只有当某个 project 命名空间**已经**在 `memory/counts` 里出现过才会被 `:182` 的 spread 补进来 ⇒ **新项目的 procedure 在 UI 上选不出来，尽管服务器接受它**（能力被界面藏住）；③ 这条已被守护进程侧 t52 **点名路由**给这张表（lib.rs:137-139），说明它是一条**未闭合**的历史 finding，不是新发现。
- **可证伪的修复判据**：矩阵的每一行必须与 `allowed_kinds()` 一致（`observation=[user,project,agent]`、`procedure/lesson=[project,global]`），且 `project:<x>` 有输入路径；更强的判据：**UI 可选集合 ⊆ 服务器接受集合**，用一个"对每个可选组合打一次 `/memory/write`，不许出现 `RejectedNamespace`"的判据自动核（今天有 1 个反例：observation×global）。更根本的修复是让面板**读** `write_vocabulary()`（daemon 已在拒绝文本里下发它）。
- **owner**：panel（Memory.tsx）；词表 owner = memory（t52 已路由）。

### P2（high · 失败可见性）召回日志的读取失败 → 卡片消失（"不可读"被伪装成"没有记录"）

- **file:line**：`panel/src/views/Memory.tsx:152-168`（`:163-167` 的 catch → `setRecallLog([])`）、`:309`（`{recallLog && recallLog.length > 0 && (…)}`）；对照 `panel/src/views/Wiki.tsx:70-76`（`setFailed(true)` + 注释）、`panel/src/views/Home.tsx:150-176`（`markFailed`/`settled`）、`panel/src/views/SessionsView.tsx:277-290`（`state.kind === "error"` + stale 秒数）。
- **复现**：`sed -n '152,168p;305,318p' panel/src/views/Memory.tsx`（静态）；运行时：让 `/recall/log` 返回 500（例如把面板指向一个停掉的守护进程）⇒ 该 Zone 不出现，页面上**没有**任何错误或空状态文案。
- **证据读数**：catch 里三行全是"清空"（`setRecallLog([])`、`setLogTotal(null)`、`setLogFilter(null)`），渲染条件是 `length > 0` ⇒ 失败路径与"这个库从来没有召回过"在 DOM 上**逐字节不可区分**。同仓的正确形态就在隔壁文件（`Wiki.tsx:73` 的注释把规则写死了："an unreadable wiki is not an empty wiki"）。
- **为什么重要**：召回日志是"检索→注入"闭环的**唯一**人可见读数（含 `top_memory_score`/`top_knowledge_score`）；它失败时用户会得到"没有召回记录"这个**错误结论**，而不会去查守护进程。这正是本单要求"不可判定不得被布尔（长度/数组）掩盖"的那一类。
- **可证伪的修复判据**：`/recall/log` 失败时页面必须出现可见的错误态（复用 `ErrorState` 或 SessionsView 的 stale 提示），并且存在一条自动化读数：让该端点返回 500（或在 e2e 里拦截路由），断言 DOM 里出现错误文案且**不**出现"无记录"文案。
- **owner**：panel（Memory）。

### P3（medium · 失败可见性）Runtimes 的模型数探针失败 → 显示 `0`（把"量不到"当"量到了 0"）

- **file:line**：`panel/src/views/Runtimes.tsx:26-41`（`:36` `.catch(() => {})`；`:34` `choices.length ?? 0`）；同文件 `:42-44` 的对照注释。
- **复现**：`sed -n '22,45p' panel/src/views/Runtimes.tsx`；运行时：让 `GET /agents/{n}/options` 失败（不存在的 agent、或守护进程不可达）⇒ chip 显示 `0`。
- **证据读数**：失败分支**不写** `setCounts`，而渲染侧把"没有条目"读成 `0`；文件自己的注释说同类吞异常已被修（"this catch used to swallow it"）⇒ 这是**同一个 bug 的另一个实例**。对比 `SessionsView` 的 `unknown` 三态、`Wiki` 的 `failed` 标志：本仓已有"不可判定要显式"的模式。
- **为什么重要**：运行时卡片上的"0 models"是一个**看似读数**的 UI 值（用户会据此判断该 runtime 坏了/没配模型），而真实原因是探针没回来。
- **可证伪的修复判据**：探针失败时该 chip 显示未知态（例如 `—`/`?`）而不是 `0`；e2e 判据：拦掉 `agentOptions` 的响应，断言 chip 文本**不等于** `0`。
- **owner**：panel（Runtimes）。

### P4（medium · e2e 覆盖）`Settings.tsx` 零 e2e 覆盖：13 个路由里唯一没被任何 spec 走过的

- **file:line**：`panel/src/views/Settings.tsx`（整文件）；`panel/e2e/views.spec.ts`（走 11 个路由）；`panel/e2e/*.spec.ts` 全文。
- **复现**：
  ```bash
  grep -rn 'settings\|Settings' panel/e2e/ | wc -l      # 0
  grep -rn '#settings' panel/e2e/ | wc -l               # 0
  # spec → 路由清点（我的 t79-census.py 第 D 段）
  ```
- **证据读数**：`views.spec.ts` 覆盖 agents/board/chat/graph/inbox/knowledge/memory/runtimes/sessions/stats/task（11），`responsive.spec.ts` 加 home，`settings` **0 次**。而 Settings 承载 **`api.setDistillPolicy`（PUT，会改 `policy.toml` 的 `[distill]` 表）** 与模型/运行时/蒸馏策略等写操作（`Settings.tsx:8,27-30,43,64-65,82,100-103,123-139`）。
- **为什么重要**：`panel/` 不被任何 Rust 门禁覆盖（AGENTS.md 已声明），面板唯一的运行时门禁就是 e2e；**写路径最重的视图没有任何运行时门禁**。本代 t57/t59 改的正是 settings/distill 这条链。
- **可证伪的修复判据**：存在一个 spec 加载 `#settings`、断言 distill 卡片由 `GET /distill` 渲染、并做一次 PUT 往返（对着临时 root）；或 coverage declaration 明确写"settings 不在 e2e 覆盖内 + 理由"。抽查：`grep -c '#settings' panel/e2e/*.spec.ts > 0`。
- **owner**：panel e2e owner。

### P5（medium · 成本/轮询）9 个轮询点里 5 个**不因标签页隐藏而停**；"每视图成本上限"结构性看不到叠加

- **file:line（有 visibilitychange 门）**：`panel/src/App.tsx:357-368`、`panel/src/views/Agents.tsx:44-53`、`panel/src/views/Board.tsx:121-130`、`panel/src/views/Runtimes.tsx:105-106`。
  **（无门）**：`panel/src/views/Wiki.tsx:78-83`（**3000 ms**）、`panel/src/views/TaskDetail.tsx:96-101`（**2000 ms**）、`panel/src/views/SessionsView.tsx:267-275`（10000 ms）、`panel/src/views/Home.tsx:159-163`（10000 ms）、`panel/src/views/Chat.tsx:1339-1343`（3000/5000 ms）。
- **复现**：`grep -n 'setInterval' panel/src/views/*.tsx panel/src/App.tsx`；`grep -c 'visibilitychange' panel/src/views/*.tsx`。
- **证据读数**：清点 = 9 个 `setInterval` 轮询点，4 个有 `visibilitychange` 门、5 个没有（每个都有 `clearInterval` 卸载清理 ✓，所以不是泄漏，是**后台空转**）。算术：隐藏标签停在 TaskDetail = **30 请求/分**、Wiki = 20/分、Chat = ≤20/分（它自己的注释）、Home/Sessions = 各 6/分 ⇒ **一个后台标签可轻易 ≥50 请求/分**，而 `/wiki/pages` 的单页成本按 t68 的 A6 是**线性增长**的。Chat 的注释把自己的账算成 *"≤12 requests/min idle … well under the row-28 cap of 7"* ——那个 cap 由 `panel/tools/design-audit.mjs` 判定，而它**一次只渲染一个路由**（13 路由逐条跑）⇒ **这是按视图定义的门，结构上看不到"N 个视图同时挂载"的叠加**。
- **为什么重要**：单用户本地平台，成本主要落在 SQLite/lancedb 读与风扇上；隐藏标签长期空转是"无声的常驻负载"，而它**永远**不会让任何门禁变红。
- **可证伪的修复判据**：每个轮询点要么有 `visibilitychange` 门（并说明为什么不能有），要么在视图注释里声明"故意不停"；并补一条聚合读数：同时挂载 N 个视图、统计 60 s 内请求数 ≤ 某个写明上限（今天没有任何闸门测量聚合）。
- **owner**：panel owner（逐视图）。

### P6（low · UI 语义 vs API）"自定义 prompt"徽章是**客户端字符串比较**，因为服务端不给 `prompt_hash`

- **file:line**：`panel/src/views/Settings.tsx:82`（`const isCustom = !!policy.prompt && policy.prompt.trim() !== policy.builtin_prompt.trim();`）、`:83`（`effectivePrompt = policy.prompt || policy.builtin_prompt`）。
- **复现**：`sed -n '78,95p' panel/src/views/Settings.tsx`；API 侧 `GET /api/v1/distill`（我的读数：`keys=['agent','auto','builtin_prompt','graph','language','prompt']`）。
- **证据读数**：界面用"两段文本是否相等"判定"用户改过 prompt"；`/distill` **没有** `prompt_hash`（与 t20 §5 F3 的读数一致）。⇒ ① 只差一个换行的相同 prompt 会被判成"自定义"；② **内置 prompt 升级后**，所有用户的 `builtin_prompt` 变了、`prompt` 没变 ⇒ 徽章仍显示"自定义"（其实用户没改过）；③ 没有任何机械证据能回答"这份 prompt 是哪一版的"。
- **为什么重要**：这是 K-11「产出率变化可归因」的**面板侧症状**——归因的锚点（hash）在服务端缺失，于是界面退回文本比较。
- **可证伪的修复判据**：`/distill` 返回 `prompt_hash`（服务端，t57）且 `Settings.tsx` 用它判定 `isCustom`（或服务端直接给 `custom: bool`）；判据：同一文本 + 不同空白必须判为"未自定义"，且内置 prompt 变更不影响已自定义用户的判定。
- **owner**：panel（Settings）+ daemon（distill，t57 已立）。

### P7（low · 死代码/口径）6 条 `eslint-disable-next-line` 在本仓**没有对应 linter**（抑制的是空气），且面板没有 lint 门

- **file:line**：`panel/src/views/Agents.tsx:55`、`Chat.tsx:592,1325,1342`、`Graph.tsx:1327`、`Wiki.tsx:82`；`panel/package.json` 的 `scripts`（只有 `dev`/`i18n:check`/`build`/`check`/`test:e2e`）。
- **复现**：`grep -rn 'eslint' panel/package.json`（0）；`ls panel/*eslint* panel/**/*eslint*`（0，node_modules 除外）；`grep -rn 'eslint-disable' panel/src | wc -l`（6）。
- **证据读数**：6 条抑制注释全部是 `react-hooks/exhaustive-deps`，而项目里**没有 eslint 依赖、没有配置文件、没有 lint 脚本**（CI 也不跑——见 t70）。⇒ 这 6 条是**装饰性**的：它们不抑制任何检查，只留下了"这里有个 lint 例外"的信号；反过来，`panel/src/**` 的 hooks 依赖正确性**没有任何机械门禁**（只有 tsc）。
- **为什么重要**：读者会把 `eslint-disable` 读成"有 linter 放行了这一处"，实际是"没有 linter"；而 6 处 `exhaustive-deps`（包括 Chat 的两个轮询 effect、Wiki 的 3 s 轮询 effect）恰好都是**最容易写错依赖数组**的地方。
- **可证伪的修复判据**：要么真的引入 eslint + `lint` 脚本并让 CI 跑（那时这 6 条才有意义），要么删掉这 6 条注释并在 AGENTS.md/注释里写明"hooks 依赖不做机械检查"。判据：`grep -c eslint-disable panel/src` 与 `grep -c eslint panel/package.json` 同为 0，或同为正。
- **owner**：panel owner。

### P8（low · 口径）召回日志的「来源」筛选是**硬编码三元组 + 当前页来源**，其他来源不可选

- **file:line**：`panel/src/views/Memory.tsx:330-336`（`["probe","distill","user"]` + `unknown` + `__all__`，并用 `recallLog.some(...)` 从**当前页**补来源）。
- **复现**：`sed -n '320,342p' panel/src/views/Memory.tsx`；API 侧（我这一轮）：`GET /recall/log?limit=5` ⇒ `row keys = ['entities','knowledge','memories','query','source','source_label','strategy','top_knowledge_score','top_memory_score','top_n','ts','wiki']`，`retention = {policy, max_rows: 5000, rows, oldest_ts, newest_ts, rows_without_source}`。
- **证据读数**：`source` 是**调用方自报**的任意字符串（t20 我读到 `source='probe'`；服务端把它 trim+lowercase 后回显，`source_filter` 字段就是证据）。面板的可选集合 = 三个写死的值 + `unknown` + **本页 20 行里出现过的值** ⇒ 一个只出现在更早页里的来源（例如 `mcp_recall` 或别处写入的值）在 UI 上**选不到**，只能在"全部"里顺带看到。
- **为什么重要**：这是"筛选器与被筛数据同源"的循环——筛选选项由被筛页决定，翻页后选项集还会变（同一个下拉在不同页码下内容不同，用户无法稳定复现自己的视图）。
- **可证伪的修复判据**：来源选项来自服务端（例如 `retention`/一个 `sources` 字段列出该库真实出现过的 source 及计数），且 UI 选项集不依赖当前页；判据：把一页 20 行的窗口挪到只含 `mcp_recall` 的页面，该来源仍可选。
- **owner**：panel（Memory）+ daemon（若要暴露 sources 聚合）。

### P9（low · e2e 的绿代表什么）9 个 spec 里 19 个 `test.skip` 点；`42 passed` 可以是"很多条被跳过"

- **file:line**：`panel/e2e/*.spec.ts`（清点：chat 3、judge 5、wiki 4、knowledge 2、consumption 1、palette 1、recall 1、registry 1、views 1 = **19**）；`.github/workflows/e2e.yml:168`（只有 `registry.spec.ts` 被跳过才失败）。
- **复现**：`grep -c 'test.skip(' panel/e2e/*.spec.ts`；`sed -n '139,176p' .github/workflows/e2e.yml`。
- **证据读数**：我 t20 在自己的根上跑的两次：**有数据的根 42 passed / 1 failed / 2 skipped**、**空根 40 passed / 4 failed / 1 skipped**（`- 16 e2e\recall.spec.ts:10` 两次都跳过）；`wiki.spec.ts` 的 4 处跳过是数据依赖（无页时不测渲染）。⇒ "套件绿"的准确含义是「**有数据的那些 spec 跑了**」；跳过的 spec 只打印计数（t70 F5 已判：这属于 CI 闸门缺口，不再重复立 finding）。
- **为什么重要**：本单要的就是"e2e 的绿代表什么"——它代表 `views.spec.ts` 的 11 个路由壳 + 有数据时的 consumption/wiki/knowledge 读数，**不代表** settings（P4）、不代表写路径失败态（P2/P3 的形态没有 spec）。
- **可证伪的修复判据**：见 t70 F5（逐 spec 名 skip 预算）；面板侧的判据：每个 skip 点必须能被一个**已知根状态**触发并记录在 evidence 里（今天只打印总数）。
- **owner**：panel e2e owner + CI owner。

### P10（low · UI 语义 vs API）procedure/lesson 的 `project:<x>` 只有"已存在"才可选（能力被界面藏住）

- **file:line**：`panel/src/views/Memory.tsx:171-182`（`projectNamespaces`/`agentNamespaces` 来自 `counts`，再 `...new Set([...NAMESPACES[store], ...])`）。
- **复现**：`sed -n '165,190p' panel/src/views/Memory.tsx`；API 侧权威表 `Procedure | Lesson => [Project, Global]`。
- **证据读数**：基础矩阵没有 `project`（见 P1），补齐靠 `memory/counts` 里**已经出现**的 project 命名空间 ⇒ 在一个**新**项目里，用户无法把 procedure 写到 `project:<new>`（服务器允许），只能先别处写入才会出现。
- **为什么重要**：与 P1 是同一根因（矩阵是手抄的服务端规则），但症状不同：P1 是"给了假选项"、P10 是"藏了真选项"。两者合起来说明这张表**既多又少**。
- **可证伪的修复判据**：namespace 选择器提供 `project:`/`agent:` 的自由输入（或从服务端拿该 store 的允许前缀），且"新项目 → 立刻可选 `project:<x>`"；判据：在空 root 上打开 memory 视图，procedure 的 namespace 下拉包含 `project:` 输入路径。
- **owner**：panel（Memory）。

## 3 已查但**不是**缺陷（免得下一轮重复劳动）

1. **召回日志表格的列与端点返回的行键一致**（我这一轮的成对读数）：端点返回 12 个键（`ts,query,strategy,top_n,memories,knowledge,wiki,entities,top_memory_score,top_knowledge_score,source,source_label`），面板正是读这些（`Memory.tsx:367-390`）。**新遥测列（score_kind/fusion/graph_entities…）面板看不到，是因为端点不返回**——服务端缺口已在 t20 §5 F1 / t57 立过，不是面板缺陷。
2. **i18n 的「76 个键无字面量 / 其中 40 个不在任何动态族前缀内」不是死键**：`panel/e2e/i18n-check.mjs:15-18` 逐字声明非字面量引用**只列出、不算失败**（"pretending they were verified would be the same lie as not checking at all"）。⇒ 我的 t20 读数（exit 0 + 那两个数字）与它的契约一致。
3. **类型纪律很好**：`@ts-ignore`/`@ts-expect-error` **0**；`as any` **1**（在注释里）；非空断言 **5 处**（`main.tsx:14` 的 root、`Agents.tsx:234`、`Chat.tsx:1016`、`Memory.tsx:474`、`TaskDetail.tsx:348`）全部落在语义上必然存在的形状上。
4. **失败可见性的正确形态在仓里已有四处**（这就是 P2/P3 是"例外"而非"政策"的证据）：`Wiki.tsx:70-76`、`Home.tsx:150-176`、`SessionsView.tsx:284-290`、`Board.tsx:365-372`（后者还只在卡片打开时轮询并写明成本）。
5. **`registry.spec.ts` 的写保护做法是对的**（`writeAccess()` + finally 清理，AGENTS.md 有表）；t70 F1 说的是"这套 guard 还没进提交树"，与本单无关。
6. **`CommandPalette.tsx:121` 自己记着"这两个 catch 曾经吞异常"**——本代已经修过同类问题，P3 是同类漏网，不是"没人管过"。

## 4 未验证猜想（**不是** finding）

- **G1** P1 的用户影响面我没有实测（没有驱动 UI 去点一次 observation+global）；我能证的是"API 必拒 + UI 提供该选项"。到底有多少用户会踩，无读数。
- **G2** P5 的成本数字是**间隔算术**（3 s/2 s/10 s → 次/分），**没有**测量隐藏标签下守护进程的真实请求率或 CPU；也没测"浏览器对隐藏标签的 timer 节流"（Chrome 对背景标签有 clamp，这会让真实值低于算术值）——这需要实验。
- **G3** Settings 的写路径失败表现（PUT 500 时 UI 说什么）我**没有驱动**；只读了字段使用。
- **G4** `TaskDetail.tsx` 的 2 s 轮询可能是**故意**的（权限提示 `waiting_permission` 需要低延迟）——成本/收益我没测，所以只报"频率最高 + 无可见性门"，不主张它该改成多少。

## 5 未覆盖范围（每条写原因）

| 未覆盖 | 原因 |
| --- | --- |
| **浏览器里的任何观察**（Playwright/DOM） | 本单没有跑 e2e（预算 + 需要 boot 守护进程与浏览器）。所有"UI 显示 X"都来自**源码行**；唯一的 DOM 观测是引用我 **t20** 的读数（我自己的临时 root 8811/8813，pid 已停），并逐处标明来源。 |
| lint / hooks 依赖正确性 | 仓里**没有 linter**（P7 本身就是这个读数），所以"hooks 依赖对不对"只能人读。 |
| 轮询的真实成本（请求率/CPU/延迟） | 只做了间隔算术（G2）；需要 instrumented 运行。 |
| `panel/tools/**`、`panel/scripts/**` | 不在本单 inScope（t70 已审计工具面：设计闸门只跑 `--self-test`）。 |
| Settings 之外的视图逐个"失败态"穷举 | 我按清点（49 个 catch 链）挑了 P2/P3 两个**可证的**反例 + 四处正确形态；剩下的 catch 没有逐一构造失败。 |
| 可访问性（a11y）/ 键盘路径 | 不在本单列出的五类里；`responsive.spec.ts`/`theme.spec.ts` 存在但未读。 |
| 包体/性能读数 | 由 `design-audit.mjs` 负责，而 t70 F4 已判它只在本地跑。 |

## 6 我这一轮的操作与收尾（只读证据）

- **我起的守护进程**：端口 **8815**，root `%TEMP%\ruagent-t79`，pid **60160**（记录在 `<root>\canary.pid`）⇒ 收尾 `alive after stop: False`；root 已删（`cleaned my root: True`）。
- **真守护进程**：`pid 79984 alive StartTime=2026/9/27 5:35:37`（未启停、未写活库）。
- **我写入的文件**：只有本报告 `docs/design/reviews/gen3-audit-panel.md`；`panel/src/**`、`panel/e2e/**`、`panel/tools/**` **零字节改动**。
- 仪器（`%TEMP%`，不进仓库）：`t79-census.py`、`t79-pair.py`、`t79-run.ps1`、`t79-pair.out`、`t79-pair-harness.log`。
- 引用的行号都在本轮读过（`Memory.tsx:34-40/145-190/305-345`、`Wiki.tsx:70-95`、`TaskDetail.tsx:90-110`、`SessionsView.tsx:262-290`、`Board.tsx:360-378`、`Home.tsx:152-170`、`Chat.tsx:1332-1352`、`Runtimes.tsx:19-45`、`Settings.tsx:8-139`、`App.tsx:355-368`、`Agents.tsx:44-55`、`crates/memory/src/lib.rs:125-150`、`panel/e2e/i18n-check.mjs:1-45`、`.github/workflows/e2e.yml:139-176`）。
