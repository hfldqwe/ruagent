# 同族收口：`crates/store` 的「不测也绿」仪器（t33）

> 状态：**已完成并通过验证**。产物：`crates/store/src/lib.rs`（1 条）、`crates/store/src/migrations.rs`（2 条）+ 本文件。
> 作者：recall。时间窗：**2026-09-28T02:3x → 03:0x+08:00**。
> inScope 遵守：只改 `crates/store/` 与本报告；`crates/knowledge/`、`crates/memory/`、`crates/graph/`、`crates/daemon/`、`panel/` 一行未改。

**一句话**：本代第五族失真（**空断言 / 真空通过**）在 store 侧收尾。`crates/store` 里有 **3 条**测试在缺 env 时 `println!("NOT MEASURED")` 后 `return`，于是被 harness 计成 **passed** —— 「N passed」把**测量**与**未尝试**混在一起；更糟的是，在 `#[ignore]` 之前，连「请你去测量」的那条命令（`-- --ignored`）也**空跑而绿**（没有任何 ignored 测试时它报 `0 passed … N filtered out` 并 exit 0）。现在三条都是：默认 **ignored**（harness 自己分账）、**显式运行缺 env ⇒ FAILED**、**带 env 时真读数且非真空**。判据原则一句话：**不存在一个不测量也能通过的状态。**

---

## 1 逐条验收

| # | 验收项 | 读数 | 判 |
| --- | --- | --- | --- |
| 1 | 找出全部同族测试并逐条改形成 `#[ignore = "…needs <ENV>（会写副本）…"]` + 体首 `expect` | **3 条**（不是 2 条）：`live_copy_keeps_history_nullable`、`live_copy_bigram_backfill_at_scale`（两条 `RUAGENT_T6_LIVE_COPY`）、`live_copy_upgrades_without_losing_distill_rows_and_then_holds_each_attempt`（`RUAGENT_T25_LIVE_COPY`）；全仓 `crates/store/src/**` 已无 `LIVE_COPY") else` / 早退 | **达标** |
| 2 | 读数三态 | ① 默认：**31 passed / 0 failed / 3 ignored**（三条各带 ignore 理由行）；② 显式 `-- --ignored` 缺 env：**0 passed / 3 failed**（不是跳过）；③ 带 env：**3 passed** 且读数与 t6/t25 原值逐位一致 | **达标** |
| 3 | 加真实读数断言（不只靠 env 在不在） | 三条各加**非真空**断言：`wiki_builds>0 \|\| chunks>0`、`chunks>0`、`distill_log before>0`（并逐条说明「空副本下哪几条断言会同时真空通过」） | **达标** |
| 4 | `test -p ruagent-store` 通过；clippy `-CleanFirst` 零诊断 + 两件证据 | `31 passed / 3 ignored`；clippy `Checking ruagent-store` + `Finished` 零诊断；覆盖 target = **只有 `lib`**（store 没有 `tests/` 目录，这三条是 `#[cfg(test)]` 单元测试，所以它们的 target 就是库本身） | **达标** |
| 5 | 只改 inScope、不写活库、不启停 79984 | 改动清单见 §6；活库纪律见 §6 | **达标** |

**确切命令**
```powershell
$env:PATH="$env:USERPROFILE\.protoc\bin;$env:PATH"
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 test -p ruagent-store
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 clippy -p ruagent-store --all-targets -DenyWarnings -CleanFirst ruagent-store
# ② 显式运行但缺 env（必须红）
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 test -p ruagent-store -- --ignored
# ③ 带 env 的真读数
$env:RUAGENT_T6_LIVE_COPY="$env:TEMP\ia-live\data\ruagent.db"
$env:RUAGENT_T25_LIVE_COPY="$env:TEMP\ra-t33-t25.db"          # 活库的 VACUUM INTO 副本（v18）
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 test -p ruagent-store -- --ignored
```

---

## 2 三态读数（改前 → 改后）

### ① 默认跑：`passed` → `ignored`
```
改前：test result: ok. 34 passed; 0 failed; 0 ignored      ← 其中 3 条只打印 NOT MEASURED 后 return
改后：test migrations::tests::live_copy_keeps_history_nullable ... ignored, measures the history-NULL
      reading: needs RUAGENT_T6_LIVE_COPY=<a migrated COPY of the live db> (the test APPLIES
      migrations, i.e. it WRITES) and runs with `-- --ignored`
     test tests::live_copy_bigram_backfill_at_scale ... ignored, measures the at-scale grams backfill
      and bigram/LIKE equivalence: needs RUAGENT_T6_LIVE_COPY=…
     test migrations::tests::live_copy_upgrades_without_losing_distill_rows_and_then_holds_each_attempt
      ... ignored, measures the pre-0024 -> 0025 upgrade on a COPY: needs RUAGENT_T25_LIVE_COPY=…
     test result: ok. 31 passed; 0 failed; 3 ignored; 0 measured; 0 filtered out
```
`N passed / M ignored` 由 harness 自己分账，且**每条 ignore 的理由印在结果行上**（含「它会写」这句）—— 读者不会把 3 条未尝试当成 3 次测量。

### ② 显式 `-- --ignored` 但缺 env：**FAILED**，不是跳过
```
test migrations::tests::live_copy_keeps_history_nullable ... FAILED
test tests::live_copy_bigram_backfill_at_scale ... FAILED
test migrations::tests::live_copy_upgrades_without_losing_distill_rows_and_then_holds_each_attempt ... FAILED
thread '…live_copy_keeps_history_nullable' panicked at crates\store\src\migrations.rs:775:58:
this instrument has no state in which it passes without measuring: set RUAGENT_T6_LIVE_COPY=<a migrated
COPY of the live db> (the test APPLIES migrations, i.e. it WRITES -- never point it at ~/.ruagent) and
run with `-- --ignored`: NotPresent
（另两条同形，各自点名自己的 ENV）
test result: FAILED. 0 passed; 3 failed; 0 ignored; 0 measured; 31 filtered out
```
这一条正是「**不存在一个不测量也能通过的状态**」的可执行形式：想测量就得给副本，不给副本这一跑就是红的。

### ③ 带 env：真读数，与既有读数逐位一致
```
[t25] live copy BEFORE: rows=33 already-0024=false columns=[6 列] outcome-unknown=33/33
[t25] live copy AFTER migration: rows=33 outcome-unknown(NULL)=33 ids unique=true
[t25] live copy AFTER two SQL attempts on one session: rows=2 ok=1 failed=1
      failure_reason=Some("prompt longer than the context window") prompt_hash=Some("ph-bad")
[t25] live copy: recorded-outcome view = 2 of 35 rows (the rate's denominator)

[t33] live copy object set: wiki_builds=8 chunks=10765                    ← 本次新增的非真空读数
[t6]  live copy: wiki_builds rows=8 unfinished_plans=0
[t6]  live copy: memories.access_count NULL 163/163
[t6]  live copy: memories.last_used_at NULL 163/163
[t6]  live copy: memories.valid_from NULL 163/163
[t6]  live copy: distill_log.status NULL 33/33
[t6]  live copy: entity_edges.event_time_source NULL 67/67
[t6]  live copy: chunks.grams NULL 0/10765 (all-NULL = freshly migrated, none-NULL = backfilled)
[t6]  live copy: chunks=10765 grams_missing_before=0 filled=0 elapsed=189ms (189.53 ms/row)
[t6]  live copy: recall_log.knowledge_leg_window NULL 651/651
[t6]  live copy: bigram-index vs LIKE equivalence on 86 derived 2-char Han substrings -> 0 disagreement(s) []
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 31 filtered out
```
与既有读数逐位一致：`33/33`、两条尝试 `rows=2`（`ok=1 failed=1`）、分母 `2 of 35`（t25）；`wiki_builds=8 · memories 163 · distill_log 33 · entity_edges 67 · chunks 10765 · recall_log 651 · 86 子串 0 分歧`（t6 / V-SCHEMA 同批读数）。**改的是仪器的形状，不是读数。**

### 一条 harness 事实（为什么「早退」比 `#[ignore]` 更坏）
没有 ignored 测试时，`-- --ignored` 会**空跑并绿**：本次输出里那个 0 测试的 target 就是证据行 `test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; … filtered out`。也就是说，改前那 3 条即使有人**专门**去跑「测量命令」，看到的也是一次绿的、什么也没测的跑。`#[ignore]` + `expect` 之后，同一个命令在缺 env 时必然红。

---

## 3 非真空断言：每一条防的是哪一种真空

| 测试 | 新增断言 | 空/退化副本下哪些断言会**同时真空通过**（所以需要这条） |
| --- | --- | --- |
| `live_copy_keeps_history_nullable` | `wiki_builds > 0 \|\| chunks > 0` | 空副本上 `unfinished_plans == 0`（0 个未完成计划）、`grams NULL == 0 \|\| == total`（0/0）、每条历史列 `NULL n/n`（0/0）**全部为真** ⇒ 「历史保持 NULL」这句话被 0 行证明 |
| `live_copy_bigram_backfill_at_scale` | `chunks > 0` | 空副本上 `filled == missing_before`（0 == 0）、第二次 `again == 0`、`sample` 的 `!is_empty()` 会先失败（这条已有）—— 但 `filled==missing_before` 与 `again==0` 这两条本来可以真空通过 |
| `live_copy_upgrades_…` | `distill_log before > 0` | 空副本上「升级不丢行」退化 `0 → 0`、`lost == 0`、`outcome-unknown == 0` 全真（两条尝试那半仍然有效，因为探针自己造 2 行 —— 但「历史行不丢」这半会真空） |

失败信息都写成「**这一跑什么也没测，不能算通过**」，而不是「断言为假」—— 因为要表达的是读数无效，不是产品错。

---

## 4 覆盖与重查证据（契约要求两件）

1. **覆盖哪些 target**：`--all-targets` 对 `ruagent-store` = `lib`（`unittests src\lib.rs`，34 tests）+ doc-tests（0 tests）。`crates/store` **没有 `tests/` 目录** —— 这三条 live-copy 仪器本身是 `#[cfg(test)]` 单元测试，落在库里，所以它们的 target 就是 `lib`，`--all-targets` 与 `-p ruagent-store` 在本包上覆盖同一棵树（本次输出只有两行 `test result`：`31 passed / 3 ignored` 与 doc-tests 的 `0`）。
2. **本次确实重查了哪个包**：`-CleanFirst ruagent-store` 下输出出现 `Checking ruagent-store v0.1.0 (…\crates\store)` + `Finished` ⇒ 该包本次真被重查，不是命中上一轮缓存指纹（本代第六族失真「缓存假绿」的判据）。

---

## 5 与前几单的关系（形状来源与差别）

| 单 | 位置 | 形状 |
| --- | --- | --- |
| t30 / RV-A-5 | `crates/knowledge/tests/retrieval-gold-{copy,live}.rs` | `#[ignore]` + `expect` + `rows==15 && no_answer==7` / `queries_measured==15` |
| **t33（本单）** | `crates/store/src/{lib,migrations}.rs` | 同一形状；多一条 **harness 事实**：不做 `#[ignore]` 时连 `-- --ignored` 都空跑绿；非真空断言按每条测试**各自的真空路径**选（§3） |

三态的**判据形态完全一致**：默认 `ignored`、显式缺 env `FAILED`、带 env 真读数且非真空。

---

## 6 边界与未测

- **改动清单**（全部在 inScope）：`crates/store/src/lib.rs`（`live_copy_bigram_backfill_at_scale`）、`crates/store/src/migrations.rs`（`live_copy_keeps_history_nullable`、`live_copy_upgrades_without_losing_distill_rows_and_then_holds_each_attempt`）、本报告。`git status` 里 `crates/store` 的其余条目（`fts.rs`、0019–0025）属我 t6/t25/t29 的在途工作；`crates/memory`/`crates/graph`/`crates/daemon`/`panel` 的改动**没有一行是我的**。
- **活库纪律**：活库只被**只读**接触一次（`file:…?mode=ro` + `VACUUM INTO` 造 `%TEMP%\ra-t33-t25.db`，v18/33 行）；t6 侧用既有的 `%TEMP%\ia-live` 副本（v25）。pid 79984 未启停；未使用 `/api/v1/recall`。
- **未测（不静默跳过）**：
  1. 三条测试**未在「空副本」上实跑**验红（构造一个空库会让 `apply` 之后的断言链先失败，且那本身不是产品缺陷）—— §3 的「真空路径」是**静态推演**，不是读数；如实标注为推演。
  2. 本单**不动** store 侧真正的读数口径（例如 `chunks.grams NULL 0/10765` 的语义），只改仪器形状。
  3. `crates/store` 之外是否还有同族（例如 `crates/daemon`/`crates/mock-agent` 的 live 仪器）**未查**：不在本单 inScope，留给相应 owner；建议作为下一代一次 grep（`NOT MEASURED`）扫全仓收口。
