# 同族收口的可证伪读数 + 全仓 NOT MEASURED 盘点（t36）

> 状态：**已完成并通过验证**。产物：`crates/store/src/migrations.rs`（真空输入夹具 + 预-t33 谓词求值 + 一处新发现陷阱的守卫）+ 本文件。
> 作者：recall。时间窗：**2026-09-28T03:0x → 03:3x+08:00**。
> inScope 遵守：只改 `crates/store/` 与本报告；`crates/knowledge/`、`crates/memory/`、`crates/graph/`、`crates/daemon/`、`crates/mock-agent/`、`panel/` 一行未改。

**一句话**：t33 的三条非真空断言，其「红」当时只是**静态推演**；本单把它变成读数 —— 造一个真空输入（`%TEMP%\ra-t36-empty`，schema 到 `SCHEMA_VERSION`、**每张表 0 行**），把三条仪器分别对同一输入实跑，并且**用一个导出的 t33 之前形态的树**（`git stash create` + `git archive`，共享树零改动）跑同一输入做对照。结果是三分法，其中**一条与我 t33 的判断不符，已如实改正**：`live_copy_keeps_history_nullable` 与 t25 那条**确实是**「不测也绿」（改前跑绿、改后红）；而 `live_copy_bigram_backfill_at_scale` **本来就不在**这一类 —— 它自带的 `assert!(!sample.is_empty())` 在真空输入上早就红了，我加的那条只是**更早红、并说明原因**。另外顺带跑出一个真陷阱：两条 t6/t25 仪器**必须指向不同副本**，否则它们互相读对方的写入 —— 已加守卫让这个陷阱自己喊出来。

---

## 1 逐条验收

| # | 验收项 | 读数 | 判 |
| --- | --- | --- | --- |
| 1 | 3 条仪器逐条在真空路径上实跑（红读数 + 失败信息） | 三条全红且信息都是「这一跑什么也没测，不能算通过」（§2.2 原文） | **达标** |
| 1b | 改前形态在**同一输入**上绿的对照读数 | 导出树（预-t33）实跑：history 仪器 **`ok. 1 passed`**（各列 `0/0`）、t25 **passed**、at-scale **FAILED**（自有断言）—— 三分法见 §2.2 | **达标（含一处自我纠正）** |
| 2 | 全仓盘点同族（`NOT MEASURED` + 「缺条件就 return」形态），store 内修掉、store 外登记 owner | `NOT MEASURED` **6 处全部是注释**（无活体早退）；精化扫描出 **8 条**真同族（store 内 0 条 ✓，acp 1 条、daemon 7 条）—— §3 有表与 owner | **达标** |
| 3 | 三态读数复现 | ① 默认 **32 passed / 0 failed / 3 ignored**；② 显式 `-- --ignored` 缺 env **0 passed / 3 failed**；③ 带 env **3 passed** 且与 t6/t25 原值逐位一致 | **达标** |
| 4 | `test -p ruagent-store` 通过；clippy `-CleanFirst` 零诊断 + 两件证据 | 见 §5（含首轮 `type_complexity` 修后重跑） | **达标** |
| 5 | 只改 inScope、不写活库、不启停 79984 | 见 §6 | **达标** |

---

## 2 断言的红，是读数了

### 2.1 真空输入（同一输入，三条仪器共用）

`crates/store/src/migrations.rs::a_vacuum_input_satisfies_every_pre_t33_predicate`（新）在 `%TEMP%\ra-t36-empty` 造出并**打印**这个输入，同时把三条仪器的**预-t33 谓词集合**在其上求值：

```
[t36] VACUUM INPUT at "…\ra-t36-empty\data\ruagent.db": version=25 wiki_builds=0
      unfinished_plans=0 chunks=0 grams_NULL=0 recall_log=0 leg_window_NULL=0 distill_log=0
      memories_access_count_NULL=0 edges_event_time_source_NULL=0 distill_status_NULL=0 distinct_ids=0
[t36] A live_copy_keeps_history_nullable: every PRE-t33 predicate HOLDS … -> the pre-t33 instrument
      reported a PASS here. The t33 assert `wiki_builds>0 || chunks>0` is therefore load-bearing.
[t36] B live_copy_upgrades_…: the HISTORICAL half of its pre-t33 predicate set also HOLDS (0 -> 0 rows
      preserved, lost=0, outcome-unknown 0/0, ids unique 0==0). … so the t33 assert `before>0` is what
      makes the historical half load-bearing.
[t36] C live_copy_bigram_backfill_at_scale: `filled==missing_before` HOLDS (0==0) and the second pass
      would return 0, BUT its pre-existing `assert!(!sample.is_empty(), …)` ALREADY FAILS on this input
      -> this instrument was NOT in the false-green class; the t33 assert `chunks>0` only fails EARLIER
      with a message that says why.
test result: ok. 1 passed
```

### 2.2 红/绿两侧都是读数（导出树对照）

**方法（两条纪律都守住了）**：`git stash create` 取一份**工作树快照的 commit**（不动共享树/索引），`git archive` 导出到 `%TEMP%\ra-t36-pre`，把 t33 的 4 个文本块**精确删除**（脚本对每块断言「必须恰好命中 1 次」，命中数不是 1 就拒绝执行 —— 不做静默半剥离）。判据：

```
[t36] git stash create -> f0144e8c89cb5ae0527def5b7eae0ea036fd5b0f
[t36] shared worktree untouched by the export: True | changed paths: 79      ← 导出前后 git status 完全相同
[t36] stripped 1 t33 block from crates/store/src/migrations.rs (728 chars)
[t36] stripped 1 t33 block from crates/store/src/migrations.rs (208 chars)
[t36] stripped 1 t33 block from crates/store/src/migrations.rs (521 chars)
[t36] stripped 1 t33 block from crates/store/src/lib.rs (448 chars)
```
（第一跑还暴露了导出树缺**未跟踪**文件：`include_str!` 的 0019–0025 七条迁移不在 `git archive` 里 ⇒ 补拷进副本后编译通过。这是方法本身的坑，记在 §2.4。）

| 仪器 | **预-t33 形态**（导出树，同一真空输入） | **本单形态**（同一真空输入） | 判 |
| --- | --- | --- | --- |
| `live_copy_keeps_history_nullable` | **`test result: ok. 1 passed`** —— `wiki_builds rows=0 unfinished_plans=0`、`memories … NULL 0/0`、`distill_log.status NULL 0/0`、`chunks.grams NULL 0/0`、`recall_log … 0/0`（**0 行证明了「历史保持 NULL」**） | **FAILED** @`migrations.rs:792`：`the copy holds neither wiki_builds nor chunks rows: there is no history to read, so this run measures nothing and must not be reported as a pass` | **原来真的不测也绿；断言承重** |
| `live_copy_upgrades_…`（t25） | **passed**（导出树同一次运行里它先跑并 `1 passed`；它自己的两条尝试探针确实测到 `rows=2`，但**历史行那一半**在 0 行上退化成 `0 → 0`） | **FAILED** @`migrations.rs:947`：`the copy holds no distill_log rows: there is no upgrade to measure (0 -> 0 proves nothing), so this run must not be reported as a pass` | **半条不测也绿；断言让历史那一半承重** |
| `live_copy_bigram_backfill_at_scale` | **FAILED** @`lib.rs:1358` —— 它**自带**的 `assert!(!sample.is_empty(), … "this reading is then vacuous and must not be reported as a pass")` 在真空输入上早就红了 | **FAILED 更早** @`lib.rs:1313`：`the copy has no chunks: there is nothing to backfill and no Han substring to compare, so this run measures nothing and must not be reported as a pass` | **不在这一类；t33 那条只是更早红、信息更准（自我纠正）** |

**三条各自的红（本单形态，同一输入，一次运行）**：
```
test migrations::tests::live_copy_upgrades_… FAILED   (migrations.rs:947)
test migrations::tests::live_copy_keeps_history_nullable FAILED   (migrations.rs:792)
test tests::live_copy_bigram_backfill_at_scale FAILED  (lib.rs:1313)
test result: FAILED. 0 passed; 3 failed; 0 ignored; 0 measured; 32 filtered out
```

### 2.3 顺带跑出的真陷阱（并已加守卫）

预-t33 形态在**同一个副本**上连跑两条 t6/t25 仪器时，我看到 history 仪器**因为 t25 探针刚写的两行**而红：

```
[t6] live copy: distill_log.status NULL 0/2
panicked … assert_eq! failed: distill_log.status: every historical row must be NULL (unknown), not a backfilled value
```
即：**两条仪器的 env 必须指向不同副本**（t25 往 `distill_log` 写 2 行带 status 的探针行）。这个坑在「真实副本 + 两个不同 env」的受理流程里不会触发，但验证者很可能把两个 env 指到同一个副本。已加守卫，让它自己喊出来（而不是伪装成历史问题）：

```
（T6 指向 T25 写过的副本）
[t33] live copy object set: wiki_builds=8 chunks=10765
panicked … assertion `left == right` failed: this copy carries the t25 instrument's probe rows: point
RUAGENT_T6_LIVE_COPY and RUAGENT_T25_LIVE_COPY at SEPARATE copies, otherwise this test reads another
instrument's writes
（两个 env 指向不同副本：`[t6] live copy: distill_log.status NULL 33/33` + `test result: ok. 1 passed`）
```

### 2.4 方法本身的坑（登记，供后来者）

1. `git stash create` **不含未跟踪文件** ⇒ 导出树缺 `include_str!` 的 0019–0025，编译直接失败 `couldn't read crates\store\src\migrations/0019….sql`。修法：把 `crates/store/src/migrations/*.sql` 从工作树补拷进副本（两侧同内容，差异只在被测的断言）。
2. 导出树要用**独立 `-TargetDir`**（本单 `%TEMP%\ra-t36-target`），否则与共享 target 的指纹互相污染。
3. 共享树零改动有读数判据：导出前后 `git status --porcelain` 逐字节相同（79 个改动路径）。

---

## 3 全仓盘点（同族：缺条件就早退 ⇒ 计 passed）

### 3.1 `NOT MEASURED` 字样：6 处，**全部是注释**（无活体早退）
| 文件:行 | 性质 |
| --- | --- |
| `crates/knowledge/tests/retrieval-gold-copy.rs:30` | 文档注释（t30 说明「它曾经这样」） |
| `crates/knowledge/tests/retrieval-gold-live.rs:25` | 同上 |
| `crates/memory/src/lifecycle.rs:325` | 注释里引用了这个说法（不是门） |
| `crates/store/src/lib.rs:1278`、`crates/store/src/migrations.rs:764`、`874` | 文档注释（t33 说明「它曾经这样」） |

判据：逐处按行内容分类（`//`/`///` 注释 vs `println!`/`eprintln!`）—— **0 处是活体打印**。`crates/store` 内残留 = **0** ✓。

### 3.2 精化扫描：`#[test]`/`#[tokio::test]` 函数体内**在任一断言之前**就能 `return` 的（真正的同族定义）

```
[t36] test fns whose body reaches a RETURN before any assert/panic/unwrap: 8
```
| 文件:行 | 测试 | 跳过的条件 | 今天默认 | owner |
| --- | --- | --- | --- | --- |
| `crates/acp/src/adapter.rs:160` | `spawn_spec_appends_args` | `resolve_program("dsh").is_none()`（harness CLI 不在 PATH）→ `eprintln!("skipping: dsh not on PATH"); return;` | **passed**（无 `#[ignore]`） | acp 侧 · 下一代（或一张小单） |
| `crates/daemon/tests/injection_e2e.rs:329` | `run_injects_memory_knowledge_and_wiki` | `boot(...)` 返回 `None`（测试二进制旁**没有 ruagent-mock-agent**）→ `skip_missing_mock(); return;` | **passed** | integ / t19（该文件已在 t19 inScope） |
| `crates/daemon/tests/injection_e2e.rs:379` | `run_without_knowledge_documents_has_no_knowledge_block` | 同上 | **passed** | integ / t19 |
| `crates/daemon/tests/injection_e2e.rs:495` | `chat_injects_memory_knowledge_and_wiki` | 同上 | **passed** | integ / t19 |
| `crates/daemon/tests/injection_e2e.rs:535` | `chat_without_knowledge_documents_has_no_knowledge_block` | 同上 | **passed** | integ / t19 |
| `crates/daemon/tests/injection_e2e.rs:566` | `chat_and_run_use_the_same_block_wording` | 同上 | **passed** | integ / t19 |
| `crates/daemon/tests/injection_e2e.rs:651` | `both_paths_emit_the_contract_truncation_marker` | 同上 | **passed** | integ / t19 |
| `crates/daemon/tests/injection_e2e.rs:693` | `a_retried_run_carries_the_shared_retry_prefix` | 同上 | **passed** | integ / t19 |

**一句精确的话**：daemon 那 7 条不是完全没有防线 —— `skip_missing_mock()`（`injection_e2e.rs:91`）里有 `RUAGENT_REQUIRE_MOCK`：设了就把 skip 变成 `panic!`。但**默认门不设它**，且没有任何契约命令会设 ⇒ 今天的 `N passed` 仍然把这 7 条未运行的算进去。**「谁来跑」不是判据，判据得让漏测自己红** —— 这正是 t30/t33 形状要修的东西，登记给 t19 按同一形状处理（`#[ignore]` + 显式缺件 FAILED + 真读数断言）。

**误报（明确登记，免得后来者「修」它）**：粗启发式（env 读取后 5 行内 return）会命中 `crates/knowledge/tests/common/mod.rs:391` —— 那是 t29 的**活库守卫**（`refuse_live_root`：路径无法证明**不是**活库时直接返回），不是测量门。

---

## 4 三态读数（本单复现）

| 态 | 命令 | 读数 |
| --- | --- | --- |
| ① 默认 | `test -p ruagent-store` | `test result: ok. **32 passed; 0 failed; 3 ignored**; 0 measured`（35 个测试；三条 ignored 各带理由行） |
| ② 显式缺 env | `test -p ruagent-store -- --ignored` | `test result: FAILED. **0 passed; 3 failed**; 0 ignored; 0 measured; 32 filtered out`（三条 panic 各自点名 ENV） |
| ③ 带 env | 同上 + `RUAGENT_T6_LIVE_COPY=%TEMP%\ia-live\data\ruagent.db`（v25）· `RUAGENT_T25_LIVE_COPY=%TEMP%\ra-t36-t25.db`（活库 `VACUUM INTO` 的 v18 副本，33 行） | `test result: ok. **3 passed**`，读数与 t6/t25 原值逐位一致：`rows=33 already-0024=false outcome-unknown=33/33` → `rows=33 outcome-unknown=33 ids unique` → 两条尝试 `rows=2 ok=1 failed=1` + reason + `prompt_hash=Some("ph-bad")` → 分母 `2 of 35`；`wiki_builds=8 unfinished_plans=0`、`chunks.grams NULL 0/10765`、`recall_log.knowledge_leg_window NULL 651/651`、`bigram-index vs LIKE equivalence on 86 derived 2-char Han substrings -> 0 disagreement(s)` |

---

## 5 覆盖与重查证据（契约要求两件）

1. **覆盖哪些 target**：`--all-targets` 对 `ruagent-store` = `lib`（`unittests src\lib.rs`，35 tests）+ doc-tests（0 tests）—— 本包没有 `tests/` 目录，仪器与夹具都是 `#[cfg(test)]` 单元测试（captain 已裁决**保持现状、不搬目录**：`-p` 与 `--all-targets` 在本包覆盖同一棵树，证据无损失）。
2. **本次确实重查了哪个包**：`-CleanFirst` 下输出出现 `Checking ruagent-store v0.1.0 (…\crates\store)` + `Finished` ⇒ 非缓存命中。**首轮 clippy 报了一条 `type_complexity`**（我那个 12 元 i64 元组）—— 已改成具名 `struct VacuumCounts`，**修后重跑**才取的零诊断读数（不拿修之前的绿）。

---

## 6 边界与未测

- **改动清单**（全在 inScope）：`crates/store/src/migrations.rs`（新增真空输入夹具+预-t33 谓词求值+共享副本守卫）、本报告。`git status` 里 `crates/store` 的其余条目（`lib.rs` = t33、`fts.rs` + 0019–0025 = t6/t25/t29）属既有在途工作；out-of-scope 六个目录一行未改（`crates/daemon/tests/injection_e2e.rs` 只**读**了 :327-336/:692-700/:91-98 进行分类）。
- **活库纪律**：只读接触一次（`file:…?mode=ro` + `VACUUM INTO %TEMP%\ra-t36-t25.db`，实测 v18/33 行）；未启停 pid 79984；未用 `/api/v1/recall`。
- **未测（不静默跳过）**：
  1. `crates/acp` 那条 skip 是否**有意**（CI 不装 dsh 时的策略）—— 我只登记形状与 owner，**没有**判断它该不该跳（那需要 acp owner 的意见）。
  2. daemon 那 7 条在**装了 mock-agent** 的环境下是否真会运行 —— 未验证（本单不跑 daemon 测试，避免与 t19 并发冲突）。`RUAGENT_REQUIRE_MOCK` 是否在 CI/任何脚本里设置过 —— **未查**（建议 t19 顺手 grep 一次）。
  3. `crates/store` 里是否还有「不靠 env、而靠别的前提早退」的仪器：我用「断言前 return」这一判据扫了全仓（8 条，均在上面），但那条判据**不含**「断言恒真」这一类（例如对空集合恒成立而**没有** return 的断言）—— t33/t36 修的正是这一类的一个实例，全仓是否还有别的实例**未查**。
