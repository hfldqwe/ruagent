# t315 四项不可判定的收口（与 t314 同一件事的第二次独立读）

**结论：pass** —— 四项**全部**判为**【不更严】**，本次是**重新读源**（不是复用 t314 的读数），并补一条 t314 没写的：**行号会随提交漂移，故出处必须连提交一起引**。

## 与 t314 的关系（先说清）
本单与 **t314** 的 objective/acceptance **逐条相同** ⇒ 我在 t314 已完成同一件事（文档 docs/design/reviews/t314-response-row-types.md）。
本单**没有复用** t314 的读数：四个类型源**重新读了一遍**，并**先查了它们自 t314 以来是否变过**。

## 变更检查（决定 t314 的结论是否仍成立）
| 文件 | 最后一次提交 |
|---|---|
| crates/memory/src/lib.rs | a33663d 2026-09-26（t276 的两条硬删路径） |
| crates/knowledge/src/store.rs | 9860bce 2026-09-26（t250 的 prefix/LIKE） |
| crates/daemon/src/wiki.rs | 9fb480b 2026-09-26（t252 入库 CLI） |
| crates/daemon/src/api.rs | d0a27b3 2026-09-26（知识命中的两个集合） |
⇒ **四者都早于 t314** ⇒ 结构未变 ⇒ 结论应当仍成立（下面逐条重读确认）。

## 四项判定（重新读出的出处）
| # | 字段 | 响应行类型 | Option 性 | 判定 | 出处（本次读出） |
|---|---|---|---|---|---|
| 1 | memory 行 store | MemoryRow.store: MemoryStore | **非 Option**（unit enum ⇒ serde 序列化为字符串 ⇒ 永不为 null） | **不更严** | crates/memory/src/lib.rs:70, **72** |
| 2 | memory 行 namespace | MemoryRow.namespace: String | **非 Option** | **不更严** | crates/memory/src/lib.rs:70, **73** |
| 3 | documents created_at | KnowledgeDocument.created_at: String | **非 Option** | **不更严** | crates/knowledge/src/store.rs:103, **108** |
| 4 | wiki/pages slug | WikiPageInfo.slug: String | **非 Option** | **不更严** | crates/daemon/src/wiki.rs:**1278,1279** |
**响应行 = 该结构**的依据（本次重读）：crates/daemon/src/api.rs 的 memory_list 里 ruagent_memory::query::all_memories(...) 的返回值被**直接**放进 "memories"（本次读到 **:3733** 取数、**:3737-3738** 组响应）。
**⚠️ 出处要连提交引**：t314 记的是 api.rs:3732-3738，本次同一条语句在 **:3733**（d0a27b3 之后行号下移 1）⇒ **行号随提交漂移** ⇒ 引用类型出处时应写「文件:行 + 提交」。

## 可信度旁证（与 t314 一致）
MemoryRow 里**确实可空**的字段**都**是 Option：supersedes: Option<i64>（本次读出 **:76**）· superseded_at · deleted_at（t251 墓碑）⇒ 该结构**对可空性有表达** ⇒ store/namespace 非 Option 是**刻意的** ⇒ 面板对它们用 string、对 supersedes 用 present **与类型完全一致**。

## 结论
**4 项不可判定 ⇒ 0 项**（无剩余不可判定，故无「新理由」需要写）。**t299 的 F-t299-03 关闭**。
