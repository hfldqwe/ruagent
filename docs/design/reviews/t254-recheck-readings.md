# t254 独立复核读数阶段 t245–t249

复核者：ui-audit · 日期：2026-09-26 · 任务 t254（verification round 1）
方法：不采信四份报告的自述；只读重放 + 钉住修订重跑；每条给「复现命令 → 输出 → 判定」。

## 0 对象集 / 采样面 / 环境

| 项 | 值 |
| --- | --- |
| 对象集 | 四份读数报告（t245 harness、t246 记忆召回、t247 查询构造、t248 语料入库、t249 四页 IA）及其产物 |
| 采样面 | 只读 DB `file:~/.ruagent/data/ruagent.db?mode=ro` · 只读 HTTP GET · `git archive 1795dc2` 钉住修订重跑 harness · 文件系统 walk |
| 未做的事 | **未调用 /api/v1/recall**（会写 recall_log）· 未跑 doctor · 未改任何产物（inScope 只有 docs/design/reviews/） |
| 活库自变的证据 | recall_log 574 → **582** 行、memories 154 → **164** 行：守护进程在蒸馏写库，不是我的动作（我只跑 GET 与只读 SQL） |

### 0.1 一条坑，把「在工作树里重跑 harness」直接挡住

在工作树里跑 `cargo test -p ruagent-knowledge --test retrieval-quality` **编译失败**：

```
error[E0433]: cannot find `fts` in `ruagent_store`  --> crates\knowledge\src\store.rs:508:36（另 515/519/527）
error[E0277]: the size for values of type `str` cannot be known  --> crates\knowledge\src\store.rs:512:21
链接的依赖：--extern ruagent_store=D:/rust_cache\debug\deps\libruagent_store-d7f32265aa2a6468.rmeta
```

同一时刻：`crates/store/src/lib.rs:8` 有 `pub mod fts;`（已提交、非在途改动）· `cargo build -p ruagent-store` 自己**无错** ·
那个被链接的 rmeta mtime = **19:20:29**（正是我 t256 两棵临时树构建的窗口），而更新的 store rmeta（19:52:47）就在同目录。

⇒ **归因：共享 `CARGO_TARGET_DIR` 的跨树产物串扰**（t256 已登记的那条陷阱，这次踩在工作树上；其中一份产物是我 t256 的方法留下的），
不是 store 源码坏了。修法（未代修，会动别人在跑的构建）：`cargo clean -p ruagent-store`，或对比用的树各自独立 target dir。
**这条本身是给全队的读数：谁现在在工作树里跑 knowledge/daemon 的测试，都会看到同一份假错误。**

## ① acceptance #1：独立重跑 t245 的 harness 两次 → **逐字节一致**

复现命令（钉住修订，避开串扰）：

```bash
git archive 1795dc2 | tar -x -C C:/tmp/t254/t245
# 包版本改成 0.2.1（knowledge/store），让 rlib 身份与工作树不撞
CARGO_TARGET_DIR=D:/rust_cache cargo test -p ruagent-knowledge --test retrieval-quality -- --nocapture   # 跑两次
```

| 判据 | 输出 |
| --- | --- |
| 仪器版本 | `crates/knowledge/tests/retrieval-quality.rs` md5 **b7549abacc915c33eb79560032721825** |
| 进程内（harness 自己的 assert） | `[t245] byte-identical across two runs = yes` · `test result: ok. 2 passed` |
| **进程级（两次独立运行各写一次 JSON）** | 两次都写 `$CARGO_TARGET_DIR/retrieval-quality.json`；md5 两次都是 **24bff1a78a4285bf93052de6d194fb51**，`cmp` 无差异，**6405 字节** ⇒ **逐字节一致 ✓** |

顺带复现 t245 的四个读数（同一份 JSON）：

```
metrics.fused = { recall_at_1: 0.6667, recall_at_5: 0.8667, mrr: 0.75, n: 15 }
chinese rows gold_rank = [('泡茶 水温', None), ('手冲咖啡 研磨度', None)]
no_answer_top_scores = 三条全 0.016393（= 1/61 = 0.01639344262295082）
absent_legs = [semantic, keyword, entity] · provenance: 18 queries / 15 answerable
```

## ② 逐条判定（t245–t249）

判定口径：**已复现** = 我用独立命令拿到了同一事实；**未复现** = 拿到相反事实；**无法判定** = 现有手段（只读、inScope 外不能加探针）拿不到。
「源码」标注 = 复现的是代码路径，运行时那一半另标。

### t245 harness

| 结论 | 判定 | 复现命令 → 输出 |
| --- | --- | --- |
| fused recall@1 0.6667 / recall@5 0.8667 / MRR 0.7500（n=15） | **已复现** | 见 ① |
| 两条中文查询 gold_rank = null | **已复现** | 见 ① 的 chinese rows |
| 三条无答案查询 top_score 全 = 0.016393 = 1/61 | **已复现** | 见 ① |
| 两次运行逐字节一致（assert 在测试里） | **已复现** | 进程内 yes + 进程级 md5 相同（比报告更强：跨进程也比了） |
| 「逐腿列由 t256 在 t250 落地后改成从产物取」 | **未复现（是计划不是读数）** | t245 修订的 JSON 里三腿仍是 ABSENT；工作区那份改动至今**未提交**（t256 报告 ④） |

### t246 记忆召回

| 结论 | 判定 | 复现命令 → 输出 |
| --- | --- | --- |
| ③ 嵌入覆盖 154/154、embedder 一致、dims 全 384、current 153 | **已复现（口径）/ 数值已变** | 只读 SQL：total **164**、with_embedding **164**、embedder 唯一 `fastembed:multilingual-e5-small`、dims 唯一 **384**、current **163** ⇒ 结论（无维度错配、覆盖完整）成立，数字随蒸馏增长 |
| ① FTS 腿不是被丢弃（合成记忆 7>5、反向控制 len=2、真语料 15/15） | **无法判定** | 探针 `crates/daemon/tests/t246_probe.rs` 已删，重建要动 crates/（inScope 外）。可复现的只有源码那一半：`api.rs:2344/2345` 对 wiki/knowledge 腿 `take(top_n)`，记忆侧 `out_memories.push` 无上限（`api.rs:2403`）⇒ 「合并不受 top_n 约束」源码成立 |
| ② 关键词条目没有 score / 两腿分数不可比 | **未复现（已被修）** | 现值 `api.rs:2398-2420`：记忆条目已带 `score` + `score_kind: "rrf_rank"` + `legs` + `semantic_score` + `keyword_score`，且注释直接写「the panel showed the knowledge leg's RRF score next to the memory leg's cosine … and invited exactly that misreading (t247)」⇒ 报告时的结论**已过时**（改动未提交） |
| ② min_score 默认 0.0、记忆侧阈值三份字面量 | **已复现（源码）** | `grep -n min_score crates/daemon/src/api.rs`；运行时「裁掉结果的是 top_n 不是阈值」无法复现（同上，探针已删） |

### t247 查询构造与分数

| 结论 | 判定 | 复现命令 → 输出 |
| --- | --- | --- |
| ② 574 行只有 3 个取值且逐位等于 f32 公式：0.016393441706895828×295 / 0.032522473484277725×276 / 0.032266460359096527×3 | **已复现** | 只读 SQL（现 582 行）：**同一组三个值** 0.016393441706895828×**301** / 0.032522473484277725×**278** / 0.03226646035909653×**3** ⇒ 可达取值集合不变，只有计数增长 |
| ② 上界 2/61、观测最大值 = 上界的 99.19% | **已复现** | `MAX(top_knowledge_score)` = 0.032522473484277725，2/61 = 0.03278688524590164 ⇒ 99.19% |
| ① 实体腿对 autohotkey-v2 / 潜艇 / 麒麟 命中 0；真实 15 条查询实体腿 0/15 | **已复现** | t256 复跑的 strict 列即改前行为（graph/src/lib.rs 在 t250 是 +89/−0）：三格全 0；只读 SQL `SELECT COUNT(*) FROM recall_log WHERE entities != 0` = **0 / 582 行** |
| ① 引号化修掉一类真错（autohotkey-v2 → no such column: v2 等） | **已复现（源码）/ 未跑 SQL** | 未在本次重建 FTS5 表复现那四条错误串；报告给的错误串我未独立复现 ⇒ 判为「部分复现，运行时那半无法判定」 |

### t248 语料与入库

| 结论 | 判定 | 复现命令 → 输出 |
| --- | --- | --- |
| ② 可入库字节量：transcripts 146 文件 / 27.83 MB、.claude/projects 136 / 103.89 MB | **已复现（逐字节相同）** | `python os.walk`：transcripts **146 / 29,186,749 B**、`.claude/projects` **136 / 108,933,341 B** —— 与报告完全一致 |
| ② `~/.ruagent/knowledge` = 5 文件 / 3,516 B（含残留 doctor-probe.md 111 B） | **已复现（口径）/ 数值已变** | 现值 **4 文件 / 3,405 B**（`ahk-notes 236 / ops-handbook 664 / wiki/autohotkey-v2 2218 / wiki/index 287`）——残留那份已被清掉 |
| ② 仓库 docs/ = 665 文件 / 70.45 MB | **已复现（口径）/ 数值已变** | 现值 **668 文件 / 73,899,495 B (70.48 MB)** |
| ① scan 唯一调用点 daemon/lib.rs:202（boot + 每 60s）· rebuild 只在 api.rs | **已复现（源码）** | `grep -n 'scan(' crates/daemon/src/lib.rs` → :202 `kb_scanner.scan()`（:199/:208/:211 同一段） |
| ① 写进 knowledge 的入口只有 4 处 save | **已复现（源码）/ 行号漂移** | 现值：`api.rs:605`、`api.rs:3668`、`wiki.rs:1179`、`wiki.rs:1267`（报告写 api.rs:604/3411 —— 在途改动把行号推了） |
| ① 「没有把 docs/ 或 transcripts 自动喂进知识库的路径」 | **无法判定（是论据）** | 见 ③-1 |

### t249 四页 IA

| 结论 | 判定 | 复现命令 → 输出 |
| --- | --- | --- |
| 知识库搜索展示的 score 是 RRF 名次分（0.016 等） | **已复现** | 只读 GET `/api/v1/knowledge/search?q=RRF&limit=3` → `[0.016393, 0.016129, 0.015873]` = 1/61, 1/62, 1/63 |
| 召回日志在 #stats（App.tsx:48 → Agents.Stats），不在 #agents | **已复现（源码）** | `sed -n '46,49p' panel/src/App.tsx` → `stats: () => import("./views/Agents").then((m) => ({ default: m.Stats }))`（DOM 层未重跑，见 ④） |
| memory readout = 当前（未取代）行数：43/43/27/40 | **已复现（口径）/ 数值已变** | 只读 SQL：`lesson 40 / observation 51 / procedure 28 / profile 44`（现值；报告时 43/43/27/40） |
| confidence < 0.5 的行 = 0 ⇒ 该读数 154/154 不可见 | **已复现（口径）/ 数值已变** | 只读 SQL：`confidence < 0.5` 计数 = **0**（现 164 行） |

## ③ 「是论据不是读数」的陈述（4 条）

1. **t248 ①**：「⇒ 判定：**没有**把 docs/ 或 transcripts 自动喂进知识库的路径」。
   它是 grep 的推断；报告自己给出的可证伪判据（「在 `<root>/knowledge` 放一个新 .md 后 60s 内 documents 行数不增 ⇒ 本判据错」）
   **从未被执行**。⇒ 论据，不是读数。
2. **t246 ①**：「合并不受 top_n 约束 ⇒ **上界 2*top_n**」。
   上界是**代码推出来的**（`take(top_n)` + 无上限 push）；唯一观测值 7>5 来自一个已删除的合成探针。
   我这次能独立复现的只有源码那一半，运行时上界没有第二份证据。⇒ 论据。
3. **t247 ①**：「引号化确实修掉一类真错（**否则 api.rs 会 400 整条失败**）」。
   括号里那句是对**另一个代码路径**（handler 的错误映射）的推断，没有被测；测到的是 FTS5 的 SQL 错误串。
   ⇒ 一半读数（错误串）、一半论据（400）。
4. **t245**：「逐腿列由 t256 在 t250 落地后改成从产物取」。
   这是**计划**，被写在一份读数报告的结论栏里；实测 t245 修订的 JSON 三腿仍 ABSENT，工作区那份改动至今未提交。⇒ 论据。

## ④ 未测 / 无法判定（不静默）

* **t246 的运行时部分**（合成记忆 7>5、反向控制 len=2、真语料 15/15、阈值三档空转）：探针 `crates/daemon/tests/t246_probe.rs` 已删；
  重建要往 crates/ 写文件，而 t254 的 inScope 只有 docs/design/reviews/ ⇒ **无法判定**。
* **t248 的 doctor 副作用清单**：跑 doctor 会改 `~/.ruagent`（真实数据）⇒ 未跑，**无法判定**。
* **t249 的 DOM 层读数**：我是 t249 的作者，所谓「独立复核」对我自己的报告是打折的；
  本次只复现了可只读复现的部分（DB 口径、HTTP score、路由映射），页面渲染未重跑（Playwright 成本 + 作者身份）。
* **全 workspace fmt/clippy/test**：未跑——工作树当前**编译不过**（见 0.1），且 cli/daemon/memory/store 有大量在途改动，
  跑了也会把别人的半成品算进我的结论。
* **t247 的四条 FTS5 错误串**：未在本次重建 FTS5 表复现。

## ⑤ 改前读数 → 改后读数

| 项 | 报告时（改前） | 本次独立读数（改后/现值） |
| --- | --- | --- |
| t245 harness 在工作树里 | 可编译、recall@5 0.8667 | **编译不过**（E0433 fts / E0277 str，链接 19:20:29 的 store rmeta）· 钉住修订上仍 = 0.6667/0.8667/0.7500 且两次逐字节一致 |
| recall_log 行数 / 取值集合 | 574 行；3 个取值 295/276/3 | 582 行；**同 3 个取值** 301/278/3（max 仍 = 2/61 的 99.19%） |
| memories | 154 行 / current 153 / 覆盖 154 | 164 / 163 / 164（embedder、dims 不变） |
| memory readout（t249） | 43 画像 / 43 观察 / 27 流程 / 40 经验 | 40 / 51 / 28 / 44（守护进程在写） |
| confidence < 0.5 | 0 行 | 0 行（结论不变） |
| t246「关键词条目没有 score」 | 无 score 字段 | **已带 score + score_kind=rrf_rank + legs + semantic_score + keyword_score**（api.rs:2398-2420，未提交）⇒ 该结论已过时 |
| knowledge 目录 | 5 文件 / 3,516 B | 4 文件 / 3,405 B（doctor-probe.md 已清） |
| transcripts / .claude/projects | 146 / 27.83 MB、136 / 103.89 MB | **逐字节相同** |
| docs/ | 665 文件 / 70.45 MB | 668 文件 / 70.48 MB |

## 结论

* acceptance #1：**passed** —— 钉住修订上两次运行逐字节一致（进程内 assert + 进程级 md5/cmp）。
* acceptance #2：**passed** —— t245–t249 的结论逐条判定已给；「无法判定」的每一条都写明了原因。
* acceptance #3：**passed** —— 4 条「是论据不是读数」已列出。
* 同时登记两条环境读数：**工作树当前编译不过（跨树产物串扰，我 t256 的方法留了一份）**；**t246/t247 的两条结论已被在途改动修掉/过时**。
