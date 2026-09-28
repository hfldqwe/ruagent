# RV-D round 2 评审（t35）：t34 的六条修缮逐条判决 + C 节重判

> **结论：`pass`**。t34 把 RV-D 的 **6 条 finding 全部关闭**（1 high + 5 low），而且**每一条我都在当前字节上自己复核过**：RV-D-1 的四件（读记录值 / 诚实改名 / 第三态 / 污染标注）在代码面与 black-box 用例上都成立，**写侧准入逻辑一字未改**（负例族仍被拦住）。C 节里 **I-D 自有的 G1–G7、G9 现在全部有独立可复现读数**；**G8** 仍按 owner 分工判 `not_measured`（I-D 的生产侧数据已就绪，消费端在 I-B + I-INT/t19）—— **excused**，其终判在 t20/t21。
> **契约命令（我自己的运行）**：`test -p ruagent-mock-agent --test wiki_pipeline` = **15 passed / 0 failed / 0 ignored**（exit 0）。
> **评审员**：review（独立评审员，**不是** I-D 的作者；`crates/` 与 `panel/` 一行未改，本单只写本文件）。
> **被评审对象**：t34 报告（重写的 `docs/design/reviews/gen2-wiki-impl.md` §5bis/§8.3，502 行）与 `crates/daemon/src/wiki.rs`、`crates/mock-agent/tests/wiki_pipeline.rs`、`docs/design/reviews/gen2-wiki-spec.md` 的修订记录。
> **字节绑定**：`crates/daemon/src/wiki.rs`（`has_anchors` L3007 · `anchored_sections` L3014 · `recorded_coverage` L3780 · `build_id IS NOT NULL` L3707/3740）· `crates/mock-agent/tests/wiki_pipeline.rs`（不变量用例 L1165 / 无 wiki_pages 行 L1131 / stale 行 L963）· 规格修订记录 L14–L19 与 D.5 文档块 L267/L287/L419–L444。
> **时间窗**：2026-09-28T04:1x → 04:4x +08:00。
> **纪律**：每一次 cargo 都走 `scripts/cargo-team.ps1`（单一飞行锁 / 共享 target，**未**自设 `CARGO_TARGET_DIR`）。真守护进程 **pid 79984 未启停**；`~/.ruagent` 只读；**未在真库上跑 wiki build**；**未用** `GET /api/v1/recall`。

---

## 0 一句话

RV-D-1 这条 high 的实质是「一个**构造性满分**被当成读数」：`cite_coverage_of` 的分子与分母同源 ⇒ 只能是 0/1，而注释还自称「构建期算的」。t34 把它换成**唯一取数口** `recorded_coverage(record, freshness)`（只在页面仍 fresh 时给数）、把恒等式函数**删掉**、并给「没有 `wiki_pages` 行」与「失效扫写的行」两条路都留了 `unknown`，还加了一条**两侧都断言**的 black-box 不变量。我在代码面确认没有留下任何重算或数值兜底。

---

## 1 我自己的读数

### 1.1 契约命令与负例（我的运行）

```
scripts/cargo-team.ps1 test -p ruagent-mock-agent --test wiki_pipeline   → exit 0
  test result: ok. 15 passed; 0 failed; 0 ignored; 0 measured; finished in 19.84s
  逐条 OK 里包含（负例族仍在拦住）：
    the_citation_gate_fails_a_page_whose_claims_cannot_be_traced
    a_page_with_nothing_to_cite_fails_by_its_own_name
    an_unresolvable_anchor_cannot_keep_reporting_full_coverage   ← t34 新增
    an_unanswerable_freshness_reading_is_unknown_never_fresh
    dry_run_has_one_vocabulary_and_rejects_the_combined_request
    the_two_graph_endpoints_cannot_disagree
```

### 1.2 RV-D-1 的四件（我逐件复核）

| 件 | requiredFix（t18/t35 契约） | 我在当前字节上读到的 |
| --- | --- | --- |
| **(a) 读记录值** | 四个读面改取构建期记录值 | **单源**：`wiki.rs:3780 pub fn recorded_coverage(record, f) -> Option<f32>`；调用点只有 `:2842`（`/pages`）与 `:2958`（`lead_for`）。同步面 `:2924`（`lead_from`）与 `:3204`（hints JSON / 实体软链卡）报 `cite_coverage: None` / `null` **+ `has_anchors`**（页级事实，不许猜） |
| **(b) 诚实改名** | `cite_coverage_of` 改名/删除 | **删除**：全 `crates/**` 搜 `cite_coverage_of` 只剩一条解释性注释（`wiki.rs:2998`）；新函数 `has_anchors`(:3007) / `anchored_sections`(:3014) 语义与命名一致（「页里有无锚」≠「锚能不能解析」） |
| **(c) 第三态** | 无 `wiki_pages` 行 ⇒ `None/unknown`，**不许报 1.0**；失效扫写的行（DEFAULT 0.0）也不许读成 0 | SQL 面 `:3707/:3740 CASE WHEN build_id IS NOT NULL THEN cite_coverage END`（**扫写行恒为 NULL**）；black-box 用例 `wiki_pipeline.rs:1131-1135`「no `wiki_pages` row ⇒ `cite_coverage` is null」、`:963-965`「stale 页不引用记录值 ⇒ null」；单测 `recorded_coverage_is_quoted_only_on_a_fresh_page`(:4273) |
| **(d) 污染标注** | G1 的标定读数与 G8 的 `WikiLead.cite_coverage` 标为受污染、修好后重读 | impl 报告 L16 明写「这条的达标证据原本受 RV-D-1 污染，修好后必须重读」；规格 G1 行(L194)已换成新判据（**只能读构建期记录值 + 只在 fresh 时 + 禁止重算**） |

**不变量（两侧都断言）**：`wiki_pipeline.rs:1165 an_unresolvable_anchor_cannot_keep_reporting_full_coverage` —— `:1192` 先确认首建页 `cite_coverage == 1.0`，随后把一条锚变成解析不了并断言 `:1220 coverage.is_none()`、`:1225 !(coverage == Some(1.0) && !uncited.is_empty())`、`:1243-1252` 「满分 ⇒ `uncited` 必须为空」且「`coverage` 只在 fresh 页上报」。**这正是否证旧实现的那一条**：旧 `cite_coverage_of` 在「有锚但解析不了」的页上恒返回 1.0，而同一响应里 `uncited_sections` 非空 ⇒ `:1225` 的断言必然失败。

**无兜底残留**：全 `crates/daemon/src/*.rs` + 契约测试搜 `cite_coverage_of` / `cite_coverage.unwrap_or` / `cite_coverage: 1.0` / `cite_coverage: 0.0` ⇒ **0 条**（只有那条注释）。

### 1.3 RV-D-2 … RV-D-6（我复核的落点）

| # | round 1 的要求 | 落点（我自己看到的） |
| --- | --- | --- |
| **RV-D-2**(low 一致性裁决) | 保留 `SourceMissing` 实现、改规格措辞、登记 VD T8-A/T8-B | 规格修订记录 L16 + D.5 文档块 `:419-427`：`None` 的触发条件写成「KB 整体无可用 documents row ⇒ `unknown`；页点名来源消失 ⇒ `Some(true)` + `SourceMissing`/`SourceHashDrift`」，并引 `VD T8-A`/`VD T8-B` 为证据 |
| **RV-D-3**(low) | `stale=true` 时读者要知道**为什么** | `PageFreshness.stale_reasons: Vec<String>`(`:287`) + `unknown_cause: Option<String>`(`:289`/`:438`)，并在 `:464` 给出 `unknown_cause` 的两个取值（`unknown_kb` / `unknown_no_meta`） |
| **RV-D-4**(low) | `wiki_citations` 登记本代未使用 + 反查 deferred + 真实路径 | 规格修订记录 L18 + E.2 DDL-2 段（登记未使用、反查 deferred、现状失效传播 = 重扫页文件） |
| **RV-D-5**(low) | `frozen_by` 与 G6 标定措辞对齐 | 规格修订记录 L17 + `:297` 注释：**`frozen_by` 仅 pin 路径非空**（手改冻结那条路看 `wiki_corrections`/`error`） |
| **RV-D-6**(low) | 计数订正 | impl §8.3 L369 + L15 + L282：**改前 4 → t10 交付 14（新增 10）→ t34 修复 15（新增 1）**，三个数并写；我的运行读数 = **15** |

### 1.4 消费者面（我自己扫的）

* `panel/**`（递归 `*.ts,*.tsx`）搜 `cite_coverage` = **0 命中**；`crates/daemon/src/api.rs` 与 `crates/mcp/src/lib.rs` 也是 **0 命中**（字段由 `/api/v1/knowledge/wiki/pages` 直接序列化 `WikiPageInfo`）。
* `WikiLead.cite_coverage` 由 `f32` → `Option<f32>` 是**破坏性变更**，已在规格修订记录 L19 与 impl `:306` 登记；**跨区适配已闭合**：mem-core 的 `inject::WikiLeadMeta.cite_coverage` 同步成 `Option<f32>` 且渲染 `coverage=unknown`（我在 t32 读到 `edited=None coverage=None -> … coverage=unknown`，并断言 `coverage=0.00/1.00` 不出现）；graph 报的 `memembed.rs:529` 也已改用 `merge_decision_audited`。
* ⇒ 仓内没有会被这个形状变更编译不过或静默读错的消费者；外部消费者看到的是 `number | null`（D.7 有注记）。

---

## 2 C 节逐条重判

| G | 规格 target（压缩） | 我的独立读数 | 判决 |
| --- | --- | --- | --- |
| **G1** | 无锚 ⇒ 页 `failed`；`written` 页 coverage = 1.00（**且读数只能取构建期记录值、只在 fresh 时**） | 准入：负例族逐条 `pages_written=0 / pages_failed=1 / status="failed"`（我的契约运行 §1.1）· 读数：`recorded_coverage` 单源 + 三态 + 不变量两侧断言（§1.2）；**旧实现的恒等式会在 `:1225` 上直接失败** | **达成**（RV-D-1 关闭） |
| **G2** | 三条独立比率；负例逐族点名 kind | 六族 + 归因用例仍全绿（`the_citation_gate_fails_…` / `a_page_with_nothing_to_cite_fails_by_its_own_name`） | **达成** |
| **G3** | `stale_since` 只写一次 + 三面一致 + 第三态 `unknown` | `an_unanswerable_freshness_reading_is_unknown_never_fresh` **ok**；口径按 RV-D-2 对齐（KB 整体空 ⇒ unknown；来源消失 ⇒ `stale{SourceMissing}`）；`stale_reasons`/`unknown_cause` 给「为什么」 | **达成** |
| **G4** | 计划说 `keep` 也被改写重建 | `a_stale_page_is_repaired_even_when_the_plan_says_keep` **ok**；stale 页的记录覆盖率按 RV-D-1 不再报出（`:963-965`） | **达成** |
| **G5** | 图两端口径逐位一致 + 归属 + 持久化 + 自链 | `the_two_graph_endpoints_cannot_disagree` **ok** | **达成** |
| **G6** | 被 skip 的页必有 `wiki_corrections` + `/pages` 给 `frozen_by` | pin/hand-edit 用例 **ok**；标定读数按 RV-D-5 收窄为「**仅 pin 路径非空**」 | **达成** |
| **G7** | plan-only 一个词汇 + 组合请求 400 | `dry_run_has_one_vocabulary_and_rejects_the_combined_request` **ok** | **达成** |
| **G8** | `wiki_block_lead_marks` 0/2 → 2/2 | **not_measured**：I-D 侧数据齐（`WikiLead`/`lead_for`/`lead_from`/`recall_stubs` 带 `stale`/`stale_since`/`edited`/`has_anchors`/`anchored_sections`/`cite_coverage`），消费端在 `crates/memory`(I-B) + `chat.rs`/`runs.rs`(I-INT) | **not_measured（owner 分工，excused）**，终判 t20/t21 |
| **G9** | slug churn 0（改名走 update + aliases） | `a_rename_moves_the_topic_instead_of_destroying_it` **ok** | **达成** |
| E10 | `wiki_page_hashes` 泄漏自愈 | `a_page_hash_with_no_page_is_swept_by_the_next_build` **ok** | **达成** |

---

## 3 findings

### RV-D2-1（low · **下一代假达标的入口**，owner = integ/t19）G8 的接线必须走 async `lead_for`，若用同步 `lead_from` 则 `coverage=` 恒为 `unknown`
* **事实**：`lead_from`（同步 / HTTP stub）**看不到 DB** ⇒ 按 RV-D-1 它只能报 `cite_coverage: None` + `has_anchors`（`wiki.rs:2924`）；只有 async `lead_for(kb, &RecordedPage, slug)` 走 `recorded_coverage`（`:2958`）才可能给出数字。规格 D.5 追加块**已经写明**「`lead` 取自 `wiki::lead_for`（三参 async，三态保真；同步档 `lead_from` 的 `edited`/`stale_since` 会是 `None`）」。
* **风险**：若 t19 为了省一次 async/DB 读而接 `lead_from`，注入的 `<wiki>` 块会**永远**渲染 `coverage=unknown` —— 而 G8 的判据是「块行含 `stale=`/`coverage=`/`anchors=`/`hint`」，**含 `coverage=unknown` 也算含** ⇒ 一个恒为 unknown 的面会**看起来达标**。这与本代已被抓住的「构造性满分」同族，只是这次是**构造性 unknown**。
* **requiredFix（t19/t20 的验收条款，不需 I-D 再改代码）**：t19 接 `lead_for`；t20 的 G8 读数必须能区分「有数字的 coverage」与「恒 unknown」——建议至少一条断言要求注入块里 **≥1 条 `coverage=<数字或 null 且同页有 build 记录>`**，或在 D.5 里显式允许「本代恒 unknown 且已登记」。

### 观察（不进 findings）
* **O-1** 破坏性变更的**消费面已闭合**（§1.4）：仓内零消费者、mem-core 已适配、graph 的类型涟漪已修；外部消费者看到 `number | null`，面板今天不读该字段（D.7 有注记，impl `:294` 建议面板读 `freshness` 而不是用 `stale` 推断 `unknown`）。
* **O-2** RV-D-1 的 black-box 不变量是**两侧都断言**的（`:1220`/`:1225` 与 `:1243-1252`），不是只断言「不许并存」——这正是我在 t18 要的「可证伪」形状；旧实现在同一用例上会红。
* **O-3** t34 报告记录的门禁**中间态**（撞 mem-core 的 `memembed.rs:342`、integ 的 `injection_e2e.rs:685`）按纪律只报坐标、另取收窄读数，最终取得完整绿；这与我在 t18/t28 的处置同形（并发波次里的读数绑定）。

---

## 4 关闭表：RV-D 的六条 finding

| # | round 1 的读数 | 我本轮的复核 | 判 |
| --- | --- | --- | --- |
| **RV-D-1**(high) `cite_coverage` 构造性满分 | `cite_coverage_of` 分子≡分母 ⇒ 只 0/1；四读面全用它；构建期真值写库后零读回；无 `wiki_pages` 行也报 1.0 | 单源 `recorded_coverage`(:3780) + 两处引用(2842/2958) + 同步面 null+has_anchors；`cite_coverage_of` 已删；`build_id IS NOT NULL` 守卫；black-box 不变量两侧断言 | **关闭** |
| **RV-D-2**(low 一致性裁决) | 规格 G3/D.5 的第三态措辞与实现不符 | 修订记录 L16 + D.5 文档块 L419-427 + VD T8-A/T8-B 登记 | **关闭** |
| **RV-D-3**(low) `stale=true` 但 `stale_sources=[]` | 读者不知为何 stale | `stale_reasons` + `unknown_cause`（L287/289/438/464） | **关闭** |
| **RV-D-4**(low) `wiki_citations` 无读写者 | 反查索引实践上不存在 | 修订记录 L18：登记未使用 + 反查 deferred + 真实路径（重扫页文件） | **关闭** |
| **RV-D-5**(low) 手改冻结 `frozen_by=null` | 与 G6 标定读数「非空」不符 | 修订记录 L17 + `:297`「仅 pin 路径非空」 | **关闭** |
| **RV-D-6**(low) 摘要 `13 passed` 是旧数 | 实测 14 | 4 / 14 / 15 三数并写；我的运行 = 15 | **关闭** |

---

## 5 未测 / 不判定（owner 具名，**不据此判 I-D 失败**）

| # | 项 | owner | 为什么只能在它之后测 |
| --- | --- | --- | --- |
| U-1 | G8 注入块 2/2（含 `coverage=` 是否真的有数字） | **INT/t19**（构造点）+ I-B（渲染） | 生产注入点在 `chat.rs`/`runs.rs`；任务禁止启停 pid 79984、禁止用 `/api/v1/recall` ⇒ 见 RV-D2-1 |
| U-2 | 真库（活库）上的 wiki 读数 | **运维/t19** | 纪律：不许在真库跑 wiki build、不许写 `~/.ruagent` |
| U-3 | 面板对 `cite_coverage: number \| null` 的显示 | **I-INT/panel** | 本单 inScope 外；仓内今天零消费者（§1.4） |
| U-4 | 旧代码侧的两棵树反代 | **未测** | t10 与 V-D 已给（`git archive HEAD` + 独立 target，6/6 落地）；本轮我给的独立证据是**不变量在旧实现上必然红**的逻辑 + 代码面「零兜底」 |

---

## 6 复现命令（我实际跑的）

```powershell
$env:PATH="$env:USERPROFILE\.protoc\bin;$env:PATH"    # 全队包装脚本；未自设 CARGO_TARGET_DIR
scripts/cargo-team.ps1 test -p ruagent-mock-agent --test wiki_pipeline    # 15 passed / 0 failed / exit 0
git status --porcelain -- crates panel   # 我唯一新增 docs/design/reviews/gen2-wiki-review-r2.md
# code 面判据（grep，不经作者转述）：
#   wiki.rs:3007 has_anchors · :3014 anchored_sections · :3780 recorded_coverage · :3707/3740 build_id IS NOT NULL
#   wiki.rs:2842/2958 两处 recorded_coverage 调用 · :2924/:3204 同步面 null+has_anchors · :2998 只剩注释
#   wiki_pipeline.rs:963/1131/1165/1192/1220/1225/1243/1250 不变量两侧断言
#   panel/** 与 api.rs/mcp 搜 cite_coverage = 0 命中
```

---

## 7 纪律回执

* **只写本单 inScope**：本文件 `docs/design/reviews/gen2-wiki-review-r2.md`。`crates/` 与 `panel/` **一行未改**。
* **读数三件套**：对象集（当前树 `wiki.rs` / `wiki_pipeline.rs` / 规格修订记录）· 采样面（契约命令 15 条用例逐条 + code 面 grep + 不变量两侧断言）· 可证伪判据（旧实现会在 `:1225` 必然红；`build_id IS NOT NULL` 让扫写行的 0.0 不可被读成 0；`cite_coverage_of`/`unwrap_or` 零命中）。
* **期望值只写一处**（规格 §C 与本报告 §2），判据从代码与测试取，**没有复制常量**。
* **未测一律写明**（§5，4 项）；G8 按 owner 分工判 not_measured，**不据此判 I-D 失败**，其终判在 t20/t21。
* **真守护进程 pid 79984 未启停**；`~/.ruagent` 只读；**未在真库上跑 wiki build**；**未用** `GET /api/v1/recall`。
* **并发声明**：t34 期间与本期都存在并发波次（mem-core 的 `memembed.rs` 适配、integ 的 `injection_e2e.rs`、graph 的类型涟漪）；我只在读数受影响时报「哪个文件、什么错」，不替同伴改。
