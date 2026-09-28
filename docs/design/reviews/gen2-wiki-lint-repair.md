# gen2 wiki：lint 修复与「inScope 跨 crate」门禁盲区（t44）

> 单号 t44（repair round 1）· 成员 `wiki` · attempt 1 · 时间窗 2026-09-28 03:0x–03:2x（+08:00）
> inScope（只有这两条路径）：`crates/mock-agent/tests/wiki_pipeline.rs`、本报告
> 上游现场：`integ` 在 t19 的 verify 里跑的 `cargo clippy --workspace --all-targets -- -D warnings` = **exit 101**，两条诊断都在它 inScope 之外 —— 一条归 t19 的 repair（`cli/src/main.rs:809`），**一条归我**（`crates/mock-agent/tests/wiki_pipeline.rs:1225`）。
> 对象集：`crates/mock-agent/tests/wiki_pipeline.rs` **MD5 642073A17E3B0E8D…** @ 2026-09-28T03:05:52（改后；改前见 §1）。

## 0 一句话

`clippy::nonminimal_bool` 已修，**判据没有被删也没有被放宽** —— 那条断言是 RV-D-1 的不变量本体（同一响应不许同时出现 `cite_coverage == 1.0` 与非空 `uncited_sections`），我只是把它从**紧凑写法** `!(a == Some(1.0) && !b)` 改成**两个具名 bool 再 `&&`**，语义逐字不变。两条门禁（**mock-agent** 与 **daemon**，各自 `-CleanFirst` 强制重查）**都绿**，测试 15 passed / 0 failed。本单真正的收获是把这条盲区的形状钉死：**`-p <pkg>` 是包选择，不是文件选择 —— inScope 跨两个 crate 时，门禁必须覆盖两个 crate**（§3，本代第三个实例）。

## 1 改前 → 改后

| # | 命令 | 改前 | 改后 |
| --- | --- | --- | --- |
| A | `clippy --workspace --all-targets -- -D warnings`（t19 现场，**不是我跑的**） | **exit 101**，其中一条：`error: nonminimal_bool --> crates/mock-agent/tests/wiki_pipeline.rs:1225:9` | 本单**不跑**这条（会跨到同伴在途编辑，且它不是我 inScope 的门禁形状）；t19 修完 `cli/` 后应由 integ 复跑 |
| B | `… clippy -p ruagent-mock-agent --all-targets -DenyWarnings -CleanFirst ruagent-mock-agent` | **从未跑过**（这就是它活到今天的原因：t34 的门禁只有 daemon 那条） | **exit 0**，`clean -p ruagent-mock-agent (forced re-check)` → `Checking ruagent-knowledge / ruagent-mock-agent / ruagent-daemon` → `Finished in 8.10s`，**0 条 warning、0 条 error** |
| C | `… clippy -p ruagent-daemon --all-targets -DenyWarnings -CleanFirst ruagent-daemon` | t34 时 exit 0（但它**看不到** `wiki_pipeline.rs`） | **exit 0**，`clean -p ruagent-daemon (forced re-check)` → `Checking ruagent-daemon v0.1.0` → `Finished in 7.03s` |
| D | `… test -p ruagent-mock-agent --test wiki_pipeline` | 15 passed / 0 failed | **15 passed / 0 failed**（exit 0，17.94s）—— 修 lint 没有动任何断言的行为 |

## 2 修法（逐字，含"为什么不许删断言"）

**改前**（`clippy` 的 `nonminimal_bool` 命中点）：

```rust
    let coverage = p["cite_coverage"].as_f64();
    let uncited = p["uncited_sections"].as_array().cloned().unwrap_or_default();
    assert!(coverage.is_none(), "the recorded 1.0 no longer applies: {p:?}");
    assert!(!uncited.is_empty(), "{p:?}");
    assert!(
        !(coverage == Some(1.0) && !uncited.is_empty()),   // ← 1225: nonminimal_bool
        "full coverage next to an uncited section: {p:?}"
    );
```

**改后**：

```rust
    let coverage = p["cite_coverage"].as_f64();
    let uncited = p["uncited_sections"].as_array().cloned().unwrap_or_default();
    // Named parts, then one `&&`: `nonminimal_bool` rejects the compact
    // `!(a && !b)` form (it rewrites to `!a || b`, which no longer reads like the
    // invariant). The assertion itself is unchanged — this is the RV-D-1 judgement
    // body, and it stays asserted verbatim in meaning.
    let full_coverage = coverage == Some(1.0);
    let uncited_non_empty = !uncited.is_empty();
    assert!(coverage.is_none(), "the recorded 1.0 no longer applies: {p:?}");
    assert!(!uncited.is_empty(), "{p:?}");
    assert!(
        !(full_coverage && uncited_non_empty),
        "full coverage next to an uncited section: {p:?}"
    );
```

三点说明：

1. **lint 为什么命中**：`!(a && !b)` 在布尔代数上等价于 `!a || b`，`nonminimal_bool` 认为后者"更小"。**但 clippy 建议的那条形状是有代价的**：`coverage != Some(1.0) || uncited.is_empty()` 读起来像两条互不相干的条件，而原句读起来是**一条不变量**（"不许同时"）。而且我这条断言的**否定面**才是判据：不许 `1.0` 与非空 `uncited` 并存。所以修法是**把两个部分命名**（`full_coverage` / `uncited_non_empty`）再让 `!(A && B)` 保持最小形态（外层没有内嵌否定，`nonminimal_bool` 不再命中），**判据形状与语义都不变**。
2. **没有删断言、没有放宽不变量**（captain 明令）：上面两条 `assert!` 与那条不变量 `assert!` **都在**，一个字都没删。要说明的是：在**这个夹具**里，不变量被前两行蕴含（`coverage.is_none()` ⇒ `full_coverage == false`），但**它仍然是必须存在的判据**——① 它是 RV-D-1 的**判据本体**（caption 原话），② 同一个形状在另一条用例里是**逐页全局断言**（`dry_run_plan_then_confirmed_build_lands_pages` 的循环里对每页断言"不许 `1.0` 与未引用节并存"），删掉这一处就等于把一个不被夹具偶然蕴含的通用判据改成"靠上一个断言顺便保证"。
3. **为什么 t34 没抓到**：见 §3 —— 不是"跑得不够勤"，是**门禁的包选择与 inScope 的文件集不一致**。

## 3 盲区形状：inScope 跨 crate ⇒ 门禁必须覆盖**每个 inScope 文件所属的 crate**

**形状一句话**：`cargo clippy -p <pkg> --all-targets` 选的是**包**，不是**文件**。`--all-targets` 只在**被选中的包**里展开（lib + bins + tests + benches + examples）；一个 inScope 里的文件如果属于**另一个**包，那条门禁**永远编译不到它**，于是"本地绿"与"工作区红"可以同时成立。

**本代三个实例（同一个形状，三种表现）**

| # | 实例 | 谁拥有 | 为什么没被自己的门禁看见 | 被谁抓到 |
| --- | --- | --- | --- | --- |
| 1 | `crates/daemon/src/distill.rs` 的两条 `type_complexity` | graph | graph 只跑 `-p ruagent-graph`，而该文件属 **daemon** crate（graph 自己在报告 §10 R-2b 登记了这个盲区） | 我在 t10 的 `-p ruagent-daemon --all-targets` 门上 |
| 2 | `cli/src/main.rs:809`（`items_after_test_module`）· `crates/mock-agent/tests/` | **无人拥有**（`cli/` 此前全 DAG 没有 owner） | 没有任何单子的 inScope 覆盖它 ⇒ 不会有人为它跑门 | 只有 workspace 门禁（integ 的 t19） |
| 3 | **`crates/mock-agent/tests/wiki_pipeline.rs:1225`（本单）** | wiki（t34 的 inScope 里） | **t34 的 inScope 跨两个 crate**（`crates/daemon/src/wiki.rs` + `crates/mock-agent/tests/wiki_pipeline.rs`），而 t34 只跑了 `-p ruagent-daemon --all-targets` —— 这个文件属 **mock-agent** 包 ⇒ **从未被 lint 过** | 只有 workspace 门禁（integ 的 t19） |

**处方（可机械执行，别靠记性）**

- **从 inScope 的文件反推包集合**再逐个跑门：inScope 给的是**路径**，门禁要的是**包**。映射很便宜（`cargo metadata --no-deps` 的 `targets[].src_path`，或看 `crates/*/Cargo.toml` 的 `[lib]/[[bin]]/[[test]]` 与 `src/`、`tests/` 布局）。本单的实际集合就是 `{ruagent-daemon, ruagent-mock-agent}`。
- **判据两条**（沿用 t34 的仪器教训）：① 命令 `exit=0`；② 输出里**有本次**的 `Checking <pkg>`（配 `-CleanFirst <pkg>` 在同一持锁窗口内强制重查）——否则 `-- -D warnings` 的尾参不进 cargo 指纹，命中缓存时会在 1s 内**假绿**。
- **`--workspace` 是唯一无盲区的形状**，但代价是它会跨到同伴在途编辑（本代多次因此把"同伴的中间态"误判成自己的回归，本单 §1 的 A 行就是这种读数的来源）。所以实践形状是：**inScope 反推的包集合逐包跑门**，把 `--workspace` 留给集成/评审阶段。
- **反事实**：t34 若按本单 §4 的两条门禁跑，这条 lint 在 t34 **当场**就会被抓住（t34 已有两件证据的形状，只是少覆盖了一个包）。

## 4 两条门禁各自"覆盖哪些 target + 本次确实重查了哪个包"

| 门 | 命令 | 本次重查的包（日志行） | 覆盖的 target | 强制重查的证据 |
| --- | --- | --- | --- | --- |
| B（mock-agent） | `clippy -p ruagent-mock-agent --all-targets -DenyWarnings -CleanFirst ruagent-mock-agent` | `Checking ruagent-knowledge` / **`ruagent-mock-agent`** / `ruagent-daemon`（后两个是 mock-agent 的 **dev-dependencies** 带来的：`ruagent-daemon.workspace = true` ⇒ 只编到 daemon 的 **lib**） | mock-agent 的 lib（`src/lib.rs`）+ bin（`src/main.rs`、`src/bin/ruagent-mock-mcp.rs`）+ **7 个集成测试**（`chat_experience` / `e2e_daemon` / `integration` / `judge` / `mcp_health` / `registry_api` / **`wiki_pipeline`**） | 输出含本次 `Checking ruagent-mock-agent`；同一持锁调用内先 `clean -p ruagent-mock-agent (forced re-check)` |
| C（daemon） | `clippy -p ruagent-daemon --all-targets -DenyWarnings -CleanFirst ruagent-daemon` | **`Checking ruagent-daemon`** | daemon 的 lib + lib 单测 + 3 个集成测试（`injection_e2e` / `knowledge_api` / `smoke`；无 bin/bench/example） | 输出含本次 `Checking ruagent-daemon`；同一持锁调用内先 `clean -p ruagent-daemon (forced re-check)` |

**必须收窄的声称面**（graph 提的、t34 采纳）：cargo 的 `Checking` 进度行是**按包**打印的，所以上表只能声称"**这些包**被重查、命令 exit=0"，**不能**逐 target 枚举 rustc 调用清单。**但这不影响本单的结论**：门 B 与门 C 的**包集合不同**，而两个 inScope 文件分别落在两个包里 ⇒

- 门 C **看不到** `crates/mock-agent/tests/wiki_pipeline.rs`（它是 mock-agent 的 test target）；
- 门 B **看不到** daemon 自己的 test target（`injection_e2e.rs` 等）—— 它只编到 daemon 的 lib；
- 所以**两条都是必要不充分**，**合起来**才覆盖了两个 inScope 文件所在的 target。这就是 t34 漏掉它的机制，逐字如此。

## 5 未测 / 没做（不静默）

| # | 项 | 状态 | 原因 |
| --- | --- | --- | --- |
| U-1 | `clippy --workspace --all-targets -- -D warnings` 的**本单后**复跑 | **未测** | 不在本单 verify 里；且它会跨到同伴在途编辑（本单读到的那次 exit 101 是 integ 的现场值，我引用而不冒充）。建议由 t19 的 repair 收口后复跑 |
| U-2 | `cli/src/main.rs:809` 那条 | **不在本单** | 属 t19 的 repair（`cli/` 已由 captain 并入 t19）；我只在 §3 登记它的形状 |
| U-3 | 旧代码反证 | **不适用** | 本单是 lint 修复，不是判据变更；判据本体（RV-D-1 不变量）的存在与否由 t34 的两侧读数负责 |
| U-4 | 逐 target 的 `clippy-driver --crate-name` 清单 | **未取** | 包装脚本的管道会滤掉它（graph 试过）；以 §4 的收窄声称面为准 |
| U-5 | 其它 crate 是否还有同类 lint | **未普查** | 本单只修"属于我的那一条"；全 workspace 的普查是 workspace 门禁（U-1）的职责 |

## 6 纪律回执

- **只改 inScope 两条路径**：`crates/mock-agent/tests/wiki_pipeline.rs`（M，本单唯一的代码改动）+ 本报告（新增）。**没有**改 `crates/daemon/`、`crates/store/`、`crates/memory/`、`crates/graph/`、`crates/knowledge/`、`cli/`、`panel/`。
- **不写活库、不启停进程**：本单只跑 clippy 与 mock-agent 的集成测试（每个测试自带临时 root）；**未触碰 pid 79984**、未写 `~/.ruagent`、未跑 `GET /api/v1/recall`。
- **读数三件套**：对象集 = `crates/mock-agent/tests/wiki_pipeline.rs`（改后 MD5 `642073A17E3B0E8D…` @ 03:05:52）；采样面 = 两条门禁（各自 `-CleanFirst` + `Checking` 行）+ 一条集成测试；可证伪判据 = ① 命令 `exit=0` **且** ② 输出含**本次**的 `Checking <pkg>`（缺 ② 则 ① 什么也不证明）。
- **cargo 纪律**：全部走 `scripts/cargo-team.ps1`（共享 target、一次一个编译、0–11 核、BelowNormal），clippy 用 `-CleanFirst <pkg>`，形状先用 `-DryRun` 核过（`(under the same lock) cargo clean -p ruagent-mock-agent` + `cargo clippy -p ruagent-mock-agent --all-targets -- -D warnings`）。
