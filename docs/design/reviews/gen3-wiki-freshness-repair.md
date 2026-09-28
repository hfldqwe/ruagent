# gen3 wiki 修复：读数会骗人（A1 正文锚点 + A2 stale_since 无起点）

> 单号 t74（repair round 1）· 成员 `wiki` · attempt 1 · 时间窗 2026-09-29 03:03–03:2x UTC（+08:00 11:03–11:2x）
> 上游：`docs/design/reviews/gen3-audit-wiki.md`（t68 的 A1 与 A2，两条 high，同一「读数会骗人」族）
> inScope：`crates/daemon/src/wiki.rs` + 本报告 ·（**越界一处，见 §6**：`crates/mock-agent/tests/wiki_pipeline.rs` 的 **1 条断言**）
> 纪律：端到端只用临时 root + 临时端口；不写活库；未启停真守护进程 pid 79984（读数 `79984 alive: True`）。

## 0 一句话

两处读数不再骗人：① **同一个锚点写在正文里还是 frontmatter 里，判定一致**（正文锚点进入 freshness 与 uncited 判定，"没有记录"不再被读成"没有锚点"）；② **`stale=true` 的响应必带非空 `stale_since`**，且**连读两次逐字相同**（首次观测在读路径落地，不再等下一次构建）。代价是诚实的代价：`/wiki/pages` 每页多跑一次构建闸门 ⇒ 53 页规模上 ~2×（见 §5）。

## 1 改动（`crates/daemon/src/wiki.rs`：4 处代码 + 2 条单测）

| # | 位置 | 改前 | 改后 |
| --- | --- | --- | --- |
| 1 | `freshness_with` | 只遍历 frontmatter `citations`；`if !meta.citations.is_empty()` ⇒ **正文锚点完全不参与**判定 | 同一趟解析**两处**锚点：frontmatter + 正文（新增纯函数 `body_anchor_gaps`）；正文独有的锚点也进 `chunk_index`，解析不到 ⇒ `ChunkMissing`。正文锚点没有 `chunk_hash` ⇒ **只判"在不在"**，且"没有记录"不等于通过（`cite_coverage` 仍是 `null`） |
| 2 | `freshness_with` 的文件读取 | 文件只在 `recorded_hash` 分支里读（这就是正文不可见的根因） | **一次读取服务两件事**：手改检测 + 正文锚点 |
| 3 | `uncited_sections_of` | `if meta.citations.is_empty() { return Vec::new(); }` ⇒ **没有记录就报"没有未引用节"** | 删除早退：始终用构建闸门同一个 `verify_page` 读正文（"没有记录 ≠ 没有锚点"） |
| 4 | `pages()` | `stale_since: record.stale_since.clone()` ⇒ 行里没写就发 `null` | 先算完所有页 freshness；把 **stale 且尚无起点**的页**批量**交给新函数 `stamp_stale_since`（复用构建路径的 `record_invalidation`：`COALESCE(existing, now)`，只记首次），发布的是**库里的值**；一批里没有需要盖章的页 ⇒ **零写入** |
| 5 | 单测 `body_anchor_gaps_are_the_anchors_the_frontmatter_forgot` | — | 未记录的可解析锚点进 gap（去重）；已记录的不重复合并；`#0`/缺 `#id` 仍被解析器丢弃；**空 frontmatter ⇒ 每个正文锚点都是 gap** |
| 6 | 单测 `stale_since_is_stamped_once_and_then_never_moves` | — | 首次盖章非空；第二/第三次调用返回**同一个字符串**（起点不动）；空批次零写入 |

关键设计约束写进了注释：`stale_since` **必须发布库里的值**，否则每次轮询都是新的 `now`，"连读两次相同"立刻不成立。

**失败模式（登记）**：如果那次写入没落地（只读库/写者不可用），`stamp_stale_since` 返回空 ⇒ 该页回到旧行为（`stale_since: null`），并且 `run_write` 会 `tracing::error!("wiki write did not land")` —— **不会**悄悄给一个假的起点。即"给不出起点"与"起点是假的"是两件事，前者仍然可用、后者不存在。

## 2 改前 → 改后读数（**原读法**：t68 的同一脚本、同一批夹具、同一端口形状，只换新 root）

- 改前：`%TEMP%\ruagent-t68-root`、端口 8898、PID **37172**、二进制 02:43:11（审计那一次）
- 改后：`%TEMP%\ruagent-t74-root`、端口 8898、PID **71884**、二进制 **03:15:19**（含本单改动）
- 命令：`powershell -NoProfile -ExecutionPolicy Bypass -File %TEMP%\t68-probe.ps1`（**未改一字**）

| 夹具 | 改前（审计） | 改后（本单） |
| --- | --- | --- |
| `p-body-dangle`（正文里一处断链，frontmatter 无记录） | `freshness=fresh, stale=false, stale_since=null, stale_reasons=[], citations=0, uncited_sections=[]` | **`freshness=stale, stale=true, stale_since="2026-09-28T19:16:38.820230500+00:00", stale_reasons=["chunk missing"], citations=0, uncited_sections=["Body"]`** |
| `p-front-dangle`（**同一句**锚点记进了 frontmatter） | `freshness=stale, stale_reasons=["chunk missing"], uncited_sections=["Body"], stale_since=null` | `freshness=stale, stale_reasons=["chunk missing"], uncited_sections=["Body"], **stale_since 非空**` |
| `p-drift`（记录 hash 过期） | `freshness=stale, stale_reasons=["source hash drift","chunk hash drift"], uncited_sections=["Body"], stale_since=null` | 同上 + **`stale_since` 非空** |

**A1 判据（搬动锚点不改变判定）**：`p-body-dangle` 与 `p-front-dangle` 的 `(freshness, stale, stale_reasons, uncited_sections)` **现在逐位相同**；改前是 `fresh` vs `stale`。两页正文逐字相同，唯一差别只是锚点在正文/ frontmatter。
**A2 判据（不跑构建、连读两次相同）**：三个 stale 页的 `stale_since` 是**同一个字符串** `2026-09-28T19:16:38.820230500+00:00`（同一批首次观测），第二次读返回**逐字相同**的值（见 §3 的 READ1/READ2）。
**未变的读数（本单不管，登记以防误读）**：POST 纠错到不存在的 slug 仍是 `HTTP 200 {"id":1,...}`（t68 的 A3，另一张单）；`cite_coverage` 在这些页上仍是 `null`（没有构建期记录 ⇒ unknown，符合 RV-D-1）。

## 3 负控与正控（证明闸门没被关掉）

命令：`powershell -NoProfile -ExecutionPolicy Bypass -File %TEMP%\t74-probe2.ps1`（同一守护进程；READ1 与 READ2 之间隔 2 秒）

- **负控** `p-body-bad`（正文锚点指向**不存在**的 chunk `probe-src#424242`）：`freshness=stale, stale=true, stale_reasons=["chunk missing"], uncited_sections=["Body"]` ⇒ **真正缺失的 chunk 仍被判 stale 并报出原因** ✓
- **正控** `p-body-ok`（正文锚点指向**存在**的 chunk `probe-src#1`）：`freshness=fresh, stale=false, stale_since=null, stale_reasons=[], uncited_sections=[]` ⇒ 修复**没有**把"看不见"变成"一律 stale" ✓
- **连读两次逐字相同**（READ1 = READ2）：
  - `p-body-dangle` `stale_since=2026-09-28T19:16:38.820230500+00:00`（同一字符串）
  - `p-body-bad` `stale_since=2026-09-28T19:17:01.407951400+00:00`（**首次读数即已盖章**；第二次读相同）
  - `p-body-ok` / 所有 fresh 页 `stale_since=null`（该字段仍只表示"何时开始失效"，不是"总是有值"）

## 4 门禁

| 命令 | 读数 |
| --- | --- |
| `scripts/cargo-team.ps1 test -p ruagent-daemon --lib` | **exit=0**，`test result: ok. **80 passed**; 0 failed; 0 ignored`（**当字节最终一轮**：3.36s，总 31.8s；其中 `wiki::tests::body_anchor_gaps_are_the_anchors_the_frontmatter_forgot` 与 `wiki::tests::stale_since_is_stamped_once_and_then_never_moves` 是本单新增的 2 条） |
| `scripts/cargo-team.ps1 test -p ruagent-mock-agent --test wiki_pipeline`（**受影响 target**：§6 那条断言就在这里） | **exit=0**，`test result: ok. **15 passed**; 0 failed; 0 ignored`（33.08s，总 62.7s） |
| `scripts/cargo-team.ps1 clippy -p ruagent-daemon --all-targets -DenyWarnings -CleanFirst ruagent-daemon` | **exit=0**，0 warning / 0 error。两件证据：①同一持锁调用内先 `clean -p ruagent-daemon (forced re-check)`（排除假绿，t44 的教训）；②本次输出含 `Checking ruagent-daemon v0.1.0 (…\crates\daemon)`，`Finished dev profile in 11.87s` |
| `cd panel; npm run build` | **exit=0**，`i18n dictionary integrity` + `✓ 1852 modules transformed` + `✓ built in 7.15s`（本单未触及面板；按契约仍给读数） |
| `rustfmt --edition 2024 --check crates/daemon/src/wiki.rs crates/mock-agent/tests/wiki_pipeline.rs`（AGENTS.md「提交前格式化」t66） | **exit=0**（无 diff）。**只格式化我碰过的两个文件**，没有跑 `cargo fmt --all`（那会把局部 diff 变成全仓 diff） |

**中途红过一次（值得登记）**：第一次跑 clippy 门禁时，我自己的新代码触发 `clippy::collapsible_if`（`crates/daemon/src/wiki.rs:973`，`if let Some(..) { if .. } }`）⇒ **exit=101**。改成 edition-2024 的 let-chain 后重跑得 exit=0。也就是说这条门禁**确实咬到了本单的代码**（不是复述旧绿）。

## 5 代价（诚实登记）：`/wiki/pages` 约 2× —— 审计 A6 变严重了

同一脚本的延迟 A/B 段（best-of-5，同一台机）：

| 规模 | 改前 `/wiki/pages` | 改后 `/wiki/pages` | 改前/改后 `/wiki/links` |
| --- | --- | --- | --- |
| 3 页 | 30.2 ms | 37.7 ms | 27.7 / 68.1 ms |
| 28 页（+25 无 citations） | 73.7 ms | 134.4 ms | 33.0 / 39.9 ms |
| 53 页（+25 带 citations） | 157.8 ms | **317.6 ms** | 47.7 / 55.5 ms |

**为什么**：修好 A1 之后，`uncited_sections_of` 对**每一页**都跑一次 `verify_page`（改前对 `citations: []` 的页直接早退），加上 `freshness_with` 对每页多读一次文件。**这是真话的价钱**，但面板对这个端点做 3 秒轮询（`Wiki.tsx:80`）⇒ A6 从"中"变成"中高"。
**建议的下一步（不属于本单）**：把 `uncited_sections` 变成**构建期记录值**（像 `cite_coverage` 一样落库、按 freshness 引用），读端点不再每页跑闸门；或在 `pages()` 里把 `chunk_index` 复用一次给 freshness 与 uncited（现在是同一页跑两遍）。本单**没有**顺手改这个 —— 它属于另一张单（A6）。

## 6 越界声明 —— **已由 captain 裁决并入 inScope**（保留原文）

> **裁决（captain，2026-09-29）：选项 1 —— 该文件已加入 t74 的 inScope，原样提交。** 理由原文：那条断言**逐字把你判定的失真行为钉成期望**，与本单 A2 的验收直接矛盾；本代已多次遇到这一族（t88 的 `status_predicates`、t78 的 C-4）——**绿测试把问题行为断言成期望**。所以规则是：**改变行为的单必须拥有钉住该行为的测试**，否则一行断言要走两次派单。其余行未动，符合范围纪律。以下为提交时的原文，保留。

`crates/mock-agent/tests/wiki_pipeline.rs` **不在本单 inScope**，我改了它**一条断言 + 其注释**（原 `:852-857`）：

```
旧：// …and the OBSERVATION TIME is written by the next build, which is this
    // reading's window: nothing else observes drift.
    assert!(pages[0]["stale_since"].is_null(), "not observed by a build yet: {pages:?}");
新：assert!(!since.is_empty(), "stale_since must be observed on the read path");
    // 追加：第二次读同一个值
```

**理由**：这条断言**逐字编码了本单判定为失真的那个行为**（"nothing else observes drift" 正是 A2 说的"读数没有起点"）。本单验收要求 `stale_since` 在读路径落地 ⇒ 该断言必红；不改它，交付就是一条红门禁 + 一对自相矛盾的判据。处理方式是最小改动（1 条断言 + 注释，未动该文件任何其它行），并列入 `changedPaths`。**若 captain 判定该文件应由 INT 改，我可回滚这一处并把新判据交给 INT**（回滚不影响 wiki.rs 的行为）。

## 7 未取到 / 不做（不静默，不写成 not_measured）

**未取到 1 条（带原因，且已补取）**：本单**第一次**尝试取"改后读数"时取不到 —— 当时 `cargo build -p ruagent` 被**别的成员的在途编辑**挡住（`crates/memory/src/write.rs:236/284/339` 的 `Step::Outcome` 用法错误），而"改后读数必须来自**含本单改动**的二进制"。当时能拿到的唯一读数是**不含本单改动**的旧二进制（02:43:11）的输出 —— 那等于把改前读数贴成改后，**故不做**。等 workspace 编译通过（03:15:19 的新二进制）后才取，§2/§3 的全部改后读数都来自它。

**中途被同伴打断（只报告，不动手）**：本单期间 workspace 被三处在途编辑打断过 —— `crates/memory/src/write.rs`（`Step::Outcome`，行号从 :236/:284 漂到 :339）、`crates/graph/src/lib.rs`（:1314/:1368 → :1126/:1134）、`crates/graph/src/community.rs:215`（`cannot borrow *conn as mutable`）。按纪律我只记录"哪个文件、什么错"并重试，**没有**替同伴改任何文件；三条门禁的最终读数都取自 workspace 编译通过后的**当字节**（`--lib` 47.8s / `clippy` 10.17s / `wiki_pipeline` 72.0s 总耗时，见 §4）。

**明确不做（属别的单）**：
- **A6 的性能修法**：本单只登记 A1 带来的 ~2× 代价（§5），不顺手改 `uncited_sections` 的取数口（把构建期读数落库是新的一张单）。
- **t68 的 A3/A4**（纠错端点不校验 slug、`corrections()` 静默丢弃无法解析的行）：与本次两处读数无关，未触及。
- **面板 A8**（`stale_reasons`/`stale_since`/`unknown_cause` 不显示）：本单未触及 `panel/src`；不过现在 `stale_since` 终于有值了，A8 的显示项才真正可读（登记为依赖关系）。
- **G8 的注入侧**：`WikiLead.stale_since` 仍来自 `lead_from`（无 `Db` 的同步路径），本单只让 **HTTP 读端点**有起点；注入侧要拿到同一个值需要单独的设计（`lead_from` 拿不到库、只有 `lead_for` 能拿）。**这一点是真正的未覆盖面，不是"顺带没做"。**

## 8 方法学与纪律

- 读数取自 `cargo-team.ps1 build -p ruagent` 产出的二进制（**03:15:19**，含本单改动），不是旧构建。
- 临时资源：root `%TEMP%\ruagent-t74-root`（已删）· 端口 **8898** · PID **71884**（`Stop-Process` 后 `alive after stop: False`）；探针脚本在 `%TEMP%`（不入库）。
- 未写活库；未启停 pid 79984。
- 改动前后各一轮「同一脚本、同一夹具、同一端口形状」的读数（§2），以及正/负控与连读两次（§3）。
