# O1 修复：让「未被告诉」在整条链上活下来（C1–C4）

**任务**：`t182`（kind = repair，**产品行为修复，中优先、非紧急**）· **日期**：2026-10-04
**inScope 写入**：`crates/acp/src/chat.rs`、`crates/daemon/src/chat.rs`、`crates/daemon/src/api.rs`、本文件
**来源**：`t180` 的 O1 结论（选 ②：live 会话首答「不应当权威」）+ C1–C4 四条可机械化判据。

**最终字节（含入库 blob，C52 口径）**

| 文件 | sha256 | git blob |
| --- | --- | --- |
| `crates/acp/src/chat.rs` | `608683429DE386E8502558D1D3EEC84C93EA8745D49661E6E4C82AE7A63118F4` | `9e3a1f39e51734c83464088d8c7e96167418e6e6` |
| `crates/daemon/src/chat.rs` | `FBEA0521560EB657AC727C2C6B7072C3DEBFE61445E57EC73C78C171C2AA46A5` | `1e3386e0ba76201062a9407442d90c84f3b0f141` |
| `crates/daemon/src/api.rs` | `E07B0F79500BAD4838C7BE5304AF27B5F8A5E5F887D42BA513C8E6660252A41B` | `ea9b3cd699399673cb93cdccab6b9a21b342d0fc` |

**一句话**：把「**未被告诉**」这条信息从发布端一路保到消费面 —— 发布端不再把 `None` 折叠成空广告；跟踪器只跟踪**报告过**的状态；开机不回填空目录；路由输出 `known` 字段让面板能区分「不知道」与「没有」。**没有反向改** `record_probe`（t192 策略原样保留）。

---

## 1. C1 —— 发布端不再折叠 `None`（先决条件）

**改前**（`crates/acp/src/chat.rs:442-445`）：
```rust
            let advertised = extract_options(session.config_options.as_deref().unwrap_or(&[]));
            options_tx.send_replace(Some(advertised));
```
`unwrap_or(&[])` 把「服务端什么都没说」（`None`）与「服务端说了：没有」（`Some(&[])`）**压成同一个值**；而通道的初始值本来就是 `None`（`chat.rs:235-236 watch::channel::<Option<Vec<SessionOptionState>>>(None)`）⇒ **「尚未报告」这个状态本来就被类型表达着**，是这行把它抹掉了。

**改后**：新增纯函数（与 `extract_options` 相邻，附 t182/C1 的文档）：
```rust
fn advertised_options(
    config_options: Option<&[SessionConfigOption]>,
) -> Option<Vec<SessionOptionState>> {
    config_options.map(extract_options)
}
```
调用点改为：
```rust
            if let Some(advertised) = advertised_options(session.config_options.as_deref()) {
                options_tx.send_replace(Some(advertised));
            }
```
⇒ **`None` 保持通道的 `None`**（「未被告诉」），`Some([])` 仍然发布（「被告知：没有」），两者**在类型上不同形**。

**判据（C1）落实**：`as_deref().unwrap_or(&[])` 这条**折叠在该路径上消失**。
**点名的其余两处（为什么那里不构成同一问题）**：`acp/src/chat.rs` 另有两处 `send_replace(Some(...))` ——
- `:470` 与 `:612`，二者都取 `resp.config_options`（**一次应答里给出的具体列表**，不是 `Option`）⇒ 那是**第一手回答**，`Some([])` 在这里的诚实含义就是「它回答：没有」，**不涉及折叠**；
- **`crates/acp/src/run.rs:265`** 仍是同一写法 `extract_options(session.config_options.as_deref().unwrap_or(&[]))` ⇒ **在本单 inScope 之外，只登记**（见 §7）。

**可机械化的读数（不是「0/N 绿」）**：新增 `acp::chat::tests::a_silent_server_is_not_an_empty_advertisement`，断言 `advertised_options(None).is_none()` 且 `advertised_options(Some(&[]))` 是 `Some(长度 0)` ⇒ 「未被告知」与「被告知没有」**必须不同形**。

## 2. C2 —— 跟踪器只跟踪**报告过**的状态

**改前**（`crates/daemon/src/chat.rs:953-967`）：分支判据**只有「变了」**（`last.as_ref() != Some(&state)`）⇒ 任何 `Some(state)`（含空）都 `cache.insert(...)` **并** `persist_options(...)`；**「未被告诉」在新形状下是 `None`，但旧形状下它被折叠成 `Some([])` ⇒ 于是被当作知识写进缓存和 `agent_options` 表**。

**改后**：把判定拆成可测的纯函数（与 `record_probe` 当年从 `probe_options` 拆出同因）：
```rust
fn catalog_to_track(
    state: Option<&[SessionOptionState]>,
    updated_at: i64,
) -> Option<CachedOptions> {
    let options = state?;                       // 「未被告诉」=> 不跟踪
    Some(CachedOptions { options: options.to_vec(), updated_at, known: true })
}
```
跟踪器循环改为 `if let Some(entry) = catalog_to_track(Some(state.as_slice()), updated_at) { insert; persist_options(..., &entry.options, ...) }` ⇒ **`None` 不可达 cache+persist 分支**，且写入的条目**带 `known: true`**（见 C4）。

**测试形状照 t192 那条**（不新建 agent）：新增 `chat::generating_tests::the_tracker_tracks_only_what_a_runtime_reported`：
- `catalog_to_track(None, 7).is_none()` ⇒ **缓存与表都不可达**（这正是 O1 路径）；
- 一个**报告了空**的运行时是**知识**（`known: true`，`options` 空）——与「未被告知」**由 `known` 区分**；
- 一个**报告了选项**的：`options.len() == 1`、`updated_at` 原样。
（该测试**只测判定函数**，与 `record_probe` 那条同一形状：判定是契约，spawn 不是。）

## 3. C3 —— 开机不回填「从未第一手报告过」的空目录

**改前**（`load_option_cache`，`:1425-1434`）：`cache.entry(runtime).or_insert(CachedOptions{options, updated_at})` ⇒ 表里任何一行（**含 `'[]'`**）都会成为权威目录；守护进程**重启后** `GET /api/v1/agents/{name}/options` 默认就服务这份持久化目录 ⇒ 空 picker **跨重启固化**（t192 描述的故障形状）。

**改后**：解析出的 `options.is_empty()` ⇒ **`continue`**（不回填、不计入 seeded 计数），非空的照旧 `known: true` 回填。理由写进注释：**存储里的空目录既可能是 pre-t182 折叠产生的残件，也可能只是「被告知没有」；无论哪种，它在本进程里都不是第一手读数** ⇒ 让该运行时保持 **UNKNOWN**（由 C4 如实报出），而不是断言「它没有」。

**可机械化读数**：新增 `chat::generating_tests::an_empty_catalog_row_is_not_seeded_as_knowledge` —— 直接向 `agent_options` 插两行（`'silent' → '[]'`、`'real' → [一个真选项]`），调 `load_option_cache()`，断言 **`model_cache` 里没有 `silent`**、而 `real` 被回填且 `known == true`。

## 4. C4 —— 路由能区分「未知」与「没有」

**载体**：`CachedOptions` 增加一个字段
```rust
pub struct CachedOptions {
    pub options: Vec<SessionOptionState>,
    pub updated_at: i64,
    pub known: bool,     // t182/C4
}
```
赋值规则：跟踪器写入 ⇒ `true`；`record_probe` 的 `Reported(...)` ⇒ `true`；**`record_probe` 的「从未被告知」（超时且无上一次）⇒ `false`**（它返回 `previous.unwrap_or(CachedOptions{ options: vec![], updated_at, known: false })`）；`load_option_cache` 回填 ⇒ `true`（且不回填空的）。
**路由**（`api.rs:4822-4833`，处理器 `agent_options`，路由 `:39 GET /api/v1/agents/{name}/options`）新增一个字段：
```rust
        "known": entry.known,
```
⇒ 响应现在是 `{agent, options, cached, known, updated_at}`。

**面板应如何据此收紧（面板改动不在本单）**：`Runtimes.tsx` 的 `useModelCounts`（`:22-38`，那条 `t84 / audit P3` 注释所在）今天用「探针失败」来避免把「0 models」渲染成事实；现在它还应把 `known === false` 当作**同一种情形**处理（显示「未知 / 重试」而不是 0），因为走跟踪器那条路时**没有失败**可依凭 —— 这正是 t180 §2 说的「那道防线救不了它」的补法。具体位置：`Runtimes.tsx` 的模型数 chip、`Agents.tsx` 与 `Chat.tsx` 的选项目录（都经 `panel/src/api.ts:864 agentOptions`）。**建议另立面板单**（本单只做后端信号，按契约）。

## 5. 明确**没有**做的事（「不许反向改」）

`record_probe` 的**策略一字未动**：`NotReportedInTime | ReportedNone` 臂仍然**只读 cache、保留上一次、不 insert、不 `persist_options`**（t192/t172 的规则）。本单只往里加了 **`known` 的取值语义**（有上一次 ⇒ 沿用上一次的 `known`，那是我们已经知道的事；无上一次 ⇒ `false`），**没有**让它接受空。证据：t192 那条测试 `an_empty_probe_neither_overwrites_nor_persists_the_catalog` **仍在并通过**（§6），且它对「超时」新增的断言是「保留的上一份是知识（`known` 仍为 true）」+「从未报告过的运行时是 `known: false`」——**两个方向都断言了**。

## 6. 证据（C47 口径：本处**无前置红证**）

**因此本单不用「0/N 绿」收口**，也不声称「修好了一条会红的 flake」。本单声称的是：**移除了一个把「不知道」渲染成「没有」的路径**，并用**可判定的读数**支撑：

| 读数 | 内容 |
| --- | --- |
| ① C1 的不折叠 | `advertised_options(None).is_none()` 且 `advertised_options(Some(&[]))` 为 `Some(len 0)`（新测试） |
| ② C2 的跟踪器规则 | `catalog_to_track(None, _).is_none()`；报告了空 ⇒ `known: true`；报告了选项 ⇒ 内容与时间原样（新测试） |
| ③ C3 的开机行为 | `'[]'` 行**不进** `model_cache`；非空行进且 `known: true`（新测试） |
| ④ C4 的信号 | `CachedOptions.known` 经 `record_probe` 的「从未被告知」分支取 `false`、经报告取 `true`（t192 测试里新增的两条断言），并由路由输出为 `"known"` |
| ⑤ t192 的既有测试 | **仍通过**（`an_empty_probe_neither_overwrites_nor_persists_the_catalog`） |
| ⑥ **确定性红证（单点变异 ×2）** | 见下 |

**确定性红证（窗口 `20:03:04 → 20:03:40`，开窗前已宣告含两锚；共享树按字节恢复）**
- **变异 A**（`crates/acp/src/chat.rs`，非 test 路径 `advertised_options`）：`config_options.map(extract_options)` → `Some(extract_options(config_options.unwrap_or(&[])))`（**重新引入折叠**）⇒ **exit=101**，`panicked at crates\acp\src\chat.rs:677`：「`config_options == None` (never told) must NOT publish an advertisement」。
- **变异 B**（`crates/daemon/src/chat.rs`，非 test 路径 `catalog_to_track`）：`let options = state?;` → `state.unwrap_or(&[])`（**重新让「未被告知」被当作空目录跟踪**）⇒ **exit=101**，`panicked at crates\daemon\src\chat.rs:2101`：「`None` (never reported) must reach neither the cache nor `agent_options`」。
- **恢复**：开窗前 `acp` sha256 `60868342…`/blob `9e3a1f39…`、`daemon` sha256 `FBEA0521…`/blob `1e3386e0…`；恢复后**同两值逐字相同** ✓，`t182 WINDOW` 标记 **0** 处 ✓，按 **C52 逐行读回**两个站点（`acp chat.rs:123 config_options.map(extract_options)`、`daemon chat.rs:1789 let options = state?;`）✓，并按 **C49 刷新 mtime 之后**才跑门禁 ✓。

## 7. ⚠️ 一次窗口纪律事故（如实登记，含新的陷阱）

**我的 `finally` 恢复块整体失败**：我在同一段脚本里用了 `$A`/`$B` 保存路径、又用 `$a`/`$b` 保存测试输出 —— **PowerShell 变量名大小写不敏感** ⇒ `$a = powershell ...` **把路径变量覆盖成了输出行数组**，于是 `Copy-Item <备份> $A` 的 Destination 变成 `System.Object[]` ⇒ 报错、**没有恢复**。我**从错误输出立刻发现**并在**下一条命令**里恢复（`20:03:40`），因此两文件的变异状态持续 **36 秒**（宣告的是「~30-40 秒」⇒ **仍落在宣告时长内**），最终字节经 sha256 **与** blob **双锚核对逐字相同**、且门禁是在恢复并刷新 mtime 之后跑的。
**教训（建议入账）**：**PowerShell 里不要用与路径变量同名（仅大小写不同）的变量存输出** —— 这会把一条「恢复」语句变成一个**静默失败的 no-op**，而 `finally` 里失败**不会阻止脚本继续往下走**（这次的执行顺序是：`try` 里变异+跑测试 → `finally` 里恢复失败 → 后续命令继续）。⇒ 与 C49（`Copy-Item` 带旧 mtime）同源：**窗口的「恢复」必须自证**（恢复后逐字核对 + 标记检查），不能假定 `finally` 成功。

## 8. 门禁（最终字节，退出码**先于任何管道**取值；逐 target 计数）

| 门禁 | 读数 |
| --- | --- |
| `scripts/cargo-team.ps1 test -p ruagent-daemon` | **exit=0** —— `unittests src/lib.rs` **159 passed / 0 failed / 0 ignored**（比改前 **+2**：两条新测试）· capabilities 11 · capability_defaults 5 · event_compat 2 · graph_ingest 16 · injection_e2e **8 ignored** · knowledge_api 16 · recall_evidence_announced 3 · smoke 0 · version_points 5 · doc 0 ⇒ **合计 217 passed / 0 failed / 8 ignored** |
| `scripts/cargo-team.ps1 test -p ruagent-acp` | **exit=0** —— lib **16 passed / 0 failed / 0 ignored**（比改前 **+1**：C1 的测试）· doc 0 |
| `scripts/cargo-team.ps1 clippy -p ruagent-daemon -p ruagent-acp --all-targets -DenyWarnings` | **exit=0**，`^error` = **0**（且无 `warning`） |
| `cargo fmt --all --check` | **exit=0**，零 `Diff in` |

## 9. 未覆盖 / 边界（明写）

- **触发条件仍未证实**：`t180` 已承认、我在此重申 —— 「某个运行时会话**首答为空**、稍后**真有选项**」需要**真实 ACP 运行时**才能观测；本单**没有** harness ⇒ 用户可见后果是**条件性**的（若运行时确实没有可选项，旧行为本来就是正确的）。本单修的是**「把不知道渲染成没有」这条路径**，它在字节上无条件存在过。
- **`acp/src/run.rs:265`**：同一 `unwrap_or(&[])` 折叠写法，**只登记不改**（不在 inScope；其消费者未评估）。
- **`O2`**（`t175` 提出）**只登记**：删掉那处 sleep 后「没有 spawn」不可观测。
- **面板改动不在本单**：只做了后端信号（`known`）；面板如何据此收紧见 §4，建议另立单。
- **未跑真实模型/ACP 会话**；**未起活守护进程**（8787 未启停、未发任何请求、未写其库）；`crates/daemon/tests/**`、`crates/store/**`、`panel/**`、`.github/**`、`scripts/**` **零改动**；收尾未用 glob（备份文件在两处 `%TEMP%` 具体路径，未删任何项目文件）。

## 10. 建议的提交信息

```
fix(daemon,acp): never told is not "told: none" for a runtime's option catalog

The ACP session's first answer folded `config_options == None` (the server said
nothing) into `Some(vec![])` (it said: none). The daemon's option tracker then
cached AND persisted that emptiness as knowledge, so a picker could assert "this
runtime has no models" for a runtime the daemon was never told about -- and the
empty catalogue survived a restart through load_option_cache. t192 fixed the
probe path; the tracker path was the sibling left behind.

- acp: `advertised_options(None) == None` keeps the channel at "has not
  reported" instead of publishing an empty advertisement.
- daemon: the tracker tracks only a REPORTED state (`catalog_to_track` returns
  None for "not told"); the probe rule (t192) is unchanged -- an empty or
  unreported answer still never overwrites or persists.
- daemon: boot seeding skips an empty stored catalogue (it is not first-hand
  knowledge in this process).
- daemon: `GET /api/v1/agents/{name}/options` reports `known`, so the panel's
  existing "a failed probe must not render as 0 models" guard (t84) can also
  cover "never told" -- panel change filed separately.

Not proven: the trigger (a runtime whose first answer is empty but which has
options later) needs a real ACP runtime; this removes the path, it does not
demonstrate the outage. MODELS_WAIT and the probe policy are unchanged.
```
