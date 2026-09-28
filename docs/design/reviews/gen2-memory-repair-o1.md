# R-O1 修复报告：`WikiLeadMeta.edited` 三态保真（t24，mem-core）

- 任务：t24 R-O1（repair）—— 修掉 O-1：`wiki::WikiLead.edited: Option<bool>`（三态）映射到 `inject::WikiLeadMeta.edited: bool`（二态）时，`None`（该面看不到 DB）被压成 `false`（一个确定的「未手改」）
- attempt：`ed4ca903-1923-48e3-b418-ac90ae44f5ac`
- 改动面（inScope）：`crates/memory/src/inject.rs`（字段类型 + 渲染分支 + 一条最小测试）+ 本报告。**另修了一处 inScope 之外、但必须修的文件**：`crates/daemon/src/memembed.rs`（我 t8 加的三个适配 helper 引用了 I-A 重构中已删除的 `ruagent_knowledge::RelevanceScore` ⇒ `check -p ruagent-daemon` 曾 exit=101 并连累 wiki 的 t10）；细节与理由见 §6/§9，已报 captain 备登。`crates/daemon/src/chat.rs` / `runs.rs` 一行未动（接线归 t19）、`crates/store`、`crates/knowledge`、`crates/graph`、`panel` 未动
- 裁决：captain 选**方案 ①**（字段改保真三态）；② （再加 `edited_unknown: bool`）会让同一事实有两个真相源，③（整条 lead 不注入）是信息损失
- 时间：2026-09-28 00:2x–01:0x +08:00；环境：**全队包装脚本** `scripts/cargo-team.ps1`（共享 target + 单一编译，见 §9；captain 00:4x 的资源纪律）；真守护进程 pid 79984 只读（未启停、未写 `~/.ruagent`）

---

## 1 结论（一句话）

`edited` 现在是**保真三态** `Option<bool>`，`None` 渲染为 `edited=unknown`（**不是**「不出现该标记」，理由见 §3），并且在**改动前的 `bool` 形状上这条判据是红的**：旧形状下 `None` 与 `Some(false)` 的渲染**逐字节相同**（都 `edited=false`），两个反例断言 2/2 失败（§4）。

## 2 改前 → 改后：签名对照

| 项 | 改前（O-1 缺陷形状） | 改后（本单） |
| --- | --- | --- |
| 字段 | `pub struct WikiLeadMeta { … pub edited: bool, … }` | `pub struct WikiLeadMeta { … pub edited: Option<bool>, … }` |
| 三态表达力 | `true` / `false` —— 「看不到 DB」只能被塞进二者之一 | `Some(true)` / `Some(false)` / **`None`（不可判定）** |
| 渲染分支 | `format!("… edited={}", lead.edited)` | `match lead.edited { Some(true) => "true", Some(false) => "false", None => "unknown" }` |
| `<wiki>` 行（`None`） | `… edited=false` ← 把「不知道」说成「没有编辑过」 | `… edited=unknown` |
| 与 `stale` 的关系 | `stale: Option<bool>` 已三态（**同一结构体里两个同族字段，一个三态一个二态**） | 两个字段都是 `Option<bool>`，渲染规则一致 |
| `RetrievalHit` / `knowledge_items` / `render_context` / 预算 / 截断词表 | 未变 | **未变**（golden 仍绿，§5） |

字段文档里写明了更正来源（t8 + integ 的 O-1 / DEP-INT-8）与判据（`None` 不渲染 `edited=false`，有测试钉住），所以下一个读者不必从 git 历史里找这条决定。

## 3 选了哪一种，为什么（判据：读者不能把「不知道」读成「没有编辑过」）

- 选：**渲染为 `edited=unknown`**（与 `stale=unknown` 同一词表）。
- 不选「不出现该标记」：`<wiki>` 行的**每一行必须能在不看正文的情况下自证**（R-D D.7 的判据）。若 `None` 时干脆不写 `edited=`，读者会面对**三种可能**：`edited=true`、`edited=false`、以及「这行没写 —— 是旧版本？是忘了渲染？还是不可判定？」。省略把三态压回二态**再加一个不可区分的缺省**，比 `bool` 更差。
- 不选方案 ②（`edited_unknown: bool`）：与 `edited` 是两个字段表达同一事实，二者可以同时自相矛盾（`edited=true, edited_unknown=true`），本代已为同类问题定过型（R-B 的 `ExternalStatus{Readout|NotAvailable|Unknown}`、closure §6.4 的「NULL ≠ user」）——**一个只能取二值的字段承载三态事实，缺陷在字段类型本身**。
- 词表：`unknown` 与 `stale` 的第三态用同一个词，读者只需学一次；`Some(false)`/`Some(true)` 仍是 `false`/`true`，对既有读者零变化。

## 4 三态三读数 + 改前红读数

### 4.1 改后：三态各一（当前树）

命令（**全队包装脚本**，见 §9）：`scripts/cargo-team.ps1 test -p ruagent-memory -Nocapture`

| 生产者状态 | 语义 | 渲染（原文） | 判据 |
| --- | --- | --- | --- |
| `edited: None` | 这一面看不到 DB | `wiki/kubernetes-troubleshooting: kubernetes-troubleshooting — stale=unknown coverage=0.00 anchors=0 edited=unknown` | **不含 `edited=false`** ✅ |
| `edited: Some(false)` | 未手改 | `… stale=unknown coverage=0.00 anchors=0 edited=false` | 含 `edited=false` ✅ |
| `edited: Some(true)` | 手改过 | `… stale=unknown coverage=0.00 anchors=0 edited=true` | 含 `edited=true` ✅ |

同一测试还断言 `stale: None ⇒ stale=unknown`（三态在这一行上一致）。三条读数来自同一次运行的 stdout（`READING O-1 …`）。

### 4.2 改前：同一判据在旧 `bool` 形状上是红的

装置（不改同伴文件、跑完清理）：`git worktree add %TEMP%\ruagent-t24-before HEAD`（`HEAD = 0a39e5b8`，t8 之前没有本判据），把工作树的 `crates/memory/src/*.rs` 复制进去（让 crate 能编译），然后**只把 `inject.rs` 退回旧形状**（`pub edited: bool` + `format!("… edited={}", lead.edited)`），再放入一个**只使用旧形状就能编译**的反例测试 `crates/memory/tests/o1_before.rs`：

```rust
/// 旧 `bool` 字段唯一能做的映射：三态 → 二态
fn old_bool_field_map(edited: Option<bool>) -> bool { edited.unwrap_or(false) }

#[test] fn o1_unknown_edited_must_not_render_as_false() { /* edited: old_bool_field_map(None) */ }
#[test] fn o1_unknown_and_false_must_be_distinguishable() { /* assert_ne!(render(None), render(Some(false))) */ }
```

读数（`CARGO_TARGET_DIR=%TEMP%\ruagent-t24-before-tgt cargo test -p ruagent-memory --test o1_before`，**2 failed / 0 passed**）：

```
BEFORE-SHAPE edited=None   -> "<wiki>\nwiki/kubernetes-troubleshooting: kubernetes-troubleshooting — stale=false coverage=1.00 anchors=1 edited=false\n …"
BEFORE-SHAPE edited=Some(false) -> "<wiki>\nwiki/kubernetes-troubleshooting: kubernetes-troubleshooting — stale=false coverage=1.00 anchors=1 edited=false\n …"
  left:  "… edited=false …"      ← assert_ne! 的两个边逐字节相同
  right: "… edited=false …"
failures:
    o1_unknown_and_false_must_be_distinguishable
    o1_unknown_edited_must_not_render_as_false
test result: FAILED. 0 passed; 2 failed
```

⇒ 旧形状下「不可判定」与「未手改」**在契约层不可区分**，这就是 O-1；新形状下两者分别为 `unknown` / `false`。

### 4.3 我自己在这次取证里踩到并纠正的一个假读数（写进报告，别让它变成别人的证据）

第一次跑验证时我把**两个不同源码树共用同一个 `CARGO_TARGET_DIR`**（`%TEMP%\ruagent-repair-o1`：先在 HEAD 导出树上跑反例，再在当前树上跑 `cargo test`）。结果当前树的 4 条测试报 `no such table: memory_sources` / `no such column: access_count` —— 那是 **HEAD 树的 `ruagent-store` 产物（没有 0020 迁移）被复用到当前树**造成的，**不是**记忆 crate 的真实失败。换成干净的 target 目录后同一条命令 **54 passed / 0 failed**（§6）。
教训与「两个读数不一样时先列对象集与口径差，再谈时间差」同族，只是对象换成了**构建产物**：**不同源码树不许共用一个 target 目录**；反例树与主树的 target 目录必须分开（本报告的两条读数分别来自 `ruagent-t24-before-tgt` 与 `ruagent-repair-o1`）。

## 5 golden 未变证据（不许在修三态时改动无关渲染）

| 判据 | 读数 |
| --- | --- |
| 无 lead 的 wiki 行逐字不变 | `READING C6/R-D D.7 plain wiki line:` → `<wiki>\nwiki/kubernetes-troubleshooting: 当 Pod 出现 crash-loop 时 …\n</wiki>\n`（与 t8 落地时那一条断言完全相同，测试 `a_wiki_lead_renders_its_marks_and_a_plain_wiki_hit_does_not_change` 仍绿） |
| 带 lead 的既有断言（`coverage=1.00 anchors=2 edited=false` + hint + anchors 行） | 仍绿（该测试改用 `edited: Some(false)`，字节不变） |
| 记忆-only 路径（`render_injection` / `MemoryForInjection`） | **未触碰**；`confidence: None` 行为不变 |
| 预算 / 可见截断 / 丢块计数 / `never_exceeds_total` | 未触碰，仍绿 |

## 6 验收命令（t24 的三条，全部走全队包装脚本）

| 命令（`scripts/cargo-team.ps1`，见 §9） | 结果 |
| --- | --- |
| `test -p ruagent-memory -Nocapture` | **exit 0** · `test result: ok. 54 passed; 0 failed; 0 ignored`（t8 时 53 → 本单 +1 条三态判据）；三态读数见 §4.1 |
| `clippy -p ruagent-memory --all-targets -- -D warnings` | **exit 0** · `Finished dev profile`（0.39s） |
| `check -p ruagent-daemon --all-targets` | **exit 0** · `Finished dev profile`（8.92s） |

`check` 这一条**中途红过一次，是我的错并已修**：本单开始时我在 `memembed.rs` 写的适配器引用了 `ruagent_knowledge::RelevanceScore`。**我当时写「该类型已被删除」是不准确的，此处更正（原文不删）**：类型一直在 `crates/knowledge/src/store.rs:182`，我实际撞到的是**该类型的 crate-root re-export 在那一刻不在**——`crates/knowledge/src/lib.rs` 的 `pub use` 列表在我两次读取之间变化过（00:1x：只导出 `Knowledge/KnowledgeError/LegHit/SearchHit/SearchLegs`；01:0x：`RelevanceScore/RankedHit/SearchPage/ScoreKind/SearchEvidence/ResidualHit/…` 都在，见 `lib.rs:23-24`）。也就是说 **I-A 的那一面当时正在编辑中**（接口漂移），而我的文件恰好把「路径」当成了稳定事实 ⇒ `exit=101`，两个 `cannot find type RelevanceScore`（坐标 `memembed.rs:670/685`），并连带挡住 I-D 的 `test -p ruagent-mock-agent --test wiki_pipeline`（wiki 报给我的就是这条）。
**修法（不再跟着别人的编辑走）**：适配器只读**稳定面**（`ruagent_knowledge::{SearchHit, LegHit, SearchLegs, Knowledge}`），校准分数**按值接收**：
```rust
pub fn relevance_meta(value: f32, kind: &'static str, version: u32, query_background: Option<f32>) -> RelevanceMeta
pub fn enriched_hit(hit: &ruagent_knowledge::SearchHit, relevance: Option<RelevanceMeta>, lead: Option<WikiLeadMeta>) -> EnrichedHit
```
I-A 定稿后，调用点一行 `relevance_meta(r.value, r.kind.as_str(), r.version, Some(r.query_background))`，我这边不再改。修后 wiki 侧复核 `Finished dev profile`、`wiki_pipeline` 已能编译运行。**这条修法与「类型在不在」无关**：它的价值是让我的编译状态不再随别人的编辑波动（01:0x 复读：`RelevanceScore` 又可以从 crate root 引用了，而我的文件不需要因此再改一行）。

## 9 团队资源纪律（本单期间新增，我的两次踩坑与纠正）

captain 在 00:4x+08:00 实测到 5 个 cargo 并发、两个 rustc 同时编 `lancedb`、32.5 GB 内存只剩 3.77 GB，于是新增 `scripts/cargo-team.ps1`（单一编译 + 共享 target + CPU 0–11 + BelowNormal），并要求所有 cargo 调用走它。本单期间我遇到的两次与它有关的事：

1. **脚本 v1 参数绑定坏**（不是我该改的文件，报 captain）：`test`/`check`/`clippy` 落在位置 0（`$Jobs`）⇒ 文档头部三条示例全不可用；`-- --nocapture` 里裸 `--` 被 PowerShell 当空参数名。我按等价纪律手动跑（共享 target + jobs 4 + 亲和 0–11 + BelowNormal + 手动持 `ruagent-team-build.lock` 重试），并把根因与补丁建议报给 captain；captain 的 v3（不声明参数、自解析 `$args` + `-Nocapture`/`-TargetDir`/`-DryRun`）已修复，本报告 §4/§6 的读数全部来自 v3。
2. **我自己踩的跨树串扰**（§4.3）：两棵源码树共用 `CARGO_TARGET_DIR` ⇒ 当前树拿到 HEAD 树的 `ruagent-store` 产物，出现 4 条假的「no such table」失败。纪律：**对照树必须 `-TargetDir` 单独目录**，并且**不要把不同树的编译结果当成两棵树的行为差**。

## 10 接口漂移留痕：`ruagent_knowledge` 的校准相关分（captain 要求登记）

**事实（两条读数，各自带时刻；不要合并）**

| 时刻 | 读数 | 命令 |
| --- | --- | --- |
| 2026-09-28T00:1x+08:00 | `store.rs:182 pub struct RelevanceScore` / `:237 RankedHit` / `:245 pub relevance` / `:1036 search_page` / `:1149 residual_scan` **在**；`lib.rs` 的 `pub use` **不含**这些名字（只导出 `Knowledge/KnowledgeError/LegHit/SearchHit/SearchLegs`） | 全仓 grep + 读 `crates/knowledge/src/lib.rs` |
| 2026-09-28T01:0x+08:00 | `lib.rs:23-24` 已 `pub use` `RELEVANCE_VERSION, RankedHit, RelevanceScore, ResidualHit, ResidualOrigin, ResidualPage, SCORING_VERSION, ScoreKind, SearchEvidence, SearchHit, SearchLegs, SearchPage` | 同上（重读） |

**⇒ 结论（精确版）**：这不是「类型被删」，而是**同一面在两分钟内被重排过**（I-A 正在编辑 `crates/knowledge`）：类型始终在 `store.rs`，**crate-root 的导出名单**变过。对下游的含义是**真实的破坏性风险**：任何按路径引用 `ruagent_knowledge::RelevanceScore` 的文件，其编译状态在那段时间里跟着别人的编辑走 —— 我的 `crates/daemon/src/memembed.rs` 就是这么红的（`cannot find type`，并连带挡住 wiki 的 t10）。

**给 V-B / RV-B / t19 的建议（不是判据，是形状）**：跨区边界上引用的类型，要么由**生产方**提供一处稳定的 re-export 并把它写进冻结接口（R-A D.2/D.3 已经定义了这个形状），要么由**消费方**按值接收（本单采取的办法）。两者选一即可；两边都不做，就会出现「同一时间点两棵子树各自绿/红」的假读数。

**全仓引用清单（2026-09-28T01:0x+08:00 grep）** —— 除 knowledge 自身与文档外，**没有**别的代码引用这些名字：

| 位置 | 引用方式 | 状态 |
| --- | --- | --- |
| `crates/knowledge/src/store.rs` | 定义（`RelevanceScore`/`RankedHit`/`SearchPage`/`ScoreKind`/`relevance_from_distance`/`search_page`/`residual_scan`） | 正常 |
| `crates/knowledge/src/lib.rs:23-24` | re-export | 正常（00:1x 时不在，01:0x 已在） |
| `crates/knowledge/tests/retrieval-cjk.rs` · `retrieval-gold-live.rs` · `residual-scan.rs` · `retrieval-gold-copy.rs` | 用 `ScoreKind::*` / `search_page` / `residual_scan` | recall 自己的测试 |
| `crates/daemon/src/memembed.rs` | **只剩注释**（`:636/:645/:649/:682`）；代码路径已改为只收稳定面 + 按值接收 | 已修 |
| `crates/memory/src/inject.rs:376` | 注释里提到 `RelevanceScore`（说明本 crate 刻意不依赖它） | 正常（注释） |
| `docs/**` | 规格/契约文档（R-A D.2/D.3、集成契约 DEP-INT-3/8、本报告） | 正常 |

**未受影响**：`crates/memory/src/lifecycle.rs` 的 `residual_scan` 提及是 `NotAvailable` 的**说明文字**（不是代码依赖）；`forget_report` 仍按「本 crate 不依赖 knowledge」设计，等 INT 在 HTTP 面拼装（集成契约 U-6 已把统一入口判给 `GET /api/v1/forget-report`）。

## 7 交给 integ（t19）的确切形状与调用方式

**改动后的契约（`crates/memory/src/inject.rs`）**

```rust
pub struct WikiLeadMeta {
    pub slug: String,
    pub stale: Option<bool>,          // Some(true)=stale / Some(false)=fresh / None=unknown
    pub stale_since: Option<String>,
    pub edited: Option<bool>,         // Some(true)=手改 / Some(false)=未手改 / None=该面看不到 DB
    pub cite_coverage: f32,
    pub anchors: Vec<(String, i64)>,  // (document, chunk_id) —— 去 expand{chunk_id} 取证据
    pub hint: String,
}
```
渲染（`<wiki>` 行）：`stale=` 与 `edited=` 各取 `true|false|unknown`；`edited=None` **绝不**产出 `edited=false`。

**单一映射点（`crates/daemon/src/memembed.rs`，t8 已交付、integ 已核对）**

```rust
pub fn lead_meta(lead: &crate::wiki::WikiLead) -> ruagent_memory::inject::WikiLeadMeta
    // 三态原样透传；anchors: Citation{section,document,chunk_id,chunk_hash} → (document, chunk_id)
pub fn relevance_meta(r: &ruagent_knowledge::RelevanceScore) -> ruagent_memory::inject::RelevanceMeta
    // kind 用 ScoreKind::as_str()（"calibrated"），不复写拼写（R-A A7）
pub fn enriched_hit(hit: &ruagent_knowledge::SearchHit,
                    relevance: Option<&ruagent_knowledge::RelevanceScore>,
                    lead: Option<ruagent_memory::inject::WikiLeadMeta>)
                    -> ruagent_memory::inject::EnrichedHit
```

调用方式（chat.rs / runs.rs，t19 的两处构造点）：

```rust
let page = kb.search_page(query, CHAT_KNOWLEDGE_SEARCH_N).await?;   // I-A 已落地
for rh in &page.hits {
    let lead = if rh.hit.document.starts_with("wiki/") {
        let slug = rh.hit.document.trim_start_matches("wiki/").trim_end_matches(".md");
        crate::wiki::lead_for(kb, None, slug).await.as_ref().map(crate::memembed::lead_meta)
    } else { None };
    enriched.push(crate::memembed::enriched_hit(&rh.hit, rh.relevance.as_ref(), lead));
}
items.extend(knowledge_items_enriched(&enriched, KNOWLEDGE_SOURCES, WIKI_PAGES));
```

`wiki::lead_for` 的实际签名是**三参**（`kb, recorded_hash: Option<&str>, slug`，相对 R-D D.7 的冻结签名加参，由 I-D 登记）；三态字段由 `lead_meta` 原样带过来，**integ 不需要再判断 `edited`**。

## 8 规格侧登记

- `docs/design/reviews/gen2-memory-spec.md` 的 **D.7**（`WikiLeadMeta` 冻结形状）与 **R-8** 之后新增 **R-9** 一行：`edited` 由 `bool` 更正为 `Option<bool>`（改前红读数 2 failed、`None`/`Some(false)` 逐字节相同），并注明这是**接口更正**（三态事实不能用二值字段承载），原文保留、不覆盖。
