# 命名空间写词表：`global` 的冲突收口（t52 / mem-core）

- 任务：t52（kind=implementation）；attempt `764b24b8-66b4-4cfd-a9db-97a083c4f845`
- inScope（stored）：`crates/memory/` · `docs/design/reviews/gen2-memory-spec.md` · 本报告
- outOfScope（未触碰）：`crates/knowledge/` · `crates/graph/` · `crates/store/` · `crates/daemon/src/wiki.rs` · `panel/` · `cli/`（另：**`crates/daemon/src/api.rs` 未改**，所以没有触发「第 16 条」的 daemon clippy 门——我仍额外跑了 `check -p ruagent-daemon --all-targets` 作为第二件证据）
- 纪律：临时 root + 临时端口自测；**未写活库、未启停 pid 79984、未用 `/api/v1/recall`**

## 1 先复现（我自己的两次调用）

方法：`ruagent serve --addr 127.0.0.1:<临时端口> --root %TEMP%\ruagent-t52*`（**不是** 8787，**不是** `~/.ruagent`），脚本 `%TEMP%\ruagent-rb\t52_probe2.ps1`，取原始响应体。

**改前（共享 target 里现成的 exe，`ruagent-team-target\debug\ruagent.exe`，未含本单改动）**

| (store, namespace) | HTTP | 响应体（逐字） |
| --- | --- | --- |
| `observation` × `global` | 200 | `{"outcome":"RejectedNamespace"}` ← **复现 wiki 的读数** |
| `observation` × `user` | 200 | `{"outcome":"Inserted(1)"}` |
| `procedure` × `global` | 200 | `{"outcome":"Inserted(2)"}` |
| `lesson` × `global` | 200 | `{"outcome":"Inserted(3)"}` |
| `observation` × `project:t52` | 200 | `{"outcome":"Inserted(4)"}` |
| `observation` × `agent:x` | 200 | `{"outcome":"Inserted(5)"}` |
| `profile` × `global` | 200 | `{"outcome":"RejectedNamespace"}` |
| `profile` × `user` | 200 | `{"outcome":"Inserted(6)"}` |
| `bogus` store | **400** | `unknown store \`bogus\` (observation \| profile \| procedure \| lesson)` |
| `bogus` namespace | **400** | `invalid namespace \`bogus\` (user \| global \| project:<x> \| agent:<x>)` |

同一次运行的临时库回读（`%TEMP%\ruagent-rb\t52_db.py`）：

```
memories: (1,observation,user) (2,procedure,global) (3,lesson,global)
          (4,observation,project:t52) (5,observation,agent:x) (6,profile,user)
memory_diffs op='reject': (observation, global, "store `observation` cannot write namespace `global`")
                          (profile,     global, "store `profile` cannot write namespace `global`")
episodes by kind: [('mcp_write', 8)]
```

**结论（这正是 finding 的真相）**：`user | global | project:<x> | agent:<x>` 是**语法（可解析）词表**，不是**可写词表**。`global` 既不是「普遍支持」也不是「不支持」—— 它在 `procedure`/`lesson` **写入成功**（上表 2/3 行），在 `observation`/`profile` 被治理规则拒绝。写侧真正的词表是**矩阵**：4 store × 4 命名空间种类 = 16 格里 **8 格可写、8 格被拒**。

## 2 裁决：选 (b)，不实现 `observation × global`

**理由**
1. `global` 的语义是**跨项目共享**，设计 §6.1 只把它给 `procedure`/`lesson`（蒸馏出来的「怎么做」与「教训」）；Observation 是「非结构化观察」，un 让它写 global 会**静默改变共享语义**（观察项从此跨项目对所有 agent 可见）—— 这是一次治理变更，而 finding 里没有任何需求要求它。
2. 调用方（面板下拉）提供的 `observation × global` 是**面板自己的矩阵写错了**（`panel/src/views/Memory.tsx:35-40` 写的是 `observation: ["user","global"]`），修面板的真值比放宽存储层治理更对症，且不改变数据可见性。
3. 规格/代码里早有一条把 `observation/global` 称为**死组**的记录（规格 **§D.5** 第 794 行「chat 的 `observation/global` 组是**死组**（Observation 不能写 global，`lib.rs:54-67`）」）—— 也就是说**实现本来就是这么设计的**，出问题的是**声明**。

**影响面**
- 治理矩阵、`write_memory` 的判据、可见性与注入选择：**一律不变**（无行为回归风险）。
- 变化的是**声明**：规格新增写词表矩阵（一个真相源），`crates/memory` 把矩阵从 `match` 提升为**数据**，拒绝响应改为**可辨**。
- 不改的：`api.rs`（INT/t19）· `0004_memory.sql`（I-SCHEMA/store）· `panel/`（面板属主）—— 三处的声明与面板矩阵作为 finding **路由**，见 §5。

## 3 改前 → 改后读数

**(a) HTTP 面（线格式）**：为拿到「改后」的真读数，我用**私有 target 目录**单独构建了一个二进制（`%TEMP%\ruagent-t52-target`）：共享 target 里的 `ruagent.exe` 被**真守护进程 pid 79984 占用**，cargo 无法重新链接（报 `failed to remove file ... ruagent.exe`）—— 我不停真守护进程，所以换私有目录。同一脚本、同一组请求：

| (store, namespace) | 改前 | 改后 |
| --- | --- | --- |
| `observation` × `global` | `{"outcome":"RejectedNamespace"}` | `{"outcome":"RejectedNamespace (this store does not allow that namespace; write vocabulary = profile×{user} \| observation×{user,project:<x>,agent:<x>} \| procedure×{project:<x>,global} \| lesson×{project:<x>,global}; the refused pair is recorded in the write audit as op='reject')"}` |
| `profile` × `global` | `{"outcome":"RejectedNamespace"}` | 同上（**同一句话**，因为这串是静态可写集合，不是 per-request 细节） |
| `observation` × `user` / `procedure` × `global` / `lesson` × `global` / `observation` × `project:t52` / `observation` × `agent:x` / `profile` × `user` | `Inserted(1..6)` | **逐字节不变** `Inserted(1..6)` |
| `bogus` store / `bogus` namespace | 400 原文 | **逐字节不变** |

「哪一对被拒」不在响应里（变体**故意不带字段**：`distill.rs:763` 按 `W::RejectedNamespace` 匹配，加字段会让它编译不过），而在审计行 `memory_diffs.op='reject'` 的 `reason` 里（§1 已给读数）。

**(b) crate 面**（`cargo test -p ruagent-memory`，含 `--nocapture` 的读数）

```
READING t52 write vocabulary: profile×{user} | observation×{user,project:<x>,agent:<x>} | procedure×{project:<x>,global} | lesson×{project:<x>,global}
READING t52 allowed pairs (8): ["profile×user","observation×user","observation×project:<x>","observation×agent:<x>","procedure×global","procedure×project:<x>","lesson×global","lesson×project:<x>"]
READING t52 refused pairs (8): ["profile×global","profile×project:<x>","profile×agent:<x>","observation×global","procedure×user","procedure×agent:<x>","lesson×user","lesson×agent:<x>"]
READING t52 write procedure×global -> Inserted(1) | lesson×global -> Inserted(2)
READING t52 write observation×global -> RejectedNamespace (…)
READING t52 write profile×global -> RejectedNamespace (…)
READING t52 reject audit reasons: ["store `observation` cannot write namespace `global`", "store `profile` cannot write namespace `global`"]
```

三条新测试（`crates/memory/src/write.rs`）：
1. `the_published_vocabulary_and_the_enforcement_agree_on_all_sixteen_pairs` —— 16 格里逐格断言 `allows_namespace(store, ns) == 渲染串里那一格`，并用 `ns.kind().as_str()` 取拼写 ⇒ **矩阵与执行不可能再漂移**（改前：两者分别写在 `match` 与注释里，没有任何断言连起来）。
2. `global_is_writable_where_the_global_scope_lives_and_refused_elsewhere` —— 4 格边界（两个可写 + 两个拒绝）+ 拒绝必须留审计。
3. `the_refusal_names_the_supported_values_and_the_other_strings_are_frozen` —— 首个 token 仍是 `RejectedNamespace`（面板 `outcomeKey` 映射不变）、串里含新词表、**不含**扁平列表式的 `observation×{user,global`；同时**逐字节冻结**另外三个正例串 `Inserted(7)` / `Superseded { old: 1, new: 2 }` / `SkippedDuplicate(7)`。

**边界**：
- `project:<x>` / `agent:<x>` 的 `<x>` 为空 ⇒ `Namespace::parse` 回 `None` ⇒ 400（既有行为，未改）。
- `global` 的**可写性取决于 store**，不取决于大小写/空格（`parse` 会 `trim`，既有行为）。
- 未知 namespace ⇒ 400（不是 200+outcome）；未知 store ⇒ 400（t19 已交付）。

## 4 这一条属于哪一族

**「声称的词表 vs 实际的词表」**，与 **t30「冻结字面量必须与序列化输出同源」同族**：两者的病都是**同一个事实有两个来源**（本例：语法词表与写矩阵；t30：字面量与序列化输出），治法也一样 —— **收敛到一个来源，并让断言（而不是注释）守住它**。本单把矩阵收敛成 `MemoryStore::allowed_kinds()`（数据）→ `allows_namespace()`（执行）+ `write_vocabulary()`（对外字符串，进拒绝响应），三处同一个来源。

先例（任务书提到）也同源：`RUAGENT_REQUIRE_MOCK` 那个从未被启用的严格开关 —— **文档/代码声称的能力 vs 实际的能力**。

## 5 路由给其它属主（只报不改；坐标 + 要求）

| # | 位置（不在本单 inScope） | 问题 | 要求 |
| --- | --- | --- | --- |
| R-1 | `crates/daemon/src/api.rs:4234`（`MemoryWriteRequest` 的 doc 注释：`/// user \| global \| project:<x> \| agent:<x>`） | 把**语法**词表读成**可写**词表，正是 finding 的来源之一 | 改为指向 `MemoryStore::allowed_kinds()` / `write_vocabulary()`，或至少写明「可写性取决于 store」 |
| R-2 | `crates/store/src/migrations/0004_memory.sql:21`（`-- user \| global \| project:<x> \| agent:<x>`） | 同上（它描述**列能存的值**，不是写侧允许的组合） | 同上；建议注明「列值域 ≠ 写词表」 |
| R-3 | `panel/src/views/Memory.tsx:35-40` `NAMESPACES = { profile:["user"], observation:["user","global"], procedure:["global"], lesson:["global"] }` | **面板矩阵错**：`observation` 里出现 `global` ⇒ 用户会被引导选到必定被拒的组合；四处都缺 `project:<x>`/`agent:<x>` | 改成真值；最好由 API 下发（见 R-5），否则又是一处手工第二来源 |
| R-4 | `api.rs:4267-4273`：`record_episode(McpWrite)` 在 `write_memory` 的治理检查**之前** | 被拒的写入**仍留下 episode**（实测 10 次请求 = 8 个 `mcp_write` episode，含 2 次被拒）⇒ **episode 数不能当「写入发生过」的代理** | 与 t41 §4 的通则同族（「任何『发生过』的代理必须能区分状态」）：要么把 episode 记录挪到治理检查之后，要么在 episode 上标明被拒 |
| R-5 | `api.rs:4298` `format!("{outcome:?}")`（Debug 串当线格式） | 结构化信息被塞进一个字符串；本次已让它可辨，但仍不是结构化字段 | 后续把拒绝序列化为字段（`store`/`namespace`/`allowed`）或 400；**唯一来源已备好**：`crates/memory` 的 `write_vocabulary()` / `allowed_kinds()` |

面板侧一个**已核对的好消息**：`memory.outcome.rejectedNamespace` 的 i18n key 在 `panel/src/i18n/memory.ts:76`（zh）与 `:156`（en）都存在，且 `outcomeKey` 取首 token ⇒ 我的串变化**不影响面板映射**（不需要改面板）。

## 6 门禁与本单读数

| 命令 | 读数 |
| --- | --- |
| `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 test -p ruagent-memory` | **exit=0** · `62 passed; 0 failed; 0 ignored`（3.64s）+ doctest 0/0（t41 后 59 → +3 条新测试） |
| `... clippy -p ruagent-memory --all-targets -DenyWarnings -CleanFirst ruagent-memory` | **exit=0** · 输出含本轮 `Checking ruagent-memory v0.1.0`（同锁窗口 clean+gate ⇒ 真重查），零诊断 |
| （额外，不是第 16 条要求）`... check -p ruagent-daemon --all-targets` | **exit=0** · `Checking ruagent-daemon v0.1.0` ⇒ 记忆侧的新增公共面（`allowed_kinds`/`ALL`/`write_vocabulary`/`NamespaceKind`）与同伴文件（含 `distill.rs` 对 `RejectedNamespace` 的匹配）仍一起编译 |

**越界证明**：本单改动仅 `crates/memory/src/{lib.rs,namespace.rs,write.rs}` + 规格追加段 + 本报告；`git status --porcelain` 里 `crates/store`/`crates/knowledge`/`crates/graph`/`daemon/src/api.rs`/`wiki.rs`/`distill.rs`/`panel` 的 ` M` 全是**同伴在途窗口**，没有一条来自我；新符号（`allowed_kinds`、`write_vocabulary`、`NamespaceKind`、`namespace×{`）在全仓扫描下只出现在 `crates/memory/` 与 `docs/design/reviews/gen2-memory-*` 内。

## 7 未做 / 边界（无静默跳过）

- 未改 `api.rs`（R-1/R-4/R-5 的落点归 INT/t19）· 未改 `0004_memory.sql`（归 I-SCHEMA/store）· 未改 `panel/`（归面板属主）。
- **未写活库**：所有写入都发生在 `%TEMP%\ruagent-t52{b,c,d}` 三个临时 root；真 daemon pid 79984 全程在跑、未启停；未调用 `/api/v1/recall`。
- 我起的临时 daemon 都记录了 PID 并在脚本 `finally` 里 `Stop-Process`：18787/`77792`（**超时被杀，不是脚本收尾**，之后我按 AGENTS.md 的规矩用「命令行匹配自己的临时路径」复查过孤儿，0 条）· 18788/`66920` · 18789/`83348`（那次的 exe 是旧的：共享 target 被占用无法重链接，读数与改前相同，已在 §3 说明）· 18790/`30960`。收尾复查：匹配 `ruagent-t52` 的 `ruagent.exe` 进程 0 条。
- 私有 target 目录 `%TEMP%\ruagent-t52-target`（约 6m22s 构建，protoc 需在 PATH 上）留在盘上备复核；需要清理时它是我自己的目录，可删。
- 规格追加段的**旧文字逐字引用**：finding 原文（§1 引文）+ 规格 **§D.5**（第 794 行）「死组」原句；`user | global | project:<x> | agent:<x>` 的**实际位置**已写明是 `api.rs:4234` / `0004_memory.sql:21`（**不在** R-B 规格正文里）—— 我不假装改了不在该文件里的字面，而是**把缺的矩阵补进去**并登记那两处的路由。
