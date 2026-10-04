# t165 收口 t163 的 F1+F2+F3：不可见跳过进清单 + 声明对账 + 三道机械守卫（mem-core）

- 任务：t165（kind=repair）；attempt `bee58a14-ec28-49c6-8e7a-602dec3f280e`
- 改动：`crates/memory/src/lifecycle.rs`（**+547/−8**）· `.github/workflows/ci.yml`（**+10/−7**，只 3 处文本）· 本报告
- 一句话：那条**打印 `SKIPPED` 就 `return`、于是默认跑里算 `passed` 而 `--ignored --list` 又看不到**的测试，现在**进清单了**（**16 → 17**，`skip accounting OK: 17 ignored = 8 run + 9 declared`）；manifest 里那两处与实现不符的声明改对了；三处发现各配一道**会红**的机械守卫（B32 三件套）。

## 0 三个结论（先给读数，后给理由）

| 项 | 改前 | 改后 |
| --- | --- | --- |
| `--ignored --list` | **16** | **17**（新条目 = `lifecycle::tests::the_real_root_inventory_is_read_when_it_is_named`） |
| gate 自己的算术 | `16 = 8 + 8` | **`skip accounting OK: 17 ignored = 8 run + 9 declared`**（抽出该 step 实跑，exit 0） |
| 那条测试的两种形状 | 缺 premise ⇒ **打印 SKIPPED、`return`、聚合里 `passed`** | 缺 premise ⇒ `-- --ignored` **exit 101 FAILED**；给了 premise ⇒ **exit 0 + 真读数** |

## 1 F3 主体：为什么选「`#[ignore]` + 体首 `expect`」

契约给了两条路，**我选 ①（`#[ignore]`）并且同时把 ② 的实质（体首 `expect`）一起做**，理由是实测的：

- **只做 ②（改成真断言、不 `#[ignore]`）会把「跳过」变成「每个默认跑都红的失败」**：本机（以及 CI 的两个 runner）都没有 `RUAGENT_T75_REAL_ROOT` ⇒ `cargo test --workspace` 会在**任何**机器上红一条没人能满足的用例 —— 那不是「可见」，是把一类**假红**换掉一类**假绿**，正是本代在打的形状。
- **①+`expect` 才是那 16 条的同形**：`#[ignore]` 让计数栏说 `ignored`（而不是 `passed`）**并且**把它送进 manifest 的账；体首 `expect` 让「有人 `-- --ignored` 却没给前提」**失败**而不是在空物上通过。原话级理由（`gen2-graph-instrument-repair.md:122`）：**缺输入是「你让我量，但我量不了」，不是「没有数据 ⇒ 通过」**。
- 改法（同一函数）：`#[tokio::test]` 前加 `#[ignore = "measures this machine's real backup inventory: needs RUAGENT_T75_REAL_ROOT=<a ruagent root> and runs with `-- --ignored`"]`，`let Ok(root) = … else { println!(SKIPPED); return; }` 换成体首 `expect`。**测量逻辑一行未动**（对象集、断言、打印都保持原样）。

**两个方向的本地读数（本单唯一的「真跑」用在这里，成本近乎零）**：

| 方向 | 命令 | 读数 |
| --- | --- | --- |
| 缺前提 | `test -p ruagent-memory --lib -- --ignored --exact lifecycle::tests::the_real_root_inventory_is_read_when_it_is_named`（不设 env） | **exit=101**，`FAILED`，消息 = `this instrument has no state in which it passes without measuring: set RUAGENT_T75_REAL_ROOT=<a ruagent root> …` |
| 有前提 | 同上 + `RUAGENT_T75_REAL_ROOT=<合成空目录>`（**绝不指 `~/.ruagent`**） | **exit=0**，`1 passed`，真读数：`READING t75 real root … backup surface = Readout(ResidualCount { hits: 0, truncated: false, total: Some(0) })` · `READING t75 real candidates = 0` |

⇒ 这条仪器现在**两个方向都被本地证过**（这恰好补上 t163 说的「那 7 条没有对照组」里的**一条**：本条有；另外 7 条仍然没有，见 §7）。

## 2 F1：manifest 里的变量名与实现不符

- **改前**（逐字）：`| \`store\`: …, live_copy_upgrades_…, … | NOT RUN: needs \`RUAGENT_T6_LIVE_COPY\` = a migrated COPY …（and for t25 a PRE-0024 copy）…`，而 `migrations.rs:1297/1302` 的第三条**实际读 `RUAGENT_T25_LIVE_COPY`**；改前 `ci.yml` 里 `RUAGENT_T25_LIVE_COPY` 命中 **0 次** ⇒ 按声明字面设 env，第三条会在 `expect` 处失败，读者很容易误判成「仪器坏了」。
- **改后**：写成**两条并列的变量名**并说清谁用哪条 —— `needs \`RUAGENT_T6_LIVE_COPY\` (for live_copy_keeps_history_nullable and live_copy_bigram_backfill_at_scale) and \`RUAGENT_T25_LIVE_COPY\` (for live_copy_upgrades_…, which needs a PRE-0024 copy)`，并补上 t36 的族内约束 `T6 and T25 must point at SEPARATE copies`。
- **守卫（F1）**：`manifest_mismatches(declared, reads)` 双向检查 —— ① manifest 里每个 `RUAGENT_*` 都必须能在 `crates/**` 的 `std::env::var("…")` 里找到；② `crates/**` 里每个 `*_LIVE_COPY` 读取都必须在 manifest 里被声明。**植入控制用的是改前那行原文**：用改前文本 + 今天的读取集跑检查器 ⇒ 必须报 `read but never declared: RUAGENT_T25_LIVE_COPY` ✓（不需要动共享树：**样本就是历史文本**）。

## 3 F2：被声明的名字被省略

- **改前**：`live_copy_upgrades_…`（省略号不是实际名字）。**改后**：写全名 `live_copy_upgrades_without_losing_distill_rows_and_then_holds_each_attempt`。
- **守卫（F2）**：每个被声明的测试名必须在 `crates/**` 的函数名集合里**恰好命中一个**（命中 0 = 虚构；命中多个 = 不是一个位置；带 `…` 的名字命中 0）。**植入控制**：用改前那行原文 ⇒ 必须把 `live_copy_upgrades_…` 报成不可解析 ✓。

## 4 F3 的守卫：按 B32 三件套，且**自证能红**

`crates/memory/src/lifecycle.rs` 的 `mod tests` 里新增 3 个测试 + 若干纯函数（扫描器/解析器都做成纯函数，才好喂植入样本）：

| 守卫 | 扫描下限 | 植入控制（必须红） | 具名排除 + 理由 | 红证是否落在共享树 |
| --- | --- | --- | --- | --- |
| F3 `no_test_announces_a_skip_and_returns_without_being_ignored` | `crates/**` 的 `.rs` 文件 **< 100 个就失败**（今天 117） | **在隔离根**里造 `crates/planted/src/lib.rs`（含该形状）⇒ 真树扫描必须报 `crates/planted/src/lib.rs:3`；把同一文件换成 `#[ignore]` 版本 ⇒ 必须**不报** | 7 条，逐条给理由（`lifecycle.rs:688` `-shm/-wal` 是 sidecar、`fts.rs:229` NON-HAN 术语有意跳过、`memembed.rs:243` 作用域不可解析就跳过、`store/lib.rs` + `store/migrations.rs` + `knowledge/retrieval-gold-{copy,live}.rs` 的**退役形状注释**），并断言每条排除的 `SKIPPED`/`NOT MEASURED` 字样**仍在注释里**（防排除表腐烂） | **零**：隔离根在 `%TEMP%`，测完按名字删（实测无残留） |
| F1 `the_manifest_declares_exactly_the_variables_the_instruments_read` | 声明的名字 ≥ 4 **且** 树里的读取 ≥ 4（今天 5 / 14） | 合成文本双向各一条 + **改前那行原文** | 无（注释不参与：只从 manifest step 的文本里取名字） | **零**（纯文本样本） |
| F2 `every_test_name_the_manifest_declares_resolves_to_one_function` | 函数名集合 ≥ 1000（今天 1922）**且** 声明的测试名 ≥ 8（今天 9） | 省略形式必须报、全名必须解析 + 改前那行原文 | 无 | **零**（纯文本样本） |

**过程诚实（三道下限各自抓到过东西，这也是「下限」存在的意义）**：① F1/F2 的**捕获边界**第一版按「缩进 < 2」切，切在 step 自己的 `if: always()` 上 ⇒ 下限报 **0 个声明名**，守卫红在「你什么都没读到」而不是「你读了但一致」；② 修成「缩进 < 6」后又被 step 里的**空行**（缩进 0）切断 ⇒ 下限再次抓到（0 个声明名）；③ 换成「空行不算边界」后才读到真正的 57 行体；④ `ruagent_names` 的第一版按**字节** `+1` 前进，在改前那行的 `…`（多字节）上 `&text[i..i+7]` **panic**（`not a char boundary`）—— 现在两个扫描器都按 **char 边界**前进。**四条都是我自己的 bug，全部由「先断言下限/植入控制」暴露**，而不是由人肉 review。

## 5 16 → 17：门禁的账同步了，且**用门禁自己的代码**读出来

`ci.yml` 的三处改动（`git diff --numstat` = **+10/−7**，逐行长这样）：

1. `declared_not_covered=8` → **`9`** + 注释从 `(8)` 改 `(9)`，并写明第九条是谁（memory 的 t75 仪器、read-only、t165 起 `#[ignore]`）；
2. **store 行**（F1+F2，§2/§3）；
3. 新增 **memory 行**：`| \`memory\`: the_real_root_inventory_is_read_when_it_is_named | NOT RUN: needs \`RUAGENT_T75_REAL_ROOT\` … 并写下这段历史（打印 SKIPPED+return ⇒ 算 passed 且 `--ignored --list` 看不见）`。

**`covered=8` 不变**（新那条**不在 CI 里跑**：CI 只用 `--include-ignored` 跑 `-p ruagent-daemon --test injection_e2e`）⇒ 声明为「未覆盖」是**准确**的，不是图省事。**t144 钉住的字节保持不动**：manifest step 的调用、`::error::` 那句判据文本、daemon 8 个名字**逐字未改**（diff 的 3 个 hunk 都不落在它们身上）。

**最强的一条读数是「让门禁自己说话」**：把 `Ignored-instrument manifest` 这个 step 的 `run:` 体**原样抽出来**（57 行）在 Git Bash（有 Windows 侧 cargo）里实跑：

```
`cargo test --workspace -- --ignored --list` found **17** ignored test(s);
this job accounts for **17** (8 run with `--include-ignored`,
9 declared not covered below).
…
skip accounting OK: 17 ignored = 8 run + 9 declared
FINAL_STEP_EXIT=0
```

（口径说明：第一次在 **WSL** 里跑这条 step 时它走了 `NO READING` 分支 —— **WSL 没有 cargo**，不是树的问题；换成 Git Bash 才有读数。这一步也顺带证明：那条「NO READING ⇒ exit 0」的分支**不会把无读数伪装成算术通过**，它自己在 summary 里写明「Derivative notice, not a count mismatch」。）

## 6 门禁（退出码在管道之前取；都是**最终字节**）

| 命令 | 读数 |
| --- | --- |
| `scripts/cargo-team.ps1 test -p ruagent-memory` | **exit=0**：lib `81 passed / 0 failed / 1 ignored` + 4 + 9 + 0（三个新守卫都在里面） |
| `scripts/cargo-team.ps1 clippy --workspace --all-targets -DenyWarnings` | **exit=0**（8.7s，尾部 `Checking ruagent-mcp`）。**中途红过一次，是我的真缺陷**：第一版守卫留下一个没人调用的脚手架函数（`skips_that_counts_as_passed_for_the_control`），`clippy -D warnings` 直接把它报成 `function is never used` ⇒ 删掉后转绿（这正是 `-D warnings` 该干的事） |
| `cargo fmt --all --check` | **exit=0 / 0 行**。中途红过一次：**只在我的文件** 5 处（`cargo fmt --all --check` 会暴露全树，所以我用 `rustfmt --edition 2024 crates/memory/src/lifecycle.rs` **只格式化我改的文件**，没有顺手重排别人的） |
| `bash .github/workflows/scripts/check-workflow-refs.sh` | **exit=0**（16s）：22 refs / not tracked 0 / 四份 workflow 全 `parses as YAML (PyYAML)` |
| PyYAML 形状复核 | 三个 job 步骤数 **13 / 9 / 6 不变**，`if`/`runs-on`/timeout 逐字不变；钉版载体仍 `{'1.95.0'} => SAME` |

**共享树零变异（负控全在隔离处）**：F3 的红证在 `%TEMP%\ruagent-mem-gen2-t165-control-<pid>`（测完按名字删，实测**无残留**）；F1/F2 的红证是**纯文本样本**（含改前那行原文）。三个文件的最终哈希：`crates/memory/src/lifecycle.rs` **`ffc50e3b…`** · `.github/workflows/ci.yml` **`e62b0581…`**（31636 B）· `test-evidence.sh` **未改**（在 inScope 里但本单不需要它：新那条是「声明」不是「跑」）。

## 7 未覆盖 / 仍未证（点名）

1. **t163 §6 那 7 条仍然没有带前提真跑**（store 3 / graph 3 / knowledge 2）：本单只改**声明**与**守卫**，不改它们的处境 ⇒ 「对照组」强度仍是 t163 的三档（daemon 8 = 真跑；graph/store = 文档里的带 env 正向读数；knowledge = 只有运行命令、未核对数字）。**新添的对照组只有 memory 这一条**（§1 的两向读数）。
2. **「撤掉 env 它会失败」现在只对 memory 那条有前置红证**（§1 方向 A）；另外 7 条要补，需要在隔离副本里变异（本单不做）。
3. **F4 只登记不改**：knowledge 行的「and the e5 model」比代码硬前提强（`FastEmbedder::try_new()` 失败会走 `Err` 分支记 note）—— 我判断它描述的是「有意义的读数」的前提而**不是**硬前提，改写措辞是文档偏好、不是正确性问题 ⇒ **登记于此**，留给需要的人。
4. **守卫的对象集是 `crates/**`**（契约就是 `crates/**`）：`cli/**`、`panel/**`（TS）、`scripts/**`（bash）**不在**扫描范围内 ⇒ 同样的「打印跳过 + return」形状若出现在那些地方，本守卫**看不到**（写下来，别把它读成「全仓无此形状」）。
5. **具名排除表是有意耦合**：那 7 条排除断言「`SKIPPED`/`NOT MEASURED` 仍在注释里」，所以有人删掉那些注释时守卫会红并给出「更新列表或改文件」的指令 —— 这是 B32 要的（排除必须具名且可腐烂被检出），但它确实把「注释删除」变成了需要一起收口的动作。
6. 守卫读的是**仓库里的** `ci.yml`（路径 = `CARGO_MANIFEST_DIR/../../.github/workflows/ci.yml`）；若这个 crate 被单独打包出去（没有仓库布局），守卫会**失败而不是跳过** —— 这是故意的（「扫不到 = 失明」），写下来供打包/发布流程参考。

## 8 残留

- 改动文件：`crates/memory/src/lifecycle.rs` · `.github/workflows/ci.yml` · 本报告。`crates/daemon/**`、`store/**`、`graph/**`、`knowledge/**`、`panel/**`、`scripts/**`、`tools/**` **零改动**。
- 未 push / dispatch / rerun / cancel / 建 tag；未启停任何进程（**没有**用 C32 读活守护进程的端口 —— 本单不需要）；**没碰 `~/.ruagent`**（方向 B 用的是 `%TEMP%` 里的合成空目录）。
- 复核材料：`%TEMP%\ruagent-t165\`（`list17.txt`（17 条清单原文）· `manifest-step.sh`（抽出的 57 行 step 体）· `summary2.md`/`summary-final.md`（step 两次自述）· `ignored-noenv.out`（方向 A 的 101）· `ignored-env.out`（方向 B 的真读数）· `clippy.out`/`clippy2.out`（含那次 `is never used`）· `fmt*.out` · `extract.py`/`mirror.py`（抽取与镜像脚本））。
