# 架构 / 简化审计（issues 56–59 轮）

> 本文只做**审计**：给出「同一事实的第二份定义」与「重复造轮子」两类可优化点，按 **代价（改动面 × 风险）÷ 收益** 排序。
> **不改任何源码**（本轮 in scope 只有本文件）。每条都给 file:line、≥2 个实例、不修的具体后果、最小修法、影响面，以及**为什么不是无证据的重写**。
> 取证方式全部是可复现的 grep（见文末「复现命令」），没有一条来自印象或推测。

## 0. 排序表

| # | 类别 | 一句话 | 实例数 | 代价 | 收益 |
| --- | --- | --- | --- | --- | --- |
| F1 | 第二份定义 | `distill_log` 的列集合被写死 4 处（+ 位置参数镜像） | 5 | 中（2 文件 + 1 测试；风险=拼 SQL） | 高（t21 加一列踩过全场） |
| F2 | 第二份定义 | distill 状态词表 Rust 内 3 份，且用**下标**取 not_recorded | 3+2 | 低 | 高（下标=词表顺序变成语义） |
| F3 | 造轮子 | 手搓临时根 112 处，而 `tempfile` 已在依赖图里 | 112 | 低（只对新代码生效） | 高（实测 4840+66 个残留目录） |
| F4 | 第二份定义 | 「pid + 启动时刻」身份有两个 struct、两处比较 | 2 | 低 | 中高（t28 的平台语义必须同时命中两条路径） |
| F5 | 第二份定义 | 退出原因词表 Rust 一份、PowerShell 一份，脚本在解释它 | 2 | 低（但 .github/scripts 本轮 out of scope） | 中高（优雅退出 vs 被杀 的判据） |
| F6 | 第二份定义 | 工具链 pin 有 3 个载体，其中一个是 float | 3 | 低 | 中（CI 与 audit job 可以跑不同工具链） |
| F7 | 第二份定义 | 双倍轮询常量逐字重复两份 + 第三处同形循环 | 3 | 低 | 中（t24 实测两处必须同步） |
| F8 | 第二份定义 | 路由字面量跨语言：api.rs 240 次 vs panel 113 次 | 2 语言 | 中（只能加守卫） | 中（改路由=运行期 404，无编译期保护） |
| F9 | 造轮子 | 定位 mock agent 二进制有两套规则，一套靠猜 | 2 | 低 | 中（t28 实测要先手动 build 才不假红） |
| F10 | 造轮子 | `SystemTime::now()` 手搓 25 处，而仓里已有 `pub fn now_unix_ms()` | 25 | 极低（只提示新代码） | 低 |

「证据不足、故不建议动」的 8 个方向在 §11——它们**没有被计入**上面的排序，因为我没有拿到足以支撑改动的证据。

---

## F1　`distill_log` 的列集合被写死 5 处（其中一处是位置参数镜像）

**事实**：账本表的列名清单是一个事实，它在下列位置各自写了一遍：

1. `crates/daemon/src/api.rs:4575` — `const DISTILL_LOG_COLUMNS: &str = "id, session_key, distilled_at, status, failure_reason, agent, ..."`（读端 SELECT 用）
2. `crates/daemon/src/distill.rs:1268-1274` — INSERT 的列清单（19 列），紧随其后的 `VALUES (?1,?2,...)` 把**同一集合的顺序**再镜像一遍
3. `crates/store/src/migrations.rs:1143` — 测试里手打的第三份列清单（`windows, truncated, memories_refused)`）
4. `crates/store/src/migrations.rs:709` — 迁移 census 的 `("distill_log", "memories_refused")`；同文件 `:624`、`:1017-1021`、`:1109` 又各出现一次列名
5. `panel/src/api.ts` 的行类型（`from_seq` / `to_seq` / `total_messages` 等，:356 一带）——跨语言第五份

**不修会怎样（具体）**：本轮 t21 为「拒绝留痕」加一列 `memories_refused`，必须同时改 ① `crates/store/src/migrations/0031_distill_refused.sql:58` 的 ALTER、② `migrations.rs` 的 census 与测试、③ `distill.rs` 的 INSERT 与 `?N` 序号、④ `api.rs` 的 `DISTILL_LOG_COLUMNS`、⑤ 读端结构体、⑥ panel 类型、⑦ CLI 计数。**漏一处不是编译错误**：列名写错会变成 SQL 运行期 `no such column`，写对但顺序错会静默串列（`?N` 是位置绑定的），读端漏字段则表现为计数恒为 0。本轮还实测过一次同形现象：`ruagent-store` 的 rlib 里缺列、源码里却有，`cargo check` 是绿的（AGENTS.md 的 `cargo clean -p` 条目）——说明这类事实一旦分散，连「谁的读数是权威」都会变模糊。

**最小修法**：把 1 与 2 收敛成一份 `pub const DISTILL_LOG_COLUMNS: &[&str]`，INSERT 用 `format!("INSERT INTO distill_log ({}) VALUES (...)", cols.join(", "))` 生成列名部分；再补一条 `#[test]`（形态照 `migrations.rs` 已有的 census 测试）断言 INSERT 语句包含 const 里的每一个列名。
**影响面**：`distill.rs` + `api.rs` + 1 条测试；`api.rs` 已有 3 个读端测试（:4392 默认值、:4638 by_status 桶、:4771 校验 400）语义不变；迁移与 schema 不动。
**为什么不是重写**：不新增抽象层（就是「一个 const + 一条测试」），不碰 schema/迁移/读端结构，只把同一份列名从 2 个 Rust 副本变成 1 个。风险点是拼字符串进 SQL，所以最小修法**自带**守卫测试。

## F2　distill 状态词表：Rust 内 3 份，且用下标取「未记录」

**事实**：

1. `crates/daemon/src/api.rs:4546` — `const DISTILL_LOG_STATUSES: [&str; 4] = ["ok", "empty", "failed", "not_recorded"]`
2. `crates/daemon/src/api.rs:4638` — 同 4 个字面量第二次出现（by_status 的桶清单）
3. `crates/daemon/src/api.rs:4392` — `status.unwrap_or_else(|| DISTILL_LOG_STATUSES[3].to_string())`：**用下标 3** 取 not_recorded，于是「词表顺序」变成了语义
4. `crates/daemon/src/api.rs:4112 / 4174 / 4198` — 批量路由里再手打 `"empty"` 作为桶名与计数键
5. 跨语言：`cli/src/main.rs:1041-1048`（`tokens_used_not_recorded`、`[not_recorded]` 文案）、`panel/src/api.ts:313`（`"ok" | "empty" | "failed"`）与 `:378/:405`（`RefusedFacet` / `RefusedState` 又一套 4 值联合）

**不修会怎样（具体）**：本轮刻意**不加第 5 个状态词**（改用加一列 `memories_refused` 表达），所以风险不在「加词」而在 `[3]`：把词表按字母序整理（最自然的重构）会让 `:4392` 的默认值、`:4638` 的桶清单、`:4771` 的合法性校验指向不同的词，而**没有任何一处会报错**——它会静默把「未记录」变成别的语义。同时 `4638` 与 `4546` 是两份名单，改一份不会带动另一份（:4771 只认 `4546`）。

**最小修法**：把第 4 项单独命名（`const NOT_RECORDED: &str = "not_recorded";` 并让词表引用它），`:4392` 改用 `NOT_RECORDED`；`:4638` 从 `4546` 派生；`:4112/4174/4198` 的桶名同样引用常量。跨语言不做 codegen（见 §11.6）。
**影响面**：`api.rs` 内 3 条测试（:4392 / :4638 / :4771）与 :10487 的断言，语义不变；`cli`/`panel` 的字符串输出不变（`cargo test -p ruagent-daemon` + CLI 既有测试即可覆盖）。
**为什么不是重写**：常量重命名 + 三处引用，没有新类型、没有新模块、没有跨语言生成。

## F3　手搓临时根 112 处，而 `tempfile` 已经在依赖图里

**事实**：

1. `crates/daemon/src/chat.rs:2223 / 2342 / 2460 / 2665 / 2766 / 2887`（同一文件 6 处 `std::env::temp_dir()`）
2. `crates/daemon/src/api.rs`、`distill.rs`、`sessions.rs`、`skills.rs`、`registry.rs`、`lib.rs`、`runs.rs`、`extract_plane.rs`、`knowledge_graph.rs`、`acp/src/fs_tools.rs`、`mock-agent/tests/e2e_daemon.rs`（同形调用；合计 **112** 处）
3. `Cargo.lock:7231` — `name = "tempfile"` **已在依赖图里**（是传递依赖：`grep -rn tempfile crates/*/Cargo.toml cli/Cargo.toml Cargo.toml` 无命中）

**不修会怎样（具体，全部是本轮实测）**：每个手搓根都要各自决定「删不掉怎么办」，于是同一类 bug 被写了 112 遍——t27 量到 `%TEMP%\ruagent-e2e-*` **4840 个**、`ruagent-t16-*` **66 个**；修复时发现清理失败是被 `let _ = remove_dir_all(...)` **吞掉**的（所以没人知道）；删除语义还是平台相关的（Windows 上测试进程持有那个根，`remove_dir_all` 会 `os error 32`，同一根在进程退出后立刻可删）。这两条我都实测过并写进了 t27 的报告。`tempfile::TempDir` 恰好把「唯一名字 + drop 时清理 + 失败可报告」这三件事一次给全。

**最小修法**：`[dev-dependencies] tempfile = "3"`（一行，按 lock 里已有的版本约束写），**只对新写的临时根**用 `TempDir`；存量 112 处不动。
**影响面**：新增一行依赖 + 新测试的写法；生产代码路径不受影响（112 处里绝大多数是 `#[cfg(test)]`）。
**为什么不是重写**：**明确拒绝**「一次性替换 112 处」——那会把一个可审的改动变成不可审的 diff，也会把已经修好的清理语义重新置于风险中。这里的收益来自「不再新增第 113 处」，不是来自搬完存量。

## F4　「pid + 启动时刻」这一个身份事实有两个 struct、两处比较

**事实**：

1. `crates/daemon/src/orphans.rs:24-28` — `pub struct PriorDaemon { pub pid, pub started_at_ms }`（上次守护进程的记录），读写在 `:38 read_prior` / `:58-64 write_current`
2. `crates/daemon/src/orphans.rs:234-240` — `ChildProcess`/`ChildRef { pid, started_at_ms }`（本轮 t16/t24 的子进程身份），判定在 `:245 is_still_this_process`
3. 两处各自做**同一个比较**：`:85-86` `process_started_at_ms(prior.pid) == Some(prior.started_at_ms)` 与 `:245` `process_started_at_ms(self.pid) == Some(self.started_at_ms)`

**不修会怎样（具体）**：这个比较的语义在本轮刚刚改过一次——t28 量到「被 KILL 但未 reap 的子进程在 Linux 上是僵尸（`/proc` 条目仍在，state=Z），于是仍被算作活着」，修正落在 `process_started_at_ms` 里，**两条路径是碰巧一起变对的**。如果哪天有人在 boot-sweep 路径上就地计算（它已经有一点分叉：`:64` 用 `unwrap_or(0)` 写哨兵值，于是 `Some(0)` 既可能表示「启动时刻真的是 0」也可能表示「读不到」），两条路径就会分叉，而分叉的后果是「残留子进程清理」与「残留守护进程清理」对同一事实给出不同答案。

**最小修法**：让 `PriorDaemon` 直接持有 `ChildRef`（同一个 struct，不新增类型），或把比较收敛为 `ChildRef` 上的一个方法；`write_current` 对 `None` 显式处理，不写哨兵 0。
**影响面**：`orphans.rs` 的 boot-sweep 测试 + `daemon/src/lib.rs` 的调用点；t16/t24 的子进程测试不动。
**为什么不是重写**：不引入新抽象——两个 struct 合并成一个，比较只剩一处。

## F5　退出原因词表：Rust 一份、PowerShell 一份，而且脚本在解释它

**事实**：

1. `crates/daemon/src/lib.rs:202-203` — 词表的**唯一权威说明**（`graceful, stop-request, signal:console, panic, error, server-returned`），写入点 `:711 server-returned` / `:758 signal:console` / `:763 stop-request` / `:850`
2. `scripts/ruagent-daemon.ps1:39` — 同一词表第二次写死（注释里再列一遍），`:195` 起脚本按 `stop-request` 的有无与内容判定「有记录 / 死无记录」

**不修会怎样（具体）**：新增一个原因（`lib.rs:202` 自己就先把 `graceful` 列进了词表，但 grep 不到任何写入点——这说明词表与实现已经不同步）时，脚本的解释分支不会跟着变：`status` 会把新原因一律当成「有记录」，于是「优雅退出」与「被强制杀」的判据（AGENTS.md 里 `DIED WITHOUT A TRACE` 那条正是靠这个区分）悄悄退化，且**不会有测试变红**。
**最小修法**：不新增机制——脚本其实不需要枚举原因，它只要区分「有 exit 记录 / 无记录」；把 ps1 里那份枚举降级为「存在任意 `exit reason=` 即视为有记录，`stop-request` 单独说明」，词表的解释留在 Rust 侧（`status` 直接打印 Rust 写下的整行）。
**影响面**：`scripts/ruagent-daemon.ps1`（本轮 out of scope，仅记录）+ `lib.rs` 的 exit_trace 测试（:860/:918 断言字面量，不受影响）。
**为什么不是重写**：这不是引入「原因注册表」，而是**删掉**一份重复的枚举。

## F6　工具链 pin 有 3 个载体

**事实**：

1. `rust-toolchain.toml:31` — `channel = "1.95.0"`，且 `:15` 自陈「两个载体，必须同时改」
2. `.github/workflows/ci.yml:52-58` — 同一 action 的 `toolchain:` input（注释解释 action 从 @rev 选，input 只影响 runner 的 INSTALL）
3. `.github/workflows/audit.yml:112` — 同一个 action **但注释明说 `the TOOLCHAIN still floats`**

**不修会怎样（具体）**：`Rust (ubuntu/windows)` 用 1.95.0，而 audit job 用浮动 stable——于是「同一个提交在 CI 绿、在 audit 红」可以纯粹由工具链差异制造，而这条弯路在仓里已有前例（AGENTS.md 记录的「float stable 让零 diff 提交变红」，以及 `rust-toolchain.toml:15` 那条「两个载体必须保持相等」的说明正是被这么写下来的）。
**最小修法**：audit.yml 与 ci.yml 用同一个 action SHA 且都补 `toolchain: 1.95.0`（或让 audit 读 `rust-toolchain.toml`）——复制一个值，不是新机制。
**影响面**：CI job 定义（本轮 out of scope，仅记录）。
**为什么不是重写**：pin 的值本来就该只有一处权威；改动是「让第二个/第三个读它」。

## F7　双倍轮询的常量逐字重复两份 + 第三处同形循环

**事实**：

1. `crates/daemon/src/chat.rs:450-451` — `ATTRIBUTION_FIRST_POLL_MS = 20` / `ATTRIBUTION_MAX_POLL_MS = 250`
2. `crates/daemon/src/runs.rs:1356-1357` — `RUN_CHILD_FIRST_POLL_MS = 20` / `RUN_CHILD_MAX_POLL_MS = 250`（**数值逐字相同**），循环体同形：`chat.rs:1233/1268` 对 `runs.rs:1532`（`wait_ms = (wait_ms * 2).min(MAX)`）
3. `crates/daemon/src/orphans.rs:215-216` — `EXIT_GRACE = 1000ms` / `EXIT_POLL = 25ms`：同一个「等到条件成立或到点」的**第三种形态**（固定间隔）

**不修会怎样（具体）**：t24 把归因轮询从固定 3 s 降到 20→250 ms 双倍（实测窗口开销 8760 ms → 100 ms），两个调用点必须同步改；只改一个的后果是「一条路径快、一条路径慢」，而且因为两条路径各自的超时都够用，**不会有任何测试变红**——只在负载机上表现为偶发超时或假失败（这正是 t24 要修的那类形状）。数值逐字相同是这条最强的证据：它不可能是两次独立选择。
**最小修法**：两组常量合并为一组（例如 `POLL_FIRST_MS` / `POLL_MAX_MS`），两个调用点引用它。**不抽**「poll-until-deadline」抽象：三处的退出条件不同（找到即返回 / 消失即返回 / 到点即返回），抽出来要传闭包，是把 6 行换成一层间接（YAGNI）。
**影响面**：0 条断言变化（常量重命名）；可选加一条断言两组数值相等的测试。
**为什么不是重写**：合并两个常量、改两处引用。

## F8　路由字面量跨语言两份（daemon 240 次 vs panel 113 次）

**事实**：

1. `crates/daemon/src/api.rs` — `"/api/v1` 出现 **240** 次（路由注册 + 文档表 + 测试）
2. `panel/src/api.ts` — `/api/v1` 出现 **113** 次；另有 `panel/e2e/*.spec.ts` 各自拼路径（本轮刚修过它入口的健壮性）
3. 同形小例子在同一事实里成对出现：`panel/src/api.ts:378 RefusedFacet = "only" | "none" | "not_recorded"` 与 `:405 RefusedState = "refused_all" | "refused_part" | "none" | "not_recorded"`——两侧各自定义拒绝口径的取值

**不修会怎样（具体）**：daemon 侧改一个路由或字段名（例如 t21 的 `?refused=` 与 `refused[]`），panel **不会编译错**，它在运行期 404 或读到 `undefined`；唯一的守卫是面板 e2e（另一个 gate，且只在 `npm run test:e2e` 这条入口里跑）。也就是说：类型系统在这里给不了任何保护，而两侧的字符串加起来 353 处。
**最小修法**：**不生成客户端**（那是重写）——加一条守卫测试：取 `panel/src/api.ts` 中的路径字面量集合，断言每个都能在 `api.rs` 的路由字面量里找到（形态照仓里已有的 `panel/e2e/i18n-check.mjs` 那种跨文件一致性检查）。
**影响面**：新增 1 个守卫；不改任何响应形态与路由本身。
**为什么不是重写**：不引入 codegen、不引入契约文件、不改任何一端的结构。需要说明的是：**我没有**把 240 处逐一分类成「注册 / 文档 / 测试」，所以这里的修法刻意写成「守卫」而不是「提取常量表」——在没分类的前提下声称能收敛是不诚实的。

## F9　定位 mock agent 二进制有两套规则，其中一套靠猜

**事实**：

1. `crates/daemon/src/chat.rs:2435-2443` — `fn mock_agent()`：从 `current_exe()` 往上找 target 目录，再依次试 `ruagent-mock-agent.exe` / `ruagent-mock-agent`，找不到返回 `None`，调用点以 `mock agent binary not built next to the test exe` panic
2. `crates/mock-agent/tests/e2e_daemon.rs:144-146` — `fn mock_bin()`：用 `env!("CARGO_BIN_EXE_ruagent-mock-agent")`（cargo 注入的权威路径）再复制成每个测试唯一的名字（t24 的映像名归属依赖这个唯一性）

**不修会怎样（具体，t28 实测）**：在 Linux 容器里跑 daemon 的 chat 测试，必须先 `cargo build -p ruagent-mock-agent --bins`，否则相关测试全部以那句 panic 假红（我第一次跑就踩到：`--lib chat::generating_tests::a_` 3 条测试全红，消息是 `mock agent binary not built next to the test exe`，而不是任何真实缺陷）。同一事实、两种取法：一种有 cargo 保证，一种是路径猜测，于是只有走猜的那一侧会以「环境没准备好」的形状失败。
**最小修法**：不建共享抽象（`CARGO_BIN_EXE_*` 只在**同 crate 的测试**里可用，所以 chat.rs 用不了它）——给该查找加一条 workspace `target/{debug,release}` 兜底，并把 panic 文案改成「先跑 `cargo build -p ruagent-mock-agent --bins`」的可操作指引。
**影响面**：`chat.rs` 的测试辅助函数；生产路径（`ChatManager` 的 spawn）不动。
**为什么不是重写**：跨 crate 共享一个查找器需要新建 dev-only 模块（新抽象）；3 行兜底 + 一句文案的性价比更高。

## F10（最低优先）　`SystemTime::now()` 手搓 25 处，而仓里已有 `pub fn now_unix_ms()`

**事实**：

1. `crates/daemon/src/orphans.rs:364-365` — `pub fn now_unix_ms()`（仓里**已经有**这个函数，且是 pub）
2. 另有 **25** 处直接 `SystemTime::now()`：`api.rs:7774`、`chat.rs:2345 / 2463 / 2668`、`mock-agent/tests/e2e_daemon.rs:1689`，以及 `:116 fn chrono_tag()` 这第三套「造唯一后缀」的写法

**不修会怎样（具体）**：程度有限——这 25 处里相当一部分要的不是「当前时刻」而是「保证唯一的名字后缀」，语义并不相同；真正的代价是每个新写的测试都要重新读一遍「时间是取 ms 还是取后缀」，t27/t28 里我每次都重新确认了一次。
**最小修法**：新代码调用现成的 `now_unix_ms()`；**明确不动**存量 25 处（它们的语义不是同一个）。
**影响面**：无（只是约定）。
**为什么不是重写**：这一条我刻意评为最低，因为它**不能**机械统一——把「造唯一名字」也换成 `now_unix_ms()` 会引入「同一毫秒内两次调用同名」的新缺陷。

---

## 11. 证据不足、故**不建议**动的方向（不凑数）

1. **`libc` → `nix`**：实例只有 `crates/daemon/src/orphans.rs:129-130`（两处 `unsafe { libc::kill(..) }`，`libc::SIGKILL`）。`nix` 确实在 `Cargo.lock:4876`，但我没有确认它是谁的传递依赖、也没有确认它会带来哪些 feature；收益是两个 unsafe 调用，风险是更大 API 面 + 与 `kill_tree` 的进程组语义耦合。**证据不足以证明收益 > 风险，不建议**。
2. **`/proc` + Toolhelp32 手写枚举 → `sysinfo`**：`sysinfo` **不在** `Cargo.lock`（grep 无命中）⇒ 要引入新依赖，换掉本轮刚用 C 探针 + 真机验证过的双平台实现（t24/t28 的僵尸语义、btime 截断、映像名唯一性都建立在这份实现上）。**不建议**。
3. **手写目录遍历 → `walkdir` / `glob`**：两者都在 `Cargo.lock:7931 / :2929`，但仓里现在的遍历是「一层 + 名字前缀」（t27 的 pruner），`walkdir` 的递归与错误语义会改变现有断言的行为。**收益低，不建议**。
4. **测试框架类（`assert_cmd` / `wiremock` / `insta`）**：三者都**不在** `Cargo.lock` ⇒ 属于引入新依赖替换现有手写 CLI/HTTP 测试，且没有第二个消费者。**不建议**。
5. **「一次性替换 112 处 `temp_dir()`」**：见 F3——那是重写，不是审计建议。
6. **状态词表的跨语言 codegen**：仓里没有 codegen 工具链，TS 侧只有 2 个联合类型；引入生成器 = 新构建步骤 + 新失败模式，而 F2 的守卫断言性价比更高。**证据不足以支持，不建议**。
7. **F8 的「提取路由常量表」**：我**没有**把 api.rs 的 240 处 `/api/v1` 逐条分类为路由/文档/测试，所以无法声称「收敛成表」是安全的最小修法。**以守卫代替，不建议先动结构**。
8. **面板 i18n 的重复文案**：`panel/src/**` 本轮 out of scope，且我**没有取到**「同一文案在两处各自定义」的具体实例（只在 `api.ts` 看到状态词的联合类型，已归入 F2）。**找不到证据，不列**。

## 12. 复现命令（本文每条 file:line 的来源）

全部在仓库根、Windows/Git Bash 下执行（Linux 同样成立，它们只读文件）：

```bash
# F1 / F2 的列集合与状态词表
grep -rn 'DISTILL_LOG_COLUMNS' crates/daemon/src/api.rs
grep -n 'INSERT INTO distill_log' -A 6 crates/daemon/src/distill.rs
grep -rn 'DISTILL_LOG_STATUSES\|not_recorded' crates/daemon/src/api.rs cli/src/main.rs | head -20
grep -rn 'memories_refused' crates/store/src/ crates/daemon/src/api.rs crates/daemon/src/distill.rs | head -10
grep -rn 'ok.*empty.*failed' panel/src/*.ts | head

# F3 手搓临时根 + tempfile 是否已在图里
grep -rn 'std::env::temp_dir()' --include=*.rs crates/ cli/ | wc -l          # -> 112
grep -rln 'std::env::temp_dir()' --include=*.rs crates/ cli/ | head -12
grep -n 'name = "tempfile"' Cargo.lock                                        # -> 7231
grep -rn 'tempfile' crates/*/Cargo.toml cli/Cargo.toml Cargo.toml              # -> 无命中（传递依赖）

# F4 身份事实的两个 struct / 两处比较
grep -n 'fn write_current\|fn read_prior\|started_at' crates/daemon/src/orphans.rs | head -12

# F5 退出原因词表的两种语言各一份
grep -rn 'stop-request\|signal:console\|server-returned' crates/daemon/src/lib.rs scripts/ruagent-daemon.ps1 | head

# F6 工具链 pin 的 3 个载体
grep -n 'channel' rust-toolchain.toml; grep -n 'toolchain' .github/workflows/*.yml

# F7 轮询常量与循环
grep -rn 'EXIT_GRACE\|EXIT_POLL\|ATTRIBUTION_FIRST_POLL_MS\|ATTRIBUTION_MAX_POLL_MS\|RUN_CHILD_FIRST_POLL_MS\|RUN_CHILD_MAX_POLL_MS' --include=*.rs crates/daemon/src/*.rs

# F8 路由字面量跨语言
grep -rc '"/api/v1' crates/daemon/src/api.rs    # -> 240
grep -rc '/api/v1' panel/src/api.ts              # -> 113

# F9 mock 二进制定位的两套规则
grep -rn 'fn mock_agent\|fn mock_bin' -A 14 crates/daemon/src/chat.rs crates/mock-agent/tests/e2e_daemon.rs | grep -E 'current_exe|CARGO_BIN_EXE|ruagent-mock-agent'

# F10 时刻/唯一名的三种写法
grep -rn 'SystemTime::now' --include=*.rs crates/ cli/ | wc -l          # -> 25
grep -rn 'fn now_unix_ms\|fn chrono_tag' --include=*.rs crates/
```

### 本轮三个「形状样板」（本次审计的判据）

审计前先定死了什么算「值得改」，用的是本轮已经消灭的三例：

| 样板 | 改前形状 | 改后形状 | 本文用它筛掉了什么 |
| --- | --- | --- | --- |
| 命名空间词表 | 各处自己列允许值 | `default_namespace()` 向词表提问（`distill.rs:1795/1804`、`api.rs:6705/6717`） | 筛掉「再写一份词表」的修法（§11.6） |
| 拒绝口径 | api/mcp/cli 各写一套字符串 | typed `WriteError` + 409 + `refused[{store,namespace,allowed_namespaces}]` | 筛掉「用正则对齐文案」这类修法；F8 的守卫是它的最小形态 |
| 状态词表 | 新增第 4 个状态词 | 用 additivity 加一列 `memories_refused`（`0031_distill_refused.sql:58`） | F2 因此**不**建议加词，只收敛现有 3 份 |

**本文件未改动任何源码**：唯一新增/变更的路径就是本文件自己（`docs/design/reviews/issues-56-59-architecture-audit.md`）。