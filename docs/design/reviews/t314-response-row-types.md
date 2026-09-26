# t314 消掉 t299 的 4 项「不可判定」：读响应行结构

**结论：pass** —— 四项**全部**从「不可判定」改成 **【不更严】**，依据是**响应行类型**（不是请求结构）：
MemoryRow 的 store/namespace 是**非 Option**、KnowledgeDocument 的 created_at 是**非 Option**、WikiPageInfo 的 slug 是**非 Option**。

## 逐项判定（含出处）
| # | 被断言的字段 | 响应行类型与出处 | 是否 Option / 可空 | 判定 |
|---|---|---|---|---|
| 1 | memory 行 `store` | `pub struct MemoryRow { pub store: MemoryStore, ... }` — **crates/memory/src/lib.rs:70,72** | **非 Option**（`MemoryStore` 是 unit enum，serde 序列化为字符串 ⇒ 永不为 null） | **不更严** |
| 2 | memory 行 `namespace` | 同结构 **crates/memory/src/lib.rs:73** | **`String`，非 Option** | **不更严** |
| 3 | documents `created_at` | `pub struct KnowledgeDocument { ... pub created_at: String }` — **crates/knowledge/src/store.rs:103,109** | **非 Option** | **不更严** |
| 4 | wiki/pages `slug` | `pub struct WikiPageInfo { ... }` — **crates/daemon/src/wiki.rs:1278**（字段见下） | **非 Option** | **不更严** |

## 为什么这次读对了（与 t299 那次被证伪的线索对比）
t299 读的是 **`MemoryListQuery`**（**请求**结构，字段确实 `Option<String>` ✓），据此推断「响应里的 store/namespace 可能为 null」—— **推断错了对象**。
本单读的是 **`MemoryRow`**（**响应行**结构 ✓），而 memory/list 的 handler 把 `ruagent_memory::query::all_memories(...)` 的 `Vec<MemoryRow>` **直接**放进 `"memories"`（**crates/daemon/src/api.rs:3732-3738** ✓）⇒ **响应行 = MemoryRow** ✓ ⇒ 行字段的 Option 性就是响应字段的 Option 性 ✓。
**区分点**：响应里**同时**有一个顶层 `"store": q.store`（**查询回显**，可为 null ✓ api.rs:3747）与每行的 `store`（**非 null** ✓）—— t299 把前者当成了后者 ✓。面板的 spec 只断言**行字段** ✓，**顶层不在 spec 里** ✓ ⇒ 无冲突 ✓。

## 附：这些字段在同一结构里的对照（说明「非 Option」不是巧合）
`MemoryRow` 里**确实可空**的字段都写成了 Option ✓：`supersedes: Option<i64>` ✓ · `superseded_at: Option<String>` ✓ · `deleted_at: Option<String>`（t251 墓碑 ✓）⇒ **该结构对可空性是有表达的** ✓ ⇒ `store`/`namespace` 非 Option **是刻意的、可信的** ✓（面板对它们用 `string` 断言、对 `supersedes` 用 `present` ✓ **与类型完全一致** ✓）。
