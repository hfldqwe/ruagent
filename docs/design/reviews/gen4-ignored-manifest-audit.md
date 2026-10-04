# t163 审计「忽略清单门禁」的诚实性：16 条 `#[ignore]` 逐条读 + 与 ci.yml 逐条对账（mem-core，只读）

- 任务：t163（kind=work，**report-only**：`crates/**`、`panel/**`、`.github/**`、`scripts/**` 零改动）；attempt `2656f55a-9ba9-4398-bdc8-29a732b9d81e`
- 工具链：钉住的 **1.95.0**（`rustc 1.95.0 (59807616e 2026-04-14)`），`--ignored --list` 走 `scripts/cargo-team.ps1`
- **一句话结论**：**条数上没有缺口**（`found = 16` = `covered 8` + `declared 8`，逐条都能映射）；但**门禁保证的是「有人声明过」，不是「声明得对」** —— 我在声明里抓到 **2 处与实现不符的名字/变量**，并在门禁的盲区里抓到 **1 条自跳过的测试**（它**不是** `#[ignore]`，所以这道门**永远看不到它**）。**16 条里没有一条是「只是没人修」**：每条都有一个真实前提，且缺前提时**是失败或响亮的跳过，不是静默通过**。

## 1 我实际做了什么（先说边界，再说读数）

| 做了什么 | 读数/来源 |
| --- | --- |
| 在**当前字节**、钉版工具链上重取清单 | `scripts/cargo-team.ps1 test --workspace -- --ignored --list` ⇒ **exit=0**，`grep -cE ': test$'` = **16**，16 个名字与 t144 的清账**逐个相同** |
| 逐条**读字节**（`#[ignore]` 的属性行 + reason 原文 + 体首前提 + 前提的读取点） | 6 个文件：`injection_e2e.rs` · `graph/tests/live-after.rs` · `knowledge/tests/retrieval-gold-copy.rs` · `retrieval-gold-live.rs` · `store/src/lib.rs` · `store/src/migrations.rs` |
| 对账 `ci.yml` 的 manifest 段（`covered` / `declared_not_covered` / 具名表） | 逐字引用见 §3 |
| **真跑过哪几条** | 只跑了 `--ignored --list`（列 16）；**带前提真跑**：① t137 我在最终字节上跑过 `chat_event_carries_the_injection_budget_report -- --ignored` ⇒ **1 passed**；② CI run `37194596072` 的第 7 步 `--include-ignored` 跑过 daemon 那 8 条（captain 转来的 runner 证据）。**其余 7 条（store 3 / graph 3 / knowledge 2）我一条都没有真跑**（要活库副本 + 模型）⇒ 它们的状态是**读字节 + 引用历史报告**得出的，不是本次读数 |
| **没有做的** | 没有在共享树上造变异 ⇒ 凡涉及「它能红」的地方记为**无前置红证**；没碰守护进程（C32 未引用）；没重盘 t157 的等待形状、没碰 t161 的 `panel/e2e`/`scripts` |

## 2 16 条逐条（名字 · 位置 · reason 原文 · 前提 · 何时真跑 · 对照组）

### 2.1 `crates/daemon/tests/injection_e2e.rs`：8 条（**唯一在 CI 里真跑的一族**）

reason 原文（**8 条逐字相同**）：`"needs ruagent-mock-agent next to the test binary; run with -- --ignored"`

| 测试 | `#[ignore]` 行 | 体首前提 |
| --- | --- | --- |
| `run_injects_memory_knowledge_and_wiki` | 330 | `boot("withkb", true).await.expect(NEEDS_MOCK)` |
| `run_without_knowledge_documents_has_no_knowledge_block` | 378 | `boot("nokb", false)…` |
| `chat_injects_memory_knowledge_and_wiki` | 492 | `boot("chatwithkb", true)…` |
| `chat_without_knowledge_documents_has_no_knowledge_block` | 530 | `boot("chatnokb", false)…` |
| `chat_and_run_use_the_same_block_wording` | 559 | `boot("wording", true)…` |
| `both_paths_emit_the_contract_truncation_marker` | 642 | `boot("markers", true)…` |
| `a_retried_run_carries_the_shared_retry_prefix` | 682 | `boot("retry", false)…` |
| `chat_event_carries_the_injection_budget_report` | 796 | `boot("chatbudget", true)…`（t137 新增） |

- **前提的准确形状**：`mock_bin()`（:66-78）向上两级找 `<target>/<profile>/ruagent-mock-agent[.exe]` —— 是**跑测试的那个 target 目录里有没有这份二进制**，**不是** cargo 的 dev-dependency；缺了它 `boot()` 返回 `None` ⇒ `.expect(NEEDS_MOCK)` **panic**（`NEEDS_MOCK` 原文在 :97）。⇒ **缺前提 = 失败，不是静默跳过**。
- **何时真跑**：CI 的 `Ignored instruments CI can satisfy` 那一步用 `--include-ignored` 跑**整份 `injection_e2e` 文件**，并在 `test-evidence.sh` 里**逐名要求**（t144 把第 8 个名字也加进去了）⇒ **这 8 条在 CI 上真跑**。
- **对照组**：**有**，而且是真跑读数 —— CI run `37194596072` 该步绿（captain 转来）；我在 t137 也单独跑过其中 1 条（`1 passed`）。

### 2.2 `crates/graph/tests/live-after.rs`：3 条

reason 原文（**3 条逐字相同**）：`"needs RUAGENT_GRAPH_LIVE_COPY pointing at a COPY of the live db"`

| 测试 | `#[ignore]` 行 | 前提 |
| --- | --- | --- |
| `the_real_query_frame_through_the_new_legs` | 206 | `own_copy("frame")`（:120-…）⇒ `source_path()` 读 `RUAGENT_GRAPH_LIVE_COPY`（:39 `expect`） |
| `redundant_pairs_on_the_real_graph_go_to_zero` | 275 | `own_copy("redundancy")`（**写**：merge） |
| `community_coverage_and_multihop_on_the_real_graph` | 335 | `own_copy("communities")`（**写**：partition） |

- **前提对象集**：一份**活库的副本**（`VACUUM INTO` 到私有临时目录）；文件头 :3-6 说明为什么不指 `~/.ruagent`（守护进程持有 + 要带上活文件没有的迁移）。缺 env ⇒ `expect` **失败**。
- **何时真跑**：只在带 env + `-- --ignored` 的机器上；**CI 上不跑**（已声明）。
- **对照组**：**有（文档引用，非本次重跑）** —— `gen2-graph-impl.md:232` 记着「`live-after` **3 ignored**（带 `RUAGENT_GRAPH_LIVE_COPY` 跑过后 **3 passed**）」，:22 还有命令形状与读数表。

### 2.3 `crates/knowledge/tests/retrieval-gold-{copy,live}.rs`：2 条

| 测试 | `#[ignore]` 行 | reason 原文（逐字） |
| --- | --- | --- |
| `live_copy_gold_set` | `retrieval-gold-copy.rs:37` | `"measures the C1/C2/C9 live reading: needs RUAGENT_IA_LIVE_COPY (a COPY of a live root -- it WRITES) and runs with `-- --ignored`"` |
| `live_copy_evidence_and_run_record` | `retrieval-gold-live.rs:29` | `"measures the per-hit evidence reading: needs RUAGENT_IA_LIVE_COPY (a COPY of a live root -- it WRITES) and runs with `-- --ignored`"` |

- **前提**：`std::env::var("RUAGENT_IA_LIVE_COPY").expect(...)`；`gold_set` 还断言 `root/data/ruagent.db` 存在（**必须指一个被复制的 root，不是 .db 文件**），并 `FastEmbedder::try_new()` —— **模型缺失时走 `Err(e)` 分支记一条 note**，所以「e5 模型」是**读数的可比性前提，不是硬前提**。
- **何时真跑**：带 env + `-- --ignored`；CI 上不跑。
- **对照组**：**有（文档引用，命令 + 读数段落，非本次重跑）** —— `gen2-recall-impl.md:36` / `gen2-recall-gold-repair.md:32` 都记了 `$env:RUAGENT_IA_LIVE_COPY="$env:TEMP\ia-live"`（+ `HF_HOME`）的运行形状；`gen2-recall-gold-drift.md:117-140` 记着这次改造（`NOT MEASURED` + `return` → `#[ignore]` + 体首 `expect`）与它的判据「no state in which it passes without measuring」。**强度=中等**：我引用的是命令与说明，**没有**逐条核对那些报告里的数字。

### 2.4 `store`：3 条

| 测试 | `#[ignore]` 行 | 前提 env（体首 `expect`） | reason 原文要点 |
| --- | --- | --- | --- |
| `tests::live_copy_bigram_backfill_at_scale` | `store/src/lib.rs:1290` | **`RUAGENT_T6_LIVE_COPY`**（:1295） | 「measures the at-scale grams backfill and bigram/LIKE equivalence: needs `RUAGENT_T6_LIVE_COPY=<a COPY of the live db>`（**会应用迁移并写 grams**）」 |
| `migrations::tests::live_copy_keeps_history_nullable` | `store/src/migrations.rs:1168` | **`RUAGENT_T6_LIVE_COPY`**（:1173） | 「measures the history-NULL reading: needs `RUAGENT_T6_LIVE_COPY=<a migrated COPY of the live db>`（**会应用迁移，即写**）」 |
| `migrations::tests::live_copy_upgrades_without_losing_distill_rows_and_then_holds_each_attempt` | `store/src/migrations.rs:1297` | **`RUAGENT_T25_LIVE_COPY`**（:1302） | 「measures the pre-0024 -> 0025 upgrade on a COPY: needs `RUAGENT_T25_LIVE_COPY=<a copy of a PRE-0024 live db>`」 |

- **对照组**：**有（文档引用，含数字，非本次重跑）** —— `gen2-store-instrument-falsifiability.md:141` 的「③ 带 env」行记着 `test result: ok. **3 passed**` 与逐项读数（`rows=33 …`）；同报告 :22 还记着一条**族内约束**：「T6 与 T25 **必须指向不同的副本**」（t25 会写两条 annotated 行；共用一个副本会让 history 断言因为**别人的写入**而红）。

## 3 与 `ci.yml` 的对账（逐字引用当前字节）

```
            # RUN by the step above (8). Their names are demanded there.
            covered=8
            # DECLARED NOT COVERED here (8). CI has no ~/.ruagent database and no e5
            # model; ... that is why they are named instead of faked.
            declared_not_covered=8
            expected=$((covered + declared_not_covered))
```
具名表（四行，逐字）：`injection_e2e`: 8 names → RUN；`store`: `live_copy_keeps_history_nullable, live_copy_upgrades_…, live_copy_bigram_backfill_at_scale` → NOT RUN（`RUAGENT_T6_LIVE_COPY`，并注明 t25 需要 PRE-0024 副本）；`graph`: 三个全名 → NOT RUN（`RUAGENT_GRAPH_LIVE_COPY`）；`knowledge`: 两个全名 → NOT RUN（`RUAGENT_IA_LIVE_COPY`）。

**一一映射**：`covered 8` ↔ §2.1 的 8 条 ✓（8 个名字在 CI 第 7 步的 `test-evidence.sh` 名字表里**逐字**出现）；`declared 8` ↔ §2.2/2.3/2.4 的 3+2+3 = 8 条 ✓。**没有一条「实际存在但没被声明」，也没有一条「声明了但不存在」** ⇒ `found == expected` 在语义上也成立（不只是计数）。

**但逐条读之后有两处对不上（这正是门禁自己要防的那类），点名**：
- **对不上 A（变量名）**：`store` 那一行把三条都记成 `RUAGENT_T6_LIVE_COPY`，而其中 `live_copy_upgrades_…`（`migrations.rs:1297`/`:1302`）**实际读的是 `RUAGENT_T25_LIVE_COPY`**。行内括号提了「for t25 a PRE-0024 copy」，**但没有点出变量名** ⇒ 按声明字面设 `T6` 去跑，第三条会在 `expect` 处失败，读者可能误读成「仪器坏了」。
- **对不上 B（名字被省略）**：`store` 行里第二条写成 `live_copy_upgrades_…`，**不是**实际名字（`migrations::tests::live_copy_upgrades_without_losing_distill_rows_and_then_holds_each_attempt`）。门禁只数条数，所以这种省略**永远不会让它变红** ⇒ 「名字被声明」在这一行只是**看起来**成立。

## 4 每一类「跳过」的合规判定

- **① 有真实前提 ⇒ 合规**：**16/16**。而且是**同一个设计**（`gen2-graph-instrument-repair.md:122` 的原话：「缺输入是『你让我量，但我量不了』，不是『没有数据 ⇒ 通过』」）—— 体首 `expect` 让「缺前提」**失败**，而不是让聚合里多一个 `passed`。这正是「跳过必须可见」的实现方式，而且是**比 `#[ignore]` 更强的一层**：`#[ignore]` 让它默认不跑，`expect` 让它**没法假装跑过**。
- **② 只是没人修 ⇒ 不合规**：**一条都没有**（我逐条读过体首：每条都有 `expect`/`assert` 的非空与非平凡判据，没有「跑不了就先忽略」那种形状；每条 reason 原文也都指向**一个具体的 env 或一份具体的二进制**）。
- **③ 前提已过期 ⇒ 无**：四个 env 名（`RUAGENT_GRAPH_LIVE_COPY` / `RUAGENT_T6_LIVE_COPY` / `RUAGENT_T25_LIVE_COPY` / `RUAGENT_IA_LIVE_COPY`）**都仍然在被对应测试读取**（§1 的「env 读取点」是逐条 grep 出来的），没有一个是废弃名。

## 5 发现（report-only；每条给建议判据）

- **F1（中）声明里的变量名与实现不符**（§3 对不上 A）。**建议**：把该行写成两条并列的变量名（`RUAGENT_T6_LIVE_COPY` 两条 / `RUAGENT_T25_LIVE_COPY` 一条）。**建议判据（机械、可做成测试）**：解析 manifest 里每个 `RUAGENT_*` 名字，要求它**出现在对应文件里某次 `std::env::var("…")` 调用**中；反向也成立。
- **F2（低）声明里的名字被省略**（§3 对不上 B）。**建议**：写全名，或明确标注「此处为省略」。**建议判据**：每个被声明的名字必须是**实际清单里某个名字的子串且唯一**（`live_copy_upgrades_…` 现在失败于此）。
- **F3（中，门禁的盲区）**：**有一条测试用「打印 SKIPPED + `return`」跳过，而它不是 `#[ignore]`** ⇒ `--ignored --list` **列不到它**，manifest 的 `found == expected` **照样成立**，默认跑里它算 **passed**：`crates/memory/src/lifecycle.rs:1942 fn the_real_root_inventory_is_read_when_it_is_named()`（**`SKIPPED` 那行在 :1945**，随后 `return;`）。我把源码里所有 `SKIPPED`/`NOT MEASURED` 命中都读了一遍：**其余命中都是注释**（描述已退役的旧形状，见 `store/*`、`knowledge/*` 的 t33/t30 注释）或**非测试语义**（`memembed.rs:243`、`store/src/fts.rs:229`、`lifecycle.rs:688`）⇒ **live instance 就这一条**。**建议**：把这类自成跳过的测试也变成 `#[ignore]`（于是它进入 manifest 的 16）或改成断言。**建议判据**：源码扫描——某个 `#[test]`/`#[tokio::test]` 函数体内出现「`SKIPPED`/`NOT MEASURED` 的 `println!` + 紧跟 `return;`」而**没有** `#[ignore]` ⇒ 红。
- **F4（信息）knowledge 行的「and the e5 model」比代码的硬前提强**：`live_copy_gold_set` 在 `FastEmbedder::try_new()` 失败时走 `Err(e)` 分支并记 note（代码容错），模型缺失时它**不会失败**，但读数不再可比。**建议**：若要与实现逐字对齐，加一句「模型缺失时会记录 fallback，读数不再可比」；否则保持现状也**不算错**（它描述的是「有意义的读数」的前提）。

## 6 未测 / 未覆盖（点名）

1. **我没有带上前提把 16 条都跑一遍**（要活库副本 + 副本上的迁移/写入 + 模型；成本高且会写数据）⇒ §2.2-2.4 的 7 条是**读字节 + 引用历史报告**，其中 knowledge 那 2 条的对照组我只引到**命令与说明**，**没有**核对那些报告里的数字。**真跑过的只有**：`--ignored --list`（列表，16 条）+ `chat_event_carries_the_injection_budget_report`（t137，1 passed）+ CI 第 7 步的 daemon 8 条（captain 转来的 runner 证据）。
2. **没有前置红证**：本单**不许在共享树造变异**，所以「把 env 撤掉它会失败」这一条我**没有**在本机亲手证明 —— 它的依据是**读代码**（体首 `expect`）+ 那些报告里记过的「不带 env 会失败/曾把 `NOT MEASURED` 记成 passed」的历史。要补：在**隔离副本**里把某个 `expect` 换成 `println!+return` 跑一次，看它是否从 FAILED 变成 passed。
3. **不在本单范围内的等待**：`crates/**` 里其它等待形状由 **t157** 盘；`panel/e2e` 与 `scripts` 的等待由 **t161** 另有其单 —— 我没有重复它们。
4. **我读的是当前工作树**（含同伴在途改动）；`--ignored --list` 是**当前字节**的读数，但**清单之外的代码**可能在我读之后移动（t20 形状：本报告里每个文件:行都取自这次读到的字节）。

## 7 残留

- **只写了本报告**；`crates/**`、`panel/**`、`.github/**`、`scripts/**`、`tools/**` **零改动**（由我造成的一行改动都没有；`git status` 里那四个目录下若有 ` M`，是**同伴在途的编辑**——本单收尾时看到的是 `panel/src/index.css`，**不是我**改的）。
- 未 push / dispatch / rerun / cancel / 建 tag；未启停任何进程（C32 未引用）。
- 复核材料：`%TEMP%\ruagent-t163\ignored-list.txt`（本次 `--ignored --list` 的原始输出，CRLF；比较时需 `tr -d '\r'`）。
