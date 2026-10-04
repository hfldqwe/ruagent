# 活库探针残留清理（t153）—— memory 软删完成；`recall_log` 无合规路径，如实留置

> 单号 **t153**（repair）· 成员 `recall` · attempt **1** · 2026-10-04 17:28–17:31
> inScope：**本报告**。`crates/**`、`panel/**`、`scripts/**`、`.github/**`、`tools/**` **零改动**。
> 来源 = **用户的决定**（「清掉」）。硬约束（AGENTS.md）：**所有 SQLite 写入必须经 `store` 的单写者 actor；活守护进程在跑时绝不允许开第二条写连接**。

## 0 结论

| 项 | 结果 |
| --- | --- |
| **memory 残留** | **清掉了**（**软删**，走**守护进程自己的** API）：`DELETE /api/v1/memory/165` → **200** `{"outcome":"deleted","id":165,"deleted_at":"2026-10-04T09:29:15.860704700+00:00","soft":true}`；同一行读回 `deleted_at` 已置位、列表端点不再返回它；`memory_diffs` 新增一行 `op=delete … reason=soft delete` |
| **`recall_log` 残留（4 行）** | **未删 —— 不存在「定向删这几行」的合规路径**（§4）。**我没有**为完成而开第二条写连接、**没有** `DELETE`/`VACUUM`/停守护进程，**如实留置**并给出出路供你/用户选 |
| **只动那一行** | **65 张表全量 `COUNT(*)` 对照**（删除前快照 vs 删除后快照）⇒ **只有 `memory_diffs` 变化（241 → 242）**，其余 **64/65 张表逐张相等**（含 `recall_log` **657 → 657**） |
| **活守护进程** | **全程未启停**：开工 **17:28:22** health **200** · 收尾 **17:30:27** health **200** · listener 两次都是 **pid 14944** |

## 1 残留逐条取证（只读；两个独立快照）

**方法**：把 `ruagent.db` + `-wal` + `-shm` **复制到仓外**（复制动作只**读**原库），对**副本**以 `file:…?mode=ro` 打开。快照 **A** = 17:28（删除前）· 快照 **B** = 17:30（删除后）。`sqlite3` CLI 本机没有，用 python 的 `sqlite3`（3.50.4）。**读完后两个副本已按具体路径删除**（含用户记忆数据，§6）。

**① memory 残留 = `#165`**（只出现在 `memories` 表）

| 字段 | 值 |
| --- | --- |
| id / store / namespace | **165** / `observation` / `user` |
| created_at | `2026-09-28T20:54:56.912909700+00:00` |
| content | `consumption probe memory: the recall log reads the memory leg` |
| deleted_at（删除前） | `NULL` |
| 上下文 | `memories` 共 **164** 行，其中 `deleted_at IS NOT NULL` = **0**（⇒ 软删后应正好为 1） |

**② `recall_log` 残留 = 4 行，同一个探针会话**（20:54:56 写入 memory ⇒ 20:55:02 连查 4 次）：

| id | ts | query | memories / knowledge / wiki / entities | source |
| --- | --- | --- | --- | --- |
| **652** | `2026-09-28T20:55:02.307150400+00:00` | `consumption` | 3 / 3 / 1 / 0 | NULL |
| **653** | `2026-09-28T20:55:02.556231600+00:00` | `consumption` | 3 / 3 / 1 / 0 | NULL |
| **654** | `2026-09-28T20:55:02.569004700+00:00` | `consumption` | 3 / 3 / 1 / 0 | NULL |
| **655** | `2026-09-28T20:55:02.597355500+00:00` | `consumption` | 3 / 3 / 1 / 0 | NULL |

⇒ 与「先前记录 = 4 条」**逐条对上**（同一查询连打 4 次、腿计数完全相同）。`recall_log` 共 **657** 行。

**③ 同一分钟还有 2 行，我**没有**把它们算进那 4 条、也**没有**动**：`#656` `kettle`（20:55:14，5/5/0/0）· `#657` `autohotkey-v2`（20:55:19，5/5/2/0）。理由：这两句查询词与 09-26/27 的 `#646–#651`（`kettle`/`autohotkey-v2` 交替）**同型**，而源码写明它们正是**自检探针**用的查询词——`crates/daemon/src/api.rs:4105-4116`：「the self-check probes alone wrote 554 of the 574 live rows (t247)…the only signal that separates the self-check probe from a real user is the query text ("kettle material")」。⇒ **656/657 的归属请你/用户定**；本单按「记录在案的 4 条」= 652–655 处理。

**④ 更早的 probe 痕迹（不在本单范围，未动）**：`#140` `T125 probe memory`（2026-09-23）、`#141` `(t125 probe cleanup: superseded, no content)`——上一代已经处理过的痕迹（#141 自带说明），**我没有碰**；`#139` 是一条**正当的 lesson**（探针/标签前缀的处置规则），更不该碰。

## 2 memory：走守护进程自己的 API 软删（合规路径）

**请求/响应（原文）** —— 2026-10-04 17:29:15：
```
DELETE http://127.0.0.1:8787/api/v1/memory/165
HTTP 200
{"outcome":"deleted","id":165,"deleted_at":"2026-10-04T09:29:15.860704700+00:00","soft":true}
```
**同一条读回来**（`GET /api/v1/memory/165`）⇒ **HTTP 200**，行**仍在**，`deleted_at` 已置位：
```
{"memory":{"id":165,…,"content":"consumption probe memory: the recall log reads the memory leg",
           "deleted_at":"2026-10-04T09:29:15.860704700+00:00","created_at":"2026-09-28T20:54:56.912909700+00:00",
           "updated_at":"2026-10-04T09:29:15.860704700+00:00","source_episode":18}}
```
**列表端点不再出现**（`GET /api/v1/memory/list?limit=500`）：`total`=**157**、`matched`=**157**、响应里**没有 id 165**、响应体里**不含** `consumption probe memory` 这个字符串 ✓。

**留痕（删除后快照的 `memory_diffs` 末行）**：
```
id=242 | ts=2026-10-04T09:29:15.861774200+00:00 | op=delete | mem_store=observation | namespace=user
       | before=id=165 | after=None | reason=soft delete
```
**合规性**：该路径就是契约指的 `crates/daemon/src/api.rs:87`（`get(memory_get).delete(memory_delete)`），handler 在 `:5145`，非 `purge` 分支调 `ruagent_memory::delete_memory(state.mgr.db(), id)` ⇒ **写的是守护进程自己的 store actor** ✓。
**未用硬删**：同一 handler 还有 `?purge=true` 分支（`:5153` → `ruagent_memory::purge_memory` → `DELETE FROM memories`）。本单按契约走**软删**（保留行 + 留痕，可审计），**我没有**用它。

**一处要讲清楚的读数（避免被读成「删多了」）**：列表 `matched`=157 而表里 164 行 ⇒ **7 行不在列表里**。其中**我造成的只有 1 行**（就是刚软删的 #165，我直接验了它 `deleted_at` 已置位且不在那 157 里）。**另外 6 行与我的改动无关**：它们被 store 自己的「非当前行」过滤排除——我在 `#164` 上直接量到 `superseded_at=2026-09-26T14:59:24.902230800+00:00`（`GET /api/v1/memory/164`），而 supersede 是常规路径（`api.rs:85` 有 `/api/v1/memory/supersede`）；`crates/memory/src/lib.rs:164` 也写明该查询过滤 `deleted_at IS NULL`。**如实标注**：那 6 行我**没有逐行枚举**（快照已按纪律删除），此处是「6 = 164 − 157 − 1」的差集 + 机制已在 #164 上验证，不是逐行点名。

## 3 「只动那一行」：65 张表的全量计数对照

对快照 A（17:28，删除前）与快照 B（17:30，删除后）逐表 `SELECT COUNT(*)`：

```
=== COUNT(*) diff over ALL 65 tables (before -> after) ===
  CHANGED  memory_diffs: 241 -> 242  (delta +1)
  unchanged tables: 64 of 65
  memories rows            : 164 -> 164          （软删不删行 ✓）
  memories with deleted_at : 0 -> 1              （只多了我这一行 ✓）
  memory_diffs rows        : 241 -> 242          （软删的留痕，按设计 ✓）
  recall_log rows          : 657 -> 657          （recall_log 一行未动 ✓）
  memory #165              : deleted_at=2026-10-04T09:29:15.860704700+00:00 (updated_at 同步)
```
⇒ **只有 `memory_diffs` 多了 1 行（那是软删按设计写的留痕），其余 64 张表逐张相等**。**没有**批量清理、**没有**按模糊匹配删、**没有** `UPDATE` 任何行、**没有** `VACUUM`。

## 4 `recall_log`：**没有**合规路径 ⇒ 如实上报 + 出路（不自己硬做）

**查证（只读全仓搜索）**：
1. **没有任何端点能删 `recall_log`**：全仓 `delete(...)` 路由只有一条 —— `api.rs:186` `/api/v1/chats/{id}`。`recall_log` 只有**读**端点 `api.rs:200` `GET /api/v1/recall/log`。
2. 全仓唯一的 `DELETE FROM recall_log` 是 **保留期清扫**（`api.rs:3831`，在召回**写入路径**里、经 store 的单写者 actor 执行 ⇒ 本身合规）：
   `DELETE FROM recall_log WHERE id <= (SELECT MAX(id) FROM recall_log) - ?1`，`RECALL_LOG_KEEP = 5000`（`api.rs:4109`）。它是**按期/按年龄批量**的 —— **无法指定行**，用它等于**批量删**（违反本单「只动那几行」），而且**它今天根本不动**:表里 657 行 ≪ 5000 ⇒ **no-op**；要靠它自己带走这 4 行，需要再累积 **4,343** 行新召回。
3. CLI 无维护子命令（现为 `serve / doctor / run / knowledge / wiki / build / show / ingest`）⇒ 没有 `ruagent db purge` 这一类出口。
4. 我也没有对活库**打开任何写连接**，所以**不存在**「绕过 actor 直接删」这种选项（AGENTS.md 明令禁止）。

**⇒ 结论：不存在「定向删掉这 4 行」的合规路径。我因此不做，并把它如实留在库里。**

**建议出路（三种，按我看到的代价排序；请你/用户决定，我不替你选）**：
- **① 加一个维护端点**（唯一能**定向**清的合规形态）：例如 `DELETE /api/v1/recall/log?before=<ts>&reason=…`，实现上仍由 store 的 actor 执行；顺手可把「谁在清、清了几行」写进一个审计行。代价 = 一次实现单（本单 inScope 不许我改 `crates/**`）。
- **② 由用户在守护进程停止时清**：窗口内没有第二个写者，用外部工具删这 4 行即可。代价 = 要停服务（本单纪律禁止我启停），收益 = 立刻干净。
- **③ 当作 append-only 历史保留**：该表语义本来就是「调参样本，不是账本」（`api.rs:4105-4108`），且保留期清扫会在超过 5000 行后**按年龄自动**带走它们（合规、无需动作，代价是等 4,343 行）。
- 我的倾向（仅供你参考）：**① 或 ③**。**这 4 行没有假数据风险**——它们只是「谁在何时查了什么」的样本，不影响任何读路径的语义（`recall_log` 是观测面，不参与召回打分）。

## 5 未覆盖 / 未做（第 19 条）

**未覆盖**
1. **`recall_log` 的 4 行未清** —— 不是遗漏，是 §4 的合规约束下**故意留置**。
2. `#656/#657`（同一分钟的 `kettle`/`autohotkey-v2`）**归属未定**，未动（§1③）。
3. `#140/#141/#139` 等更早痕迹**未动**（不在本单范围）。
4. 那 6 行「非当前」记忆**没有逐行枚举**（§2 末，差集 + 机制验证）。
5. 面板侧**未验**（软删行在 UI 上如何呈现、`#memory` 列表是否隐藏它）—— 面板不在本单范围。
6. 硬删（`?purge=true`）**未验证**（我按契约只用软删）。
7. `recall_log` 里其它 653 行**未逐条审**：本单只按时间窗锁定 652–657，未做全表分类。

**没有做的危险动作（逐条）**
- **未启停/未重启/未触碰活守护进程**（8787 = pid **14944**）；未 `taskkill`、未按端口/进程名批量杀任何东西。
- **未开第二条写连接**；对库的所有读取都在**仓外副本**上以 `mode=ro` 进行。
- **未** `DELETE` / `UPDATE` / `VACUUM` / `PRAGMA`-写 / `DROP` 任何东西。
- **未**批量清理、**未**按模糊匹配（`LIKE`）删任何行（搜索只用于**定位**，没有用它生成删除）。
- **未**用 `?purge=true` 硬删；**未**动 `#140/#141/#656/#657`；**未**碰 `memory_diffs` 的历史行。
- **未**改 `crates/**`、`panel/**`、`scripts/**`、`.github/**`、`tools/**`（我唯一的写入 = 本报告）。

## 6 纪律回执与证据

- **两次 health 读数（契约要求的开工/收尾）**：
  - 开工 **2026-10-04 17:28:22** ⇒ listener pid **14944**、`GET /api/v1/health` **200** `{"status":"ok","service":"ruagent","db":{"writer":"running","writer_panics":0,"last_panic":null}}`
  - 收尾 **2026-10-04 17:30:27** ⇒ listener pid **14944**、health **200**（同上 body）
  - 契约的两条 verify 命令**逐字**跑：`(Get-NetTCPConnection -LocalPort 8787 -State Listen).OwningProcess` → `14944`（exit 0）；`$env:NO_PROXY='127.0.0.1,localhost,::1'; (Invoke-WebRequest -UseBasicParsing http://127.0.0.1:8787/api/v1/health).StatusCode` → `200`（exit 0）
- **只读方式**：副本 + `file:…?mode=ro`（python `sqlite3` 3.50.4；本机无 `sqlite3` CLI）。副本一旦读完即删。
- **收尾清理（按具体路径，不用 glob）**：删 `%TEMP%\t153\copy`（删除前副本，24,272,896 B + 4,688,592 B WAL + 32,768 B SHM）、`%TEMP%\t153\copy-after`（删除后副本，同尺寸）、`%TEMP%\t153_forensics.py`、`%TEMP%\t153_beforeafter.py`；复查 `%TEMP%\t153` 下**已无文件**。删副本的理由：它们含有用户的记忆正文（隐私面）。
- **我的写入集合**：仅 `docs/design/reviews/gen4-live-db-probe-cleanup.md`（本报告）。工作树里其它脏文件（`crates/daemon/src/chat.rs`、`panel/src/views/Settings.tsx`、若干 `docs/design/reviews/gen4-*.md`）**都是队友在途**，不是我。

## 7 事后追记：captain 的裁决（§4 的处置已落定，读 §4 请连读本节）

> ★ **本节是本单 completed 之后追加的**（同一文件、同一作者，**只加一节，上面任何读数一字未改**）。原因：§4 的三条出路与我的倾向（「①或③」）**已有处置**，只读 §4 的下一位读者会把一个**已被换掉的形态**当成待选项——这正是本代反复吃过的「建议/读数的状态在读者手里已经变了」那一类（AGENTS.md「a green gate certifies the BYTES it ran on」的同族）。

**裁决 = ③（append-only 历史保留），三条理由（captain 原话要点）**：
1. 这 4 行是**观测样本、不参与召回打分**（本报告 §4 实测）⇒ **无假数据风险**，与「污染了用户的真实记忆」不是一回事；该清的那一半（memory #165）已用软删 + 留痕合规清掉。
2. 该表的设计语义**就是有上限的 append-only 日志**（保留期清扫 `KEEP=5000`）⇒ 它会**按年龄自己带走**，不必为一个只为 4 行存在的破坏性能力开口子。
3. **① 的原始形态有安全代价**：本仓 HTTP API **没有鉴权**（AGENTS.md 自载「a reachable instance can rewrite memory bodies」）⇒ 在它上面加**定向删除**端点 = **扩大无鉴权的破坏面**；为 4 行观测样本做这件事**代价与收益不成比例**。

**① 没有被否掉，而是换形态登记为候选（待用户表态）**：**离线维护子命令**（CLI，例如 `ruagent recall-log prune --before <ts> --reason …`），硬要求 = **守护进程持库时拒绝运行**，且拒绝条件必须**可判定**；建议实现两层：① 读 `~/.ruagent/data/daemon.pid` + `GET /api/v1/health` 通 ⇒ **拒绝并非零退出**，打印它检测到什么；② 拿不到库的写锁 / 不经过 `store` 的单写者 actor 就不写。这样运行时**只有它一个写者** ⇒ 合规，且**不扩大 HTTP 面**。它仍只服务「定向清历史」这一类需求 ⇒ 记成**候选**；若用户要，请开实现单（本单 inScope 不许改 `crates/**`）。

**#656/#657 的归属已入账**：captain 按 `api.rs:4105-4116` 的记载把它们归为**自检探针家族**，**不算本单残留**；§1③「我没把它们算进 4 条、也没动它们」的处理被确认为正确。

**裁决后本单未新增任何动作**：我**没有**再碰库（无删、无改、无重开连接），活守护进程仍未启停；上面 §0–§6 的全部读数**仍是裁决前那批字节/那批时刻**的读数。
