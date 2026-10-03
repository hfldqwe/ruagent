# 同族只读盘点：消费面「读生产方从不发出的键」的协议漂移吸收器全清单（t123，只读）（mem-core）

- 任务：t123（kind=work，只读盘点）；attempt `222dcf53-a9d1-4f95-8af5-ff177bde5420`
- inScope：本报告（唯一写入）。**`crates/**`、`panel/**`、`.github/**`、`scripts/**` 一行未改**：收尾 `git status --porcelain -- crates panel .github scripts` 只列出**同伴在途**的 ` M crates/mcp/src/lib.rs`、` M crates/mcp/tests/roundtrip.rs`（t118）；全仓状态里另有 `.claude/skills/**` 与 `skills/**` 的删除/未跟踪项，**同样不是本单所为**（本单没有碰这两处）
- 被审字节（**读取当时**的坐标与哈希，逐条回读，不沿用旧坐标）：
  - `crates/mcp/src/lib.rs` sha256 `52e9f00a5e822a750481f4dd4b4f67bdbcdf838def514572fc385959d2b4b9eb`，**2561 行**（同伴 t118 在途：同一文件在本单开始读时为 2474 行、sha256 `6cd24c4c…` 到 `ce44345a…` 到 `52e9f00a…`，**行号在盘点期间移动过**，见 §8-2）
  - `panel/src/api.ts` sha256 `85b8fc71e8ba3e0fbd63a7dff43493372986db74b268ebd025f97d57914f2d74`，1417 行（干净）
- **未跑任何 rust 构建**（同伴 t118/t62 占用唯一构建名额与全局锁）⇒ 本清单所有读数都是**读代码 + 读既有测试**；需要实跑才能判定的条目按第 19 条写成「未测 + 为什么 + 需要什么才能测」（§7）
- 未触碰活守护进程（127.0.0.1:8787，pid 79984）与活库 `~/.ruagent`
- **`graph_entity` 的 `resp.get("entity")` / `resp.get("exists")` 已由 t118 处理，本清单不重复**；它只作为**样板**（§1-①、§6）

## 0 结论摘要

1. **本质形状（F1）在 MCP 桥里有 13 组读点（覆盖约 25 个键读），面板视图另 3 处**：读生产方响应键 + **吸收缺失**（`unwrap_or`/`unwrap_or_default`/`map(false)`/`or_else`）⇒ 键名漂移**静默**变成一句读起来正常的成功。
2. **一个必须说清的区别（否则本清单会被误读）**：**除 t118 的 `entity`/`exists` 之外，本清单所有 (a) 读点的生产方今天都真的发这个键**（逐条核过，见 §1 第①列）。所以「可达」= 该读点在**真实调用路径上**（今天工作正常），**不是**「今天已经是坏的」；这些点是**吸收器**：漂移一旦发生，读起来仍然正常。**已漂移（生产方从不发出）的只有两处**：t118 的 `entity`/`exists`（§1-M6、已立单）与 `hits_to_text` 的兜底键 `result`（§1-M16，`daemon` 里 0 次）。
3. **排序第一的是 `hits_to_text` 的 `hits` 吸收**（`memory_search`/`knowledge_search` 共用，坐标 72 / 157）：漂移后工具回一句 **`no results`** —— 这是「我的记忆/知识库里没有这条」的**具体假话**，而生产方明明有命中（`api.rs:4974` 的 `memory_search`、`api.rs:5377` 的 knowledge search 今天都发 `hits`）。
4. 桥里**已经有一批响亮拒绝（(c) 类）**：`field_str`/`field_bool`/`drift` 与 `capabilities`、`distill` 的承载键都是 `ok_or_else(drift(...))` ⇒ 漂移会命名文件、键与取回的键集（测试钉住：`lib.rs` 的 `capabilities_to_text_refuses_protocol_drift_instead_of_reporting_nothing` 等）。**本盘点把「哪些点已经是 (c)」也列出来**，因为同一文件里两种写法并存正是给修复用的模式；更要紧的是**同一函数里也混用**（§2 的「按键分级」表）——那才是修复的模板。
5. **面板侧比 MCP 桥严格**：`panel/src/api.ts` 用 `getChecked`/`expectShape` 做**运行期形状断言**（(c) 类），并且 `knowledgeSearchLegs` 的注释明确**禁止** `?? 0`；仍存在的 (a) 类吸收集中在**视图**对可选字段的 `?? 0`/`?? []`（如 `Agents.tsx:353` 的 `s.health.tools ?? 0`）。

## 1 MCP 桥（`crates/mcp/src/lib.rs`）逐读点四要素

| # | 读点（行号 / 读法原文） | ① 生产者与键定义（当前字节） | ② 吸收缺失的读法 | ③ 漂移后果（三态）与 (a) 给 agent/用户的原话 | ④ 钉住它的测试 |
| --- | --- | --- | --- | --- | --- |
| **M1** | `72` `Ok(hits_to_text(&resp["hits"]))`（`memory_search`）；`157` 同（`knowledge_search`） | `api.rs:4974 Ok(Json(json!({ "hits": hits })))`（memory search）；`api.rs:5377 "hits": out`（knowledge search） | `hits_to_text`：`hits.as_array().cloned().unwrap_or_default()` ⇒ 缺失/改名为 `Null` ⇒ 空数组 | **(a) 静默假成功（可达、最高优先）**：有命中却回 **`no results`**；用户/agent 读作「记忆里没有这条」 | `crates/mcp/tests/roundtrip.rs`（t118 在途，`roundtrip.rs:480 mcp_capability_plane_tools_reach_the_daemon` 等）；**无专门钉住 `hits` 改名的用例** |
| **M2** | `132` `let m = &resp["memory"];`；`135-137` `m["store"].as_str().unwrap_or("?")` / `m["namespace"]` / `m["content"]`（`memory_get`） | `api.rs:632 Ok(Json(json!({ "memory": m })))`；`m` 为 `MemoryRow`（`memory/src/query.rs:209-214` 的 SELECT 列：`id, store, namespace, content, …`） | `unwrap_or("?")` ⇒ 任一键缺失 ⇒ **`"[?/?] ?"`** | **(a) 静默假成功（可达）**：一条真实记忆显示为 **`[?/?] ?`**（未知库、未知命名空间、空内容）⇒ agent 会以为记忆是空的/别人的 | 无（`crates/mcp/src/lib.rs` 的 `mod tests` 里没有 memory_get 的形状用例） |
| **M3** | `109` `params.top_n.unwrap_or(5)`（`memory_recall`） | —— **不是响应键，是工具入参默认值** | —— | 不计入本族（点名以免混淆） | —— |
| **M18** | `95-99` `Ok(format!("memory write: {}", resp["outcome"].as_str().unwrap_or("?")))`（`memory_write`） | **`api.rs:4949 serde_json::json!({ "outcome": format!("{outcome:?}") })`**（memory write handler，今天真的发） | `unwrap_or("?")` ⇒ 键缺失 ⇒ **`?`** | **(a) 静默假成功（可达）**：回 **`memory write: ?`** ⇒ agent 读作「写入完成，结果是个问号」而不是「协议漂移了」 | 无 |
| **M4** | `179` `resp["chunks"].as_i64().unwrap_or(0)`（`knowledge_ingest`）；原文 `Ok(format!("ingested {} chunks", …))` | ingest 响应发 `"chunks": <n>`（`api.rs:5360-5385` 一带的知识 ingest handler；**同文件 `5377` 是 knowledge search 的 `"hits": out`，两者不要混**） | `unwrap_or(0)` ⇒ 键缺失 ⇒ **0** | **(a) 静默假成功（可达）**：回 **`ingested 0 chunks`** ⇒ agent 会当成「文档是空的」而不是「键改名了」 | 无（`roundtrip.rs` 有 reach 类用例，无 chunks 改名用例） |
| **M5** | `202-204` `resp["document"]/["section"]/["chunk"]` 各 `.unwrap_or("?")`（`knowledge_expand`） | `Expansion`（`knowledge/src/files.rs:52-61`：`chunk_id, document, section, chunk, file`）⇒ 键**都存在** | `unwrap_or("?")` ⇒ 缺失 ⇒ `?` | **(a) 静默假成功（可达、低危）**：展开结果里正文变成 **`?`** ⇒ agent 拿到「展开成功但内容是问号」 | 无 |
| **M6** | `233-236`（旧坐标）`resp.get("entity")` / `resp.get("exists")` ⇒ **t118 在处理，本清单不重复**（作为样板） | **`api.rs:1339-1344`**：graph entity 只发 `name`/`kind`/`aliases`/`facts`，**从不发 `entity`/`exists`** | 旧代码用 `.map(|e| e.is_null()).or_else(...).unwrap_or(false)` 把「键不存在」吸收成「false」 | **(a) 静默假成功**（t118 的原始缺陷，已立单） | `roundtrip.rs:367 a_fabricated_entity_id_is_refused_not_answered_with_no_facts`、`lib.rs:1675 entity_existence_is_read_from_name_and_drift_is_not_absorbed`（t118 已修） |
| **M7** | `251-254` `f["src"].as_i64().unwrap_or(0)` / `f["relation"]…unwrap_or("?")` / `f["dst"]…unwrap_or(0)` / `f["fact_text"]…unwrap_or("?")`（`graph_entity` 的 facts 渲染） | `graph/src/lib.rs:256-267 current_facts` ⇒ `Vec<Edge>`，SELECT 列 **`id, src, dst, relation, fact_text, valid_at, invalid_at, source_episode`** ⇒ 键**都存在**（serde 结构体字段，**不是** `json!` 字面量，见 §8-1） | `unwrap_or(0)`/`unwrap_or("?")` ⇒ 缺失 ⇒ **`#0 --?--> #0 ?`** | **(a) 静默假成功（可达）**：一条事实显示为 **`#0 --?--> #0 ?`** ⇒ agent 读到「两个编号为 0 的实体之间有个问号关系」 | 无（facts 的四字段无钉住用例） |
| **M8** | `477-484` `resp["tasks"].as_array().cloned().unwrap_or_default()` + `t["status"]/["title"]/["id"]` `.unwrap_or("?")`（`list_tasks`） | `api.rs:2012 list_tasks` ⇒ `{"tasks": [...]}`（`"tasks"` 在 daemon 出现 2 次） | 数组 `unwrap_or_default()` + 三个字段 `unwrap_or("?")` | **(a) 静默假成功（可达）**：回 **`[?] ? (?)`**（空表时回 **`no tasks`**）⇒ agent 会以为「没有任务」 | 无 |
| **M9** | `396-400` `current.get("capabilities").and_then(as_array).ok_or_else(drift)`（`capability_set`） | `api.rs` 的 GET `/api/v1/capabilities` ⇒ `{"capabilities": [...]}` | **`ok_or_else(drift(...))` ⇒ 响亮** | **(c) 响亮报错** ✅（命名键与取回的键集） | `lib.rs:2127 capability_set_refuses_to_guess_which_keys_the_file_carries`、`roundtrip.rs:755/953` |
| **M10** | `755-756` `.get("capabilities").and_then(as_array).ok_or_else(drift)`（`capabilities_to_text`） | 同上（`api.rs` capabilities） | **`ok_or_else(drift)` ⇒ 响亮** | **(c) 响亮** ✅ | `lib.rs:1910 capabilities_to_text_refuses_protocol_drift_instead_of_reporting_nothing` |
| **M11** | `760-762` `.get("config_file").and_then(as_str).unwrap_or("?")`；`768` `.filter(｜r｜ r.get("tier")…==Some(tier))` | `api.rs` capabilities 每行发 `config_file`/`tier`（`config_file` 1 次、`tier` 3 次） | `config_file` ⇒ `?`；**`tier` 过滤器 ⇒ 匹配不到任何行**。注意同一函数里 `table_present` 是 `field_bool(…)?`、行内 `id`/`tier`/`enabled`/`configured` 是 `field_str(…)?` ⇒ **那些是响亮的**；**只有 `config_file` 与 tier 过滤这两处吸收** | **(a) 静默假成功（可达）**；该行原话是 `capability plane: {selected_count} of {rows.len()} capabilities with tier={t}; [capabilities] table PRESENT in {file}` ⇒ 漂移后为 **`capability plane: 0 of N capabilities with tier=llm; [capabilities] table PRESENT in ?`** ⇒ agent 读作「这个平面**没有任何** llm 能力」（实际是 `tier` 改名） | `lib.rs:1897 capabilities_to_text_filters_by_tier_and_refuses_a_bogus_tier`（钉住 bogus tier，**不钉 tier 改名**） |
| **M12** | `793` `row.get("new").and_then(as_bool).unwrap_or(false)`；`798` `row.get("description")`；`803` `row.get("gates")` | `api.rs` 的 capabilities 行发 `new`/`description`/`gates`（各 3/11/3 次） | `unwrap_or(false)` ⇒ 缺失 ⇒ `false` | **(a) 静默假成功（可达）**：一次「新能力」的 **`, new`** 后缀消失 ⇒ agent 不会去读它的 `gates` 说明（`description` 改名则那段缩进说明整行消失）；行本身仍在（`id`/`tier`/`enabled`/`configured` 是响亮读法） | `lib.rs:2002 capabilities_to_text_refuses_a_carried_key_with_no_value`（钉住「有键无值」，不钉改名） |
| **M13** | `1047-1077` `.get("distilled")…ok_or_else(drift)` / `.get("dry_run")…unwrap_or(requested_dry_run)`（1055-1059）/ `.get("source")` / `.get("truncated")…unwrap_or(false)`（`distill_to_text`） | `api.rs` distill 响应发 `distilled`/`dry_run`/`source`/`truncated`（8/10/7/1 次） | `distilled` 与四个计数（`field_u64(…)?`）**响亮**；**只有 `dry_run`/`truncated`/`source` 吸收** | **(a) 静默假成功（可达）**：① `truncated` 改名 ⇒ 这句解释**消失**：`（the transcript was truncated to the prompt budget）` ⇒ agent 以为拿到全文；② `dry_run` 改名 ⇒ 工具把**自己请求里的 `dry_run`** 回显成 ` [DRY RUN — nothing was written]`（可能与守护进程真实行为不符）；③ `source` 改名 ⇒ ` (extractor: …)` 消失 | `lib.rs:2339 distill_to_text_refuses_a_response_without_the_load_bearing_keys`（钉住 `distilled` 缺失，**不钉 `truncated`/`dry_run`/`source`**） |
| **M14** | `1101-1106` `.get("ingest")…ok_or_else(drift)` / `.get("dry_run")…unwrap_or(false)` | `api.rs` ingest 响应 | 四个计数是 `field_u64(…)?` ⇒ 响亮；**只有 `dry_run` 吸收** | **(a) 静默假成功（可达）**：`dry_run` 改名 ⇒ **` [DRY RUN — nothing written, no ledger row]`** 标记消失 ⇒ 一次 dry-run 被读成**真写入**（`documents/entities/relations/candidates` 数字照常显示，读起来完全正常） | `lib.rs:2383 ingest_to_text_reports_the_frozen_counts_and_flags_missing_ones`（钉住 counts，**不钉 `dry_run`**） |
| **M15** | `1521-1596` `recall_to_text`/`get_info`：按 `it["kind"]` 分派（`"memory"`/`"knowledge"`/`"wiki"`/`"entity"`，`_ => continue`）+ `it["document"]`/`it["content"].or_else(it["title"])`/`it["excerpt"]`/`it["chunk_id"]`/`it["slug"]`/`it["stale"]`/`it["summary"]`/`it["related"]["wiki"]` + `resp["legs_disabled"]…unwrap_or_default()` | `api.rs:3710-3713` 发 `memories`/`knowledge`/`wiki`/`entities`；memory stub 发 `kind/id/store/namespace/title/score/…`（3474）、full 发 `content`（3486）；knowledge 发 `chunk_id/document/excerpt`（chunk 段）；entity 发 `kind/id/name/entity_kind/facts/related`（3620）与 `…/summary`（3626）；`api.rs:3861 "legs_disabled": legs_disabled` | `kind` `unwrap_or("?")` + **`_ => continue`** ⇒ 整段条目被**静默跳过**；`legs_disabled` `unwrap_or_default()` ⇒ 解释行消失 | **(a) 静默假成功（可达）**：① `kind` 改名/新取值 ⇒ 章节标题下**一行都没有**；② `legs_disabled` 改名 ⇒ **`(legs disabled by the capability plane: …)`** 这行解释消失，薄结果无法解释 | `lib.rs:1743 recall_to_text_explains_the_legs_the_plane_switched_off`（钉住 legs_disabled 的**存在**，不钉改名的**消失**） |
| **M16** | `1614-1624 hits_to_text`：`hits.as_array().cloned().unwrap_or_default()`；`h["content"].as_str().or_else(|| h["result"].as_str()).unwrap_or("?")`；`h["document"].as_str().unwrap_or("memory")` | `api.rs:4974/5377` 的 `hits` 元素发 `content`（`content` 21 次）与 `document`（4 次）；**`result` 在 daemon 0 次 ⇒ 兜底键从不发出** | `unwrap_or_default()` ⇒ 空 ⇒ **`no results`**；`or_else(result)` ⇒ 主键缺失时才会用到（**已经是死兜底**） | **(a) 静默假成功（可达）**：`hits` 整个改名 ⇒ **`no results`**；单条 `content` 改名 ⇒ **`?`**（同 `result` 也缺失，见 §9 的「已漂移子集」） | 无 |
| **M17** | `285/301/323/335/347` `serde_json::to_string_pretty(&resp).unwrap_or_else(|_| resp.to_string())` | 整个响应（`memory_forget_report`/`graph_search`/`graph_retrieve`/`wiki_pages`/`wiki_links`） | **不读任何键**（序列化整包）⇒ 改名对它们不可见 | **不算吸收器**：它们不声称任何键存在 ⇒ 单列在 §3 之后不计入排名 | —— |

## 2 已经是 (c) 响亮的读点（同一文件里的模式，给修复当样板）

`field_str`（`596-604`）、`field_bool`（`606-610`）、`drift`（命名「文件、键、取回的键集」的构造函数）、`options_schema`/`options_set`/`options`（`681-700`，`.ok_or_else(...)`）、`capabilities` 数组（`396`、`755`）、`distill` 的承载键（`1047`）、`ingest` 的承载键（`1101`）——**它们把「读到空」变成一条点名键的报错**。测试钉住：`capabilities_to_text_refuses_protocol_drift_instead_of_reporting_nothing`（1910）、`capabilities_to_text_refuses_a_carried_key_with_no_value`（2002）、`distill_to_text_refuses_a_response_without_the_load_bearing_keys`（2339）、`capability_set_refuses_a_field_it_cannot_send`（`roundtrip.rs:953`）、`an_option_field_this_tool_cannot_send_is_refused_at_the_wire`（2148）。⇒ **同族里两种写法并存：新点按 (c) 写，旧点仍是 (a)**。

**同一函数里两种写法混用（这一条比「哪些函数是 (c)」更有用）**：读点不是按函数分级的，而是**按键**分级的，所以三个函数**同时**是 (c) 和 (a)：

| 函数 | 响亮 (c) 的键 | 吸收 (a) 的键 |
| --- | --- | --- |
| `capabilities_to_text`（755-812） | `capabilities`（数组）、`table_present`、行内 `id`/`tier`/`enabled`/`configured`、`options`（`options_to_text(…)?`） | `config_file`（`?`）、行内 `new`（`, new` 后缀消失）、`description`（`if let Some` ⇒ 整行缩进说明消失）、`gates`（同）、**tier 过滤本身** |
| `distill_to_text`（1040-1085） | `distilled`（对象）、`session_key`/`agent`/`memories_written`/`memories_skipped`/`entities_written`/`relations_written` | `dry_run`、`truncated`、`source` |
| `ingest_to_text`（1095-1115） | `ingest`（对象）、`documents`/`entities`/`relations`/`candidates` | `dry_run` |

（`description`/`gates` 是 `if let Some(d) = row.get("description")…` ⇒ 键改名时**那两行说明静默消失**，行本身还在 —— 与 `id`/`tier`/`enabled`/`configured` 的 `field_str(…)?` 形成对照，这一条是本报告里最容易被写错的一处：我第一版就把 `gates` 误记成响亮读法，回读 795-812 行才改过来。）

⇒ **修复模板已经在本文件里**：把同一个键的读法从 `unwrap_or(default)` 换成 `field_*(WHAT, obj, key)?`，**红的那句话就是现成的**（`drift()` 会印出文件、键与取回的键集）。本清单排序第一的 M1 只需要把 `hits_to_text` 的调用点改成 `let hits = resp.get("hits").ok_or_else(|| drift("memory_search", "hits", &resp))?;`。

## 3 面板侧（`panel/src/api.ts` `85b8fc71…` + `panel/src/views/**`；`Agents.tsx` sha256 `9d759388…`）

| 读点 | 生产者与键 | 读法与吸收 | 三态后果 | 测试 |
| --- | --- | --- | --- | --- |
| `getChecked`/`expectShape`（`panel/src/api.ts`，1417 行内） | 同一 daemon 响应 | **运行期形状断言**：键缺失/类型错 ⇒ 抛错 | **(c) 响亮** ✅ | panel e2e（未跑，见 §7） |
| `knowledgeSearchLegs`（`~1300-1345`） | knowledge search hits | 注释明确：漏腿是 **`null`，never 0**，**禁止 `?? 0`** | **(b)/(c)**（`null` ≠ `0` 的判据立法） | 无 |
| `Agents.tsx:353` `s.health.tools ?? 0`（`s.health.error ?? t("mcp.down")`、`s.url ?? s.command`） | daemon agents 列表的 `health.tools`/`health.error` | `?? 0`/`?? 默认文案` | **(a) 静默假成功（UI）**：MCP 服务器显示 **`0 tools`** / 显示「down」而实际是键改名 | 无 |
| `Agents.tsx:233` `a.runtimes?.length ?? 0`；`Agents.tsx:458` `editing.runtime ?? editing.runtimes?.[0] ?? ""` | daemon agents 的 `runtimes`/`runtime` | 可选链 + `??` | **(a) 静默假成功（UI）**：agent 显示为**没有 runtime** | 无 |

## 4 排序（后果严重度 × 是否可达）

| 排名 | 读点 | (a)/(b)/(c) | 可达？ | 严重度 | 理由（哪句话会说出口） |
| --- | --- | --- | --- | --- | --- |
| **1** | M1 `hits` ⇒ `no results`（72/157） | (a) | **可达** | 高 | 有命中却回 **`no results`** ⇒ agent 对用户说「记忆/知识库里没有」并可能重复写入 |
| 2 | M2 `memory` 三键 ⇒ `[?/?] ?`（132-137） | (a) | **可达** | 高 | 真实记忆被读成空记忆 ⇒ 覆盖/重写风险 |
| 3 | M15 `kind` + `_ => continue`、`legs_disabled`（1521-1596） | (a) | **可达** | 高 | 章节**空白**或「薄结果无法解释」 |
| 4 | M16 `hits_to_text` 兜底键 `result`（1614-1624） | (a) | 可达（改 `content` 时） | 中 | 单条内容回 **`?`** |
| 5 | M4 `chunks` ⇒ `ingested 0 chunks`（179） | (a) | 可达 | 中 | ingest 成功但报 0 chunk |
| 6 | M11 `tier` 过滤 ⇒ `0 of N capabilities`；`config_file` ⇒ `?`（760-768） | (a) | 可达 | 中 | 能力面显示为空 |
| 7 | M7 facts 四字段 ⇒ `#0 --?--> #0 ?`（251-254） | (a) | 可达 | 中 | 事实显示为 0 号实体 |
| 8 | M18 `outcome` ⇒ `memory write: ?`（95-99） | (a) | 可达 | 中 | 写入确认变成问号（**注意 `outcome` 生产方今天确实发**：`api.rs:4949` ⇒ 这是「漂移后」的后果，不是今天的既有缺陷，见 §9-2） |
| 9 | M8 `tasks` ⇒ `[?] ? (?)` / `no tasks`（477-484） | (a) | 可达 | 中 | 任务列表为空 |
| 10 | M13 `truncated`/`dry_run`/`source`（1047-1077） | (a) | 可达 | 中 | 截断不可见 / 回显自己的请求 |
| 11 | M14 `dry_run` ⇒ dry-run 标记消失（1101-1106） | (a) | 可达 | 中 | 一次 dry-run 被读成真写入 |
| 12 | M5 expand 三键 ⇒ `?`（202-204） | (a) | 可达 | 低 | 展开内容为问号 |
| 13 | 面板视图 `?? 0`/`?? []`（Agents.tsx:353/233/458） | (a) | 可达 | 低-中 | UI 显示 `0 tools` / 无 runtime |
| 14 | M6 `graph_entity` **entity/exists** | (a) | 可达 | **已由 t118 处理**（不重复） |
| 15 | M9/M10/M12/M13 承载键、M3、§3 | (c) 响亮 / 不读键 | —— | —— |

## 5 排序第一条的能红负控方案（**不落盘、可恢复、不在共享树变异**）

**第一位 = M1（`hits_to_text` 的 `hits` 吸收）。**

- **为什么用「加断言」而不是「改生产方」**：漂移的产物是**少了红**（读到空被吸收），所以「能红的负控」必须是**一条会因吸收而失败的断言**，不是一条会变红的错误 —— 吸收器的红只能由**新断言**造出来。
- **做法（隔离、不落盘、可恢复）**：
  1. 在**隔离的一棵树**上做：`git worktree add %TEMP%\t123-wt HEAD`（并按 AGENTS.md 给**它自己的 `CARGO_TARGET_DIR`**：`$env:CARGO_TARGET_DIR="$env:TEMP\ruagent-wt-t123"`，**绝不指向共享 target**）。
  2. 在该树里**只加一个测试**（不改生产代码）：在 `crates/mcp/src/lib.rs` 的 `mod tests` 加
     `fn hits_are_not_absorbed_into_no_results()`：断言
     `hits_to_text(&serde_json::json!({"hits": [{"content": "x", "document": "d"}]}))` **不等于** `"no results"`，
     且 `hits_to_text(&serde_json::json!({}))` **等于** `"no results"`（后半句钉住「空就是空」这条**既有合法行为**）。
  3. **变异（可恢复）**：在同一隔离树上把生产者键改名：`crates/daemon/src/api.rs:4974` 的 `"hits"` → `"hits_v2"`（一行；或用 `python -` 生成副本后替换，**不 sed -i 到共享树**）。
  4. **预期看到的红原文**：`cargo test -p ruagent-mcp hits_are_not_absorbed_into_no_results` ⇒
     `assertion failed: hits_to_text(&json!({"hits": [...]})) != "no results"`，
     `left: "no results"`；且**只有这一条**红（同树其它用例仍绿，因为只有 `hits` 被改名）。
  5. **恢复**：`git worktree remove --force %TEMP%\t123-wt` + 删 `$env:TEMP\ruagent-wt-t123`（**具体路径**）；共享树零改动。
- **为什么现在没跑**：**同伴 t118/t62 占用唯一构建名额与全局锁**，且本单纪律是**不跑 rust 构建** ⇒ 这条负控写成「**未测 + 需要一次构建名额**」；判定它是否真的只红一条，需要 `cargo test -p ruagent-mcp` 一次（并需要隔离 target）。

## 6 未测 + 为什么 + 需要什么才能测（第 19 条）

| 条目 | 未测的部分 | 为什么 | 需要什么才能测 |
| --- | --- | --- | --- |
| 全部 (a) 条目 | 漂移后 UI/agent 的**真实显示** | 需要**生产方改名**（改 `api.rs`/`knowledge`/`graph` 结构体字段）⇒ 需重启活守护进程或重编，**pid 79984 不许碰** | 隔离树 + 一次 `cargo test -p ruagent-mcp -p ruagent-daemon`（外加闹钟/断言），或对**离线 JSON 夹具**跑纯函数（`hits_to_text`/`recall_to_text`/`capabilities_to_text` 都是纯函数 ⇒ 可无守护进程直测） |
| M1/M16 的 `hits` 改名前后的**线上形状** | —— | 同上；本单只**读**了 `api.rs` 生产端字节 | 同上（纯函数直测即可） |
| panel 侧 `expectShape` 的实际拒绝文本 | panel 类型检查与 e2e | 本单**未跑** `npm run build`/`npm run test:e2e`（e2e 需要活守护进程） | `cd panel && npm run build` + `npm run test:e2e -- --output=/tmp/pw` |

## 7 纪律与残留

- 只读：唯一写入 = 本报告。`crates/**`、`panel/**`、`.github/**`、`scripts/**` 未改；未 push / dispatch / rerun / cancel / 建 tag；未碰 pid 79984 与 `~/.ruagent`。
- 运行读数（`node`/`powershell` 只读探查）：`%TEMP%\ruagent-rb\t123_recon{,2..6}.sh` 共 6 个探查脚本；临时输出 `%TEMP%\ruagent-t123-recon{,2..6}.txt`。
- **同伴在途**：`crates/mcp/src/lib.rs` 为 ` M`（t118，读期间从 2474 行涨到 2561 行）⇒ 本报告的行号以**读取当时**的 sha256 为准；t118 的提交落地后需**重读坐标**（这正是 t20 那条纪律：哈希动了就要重读 diff、说明读数属于哪个版本）。

## 8 方法学：本盘点自己踩到的两个读数陷阱

1. **按「引号键名」清点生产方会漏掉结构体字段**：我第一版 sweep 用 `grep -rn "\"chunk\"" crates/daemon/src/` 得到 **0**，差点把 `knowledge_expand` 的 `chunk` 读成「生产方从不发出」。实际生产方是 `#[derive(Serialize)] struct Expansion { pub chunk: String, … }`（`knowledge/src/files.rs:52-61`），serde 按**字段名**发出，**没有引号**。⇒ 清点生产方必须**两种都查**（`json!` 字面量 + `Serialize` 结构体字段 + SELECT 列名）。同理 `fact_text` 是 `graph/src/lib.rs:259` 的 SELECT 列 + `Edge` 字段。
2. **文件在我读的时候动了**：`crates/mcp/src/lib.rs` 在同一轮里行数从 **2474 → 2560 → 2561**（t118 在途）⇒ 同一读点两次 `grep -n` 得到不同行号（`capabilities` 一次 400、一次 396）。⇒ 引坐标必须带**读取时刻的 sha256**；本报告每行都是**回读当时**的坐标。

## 9 「生产方从不发出的键」实例清单（本族的**已漂移**子集）

| 键 | 消费侧读法 | 生产方实况 | 今天会说出的话 | 归属 |
| --- | --- | --- | --- | --- |
| `entity` / `exists` | `resp.get("entity")` / `resp.get("exists")`（t118 前代码） | **`api.rs:1339-1344` 只发 `name`/`kind`/`aliases`/`facts`** ⇒ 两键从不发出（`"exists"` 在 `crates/daemon/src` 出现 **0** 次） | 「实体存在但无事实」的**成功** | **t118 在处理，本清单不重复** |
| `result` | `h["content"].as_str().or_else(\|\| h["result"].as_str()).unwrap_or("?")`（`lib.rs:1621-1622`） | **`"result"` 在 `crates/daemon/src` 出现 0 次** ⇒ 兜底键从不发出 | 只在 `content` 也缺失时才被用到 ⇒ 回 **`?`** | 本清单 M16 的一部分（低危：兜底，不影响正常路径） |

**验证方法（可复现）**：`grep -rn "\"<key>\"" crates/daemon/src/`（引号键名）**加上** `grep -rn "pub <key>:" crates/{graph,knowledge,memory}/src/*.rs`（`Serialize` 结构体字段）与 SELECT 列名——两条都要查（§8-1 是我自己的假阳性）。**`chunk`/`fact_text` 经第二种查法证明是活的**（`knowledge/src/files.rs:52-61`、`graph/src/lib.rs:259`）。
