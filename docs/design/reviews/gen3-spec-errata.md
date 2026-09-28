# t85 · 规格坐标漂移：锚词检查器 + 一次性勘误（报告）

**对象**：四份规格 `docs/design/reviews/gen2-{recall,memory,graph,wiki}-spec.md` + 新增检查器 `scripts/spec-anchors.ps1`（15,418 B / sha256 `046B699F2B`）。
**范围纪律**：`gen2-integration-contract.md`（归 t64）与各 impl/review 报告**一行未改**；`panel/`、`crates/`、`.github/` 未改。**只读活库；未启停 pid 79984；未读写 ~/.ruagent；临时文件已自清**。
**我本轮改动的文件（git status 逐条核对）**：`scripts/spec-anchors.ps1`（新增）· `gen2-recall-spec.md` · `gen2-memory-spec.md` · `gen2-graph-spec.md` · `gen2-wiki-spec.md` 与**本报告**。

---

## 0 读数（同一检查器、同一窗口：改前 → 改后）

```
cd <repo>;  pwsh -File scripts/spec-anchors.ps1      # 或 powershell -NoProfile -Command "& scripts/spec-anchors.ps1"
改前：  spec-anchors: refs=158  pass=5   drift=20  suspect=43  unverified=133  unresolved=9   → exit 1
改后：  spec-anchors: refs=153  pass=20  drift=0   suspect=42  unverified=133  unresolved=9   → exit 0
负控：  副本里把 `wiki.rs:877` 改成 `wiki.rs:100` ⇒ DRIFT 1 （'regenerate_index' lives at :877）→ exit 1
```
* 「refs 158→153」不是引用变少了（原始 `….rs:NNN` 计数改前改后**逐文件相同**：58/110/17/26）：检查器按行去重，而 `gen2-memory-spec.md:251` 一行上的 3 条引用被勘误成同一个坐标 `api.rs:3617` ⇒ 去重后从 3 变 1。
* 负控用的是 `-Spec <副本>`，**仓库文件未被触碰**（副本在 `%TEMP%`，收尾已删）。

---

## 1 检查器 `scripts/spec-anchors.ps1`（可重复运行 + 可红）

**抽取**：`crates/….rs:NNN`（含 `:N-M` 范围，只判**起始**行）、同一行上继承的裸 `file.rs:NNN`（basename 唯一才解析；**多义 ⇒ `UNRESOLVED`，绝不猜**）。
**锚词**：从**引用句**里取（先双引号字面量、再反引号跨度、再普通标识符），只保留**代码形状**的 token（`snake_case`/`CONST_CASE`/`CamelCase`，≥4 字符），并排除语言关键字/标准库方法/产品名/通用词（`LIMIT`、`OFF`、`debug_assert`、`to_lowercase`、`knowledge`、`store`…）。
**判据（三层，故意不对称）**：
| 层 | 条件 | 行为 |
| --- | --- | --- |
| **DRIFT** | 引用句反引号点名了**该文件定义**的符号（`fn/struct/enum/const/…`），而所示行号 **±3 行内没有它** | **exit 1**（唯一会红的层） |
| **SUSPECT** | 只有「使用点」锚词（如 `SearchHit`、`semantic_score`）不在窗口内 | 打印，登记，**不红** |
| **UNVERIFIED / UNRESOLVED** | 引用句里没有可判别的符号 / basename 多义或文件不存在 | 打印 + 计数，**不红** |

**为什么必须这样分层（这是本单最贵的读数）**：我按 t92 的形状先写了「引用句里的任一 token 不在该行 ⇒ 红」的天真版本，结果第一次跑出 **122 处「漂移」**；收紧到「代码形状 + 稀有」后仍有 **97**、再到「反引号 + 稀有」**60**；最后加「**必须是该文件定义的符号**」才落到 **20** —— 而其中还混着被引号字面量（`"namespace"`、`{stage:?}`）与测试函数名（`_000`）误判的条目。**122 → 20 的差不是发现变少，是误报变少**：一条会误报的门禁比没有门禁更坏（t92 排序表的头号风险）。因此我把「不是定义」的那部分降为 SUSPECT/UNVERIFIED 并**逐条打印**（第 11 条的「空跑不能算过」在这里的形态是：**跳过必须可见**）。
**已知局限（未覆盖）**：判据只覆盖「引用句反引号点名的**定义符号**」这一类；对**使用点**、**区间引用**、**概念引用**、**测试名引用**都不能机械判定 ⇒ 42 条 SUSPECT + 133 条 UNVERIFIED 仍需人工（t80 的审计是这类工作的起点，不是替代）。

---

## 2 一次性勘误（34 处坐标 + 2 处语义）

* **34 处坐标全部改正**（`t85 errata: applied=34 missed=0`）：20 处来自检查器 DRIFT 层（每个都带「该文件定义的符号在 :M」的建议），14 处来自 t80 已复核的 D-2 清单（`store.rs:610→165`、`api.rs:3003→2973-2985`、`wiki.rs:1482→3186`、`memembed.rs:310→581`、`api.rs:2466-2700→2889`、`knowledge_api.rs:478→480`、`runs.rs:1270-1279→1278` 等）。
* **旧值保留**：每条**就地**标注 `（t85 勘误：原 <旧值>；锚 <符号>）`，并在每份规格文末追加 `## t85 坐标勘误（2026-09-29）` 段，用表格**逐字**列出旧引用 → 新引用 → 锚词/依据；同时列出该文件里**未收口**的 SUSPECT/UNVERIFIED 清单与 owner。
* 例（`gen2-wiki-spec.md:110`）：`` `WikiLinks{broken, orphans}`（`wiki.rs:2666（t85 勘误：原 1283；锚 WikiLinks 定义）`） `` —— 定义行 `:2666` 由检查器给出并在改后重跑通过。
* **两处语义勘误（就地 + 旧文字逐字引用）**：
  * **D-3**（`gen2-graph-spec.md:165`）：「`tag_rank` 对**未知** tag 返回 **5**」→ **6**（今天 `inject.rs:326 fn tag_rank`、`:334 _ => 6`；契约 §3.2 的读数也是 6）。方向不变（未登记 tag **最后**丢弃），但按 5 推 `graph` 的插入位置会错一格。
  * **D-4**（`gen2-graph-spec.md:154/157`）：G9 的 metric/复现命令**落后于迁移 0024**。`0024_distill_attempts.sql` 用 `CREATE TABLE distill_log_attempts` → `ALTER TABLE … RENAME TO distill_log` → 两个索引把 `distill_log` 重建为「**一次尝试一行**」⇒ ① 「库行数 ÷ 日志尝试数」不再度量 session 级截断；② 旧 baseline「库 1 行 / 日志 22 次（差值 21）」**只在 0024 之前**可复现；③ 0024 之后的正确形式是「行数 == 尝试数（差值 0）」+ `status` 分布 + `failure_reason`。**判据本身不变**，变的是它所依据的数据模型。

---

## 3 交回给 captain 的 finding（**本单未改**）

1. **契约 §3.1 的落地表止于 `0023`** —— 没有 `0024_distill_attempts.sql` 的形状变更行（D-4 的另一半）。owner：`gen2-integration-contract.md`（t64）。
2. **9 条引用**在四份规格里**无法解析**：`lib.rs:54-67` / `lib.rs:309` / `lib.rs:36`（basename 匹配 11 个文件 ⇒ 规格必须写 crate 名）、`chat.rs:550/560-569/569/581`、`chat.rs:521-583`（匹配 2 个文件）、**`output.rs:49`（全仓没有这个文件）**。owner：各规格属主（建议一律写全 `crates/…/x.rs`）。
3. **42 条 SUSPECT**：锚词是「使用点」而非定义（例 `store.rs:614 → ann_ids 在 :943`、`wiki.rs:525 → regenerate_index 定义在 :877` 已修，但 `api.rs:2967 → KeywordStage :3528`、`inject.rs:144-147 → updated_at/recall_log` 等仍需人工读一次）。清单已逐份写进各规格的 t85 段。
4. **133 条 UNVERIFIED**：引用句没有可判别的符号（多数是区间引用或概念引用）。**这不是「已核对」**，只是机械层判不了 —— 建议下一轮抽查「引用句 first-backtick 符号」是否落在该区间内。

---

## 4 不覆盖什么（§6.0 第 19 条）

* 检查器**只判坐标**：判据的**语义/阈值**是否仍成立，它一概不知（D-3/D-4 是我**读出**来的，不是它报出来的）。
* 它**不改任何阈值/判据文字**（本单也没有）；除 D-3/D-4 外，本单**只改坐标与引用形式**。
* 它**不覆盖 `gen2-integration-contract.md`**（80 处引用里契约占 40 —— 归 t64）、不覆盖 impl/review 报告、不覆盖 `panel/`、不覆盖代码。
* 它**不判「区间引用」的第二个数字**（`:N-M` 只判 N）。
* 它**不是 CI 门禁**（`scripts/` 下可运行，但本单未接进 `.github/workflows/` —— 接线是另一件事；判据：接进去时应带上「改坏 ⇒ 红」的负控）。
* **我是本单作者** ⇒ 不要把 t85 的评审单派给我。

---

## 5 复现清单

```powershell
cd C:\Users\19410\Documents\ai\ruagent
pwsh -File scripts/spec-anchors.ps1                 # drift=0 → exit 0（最终字节）
pwsh -File scripts/spec-anchors.ps1 -Suggest        # 只打印 旧→新 映射候选（勘误用）
pwsh -File scripts/spec-anchors.ps1 -Window 0       # 更严：要求锚词就在该行
Copy-Item docs/design/reviews/gen2-wiki-spec.md $env:TEMP\neg.md
(Get-Content $env:TEMP\neg.md -Raw).Replace('wiki.rs:877（t85 勘误：原 525；锚 regenerate_index 定义）','wiki.rs:100') | Set-Content $env:TEMP\neg.md
pwsh -File scripts/spec-anchors.ps1 -Spec $env:TEMP\neg.md   # 负控：DRIFT → exit 1
Remove-Item $env:TEMP\neg.md -Force
```
