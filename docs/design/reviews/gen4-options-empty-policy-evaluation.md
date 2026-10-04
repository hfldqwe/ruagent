# O1 评估：同一个观测（报告了空选项）在跟踪器与 `record_probe` 上策略相反（只读）

**任务**：`t180`（kind = work，**report-only**）· **日期**：2026-10-04 · **写入**：仅本文件（`crates/**`、`panel/**`、`.github/**`、`scripts/**`、`tools/**` **零改动**）
**来源**：`t175` 独立评审（verdict = pass）的建议项 **O1**。**本单只评估这一处策略矛盾**：不重盘其它等待/缓存形状；O2 只登记（§7）。

## 结论（先说，二选一已选定）

> **②「live 会话第一手的『我没有选项』**不**应当权威」** ⇒ `chat.rs:945-957` 的跟踪器**就是 t192 那条注释所描述故障形状（空 picker + 跨重启固化）的实际入口**（t192 只堵住了探针那个入口）。

**三条证据（彼此独立）**：
1. **发布端发出的那个值根本不是「第一手陈述」**：`acp/src/chat.rs:444-445` 用 `session.config_options.as_deref().unwrap_or(&[])`，把「ACP 服务端**什么都没说**」（`None`）与「服务端说了：没有可选项」（`Some(&[])`）**压成同一个值**再 `send_replace(Some(...))`。⇒ 跟踪器拿到的 `Some(vec![])` **无法区分「被告知没有」与「未被告诉」** ⇒ 它没有资格当「首答」。
2. **同一个 watch 会被再次发布**（`acp/src/chat.rs:470`、`:612`，都在 `SetSessionConfigOption` 的应答之后 `send_replace(Some(...))`）⇒ 会话生命周期里的**首次空值是一个瞬态投影**，不是该运行时的身份。
3. **产品自己在两处已经裁定过这个问题**：`t192`（探针侧：注释 + 测试 `an_empty_probe_neither_overwrites_nor_persists_the_catalog`）与**面板**侧的 `t84 / audit P3`（`panel/src/views/Runtimes.tsx:24-28`：「a failed probe must not render as "0 models" … **indistinguishable from a runtime that really has none**」）。⇒ 「**空/未知 ≠ 证据**」是**产品级意图**，而跟踪器恰恰是唯一能把这条意图**持久化破坏**的路径。

⇒ 因此**不采纳「场景不同所以两处可以不同」**：唯一的区别论证是「live 会话的首答比一次性探针更权威」，但它**被证据 1 击穿** —— 那个首答并不是一次陈述，而是一次**缺数据被折叠成的数据**。

---

## 1. 两条策略的原文与坐标（纯字节）

### ① 跟踪器：对「报告了空」**cache 并且 persist**（无任何判据）

`crates/daemon/src/chat.rs:937-963`（会话启动时 spawn 的 per-runtime 目录刷新器）：
```rust
        // Option-state tracker: whenever this session's advertised options
        // or current selections change, refresh the per-runtime catalog
        // (memory + `agent_options` table) the panel pickers read.
        let mut watch = session.options_watch();
        …
        tokio::spawn(async move {
            loop {
                if let Some(state) = watch.borrow_and_update().clone()
                    && last.as_ref() != Some(&state)
                {
                    last = Some(state.clone());
                    let updated_at = Utc::now().timestamp_millis();
                    cache.lock().expect("model cache lock").insert(
                        runtime_key.clone(),
                        CachedOptions { options: state.clone(), updated_at },
                    );
                    persist_options(&db, &runtime_key, &state, updated_at);   // :957
                }
                if watch.changed().await.is_err() { return; }                // chat closed
            }
        });
```
判据只有「**变了**」（`last != state`）⇒ `state == vec![]` 与 `state == [model, effort, …]` **走同一条路**：写内存缓存 **并** `persist_options` 到 `agent_options` 表。

### ② `record_probe`：对**同一观测**保留上一次、不 insert、不 persist

`crates/daemon/src/chat.rs:1339-1383`（t192 建立、t172 改成显式匹配）：
```rust
        match state {
            OptionsRead::NotReportedInTime | OptionsRead::ReportedNone => {
                … // (t172) 只 debug + 读 model_cache
                let previous = self.model_cache.lock()…get(runtime).cloned();
                previous.unwrap_or(CachedOptions { options: Vec::new(), updated_at })   // 保留上一次
            }                                                                            // ← 既不 insert，也不 persist
            OptionsRead::Reported(options) => { …insert…; persist_options(&self.db, runtime, &options, updated_at); … }
        }
```
⇒ **同一观测（「报告了空」）在两处得到相反权威性**：探针侧「不算证据、不落库」，跟踪器侧「当事实、落库」。

### ③ 发布端**能**发出 `Some(vec![])` —— 能，两条独立机制

- **机制 A（缺数据被折叠）**：`acp/src/chat.rs:444` `extract_options(session.config_options.as_deref().unwrap_or(&[]))` ⇒ 服务端**没给** `config_options` 时喂进去的是空切片 ⇒ 结果为空 `Vec`；`:445` `send_replace(Some(advertised))` ⇒ 发出 **`Some(vec![])`**。
- **机制 B（被过滤掉）**：`extract_options`（`acp/src/chat.rs:53-88`）只保留 `SessionConfigKind::Select`；遇到 `choices.is_empty()` 或非 Select 形态就 `return None`（filter_map 丢弃）⇒ **即使服务端给了 config options，结果仍可能是空 `Vec`**。
⇒ **能**，且注意：**机制 A 与「服务端明确说没有」在当前类型里不可区分**（这正是 §0 证据 1）。
**同形登记（不在本单评估范围）**：`crates/acp/src/run.rs:265` 有同一写法 `extract_options(session.config_options.as_deref().unwrap_or(&[]))` —— 只登记，未评估它的消费者。

---

## 2. 用户可见后果（C48 口径，具体到面板哪一处）

**数据流（字节）**：跟踪器写 `model_cache` + `agent_options`（`:950-957`）→ 开机 `load_option_cache`（`chat.rs:1387-1416`）从该表**回填** `model_cache`（`cache.entry(runtime).or_insert(CachedOptions { options, updated_at })`）→ 路由 `GET /api/v1/agents/{name}/options`（`api.rs:39` → 处理器 `agent_options` `:4781`）**默认就服务这份持久化目录**（文档注释：「served from the persisted catalog (instant), unless `?refresh=1` forces a fresh probe」）→ 面板 `panel/src/api.ts:864 agentOptions(name, refresh?, runtime?)` → `panel/src/views/Runtimes.tsx`（模型数 chip）、`Agents.tsx` / `Chat.tsx`（选项目录）。

**可复现的条件序列（我给出的最具体形式）**：
1. 运行时 R 的 live 会话启动时，ACP 服务端**未提供** `config_options`（或只提供非 Select 形态）⇒ `:444-445` 发布 `Some(vec![])`；
2. 跟踪器 `:957` 把空目录**写进缓存并持久化**；
3. 面板（不点刷新）读到的就是**空列表** ⇒ 该运行时的模型/选项 picker 显示空白；
4. **跨重启固化**：若在下一次刷新前重启守护进程，`load_option_cache` 会把这条空目录**回填**（`:1414 or_insert`）⇒ 重启后**仍然是空**；
5. 恢复途径：面板的手动同步（`?refresh=1`）· 新的 probe（`probe_options` 的 `wait_options` 路径）· 每 6 小时的 `OPTIONS_REFRESH` 后台刷新。

**为什么面板那道已有的防线救不了它**（这是本评估最要紧的一环）：面板的 `t84 / audit P3` 守卫只覆盖「**探针失败**」这一情形（`api.ts` 的 `catch`/`probeFailed` 路径）。而走跟踪器这条路时，守护进程回报的是一次**成功的**读数（一个合法条目 + `updated_at`），**不是失败** ⇒ 面板的 `probeFailed` **不会触发**，于是它把「我们不知道」渲染成「它没有」——**正是 t84 注释里点名禁止的那件事**，只不过这一次的成因在更上游（持久化层把未知折成了已知）。

**诚实边界（C48）**：**触发条件我无法在没有真实 harness 的情况下证实** —— 「某个运行时会话首答为空、但稍后真有选项」需要真 ACP 运行时来观测。⇒ 我把它写成**条件性后果**：
- 若某个运行时**确实**没有任何会话可选项，则跟踪器的写**是正确的**（空 picker 与现实一致，**无用户可见危害**）；
- **策略矛盾本身不依赖那个条件** —— 「同一观测两条相反权威」在字节上无条件成立，且它把**唯一能把空目录持久化**的路径留成了未设防的一侧，而 t192 与面板 `t84` 各自只堵住了另外两个入口。

---

## 3. 建议与可机械化判据（**本单不改代码**，建议 captain 另立修复单）

**建议**：**立单（中优先，非紧急）**，方向是「**让『未被告诉』在整条链上活得下来**」，而不是「把空当空一律拒写」：

**C1（发布端，先决条件）**：`session.config_options == None` 时发布的值必须与「服务端明确列出了（空的）可选项」**可区分**（保留 `None`，或引入 `NotAdvertised` 变体）。
- 可机械化的红/绿：一个用例断言「`config_options: None` 的会话所发布的值 **不是** `Some(vec![])`」。**今天这条不成立**（`:444-445` 把它们压成同一个值）。

**C2（跟踪器）**：`chat.rs:945-957` 的 cache+persist 分支对「未被告诉」**不可达**。
- 可机械化：仿 `t192` 那条测试的形状，用一个驱动 `None`（未知）的 watch 状态走**跟踪器**路径，断言 **缓存未被改写 且 `agent_options` 无写**（今天空值会写两处）。

**C3（开机回填）**：`load_option_cache` 不得把一条**从未第一手报告过**的空目录当成权威目录回填。
- 可机械化：表里存在 `options = '[]'` 且 `updated_at` 早于任何真报告时，断言 `model_cache` 不因此变成空（或干脆由 C2 保证这种情况不会被写出来）。

**C4（消费面）**：`GET /api/v1/agents/{name}/options` 应能返回「**未知**」与「**没有**」两种不同读数，好让面板**已有的** `t84` 守卫有可依凭的信号（现在它拿不到差别，只能看到一次「成功的空」）。
- 可机械化：该路由对「未报告过」的情形返回一个可判定的标记（如 `known: false`），面板侧才有条件区分。

**为什么不建议「反向改」（把 `record_probe` 改成也接受空）**：那会**复活 t192 已经修掉的故障**（空 picker + 跨重启固化，且探针本来就是低置信仪器）。t192 的规则在探针侧是**保守且正确**的，问题只在跟踪器**没有同样的保守**。

---

## 4. 分类小结（C55/C48 口径，一句话）

- 这不是「等待形状」问题，而是**「同一观测、两条相反权威」**问题：探针侧（`record_probe`，t192/t172）= **「空/未知不是证据」**；跟踪器侧（`chat.rs:945-957`）= **「变了就写，含空」** ⇒ **两条不可能都对**，而证据指向「不当权威」的那一侧是**跟踪器**（§0 三条）。
- 按 **C48**：**能解释的只有「运行时确实没有选项」那一支**（那种情况下写空是对的）；**「首答为空、稍后有选项」这一支无法在无真实运行时下证实** ⇒ 我如实标注为**条件性**，并明确「矛盾本身无条件成立」这一点不依赖它。

## 5. 建议的判据一句话（给修复单的验收）

> **判据**：在同一条链上，「**未被告诉**」与「**被告知：没有**」**不得是同一个值**（发布端 C1），**不得有路径把前者写成持久化知识**（跟踪器 C2 / 开机 C3），且**消费面必须能区分这两者**（C4）—— 今天 C1 与 C4 均不成立，C2 正被跟踪器违反。

---

## 6. 边界：本单**没有**做什么

- **只评估这一处策略矛盾**（跟踪器 vs `record_probe`）。**不重盘**其它等待/缓存形状（t157/t168 已盘；本单未新增盘点）。
- **零代码改动**：`crates/**`、`panel/**`、`.github/**`、`scripts/**`、`tools/**` **未碰**；**未在共享树造任何变异**（本单不主张任何行为已改变，故无需窗口）。
- **未跑任何测试**：因此**不给出任何「绿」读数**。按 **C47**，此处**无前置红证** ⇒ 任何 0/N 绿都是**零信息**；本单的证据全部是**字节阅读**。
- **活守护进程（8787）未启停、未发任何请求、未写它的库**；未写 `~/.ruagent`；收尾无临时物（未用 glob 删任何东西）。

## 7. 只登记（不在本单开工）

- **O2**（来自 `t175`）：删掉那处 sleep 后，「**没有 spawn**」本身不可观测 ⇒ 若将来只在 `record_probe` 的空臂加一次 `persist_options`，当前无测试会红。**登记，不为此开工**（旧窗口是**假绿易发**仪器，去掉它未丢失可信读数）。
- **`acp/src/run.rs:265`**：与 `:444` 同一折叠写法（`unwrap_or(&[])`），**只登记**，未评估其消费者是否也会把「未知」写成「没有」。
