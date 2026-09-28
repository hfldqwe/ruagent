# gen3 只读审计：编排状态机 / 策略级联 / 纯域层（`orchestrator` · `policy` · `core`）

> **性质**：**只读审计**。本单没有改任何代码（唯一写入是本文件），没有写活库，没有启停 pid 79984。
> **审计者**：verify2（独立审计；这三个 crate 先前**从未有过 finding 清单**）。
> **时间窗**：2026-09-29 02:5x–03:5x +08:00（活库/文件读数带各自时间点；期间 `.github/workflows`、`crates/daemon/src/lib.rs` 等被同伴在途编辑，行号以本报告的复核时刻为准）。
> **规模**：`crates/core` 7 文件 ≈ 30 KB（`agent.rs` 4.4K / `event.rs` 7.7K / `id.rs` 2.3K / `lib.rs` 0.8K / `routing.rs` 2.2K / `run.rs` 6.9K / `task.rs` 4.2K / `usage.rs` 1.5K）；`crates/orchestrator/src/lib.rs` 11.9 KB（339 行）；`crates/policy/src/lib.rs` 14 KB（414 行）。**这三个 crate 被全文读完**（不是抽样）。
> **计数**：**13 条 finding**（按「能不能伤到正确性/可审计性」排序）+ §1.14「先证伪」读数（**6 条我否掉的线索**）+ §2 未验证猜想（5 条）+ §3 未覆盖范围。
> **基线读数（本单实测，2026-09-29T03:5x）**：三个 crate 的测试 **22 passed / 0 failed / 0 ignored**——`ruagent-core` 11、`ruagent-orchestrator` 4、`ruagent-policy` 7，doc-test 0×3（`-NoLock -Jobs 4 -TargetDir %TEMP%\ruagent-verify2-t78`，首次 15.1 s / 复跑 2.6 s，exit 0）。**这套绿灯恰好覆盖了 C-2、C-4、C-8 三处问题行为**：`status_predicates` 把 `Interrupted` 既 active 又 terminal **断言成期望**、`unknown_actions_are_dropped_not_fatal` 把"静默丢弃规则"**断言成期望**、`pipeline_chains_subtasks` 走完 3 步链路却**不检查子任务字段是否被丢**（详见各条）。

---

## 0 摘要（一段话）

这三个 crate **确实很干净**：`core` 的依赖表只有 `serde / serde_json / uuid / chrono`，**没有任何 I/O、没有 `static mut`、没有隐藏全局状态**（唯一的进程级状态来自 `Uuid::now_v7()` 的 uuid 内部计数器，见 C-10）；`policy` 的解析是**fail-closed** 的（缺省动作 = `Ask`，`PermissionPolicy` 无匹配则是 `Ask`）；`orchestrator` 是纯函数式的 `plan()`+`route()`，`PlanError` 把 `Empty` 与 `FanOutTooSmall` 分开报（**这是本仓"拒绝路径可辨"的正面样板**）；`PermissionPath::Auto(Reject)` 与 `Human` 是可区分的；权限自动放行时若 agent 没提供 allow 类选项，代码**fail closed**（`runs.rs:1414`）。但**正确性与可审计性的关键部位有三处系统性缺口**：
1. **"Task/Run 状态机"在这三个 crate 里根本不存在** —— 三个 crate 里 `grep` 不到任何 transition 校验函数，`status` 是**公开字段**，真正的状态迁移是 `crates/daemon` 里的 **13 处直接赋值**；"终态是否最终态"没有任何机制保证（C-1）。
2. **`RunStatus::is_active()` 与 `is_terminal()` 在 `Interrupted` 上互相矛盾（两者都为真）**，而 `crate` 自己的测试**把这个矛盾断言成契约**（`run.rs:189-190`）；`is_active()` 在生产代码里**零消费**（C-2）。
3. **权限决策的审计面把"默认值决定"记成"规则决定"**：`policy::decide()` 不返回规则身份（`PermissionRule` 连 id 字段都没有），于是 daemon 只能用 `rule_id = format!("title~={}", title)`（**工具标题**）充当规则身份 ⇒ 没有规则命中、走 `default` 的那次决策，在事件流里**也写成 `PermissionResolution::Rule`**（C-3）。
另外两条想知道答案的问题也有了读数：**"超时"这条路径在域模型里不存在**（`StopReason` 没有 `Timeout`，run 循环没有 deadline，一个挂死的 agent 会把该 harness 的许可位永久占住）（C-5）；而**级联/规则的静默降级**在四处出现（C-4/C-6/C-7/C-8）。

---

## 1 findings

### C-1（high · 状态机没有家）"Task/Run 状态机"在这三个 crate 里不存在：无转移函数、`status` 是公开字段、终态可被直接改回非终态

* **`file:line`**
  * 类型侧：`crates/core/src/task.rs:64-78`（`Task.status: TaskStatus` **pub**，无转移方法）、`crates/core/src/run.rs:135-157`（`Run.status: RunStatus` **pub**）、唯二构造器 `task.rs:82-95` / `run.rs:161-178`（只把状态设成 `Pending` / `Queued`）。
  * 声明侧（声称存在）：`crates/orchestrator/Cargo.toml:2` `description = "Task/Run state machines, topologies, routing cascade"`；`AGENTS.md` 的 crate 表 `orchestrator/  Task/Run state machines, topologies, routing cascade`。
  * 实际迁移侧：`crates/daemon/src/runs.rs:673,680,687,728,731,757,797,830,1452,1477,1488,1496`（12 处 `run.status = …`）+ `crates/daemon/src/lib.rs:117`（恢复循环里 `run.status = RunStatus::Interrupted`）。
* **可复现命令**
  ```powershell
  # 1) 三个 crate 里没有任何 transition/set_status/can_* 校验函数（应输出空）
  Select-String -Path crates/core/src/*.rs,crates/orchestrator/src/*.rs,crates/policy/src/*.rs -Pattern 'fn .*transition|fn set_status|fn update_status|fn can_'
  # 2) status 是公开字段：任何持有 &mut Run 的代码都能写
  Select-String -Path crates/core/src/run.rs,crates/core/src/task.rs -Pattern 'pub status'
  # 3) 真正的迁移点在 daemon（13 处赋值）
  Select-String -Path crates/daemon/src/runs.rs,crates/daemon/src/lib.rs -Pattern 'run\.status = RunStatus::'
  ```
* **证据读数**：命令 1 输出 **0 行**；命令 2 输出 `task.rs:70 pub status` / `run.rs:140 pub status`；命令 3 输出 **13 行**。⇒ **"非法转移能否发生"的答案是：结构上完全可以**——`run.status = RunStatus::Queued` 写在任何 `&mut Run` 上都编译通过，终态没有任何"回不去"的保证；`is_terminal()` 只是**读**谓词，不参与写入路径。审计问的"终态能否回到非终态"因此不是"某处有 bug"，而是**本代没有任何地方在管这件事**。
* **为什么重要**：这是 `design §5.1 / §8.3`（Run 生命周期、崩溃恢复、重试）所依赖的**唯一不变量**（"终态是最终态"）的落点缺失。`runs.rs:1688`、`api.rs:429`、`api.rs:2358` 都在用 `is_terminal()` 决定"这个 run 结束了没有"；而写入侧无守卫，任何后来的动手者（或一次 auto-fix）都能把 `Completed` 改回 `Queued`，没有任何测试或类型会抗议——本代反复抓的"声称与实现不符"在这里是**结构级**的。注意我在这次审计里**抽样复核了 5 处**赋值（`660-690`、`1445-1504`），**没有发现已有的非法转移**（见 §1.14），所以这条是**能力缺失**，不是"已经坏掉"。
* **可证伪的修复判据**：存在一个转移入口（`core` 或 `orchestrator` 上的 `fn transition(&mut self, to: RunStatus) -> Result<(), TransitionError>` / `Task` 同形），并且**终态 → 非终态被拒**：测试 `assert!(run.transition(RunStatus::Queued).is_err())` 在 `Completed`/`Failed`/`Cancelled`/`Interrupted` 四个终态上逐个成立；或退一步，把 `AGENTS.md`/`Cargo.toml` 的描述改成"状态机由 `crates/daemon` 持有"并指名文件。二者必取其一——**判据是"声称的那个家能被执行点验证"**。
* **建议 owner**：`core` + `orchestrator`（类型与入口）+ **captain 裁决**（"状态机住哪"是 spec 级决定；本审计只报告不代改）。

---

### C-2（high · 谓词自相矛盾且被测试锁死）`Interrupted` 同时"在动"且"已终结"；`is_active()` 生产零消费

* **`file:line`**：`crates/core/src/run.rs:33-39`（`is_active = !matches!(status, Completed|Failed|Cancelled)` ⇒ `Interrupted` 为 **true**，doc 写"Whether the run is still moving (not in a terminal state)"）；`crates/core/src/run.rs:55-64`（`is_terminal = matches!(status, Completed|Failed|Cancelled|**Interrupted**)` ⇒ `Interrupted` 为 **true**）；`crates/core/src/run.rs:186-193`（测试**同时**断言两者）。
* **可复现命令**
  ```powershell
  Get-Content crates/core/src/run.rs | Select-Object -Skip 32 -First 8      # is_active
  Get-Content crates/core/src/run.rs | Select-Object -Skip 54 -First 11     # is_terminal
  Get-Content crates/core/src/run.rs | Select-Object -Skip 185 -First 9     # 测试：两个都 assert!(…)
  Select-String -Path crates -Include *.rs -Pattern 'is_active\(\)'          # 消费点
  ```
* **证据读数**：`run.rs:189-190` 原文是
  ```rust
  assert!(RunStatus::Interrupted.is_active());
  assert!(RunStatus::Interrupted.is_terminal());
  ```
  即**矛盾被写进断言**（该测试在基线里 **ok**，是 22 passed 之一 ⇒ 矛盾不但在，而且**在绿线上跑**）；`is_active()` 的消费点 grep = **只有 `run.rs:187,189,191`（同一测试内）**，生产代码 **0 处**；`is_terminal()` 的生产消费点 3 处（`daemon/src/api.rs:429`、`api.rs:2358`、`daemon/src/runs.rs:1688`）。
* **为什么重要**：两个谓词对同一个状态给出不相容答案，而这正是审计问的"有无『永远等不到终态』的循环"的**结构性温床**：`is_active()` 的 doc 说它回答"是否还在动"，于是任何按它写回收器/看门狗/超时守卫的人，都会把 `Interrupted` 的 run 当作**永远在动**（永不被回收）；而按 `is_terminal()` 写的人会认为它已经结束。更糟的是**测试把这个矛盾断言成期望行为**，所以 review、CI、后来的读者都拿到"这是对的"信号。`Interrupted` 在本仓是**崩溃恢复**产生的主路径（`daemon/src/lib.rs:110-123` 每次重启都会批量产生），不是边角状态。
* **可证伪的修复判据**：对全部 8 个变体成立 `assert_eq!(s.is_active(), !s.is_terminal())`（一条 property/exhaustive 测试即判据）；若 `Interrupted` 必须"可重试但已终结"，则删掉 `is_active()`（0 消费者，删掉无成本）或改名为 `wants_attention()` 等不与 terminal 对立的名字——**判据是"两个都叫 active/terminal 的谓词不能再同时为真"**。
* **建议 owner**：`core`（`run.rs`）。

---

### C-3（high · 可审计性/自称不符）权限审计面把「默认值决定」记成「规则决定」，且 rule_id 填的是工具标题

* **`file:line`**
  * 根因（在范围内的类型）：`crates/policy/src/lib.rs:75-80`（`PermissionRule { title_contains, action }` —— **没有 id/name/index**）、`crates/policy/src/lib.rs:84-94`（`decide() -> PermissionAction`：只回动作，不回**是哪条规则**；`for i in 0..` 的循环里连索引都没留下）。
  * 写入侧：`crates/daemon/src/runs.rs:1406-1408`
    ```rust
    resolution: PermissionResolution::Rule {
        rule_id: format!("title~={}", title.to_lowercase()),
    },
    ```
  * 事件类型（本代冻结的审计契约）：`crates/core/src/event.rs:88-98`（`PermissionResolution::Rule { rule_id }` / `ApproverAgent { agent }` / `Human`）。
* **可复现命令**
  ```powershell
  Get-Content crates/policy/src/lib.rs | Select-Object -Skip 74 -First 21     # 规则类型 + decide()
  Get-Content crates/daemon/src/runs.rs | Select-Object -Skip 1400 -First 12  # 写入的 rule_id
  Select-String -Path crates -Include *.rs -Pattern 'PermissionResolution::Rule'
  ```
* **证据读数**：`policy` 侧没有任何规则身份可返回（类型里没有该字段，`decide()` 的返回类型里也没有）；daemon 侧唯一的"身份"是 **`title~=<工具标题>`**；`PermissionResolution::Rule` 的生产者只有 `runs.rs:1406` 一处（另一个命中在 mock 测试）。⇒ 两个具体后果，都能用一次实验判定：
  (a) **无规则命中、走 `default` 的那次决策，被写成"某条规则决定了"** —— 因为 `PermissionPath::Auto(action)`（`policy:105-108`）不携带"来自规则还是来自 default"，daemon 只能照写 `Rule`；
  (b) **两条规则匹配同一标题时，同一个 `rule_id`**（都是 `title~=…`），所以从事件流里**无法判断是哪条规则的 action 生效**，也无法把这条记录反查回 `policy.toml` 的任何一行。
* **为什么重要**：这是本代反复抓的「**事件存在 ≠ 那件事发生过**」家族里最贵的一种：审计轨迹**在内容上是错的**（把 fail-open 的默认路径写成规则路径），而权限决策正是"事后必须能回答为什么放行/拒绝"的地方；`design §9.2` 要求"规则自动应答"与"默认"可区分，`PermissionResolution` 的变体名也在承诺这一点（`Rule` vs `Human`），但**没有 `Default` 变体可以表达真相**。
* **可证伪的修复判据**：`decide()`/`path()` 返回可辨的来源（如 `RuleMatched { index: usize, id: Option<String> }` / `RuleSource::Default`，或给 `PermissionRule` 加一个 `id: String` 并由 daemon 回填），且**"没有规则命中"被记录成非 `Rule` 的来源**。测试形如：配置 `[default="allow"] + 无匹配规则` ⇒ 记录的 resolution **不是** `Rule`；配置两条同标题不同动作的规则 ⇒ 两条记录的 `rule_id` **不同**，且都指向配置里的具体条目。
* **建议 owner**：`policy`（类型 + `decide` 的身份）+ `daemon/runs.rs`（写入）——**根因在 `policy`，所以建议 policy 侧先动**。

---

### C-4（medium · 静默降级）策略「编译器」是全函数且不出声：三个实例——未解析的 action 被丢弃、优先级遮蔽无告警、`per_harness = 0` 被接受

* **`file:line`**
  * 实例①（丢弃规则）：`crates/policy/src/lib.rs:232-242`（`rules: … .filter_map(|r| parse_action(&r.action).map(|a| PermissionRule { … }))`）+ `crates/policy/src/lib.rs:251-258`（`parse_action`：`"allow"/"reject"/"ask"` 之外 **`_ => None`**）；同文件 `:223` `pub fn to_policy(&self) -> PermissionPolicy`（**返回类型里没有 Result、没有 warnings**）。
  * 实例②（默认值静默替换）：`crates/policy/src/lib.rs:226-231`（`.and_then(parse_action).unwrap_or(PermissionAction::Ask)`——写错一个字母的 `default` 静默变成 `Ask`，方向是 fail-closed，但**与作者意图不可区分**）。
  * 实例③（优先级遮蔽无告警）：`crates/policy/src/lib.rs:85-92`（`for rule in &self.rules { if 命中 { return rule.action } }`——**文件顺序即优先级**，没有特异性/最长匹配，也没有"某条规则永远不可达"的检查）。
  * 实例④（零值配置被接受）：`crates/policy/src/lib.rs:156-164`（`limits()` 直接 `unwrap_or(default)`，**不校验** `per_harness == 0` / `queue_per_harness == 0`）。
* **可复现命令**
  ```powershell
  Get-Content crates/policy/src/lib.rs | Select-Object -Skip 221 -First 28   # to_policy
  Get-Content crates/policy/src/lib.rs | Select-Object -Skip 250 -First 9    # parse_action 的 _ => None
  Get-Content crates/policy/src/lib.rs | Select-Object -Skip 384 -First 11   # 把丢弃动作断言成期望
  Get-Content crates/policy/src/lib.rs | Select-Object -Skip 263 -First 22   # first_matching_rule_wins
  ```
* **证据读数**
  * 实例①：测试 **`unknown_actions_are_dropped_not_fatal`**（`:385-394`）原文断言 `assert!(config.to_policy().rules.is_empty())`，fixture 是 `action = "explode"` ⇒ **行为是故意的、且被测试钉住**（该测试在基线里 **ok**）；但 `to_policy()` 的返回类型（`:223`）**没有任何地方能告诉用户"你的规则被丢了"**。
  * 实例③：`first_matching_rule_wins`（`:265-283`）钉住"先匹配先赢"，而"先写宽规则、后写具体规则"这一常见写法会**静默遮蔽**后者（例：`title_contains="read"→Allow` 在 `title_contains="read secrets"→Reject` 之前 ⇒ 后者永远不可达），没有任何告警。
  * 实例④：`per_harness: 0` 会得到一个**永远 `try_acquire` 失败**的信号量（daemon 侧 `runs.rs:663-684` 的 gate 路径），配置本身不报错。
* **为什么重要**：这是本代反复抓的「**静默降级**」。实例①最危险：一个字符的笔误（`action = "deny"`）会让**一条安全规则消失**，而"丢弃一条规则"在 first-match-wins 语义下**会改变哪条规则生效**（更宽的规则或 `default` 顶上）——用户以为 `rm -rf` 被拒，实际走的是 `default`。实例②方向是安全的（fail-closed），但"作者写了 `denny`、系统当 `ask`"同样是不可辨的意图丢失。这四个实例的共同根因是**策略编译没有诊断通道**（全函数、无 Result、无 warnings）。
* **可证伪的修复判据**：`to_policy()` 返回 `Result<PermissionPolicy, PolicyError>` 或 `(PermissionPolicy, Vec<PolicyWarning>)`，且**至少覆盖实例①**：对 `action = "explode"` 的 fixture，调用方必须拿到一条**非空**诊断（含规则索引与原始值），测试断言该诊断存在且能点名被丢弃的条目；实例③要求"前一条的 `title_contains` 是后一条的子串"时给出遮蔽告警；实例④要求 `per_harness == 0` 被拒或被钳到 ≥1。反向假读数也要挡：**合法配置必须产出空诊断**（否则就成了"永远报警"）。
* **建议 owner**：`policy`（编译器 + 诊断类型）。

---

### C-5（medium · 走不到终态 / 资源泄漏）没有任何 run 级超时：挂死的 agent 永远停在 `Running`，并永久占住它那一档 harness 的许可位

* **`file:line`**：`crates/daemon/src/runs.rs:1341-1505`（run 主循环的 `tokio::select!`：四个分支 `ev_rx.recv()`（`:1342`）、`ask_rx.recv()`（`:1382`）、`cancel_token.cancelled()`（`:1442`）、`res = &mut driver`（`:1472`）——**没有 timer/deadline 分支**）；`crates/daemon/src/runs.rs:1680-1698`（`wait_terminal`：轮询 + **1800 s** 上限，超时 `anyhow::bail!`）；`crates/daemon/src/runs.rs:663-690`（harness gate：`try_acquire_owned` 成功才 `Spawning`，否则排队或被拒）；`crates/core/src/run.rs:80-93`（`StopReason` 六个变体，**没有 `Timeout`**；而 `Error` 的 doc 明写 `/// Error (subprocess crash, protocol violation, timeout).`）。
* **可复现命令**
  ```powershell
  Select-String -Path crates/daemon/src/runs.rs -Pattern 'tokio::select!|= .*\.recv\(\)|_ = cancel_token|res = &mut driver'   # 4 个分支，无 timer
  Select-String -Path crates/daemon/src/runs.rs -Pattern 'sleep|deadline|timeout|Duration::from'                              # 只有 300ms/100ms/1800s 观察者
  Get-Content crates/daemon/src/runs.rs | Select-Object -Skip 1679 -First 19                                                  # wait_terminal
  Get-Content crates/core/src/run.rs | Select-Object -Skip 79 -First 15                                                        # StopReason（无 Timeout）
  ```
* **证据读数**：`select!` 分支恰好 4 个（上面四行），**无 `sleep`/`interval` 作为 run 的截止**；`runs.rs` 里所有时间量只有三处：`:1336`（驱动结束后的 300 ms drain）、`:1439`（`done` 后的 100 ms tick）、`:1682-1694`（`wait_terminal` 的 1800 s 观察者上限）。`StopReason` 六个变体里没有超时项，而 `Error` 的 doc 声称覆盖 `timeout` ⇒ **"超时"这条路径在域模型里没有类型，在 run 循环里没有实现**。
  **对照读数（说明这不是"这个仓不会写超时"）**：同一个仓里 `crates/daemon/src/distill.rs:612` 就用 `tokio::time::timeout(std::time::Duration::from_secs(300), done_rx)`，`crates/acp/src/chat.rs:1095/1148/1546/1557/1587` 用 `MODELS_WAIT` 包裹等待 ⇒ **模式是现成的，run 执行路径只是没用它**（我全量 grep 了 `crates/acp/src/*.rs` 与 `crates/daemon/src/*.rs` 的 `tokio::time::timeout|.timeout(|timeout(`：命中全部落在 `chat.rs` 与 `distill.rs`，**没有一处包住 `session/prompt`**）。
  **许可位被永久占住这条是读出来的，不是推的**：`runs.rs:718` 的 `let _permit = match hold { … }` 是 `pub async fn start_run`（`:618-853`）的局部量，而同一个函数在 `:808` **`.await` 了 `supervise(`**（不是 `spawn`）⇒ `_permit` 活到 `start_run` 返回，即**活到 run 终态**（上游 `api.rs:2044-2058` 也是 `await` 这条 `start_run`）。⇒ 一个永不终态的 run = 一个永不释放的许可位 + 一条永不返回的请求任务；被挂住的 run：`run.status` 一直是 `Running`（不满足 `is_terminal()`），`driver` future 永不 resolve，`per_harness`（默认 2）里那一份**永不归还**，`wait_terminal` 唯一的反应是 30 分钟后**抛错给观察者**——它**不移动 run 行、不释放许可位**。
* **为什么重要**：这就是审计问的「**永远等不到终态的循环**」的真实形态，而且是**资源型**的：默认 `per_harness = 2`，两个挂死 run 就让该 harness 的容量归零，之后新任务按 `queue_per_harness = 8` 排队、再之后被明确拒绝（`reserve_slot` 的错误是可见的，见 `runs.rs:673-678`）——所以**新任务会看到清楚的错误，而"两个 run 永远在跑"这件事没有任何遥测**（没有 WARN、没有状态、没有事件）。这与 C-2 叠加：`is_active()` 会说这两个 run "still moving"，于是任何基于它的清理都不会碰它们。
* **可证伪的修复判据**：存在一个可配置的 run 截止（`[concurrency]`/`policy` 里的 `run_timeout_secs` 或等价物），超时后 run 必须**进入终态**（新增 `StopReason::Timeout` 并在 `RunEvent` 里留下可辨的记录），并且**释放 gate**。可测形式：mock agent 接受 `session/new` 后**永不应答 `prompt`** ⇒ 断言 (a) 该 run 行在 ≤ 上限时间内变成终态且原因是超时，(b) 同 harness 的下一次启动**不再排队**（许可位已归还）。`StopReason` 的 doc 若继续声称覆盖 timeout，则必须有对应变体与生产者——**判据是"doc 里说的那条路径能在测试里被指出来"**。
* **建议 owner**：`daemon/runs.rs`（实现超时）+ `core`（新增 `Timeout` 变体与事件字段）。**注意**：本次审计只读了 run 循环与 `wait_terminal`，没有读 `reserve_slot` 的全部算术（见 §3）。

---

### C-6（medium · 跨 crate 抽查 · 持久化读数静默降级）枚举↔字符串映射有两份、语义不同，且都带 fail-open 的兜底

* **`file:line`**（**这三个文件不在被审的三个 crate 内**；这是主动出的范围抽查，因为**根因是 `core` 的枚举没有权威字符串与 `ALL`/往返契约**）
  * `crates/store/src/lib.rs:558-566`：`task_status_from_str` 的兜底是 **`_ => TaskStatus::Pending`**（非终态、且是"待执行"）。
  * `crates/store/src/lib.rs:600-609`：run 状态兜底是 **`_ => RunStatus::Queued`**（非终态）。
  * `crates/store/src/lib.rs:623-632`：`stop_reason_from_str` 的兜底是 **`_ => StopReason::EndTurn`**（**"成功"**）。
  * 对照（同一语义的第二份实现，语义**不同**）：`crates/daemon/src/api.rs:4719-4727` `parse_task_status` 返回 `Option<TaskStatus>`，未知 ⇒ **`None`**（严格）。
  * 根因侧：`crates/core/src/task.rs:8-21`、`crates/core/src/run.rs:11-30`、`:78-93` —— 三个枚举**没有** `ALL` 常量、没有 `FromStr`/权威字符串（对比 `crates/core/src/agent.rs:21-29` 的 `HarnessKind::ALL` **是有的**）。
* **可复现命令**
  ```powershell
  Get-Content crates/store/src/lib.rs | Select-Object -Skip 557 -First 9    # task_status_from_str: _ => Pending
  Get-Content crates/store/src/lib.rs | Select-Object -Skip 622 -First 10   # stop_reason_from_str: _ => EndTurn
  Get-Content crates/daemon/src/api.rs | Select-Object -Skip 4718 -First 10 # parse_task_status: Option
  Select-String -Path crates/core/src/*.rs -Pattern 'pub const ALL'         # 只有 agent.rs
  ```
* **证据读数**：三处兜底分别把"不认识的值"映射到 `Pending` / `Queued` / **`EndTurn`**；而"未识别"在这些位置**只可能来自**（a）更新的版本写入了新变体，（b）DB 里的值被改坏/截断。第一条是**真实的跨版本场景**：一个未来的 `StopReason::Timeout` 行被当前版本读回 ⇒ **报成 `end_turn`（成功）**；一个未来新增的 `TaskStatus` 行被读回 ⇒ 报成 `Pending`，而 `daemon/src/runs.rs:804` 正是用 `matches!(task.status, TaskStatus::Pending | TaskStatus::Done)` 决定**要不要执行这个 task** —— 于是一条"未知状态"的任务会被当作**新任务再执行一遍**。
  **兜底还会把"漏写一个分支"藏起来（本单实测）**：`run_status_to_str`（`store/src/lib.rs:586-597`）**8 个变体全有**（含 `:588 Queued => "queued"`），而 `run_status_from_str`（`:599-610`）只有 **7** 条显式分支、**没有 `"queued"`**——它的正确性完全由兜底 `:608 _ => RunStatus::Queued` 恰好等于真值来保证。⇒ 这条映射的"完整性"是靠兜底**冒充**出来的；换成任何另一个值，同一个兜底就变成错的（`EndTurn`/`Pending` 两处正是如此）。这也说明**只做往返测试抓不到这个漏项**（`from_str(to_str(Queued))` 会通过），必须让"未知/漏写"变成**可辨的错误**。
* **为什么重要**：这是「**静默降级**」在**持久化读数**上的形态：错误不是抛出来的，而是变成"看起来正常"的值（成功/待执行）。同时这条也暴露了"同一套映射写两遍、语义不一致"的漂移：`api.rs` 版本严格、`store` 版本宽松，谁是真值取决于路径。修 `core` 一处即可收敛（权威字符串 + ALL + 往返测试）。
* **可证伪的修复判据**：映射只有一份（`core` 上实现 `as_str`/`FromStr` 或 `serde` 单一来源），并有一条**穷尽往返测试**：`for v in StopReason::ALL { assert_eq!(StopReason::from_str(v.as_str()), v) }`（`TaskStatus`/`RunStatus` 同形）；未知值的处理必须是**可辨的**（返回 `Result`/跳过该行并 `warn!`），不得映射成成功或非终态；**并且解析侧不得存在"等于某个合法变体"的兜底**（`run_status_from_str` 缺 `"queued"` 这个漏项今天靠兜底掩盖——漏写分支必须变成 `Err` 或编译错误，不能靠兜底补上）。反向读数：篡改 DB 里一个 status 字符串为 `"wat"`，读回时**必须**产生一条可见的诊断或 `None`，**不得**得到 `EndTurn`/`Pending`。
* **建议 owner**：`core`（权威字符串 + `ALL` + 往返测试）+ `store`（去掉 fail-open 兜底）。

---

### C-7（medium · 级联不可辨）路由级联的"没匹配"与"默认值也是坏的"不可区分；rule_id 是 `Debug` 拼串；上游还在静默丢弃规则

* **`file:line`**
  * `crates/orchestrator/src/lib.rs:182-226`：`pub fn route(task, config) -> Option<RoutingDecision>` —— **返回裸 `Option`，没有任何理由通道**；`:219-225` 的兜底是 `config.default.as_ref().and_then(|agent| agent.parse().ok().map(…))`（默认值解析失败 ⇒ 直接 `None`）。
  * `crates/orchestrator/src/lib.rs:208`：`let agent: ruagent_core::AgentId = rule.agent.parse().ok()?;` —— 在 `Option`-返回函数里对 `Option` 用 `?` ⇒ **一条坏规则会让整个级联立刻返回 `None`，连 `default` 都不再尝试**（**可达性已被我否掉**，见下）。
  * `crates/orchestrator/src/lib.rs:211-213`：`rule_id: format!("project~{:?}+title~{:?}", rule.project, rule.title_contains)` —— 用 Rust `Debug` 语法拼出的字符串（`Some("ruagent")`），**既不稳定也不唯一**（同谓词的规则同 id），且**不含规则索引**。
  * 上游静默丢弃：`crates/daemon/src/api.rs:4706-4713`（`.filter_map(|r| resolve(&r.agent).map(…))` —— 规则里写了一个**未注册的 agent 名**，这条规则**无声消失**）。
  * `crates/core/src/routing.rs:51-53`：`pub fn primary(&self) -> AgentId { self.agents[0] }` —— 对空 Vec **panic**；"非空"不变量的唯一落点是 `:23-24` 的 **doc 注释**，字段全是 pub 且无构造校验。
* **可复现命令**
  ```powershell
  Get-Content crates/orchestrator/src/lib.rs | Select-Object -Skip 181 -First 45   # route() 全貌
  Get-Content crates/daemon/src/api.rs | Select-Object -Skip 4691 -First 26        # 上游：filter_map 丢弃 + 预解析成 id
  Get-Content crates/core/src/routing.rs | Select-Object -Skip 22 -First 32        # 非空 doc vs primary() 的 [0]
  ```
* **证据读数（含"先证伪"）**：`api.rs:4692-4717` 的 `routing_decision()` 在调用 `route()` **之前**就把配置里的 agent **名字**解析成了 `AgentCard.id.to_string()`（`resolve` = 查注册表 ⇒ `candidates` 只可能是合法 UUID 字符串）⇒ **`orchestrator:208` 的 `.ok()?` 在今天的生产路径上不可能失败**（我在报告里明确否掉它的"高危"定性，只留"潜在形状"这一层）。对照：`api.rs:4706-4713` 的 `filter_map` 是**现在就会发生**的静默丢弃（名字打错 ⇒ 规则消失 ⇒ `route()` 走 `default` ⇒ **另一个 agent 接了这份工作**），而 `route()` 的返回类型让调用方**无法区分**"没有规则命中"与"规则命中了但上游把它丢了/默认值也是坏的"。
* **为什么重要**：`core/src/routing.rs:6-7` 的原文承诺是 "**Always recorded with the decision so 'why this agent' is forever answerable**"；`orchestrator:180-181` 也说 "the cascade records provenance either way"。但 (a) `Option` 型返回在**没有决策**时不留任何记录，(b) 有决策时记录的是 `Debug` 拼串而不是可反查的规则身份，(c) 上游还会静默丢规则。⇒ "why this agent"在今天**不是**永远可回答的；这条与 C-3 是同一族（审计面自称可辨、实际不可辨），只是发生在路由而不是权限。
* **可证伪的修复判据**：`route()` 返回 `Result<RoutingDecision, RouteError>`（或带 `rationale`/`warnings` 的决策），其中**至少**区分三种结局：无规则命中（且无默认）、规则命中但规则本身不可用、默认值不可用；`rule_id` 是**稳定身份**（规则索引或配置里的显式 `id`）；`api.rs` 的名字解析对每条被丢弃的规则产出可见诊断。测试：`routes=[{project:"a", agent:"<未注册>"}] , default=Some("<valid>")` ⇒ 断言拿到"走了默认 **且** 有一条规则被丢弃"的诊断；`RoutingConfig {}`（无规则无默认）⇒ 断言 `Err(NoMatch)` 而不是 `None`。附带：`primary()` 改成返回 `Option`/`NonEmpty`，或 `RoutingDecision` 只经校验构造器产生。
* **建议 owner**：`orchestrator`（返回类型 + rule 身份）+ `daemon/api.rs`（丢弃诊断）+ `core`（`primary()` 的不变量）。

---

### C-8（medium · 静默降级）pipeline 第 2 步起丢掉 `project` / `pinned_agent`，而"单步 pipeline"保留它们 ⇒ 同一逻辑形状因步数不同而路由不同

* **`file:line`**：`crates/orchestrator/src/lib.rs:120-131`
  ```rust
  let sub = if i == 0 && steps.len() == 1 {
      task.clone()                                   // 单步：project / pinned_agent 保留
  } else {
      Task::new(format!("{} · step {} ({})", …), task.intent.clone(), TaskCreator::Rule { name: "pipeline".into() })
  };
  ```
  而 `crates/core/src/task.rs:82-95`（`Task::new`）把 `project: None, pinned_agent: None` **写死**；消费侧：`crates/orchestrator/src/lib.rs:193-201`（`project_match` 用 `task.project` 与规则的 `project` 比，`None` ⇒ **不匹配**）。
* **可复现命令**
  ```powershell
  Get-Content crates/orchestrator/src/lib.rs | Select-Object -Skip 112 -First 30   # pipeline 分支
  Get-Content crates/core/src/task.rs | Select-Object -Skip 80 -First 17           # Task::new 把 project/pinned_agent 置 None
  Get-Content crates/orchestrator/src/lib.rs | Select-Object -Skip 191 -First 16   # project 匹配
  ```
* **证据读数**：`Task::new` 是唯一构造器，`project = None`、`pinned_agent = None`（`task.rs:90-91`）；`plan()` 的 `Direct`/`FanOut` 分支**克隆原任务**（`:84-88`、`:100-111`）⇒ 保留；`Pipeline` 的第 2 步起用 `Task::new` ⇒ **丢失**；`steps.len() == 1` 时又回到克隆 ⇒ **保留**。于是配置 `routes=[{project:"ruagent", agent:claude}], default=dsh` 下：**第 1 步走 claude，第 2 步走 dsh**（因为 `project=None` 让 project 规则不再匹配），而用户显式 `pinned_agent` 也只对第 1 步生效。文档（`orchestrator:72-76`）只说了"每个 step 建一个子任务、以 agent 命名"，**没有说会丢字段**。
* **为什么这条能在绿线上活下来（基线读数）**：唯一的 pipeline 测试 `pipeline_chains_subtasks`（`orchestrator:275-306`）在基线里 **ok**，它断言的是 `runs.len()==3`、`edges.len()==2`、边方向（`from`/`to`）、`!concurrent`、以及 `prompt` 的传递（`:296-305`）——**没有一条断言涉及子任务的 `project` 或 `pinned_agent`**。⇒ 字段被丢这件事在"22 passed"下完全不可见。
* **为什么重要**：这是**范围内的**静默降级：同一份任务，因 topology 从 1 步变 2 步，路由、pin、以及任何按 project 做的策略**悄悄改道**；下游（子任务的 run）拿到的 agent 与用户显式指定的不同，而没有任何读数指出这件事。pipeline 是多步长任务的标准形状（设计 §5.2），所以这不是边角场景。
* **可证伪的修复判据**：子任务**继承** `project`（`Task::new` 之后显式赋值，或加 `Task::with_project`/`fork_of` 构造器）；`pinned_agent` 要么继承、要么**被明确清除并记进计划**（`Plan`/`PlannedRun` 上加字段或文档写明），二选一但要可辨。测试：`project=Some("ruagent")` 的任务做 2 步 pipeline ⇒ 断言 `plan.runs[1].task.project == Some("ruagent")`（或断言计划里带着"pin 已清除"的理由）。
* **建议 owner**：`orchestrator`（`plan`）+ `core`（构造器形状）。

---

### C-9（low · 漂移隐患）`HarnessKind::ALL` 靠注释维持同步，漏一个变体就静默少一档 harness 门

* **`file:line`**：`crates/core/src/agent.rs:21-29`（`pub const ALL: [HarnessKind; 4] = […]`，doc 是 "Keep in sync when adding a variant."）；消费侧 `crates/daemon/src/runs.rs:272`（`gates: HarnessKind::ALL…` 建 per-harness 门）。
* **可复现命令**
  ```powershell
  Get-Content crates/core/src/agent.rs | Select-Object -Skip 20 -First 10
  Select-String -Path crates -Include *.rs -Pattern 'HarnessKind::ALL'
  Select-String -Path crates/core/src/agent.rs -Pattern 'enum HarnessKind' -Context 0,8   # 4 个变体
  ```
* **证据读数**：`ALL` 有 4 项，`HarnessKind` 今天也是 4 个变体 —— **当前一致**（不是已存在的缺陷）；但**没有任何机制**保证一致：没有 `match` 穷尽测试、没有 derive、没有 `ALL.len()` 断言。`ALL` 的唯一生产消费者是建门（`runs.rs:272`）⇒ 新增第 5 个 harness 而忘了 `ALL` ⇒ **该 harness 没有门**（并发限制静默失效）。
* **为什么重要**：本代"声称与实现不符/漂移"家族的典型低成本项——注释是唯一的执法者，而这类漂移**只在事故里才被看见**。
* **可证伪的修复判据**：一条测试让"忘记同步"变成**编译/测试失败**，例如在测试里对 `HarnessKind` 做一次穷尽 `match`（新增变体 ⇒ 编译错），或断言 `ALL.len()` 等于穷尽计数值；判据是"往枚举里加一个变体，测试必须红"。
* **建议 owner**：`core`。

---

### C-10（low · 纯度/不确定性）"纯层"的构造器读挂钟，而"时间有序"的承诺只是**进程内**的；同源赋值让一条断言恒真

* **`file:line`**：`crates/core/src/task.rs:83`（`let now = Utc::now();` → `:86-87` 同时赋给 `created_at`/`updated_at`）、`crates/core/src/run.rs:162`（同形）、`crates/core/src/id.rs:26`（`Self(Uuid::now_v7())`）、`crates/core/src/id.rs:3-5`（doc："All IDs are UUIDv7 (time-ordered) … **sort chronologically for free**"）、`crates/core/src/task.rs:106`（`assert!(t.created_at <= t.updated_at)`）。
* **可复现命令**
  ```powershell
  Select-String -Path crates/core/src/*.rs -Pattern 'Utc::now\(\)|now_v7\(\)'
  Get-Content crates/core/src/id.rs | Select-Object -First 8
  Get-Content crates/core/src/task.rs | Select-Object -Skip 100 -First 8      # 恒真断言
  # 依赖里的 uuid 版本与其共享上下文（进程内计数器）
  Select-String -Path Cargo.lock -Pattern 'name = "uuid"' -Context 0,2
  Select-String -Path "$env:USERPROFILE\.cargo\registry\src\*\uuid-1.26.1\src\v7.rs" -Pattern 'fn now_v7|shared_context_v7'
  ```
* **证据读数**：`grep` 命中 `task.rs:83`、`run.rs:162`、`id.rs:26` —— **"纯类型、零 I/O"的 crate 里有三处读挂钟**（不是 I/O，但使构造**不可确定**、无法注入时钟、无法做性质测试）。`id.rs` 的"时间有序"承诺依赖 uuid 1.26.1 `v7.rs:17-19` 的 `shared_context_v7()`（**进程内共享计数器上下文**）⇒ 单调性只在**单进程内**成立。`task.rs:106` 的断言：`created_at`/`updated_at` 来自**同一个 `now`**（`:83` 只调一次）⇒ 两者**恒等**，该断言对"构造器把 `updated_at` 写在 `created_at` 之前"这类真实缺陷**永远不会红**（恒真断言家族）。
* **为什么重要**：`AGENTS.md` 把 `core` 定为 "pure, zero I/O" 的基座，依赖方向严格向下——今天它**确实没有 I/O**（依赖表只有 `serde/serde_json/uuid/chrono`，无 `fs/net/env/thread/static mut`，见 §1.14），但"构造器读全局时钟"让这一层的**可测性**与 doc 的"free chronological sort"承诺都超出了实际。恒真断言则让一条本该守卫时间顺序的测试成为噪声。
* **可证伪的修复判据**：(a) 构造器接受注入的时间（`Task::new_at(now, …)`，`new()` 保留为边界默认）⇒ 测试可以用固定时刻断言 `created_at == updated_at` 与一次显式 `update` 之后 `created_at < updated_at`；(b) `id.rs` 的 doc 改成"进程内单调"（或在跨进程场景不承诺全序）。判据：把 `Task::new` 改成"先写 `updated_at` 再写 `created_at`"之后，**测试必须红**（今天 `:106` 不会）。
* **建议 owner**：`core`。

---

### C-11（low · 不可辨）`path()` 把三种"要人"的原因压成一个无理由的 `Human`

* **`file:line`**：`crates/policy/src/lib.rs:99-128` —— `PermissionPath::Human` 在三条不同理由上返回：`:109-111`（**没有配置 approver**）、`:112-120`（**命中 `high_risk`**）、`:121-126`（**approver 就是提问的 agent**）；类型定义 `:25-32` 不带任何原因字段。
* **可复现命令**
  ```powershell
  Get-Content crates/policy/src/lib.rs | Select-Object -Skip 95 -First 34
  Select-String -Path crates/policy/src/lib.rs -Pattern 'PermissionPath::Human'
  ```
* **证据读数**：三个 `return PermissionPath::Human;`（`:110`、`:119`、`:125`）携带**零信息**；`PermissionPath::Auto(action)`（`:107`）同样不带"来自哪条规则"（C-3）。⇒ 收件箱里的一件待批，事后无法从域模型回答"为什么不是 approver 自动决的"。
* **为什么重要**：`design §9.2` 的 M2 是"approver 代理 + 高危人工升级"；"为什么升级到人"是这条链的审计要点，也是运维排查（"为什么没自动放行"）的第一问。今天它只能靠"人肉重放 `path()` 的输入"来推断。
* **可证伪的修复判据**：`Human` 带上原因（如 `Human { cause: HumanCause::NoApprover | HighRisk { matched: String } | ApproverIsAsker }`），并有测试断言**三种输入产生三个不同的可读原因**。
* **建议 owner**：`policy`。

---

### C-12（low · 可表达性）`AgentCard.command: String` + 空白切分 ⇒ 含空格的 agent 可执行路径无法表达（Windows 上是常态）

* **`file:line`**：`crates/core/src/agent.rs:61-62`（`pub command: Option<String>`）与 `:100-107`（`spawn_command() -> &str`，doc："argv (command string **split on whitespace**; config keeps it simple — adapters may need structured args later)"）；实际切分：`crates/acp/src/run.rs:43-48`（`split_command_line`：`command.split_whitespace()`，doc 自陈 "M1 keeps this deliberately dumb (**no quoting**)"）。
* **可复现命令**
  ```powershell
  Get-Content crates/core/src/agent.rs | Select-Object -Skip 99 -First 9
  Get-Content crates/acp/src/run.rs | Select-Object -Skip 38 -First 11
  Select-String -Path crates -Include *.rs -Pattern 'split_command_line'
  ```
* **证据读数**：`split_whitespace()` 无引号处理 ⇒ `command = "C:\Program Files\acme\agent.exe --acp"` 被切成**三段**，程序名变成 `C:\Program`（不存在）。`core` 的 doc **承认**这件事，但没有测试覆盖含空格路径，也没有在 config 解析处给出警告。
* **为什么重要**：Windows 是第一类平台（AGENTS.md），而 `C:\Program Files\…` 是 Windows 上最常见的安装路径形状；这类错误在**运行期**表现为"agent 起不来"，而错误信息来自进程 spawn（不是"你的 command 里有空格"）。
* **可证伪的修复判据**：`command` 变成 `Vec<String>`（或 `spawn_command()` 返回 `Vec<OsString>`/带引号解析）⇒ 含空格的路径可启动；测试：mock agent 的可执行路径里放一个空格，断言 spawn 成功（今天会失败）。
* **建议 owner**：`core`（类型）+ `acp`（切分/消费）。

---

### C-13（low · 声称与实现不符）三处描述把不存在的东西写成存在，`event.rs` 还有一处 doc 挂错变体

* **`file:line` + 读数**
  1. `crates/policy/Cargo.toml:2` `description = "Permission cascade, routing rules, cost policy"` 与 `AGENTS.md` 的同句：`Select-String -Path crates/policy/src/lib.rs -Pattern 'cost'` ⇒ **0 命中**；`grep -ri 'routing' crates/policy/src` ⇒ 0 命中（路由规则实际在 `orchestrator::RoutingConfig`）⇒ **"cost policy" 在仓里没有家**（三 crate 里 `cost` 只出现在 `core`：`usage.rs:16` 的 `cost_usd: Option<f64>` **读数**，不是策略）。
  2. `crates/orchestrator/Cargo.toml:2` 的 "Task/Run state machines" 见 C-1。
  3. `crates/core/src/event.rs:104-112`：
     ```rust
     /// Our own state machine moved.
     /// A user prompt entering the conversation (chat sessions record
     /// these so transcripts are complete, not assistant-only).
     UserMessage { text: String },
     StateChanged { status: RunStatus },
     ```
     "state machine moved" 这句 doc 被挂在 **`UserMessage`** 上（下方 `UserMessage` 的第 2-3 行才是它真正的说明），而 `StateChanged` **没有 doc**；`event.rs:1-8` 的模块 doc 又承诺"lifecycle events (state changes …) share the same stream so one timeline tells the whole story"。
* **可复现命令**
  ```powershell
  Select-String -Path crates/policy/src/lib.rs -Pattern 'cost|routing'          # 空
  Get-Content crates/core/src/event.rs | Select-Object -Skip 99 -First 16
  Select-String -Path crates/core/src/event.rs -Pattern 'StateChanged' -Context 3,1
  ```
* **为什么重要**：`RunEvent` 是**冻结的转录事件契约**（transcript/panel/回放都吃它），它上面的 doc 是下游读契约的第一手材料；把"状态机移动了"的说明挂到"用户消息"上，会让读 `UserMessage` 的人以为它表示状态变化——这正是本代抓的"声称与实现不符"，成本极低、收益是读者不再被误导。crate description 的偏差则会让找策略的人**去错地方**（我这次就是先按描述去 `policy/` 找路由规则和成本策略）。
* **可证伪的修复判据**：`crates/core/src/event.rs` 里 `grep -B2 'StateChanged'` 显示那句 doc 紧贴 `StateChanged`（且 `UserMessage` 只带它自己的说明）；`Cargo.toml`/`AGENTS.md` 的描述与实际内容一致（要么删掉 "cost policy"，要么指名它住在哪）。
* **建议 owner**：`core`（event.rs 注释）+ 文档/描述（`captain` 或各 crate owner）。

---

## 1.14 「先证伪」读数：我**否掉**的线索（列出来，避免下游重复追）

| # | 线索（看起来像缺陷） | 我做的判据 | 结论 |
| --- | --- | --- | --- |
| F1 | `orchestrator:208` 的 `rule.agent.parse().ok()?` 会让一条坏规则**跳过 default** 直接返回 `None` | 追生产调用链：`crates/daemon/src/api.rs:4692-4717` 的 `routing_decision()` **先**把配置里的名字 `resolve` 成 `AgentCard.id.to_string()`（`resolve = by_name.get(name).map(|c| c.id.to_string())`），再填进 `RoutingRule.agent` | **生产路径不可达**（今天的规则 agent 永远是合法 UUID 串）⇒ 只作 C-7 里的"潜在形状"，**不单列为高危** |
| F2 | `runs.rs:673` 设 `Failed` 之后 `:680` 设 `Queued`，疑似终态→非终态 | 读 `660-690` 全段 | **否**：两者是同一个 `if let Err(e) = reserve_slot(…)` 的**互斥分支**（队列满 ⇒ Failed 并 `return Err`；排队成功 ⇒ Queued）。**无非法转移** |
| F3 | 崩溃恢复把 run 标成 `Interrupted`，疑似覆盖终态 | 读 `crates/daemon/src/lib.rs:110-123` | **否**：只遍历 `Queued/Spawning/Running/WaitingPermission` **四个非终态**（`:110-115`），从不触碰 `Completed/Failed/Cancelled` |
| F4 | 取消分支先发 `Stopped` 再发 `StateChanged`，而别的分支疑似**没有** `StateChanged`（"事件缺失"） | 读 `runs.rs:1472-1504` 的作用域与缩进，并 grep 全部 `StateChanged` 生产者（12 处） | **否**：`1501-1503` 的 `emit(StateChanged{ status: run.status })` 在 `res = &mut driver` 分支内、**在 match 之后**，对 `Completed`/`Failed`/**监督任务 panic** 三个出口都会发；取消分支在 `1467` 自己发过一次 ⇒ 四个终态**都有** `StateChanged`。（仅发现 `Err(join_err)` 出口没有 `RunEvent::Error` 而只有 `StateChanged{Failed}`——不对称但**不是缺失**，故不列 finding） |
| F5 | `core/src/id.rs` 的 `ids_are_time_ordered`（连续两次 `now_v7()` 断言 `a < b`）疑似**随机 flaky** | 读依赖源码：uuid 1.26.1 `src/v7.rs:17-19` 的 `now_v7()` 走 `crate::timestamp::context::shared_context_v7()`（**进程内共享计数器**）；并实测该 crate 的套件 | **否**：同毫秒内由计数器保证递增（该测试在 22 passed 里 **ok**）⇒ 不报（这一点改写成 C-10 里的"仅进程内有序"） |
| F6 | `core` 是否真的零 I/O？（AGENTS.md 硬约定） | 依赖表 `serde / serde_json / uuid / chrono`；`grep 'fs::\|net::\|env::\|std::process\|thread::\|static mut\|lazy_static\|OnceLock'` 在 `crates/core/src` ⇒ 0 命中；`serde_json` 是**生产依赖**（`event.rs:138,153,159,173` 的 `serde_json::Value` 字段）而非仅测试 | **纯度成立**（I/O 层面）；唯一"外部状态"是时钟与 uuid 计数器（C-10） |

---

## 2 未验证猜想（**明确未经证实**，不是 finding）

1. **跨进程 UUIDv7 全序**：`core/src/id.rs:3-5` 说"time-ordered … sort chronologically for free"，而单调性来自**进程内**计数器 —— 两个进程（例如 CLI 与 daemon，或两个 root 的 daemon）在同一毫秒各生成一个 id 时，**有可能**不按创建顺序排列。我没有构造出实例：CLI 的建任务今天走 daemon 的 HTTP（由 daemon 生成 id），`~/.ruagent` 也是单用户单 daemon，所以进不到这个场景。**要证实**：把 `RUST_TEST_THREADS`/双进程探针写在同一毫秒上，比较两个 id 的字节序。
2. **`high_risk` / `title_contains` 的 Unicode 大小写折叠绕过**：匹配用 `title.to_lowercase().contains(&h.to_lowercase())`（`policy:113-117`、`policy:86-88`）。土耳其语 `İ`/点号变体、全角字符、`NFKC` 差异**可能**让一条本应命中 `high_risk` 的标题不命中。我没有构造具体反例，也没有找到本代的 Unicode 规范化约定 ⇒ 只记猜想。
3. **`per_harness = 0` 的确切运行结局**：我据 `runs.rs:663-684` 的 `try_acquire_owned()` 推断"永远抢不到 ⇒ 走排队/拒绝"，但**没有读 `reserve_slot` 的完整算术**（`crates/daemon`，不在本单 inScope）⇒ 结局（是"永远排队"还是"立刻拒绝"）未证实。
4. **`WaitingPermission` run 遇到 daemon 重启**：`runs.rs:1445-1449` 的注释说停掉的 ask sender 会让 ACP 侧读到 `Cancel`（fail-closed），恢复循环会把它标成 `Interrupted`（`daemon/src/lib.rs:110-123`）。**agent 那一侧的真实行为**（收到取消后是回 `Cancelled` 还是继续吐 chunk）我没有验证——无条件起真 harness。
5. **权限/路由的 provenance 是否真的到达人类视野**：C-3/C-7 说的是**记录**不可辨；`rule_id` 是否被 panel/CLI 展示（或只落在 JSONL/SQLite 里）我没查（panel 与 CLI 不在本单范围）⇒ "用户是否看得见"未证实。

---

## 3 未覆盖范围（写清楚，避免读成"查过了"）

* **这三个 crate 的测试套件已实测（不再是缺口）**：`scripts/cargo-team.ps1 test -p ruagent-core -p ruagent-policy -p ruagent-orchestrator` 在**共享单飞锁**上等待超时（300 s 上限到期，wrapper 无输出）；改以 `-NoLock -Jobs 4 -TargetDir %TEMP%\ruagent-verify2-t78` 跑成：**core 11 + orchestrator 4 + policy 7 = 22 passed / 0 failed / 0 ignored**，doc-test 0×3，exit 0。⇒ 本报告的 13 条 finding **全部是在"22 个测试全绿"的基线上**提出的；我没有跑 `clippy`（无读数），也没有 `--release` 或 `--all-targets`。
* **状态迁移只抽样复核**：13 处 `run.status = …` 里我逐段读了 `660-690`、`1445-1504`、`daemon/lib.rs:104-131`（覆盖 5-6 处）；`runs.rs:728/731/757/797/830` 与 `api.rs` 里的 retry 路径**只看了 grep 行**，没有逐段判读 ⇒ "今天不存在非法转移"这句只对我读过的段落成立。
* **`policy` 的解析语义边界**：`RuleConfig.title_contains` 是 `String`（必填，无 `default`）：一个**漏写 `title_contains`** 的规则会让整份 `policy.toml` 解析失败（`PolicyConfig::parse` → `toml::de::Error`）——这方向是 fail-loud，我**没有**测试"整份配置因一条规则拼错而全部失效"在 daemon 启动时的实际后果（是否降级到默认策略？`daemon/config.rs` 不在范围）。
* **未读**：`crates/daemon` 的 `reserve_slot`、retry/`retry_run` 全路径、`api.rs` 的 run 端点全貌；`crates/acp` 除 `split_command_line` 之外的实现；`crates/store` 除两处枚举映射之外的实现（如 `update_run` 的 SQL 形状）；`crates/mcp` 的工具面；panel/CLI 展示层；`docs/plans/2026-09-11-ruagent-design.md` 的 §5.1/§5.2/§5.4/§8.3/§9.2 原文（我用的是代码与注释里的章节引用，**没有逐条比对 spec 正文**——所以"声称"指代码/描述里的声称，不等于 spec 原文）。
* **未做的验证手法**：模糊测试、property 测试（除 C-1/C-2 建议里的形式）、Miri/loom 之类的并发模型检查、`cargo audit`、跨版本 DB 兼容实验（C-6 的"未来变体被当前版本读回"是**推演**而非实验：我没有造一个含未知 `stop_reason` 串的 DB 行去回读——若要做，只需在**临时 root** 里插一行再 `list_runs`）。

---

## 4 复现命令汇总（全部只读）

```powershell
# C-1 状态机没有家
Select-String -Path crates/core/src/*.rs,crates/orchestrator/src/*.rs,crates/policy/src/*.rs -Pattern 'fn .*transition|fn set_status|fn update_status|fn can_'
Select-String -Path crates/core/src/run.rs,crates/core/src/task.rs -Pattern 'pub status'
Select-String -Path crates/daemon/src/runs.rs,crates/daemon/src/lib.rs -Pattern 'run\.status = RunStatus::'

# C-2 谓词矛盾 + 测试锁定 + 零消费
Get-Content crates/core/src/run.rs | Select-Object -Skip 32 -First 8
Get-Content crates/core/src/run.rs | Select-Object -Skip 54 -First 11
Get-Content crates/core/src/run.rs | Select-Object -Skip 185 -First 9
Select-String -Path crates -Include *.rs -Pattern 'is_active\(\)'
Select-String -Path crates -Include *.rs -Pattern 'is_terminal\(\)'

# C-3 权限审计归属错误
Get-Content crates/policy/src/lib.rs | Select-Object -Skip 74 -First 21
Get-Content crates/daemon/src/runs.rs | Select-Object -Skip 1400 -First 12
Select-String -Path crates -Include *.rs -Pattern 'PermissionResolution::Rule'

# C-4 策略编译静默
Get-Content crates/policy/src/lib.rs | Select-Object -Skip 221 -First 28
Get-Content crates/policy/src/lib.rs | Select-Object -Skip 250 -First 9
Get-Content crates/policy/src/lib.rs | Select-Object -Skip 384 -First 11
Get-Content crates/policy/src/lib.rs | Select-Object -Skip 263 -First 22

# C-5 没有超时
Select-String -Path crates/daemon/src/runs.rs -Pattern 'tokio::select!|= .*\.recv\(\)|_ = cancel_token|res = &mut driver'
Select-String -Path crates/daemon/src/runs.rs -Pattern 'sleep|deadline|timeout|Duration::from'
Get-Content crates/daemon/src/runs.rs | Select-Object -Skip 1679 -First 19
Get-Content crates/core/src/run.rs | Select-Object -Skip 79 -First 15

# C-6 持久化读数的 fail-open 兜底
Get-Content crates/store/src/lib.rs | Select-Object -Skip 557 -First 9
Get-Content crates/store/src/lib.rs | Select-Object -Skip 599 -First 34
Get-Content crates/daemon/src/api.rs | Select-Object -Skip 4718 -First 10
Select-String -Path crates/core/src/*.rs -Pattern 'pub const ALL'

# C-7 级联不可辨
Get-Content crates/orchestrator/src/lib.rs | Select-Object -Skip 181 -First 45
Get-Content crates/daemon/src/api.rs | Select-Object -Skip 4691 -First 26
Get-Content crates/core/src/routing.rs | Select-Object -Skip 22 -First 32

# C-8 pipeline 丢字段
Get-Content crates/orchestrator/src/lib.rs | Select-Object -Skip 112 -First 30
Get-Content crates/core/src/task.rs | Select-Object -Skip 80 -First 17
Get-Content crates/orchestrator/src/lib.rs | Select-Object -Skip 191 -First 16

# C-9 / C-10 / C-11 / C-12 / C-13
Get-Content crates/core/src/agent.rs | Select-Object -Skip 20 -First 10
Select-String -Path crates/core/src/*.rs -Pattern 'Utc::now\(\)|now_v7\(\)'
Get-Content crates/core/src/task.rs | Select-Object -Skip 100 -First 8
Get-Content crates/policy/src/lib.rs | Select-Object -Skip 95 -First 34
Get-Content crates/core/src/agent.rs | Select-Object -Skip 99 -First 9
Get-Content crates/acp/src/run.rs | Select-Object -Skip 38 -First 11
Select-String -Path crates/policy/src/lib.rs -Pattern 'cost|routing'
Get-Content crates/core/src/event.rs | Select-Object -Skip 99 -First 16

# 基线（本单实测）：三个 crate 的测试全绿
#   首次在共享单飞锁上等待超时（300s），改 -NoLock + 私有 TargetDir 跑成：
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 test -p ruagent-core -p ruagent-policy -p ruagent-orchestrator -NoLock -Jobs 4 -TargetDir "$env:TEMP\ruagent-verify2-t78"
#   ⇒ core 11 + orchestrator 4 + policy 7 = 22 passed / 0 failed / 0 ignored，doc-test 0×3，exit 0
```

---

## 5 纪律回执

* **只读**：唯一写入 = 本报告文件（`docs/design/reviews/gen3-audit-orchestrator.md`）；`crates/orchestrator/`、`crates/policy/`、`crates/core/` 一行未动；**未写活库**；**未启停 pid 79984**。
* **不把"看起来不对"当 finding**：§1.14 列了 **6 条我否掉的线索**（其中 F1、F2、F3、F4、F5 是"看起来是缺陷、读到底发现不是"）并给出各自判据；C-7 因此从"高危的级联中断"降级为"潜在形状"，C-10 只保留"仅进程内有序"而不是"id 会乱序"。
* **每条 finding 六要素齐**：`file:line` · 可复现命令 · 证据读数 · 「为什么重要」 · **可证伪的修复判据** · 建议 owner；共 **13 条**（≤15），按正确性/可审计性影响排序。
* **否认/猜测/未覆盖分三处**：§1.14（否掉的线索）· §2（5 条未验证猜想，每条写明我为什么没能证实）· §3（未覆盖范围，含**测试基线未跑成**这件事实）。
* **同伴在途编辑**：审计期间 `crates/daemon/src/lib.rs` 的行号发生过位移（我第一遍 grep 到 `:93 run.status = RunStatus::Interrupted`，复核时是 `:117`），`.github/workflows/*` 与 `daemon/src/distill.rs` 也在被改 ⇒ 本报告所有行号以**复核时刻**为准，且我**没有替任何人改任何一行**。
