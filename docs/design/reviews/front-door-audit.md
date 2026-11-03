# 主智能体这一批改动的独立复核（898e8ad..e58a416）

复核人：rust-distill（这批代码不是我写的，也没参与它的评审）
性质：只读复核。**除本文件外没有改动任何产品代码**（证明见 §5）。

## 0　我验了什么，怎么验的

### 0.1 范围：读过的 diff

    git log --oneline 898e8ad^..e58a416        # 8 个提交
    git diff --stat 898e8ad^..e58a416          # 19 files, +3237 / -127

逐行读（不是只 grep）的部分：

| 文件 | 读的范围 | 为什么 |
| --- | --- | --- |
| crates/daemon/src/config.rs | 全部 +283 行（parse_agents / enforce_single_primary / PrimaryEditor / RoutingEditor / leading_prefix / put_leading_prefix / 种子与测试） | 两个写入口都在这儿 |
| crates/daemon/src/api.rs | front_door_json、effective_front_door、get_primary、put_primary、get/put_routing、update_agent_card、start_run 的路由分支、primary_decision、routing_decision_with、card_json | 「一个答案」的读端与写端 |
| crates/mcp/src/lib.rs | 新增 7 个工具（task_create / task_dispatch / run_get / agents_list / agent_set_primary / routing_show / routing_set_default）与 agents_to_text / front_door_to_text / routing_to_text / routing_written_to_text / dispatched_to_text | 工具会不会自己再写一遍两份 |
| panel/src/api.ts | PrimaryInfo、primaryShape、api.primary / api.setPrimary | 「读失败不能变成没有」的第一道闸 |
| panel/src/views/Chat.tsx | 落地规则、frontDoorNote、agentOption、switchAgent | 新会话落在谁身上 |
| panel/src/views/Agents.tsx | frontDoor/fv* 状态、applyPrimary、primaryBadge、band、行徽章、popconfirm | 写操作与「不知道」的呈现 |
| panel/src/i18n/chat.ts、panel/src/i18n/agents.ts | 新增键，zh/en 对照 | 漏项 |
| docs/design/primary-agent.md、docs/design/primary-agent-prompt.md | 全文（含 One answer to "who will answer me"、Failure modes、Measurements） | 这是契约，报告端要按它验 |

**行号约定**：crates/daemon/src/config.rs、crates/daemon/src/api.rs、panel/src/** 与 docs/design/primary-agent.md 在工作树里与 e58a416 相同（复核时这四类文件 git status 干净），所以行号两者一致；**crates/mcp/src/lib.rs 不同**——复核期间有别的成员在这条工作流上又加了未提交的 +309 行，把工具函数整体下移。本报告对 mcp/lib.rs 一律引用 **e58a416** 的行号（并在 F6 的括号里给出工作树当前行号），因为被复核的是那个提交，而不是别人正在写的字节。

参考阅读（不在 diff 里但决定判断）：crates/daemon/src/registry.rs 的 apply_agent / update_agent（它写不写 primary 标记）、crates/daemon/src/runs.rs 的 agent()、crates/orchestrator 的 route()。

### 0.2 方法

1. **三处交叉比对**：契约（文档承诺）↔ 报告端（GET /api/v1/primary 的 name/source/why）↔ 真正执行的路（start_run → routing_decision_with → orchestrator::route → primary_decision）。只有三者说得不一样的地方才报。
2. **每条发现都在源码里定位到行**，并抄原文；引用的是 898e8ad..e58a416 之后的文件内容（行号以当前工作树为准）。
3. **对每一类点名的失效逐条找**：并发写文件、半写状态、错误被吞、拒绝后仍改状态、t41/t43 不变量、面板把「读失败」显示成「没有」、i18n 漏项、routing.toml 写后丢注释/规则。
4. 只在能给出**触发场景**（什么输入/什么时序）时才写进 §2；给不出触发场景的一律放进 §3「验过、干净」。

### 0.3 限制（先声明，免得高估这份报告）

- **没有跑任何测试、也没有跑 daemon**。本任务要求绝对只读（产品代码一行都不能改），而把下面的发现变成可执行复现要么得往仓库里加测试、要么得另起一个私有 harness；两者都越界。所以 §2 的每一条都是**静态代码路径论证**（文件:行 + 原文 + 触发输入），不是运行时读数。凡能用「代码里已存在的测试」旁证的地方都注明了测试名。
- 只覆盖任务点名的那一小组文件。mcp 的工具描述文本、panel 的样式/CSS、Cargo.lock/版本号、README、CI 脚本没有逐字复核（CI 的两个脚本只扫了一眼提交信息）。

## 1　结论速览

| 编号 | 严重性 | 一句话 | 位置 |
| --- | --- | --- | --- |
| F1 | 中 | PUT /api/v1/primary 写两个文件，第二次失败时第一次已经落盘；注释却声称「半写比拒绝更糟」，而它只覆盖了校验失败那条路 | api.rs:2608-2618 |
| F2 | 中 | 两个 editor 的互斥锁是「每个实例」的，而每个请求都 new 一个；两个 editor 还共用同一个固定的 .toml.tmp 路径 | config.rs:1295/1321/1377/1417 + api.rs:2608/2613/2650 |
| F5 | 中 | routing.toml 的 default 指向被禁用的 agent 时，GET /api/v1/primary 说「下一次运行会回落到第一张启用卡」，而运行路径真的会路由到那个被禁用的 agent | api.rs:2501-2504 vs api.rs:7619-7635 / 3076-3090 |
| F3 | 低 | 把 routing 清空（default=null 且没有 rules）时，文件头注释块被丢弃——t52 修好的那个缺陷在「清空」这条路上还在 | config.rs:1351-1354 |
| F4 | 低 | 「标记了 primary 但被禁用」+「routing 有启用的 default」时，why 说「没有 agent 被标记 primary」 | api.rs:2505 / 2513-2521 |
| F6 | 低 | 契约文档写「Four tools, deliberately few」并只列四个，代码发了七个 | primary-agent.md:107 vs mcp/lib.rs:442/467/505/559/571/596/608（e58a416） |
| F7 | 低 | 选中的 agent 从注册表消失后，Chat 不再自动落位（改前会落到第一张启用卡） | Chat.tsx:503 / 570-571 |
| F8 | 低 | api.rs 测试里的一段注释把 t52 已经修好的缺陷仍写成「已归档、未修」 | api.rs:12645-12650 |

没有高危（数据损坏级、静默改错人、拒绝后仍改状态）的发现。下面逐条给证据。

## 2　逐条发现

### F1（中）PUT /api/v1/primary 的半写窗口：第二次写失败时，第一次已经改了盘

**位置**：crates/daemon/src/api.rs:2608-2618（put_primary）

**原文**：

    crate::config::PrimaryEditor::new(config_dir.join("agents.toml"))
        .set(target.as_deref())
        .map_err(|e| ApiError::internal(format!("{e:#}")))?;
    let mut routing = crate::config::load_routing_file(&config_dir, &state.config.routing);
    routing.default = target.clone();
    crate::config::RoutingEditor::new(config_dir.join("routing.toml"))
        .write(&routing)
        .map_err(|e| ApiError::internal(format!("{e:#}")))?;

**触发场景 A（routing.toml 解析不了）**：

1. 用户手改了 routing.toml，留下一个 TOML 语法错误（这只影响 level 3，daemon 照常跑：config.rs:1250-1272 的 load_routing_file 在解析失败时只 WARN 并回退到启动快照）。
2. 用户在面板 Agents 页点「设为主智能体」（或主智能体调 agent_set_primary）→ PUT /api/v1/primary {"name":"dsh"}。
3. 第一行成功：agents.toml 里 dsh 带上 primary = true，其它条目被清掉。
4. load_routing_file 只 WARN，返回**启动时的**快照（routing.default 仍是 claude）。
5. RoutingEditor::write 重新读文件并 text.parse()（config.rs:1379-1382），对坏文件失败 → 外层返回 500。
6. 结果：应答是 500，盘上却是「flag = dsh，routing default = claude」——正是这个接口存在的理由所要消灭的那种状态（文档 §One answer to "who will answer me"：「Anything that sets the front door writes BOTH places, so they cannot drift by accident」）。下一次 GET /api/v1/primary 会如实报告两者不一致，于是用户看到的是「我刚设的，怎么还是别人」。

**触发场景 B（routing.toml 写不了）**：routing.toml 只读 / 被杀毒软件短暂锁住 / 目录权限缺失 → 第 3 步成功，第 5 步在 read_to_string 或 write/rename 处失败 → 同样的半写。

**为什么注释没覆盖它**：put_primary 的文档注释写「A name that is not in the registry, or is disabled, is refused BEFORE anything is written: a half-written swap would be worse than the refusal.」——这句对**名字校验**是成立的（我核过：2608 之前就把 unknown/disabled 拒了，且 api.rs:12664 的 an_unknown_name_is_refused_without_a_partial_write 断言两个文件都没变），但**文件级失败**不在它的覆盖里。

**建议修法**（任选，都保持现有语义）：

- 先做 routing 的写（或先做一次「能读能解析」预检），成功后再写 flag；失败时整体中止，flag 不动。
- 或者：两次写各自写 tmp、都成功后再各自 rename（commit 阶段），失败发生在 commit 之前。
- 或者最小改法：RoutingEditor 失败时回滚已经写下的 flag（再调一次 PrimaryEditor::set(旧值)），并把回滚写进错误文本。

### F2（中，需要并发才显形）两个 editor 的互斥是按实例的，而每个请求新建一个；两者又共用固定的 .toml.tmp

**位置**：

- config.rs:1293-1295（PrimaryEditor::set 拿 per-instance 锁）
- config.rs:1321-1326（写 self.path.with_extension("toml.tmp") 再 rename）
- config.rs:1376-1377 与 1417-1420（RoutingEditor::write 同形）
- api.rs:2608 / 2613 / 2650（每个请求都 PrimaryEditor::new / RoutingEditor::new）

**原文**（config.rs:1321-1326）：

    let tmp = self.path.with_extension("toml.tmp");
    std::fs::write(&tmp, doc.to_string())
        .with_context(|| format!("writing {}", tmp.display()))?;
    std::fs::rename(&tmp, &self.path)
        .with_context(|| format!("renaming into {}", self.path.display()))?;

以及 api.rs:2650：

    crate::config::RoutingEditor::new(state.config.root.join("config").join("routing.toml"))
        .write(&req)

**触发场景**：同一瞬间两个写请求（面板两个标签页各点一次保存；或面板保存 + 主智能体调 routing_set_default；或 PUT /api/v1/primary 与 PUT /api/v1/routing 并发）。两边是不同 worker 线程上的 tokio 任务，std::fs::write / rename 是阻塞调用，没有任何东西把它们串起来——每实例的 Mutex 保护不到对方。于是：

1. A、B 各自 read → parse（各自一份 doc）；
2. 两者都 fs::write 到**同一个** routing.toml.tmp（截断+写），内容可能互相覆盖或混写；
3. A rename 成功；B rename 拿到的 tmp 已经换成 A 的内容，于是 B 的内容整体丢失；或者 B 的 rename 因路径已被移走而失败 → 一方拿到 500，盘上是另一方的版本。

最坏情况：混写出一个不是合法 TOML 的 routing.toml → 之后每次 load_routing_file 都静默回退到启动快照，PUT 也会一直失败（于是 F1 的半写随之出现）。

**我的判断**：锁字段的存在说明作者知道要串行化（distill / capabilities editor 是同样形状），而「每请求 new 一个」把它的意图抹掉了。不是理论问题，只是需要并发才显形；一次撕裂的后果（配置静默失效）远大于触发概率，所以定「中」而不是「低」。

**建议修法**：两者选一（都不改行为）：

- editor 变成进程内单例（AppState 里放一个 Arc 或按路径 OnceLock 缓存）；
- 或每次写用唯一 tmp 名（<file>.<pid>.<counter>.tmp）——仅此一条就能消除「互相写同一个 tmp」的撕裂，剩下的只是 last-writer-wins。

### F5（中）报告的前门与真正的 router，在「routing default 指向被禁用的 agent」时不一致

**位置**：

- 报告端：api.rs:2501-2505
- 执行端：api.rs:7619-7635（routing_decision_with）与 api.rs:3076-3090（start_run 的路由分支）

**原文**（报告端，2501-2505）：

    let configured = routing
        .default
        .as_deref()
        .and_then(|name| cards.iter().find(|c| c.name == name && c.enabled));
    let marked = cards.iter().find(|c| c.primary && c.enabled);

**原文**（执行端，7619-7635）：

    let by_name: std::collections::HashMap<&str, ruagent_core::AgentId> =
        cards.iter().map(|c| (c.name.as_str(), c.id)).collect();
    ...
            default: routing.default.as_deref().and_then(resolve),

执行端**不看 enabled**；start_run（3076-3090）拿到 decision 后也只按 id 找 card、不检查 enabled，然后就用它起 run。

**触发场景**（完全可以在 UI 上做出来）：

1. Agents 页把 opencode 关掉（enabled = false）；routing.toml 的 default 仍是 "opencode"（关 agent 不会改 routing）。
2. GET /api/v1/primary：configured = None（因为那个 && c.enabled），若没有别的 flag，返回 {"name": null, "why": "...routing.toml's default names opencode, which is not an enabled registered agent, and no agent is marked primary: the next run falls through to the first enabled card..."}（api.rs:2550-2556）。
3. 面板照着这个渲染：没有可用的前门，并说下一次运行会落到第一张启用卡。
4. 但 POST /api/v1/tasks/{id}/runs（不带 agent）会走 routing_decision_with → route() → decision 的 agent 就是 opencode → start_run 用它起 run。**下一次运行不会回落到第一张启用卡，而是去了那个被关掉的 agent。**

**为什么这是这一批的问题（而不是既有行为）**：level 3 不看 enabled 是既有行为；但「下一次运行会回落到第一张启用卡」这句话是这一批新写的 GET /api/v1/primary 说的，而它是文档里 One answer to "who will answer me" 的唯一答案来源——现在这个答案与真正跑的路不一致，正好是文档点名要避免的形状。

**可达性旁证**：put_routing 的校验只查「名字在注册表里」，不查 enabled（api.rs:2634-2647：先 let known: Vec<String> = state.mgr.agents().iter().map(|c| c.name.clone()).collect(); 再 check(&rule.agent, "a routing rule")?），所以「default 指向一个已禁用的 agent」不是只有手改文件才能得到：在 Agents 页禁用 agent 后，routing.toml 仍指着他，就是这个状态。

**建议修法**：二者取一，并让它成为唯一的说法：

- 执行端跳过被禁用的 default（与报告端一致），并在 rationale 里说明「routing.toml 的 default opencode 被禁用，回落到 …」；
- 或报告端把「被禁用的 default」当作**当前答案**并警告（name = opencode，why = 「…但它是 DISABLED，下一次运行仍会把它当作 level 3 的选择」）。

配一条测试：把 default 指向 enabled=false 的卡，断言 GET /api/v1/primary 的 name 与「不带 agent 起 run 后 params.agent 解析到的 card」一致。

### F3（低）清空 routing 时，文件头注释块被丢弃

**位置**：config.rs:1351-1354（put_leading_prefix 的早退）与 config.rs:1414-1420（调用点）

**原文**：

    fn put_leading_prefix(doc: &mut toml_edit::DocumentMut, prefix: toml_edit::RawString) {
        let Some(first) = doc.iter().next().map(|(k, _)| k.to_string()) else {
            return;
        };

**触发场景**：routing.toml 只有 default 一条（出厂种子就是这样：规则是注释掉的）。主智能体调 routing_set_default name=none（工具描述里明确支持这个用法），或客户端 PUT /api/v1/routing {"default": null, "routes": []}：

1. write 里 doc.remove("routes")；default 为 None 走 doc.remove("default")（config.rs:1391-1400）；
2. 文档此时**没有任何 item**；
3. put_leading_prefix 第一行就 return → 举起的前缀（整块「# ruagent routing rules …」）被丢掉；
4. 盘上文件变成空文件。

t52 修好的正是这件事（config.rs:1387-1392 的注释写着 Measured before this fix: after one write the file began at default = "beta" and every "# ruagent routing rules ..." line was gone）：t52 之后**普通写**保住了，**清空写**没有。

**建议修法**：doc 为空时把 prefix 写进文档自身的 decor（或写成一个纯注释头），而不是 return；配一条测试：先写 default，再写 None 且 routes 为空，断言文件里仍有那条文件头。

### F4（低）why 在「标记了 primary 但被禁用」时说「没有 agent 被标记 primary」

**位置**：api.rs:2505（marked 只收 enabled 的）与 api.rs:2513-2521（configured 命中、marked 为 None 的那支）

**原文**：

    Some(primary) if primary.name == default.name => format!(...),
    Some(primary) => format!("routing.toml default `{}` OVERRIDES ..."),
    None => format!(
        "routing.toml's `default` names `{}` and no agent is marked primary",
        default.name
    ),

**触发场景**：agents.toml 里 builder 带 primary = true 且 enabled = false；routing.toml 的 default = "claude"（启用）。GET /api/v1/primary 返回 name = claude，why = 「routing.toml's default names claude and no agent is marked primary」——而 GET /api/v1/agents 里 builder 明明有 primary: true。面板此时会渲染 chat.primary.conflict（「primary 标记是 builder，但实际回答你的是 claude」），与 daemon 自己的 why 直接矛盾：同一页上两句话互相否认。

**建议修法**：这一支里用 cards.iter().find(|c| c.primary) 区分「一个都没标」与「标了但被禁用」——后者用 2550-2556 已有的措辞（marks X primary but that agent is DISABLED）。

### F6（低）契约文档说四个工具，代码发七个

**位置**：docs/design/primary-agent.md:107（The primary can only operate the platform if it has hands. Four tools, deliberately few:）与其下的四行表；代码（**e58a416 行号**）：crates/mcp/src/lib.rs:442（task_create）、467（task_dispatch）、505（run_get）、559（agents_list）、571（agent_set_primary）、596（routing_show）、608（routing_set_default）。工作树里同七个函数现在是 621 / 646 / 684 / 738 / 750 / 775 / 787（因为别人又在这文件里加了 +309 行未提交内容），但「文档四、代码七」这个事实在 e58a416 上就成立——它就是这一批提交进仓库的样子。

**触发场景**：读契约的人（下一个实现者、写面板的人、写测试的人）按文档只准备四个工具，就会漏掉 task_create / task_dispatch / run_get；反过来「deliberately few」这句现在与代码不符。文档开头就写着 This document is the contract the backend, the MCP tools and the panel are built against.，所以这条不是风格问题。

**建议修法**：把表补成七个（分两类：读写前门、任务与运行），或明确写「v1 的四个前门工具 + 三个任务工具」。

### F7（低，面板回归）选中的 agent 从注册表消失后，不再自动落位

**位置**：panel/src/views/Chat.tsx:503（loadAgents 把失效选择清成空串）与 570-571（落地规则的早退）

**原文**：

    setAgent((cur) => (cur && enabled.some((a) => a.name === cur) ? cur : ""));

    const mayRedecide = !!prev?.fallback && messages.length === 0 && agent === prev.name;
    if (prev && !mayRedecide) return;

**触发场景**：用户在 Chat 里已经有过一次自动落位（autoPickRef 非空、fallback=false，即前门命中的那次），然后到 Agents 页把那个 agent 删除（或禁用，它就从 enabled 列表里消失）。回到 Chat（没有打开的会话、也没手动选过）：

1. loadAgents 把 agent 清成空串（503 行）；
2. 落地规则：prev 存在，mayRedecide 要求 agent === prev.name（现在 agent 是空串）→ false → 直接 return；
3. 结果是选择器空着，也没有任何 note 解释（note 只覆盖 ?agent= 不可用、前门不可用、读失败这几种）。

改动前 503 行同一处会回落到 enabled[0]，所以这是这一批带进来的行为变化。

**建议修法**：把「agent 为空」也当成需要重新落位的理由（if (prev && !mayRedecide && agent) return;），或在 loadAgents 丢弃选择时清掉 autoPickRef。

### F8（低）api.rs 测试里的一段注释把已修好的缺陷写成「未修」

**位置**：crates/daemon/src/api.rs:12645-12650

**原文**：

    // NOT asserted here: whether the HEADER comments survive this write. Measured in t51:
    // RoutingEditor (config.rs, outside this task's scope) replaces the "default" item together
    // with its decor, so "# ruagent routing rules" is lost on the first write. Filed as a
    // finding instead of being papered over -- what this test owns is that BOTH carriers changed.

**触发场景**：t52（同一批的 125906e）已经修好：RoutingEditor::write 现在举起并放回 head 前缀（config.rs:1389 / 1414），并有 a_default_only_file_keeps_its_header_comments 守着。可这段注释仍然告诉读者「这一写会吃掉文件头」——下一个读到这里的人要么以为缺陷还在（于是再修一次），要么以为这个测试故意不验（于是不加断言）。同一批里 t52 也把这个测试本可以断言的事实变成了可断言的。

**建议修法**：删掉这段注释，并在这个测试里加一行断言（read(&root, "routing.toml") 含文件头那一行）——它现在会通过，因为 t52 的修复。

## 3　明确干净的区域（验过，没发现）

这些是任务点名要看的失效类别里，我逐条找过而**没找到**的。写在这里是为了让下一位复核者不必重做：

1. **t43「一个 primary」不变量**（干净）。config.rs:195-199 的 parse_agents 是唯一入口（先 parse_agents_unchecked 再 enforce_single_primary）；两个 primary 是加载错误且报出两个名字（208-221），被禁用的 primary 只 WARN（222-228）。关键的一点：registry editor 的 AgentPatch **不携带** primary（registry.rs 的 apply_agent 只设 prompt/description/model/mcp_profile/runtimes/runtime/options/enabled），所以「PATCH 里点亮 flag」不会让文件短暂出现两个标记；PUT /api/v1/agents/{n} 的 primary 分支（api.rs:2453-2481）把实际的原子交换交给 PrimaryEditor::set（config.rs:1302-1320：clear the flag from EVERY other entry IN THE SAME WRITE）。测试：the_agent_route_swaps_the_flag_atomically。
2. **t41「default 级必须有 rationale，且必须记 registry id」**（干净）。primary_decision（api.rs:2969-2992）用 card.id（注册表卡自己的 id，不是配置文件视图每次新生成的 id——t38 那个坑），source = Default，rationale 点名 agent 并说明来源是 agents.toml 的标记；如果这张卡正好是 policy.toml 指定的审批人，rationale 会额外说明。with_default_rationale（3018-3028）保证 cascade 给出 Default 时一定有 rationale；with_fallback_notes（3004-3013）把「标记了但被禁用」的说明追加到回落路径。测试：the_primary_agent_is_level_four_and_the_rationale_names_it / a_primary_that_is_the_approver_says_so / a_disabled_primary_falls_through_and_says_why / a_routing_toml_default_still_beats_the_primary。
3. **「拒绝后不改状态」——名字校验那条路**（干净）。put_primary 在写之前就拒 unknown/disabled（api.rs:2588-2606），put_routing 在写之前校验 default 与每条 rule 的名字（2634-2647）；测试 an_unknown_name_is_refused_without_a_partial_write 逐字断言两个文件未变（assert_eq!(read(&root, "agents.toml"), before_agents) 等）。注意：干净的是**校验**这条路；文件级失败是 F1。
4. **面板「读失败不能显示成没有」**（干净）。api.ts:1390-1408 的 primaryShape 明确允许 name === null（那本身就是「没有前门」这个答案）但拒绝畸形 200；api.primary() 失败是 reject（api.ts:1493），不是空答案。Agents.tsx 用 LoadState + fvUnknown（175）/ fvStale（178）：从没读到过 → band 里是 ErrorState 且**任何行都不带徽章**（不假装知道）；读到过再失败 → StaleNotice + 保留上次答案。Chat.tsx 的 frontDoorNote 第一条就是 frontDoor.kind === "error"（1671）→ ErrorState + 重试，并且明说「选择器里显示的是回落项」。三处都符合「读失败 ≠ 没有」。
5. **i18n**（干净）。i18n/chat.ts 的 zh 与 en 各新增 10 个 chat.primary.* 键；i18n/agents.ts 两边各 73 个键（实测两边数字相同）。（面板构建门禁本身会跑 node e2e/i18n-check.mjs，见 AGENTS.md。）
6. **MCP 工具没有把「写两处」再复制一遍**（干净）。agent_set_primary 只 PUT /api/v1/primary（mcp/lib.rs:571-591，e58a416；注释 577-581 也写清了为什么不在这里做两次写）；routing_set_default 先 GET 再带着原有 rules 整份 PUT（同文件 608 起，e58a416：614-616 的注释与 617/624-627 的读回），这样「只改 default」不会把规则清掉，工具描述也明说这一点。测试：routing_set_default_round_trips_and_keeps_the_rules / an_unknown_agent_is_refused_with_the_daemons_own_reason_and_changes_nothing。
7. **task_dispatch 不自己编一个默认 agent**（干净）。mcp/lib.rs:467 起（e58a416）的 body 只插入调用方真的送了的字段（读过全函数：没有 unwrap_or 之类的兜底）；测试 task_dispatch_names_the_agent_and_leaves_an_omitted_one_out_of_the_request 断言线上没有 agent 字段。
8. **GET /api/v1/agents 带上 primary**（干净）：card_json（api.rs:2702-2703）把标记放进每一行，面板不需要第二次查找就能画徽章。
9. **错误没有被静默吞掉（这一批新增的路径）**（干净）：reload_registry 的失败会 500（api.rs:2276-2288）；load_routing_file 的回退有 WARN（config.rs:1256-1266）；MCP 侧在载荷缺字段时用 drift()/field_* 报错而不是填默认值（agents_to_text）；front_door_to_text 在 daemon 没给 source 时明确说「NO source level -- read the payload before telling the user」。

## 4　我验了、但这次不单独报的

- **routing_set_default 的读-改-写竞态**（先 GET 再整份 PUT）：两次写之间别人加的规则会被覆盖。判断：单用户本地 daemon，窗口是毫秒级，而且接口形状就是全量替换，没有更好的形状可用。与 F2 同源，修 F2 的串行化时一并考虑即可。
- **「被禁用的 routing default」到底该谁赢**（F5）：这是设计决定，不是复核能替队长定的；我只报「两处说法不一致」这个事实，并给出两种修法。
- **PUT /api/v1/agents/{n} 的 primary 与 PUT /api/v1/primary 并发**：两条写路径都落到 PrimaryEditor，F2 覆盖；「一次写清其它条目」本身是对的。

## 5　状态证明（只读复核）

复核开始时的 git status --porcelain（5 条，全部是别的成员的文档/CI 改动，与本任务无关）：

    M .github/workflows/e2e.yml
    M docs/design/reviews/gen2-integration-impl.md
    M docs/design/reviews/gen3-backlog.md
    M docs/design/reviews/gen4-audit-row12-inline-style-repair.md
    M docs/design/reviews/gen4-options-known-panel-verify.md

复核结束时的 git status --porcelain：

    M .github/workflows/e2e.yml
    M cli/src/main.rs
    M crates/mcp/src/lib.rs
    M crates/mcp/tests/roundtrip.rs
    M crates/mcp/tests/tool_surface.rs
    M docs/design/reviews/gen2-integration-impl.md
    M docs/design/reviews/gen3-backlog.md
    M docs/design/reviews/gen4-audit-row12-inline-style-repair.md
    M docs/design/reviews/gen4-options-known-panel-verify.md
    M panel/src/i18n/home.ts
    M panel/src/views/Home.tsx
    ?? crates/mcp/tests/agents_write.rs
    ?? docs/design/reviews/front-door-audit.md

**怎么读这两份读数（重要）**：结束时比开始时多出 8 条，其中**只有最后一条是我**（?? docs/design/reviews/front-door-audit.md）。其余 7 条是别的成员在我复核这段时间里落的未提交改动（cli/src/main.rs、crates/mcp/src/lib.rs 及其两个测试、panel 的 Home 两处、以及新增的 crates/mcp/tests/agents_write.rs）——它们的时间戳在我开始复核之后，且与这批前门改动无关。

我能给出的、关于「我只动了报告」的**可核对证据**是：本会话里我所有的写操作只指向两个位置——%TEMP%\t56\ 下的临时文件，和 docs/design/reviews/front-door-audit.md（报告由四段临时文件拼接而成，拼接脚本也在 %TEMP%\t56\）。我没有对 crates/**、panel/**、cli/** 执行过任何写。这也解释了为什么从「复核开始」到「复核结束」之间 mcp/lib.rs 的行号会漂：那是别人在写，不是我。

（顺带一条与复核无关的提醒：crates/mcp/src/lib.rs 现在有 +309 行未提交内容，任何引用该文件行号的人都应该说明自己引的是哪个版本——本报告选的是 e58a416。）

---

## 6　处置结果（队长补记，2026-11-03）

八条全部处置完毕。表里的「修在」给的是提交，不是任务号——任务号按代际重发，提交不会。

| 编号 | 处置 | 修在 |
| --- | --- | --- |
| F1（PUT /primary 半写） | 已修：第二次写失败时第一次不动 | `3247465` |
| F2（每实例锁 + 固定 tmp） | 已修：进程级锁 + `<file>.<pid>.<counter>.tmp` | `3247465` |
| F3（清空写吃掉文件头） | 已修：空前缀交回上层，写成纯注释文件 | `3247465` |
| F4（why 自相矛盾） | 已修：区分「一个都没标」与「标了但被禁用」 | `3247465` |
| F5（报告端与执行端不一致） | 已修：两边同一条规则 | `3247465` |
| F6（文档四、代码七） | 已修：改成七个，并分成前门组与派活组 | `73cf11b` |
| F7（选中的 agent 消失后不落位） | 已修：`agent` 变空即清 `autoPickRef`，规则重新决定 | `6ef11d9` |
| F8（陈旧注释） | 已修：注释改写成两处修复之后的真实状态 | `3247465` |

**本报告之外还多出一条，而且它比 F1-F8 里的任何一条都更接近「写下不可解析的配置」**：修 F6/`PUT` 语义时
的新测试发现，清空 `default` 会让 `[[routes]]` 成为文件首个 key，而 t52 的 `put_leading_prefix` 把文件头写进
这个 key 的 decor 前缀——那里正是 toml_edit 存 `[[` 的地方，于是文件末成 `routes]]`、`load_routing_file` 报
解析错误。发现它的任务（`t58`）无权改 `config.rs`，所以它**没有偷偷改**：把回归测试留在 `#[ignore = "blocked: …"]`
并写明原因，修复交给持有该文件的任务（`t59`）。现在那条测试已摘掉 ignore 并通过，daemon lib 的读数是
**259 passed / 0 failed / 1 ignored**（剩下那条是既有的）。

一条方法论上的提醒，给下一个做同类复核的人：F2 与 F3 都是**静态论证出来的**（复核人没有跑 daemon，因为
只读约束不许它改产品代码加测试）。这种论证能指出真问题，但它给出的严重性只是估计——F2 的实际触发需要并发，
F3 只需要一次清空写。两者都值得修，但「多久会碰上」是修的人用测试回答的，不是复核的人。
