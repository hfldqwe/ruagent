# gen2 Wiki 消费面装配（t51）：wiki 端点 + 纠错写端点 + 逐字段断言 + 面板四页 DOM 读数

> 单号 t51（implementation round 1）· 成员 `wiki` · attempt 1 · 时间窗 2026-09-28 15:5x–16:3x UTC（+08:00 23:5x–00:3x）
> inScope：`crates/daemon/src/api.rs` · `crates/daemon/src/wiki.rs` · `crates/daemon/tests/knowledge_api.rs` · `panel/` · 本报告
> 上游：R-D 规格 D.2/D.3/D.6（`docs/design/reviews/gen2-wiki-spec.md`）+ RV-D-1 的三态裁决（t34 报告 §5bis）
> 现场对象：临时 root `%TEMP%\ruagent-t51-root` + 临时端口 **127.0.0.1:8899**，守护进程 **pid 15172**（我起的，已按记录 PID 收尾）；真守护进程 **pid 79984 未触碰**

## 0 一句话

把 t34 的三态语义**接进消费面**：wiki 两个读端点的字段逐条落到线上（HTTP 逐字段读数在 §1.1）、纠错写端点（`pin`/`release`/`note`）落地并**被语义读取**（`active_correction`，不只是存一行）、`knowledge_api.rs` 的 wiki 断言 **16 → 40 条**（含 `links_out` 收窄的**反证夹具**）、面板四页各一条 **DOM 级读数**且两条显示规则**在旧代码上真的红**（§1.3）。顺带发现并修掉 t10 留下的一处**词汇不一致**（§4.1）。

## 1 三件交付：改前 → 改后

### 1.1 wiki DEP-W：端点装配 + 纠错写端点

| 面 | 改前 | 改后（读数取自 8899 那个临时守护进程） |
| --- | --- | --- |
| `GET /api/v1/knowledge/wiki/pages` | 由 `WikiPageInfo` 序列化透传，但**从未逐字段读过**；t34 之后新字段（三态 `cite_coverage` 等）在线上**无人核对** | 逐字段读数（手写页，无 `wiki_pages` 行）：`{"slug":"e2e-consumption-probe","title":"Consumption probe","summary":"三态显示探针","aliases":[],"entities":[],"sources":[],"stale":false,"edited":false,"links_out":0,"links_in":0,"freshness":"fresh","stale_since":null,"stale_sources":[],"stale_reasons":[],"unknown_cause":null,"built_at":"2026-09-28T08:00:00+08:00","citations":0,"cite_coverage":null,"uncited_sections":[],"links_out_broken":0,"self_links":0,"frozen_by":null}` ⇒ **键全部在**，`cite_coverage` 是 `null`（不是 0.0/1.0），`freshness` 三态字符串在位，`built_at` 取页自己的 `generated_at` |
| `GET /api/v1/knowledge/wiki/links` | 同上 | keys 读数：`nodes,edges,broken,orphans,wanted,unreachable,self_links,degrees,readings_at`；`degrees[0] = {"slug":"e2e-consumption-probe","links_in":0,"links_out":0,"links_out_broken":0}` |
| `GET …/wiki/pages/{slug}/corrections` | **不存在** | `{"slug":"e2e-consumption-probe","corrections":[…]}`；空表时 `{"slug":…,"corrections":[]}` —— **空列表是读数，不是 404**（"没有纠错"与"没有这一页"是两件事） |
| `POST …/wiki/pages/{slug}/corrections` | **不存在** | `{"kind":"pin","reason":"probe: consumed by the t51 reading","author":"wiki"}` ⇒ `{"id":1,"correction":{"slug":"e2e-consumption-probe","kind":"pin","reason":"probe: consumed by the t51 reading","author":"wiki"}}`；`GET` 回读带 `at` 的完整行 |
| 拒收（未知名 / 空 reason） | — | `HTTP 400: unknown correction kind \`freeze\`; accepted: pin, release, note` · `HTTP 400: a correction must record why (reason is empty)` |
| 落地被**语义**消费 | — | 单测 `wiki_corrections_records_reads_and_refuses_by_name`：pin 之后 `active_correction(&db,"engine-notes") == Some(pin)`；release 之后回到 `None`；**三次拒收后表里仍只有 3 行**（拒收不落盘） |

### 1.2 `knowledge_api.rs` 逐字段断言（含 `links_out` 收窄）

**改前 → 改后**：wiki 测试的断言 **16 → 40 条**（文件 649 → 754 行；对象 = `async fn wiki_pages_and_links_inventory` 的函数体，改前取 `git show HEAD:…`，改后取工作树）。

`links_out` 的收窄用**反证夹具**证：把 `[[deploy-pipeline]]`（自链）写进 `WIKI_PAGE_A` 的正文。于是

| 读数 | 改前语义若仍在，会是多少 | 实测（改后） |
| --- | --- | --- |
| `links_out` | **3**（自链 + tea-notes + k8s 一起算） | **2** ⇒ 自链**不算**，断链**算** |
| `self_links` | 字段不存在（自链被静默丢弃） | **1** ⇒ 自链没丢，它**存在**但被排除 |
| `links_out_broken` | 字段不存在（断链混在 out 里） | **1** ⇒ 断链被单独报出 |
| 不变量 | — | `links_out == 解析到的出边(1) + links_out_broken(1)`；`degrees[slug]` 的四个数与 `/wiki/pages` 同名页**逐位相等**（两个端点不能各说一套） |

同一测试还逐字段覆盖了：`freshness`/`cite_coverage`（**键在、值为 null**）/`uncited_sections`/`citations`/`stale_since`/`stale_sources`/`stale_reasons`/`unknown_cause`/`built_at`/`frozen_by`/`self_links`/`links_out_broken`，以及 D.2 冻结的不变量 `stale == (freshness == "stale")`；`/wiki/links` 侧覆盖 `wanted`（带 `demanders`/`demand_count`）、`unreachable`、`self_links`、`degrees`、`readings_at`。
**顺带纠正一处旧笔记**：`degrees` 是**对象数组**（每节点一行：`slug/links_in/links_out/links_out_broken`），不是以 slug 为键的 map —— 我最初按 map 写断言，被测试当场打红。**注意来源**：契约（`gen2-integration-contract.md` L78）与规格的类型声明（`gen2-wiki-spec.md:323 pub degrees: Vec<PageDegree>`）**从头就是对的**，错的只是我的第一版断言与我自己几处 `degrees[slug]` 索引简写（§4.3 已撤回那条 finding）。

### 1.3 面板四页 DOM 读数 + 两条显示规则

命令（repo 的 e2e 入口；`E2E_BASE_URL` 指向我自己的临时守护进程）：

```powershell
cd panel; $env:E2E_BASE_URL="http://127.0.0.1:8899"; npm run test:e2e -- consumption.spec.ts
# → 4 passed (4.0s)
```

| 页 | DOM 级读数（原文，空白折叠后） |
| --- | --- |
| **Memory**（`#memory` → 审计日志） | `consumption激进前 3unknownmem 1 know 1 wiki 1 ent 1**记忆腿余弦 0.27**名次分 0.049刚刚` |
| **Knowledge**（`#knowledge`） | `e2e-consumption-doc**名次分 0.049**命中来源语义关键词语义腿原始分：**距离 0.8906**（LanceDB，越小越近）关键词腿原始分：**bm25 0.0000**（越负越好）# consumption probe …` |
| **Graph**（`#graph` → 列表） | 视图条 `实体图谱带双时间线的事实——被替代而非删除图列表+ 新建实体`；行 `e2e-consumption-entityprobe0 条事实consumption probe entity` |
| **Wiki**（`#knowledge` → Wiki） | 行 `Consumption probe孤儿页**覆盖 unknown**e2e-consumption-probe0 出 0 入`；徽标 `aria-label=覆盖 unknown` |

两条契约规则的落地位置与判据：

1. **并排显示必须带量纲（或改为不可比）** —— `Memory.tsx` 的召回日志行。改前：`mem 1 know 1 wiki 1 ent 1  m 0.86  名次分 0.016`（记忆腿只有一个字母 `m`，紧邻一个带量纲的名次分，读者必然比大小）。改后：记忆腿显示 `记忆腿余弦 {s}`（`title` 里写明"余弦 0–1，与旁边的名次分不同量纲、不能比较"）；命中列表里的裸 `{score.toFixed(2)}` 同样改成 `余弦 {s}`。**判据**：行文本必须含 `余弦|cosine` 与 `名次分|rank score`，且**不得**匹配 `/(?:^|\s)m \d/`（旧形状的假分子）。
2. **`number|null` 的三态显示** —— `Wiki.tsx` 的页行新增 `coverage` 徽标：`cite_coverage == null ⇒ 覆盖 unknown`（英文 `coverage unknown`），有记录值时 `覆盖 {v}`；**键不省略**，并带 `aria-label` 供 DOM 级取证，`title` 写明三态含义（unknown = 没有可用的构建期记录，既不是 0 也不是 1）。同一行的 `freshness === "unknown"` 也补了 `新鲜度 unknown` 徽标（与 `stale=unknown`/`edited=unknown` 同词表）。

**旧代码上真的红（反证，同一天同一夹具）**：把这两处显示改动**临时回退**（源文件备份→改回→`npm run build`→跑同一条命令→恢复→重建），读数：

```
DOM READING wiki page row: Consumption probe孤儿页e2e-consumption-probe0 出 0 入      ← 没有 coverage 这一段（键被省略）
✘ Wiki page: a page with no build reading shows coverage unknown      (element(s) not found)
✘ Memory page: the recall log labels each score with its dimension    (element(s) not found)
✓ Graph page / ✓ Knowledge page
2 failed / 2 passed
```
即：**这两条判据不是"顺手也能过"** —— 它们在改动前的面板上确实是红的；恢复改动后 `4 passed`（§6 有恢复后的行号读数）。

## 2 相邻面：G8 的 wiki lead 接线**未触碰**

- 本单**没有**改 `lead_for`/`lead_from`/`recall_stubs` 的任何一行，也没有改 `crates/memory` 的 `<wiki>` 渲染。理由：那是 t50 的缺口与 RV-INT 的判决对象；我 t34 已把生产侧的三态做完并登记了破坏性签名，**消费侧的重读归 t19/t50**。为避免两个属主同改一处，我在 `api.rs` 里也只碰 wiki 端点邻近的**纠错**路由，不碰召回响应装配。
- 具体地：`crates/daemon/src/wiki.rs` 在我这单里的改动**只有 3 处**，逐字如下（全在 `CorrectionKind` 的 derive/impl 内）：
  1. `#[serde(rename_all = "lowercase")]` + `pub enum CorrectionKind`（§4.1 的词表修复）；
  2. `fn parse` → **`pub fn parse`**（`api.rs` 要解析请求里的 kind）；
  3. 新增 `pub const ALL: [CorrectionKind; 3]`（拒收信息里点名可接受值）。
  它**没有**动 `lead_*` 系列函数、`WikiLead` 结构或 `recall_stubs`。
- **这条我给出诚实的证明形状**：`git diff` **不能**用来证明它 —— HEAD 早于 t10/t34，`wiki.rs` 的 diff 是 2564 行的 gen2 全量（里面**当然**有 `lead_for`/`lead_from`/`recall_stubs` 行，那些是 t34 的三态改造）。可机械核对的是另外两条：① 本单的三处编辑内容就是上面列的 3 条（本 turn 的编辑记录）；② `lead_for` 现在的签名与 t34 报告 §5bis.1 记载的**逐字一致**：`pub async fn lead_for(kb: &Knowledge, record: &RecordedPage, slug: &str) -> Option<WikiLead>`（即 t34 定稿的破坏性形状，本单没有再加一层）。

## 3 门禁与仪器（全部走 `scripts/cargo-team.ps1`）

| # | 命令 | 读数 | 两件证据（覆盖哪些 target + 本次确实重查了哪个包） |
| --- | --- | --- | --- |
| 1 | `test --workspace` | **exit=0**，195.1s，各套全部 `test result: ok`（含 `ruagent-daemon` 的 23 passed / `knowledge_api` 5 passed / `wiki_pipeline` 15 passed / `store` 32 passed 3 ignored） | 全仓测试 target |
| 2 | `clippy -p ruagent-mock-agent --all-targets -DenyWarnings -CleanFirst ruagent-mock-agent` | **exit=0**，11.63s，0 warning/error | 本次重查 **`ruagent-mcp` / `ruagent-mock-agent` / `ruagent-daemon`**（后两个来自 mock-agent 的 dev-deps）；覆盖 mock-agent 的 lib + 2 bins + 7 个集成测试 |
| 3 | `clippy -p ruagent-daemon --all-targets -DenyWarnings -CleanFirst ruagent-daemon` | **exit=0**，8.05s，0 warning/error | 本次重查 **`ruagent-daemon`**（输出含 `Checking ruagent-daemon v0.1.0`，且同一持锁调用内先 `clean -p ruagent-daemon`）；覆盖 daemon lib + lib 单测 + 3 个集成测试 |
| 4 | `cd panel; npm run build` | **✓ built in 5.58s**，1852 modules | — |
| 5 | 面板 e2e（§1.3） | **4 passed / 0 failed** | — |

**第 16 条（inScope 跨 crate ⇒ 门禁覆盖每个 inScope 文件所属的 crate）的落地**：本单的 inScope 是 `crates/daemon/…`（Rust，属 **`ruagent-daemon`**）**加上 `panel/`**（TypeScript，**不属于任何 Rust 包**）。所以两条 Rust 门禁都跑了（契约要求），但它们**覆盖不到 `panel/`** —— 面板的门禁是 `npm run build` 与 e2e 运行本身。

**更正（我先写错、随后自己读出真相）**：我原先在这里写了一条"TS 类型错误不在门禁声称面内（`npm run build` 只做 vite 构建、不做类型检查）"的盲区注 —— **这句话本身是错的，撤回**。真相有两层：
1. `panel/scripts/build-panel.mjs`（**未被我改动**，t319 提交的包装脚本）内部本来就按顺序跑 `node e2e/i18n-check.mjs` → `npx tsc -b` → `npx tsc -p e2e/tsconfig.json --noEmit` → vite，并在任何一步非零时**以同一退出码结束、且不动 `dist`**。所以本单 t51 期间每一次 `npm run build`（读数 `✓ 1852 modules transformed / ✓ built in 5.58s`、`exit=0`）**本来就已经包含了类型检查** —— 我漏读了包装脚本，把它当成裸 vite。
2. t53 又把 `check`（两个 `tsc`）**前置到 `panel/package.json` 的 `build`**里，于是即使有人直接调 vite 也绕不过类型检查。本单在 t53 之后重跑了一次：

```
cd panel; npm run build
→ i18n dictionary integrity — panel/src/i18n.tsx（信息行）
→ ✓ 1852 modules transformed. / ✓ built in 8.37s / exit=0
```

**准确的声称面**：本单的面板改动（`api.ts` 的类型、两处 i18n、两个 view、**新 spec 也走 `tsc -p e2e/tsconfig.json`**）**有类型检查覆盖**；Rust 的三条门禁不覆盖 `panel/`（这条由 AGENTS.md 与 t53 共同定下），面板自己有 `npm run build` + e2e 两条。**我把"未覆盖"写成了房子里的盲区，实际上只是我读漏了一层包装脚本 —— 记在这里当读数错误的样本。**

## 4 Findings（逐条给证据；不属于本单 inScope 的只报不改）

### 4.1 已修（t10 留下的词汇不一致）：`CorrectionKind` 的 wire 词表与存储/解析词表不同

- **现象**：枚举 `Pin/Release/Note` 的 serde 默认序列化是 `"Pin"`，而 `add_correction` 落库用 `as_str()` = `"pin"`、读回用 `CorrectionKind::parse("pin")`。于是 **POST 的响应回显 `"Pin"`，而端点只接受 `"pin"`** —— 客户端把刚读到的值回传就是 400（本代在干跑词汇上踩过同一个坑，G7）。
- **读数**：修之前的测试断言 `v["correction"]["kind"] == "pin"` 直接红（实测 body 是 `"kind":"Pin"`）。
- **修法**：`#[serde(rename_all = "lowercase")]`（wire = 存储 = 解析 = 三个 token），并加一条**回环判据**：把响应里回显的那个字符串再 POST 回去，必须 200（读数 `READING round-trip: echoed=pin HTTP 200 OK`）。

### 4.2 只报不改：`POST /api/v1/memory/write` 接受 `namespace: "global"` 却回 `RejectedNamespace`

- **读数**：`HTTP 200 {"outcome":"RejectedNamespace"}`（同一路径 `namespace:"user"` ⇒ `{"outcome":"Inserted(1)"}`）。R-B 规格的命名空间词表写的是 `user | global | project:<x> | agent:<x>`；这里 `global` 被写侧拒绝，而且**以 200 + outcome 的形式**而不是 400。
- **影响**：文档说可用的值被静默拒收（列表里查不到刚写的行，容易读成"写成功但读不到"）。
- **归属**：`crates/memory` 的命名空间校验（我 inScope 外）⇒ **只登记**。我的 e2e 夹具改用 `namespace: "user"` 并在代码注释里写明原因（没有静默绕过问题，见 `panel/e2e/consumption.spec.ts` 的 beforeAll）。

### 4.3 【**已撤回**】图端点读写路径与 `degrees` 形状：**没有任何文档写错**

> **原文保留（t51 提交时的写法）**：`POST /api/v1/graph/entity`（单数）是创建、`GET …/entities`（复数）是列表，复数 POST 是 405（实测）；`WikiLinks.degrees` 是对象数组不是 slug→map；"这两条都只影响读者，我未改任何路由/类型"。
>
> **更正（t53 逐条取证后，captain 要求点名文件；我点名后发现没有可点的文件 ⇒ 撤回）**：
> - **没有任何文档写过复数 POST**。`docs/design/reviews/gen2-integration-contract.md` **L92 原文就是 `POST /api/v1/graph/entity`（单数）**，与路由表 `crates/daemon/src/api.rs:53` 一字不差；复数只有 `GET`（`api.rs:49`）。全 `docs/` 里唯一出现 `POST …/graph/entities` 字样的地方是**该契约 L93 的 t53 读法注本身**（它在说"**没有**这条路由"）。
> - **没有任何文档把 `degrees` 写成 map**。同一份契约 **L78 原文就是 `degrees:[{slug,links_in,links_out,links_out_broken}]`（对象数组）**；`grep degrees` 全表命中 3 处（L78/L267/L427）**都是数组形状**。
> - 我当时的预期来源只有两处，**都在我自己的文件里、而且是"索引简写"而不是类型声明**：`docs/design/reviews/gen2-wiki-spec.md:198`（`degrees[slug].links_out`）与 `:342`（`degrees[s].links_out`）、`docs/design/reviews/gen2-wiki-impl.md:189`（`links.degrees[slug].links_out`）—— 而**同一份规格 `:323`/`:331` 早就写对了类型**（`pub degrees: Vec<PageDegree>` + `struct PageDegree`），我"按 map 写断言"的第一版才是真正错的那一处，它已被测试当场打红并由我改掉。
> - **结论**：这条 finding 作为"文档级错误"**不成立，撤回**。它真实对应的现象只是"我自己的两版代码预期（复数路径、map 形状）与契约不符，而契约是对的"。
> - **顺带点名（供 captain 决定是否立单）**：上面三处**索引简写**（`gen2-wiki-spec.md:198`、`:342`、`gen2-wiki-impl.md:189`）读起来像 map 访问，虽然同文件 `:323` 已声明数组类型。这是**措辞精度**问题、不是契约错误；那两份文件不在本单 inScope，所以我只点名不改（一句话即可改成「`degrees` 中该 slug 的那一行」/`degrees[i]`）。

## 5 未测 / 不做（不静默）

| # | 项 | 状态 | 原因 |
| --- | --- | --- | --- |
| U-1 | `cargo test --workspace` 上的面板 e2e | 分离 | 面板 e2e 不进 cargo（`npm run test:e2e`），两条读数各记各的 |
| U-2 | ~~`tsc --noEmit` 类型检查~~ **已覆盖** | **已取得** | t53 把 `tsc -b --noEmit && tsc -p e2e/tsconfig.json --noEmit` 接进 `npm run build`；本单在 t53 之后重跑：`exit=0`、`✓ built in 8.37s`（§3 的更正段）。**原先写"未跑"已作废** |
| U-3 | 真库（`~/.ruagent`）上的任何读数 | **未做** | 纪律：所有端到端只在临时 root + 临时端口（8899），真守护进程 pid 79984 未启停 |
| U-4 | G8 的消费侧重读 | **未做** | 归 t50/RV-INT（§2）；本单只保证生产侧与读端点一致 |
| U-5 | 面板 e2e 全量套件 | **未跑** | 只跑 `consumption.spec.ts`（契约要求的是"每页至少一条 DOM 读数"）。全量套件含 `registry.spec.ts`（会写配置），应由集成阶段在专用 daemon 上跑 |

## 6 纪律回执

- **临时资源**：临时 root `%TEMP%\ruagent-t51-root`（已删）· 临时端口 **8899** · 我起的守护进程 **pid 15172** —— `Stop-Process -Id 15172` 后实测 `alive after stop: False`；**真守护进程 pid 79984 未启停**（读数 `79984` 仍在）。
- **只改 inScope**：写入集合 = `crates/daemon/src/api.rs`(`M`) · `crates/daemon/src/wiki.rs`(`M`) · `crates/daemon/tests/knowledge_api.rs`(`M`) · `panel/src/api.ts` · `panel/src/i18n/{wiki,memory}.ts` · `panel/src/views/{Memory,Wiki}.tsx` · `panel/e2e/consumption.spec.ts`(`??`) · 本报告(`??`)；`A-D` 的 `git status --porcelain -- panel` 除 `src/`、`e2e/` 外为空（`test-results` 全部落在 `PLAYWRIGHT_OUTPUT_DIR` 指向的 TEMP）。
- **e2e 的写侧**：`npm run test:e2e` 会 `RUAGENT_E2E_ALLOW_WRITES=1`，但它指向**我的临时守护进程**，`registry.spec.ts` 也没在本次运行范围内（只跑 `consumption.spec.ts`）⇒ 真实 `agents.toml` 未被触碰。
- **读数三件套**：对象集（4 个源文件 + 1 个新 spec + 本报告；被测端点与页面的具体 URL/路由在上文逐条给出）· 采样面（HTTP 逐字段 + 40 条断言 + 4 条 DOM 读数 + 旧代码回退对照）· 可证伪判据（每条显示规则都在**回退后的代码上重跑并确认变红**；门禁两条各自带"覆盖哪些 target + 本次重查哪个包"）。
