# gen3 审计（只读）— MCP 服务 + ACP 适配层

> 任务：t91。范围：`crates/mcp/**`、`crates/acp/**`（**只读**；我的唯一写入 = 本报告）。
> 审计者：verify（独立验证员）。
> 纪律：**没有起真守护进程**（pid 79984 全程未触碰：`alive StartTime=2026/9/27 5:35:37`）；为取"契约 vs 实现"的**运行时**读数，我用了**我自己的临时 root + 临时端口 8817**，并**自己起了一个 `ruagent mcp-serve` stdio 实例**（pid **77668**，`terminate()` 后已退出；daemon pid **41496**/phase A、**87576**/phase B，均 `alive-after-stop=False`）。临时 root `%TEMP%\ruagent-t91` 在报告完成时保留供复核，随后删除（见 §6）。
> 归因：并发波次里工作（工作区在动）。所有引用的行号都在本轮读过；外部依赖 `agent_client_protocol`（`AcpAgent`/`Client`/`LineDirection`）**不属本仓**，涉及它的判断都标注出来。

## 0 结论

1. **两个面都是「薄代理 + 图书馆」**：`crates/mcp/**` 的 14 个工具**全部**是到守护进程 HTTP 路由的一行调用（`lib.rs:291-296` 自己写着 *"ONE consumption surface, not a second implementation"*），我逐条核了 URL 与参数 ⇒ **「同一件事两份实现」在 MCP 侧不成立**；`crates/acp/**` **不拥有子进程**（起停/半包/退出码在外部 `agent_client_protocol` 库里）。⇒ 这两个面的真正风险不在"重复实现"，而在**契约措辞、错误信息量与可见性**。
2. **M1（high）`graph_entity` 把「不存在的实体」报告成成功结果**：`crates/mcp/src/lib.rs:272-275` 用 `resp["facts"].as_array().cloned().unwrap_or_default()` + `if facts.is_empty() { return Ok("entity #N has no current facts") }` ⇒ 我的 wire 读数：`{"content":[{"type":"text","text":"entity #999999999 has no current facts"}],"isError":false}`。**不存在的 id 与"存在但零事实"不可区分**，而且字段一旦改名（协议漂移）`unwrap_or_default()` 会让**每个**实体都说同一句话——没有任何测试能看见。
3. **M2（high）agent 的 stderr 默认不可见**（t71 A-2 的当前字节）：`crates/acp/src/run.rs:76-82` 把子进程 stderr 落在 `tracing::debug!(target:"ruagent::acp::stderr", …)`，而 `crates/daemon/src/lib.rs:66-71` 的默认过滤器是 **`ruagent=info`** ⇒ 我在**同一份 mock agent 启动**上做了受控对照：默认过滤器下 `mock-agent starting` 命中 **0**，把该目标调到 debug 后命中 **2**（唯一变量是 `RUST_LOG`）。注释声称这条日志"invaluable for diagnosing harness startup failures"——**默认配置下它永远不出现**。

## 1 方法与可复现仪器

| 仪器 | 命令 | 产出 |
| --- | --- | --- |
| 工具契约清点 | `Select-String`/`Get-Content` over `crates/mcp/src/lib.rs`（14 个 `#[tool]` 块 + 参数结构） | §2 M1/M3/M5/M10 的源码侧 |
| **stdio 实跑** | `powershell -File %TEMP%\t91-run.ps1` → `python %TEMP%\t91-mcp.py`（`ruagent mcp-serve`，`RUAGENT_URL=http://127.0.0.1:8817`，pid 77668） | `tools/list` = **14** + 14 条描述/参数 + 7 条 tools/call 的 wire 结果 |
| **stderr 可见性对照** | 同上脚本的 phase A/B：`RUST_LOG` 未设 vs `ruagent=info,ruagent::acp::stderr=debug`，各跑一次 mock agent 启动 | §2 M2 的 0 vs 2 命中 |
| ACP 生命周期 | `Select-String` over `crates/acp/src/*.rs`（timeout/exit/kill/child/stderr）+ 逐段读 | §2 M2/M4/M6/M7/M8/M9 |
| 全仓日志过滤器 | `crates/daemon/src/lib.rs:66-71` | M2 的另一半 |

## 2 findings（10 条：high 2 / medium 3 / low 5）

### M1（high · 空成功）`graph_entity`：不存在的实体 → `isError:false` 的"没有事实"

- **file:line**：`crates/mcp/src/lib.rs:272-275`（另见 `:261-271` 的 `.error_for_status().map_err(rpc_error)?`）。
- **复现**：起我自己的 stdio 实例（§1），然后
  ```json
  {"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"graph_entity","arguments":{"id":999999999}}}
  ```
- **证据读数**：
  ```
  graph_entity unknown id -> {"content":[{"type":"text","text":"entity #999999999 has no current facts"}],"isError":false}
  （对照）memory_get unknown id -> {"code":-32603,"message":"daemon call failed: HTTP status client error (404 Not Found) for url (…/memory/999999999)"}
  ```
- **为什么重要**：MCP 是 agent 的**机器接口**，"成功 + 一句话"与"失败"是它唯一的二值语义。把不存在的 id 报成成功，agent 会把"没有这个实体"当作"这个实体存在、当前无事实"继续推理；更糟的是 `as_array()` 取不到时 `unwrap_or_default()` 把**协议漂移**也变成同一句"没有事实"（全库实体都会这么回答），而 14 个工具里没有一条测试覆盖这个分支。
- **可证伪的修复判据**：未知 id ⇒ `isError:true`（或让守护进程返回 404 并原样透出）；`resp.get("facts").and_then(|v| v.as_array())` 为 `None` 时必须报错（"daemon response lacks `facts`"）而不是默认空；判据：新增两条断言——(a) 未知 id 的 `isError==true`，(b) 用 `{"facts":[]}` 与 `{"facts":null}`/`{}` 两个桩响应分别期待"空事实"与"协议错误"。
- **owner**：mcp owner。

### M2（high · 失败可见性/诊断）子进程 stderr 的 sink 在 `debug`，而守护进程默认过滤器是 `ruagent=info`

- **file:line**：`crates/acp/src/run.rs:76-82`（`if dir == LineDirection::Stderr { tracing::debug!(target: "ruagent::acp::stderr", "{line}"); }`）；`crates/daemon/src/lib.rs:66-71`（`EnvFilter::try_from_default_env().unwrap_or_else(|_| "ruagent=info".into())`）。
- **复现**：`%TEMP%\t91-run.ps1`（两个 phase 除 `RUST_LOG` 外逐字相同：同一 root、同一 config、同一 mock agent 命令、各创建一次 chat 触发 harness 启动）；或手工 `RUST_LOG=... ruagent serve --root <tmp>` + 一次 `POST /api/v1/chat`。
- **证据读数**：
  ```
  phase A (默认过滤器)          : 'mock-agent starting' 在 daemon stdout 的命中 = 0 ; acp::stderr 目标命中 = 0
  phase B (该目标 = debug)      : 'mock-agent starting' 命中 = 2 ; acp::stderr 目标命中 = 2
  （mock agent 确实写 stderr：crates/mock-agent/src/main.rs:24 eprintln!("mock-agent starting, …")）
  ```
- **为什么重要**：harness 启动失败（npx 下载、认证、配置）**唯一**的现场就是子进程 stderr；`run.rs:76-77` 的注释说它"invaluable for diagnosing harness startup failures"，但 `ruagent=info` 让这条 sink 在默认配置下**被丢弃**。用户看到的现象是"run 卡住/失败但日志里什么都没有"——本代多次踩过的"事件存在 ≠ 那件事发生过"的日志版本。
- **可证伪的修复判据**：默认过滤器让该目标的 stderr 可见（例如默认 `ruagent=info,ruagent::acp::stderr=warn` 并把 sink 提到 `warn`），**或者**在 `--help`/AGENTS.md/coverage declaration 里写明"诊断 harness 启动失败需要 `RUST_LOG=ruagent::acp::stderr=debug`"。判据：不设 `RUST_LOG` 跑一个**会失败**的 harness（例如 command 指向不存在的可执行文件旁边的 shim），daemon 日志里应出现该子进程的 stderr 行。
- **owner**：acp owner + daemon（日志配置）。

### M3（medium · 工具契约 vs 实现）4xx/5xx 时工具丢掉守护进程的消息，而 200-with-outcome 时又带回来

- **file:line**：`crates/mcp/src/lib.rs:658-660`（`fn rpc_error(e) -> rmcp::ErrorData { ErrorData::internal_error(format!("daemon call failed: {e}"), None) }`）；调用点如 `:64-67`、`:151-153`、`:267-268`。
- **复现**：同一个 stdio 实例，两次 `tools/call`：
  ```json
  {"name":"memory_write","arguments":{"store":"bogus","namespace":"user","content":"x"}}
  {"name":"memory_write","arguments":{"store":"observation","namespace":"global","content":"x"}}
  ```
- **证据读数**：
  ```
  memory_write bad store      -> {"code":-32603,"message":"daemon call failed: HTTP status client error (400 Bad Request) for url (…/memory/write)"}
  memory_write blocked pair   -> {"content":[{"type":"text","text":"memory write: RejectedNamespace (this store does not allow that namespace; write vocabulary = profile×{user} | observation×{user,project:<x>,agent:<x>} | procedure×{project:<x>,global} | lesson×{project:<x>,global}; …"}],"isError":false}
  ```
  （`reqwest` 的 `error_for_status()` 把 400 渲染成 `HTTP status client error (400 Bad Request) for url (…)` —— **响应正文一个字都没带过来**。）
- **为什么重要**：同一个工具的信息量取决于下层走 400 还是走 200+"outcome"：走 200 时 agent 拿到了**完整写词表**（能自纠），走 400 时只拿到"400 Bad Request"。本代刚在 t52/t59/t79 上把"词表必须下发"做成了原则（`crates/memory/src/lib.rs:129-140`、我的 t79 P1），而 MCP 这条路径在 4xx 上把它**丢了一次**。
- **可证伪的修复判据**：`rpc_error` 保留状态码与**响应正文**（例如 4xx → `ErrorData::invalid_params(body, None)`，5xx → `internal_error(body)`）；判据：`memory_write{store:"bogus"}` 的 RPC 错误消息里必须出现守护进程正文里点名的合法 store 之一（今天为 0/4）。
- **owner**：mcp owner。

### M4（medium · 生命周期/超时）两个面都没有截止时间

- **file:line**：`crates/mcp/src/lib.rs:42`（`http: reqwest::Client::new()`，全文件无 `.timeout(`）；`crates/acp/src/**` 里 `timeout|Duration::from` 命中 **0**（唯一的时间常量是 `crates/mcp/src/health.rs:31` 的 `PING_TIMEOUT=10s`，那是 doctor/健康探针用的，不是工具）。
- **复现**：`grep -rn 'timeout' crates/mcp/src crates/acp/src`（只剩 health.rs 的 3 处）。
- **证据读数**：工具调用链 = `reqwest::Client::new()`（默认**无总超时**）→ 一个挂起/不可达的守护进程会让 `tools/call` 无限等待；ACP 侧一次 run/chat 的等待全在外部库的 `block_task()`/waits 上，本仓没有空闲超时。我的实跑只覆盖了"守护进程活着"（400/404 都在秒级返回），**挂起场景未测**（§5）。
- **为什么重要**：MCP 是 agent 的阻塞式工具面；没有超时的工具调用会把 agent 卡死在一个**本应立刻失败**的场景上（守护进程没起、端口没监听）。ACP 侧更微妙：长研究 run 应该允许长跑，但"连接建立"和"长时间无任何数据"是两类不同的等待，后者没有截止时间就意味着一个僵死的 harness 能把 run 永远留在 `running`（孤儿清扫只管**已死**进程，`daemon/src/lib.rs:73-75`）。
- **可证伪的修复判据**：MCP 客户端设连接超时 + 总超时（并把超时映射成可辨错误）；ACP 至少给"连接建立/首字节"设一个上限，并明确注释"run 本体不设总超时"的理由。判据：让守护进程拒绝连接（端口关闭）时 `tools/call` 在 ≤N 秒内返回可辨错误；让 harness 建立连接后不再说话时 run 在 ≤M 分钟内给出终态（今天：无限）。
- **owner**：mcp owner + acp owner。

### M5（medium · 测量面）工具集没有契约测试：断言的是 9 个名字的"包含"，不是 14 个的"相等"

- **file:line**：`crates/mcp/tests/roundtrip.rs:127-143`。
- **复现**：`sed -n '125,143p' crates/mcp/tests/roundtrip.rs`；对照 wire：`tools/list` 实测 **14**。
- **证据读数**：
  ```rust
  let names: Vec<String> = tools.iter().map(|t| t.name.to_string()).collect();
  for expected in ["memory_search","memory_write","memory_recall","memory_get","knowledge_search",
                   "knowledge_ingest","knowledge_expand","graph_entity","list_tasks"] {
      assert!(names.iter().any(|n| n == expected), "missing {expected}: {names:?}");
  }
  ```
  ⇒ ① 本代新增的 5 个（`graph_search`、`graph_retrieve`、`memory_forget_report`、`wiki_pages`、`wiki_links`）**不在断言里**；② `any()` + 没有数量断言 ⇒ **删掉那 5 个也不会有测试红**；③ 也没有"没有多余工具"的方向。整仓没有 `== 14`/`tools.len(), 14` 之类的断言（`health.rs:144/162` 只是把 `tools.len()` 当读数报出来）。
- **为什么重要**：`crates/mcp` 是**对外**的 agent 接口；一个被删掉的工具会静默破坏 agent 工作流（调用报 unknown tool 才被发现），而这是本代反复吃过的"能力悄悄消失"形状。HTTP 侧有 `knowledge_api.rs` 这类冻结断言，MCP 侧没有对应物。
- **可证伪的修复判据**：断言工具名**集合相等**于 14 个名字的冻结列表（双向：不缺不多）；判据：注释掉任一 `#[tool]` 后 `cargo test -p ruagent-mcp` 必须红（今天不红）。
- **owner**：mcp owner。

### M6（medium/low · 一致性）ACP 两个驱动各有一份 session/new 注入接线：**当前两边都在**，但没有共同测试面

- **file:line**：`crates/acp/src/run.rs:236-249`（塞 `describe_mcp_servers` 到 slot）与 `:304-309`（把 slot 变成 `mcp_injection_error_from`）；`crates/acp/src/chat.rs:418-424` 与 `:617-619`；共享措辞在 `crates/acp/src/lib.rs:44-95`。
- **复现**：`grep -n 'describe_mcp_servers\|mcp_injection_error' crates/acp/src/*.rs` ⇒ 两个驱动各 2 处 + lib.rs 定义。
- **证据读数**：**两边都做了**（我核过：chat.rs 也把注入清单塞进 error，不是"修了一个漏一个"——这一点我先证伪再写）。但接线是**两份**：`run.rs` 用 `Arc<Mutex<Option<String>>>` 的 slot 绕开库固定 error 类型，`chat.rs` 用另一份同样的 slot 逻辑；`lib.rs` 的测试只覆盖**措辞函数**（`describe_mcp_servers`/`mcp_injection_message`），**没有任何测试**驱动任一驱动的 session/new 失败路径。
- **为什么重要**：这是"两份实现"里唯一真实的一份（t63/t64 的修复只落在 `lib.rs` 的措辞上，两个驱动的接线各自长出来）。下一次改注入语义时，两个驱动可以漂移而测试仍绿。
- **可证伪的修复判据**：抽出共同的 `inject_session(...)`/或给两个驱动各一个 session/new 失败路径的测试（用假 transport / 假 connection）；判据：把一个驱动的注入清单改成空后，测试必须红。
- **owner**：acp owner。

### M7（low · 恒真断言）`crates/acp/src/lib.rs:150-153` 的 `assert_ne!` 是死守卫

- **file:line**：`crates/acp/src/lib.rs:150-153`。
- **复现**：`sed -n '146,154p' crates/acp/src/lib.rs`。
- **证据读数**：`text = mcp_injection_error(&servers, …).to_string()`，而 `AcpError::McpInjection` 的 Display 是 `mcp_injection_message(servers, raw)`（`lib.rs:33`、`:71-78`），**构造上一定以"会话建立失败（…）"开头** ⇒ 它永远不可能等于那个 `r#"ACP protocol error: …"#` 字面量。真正要断的性质（"不能是那句不透明字符串"）已被上面 4 条 `contains` 覆盖 ⇒ 这一条**不可失败**。
- **为什么重要**：本代已把"恒真断言"当一类缺陷在扫（t70 只找到 `migrations.rs:1285-1286` 两条自比较；这是**第三条**，形态是"与一个不可能产生的字面量比较"）。它在测试里看起来像"回归护栏"，实际是一句注释。
- **可证伪的修复判据**：删除，或改成能失败的形式（如 `assert_ne!(text, raw)`，`raw` 是传入的原始错误）；判据：把 `mcp_injection_message` 的返回值人为改成那句不透明字符串后，测试必须红（今天不红）。
- **owner**：acp owner。

### M8（low · 归因错误）通道关闭被报成"invalid agent command `chat closed`"

- **file:line**：`crates/acp/src/chat.rs:183-187`（`.map_err(|_| AcpError::Command("chat closed".into()))`）；措辞在 `crates/acp/src/lib.rs:24-25`（`#[error("invalid agent command \`{0}\`")]`）。
- **复现**：`sed -n '182,188p' crates/acp/src/chat.rs` + `sed -n '19,26p' crates/acp/src/lib.rs`。
- **证据读数**：`ChatSession::send` 的失败原因是**通道关闭**（chat 任务结束/协议错误），但 `|_|` 把原因丢掉、换成一个 `Command` 变体，于是 Display 变成 `invalid agent command \`chat closed\``——**用户被告知 agent 命令非法**，而真实原因（chat 结束了）不在任何地方。
- **为什么重要**：这正是本代在用户面前反复吃亏的形状（t63 的"opaque Internal error，无法行动"）：错误文本必须指向可行动的原因。此处是它的低配版。
- **可证伪的修复判据**：专门的变体（`AcpError::ChatClosed`）或保留源错误；判据：通道关闭时给用户的文本里不出现 "invalid agent command"。
- **owner**：acp owner。

### M9（low · 措辞）注入清单的兜底把 Debug 表示塞进用户可见中文

- **file:line**：`crates/acp/src/lib.rs:63`（`other => format!("{other:?}")`）。
- **复现**：`sed -n '50,67p' crates/acp/src/lib.rs`。
- **证据读数**：`describe_mcp_servers` 对 `Stdio/Http/Sse` 各有中文格式，其它变体落到 `{other:?}`（Rust Debug 结构体输出）直接嵌进 `mcp_injection_message` 的中文句子。
- **为什么重要**：低危，但这是**用户面向**的失败文案（t63 的整个教训就是"失败文案要能读懂"），一个 Debug dump 会毁掉那句话的可读性。
- **可证伪的修复判据**：未知传输类型给中文名（或"未知类型"）+ 原始字符串；判据：任一 `McpServer` 变体渲染出的文本里不含 `{`/`}` 结构。
- **owner**：acp owner。

### M10（low · 身分）MCP 服务端自报 `name: "rmcp"`（库默认），不是 ruagent

- **file:line**：`crates/mcp/src/lib.rs:455-463`（`ServerInfo::new(caps).with_instructions(…)`，**没有**设 implementation/name）。
- **复现**：我的 stdio 实例的 `initialize` 响应。
- **证据读数**：`{"result":{"protocolVersion":"2024-11-05","capabilities":{"tools":{}},"serverInfo":{"name":"rmcp","version":"3.3.0"},"instructions":"ruagent platform tools: …"}}` ⇒ 客户端/日志里这个服务叫 **rmcp 3.3.0**（库的名字/版本）；真正的身分只在 `instructions` 文本里。
- **为什么重要**：多 MCP 服务的客户端（或排障时读 `mcp.json`/日志的人）按 `serverInfo.name` 区分服务；现在所有基于 rmcp 的服务都叫 rmcp。
- **可证伪的修复判据**：`ServerInfo::new(...).with_server_info(Implementation::new("ruagent", env!("CARGO_PKG_VERSION")))`；判据：`initialize` 的 `serverInfo.name == "ruagent"`。
- **owner**：mcp owner。

## 3 正面读数（查过、**不是**缺陷；免得下一轮重复）

1. **MCP 不是第二份实现**：14 个工具逐条对照 URL/参数，全部是 `{daemon_url}/api/v1/…` 的一行代理；`lib.rs:289-296` 明写 "ONE consumption surface, not a second implementation … No write tool is added here"（我核对了：5 个 gen2 工具里确实没有写工具）。
2. **注入面干净**：`params.id: i64`、`chunk_id: i64`、`entity id: i64`（`lib.rs:513-549`）⇒ 路径插值不接受任意字符串；`query`/`content_hash`/`as_of` 走 `reqwest` 的 `.query()`（百分号编码）；`store/namespace/content`/`name/content` 走 JSON body；`crates/mcp`、`crates/acp` 里**没有 SQL 字符串**、**没有把用户文本拼进文件路径**（`acp/adapter.rs:34-68` 只扫 PATH 找可执行文件）。
3. **参数校验可辨**：`memory_recall{}` 缺 query ⇒ `{"content":[{"type":"text","text":"failed to deserialize parameters: missing field \`query\`"}],"isError":true}` ✓（`isError:true` + 点名缺哪个字段）。
4. **未知 id 的 HTTP 4xx 确实变成 RPC 错误**：`memory_get{id:999999999}` ⇒ `-32603`（可辨失败，虽然正文丢了——见 M3）。
5. **空 = 读数不是错误**：`wiki_pages` ⇒ `{"pages":[]}`、`wiki_links` ⇒ 完整空图（`nodes/edges/broken/orphans/wanted/unreachable/self_links/degrees/readings_at`）⇒ 空库不被伪装成失败 ✓（方向与 M1 相反，说明是**局部**问题不是政策）。
6. **ACP 的注入失败文案设计是对的**：`lib.rs:26-37` 的 `McpInjection` 明确反对不透明错误，并有 4 条测试覆盖措辞；`run.rs:236-249`/`chat.rs:418-424` **两个驱动都接了线**（我核对过，不是修一半）。
7. **两个 crate 没有 `#[ignore]`**：t70 清点的 15 个 `#[ignore]` 全在 daemon/store/graph/knowledge；`crates/mcp`、`crates/acp` 为 0 ⇒ 这两个面的测试不会被 CI 跳过。
8. **`mcp-serve` 生命周期干净**：`terminate()` 后进程退出（exit code 1 = 被打断，无信号语义），**stderr 为空**（没有 panic/警告），stdout 上的 JSON-RPC 一直可解析 ⇒ 没有"启动即污染 stdout"的经典 MCP 事故。

## 4 未验证猜想（**不是** finding）

- **G1** `wiki_pages` 的描述说每个页面带 `status` 与 `freshness`：我在**空根**上只能读到 `{"pages": []}`；`freshness`/`cite_coverage`/`has_anchors` 我在 `crates/daemon/src/wiki.rs` 的其它结构上看到了（`3239/3242`、`3049/3083`），但 **`status` 我没有读到**。取读数的办法：临时 root 上 ingest + 建一个 wiki 页，再 `tools/call wiki_pages` 打印键集（一次即可定因）——本单预算没做。
- **G2** M4 的挂起时长未测（我只有"守护进程活着时 400/404 秒级返回"的读数）。取法：把 `RUAGENT_URL` 指向一个不监听的端口（连接立即被拒 vs 黑洞 IP 会挂起），计时。
- **G3** ACP 的**子进程退出码 → 终态**映射我**没有**实测：`run.rs:300-302` 只把 `stop_reason` 映射进 `RunOutcome`；"进程被杀/非零退出"落到哪个终态由库决定（`agent_client_protocol`），我在本仓没有对应字节可读。取法：让 mock agent 启动后立即 `exit 3`，看 run 的终态与错误文案。
- **G4** M1 的"守护进程对未知实体返回 200 还是 404"我没直连核实（我的 wire 读数只证明 MCP 层最后给了 `isError:false`；若守护进程本来 404，那 M3 的 `error_for_status` 早就该报错）。取法：`curl /api/v1/graph/entity/999999999`。

## 5 未覆盖范围（每条写原因）

| 未覆盖 | 原因 |
| --- | --- |
| 真实 harness（claude-code / dsh / opencode）的 ACP 行为 | 需要真实 CLI 与凭据；本单只用 `ruagent-mock-agent`（仓库认可的第一类测试资产）。 |
| **外部依赖 `agent_client_protocol` 的内部**（半包处理、超时、退出码、行切分） | 它是 crates.io 依赖，**不属 `crates/acp` 的字节**；本仓只把它的事件/行 sink 接进 tracing。凡涉及半包/退出码的判断我都标注为"库拥有"。 |
| MCP 的 HTTP/SSE 传输与多客户端 | 我只验了 **stdio**（本代实际用法）；`McpServer::Http/Sse` 是 ACP 侧注入形态，不是本服务端提供的传输。 |
| `graph_retrieve` 的 `as_of`/`include_superseded` 行为 | 静态清点已做（参数确实透传、`.query()` 编码）；运行时读数在 t20 的 HTTP 侧（`as_of` 三写法逐字节一致 + 非时刻 400）——本单未重跑。 |
| 挂起/超时场景（M4）、未知实体的 HTTP 状态（G4）、wiki 页键集（G1） | 见 §4，每条给了取数办法与未做的原因。 |
| `crates/mcp/src/health.rs` 的 doctor 检查 | 只读了它的超时常量与 `tools.len()` 读数（`health.rs:144/162`），未跑 `ruagent doctor`。 |

## 6 我这一轮的操作与收尾

- **我起的进程（全部记录、全部已停）**：临时 daemon phase A pid **41496**、phase B pid **87576**（同一个 `%TEMP%\ruagent-t91`、端口 **8817**；`alive-after-stop=False` ×2）；**我自己的 `ruagent mcp-serve`** pid **77668**（`terminate()` 后退出，stderr 空）。**真守护进程 pid 79984 全程未触碰**（每步后都读到 `alive StartTime=2026/9/27 5:35:37`）。
- **我写入的文件**：只有本报告 `docs/design/reviews/gen3-audit-mcp-acp.md`；`crates/mcp/**`、`crates/acp/**` 零字节改动；未写活库。
- 仪器（`%TEMP%`）：`t91-run.ps1`、`t91-chat.py`、`t91-mcp.py`、`t91-harness.log`；临时 root `%TEMP%\ruagent-t91`（内含 phase A/B 的 daemon stdout/stderr，作为 M2 的原始证据；报告落盘后删除）。
- 我读过的行（本轮）：`crates/mcp/src/lib.rs:51-…415`（14 个 `#[tool]` 块）、`:272-287`、`:289-296`、`:455-463`、`:466-553`、`:658-660`；`crates/mcp/tests/roundtrip.rs:125-143`；`crates/acp/src/run.rs:60-105`、`:236-249`、`:300-309`；`crates/acp/src/chat.rs:182-196`、`:418-424`、`:617-619`；`crates/acp/src/lib.rs:19-95`、`:146-154`；`crates/daemon/src/lib.rs:60-79`；`crates/mock-agent/src/main.rs:24`；`crates/daemon/src/api.rs:53-60`、`:1089-1122`；`crates/daemon/src/wiki.rs:3227-3243`。
