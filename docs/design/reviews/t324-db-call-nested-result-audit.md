# t324 审计：`Db::call` 的嵌套 `Result` —— 还有没有别处把「只判外层」写成恒真/恒假

审计者：retrieval · 任务 t324（work round 1）· **只读审计**，结论只写在本文件。

起因：t321 自曝 —— 它第一版的牙齿断言写成 `db.call(...).await.is_err()`，恒假，因为 `Db::call` 返回
`Result<Result<T, rusqlite::Error>, DbError>`：外层 `Ok` 包着内层 `Err`。**一条永远不会失败的检查，
与一条写死的断言没有区别。** 本单把它从「一个自曝的坑」升级为「这类坑还有几处」。

## 0 结论（先给答案）

| 问题 | 答案 |
|---|---|
| 全仓对 `Db::call` 结果做 `.is_err()` / `.is_ok()` 的处数 | **0 处**（今天）；历史 1 处 = t321 第一版，**从未进过任何提交** |
| 其中「只判外层 ⇒ 恒真/恒假」的处数 | **0 处** |
| 是不是一类系统性假绿 | **不是**：一个孤例，且已被作者自己在提交前抓住 |
| 同一类「层次判错」在**非断言**位置 | **17 处**（1 处 `if let Err` 只匹配外层 · 4 处 flatten-to-default · 12 处 `let _ =`） |
| 对 `Db::call` 的调用点总数 | **134 处**，逐处清单见 §3 附录 |

一句话：**假绿只有一处，而且它在提交前就死了；留在盘上的是三类「静默失败」** —— 它们不会让测试变绿，
但会让一个失败的写/读在日志与返回值里都看不见。

## 1 接口的形状（嵌套是从哪来的）

`crates/store/src/sqlite.rs:70`：

```rust
pub async fn call<T, F>(&self, f: F) -> Result<T, DbError>
where
    T: Send + 'static,
    F: FnOnce(&mut rusqlite::Connection) -> T + Send + 'static,
```

关键在 `T` **不受约束**：闭包返回什么，`T` 就是什么。于是

- 闭包返回 `rusqlite::Result<X>`（绝大多数调用点，因为闭包内要 `prepare/query_row/execute`）
  ⇒ `call` 返回 `Result<Result<X, rusqlite::Error>, DbError>`；
- 闭包返回普通值（例如 `conn.execute(..).unwrap_or(0); n > 0`）⇒ 扁平 `Result<bool, DbError>`。

两个层次的含义完全不同：

| 层 | 类型 | 含义 | 现实可达性 |
|---|---|---|---|
| 外层 | `DbError` | 写线程没了（`DbError::Closed`：channel 关闭 / oneshot 取消） | 只在写线程死亡时 |
| 内层 | `rusqlite::Error` | SQL 真的失败了（语法错、表不存在、约束冲突…） | 日常可达 |

⇒ **只判外层等于只判「写线程还活着」**，而那个条件在正常进程里几乎恒真：这正是恒真/恒假的来源。

## 2 真跑证明（不是推断）

探针在仓库外（`C:/tmp/t324/probe/`，path 依赖本仓 `crates/store`，独立 `CARGO_TARGET_DIR`，**未触碰仓库**）。

```
[inner failure] outer.is_err() = false      <-- 坏 SQL，外层仍然 Ok
[inner failure] outer.is_ok()  = true
[inner failure] inner.is_err() = true
[inner failure] inner error    = "no such table: no_such_table"
[correct] .await.expect(..).is_err()        = true
[correct] matches!(.., Ok(Err(_)))          = true
[correct] .await.map_err(DbError::from) 的 .is_err() = false   <-- 只 map_err 不带 ? 仍然判外层
[writer dead] outer.is_err() = true         <-- 只有这一种情况外层才红
[writer dead] outer error    = "database writer is shut down"
```

两个读数值得单独说：

1. **内层失败 ⇒ 外层 `is_err()` 是 false**：这就是那条恒假断言的机制，用真跑复现，不是推断。
2. **`.map_err(DbError::from)` 单独用也还是判外层** —— 它映射外层（`DbError → DbError` 走恒等 `From`），
   内层 `Result` 原样留在值里。盘上没出事是因为调用点后面都跟着 `?`。
   **陷阱有两副面孔：只看 `.is_err()` 会漏，只看 `.map_err(..)` 也会漏。**

## 3 清单：134 个 `Db::call` 调用点

分类是**对消费链的机械读法**（正则），不是类型检查的结论；重要类别在 §4 里逐处读过源码。

| 消费形态 | 处数 | 两个层次是否都处理 |
|---|---|---|
| `.await??` | 44 | ✅ 都传播 |
| `.await?.<链>` | 27 | ✅ 外层 `?` + 内层由后续链消费（编译器强制内层必须被使用） |
| 其他（闭包内部自处理 / 链式形态） | 15 | 逐处看：多为闭包内 `.ok()/map_err` 自处理，或 `.await?.map_err(..)?` |
| `let _ = db.call(..).await;` | 12 | ❌ **两层一起丢** |
| `.await.unwrap().unwrap()` | 10 | ✅ 都解包 |
| `.await?.map_err(..)?` | 9 | ✅ 都传播 |
| `.await.map_err(..)??` / `.await.map_err(..)?` | 3 | ✅ 都传播 |
| flatten-to-default（`.ok().and_then(|r| r.ok()).unwrap_or_default()`） | 4 | ❌ **两层一起丢，变成默认值** |
| `.await.expect(..).expect(..)` | 1 | ✅（测试里） |
| `.await.unwrap()`（闭包返回普通值） | 1 | ✅ 扁平，无内层 |
| `.await.is_err()` / `.is_ok()` | **0** | —— |

### 附录：逐处清单（文件:行 + 该行所在提交）

| # | file:line | consumption of the call result | commit |
|---|---|---|---|
| 1 | crates/daemon/src/api.rs:910 | both ?? | 46a1e99 |
| 2 | crates/daemon/src/api.rs:951 | both ?? | 46a1e99 |
| 3 | crates/daemon/src/api.rs:991 | both ?? | 46a1e99 |
| 4 | crates/daemon/src/api.rs:2465 | flatten to default (both dropped) | 7588cdd |
| 5 | crates/daemon/src/api.rs:2695 | let _ = (both dropped) | 77bc646 |
| 6 | crates/daemon/src/api.rs:2784 | both ?? | 77bc646 |
| 7 | crates/daemon/src/api.rs:2827 | both ?? | f901f93 |
| 8 | crates/daemon/src/api.rs:2979 | flatten to default (both dropped) | 8e34503 |
| 9 | crates/daemon/src/api.rs:3261 | both map_err+? | 79f3096 |
| 10 | crates/daemon/src/api.rs:4086 | both unwrap.unwrap | 16387e2 |
| 11 | crates/daemon/src/api.rs:4339 | both unwrap.unwrap | a33663d |
| 12 | crates/daemon/src/chat.rs:212 | let _ = (both dropped) | c3e5f47 |
| 13 | crates/daemon/src/chat.rs:656 | both map_err+?? | 79f3096 |
| 14 | crates/daemon/src/chat.rs:877 | let _ = (both dropped) | c3e5f47 |
| 15 | crates/daemon/src/chat.rs:1214 | statement ends (value dropped) | c3e5f47 |
| 16 | crates/daemon/src/chat.rs:1276 | statement ends (value dropped) | c3e5f47 |
| 17 | crates/daemon/src/chat.rs:1355 | statement ends (value dropped) | c3e5f47 |
| 18 | crates/daemon/src/chat.rs:1572 | let _ = (both dropped) | c3e5f47 |
| 19 | crates/daemon/src/chat.rs:1663 | statement ends (value dropped) | d8aca05 |
| 20 | crates/daemon/src/distill.rs:217 | both ?? | b6678e6 |
| 21 | crates/daemon/src/distill.rs:246 | both map_err+?? | b6678e6 |
| 22 | crates/daemon/src/distill.rs:387 | both ?? | b6678e6 |
| 23 | crates/daemon/src/memembed.rs:32 | let _ = (both dropped) | 0551b55 |
| 24 | crates/daemon/src/memembed.rs:58 | flatten to default (both dropped) | 0551b55 |
| 25 | crates/daemon/src/memembed.rs:118 | other | e2b5ec8 |
| 26 | crates/daemon/src/sessions.rs:158 | outer ? then inner later | 2886f26 |
| 27 | crates/daemon/src/sessions.rs:204 | let _ = (both dropped) | b6678e6 |
| 28 | crates/daemon/src/sessions.rs:211 | both ?? | 2886f26 |
| 29 | crates/daemon/src/sessions.rs:239 | other | 2886f26 |
| 30 | crates/daemon/src/sessions.rs:282 | both ?? | 2886f26 |
| 31 | crates/daemon/src/sessions.rs:760 | other | 54e156b |
| 32 | crates/daemon/src/sessions.rs:776 | both ?? | 54e156b |
| 33 | crates/daemon/src/wiki.rs:787 | both ?? | 46a1e99 |
| 34 | crates/daemon/src/wiki.rs:968 | statement ends (value dropped) | 46a1e99 |
| 35 | crates/daemon/src/wiki.rs:1652 | other | 46a1e99 |
| 36 | crates/daemon/src/wiki.rs:1677 | both ?? | 46a1e99 |
| 37 | crates/daemon/src/wiki.rs:1698 | let _ = (both dropped) | 46a1e99 |
| 38 | crates/daemon/src/wiki.rs:1712 | let _ = (both dropped) | 46a1e99 |
| 39 | crates/daemon/src/wiki.rs:1722 | let _ = (both dropped) | 46a1e99 |
| 40 | crates/daemon/src/wiki.rs:1737 | other | 46a1e99 |
| 41 | crates/daemon/src/wiki.rs:1759 | let _ = (both dropped) | 46a1e99 |
| 42 | crates/daemon/src/wiki.rs:1773 | let _ = (both dropped) | 46a1e99 |
| 43 | crates/daemon/src/wiki.rs:1808 | let _ = (both dropped) | 9fb480b |
| 44 | crates/daemon/src/wiki.rs:1834 | both expect.expect | 9fb480b |
| 45 | crates/graph/src/lib.rs:57 | outer ? then chain | 18a9154 |
| 46 | crates/graph/src/lib.rs:97 | outer ? then chain | 18a9154 |
| 47 | crates/graph/src/lib.rs:136 | outer ? then chain | 7daccd5 |
| 48 | crates/graph/src/lib.rs:167 | outer ? then chain | 18a9154 |
| 49 | crates/graph/src/lib.rs:193 | outer ? then chain | 1d10344 |
| 50 | crates/graph/src/lib.rs:218 | outer ? then chain | 18a9154 |
| 51 | crates/graph/src/lib.rs:241 | outer ? then chain | 18a9154 |
| 52 | crates/graph/src/lib.rs:279 | outer ? then chain | 8d1b4ef |
| 53 | crates/graph/src/lib.rs:318 | outer ? then chain | 18a9154 |
| 54 | crates/graph/src/lib.rs:367 | outer ? then chain | 2c2a750 |
| 55 | crates/graph/src/lib.rs:529 | both unwrap.unwrap | 18a9154 |
| 56 | crates/graph/tests/empty-recall-pattern.rs:93 | other | f06334c |
| 57 | crates/knowledge/src/files.rs:199 | both ?? | 92aa344 |
| 58 | crates/knowledge/src/files.rs:293 | both ?? | 92aa344 |
| 59 | crates/knowledge/src/files.rs:327 | both ?? | 92aa344 |
| 60 | crates/knowledge/src/files.rs:361 | both ?? | 92aa344 |
| 61 | crates/knowledge/src/files.rs:473 | other | 92aa344 |
| 62 | crates/knowledge/src/files.rs:508 | both ?? | 92aa344 |
| 63 | crates/knowledge/src/files.rs:541 | both ?? | 92aa344 |
| 64 | crates/knowledge/src/files.rs:601 | both ?? | 92aa344 |
| 65 | crates/knowledge/src/files.rs:638 | both ?? | 92aa344 |
| 66 | crates/knowledge/src/files.rs:683 | flatten to default (both dropped) | 92aa344 |
| 67 | crates/knowledge/src/files.rs:711 | both ?? | 92aa344 |
| 68 | crates/knowledge/src/files.rs:730 | both ?? | 92aa344 |
| 69 | crates/knowledge/src/files.rs:754 | other | 92aa344 |
| 70 | crates/knowledge/src/files.rs:781 | statement ends (value dropped) | 46a1e99 |
| 71 | crates/knowledge/src/store.rs:195 | both ? + map_err + ? | 2d23d89 |
| 72 | crates/knowledge/src/store.rs:213 | both ?? | 2d23d89 |
| 73 | crates/knowledge/src/store.rs:254 | both ? + map_err + ? | e2b5ec8 |
| 74 | crates/knowledge/src/store.rs:273 | both ? + map_err + ? | e2b5ec8 |
| 75 | crates/knowledge/src/store.rs:329 | both ? + map_err + ? | 92aa344 |
| 76 | crates/knowledge/src/store.rs:366 | both ? + map_err + ? | 92aa344 |
| 77 | crates/knowledge/src/store.rs:554 | both ? + map_err + ? | 23add4e |
| 78 | crates/knowledge/src/store.rs:586 | both ? + map_err + ? | 23add4e |
| 79 | crates/knowledge/src/store.rs:629 | both ? + map_err + ? | 2d23d89 |
| 80 | crates/knowledge/src/store.rs:686 | other | 8d1b4ef |
| 81 | crates/knowledge/src/store.rs:714 | other | 8d1b4ef |
| 82 | crates/knowledge/src/store.rs:732 | both ?? | 8d1b4ef |
| 83 | crates/knowledge/src/store.rs:742 | both ?? | 8d1b4ef |
| 84 | crates/knowledge/src/store.rs:772 | both ? + map_err + ? | 2d23d89 |
| 85 | crates/knowledge/src/store.rs:871 | both unwrap.unwrap | e2b5ec8 |
| 86 | crates/memory/src/episode.rs:44 | both ?? | 8c07f7a |
| 87 | crates/memory/src/episode.rs:72 | both ?? | 8c07f7a |
| 88 | crates/memory/src/episode.rs:87 | outer ? then chain | 8c07f7a |
| 89 | crates/memory/src/lifecycle.rs:54 | both ?? | f901f93 |
| 90 | crates/memory/src/lifecycle.rs:114 | both ?? | f901f93 |
| 91 | crates/memory/src/lifecycle.rs:179 | both ?? | a33663d |
| 92 | crates/memory/src/query.rs:41 | outer ? then chain | 8c07f7a |
| 93 | crates/memory/src/query.rs:63 | outer ? then chain | 8c07f7a |
| 94 | crates/memory/src/query.rs:95 | outer ? then chain | f901f93 |
| 95 | crates/memory/src/query.rs:121 | outer ? then chain | 8d1b4ef |
| 96 | crates/memory/src/query.rs:151 | outer ? then chain | 8d1b4ef |
| 97 | crates/memory/src/query.rs:178 | outer ? then chain | 8c07f7a |
| 98 | crates/memory/src/query.rs:207 | outer ? then chain | f901f93 |
| 99 | crates/memory/src/query.rs:235 | outer ? then chain | f901f93 |
| 100 | crates/memory/src/write.rs:88 | both ?? | 8c07f7a |
| 101 | crates/memory/src/write.rs:186 | both ?? | 8c07f7a |
| 102 | crates/memory/src/write.rs:243 | both unwrap.unwrap | 8c07f7a |
| 103 | crates/memory/src/write.rs:267 | both unwrap.unwrap | 8c07f7a |
| 104 | crates/mock-agent/tests/chat_experience.rs:162 | statement ends (value dropped) | c3e5f47 |
| 105 | crates/mock-agent/tests/e2e_daemon.rs:2030 | both unwrap.unwrap | 55a7843 |
| 106 | crates/mock-agent/tests/e2e_daemon.rs:2171 | both unwrap.unwrap | e9f5562 |
| 107 | crates/mock-agent/tests/e2e_daemon.rs:2178 | outer unwrap | 55a7843 |
| 108 | crates/mock-agent/tests/e2e_daemon.rs:2231 | both unwrap.unwrap | e9f5562 |
| 109 | crates/store/src/lib.rs:35 | both ?? | 32a385a |
| 110 | crates/store/src/lib.rs:62 | both ?? | 32a385a |
| 111 | crates/store/src/lib.rs:78 | outer ? then chain | 32a385a |
| 112 | crates/store/src/lib.rs:95 | outer ? then chain | 32a385a |
| 113 | crates/store/src/lib.rs:123 | both ?? | 8d1b4ef |
| 114 | crates/store/src/lib.rs:146 | both ?? | 0759f9a |
| 115 | crates/store/src/lib.rs:169 | both ?? | 7501be1 |
| 116 | crates/store/src/lib.rs:197 | both ?? | 32a385a |
| 117 | crates/store/src/lib.rs:213 | outer ? then chain | 32a385a |
| 118 | crates/store/src/lib.rs:234 | both ?? | 32a385a |
| 119 | crates/store/src/lib.rs:267 | both ?? | 32a385a |
| 120 | crates/store/src/lib.rs:294 | outer ? then chain | 32a385a |
| 121 | crates/store/src/lib.rs:315 | outer ? then chain | 32a385a |
| 122 | crates/store/src/lib.rs:332 | outer ? then chain | 32a385a |
| 123 | crates/store/src/lib.rs:366 | outer ? then chain | a3e4600 |
| 124 | crates/store/src/lib.rs:404 | both ?? | 72b7e1c |
| 125 | crates/store/src/lib.rs:427 | both ?? | 72b7e1c |
| 126 | crates/store/src/lib.rs:444 | outer ? then chain | 72b7e1c |
| 127 | crates/store/src/lib.rs:660 | both ?? | 16387e2 |
| 128 | crates/store/src/lib.rs:675 | both ?? | 16387e2 |
| 129 | crates/store/src/lib.rs:682 | other | 16387e2 |
| 130 | crates/store/src/lib.rs:700 | other | 16387e2 |
| 131 | crates/store/src/lib.rs:718 | other | 16387e2 |
| 132 | crates/store/src/lib.rs:743 | other | 16387e2 |
| 133 | crates/store/src/lib.rs:766 | other | 16387e2 |
| 134 | crates/store/src/lib.rs:969 | both unwrap.unwrap | 16387e2 |

## 4 三类相邻缺陷（逐处读过源码）

### F-324a · 只判外层的断言：0 处（历史 1 处）

全仓搜 `.await.is_err()` / `.await.is_ok()`：3 处命中，**没有一处落在 `Db::call` 上** ——

| 位置 | 提交 | 判的是什么 |
|---|---|---|
| `crates/daemon/src/chat.rs:822` | e7105a9 | `watch.changed().await`（tokio watch channel），不是 `Db::call` |
| `crates/knowledge/src/files.rs:1012` | 92aa344 | `kb.save(..).await`（扁平 `Result`，无嵌套） |
| `crates/knowledge/src/files.rs:1085` | 50ea8ce | 同上 |

⇒ **恒假断言这一类今天为空**。唯一的历史实例是 t321 第一版，**在提交前**就被作者发现并改掉；
本单实测 `git log --all -S'.await.is_err()'`：只返回 3 个提交，**正好就是上表那 3 处**（watch channel 与两条 `kb.save` 断言）—— 也就是说，**没有任何提交曾经把 `.is_err()` 用在 `Db::call` 上**；t321 第一版从未进过历史。

### F-324b · `if let Err(e) = db.call(..).await` 只匹配外层：1 处

| 位置 | 提交 | 症状 |
|---|---|---|
| `crates/daemon/src/wiki.rs:968`（`if let Err` 在 980 行） | 46a1e99 | 闭包返回 `Result<(), rusqlite::Error>` ⇒ `done` 是嵌套的。`if let Err(e) = done` 只匹配**外层**：wiki build 收尾那条 `UPDATE wiki_builds SET status='done' …` 若 **SQL 失败**，`tracing::error!` 不会触发，构建被当成收尾成功 |

这是「只判外层」的非断言版本：**不是假绿，是假静默**。

### F-324c · flatten-to-default（两层一起丢，变成默认值）：4 处

形态 `.await.ok().and_then(|r| r.ok()).unwrap_or_default()` —— 两层都被 `.ok()` 抹平成 `Option`，
最后 `unwrap_or_default()` 给一个空值。**读失败与「读到了空」在返回值上不可区分。**

| 位置 | 提交 | 读什么 / 失败后变成什么 |
|---|---|---|
| `crates/daemon/src/api.rs:2465` | 7588cdd | `SELECT id, name FROM entities`（渲染边端点的 id→name 映射）⇒ 空 map |
| `crates/daemon/src/api.rs:2979` | 8e34503 | 同族（recall 响应里的名字映射）⇒ 空 map |
| `crates/daemon/src/memembed.rs:58` | 0551b55 | 记忆嵌入相关的读 ⇒ 空值 |
| `crates/knowledge/src/files.rs:683` | 92aa344 | `chunk_id → content` 映射 ⇒ 空 map |

注意：**这可能是有意的**（渲染路径宁可少个名字也别 500）。问题不在「有默认值」，在于
**失败与空值没有区分**：日志里也没有痕迹。

### F-324d · `let _ = db.call(..).await;`（两层一起丢）：12 处

| 位置 | 提交 | 写什么 |
|---|---|---|
| `crates/daemon/src/api.rs:2695` | 77bc646 | 写一行 recall_log |
| `crates/daemon/src/chat.rs:212` | c3e5f47 | 更新 chats.title |
| `crates/daemon/src/chat.rs:877` | c3e5f47 | 更新 chats 的 engine 身份 |
| `crates/daemon/src/chat.rs:1572` | c3e5f47 | 写 agent_options |
| `crates/daemon/src/memembed.rs:32` | 0551b55 | 写 memories.embedding |
| `crates/daemon/src/sessions.rs:204` | b6678e6 | DELETE sessions |
| `crates/daemon/src/wiki.rs:1698` | 46a1e99 | 更新 wiki_build_pages 状态 |
| `crates/daemon/src/wiki.rs:1712` | 46a1e99 | 更新 wiki_build_pages 字段 |
| `crates/daemon/src/wiki.rs:1722` | 46a1e99 | 把 build 标成 failed |
| `crates/daemon/src/wiki.rs:1759` | 46a1e99 | 写 wiki_page_hashes |
| `crates/daemon/src/wiki.rs:1773` | 46a1e99 | DELETE wiki_page_hashes |
| `crates/daemon/src/wiki.rs:1808` | 9fb480b | 更新 wiki_builds.finished_at |

这些是**刻意的 fire-and-forget**（`let _ =` 显式吞掉两层）。风险不对称：
`recall_log` 写不进去只是少一行证据；而 `wiki.rs:1722`（把 build 标成 failed）写不进去，
会留下一个**永远停在 running 的 build**。

## 5 建议（本单不动接口）

1. **给 `Db` 加一个扁平入口**（新单）：

   ```rust
   pub async fn call_flat<T, F>(&self, f: F) -> Result<T, DbError>
   where F: FnOnce(&mut rusqlite::Connection) -> Result<T, rusqlite::Error> + Send + 'static,
   { self.call(f).await?.map_err(DbError::from) }
   ```

   新代码用它就**不可能**写出嵌套误判；旧调用点可逐步迁移。
2. **给 F-324b 补日志**：`if let Err(e) = done` → 同时匹配内层（例如 `done.and_then(|r| r.map_err(DbError::from))`）。
3. **给 flatten-to-default 与 `let _ =` 补痕迹**：至少一行 `tracing::warn!`，让「失败」与「空」可区分；
   `wiki.rs:1722` 这种状态机收尾建议直接 `?` 或重试。
4. **机械守卫**：把 `.call(..).await.is_err()` / `.is_ok()` 加进 CI 的 grep 清单，
   或评估 clippy 的 `let_underscore_must_use`（噪声需单独评估）。
5. **接口不动**：`DbError` 已经能承载 `rusqlite::Error`（`DbError::Sqlite(#[from] rusqlite::Error)`），
   真正缺的是「一个不让调用者误判层次」的入口 —— 见 1。

## 6 方法与复现

- 清单提取：`git ls-files '*.rs'` ⇒ 逐文件正则找 `.call(`，再按括号配平取整条消费链。
- 提交归属：每个文件跑一次 `git blame --porcelain`，取该行 commit（附录与 §4 表格里的短 hash）。
- 真跑证明：`C:/tmp/t324/probe/`（独立 target dir）⇒ `cargo run`，输出见 §2。
- 只读保证：本单未修改 `crates/`、`cli/`、`panel/`、`scripts/` 下任何文件；探针在仓库外。
- 已知局限：§3 的分类是**正则读法**，「闭包是否返回 Result」没有用类型信息验证 ⇒
  类别名描述的是**消费形态**而非**类型结论**；§4 的四类是我逐处读过源码的。

