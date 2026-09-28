# F-5 定义侧 + gold drift 由判据承载 + RV-A-5（t30）

> 状态：**已完成并通过验证**。产物：`crates/knowledge/src/store.rs`（`ScoreKind` 逐变体 rename）、`crates/knowledge/tests/score-kind-wire.rs`（新，漂移钉）、`crates/knowledge/tests/common/mod.rs`（drift 从打印升级为判据 + 可失败的 `seed_checked`）、`crates/knowledge/tests/gold-seed-idempotence.rs`（判据的可证伪仪器）、`crates/knowledge/tests/retrieval-gold-copy.rs` / `retrieval-gold-live.rs`（RV-A-5）+ 本文件。
> 作者：recall。时间窗：**2026-09-28T02:0x → 02:3x+08:00**。
> inScope 遵守：只改 `crates/knowledge/src/store.rs`、`crates/knowledge/tests/`、本报告；未写活库、未启停 pid 79984。

**一句话**：三件事，都是「让判据真的落在读数上」。(1) `ScoreKind` 有**两条线格式**且当时不一致 —— serde 发 `"RrfRank"`、`as_str()` 发 `"rrf_rank"`（V-A F-5），现在逐变体显式 rename 并有一条把「serde 输出 == `as_str()`」钉住的测试；(2) `[gold] drift` 只打印 ⇒ 现在**seed 拥有的行**分歧让测试红并列出该行，**`judged_by='human'`** 允许分歧、单独计数、绝不覆盖；(3) RV-A-5：两条 live 仪器在缺 env 时**打印 `NOT MEASURED` 后返回**，于是被计成 `passed` ⇒ 现在它们 `#[ignore]`（计数里明写 `ignored`），而**显式运行时缺 env 直接失败**，并且新增「读数真的产生了」的断言。

---

## 1 逐条验收

| # | 验收项 | 改前 | 改后 | 判 |
| --- | --- | --- | --- | --- |
| 1 | **F-5 定义侧**：serde 输出 == `as_str()` 五字面量 | `serde="RrfRank"`（且五个都是 Rust 变体名） | 五个 `AGREE`（输出见 §2） | **达标** |
| 2 | 漂移钉测试（形态照 `residual-scan.rs:40`） | 无 | `tests/score-kind-wire.rs`：逐变体断言 + 字面量**集合**断言（新增变体即红） | **达标** |
| 3 | **drift 由判据承载**（seed-owned ⇒ 红并列出该行；human 允许分歧、单独计数、绝不覆盖） | 只打印：`[gold] drift: N of 22 …`，同一构造**不红** | `drift 1 seed-owned ... FAILURE: seed-owned=["Docker compose: table=(wrong-class,0,judged_by=title-derived) frozen=(exact-ascii,1)"]`；verdict=Err 且**点名该行**；human 行单独计数且**跨 re-seed 不被覆盖** | **达标** |
| 4 | **RV-A-5**：两条「没测也报绿」的测试改成读数判据 | `2 passed; 0 ignored`（各打印 `NOT MEASURED` 后返回） | 各为 `0 passed; **1 ignored**`；显式运行缺 env ⇒ **FAILED**（`no state in which it passes without measuring`）；并加「读数真的产生了」的断言 | **达标** |
| 5 | 保留并写进报告的两条取舍（不 upsert / `pragma_database_list` 活库守卫） | — | §5，两条都有测试或读数承载 | **达标** |
| 6 | `test -p ruagent-knowledge` 通过；clippy `-CleanFirst` 零诊断 + 两件证据 | — | **67 passed / 0 failed / 2 ignored**；clippy `Checking ruagent-knowledge` 且零诊断 | **达标** |
| 7 | 只改 inScope、不写活库、不启停 79984 | — | §7（改动清单 + 只读纪律） | **达标** |

**确切命令**
```powershell
$env:PATH="$env:USERPROFILE\.protoc\bin;$env:PATH"
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 test -p ruagent-knowledge
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 clippy -p ruagent-knowledge --all-targets -DenyWarnings -CleanFirst ruagent-knowledge
# 两条 live 仪器（可测量路径仍可跑；缺 env 时显式运行会失败）
$env:RUST_TEST_NOCAPTURE="1"; $env:HF_HOME="$env:TEMP\ia-hf\hub"; $env:RUAGENT_IA_LIVE_COPY="$env:TEMP\ia-live"
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 test -p ruagent-knowledge --test retrieval-gold-copy -- --ignored
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 test -p ruagent-knowledge --test retrieval-gold-live -- --ignored
```

---

## 2 F-5：同一个值，两条线格式，各自都没错

**改前读数**（用真序列化器，`--test score-kind-wire` 的第一跑；这也就是 V-A 量到的那个）：
```
[t30] RrfRank: serde="RrfRank" as_str="rrf_rank" frozen="rrf_rank" DISAGREE
left: "\"RrfRank\""   right: "\"rrf_rank\""
the_literal_set_is_frozen: left: ["\"Calibrated\"", "\"Cosine\"", "\"KeywordBm25\"", "\"RrfRank\"", "\"SemanticDistance\""]
                         right: ["\"bm25\"", "\"calibrated\"", "\"cosine\"", "\"rrf_rank\"", "\"semantic_l2sq\""]
```
即：`as_str()` 与 daemon 手写 JSON 用冻结字面量，而 `#[derive(Serialize)]` 没有 rename ⇒ 发的是 **Rust 变体名**。没有编译错、没有测试红 —— H-1 若直接 serde 序列化 `RankedHit`/`SearchPage`，**线的词汇表就被静默换掉了**。

**改后读数**：
```
[t30] RrfRank:         serde="rrf_rank"      as_str="rrf_rank"      frozen="rrf_rank"      AGREE
[t30] SemanticDistance: serde="semantic_l2sq" as_str="semantic_l2sq" frozen="semantic_l2sq" AGREE
[t30] KeywordBm25:     serde="bm25"          as_str="bm25"          frozen="bm25"          AGREE
[t30] Cosine:          serde="cosine"        as_str="cosine"        frozen="cosine"        AGREE
[t30] Calibrated:      serde="calibrated"    as_str="calibrated"    frozen="calibrated"    AGREE
test result: ok. 2 passed
```

**为什么用「逐变体 rename」而不是 `rename_all`（这条值得写下来，因为那个「显然的修法」是错的）**：
`rename_all = "snake_case"` 会派生出 `semantic_distance` 与 `keyword_bm25`，而冻结字面量是 `semantic_l2sq` 与 `bm25` —— 那会**把一条悄悄发错的名字换成另一条悄悄发错的名字**。五个字面量里有两个（`semantic_l2sq`、`bm25`）没有任何可派生的拼写，所以线格式只能写在每个变体旁边，而不是算出来。`store.rs` 的枚举注释与测试的头注释都写了这句话。

**两条断言，各自防一种事**：
- 逐变体：`serde_json::to_string(&kind) == format!("\"{}\"", kind.as_str())` —— 两条线格式不许分叉；
- 集合：五个字面量排序后 == `["bm25","calibrated","cosine","rrf_rank","semantic_l2sq"]` —— **新增一个变体就是一次可见的决定**（列表长度断言会先失败）。

跨 crate 的第二条钉（消费面）归 integ 的 t19，本单不碰。

---

## 3 drift：从「打印」升级为「判据」，以及它现在怎么分账

**改前（t29 的形态）**：`[gold] drift: N of 22 …` 只活在输出里。它**不能失败** —— 冻结常量与库表一旦分歧，所有读常量的判据照样全绿，而下一代 C3 的前置（≥60 条查询且 ≥20 条无答案）是**按表**读的。这正是「假达标」的入口。

**改后**：检查与判据分开，判据在**每个仪器都经过的那个入口**（`common::seed`）上：
```rust
pub struct DriftReport { seed_owned: Vec<String>, human: Vec<String>, absent: Vec<String>, total: usize }
impl DriftReport { pub fn into_result(self) -> Result<Self, String> }   // seed-owned / missing ⇒ Err
pub async fn db_drift_check(db, set_id) -> DriftReport                   // 只查、只打印，可被仪器复用
pub async fn seed_checked(db) -> Result<(i64,i64), String>               // seed + 判据，返回裁决
pub async fn seed(db) -> (i64,i64)                                      // = seed_checked().unwrap_or_else(panic)
```
- **`seed_owned`（`judged_by` 非 `human`）分歧 ⇒ Err**，错误信息**逐行点名**并同时给出表里值/冻结值；
- **`human` 分歧 ⇒ 允许**，单独计数并打印（判据不看它）；
- **`absent`（冻结查询在表里一行都没有）⇒ Err**（seed 没生效同样是判据失败）；
- 三种情况都**照旧打印**，所以读数在通过时也看得见。

**可证伪读数**（同一次运行、同一个构造、同一个检查器，**单变量 = 报告有没有消费者**）：
```
[gold] drift: 0 seed-owned disagreement(s), 0 human-labelled ..., of 22 -- the table still holds the frozen set
（构造一行 seed-owned 漂移：UPDATE ... SET class='wrong-class', answerable=0 WHERE judged_by <> 'human'）
[gold] drift: 1 seed-owned disagreement(s) ... -- FAILURE: seed-owned=["Docker compose: table=(wrong-class,0,judged_by=title-derived) frozen=(exact-ascii,1)"]
[t30] PRE-t30 SHAPE (report only): the checker found 1 seed-owned disagreement(s) and nothing consumed the report -> this run is not failed by it.
[t30] JUDGED (this revision): verdict=Err -> gold drift: 1 seed-owned row(s) disagree ... Rows: ["Docker compose: table=(wrong-class,0,judged_by=title-derived) frozen=(exact-ascii,1)"]
[t30] seed_checked -> Err(..) as required
test result: ok. 3 passed
```
即：**同一行漂移，在「只打印」的形态下这一跑不会红；在判据形态下 `Err` 且点名该行**。测试最后还断言 `seed_checked` 也返回 `Err`（仪器自己的入口同样会红）。

**human 行的两半（也测了）**：
```
[gold] drift: 0 seed-owned ..., 1 human-labelled disagreement(s) ... -- the table still holds the frozen set
[t30] after re-seeding, the human row still holds ("human-says-something-else", "graded by a person")
```
「human 允许分歧」与「human 永不被覆盖」都成立：判据放行，re-seed 之后那行**连 `note` 都是人写的那句**。

---

## 4 RV-A-5：没有读数的绿是假绿

**被点名的两条**（`docs/design/reviews/gen2-recall-review.md` §RV-A-5）：`retrieval-gold-copy.rs::live_copy_gold_set` 与 `retrieval-gold-live.rs::live_copy_evidence_and_run_record`。它们当时是：
```rust
let Ok(root) = std::env::var("RUAGENT_IA_LIVE_COPY") else {
    println!("… NOT MEASURED: set RUAGENT_IA_LIVE_COPY=…");
    return;                       // ← 什么都不测，然后报 passed
};
```
于是在默认 `cargo test -p ruagent-knowledge` 里，**计数＝测量数**这条不成立：`2 passed` 里有 2 条没有产生任何读数。

**改后**：`#[ignore = "…needs RUAGENT_IA_LIVE_COPY (a COPY of a live root -- it WRITES) and runs with `-- --ignored`"]` + 体首 `expect(...)`（缺 env **失败**而不是跳过）+ 新增「读数真的产生了」的断言。

**改前 → 改后读数（同一条命令 `test -p ruagent-knowledge`，不带 env）**
```
改前（t7/t29 的形态，逐二进制）：
  [t7-gold]    NOT MEASURED: set RUAGENT_IA_LIVE_COPY=…
  test result: ok. 1 passed; 0 failed; 0 ignored        ← 计成「通过」
  [t7-evidence] NOT MEASURED: set RUAGENT_IA_LIVE_COPY=…
  test result: ok. 1 passed; 0 failed; 0 ignored        ← 计成「通过」
  全库合计：66 passed; 0 failed; 0 ignored              ← 其中 2 条没有读数

改后：
  test live_copy_gold_set ... ignored, measures the C1/C2/C9 live reading: needs RUAGENT_IA_LIVE_COPY …
  test result: ok. 0 passed; 0 failed; 1 ignored
  test live_copy_evidence_and_run_record ... ignored, measures the per-hit evidence reading: …
  test result: ok. 0 passed; 0 failed; 1 ignored
  全库合计：67 passed; 0 failed; 2 ignored              ← 「测了多少」与「跳过多少」分开了
```
（合计从 66 → 67 是因为本单新增 `score-kind-wire` 的 2 条 + gold drift 判据的 1 条 = +3，减去被改成 ignored 的 2 条，净 +1。）

**显式运行时缺 env ⇒ 失败（不是静默跳过）**：
```
thread 'live_copy_gold_set' panicked at crates\knowledge\tests\retrieval-gold-copy.rs:41:54:
this instrument has no state in which it passes without measuring: set RUAGENT_IA_LIVE_COPY=<a COPY of a
live root: data/ruagent.db, data/lancedb, knowledge/> (it seeds gold rows, so never point it at ~/.ruagent)
and run with `-- --ignored`: NotPresent
test result: FAILED. 0 passed; 1 failed
```
**「读数真的产生了」也被断言了**（不只靠 env 在不在）：
- `retrieval-gold-copy`：`report.rows.len() == GOLD.len()`（15）且 `report.no_answer.len() == NO_ANSWER.len()`（7）—— 报告短了/空了就不算测量（`Report` 把可答与不可答分两个向量，所以两条各自断言）；
- `retrieval-gold-live`：`queries_measured == GOLD.len()` 且 `!relevance_values.is_empty()`。

**可测量路径没被破坏**（带 env + `-- --ignored` 重跑，真模型）：
```
[t7-gold] gold set id=1 newly written=0 embedder=fastembed:multilingual-e5-small (dim 384)
[t7-gold] n=15 misses=0 recall@1=0.7333 recall@5=0.9333 recall@20=1.0000 MRR=0.8185 nDCG@10=0.8621
test result: ok. 1 passed                                     ← retrieval-gold-copy
test result: ok. 1 passed                                     ← retrieval-gold-live（C4a/C5 与新断言全过）
```

**同族的旁证（不在本单 inScope，登记）**：`crates/store` 里也有两条同样的形态（`live_copy_keeps_history_nullable`、t25 的 `live_copy_upgrades_…`）：缺 env 时打印 `NOT MEASURED` 后返回并计为 passed。t30 的 inScope 只含 `crates/knowledge`，所以**我没改它们**；建议下一张 store 单照本单形状处理（owner 是我）。

---

## 5 两条被保留的取舍（t29 的裁决，本单照旧并写进报告）

1. **不 upsert**。`INSERT OR IGNORE` 从不碰已存在的行 —— 而 `judged_by` / `note` 正是**人工判定**唯一会落的地方（C3 的前置就是 `judged_by='human'`，今天 0 行）。upsert 会覆盖语料里**唯一的人类标签**。本单把这一点从「注释里的理由」变成**读数**：human 行分歧被判据放行，且 re-seed 后 `("human-says-something-else", "graded by a person")` 原样还在（§3 末）。
2. **`pragma_database_list` 活库守卫**。seed 会写库，所以它拒绝活库根：判据读的是连接**实际打开**的那个文件，而不是一个调用方可能忘记传的参数；判决是纯函数 `is_inside`（组件级，`.ruagent-other` 不会被误判），4 个用例单测（活库本体/活库下任意文件拒，兄弟目录与 `%TEMP%` 副本放行）。**panic 分支未对真活库根执行**（那正是它要拦的动作），如实登记。

---

## 6 验证读数

| 命令 | 读数 |
| --- | --- |
| `test -p ruagent-knowledge`（无 env） | `35+3+6+9+0+0+9+3+2+0 = **67 passed**; 0 failed; **2 ignored**`（exit 0）；两条 live 仪器各打印 `ignored, measures …` 的理由行 |
| `clippy -p ruagent-knowledge --all-targets -DenyWarnings -CleanFirst ruagent-knowledge` | `Checking ruagent-knowledge v0.1.0` + `Finished`，**零诊断**（exit 0） |
| `test -p ruagent-knowledge --test gold-seed-idempotence -Nocapture` | `3 passed`：PRE-t30 形态看见漂移但不红；JUDGED ⇒ `Err` 点名该行；human 行跨 re-seed 不变 |
| `test -p ruagent-knowledge --test score-kind-wire -Nocapture` | `2 passed`：五个 `AGREE` + 字面量集合 |
| 两条 live 仪器 + env + `-- --ignored` | 各 `1 passed`；`newly written=0`；指标与 t7 逐位相同（0.7333/0.8185/0.8621） |

**两件证据（契约要求）**
1. **覆盖哪些 target**：`--all-targets` = `lib` + **8** 个集成测试二进制，逐目标读数（本次实跑，`Running …` 行逐一对上）：
   | target | 读数 |
   | --- | --- |
   | `unittests src\lib.rs` | 35 passed |
   | `tests\gold-seed-idempotence.rs` | 3 passed（idempotence + **drift 判据** + 活库守卫） |
   | `tests\residual-scan.rs` | 6 passed |
   | `tests\retrieval-cjk.rs` | 9 passed |
   | `tests\retrieval-gold-copy.rs` | **0 passed; 1 ignored**（RV-A-5：读数在 `-- --ignored` + env 下） |
   | `tests\retrieval-gold-live.rs` | **0 passed; 1 ignored**（同上） |
   | `tests\retrieval-legs.rs` | 9 passed |
   | `tests\retrieval-quality.rs` | 3 passed |
   | `tests\score-kind-wire.rs`（新） | 2 passed（F-5 钉） |
   | **合计** | **67 passed / 0 failed / 2 ignored** |
2. **本次确实重查了哪个包**：`-CleanFirst` 下输出出现 `Checking ruagent-knowledge v0.1.0` —— 该包本次真被重查，不是命中上一轮缓存指纹。另：本单最后一轮 lint 修（`common/mod.rs:315` 两个 unused 变量）之后**重跑了这两条 gate**，不是复用修之前的读数。

---

## 7 边界与未测

- **只改 inScope**：`crates/knowledge/src/store.rs`（逐变体 rename + 注释）、`crates/knowledge/tests/{score-kind-wire.rs(新), common/mod.rs, gold-seed-idempotence.rs, retrieval-gold-copy.rs, retrieval-gold-live.rs}`、本报告。`crates/store/`、`crates/memory/`、`crates/graph/`、`crates/daemon/`、`panel/` **一行未改**。
- **活库纪律**：本单全程未打开活库（`~/.ruagent`），只是没碰它；唯一接触真实数据的是 `%TEMP%\ia-live`（t7 的副本）与 scratch 根；pid 79984 未启停。
- **未测（不静默跳过）**：
  1. 活库守卫的 **panic 分支**未对真活库根执行（有意），只单测判决与放行侧；
  2. **跨 crate 的消费面漂移钉**（H-1 侧）不在本单，归 integ/t19；
  3. `crates/store` 里同族的两条 `NOT MEASURED` 测试未改（不在 inScope），见 §4 末；
  4. `judged_by='human'` 在活库仍 **0 行** —— 本单只保证「一旦有人类标签，它不会被覆盖、也不会被误判成失败」，**不改变** C3 的数据前置（t7 起按 deferred 处理）。
