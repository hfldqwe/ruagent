# gen4 独立验证 — t129 落地：预算账本到了盘上事件里吗 + 面板命名空间逐格对账（t140）

> 被验对象：`t129` **已推送到 main** 的那部分（提交 **`7a31252`**，`feat(daemon,panel): the injection budget ledger reaches the run path, and the panel stops offering impossible namespaces`；main 当时 = `84e6294`）。单子以 `failed` 收场，但这一批已被 captain 推送，因此**没有独立验证** —— 本单补的正是这一条，且优先补**作者自己点名「未取」的那条读数**。
> 我的写入集合 = 本文件。`crates/**`、`panel/**`、`.github/**`、`scripts/**` **零字节改动**（t129 的字节我一行没动）。
> 环境纪律：活守护进程按 **C32**（8787 = **pid 14944**，`D:\rust_cache\debug\ruagent.exe`，StartTime 2026/10/2 23:05:45）**未触碰**；我的一切读数来自**我自己的临时 root**（`%TEMP%\t140\root{1,2,3}`）+ **我自己的端口**（8831/8832/8833）+ **我自己构建的二进制**；构建走 `scripts/cargo-team.ps1`，**没有** `-TargetDir`、**没有**设 `CARGO_TARGET_DIR`。

## 0 结论

**成立，而且作者自报未测的那条我取到了。** 用 mock-agent harness 跑了 **3 次真实 run**（真 daemon 二进制 + 真 ACP + 真盘上 transcript），三种口径分开、可判：

| 口径 | 盘上 `context_injected` 事件的 `budget` | 关键读数 |
| --- | --- | --- |
| 门禁**开** + 语料**溢出** | **在场**，与预算逐字段吻合 | `per_block_chars=1024`、`total_chars=4096`、`used_chars=3329`、`truncated_blocks=3`、`dropped_items=8`，`blocks=[user_profile(5,1082,截 1221), relevant_memories(8,1092,截 2536), knowledge(3,1075,截 353)]` |
| 门禁**开** + 空语料 + role | **在场且全零 = 一次实测** | `used_chars=0, truncated_blocks=0, dropped_items=0, blocks=[]`，而 `render` 非空（152 字的 role） |
| 门禁**关** + role | **键不在场**（= `None`，"未采集"） | `event` 的键只有 `path/render/type`；`render` 非空（152 字，说明"注入非空"与"采集过"是两件事） |

**四条独立交叉验证**（都在盘上那份事件内部，不依赖作者的叙述）：
1. `per_block_chars`/`total_chars` == **源码里的默认值** `InjectionBudget::default()`（`crates/memory/src/inject.rs:15-22`：`per_block: 1024, total: 4096`）✓；
2. **账本与字节同源**：无 role 的那次 `len(render) == used_chars == 3329` **精确相等** ✓（有 role 的那次差值恰是 role 文本，说明 `used_chars` 只记渲染自己产出的字节 —— 见 §3 的口径说明）；
3. `truncated_blocks=3` == `blocks` 里 `truncated_chars>0` 的块数 ✓；`dropped_items=8` == 渲染文本里的 `[+8 items dropped: context budget reached]` ✓，且**被丢掉的那个块的标记 `T140-PROJECT-0` 不在渲染里**，而保留块（`T140-PROFILE-0`/`T140-OBS-0`/`T140-KNOWLEDGE-0`）都在 ✓；
4. 同一事件经 **SSE 面**（`GET /api/v1/runs/{id}/events`）复读，形状一致 ✓；mock agent 回显的是**同一个注入串**（`agent_message_chunk` 里 `echo: <user_profile>…`）⇒ 账本描述的就是**真正进入 prompt 的那份字节** ✓。

**面板**：`NAMESPACES` 与 `MemoryStore::allowed_kinds()` **10/10 格相等**（4 个 store），`observation` **不**声称 `global` ✓；`DEFAULT_NAMESPACE` 四格都是**具体值**（`user`/`global`），理由成立（族名不可选）✓；`nsOptions` 只在 store 的族表包含 `project`/`agent` 时才并入对应的数据派生值 ✓。

**两条要和 captain 说明的差异**（不调和）：
1. **`cargo fmt --all --check` 我这次是 exit 1**，但**红的不是 t129 的字节**：`Diff in …crates/daemon/tests/injection_e2e.rs:929` 与 `:974`，而该文件此刻正被**同伴在途编辑**（`git status --porcelain` = ` M crates/daemon/tests/injection_e2e.rs`；diff 内容正是另一个单在往那里加"`used_chars` == 各块之和"的断言）。t129 自己的文件单独跑 `rustfmt --edition 2024 --check crates/daemon/src/runs.rs` **exit 0**、`event_compat.rs` **exit 0**；`runs.rs` 在 porcelain 里是**干净**的（worktree == 已提交字节，`8C0C0BCA72944042`）。
2. **`event_compat.rs` 的坐标与 captain 引的差两行**：旧事件 `budget == None` 的断言在 **`:38`**（`"an old event carries no budget: not measured"`），captain 引的是 `:36`（那一行是 `path == None` 的断言 `:34-37`）。方向一致，坐标以我的读数为准（同一份字节 `D6BD85FF45F6E631…`）。

## 1 方法与仪器（可复现）

| 步骤 | 具体 | 读数 |
| --- | --- | --- |
| 私有构建（避开共享 exe） | `scripts/cargo-team.ps1 rustc -p ruagent --bin ruagent -- -o %TEMP%\t140\bin\ruagent.exe`；`… -p ruagent-mock-agent --bin ruagent-mock-agent -- -o %TEMP%\t140\bin\ruagent-mock-agent.exe` | exit 0 / 63.1 s（328,089,600 B，sha256 `6A7910A805D55672…`）与 exit 0 / 27.2 s（5,826,560 B，sha256 `B2A0D05B5B634E45…`） |
| **C35 年份探针** | 在二进制里找 t129 才有的字符串 `serialising the injection budget report` | `ruagent.exe` = **True** ⇒ 这是 **t129 之后的**二进制 ✓（我没有拿共享 exe 读数） |
| 驱动 | `%TEMP%\t140-drive.py <mode>`：起我自己的 daemon（`serve --addr 127.0.0.1:88xx --root %TEMP%\t140\rootN`）→ 造语料 → `POST /api/v1/tasks` → `POST /api/v1/tasks/{id}/runs {"agent":"mock"}` → 轮询到 `completed` → **读盘上 transcript** | 见 §2 |
| mock harness | `agents.toml`：`[agent.mock] harness="mock"; command="<TEMP>/t140/bin/ruagent-mock-agent.exe --behavior echo"`（**正斜杠**）；`policy.toml`：`[permissions] default="allow"`（门禁关那次另加 `[capabilities.memory_inject_runs] enabled=false`） | 3 次 run 全部 `completed` |
| 语料（"溢出"的那次） | 21 条 memory（profile/user ×5、observation/user ×8、observation/project:t140 ×8，各 ~430 字）**经 API 写入** + `POST /api/v1/knowledge/ingest`（6 chunk 的 markdown，`HTTP 200 {'chunks': 6}`）；wiki 那条 `POST /api/v1/knowledge/wiki/build` 回应 `{'status':'running','pages_planned':0}`、`wiki/pages` 为空 ⇒ 本次只有 3 个 tag 块 | 见 §2 |
| 事件取证 | 盘上文件 `%TEMP%\t140\rootN\data\transcripts\run-<id>.jsonl` 的 `seq=1` 行；我用 `%TEMP%\t140-analyze.py` 逐字段比对；三条事件行的副本留在 `%TEMP%\t140-event-lines.jsonl`（4,499 B / 3 行） | §2/§3 |

## 2 盘上事件的原文（我的，逐字；只节选 render 的头部）

**门禁开 + 语料溢出**（`run-01a102af-1f4e-712c-9138-494038eed41f.jsonl`，`seq=1`，`ts=2026-10-03T16:53:13.707044Z`）：
```json
{"ts":"2026-10-03T16:53:13.707044Z","seq":1,"event":{"type":"context_injected",
 "render":"<user_profile>\n[2026-10-03] T140-PROFILE-0 ppp…\n[2026-10-03] T140-PROFILE-1 ppp…\n… <relevant_memories>\n[2026-10-03] T140-OBS-0 ooo…\n… <knowledge>\nt140-corpus: # T140 corpus overflow\n…\n<context_budget>\n… [+8 items dropped: context budget reached]\n</context_budget>\n",
 "path":"run",
 "budget":{"per_block_chars":1024,"total_chars":4096,"used_chars":3329,"truncated_blocks":3,"dropped_items":8,
   "blocks":[{"tag":"user_profile","items":5,"chars":1082,"truncated_chars":1221,"dropped_items":0},
             {"tag":"relevant_memories","items":8,"chars":1092,"truncated_chars":2536,"dropped_items":0},
             {"tag":"knowledge","items":3,"chars":1075,"truncated_chars":353,"dropped_items":0}]}}}
```

**门禁开 + 空语料 + role**（`…519f-74bb…jsonl`，`seq=1`）：
```json
{"ts":"2026-10-03T16:53:26.575317800Z","seq":1,"event":{"type":"context_injected",
 "render":"[role — you are]\nT140 ROLE MARKER: …(152 chars)…",
 "path":"run",
 "budget":{"per_block_chars":1024,"total_chars":4096,"used_chars":0,"truncated_blocks":0,"dropped_items":0,"blocks":[]}}}
```

**门禁关 + role**（`…5ad2-7754…jsonl`，`seq=1`）：
```json
{"ts":"2026-10-03T16:53:28.917495600Z","seq":1,"event":{"type":"context_injected",
 "render":"[role — you are]\nT140 ROLE MARKER: …(152 chars)…",
 "path":"run"}}
```
⇒ **键 `budget` 根本不在场**（不是 `"budget": null`）：`budget` 字段带 `#[serde(default, skip_serializing_if = "Option::is_none")]`（`crates/core` 的 `RunEvent::ContextInjected`），所以盘上的形状是"缺席"，读回来才是 `None` —— 这也正是 `event_compat.rs:38` 对**旧事件**的断言口径。

## 3 逐字段比较（我自己算的，不是转述）

| 盘上字段 | 值 | 我的独立比对依据 | 结论 |
| --- | --- | --- | --- |
| `per_block_chars` | 1024 | 源码 `InjectionBudget::default()`（`inject.rs:15-22`，同文件 sha256 `9935892772FD4F46…`） | **相等** |
| `total_chars` | 4096 | 同上 | **相等** |
| `used_chars` | 3329 | `len(render)`（字符数，逐字取自同一事件）= **3329** | **精确相等**（无 role 的那次） |
| `used_chars`（空语料那次） | 0 | `len(render)` = 152 = **role 文本 + 分隔符**；`used_chars` 只记渲染产出的字节 ⇒ 0 是"渲染产出了 0 字"，不是"没采集" | 口径可分 |
| `truncated_blocks` | 3 | `blocks` 里 `truncated_chars>0` 的块数 = **3** | **相等** |
| `dropped_items` | 8 | `render` 里的 `[+8 items dropped: context budget reached]` = **8**；且被丢块（project_context）标记 `T140-PROJECT-0` **不在** render，保留块标记都在 | **相等**（账本与字节互证） |
| `blocks[].items/chars/truncated_chars/dropped_items` | 见 §2 | 三块之和 3249 + 掉落通知（80 字）= 3329 = `used_chars` | **自洽** |
| `blocks` 缺 `project_context` | —— | 掉落顺序注释（`runs.rs:2028-2032`：user_profile > relevant_memories > knowledge > wiki > project_context，最后被丢）与实测一致 | **一致** |

**"没有读数 ≠ 读数是零"这条线，本单给的是盘上读数**：同一字段在同一事件里，门禁关 = **键缺席**（未采集），门禁开 + 空语料 = **键在场且全零**（读过，就是零），门禁开 + 溢出 = **键在场且非零**（并且文本里有 8 条掉落的可见通知）。三种是三种不同的盘上形状，不是同一种的三种说法。

## 4 向后兼容（我本人跑的）

- `scripts/cargo-team.ps1 test -p ruagent-daemon --test event_compat` ⇒ **exit 0**，日志 20 行，`running 2 tests` → `test result: ok. 2 passed; 0 failed`。
- 断言原文（我读的当前字节，`crates/daemon/tests/event_compat.rs` sha256 `D6BD85FF45F6E631…`，mtime 2026-09-29）：`:21 fn an_old_context_injected_event_still_deserializes()`，`:22-23` 断言旧 JSON（无 `path`、无 `budget`）**仍能反序列化**，**`:38 assert_eq!(budget, None, "an old event carries no budget: not measured");`**，`:31` 旧 render 字节逐字保留。⇒ 旧事件读 `None` 成立 ✓（**坐标更正**：captain 引的 `:36` 是 `path` 那条；`budget` 那条在 `:38`）。

## 5 序列化失败是不是真错误（口径 + 判据）

- **坐标**：`crates/daemon/src/runs.rs`（sha256 `8C0C0BCA72944042…`，mtime 2026-10-04T00:35:46）`:1391-1397`
  ```rust
  let budget = match &injection_budget {
      Some(report) => Some(serde_json::to_value(report)
          .with_context(|| "serialising the injection budget report")?),   // :1393-1394
      None => None,
  };
  ```
  ⇒ 它是 `with_context(...)` + `?`：**真 `Err`**，不是 `None`、不是 `unwrap_or_default`、不是 `ok()`。**判据**（为什么这比"看起来对"强）：`?` 在 `supervise(...) -> Result<()>` 内，返回途中在 `runs.rs:892-897` 被记账 —— `tracing::error!(run_id, error, "run supervisor failed")` + `run.status = RunStatus::Failed`。也就是说：**若序列化失败，run 会以失败收场并留下一条带 error 的日志**，而不是给盘上留一个"没采集"的事件 —— 这正是本代反复打的"静默 `None`"的反面。
- **"构造一个不可序列化的值去实测"我做不到，且我认为它对本类型不可达**：`BudgetReport` 的字段全是整数 / `String` / `Vec<BlockReport>` / `Option<String>`（`inject.rs:672-682`），`serde::Serialize` 对这些类型**不会失败**（能失败的是非字符串 map key、自定义 `serialize`、含浮点 NaN 到 JSON 之类，本类型都没有）。⇒ 这是**结构性不可达**，不是"没测"：它在类型变更（未来给账本加一个可失败的字段）时才可能被走到，而到那时 `?` 的形状保证了它仍然响亮。**我没有为了取证去改 `crates/**`**（那就是违规）；若要让它可测，需要的是"能失败的 `Serialize` 字段 + 一条把错误映射成 Failed 的用例"，这属于实现侧的选择（见 §7 未覆盖）。

## 6 面板命名空间矩阵（逐格）

后端真值 `MemoryStore::allowed_kinds()`（`crates/memory/src/lib.rs:99-110`，sha256 `5A1E1E82BAF394A4…`）：
`Profile => [User]` · `Observation => [User, Project, Agent]`（注释明说 **NOT global**）· `Procedure | Lesson => [Project, Global]`。

面板（`panel/src/views/Memory.tsx`，sha256 `B2BA6901D9E76554…`，mtime 2026-10-04T00:33:04）：

| store | `allowed_kinds()` | `NAMESPACES`（`:45-50`） | 逐格 |
| --- | --- | --- | --- |
| profile | [user] | `["user"]` | ✓ 1/1 |
| observation | [user, project, agent] | `["user","project","agent"]` | ✓ 3/3（**不含 `global`** ✓） |
| procedure | [project, global] | `["project","global"]` | ✓ 2/2 |
| lesson | [project, global] | `["project","global"]` | ✓ 2/2 |

合计 **10/10 格相等**。表上方的注释 `:34-44` 把真值坐标（`lib.rs:99-109`）与 t52 的错法（`observation` 曾声称 `global`）都写明了 ✓。

- **`DEFAULT_NAMESPACE`（`:52-61`）**：`profile→"user"`、`observation→"user"`、`procedure→"global"`、`lesson→"global"` —— 四格**都是具体值**；理由（`:52-55`）成立且我核对了它的论据：procedure/lesson 的族表**首项是 `project`**（族名本身不是可写值，具体值是 `project:<name>`），所以"取族表第一项"在语法上就会给出一个不可写的值 ⇒ 不能是 `NAMESPACES[s][0]` ✓。用途也核对了（`:262`/`:286` 切 store 时落到该默认值）✓。
- **`nsOptions`（`:214-223`）**：`user`/`global` 直接取本 store 表里的具体单例，`project:*` 与 `agent:*` **各自只在 `NAMESPACES[store].includes("project"/"agent")` 为真时**才并入，且都来自(`s === store` 过滤后的)真实数据计数 ⇒ **不再把 `project:*`/`agent:*` 无条件并进每个 store** ✓（`:214-216` 的注释也把"t129 之前两者对每个 store 都并"写成历史）。
- 新族值的入口 `<NewNamespaceInput>`（`:467`）按 store 给出前缀：`profile → ""`（不可新增族值）、`observation → "project:"`、`procedure|lesson → "global"` ⇒ **UI 不会提示出一个后端会拒绝的格子**（我读的是它能产出的前缀集合：`user`/`project:*`/`global`，都落在各族允许的集合里；`observation` 那条路径**不产生 `global`**）✓。

## 7 门禁（我本人在最终字节上重跑，给计数）

**字节归属**：`crates/daemon/src/runs.rs` sha256 `8C0C0BCA72944042…`（porcelain 干净 ⇒ worktree == 提交 `7a31252` 的字节；mtime 2026-10-04T00:35:46）；`panel/src/views/Memory.tsx` `B2BA6901D9E76554…`；`crates/memory/src/lib.rs` `5A1E1E82BAF394A4…`；`crates/memory/src/inject.rs` `9935892772FD4F46…`；`crates/daemon/tests/event_compat.rs` `D6BD85FF45F6E631…`。我的读数取自 2026-10-04 00:53–00:56。

| # | 命令 | 退出码 | 计数/原文 |
| --- | --- | --- | --- |
| G1 | `scripts/cargo-team.ps1 test -p ruagent-memory` | **0** | 日志 130 行；4 target：**79 passed/0 failed**、4 passed/0 failed、9 passed/0 failed、doc 0 |
| G2 | `scripts/cargo-team.ps1 test -p ruagent-daemon --test event_compat` | **0** | 日志 20 行；`running 2 tests` → **2 passed / 0 failed** |
| G3 | `scripts/cargo-team.ps1 clippy -p ruagent-memory --all-targets -DenyWarnings` | **0** | 日志 10 行；`Finished`；**`warning:` 行 0** |
| G4 | `cargo fmt --all --check` | **1** | 16 行；`Diff in …crates/daemon/tests/injection_e2e.rs:929` 与 `:974` ⇒ **同伴在途编辑**（porcelain ` M crates/daemon/tests/injection_e2e.rs`；内容是另一个单在加"`used_chars` == 各块之和"的断言）。**t129 自己的字节是干净的**：单独 `rustfmt --edition 2024 --check crates/daemon/src/runs.rs` **exit 0**、`event_compat.rs` **exit 0** |
| G5 | `npm --prefix panel run build` | **0** | 日志 107 行；`✓ built in 4.57s`、`build-panel: dist updated (55 assets, index.html swapped by rename)`（⇒ 面板的类型检查/构建在 t129 的 Memory.tsx 字节上通过） |

## 8 未测 / 未覆盖（第 19 条）

1. **`dropped_items` 只在我这次语料组合下取到（=8，project_context 整块被丢）**；`wiki` 那一路没产生块（`pages_planned:0`、`wiki/pages` 为空），`graph` 块也没有 ⇒ **5 个 tag 同时在场的最坏情况没有测**。要测需要：先让 wiki 真的建出页面（`POST /api/v1/knowledge/wiki/build` 的语义 + 一个能产出页面的 agent）或注入图证据。
2. **序列化失败路径没有活体读数**（§5：本类型结构性不可达；要做需要实现侧加一个可失败的字段或用例，属于改 `crates/**`，我不做）。
3. **`retry-prefix` 那条 "注入非空但 `None`" 的分支**：我用的是 **role** 路径（同一类的"无渲染也能有字节"），**没有**用 `mockcrash` + `/runs/{id}/retry` 复现 crash-snapshot 前缀那条（`injection_e2e.rs` 里有 `--behavior crash` 的现成卡，属于我未走的那条）。
4. **chat 路径**（`memory_inject_chat`）的账本：t129 只动 run 路径，本单也只验 run 路径；`chat` 侧的 `budget` 仍可能缺席（未测，不在本单声称范围）。
5. **面板的交互层**：我只做了**静态逐格对账 + 构建门禁**，没有起浏览器点一遍下拉（`panel/e2e` 我没有跑，且 settings/e2e 会写真实配置，按 AGENTS.md 要经 `run-e2e.mjs` 才允许）。
6. **`used_chars` 与"注入到 agent 的字节数"的差**：有 role 时二者不同（152 vs 0），我按只读的方式说明它是"渲染产出"的会计；**没有**去测"agent 实际读到多少"（那是另一个量，t129 也没声称）。

## 9 收尾与残留

- 我起的进程：**3 个临时 daemon**（pid **10252** / **3784** / **22996**，端口 8831/8832/8833），逐个 `alive after stop = False`；**8787 的常驻 pid 14944 未触碰**。
- 我的临时文件：`%TEMP%\t140\{bin,root1,root2,root3}`（含 328 MB + 5.8 MB 二进制、2 GB `.pdb`、三个 root 的库与 transcript）、`%TEMP%\t140-*.py/.ps1/.log`。**收尾时删掉 `%TEMP%\t140`，保留小日志与三条事件行副本**（`%TEMP%\t140-event-lines.jsonl`，4,499 B）作为证据；残留读数写进任务 output。
- **没有写活库**（`~/.ruagent` 只读未写）；**没有改任何被测字节**。
