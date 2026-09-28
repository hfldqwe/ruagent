# `facts_as_of` 把时刻当字符串比（t67 #3）：同一时刻三种写法必须逐行相同（t82）

> **性质**：修复单。改了 **1 个文件**（`crates/graph/src/lib.rs`）+ 本报告。`crates/daemon`（含 `api.rs`）、`crates/graph/src/retrieve.rs`、`crates/store` **一个字节未动**。
> **日期**：2026-09-29。读数一律取在**最终字节**的构建上；活库只读（`mode=ro` + `VACUUM INTO` 到 `%TEMP%`），未启停 pid 79984。
> **来源**：审计 `docs/design/reviews/gen3-audit-graph.md` #3。本单是「**同一个问题只在一处修过**」的收口：RVC-1 修了检索侧，写入/API 侧留着同一个 bug。

## 0 一句话

`facts_as_of` 现在**解析**时刻而不是比字符串：同一时刻的三种写法（`…Z` / `…+00:00` / `…+08:00`）返回**逐行相同**的结果（活库上 **30/30/30**，改前是 **38/30/43**；逐实体 **0/63** 不同，改前 **6/63**）；非时刻输入**响亮拒绝**（改前会被当成一次合法查询、给出看似正常的空结果）；排序改成「**解析后时刻降序 + id**」全序（改前是文本序，日期形行会乱序）。

| 口径 | 改前（审计 t67 + 本次复现） | 改后（最终字节） |
| --- | --- | --- |
| 同一时刻三种写法（活库 67 边） | **38 / 30 / 43**，解析正确值 30 ⇒ 差 **16 / 0 / 21** | **30 / 30 / 30**，差 **0 / 0 / 0** |
| 逐实体（API 形状，63 个实体） | **6/63** 条数不同；实体 52：**0 vs 5** | **0/63** 不同；实体 52：**0 vs 0** |
| 活库非规范形状（`valid_at` 纯日期 1/67） | 文本比 ⇒ 结果依赖调用方拼法 | `'2026-09-13'` **解析**为 `2026-09-13T00:00:00Z`，在 R 处为真（**被处理，不是被拒绝**） |
| 非时刻输入（`not-a-time` / `""` / `2026-13-45T99:99:99Z` / `now`） | 当字符串比 ⇒ 合法外观的空结果 | **`Err`: `at is not an instant: … (accepted: RFC3339 …; YYYY-MM-DD; %Y-%m-%d %H:%M:%S)`** |
| 不同时刻（负控） | — | 另一个时刻 → **1 条边** ≠ R 处 **4 条边**（不是常量、不是全选） |
| Rust 侧（同夹具，逐行 id 序） | 反做负控：`Z` **5 条** `[…"5#ends_exactly_at_R"…]` vs `+00:00` **4 条** ⇒ 断言炸 | 三种写法各 **4 条**、**id 序逐行相同** |

## 1 根因：同一个 bug 只在检索侧修过

* **检索侧（已修，RVC-1）**：`retrieve.rs:733-751` —— 在入口处 `parse_ts` **一次**，非时刻**响亮拒绝**（`as_of is not an instant: …`）；谓词 `retrieve.rs:670 true_then_at` + `:687 temporal_status`。测试也只在那边有（`crates/graph/tests/temporal.rs:154`，注释正写着「`valid_at <= as_of` as TEXT」）。
* **API 侧（本单前仍未修）**：`lib.rs` 的 `facts_as_of` 把 `at` 与两列时间**一起塞进 SQL**：
  ```sql
  WHERE (src = ?1 OR dst = ?1)
    AND valid_at <= ?2
    AND (invalid_at IS NULL OR invalid_at > ?2)
  ORDER BY valid_at DESC
  ```
  SQLite 的文本比较看不见「同一时刻的不同拼法」⇒ 同一时刻三种写法三个答案。**这是公开 API**（`pub async fn facts_as_of`），所以 bug 在这里而不在端点。

## 2 形状（与检索侧同一套语义）

```rust
let at = parse_ts(at).ok_or_else(|| DbError::Io(std::io::Error::new(
    std::io::ErrorKind::InvalidInput,
    format!("at is not an instant: {at:?} (accepted: RFC3339 …; YYYY-MM-DD; %Y-%m-%d %H:%M:%S)"),
)))?;                                   // ← 与 retrieve.rs:737-751 同形：拒绝而不是比较
… 取出该实体的全部关联边，在 Rust 里 retain(|e| in_force_at(&e.valid_at, e.invalid_at.as_deref(), at))
rows.sort_by(|a, b| parse_ts(&b.valid_at).cmp(&parse_ts(&a.valid_at)).then_with(|| a.id.cmp(&b.id)));
```

三点设计：

1. **两侧都解析**：谓语 `in_force_at` 与 `retrieve.rs:670 true_then_at` **同一规则**——**起点含、终点不含**（`valid_at <= at < invalid_at`，与 `crates/graph/tests/gold/as_of.json:2` 的判据一字对应）。
2. **不可解析的存储值仍退回原始文本比，但比的是 `at` 的规范形**（`at.to_rfc3339()`，与检索侧一致）：这样「一行不可解析 ⇒ 整行从答案里消失」不会发生，且**退回路径本身与调用方拼法无关**（否则判据「三种写法逐行相同」会被一行坏数据破坏）。
3. **顺序是全序**：解析后时刻降序 + `id` 兜底。文本序会把 `'2026-09-13'`（日期形）排到 `'2026-09-13T00:00:00+00:00'` 之后，尽管它们是同一时刻；`id` 兜底是为了**顺序不依赖执行计划**（RVC-4 的教训）。不可解析的 `valid_at` 排在最后而不是打乱可见答案。

## 3 活库读数（本次在 `mode=ro` 副本上复现，与审计同值）

对象集：`entity_edges 67`（当前 60、**纯日期 `valid_at` 1**）。基准时刻 R = `2026-09-13T18:38:15Z`。

```
=== BEFORE（改前：SQL 文本比较）===
  canonical(Z)   -> 38      +00:00 -> 30      +08:00(same instant) -> 43
  instant-correct = 30 | diffs: Z 16, +00:00 0, +08:00 21
  pairwise: Z vs +00:00 16 | Z vs +08:00 5 | +00:00 vs +08:00 21
  per-entity (API 形状): 6/63 不同；例 30、46、47、50、51 …；实体 52: Z=0 vs +08:00=5
=== AFTER（本单语义的 Python 镜像）===
  canonical(Z) -> 30   +00:00 -> 30   +08:00 -> 30   | all three identical: True
  per-entity: 0/63 不同（实体 52: 0 vs 0）
  负控 | 另一个时刻（R−12h）-> 1 条边 vs R 处 30 条 | 对称差 29 ⇒ 不同时刻不同结果
  纯日期行 id=1 '2026-09-13' -> 解析为 2026-09-13 00:00:00+00:00 | 在 R 处为真 ⇒ 该形状被处理
```

**口径声明（不静默）**：§3 的「改后」是 **Python 镜像**（把新语义按同一规则重写），不是 Rust 代码的执行结果 —— 它证明的是「在**活库的真实形状**（含纯日期、9 位纳秒、`+08:00`）上这条规则给出 30/30/30」。**Rust 侧的实证是 §4 的单元测试**（真代码、真产物、逐行 id 序）。两者互为交叉验证，但别把镜像当实现的读数。

## 4 Rust 侧一手读数（最终字节，`test -p ruagent-graph --lib -- --nocapture`）

夹具 6 行，覆盖活库里的形状与两侧边界：

| # | relation | `valid_at` | `invalid_at` | R 处是否在效 |
| --- | --- | --- | --- | --- |
| 1 | starts_before | `2026-09-13T10:00:00+00:00` | — | 是 |
| 2 | starts_exactly_at_R | `2026-09-13T18:38:15Z` | — | 是（起点含） |
| 3 | starts_one_second_after_R | `2026-09-14T02:38:16+08:00` | — | 否 |
| 4 | date_only_midnight | `2026-09-13` | — | 是（午夜 UTC） |
| 5 | ends_exactly_at_R | `2026-09-13T10:00:00+00:00` | `2026-09-14T02:38:15+08:00` | 否（终点不含） |
| 6 | ends_after_R | `2026-09-13T10:00:00+00:00` | `2026-09-13T20:00:00Z` | 是 |

```
READING t82: one instant written three ways ->
  2026-09-13T18:38:15Z          = 4 edges ["2#starts_exactly_at_R","1#starts_before","6#ends_after_R","4#date_only_midnight"]
  2026-09-13T18:38:15+00:00     = 4 edges [同上，逐行相同]
  2026-09-14T02:38:15+08:00     = 4 edges [同上，逐行相同]
READING t82 (control): a different instant -> 1 edges ["4#date_only_midnight"] vs 4 edges at R
READING t82 (refusal): "not-a-time"           -> io error: at is not an instant: …
READING t82 (refusal): ""                     -> io error: at is not an instant: …
READING t82 (refusal): "2026-13-45T99:99:99Z" -> io error: at is not an instant: …
READING t82 (refusal): "now"                  -> io error: at is not an instant: …
```

断言四条：① 三种写法 **id 序逐行相同**；② 恰为本该在效的 4 条（`5#` 因「终点恰好等于 R」被排除、`3#` 因「起点在 R 之后」被排除）；③ 顺序是**解析后降序 + id**（`18:38:15 → 10:00(id 1) → 10:00(id 6) → 00:00`，即 `2#,1#,6#,4#`）——文本序做不到这一点（`'2026-09-13'` 是 `4#`，文本上排在最前/最后取决于拼法）；④ **负控**：另一个时刻结果不同（1 vs 4 条边），证明比较没被关掉、没退化成常量、也没变成「全选」。

## 5 反做负控：坏侧必须红（两次尝试，都如实记账）

**尝试 1（失败，且是编译红）**：我的替换文本漏了两行（`.await?` 与 `.map_err(DbError::from)`）⇒ `error[E0308]: mismatched types`、`could not compile ruagent-graph (lib/lib test)`。**captain 04:31 的全量 `check --workspace --all-targets` 抓到的正是这个 ~4 秒窗口**（`CHECK_EXIT=101`）。恢复：`lib.rs` SHA-256 前 16 位 **`26121AA086A40D48`**，与反做前**逐字一致**（`identical: True`）。教训见 §8。

**尝试 2（改前代码逐字替换）**：

```
READING t82: one instant written three ways ->
  2026-09-13T18:38:15Z = 5 edges ["2#starts_exactly_at_R","6#ends_after_R","5#ends_exactly_at_R","1#starts_before","4#date_only_midnight"]
  2026-09-13T18:38:15+00:00 = 4 edges ["6#ends_after_R","5#ends_exactly_at_R","1#starts_before","4#date_only_midnight"]
test result: FAILED. 6 passed; 2 failed; … exit 101
assertion `left == right` failed: Z and +00:00 must be row-for-row identical
  left:  ["2#…","6#…","5#…","1#…","4#…"]      ← Z：5 条，多算了「起点恰在 R」的 2#
  right: ["6#…","5#…","1#…","4#…"]           ← +00:00：4 条，且把「终点恰在 R」的 5# 也算进来了
```

⇒ **同一时刻的两种写法给出不同答案**（5 vs 4，且错法还不一样：一个多算起点、一个多算终点），断言正好在该处炸；`facts_as_of_refuses_a_text_that_is_not_an_instant` 也红（改前代码不拒绝任何文本）。**两条新测试都真的挡得住改前形状**。

**恢复**：`26121AA086A40D48` —— 与反做前一致（两次尝试均 `identical: True`）。随后复跑：`test -p ruagent-graph --lib` → `test result: ok. 8 passed; 0 failed`、exit 0；整条 `test -p ruagent-graph` → ok-lines=12 / FAILED=0 / panicked=0 / exit 0。
**窗口纪律**：按 t93/t81（C22）三件套先广播（文件 / 只影响哪两条测试 / ~10s / 恢复+哈希），窗口约 10s（另加尝试 1 的 4s 编译红），窗口结束已广播「回到绿」。

## 6 门禁（第 6/16/22 条形态，三个面分开）

| 面 | 命令 | 读数 |
| --- | --- | --- |
| **编译面（我的 crate）** | `check -p ruagent-graph --all-targets` | **exit 0**，`Finished in 2.46s`，无 error |
| **编译面（全树，含测试目标）** | `check --workspace --all-targets` | **exit 0**，`Finished in 7.98s`（日志里 `crates/memory/src/lifecycle.rs:605/617/1702/1703` 的 `-->` 是**警告位置**、非 error） |
| **测试面** | `test -p ruagent-graph` | **ok-lines=12 · FAILED-lines=0 · panicked-lines=0 · exit=0**；两条新测试逐个 `ok`（`tests::facts_as_of_answers_the_same_for_one_instant_written_three_ways`、`tests::facts_as_of_refuses_a_text_that_is_not_an_instant`） |
| **crate 级门禁** | `clippy -p ruagent-graph --all-targets -DenyWarnings -CleanFirst` | **exit 0** · `-CleanFirst` 卸 **6443 files / 1.0 GiB**（如实重读）· **13 个 unit 行**（12 个不同名：`ruagent_graph`×2=lib+lib test、`ruagent_store`、`seed_resolution`、`resolution`、`extraction_gold`、`fixture_ownership`、`fixture`、`empty_recall_pattern`、`live_after`、`entity_query_shapes`、`multihop_gold`、`temporal`；**本次无 `ruagent_core` 行**——它的指纹活过了 `-CleanFirst`，如实记）· **error-lines=0** |
| 格式面（t66） | `rustfmt --edition 2024 --check crates/graph/src/lib.rs` | **0 个改动行** |

**daemon 门禁**：按判据「若最终仍碰了 `daemon`」——**未碰**（`api.rs` 只读、未改；`changedPaths` 不含 daemon），故未单独跑 `-p ruagent-daemon`；但其编译目标已被 `check --workspace --all-targets` 覆盖（exit 0）。

## 7 HTTP 侧只是掩盖 + 交回 finding

**证据（只读）**：`crates/daemon/src/api.rs:1283-1295` —— 端点自己先规范化：

```rust
// … `facts_as_of` compares timestamps as text, so an unnormalized spelling silently selects a
// different edge set -- and a non-instant used to be accepted with a 200 (measured t57: …)
let dt = ruagent_graph::parse_ts(at).ok_or_else(|| ApiError::bad_request("at must be an RFC3339 instant"))?;
ruagent_graph::facts_as_of(state.mgr.db(), id, &dt.to_rfc3339()).await?
```

⇒ HTTP 今天看似正常，**只因为端点把 `at` 预先规范化成 `+00:00` 形**；而 `facts_as_of` 是公开 API（`pub`、被 MCP/CLI/面板之外的调用者可直接用），所以 bug 属于函数本身 —— 本单修的正是它，端点未动。**判据要求「若你认为 HTTP 侧也要改 ⇒ 交回 finding」**，因此：

* **f1（文档/冗余，`crates/daemon/src/api.rs:1283-1287`）**：那段注释现在**与实际不符**（它说 `facts_as_of` compares timestamps as text；改后不再如此）。建议：更新注释，并说明「`parse_ts`+`to_rfc3339` 现在只是**防御性**预处理」——400 的友好错误仍值得保留（HTTP 层的错误形状），规范化本身已非必需。**未改**（`api.rs` 不在本单 inScope）。
* **f2（谓词重复，`crates/graph/src/retrieve.rs:670` vs `crates/graph/src/lib.rs`）**：`true_then_at`（检索侧）与我新加 `in_force_at`（本单）是**同一规则的两份拷贝**。建议后续把其中一个改成 `pub(crate)`、另一个删除，避免第三次漂移。**未改**（`retrieve.rs` 不在本单 inScope；这也是报告 §2「两侧同一规则」只能靠注释对齐的原因）。
* **f3（同类遗留，`crates/graph/src/lib.rs:200-214 current_facts`）**：它 `ORDER BY valid_at DESC` 仍是**文本序**（无时刻比较，只有排序）。纯日期行因此可能与同刻的 `T00:00:00+00:00` 行**顺序颠倒**。它与本单是同族（instant vs text），但修它=**改变公开 API 的输出顺序**，需要自己的读数与 triage ⇒ 建议单独立单，本单**未动**（避免 drive-by 改 API 顺序）。

## 8 过程自我记账（不静默）

1. **三次编译红，其中两次在本会话**：t81 的 `E0277`+`E0308`、t81 的 `E0061`、以及本单反做**尝试 1** 的 `E0308`。第三次的根因是「替换文本不完整」——**我在广播里写了「反做态可编译」，却没先证明它**。
   **升级后的做法**：① 反做负控**优先在隔离 worktree 里跑**（C22 首选）；② 若必须在共享树里跑，替换文本必须**逐字来自已知可编译的版本**（本单尝试 2 就是 `git` 里改前那段逐字），并在替换后**先 `check` 再 `test`**；③ 每次跨文件前先 `check --workspace --all-targets`（本单已做，最后两条读数 exit 0）。
2. **仪器坑（已遇两次，值得进纪律）**：PowerShell `Select-String -Pattern 'FAILED'` **默认大小写不敏感**，会把 `test result: ok. … 0 failed` 算成 FAILED ⇒ 红/绿行数一律 `-CaseSensitive` 且锚定 `test result: FAILED`。
3. **我自己的镜像 bug**：第一次把 `+08:00` 那种拼法写成「UTC 的 02:38:15」（= 另一个时刻，晚 8 小时），于是「改后」读数假红（`+08:00` 处 5 条）。修正为同日 `02:38:15+08:00`（与 R 同一时刻）后才是 30/30/30。**教训**：镜像里的「同一时刻」必须**断言等值**（我后来加了 `R == R8` 的打印），否则测的是另一个问题。

## 9 本单没有做的

* 审计 #4/#5/#6 的其它条目（全表读、合并自环、`void_episode` 吞错等）—— 各自立单；#6/#1/#2 已由 t81 关闭。
* `retrieve.rs`、`api.rs`、`crates/store`：**未动**（f1/f2/f3 已交回）。
* `current_facts` 的文本序（f3）：**未动**（改公开顺序需独立判据）。

## 10 复现命令

```powershell
# 三种写法逐行相同 + 负控 + 拒绝（Rust 侧一手读数）
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 test -p ruagent-graph --lib -- --nocapture
# 两个契约门禁
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 test -p ruagent-graph
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 clippy -p ruagent-graph --all-targets -DenyWarnings -CleanFirst
# 编译面（全树）
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 check --workspace --all-targets
```
