# UX Audit Triage — 2026-09-19

从成熟产品视角对面板做了一轮真实走查（Playwright DOM 度量 + 源码审读，daemon 在线、10 个路由全过）。
结论先行：**数据面健康（10 路由零 console 错误、零 4xx/5xx、i18n 键 430/430 齐平），欠账集中在三处：响应式外壳、断线可感知性、聊天控制权**。

## 度量证据

| 发现 | 证据 | 判定 |
|---|---|---|
| Sider 固定 228px 永不折叠 | 390px 视口下 7/8 视图横向溢出，内容区仅剩 162px | **P0 修** |
| grid 子项缺 `min-width:0` | `.dash-grid` 轨道被一条 1131px nowrap 会话标题撑到 1087px（768px 视口实测） | **P0 修** |
| 加载错误静默吞 | 所有列表视图 `.catch(() => setX([]))`；daemon 挂掉时产品看起来"没数据"而非"连不上"（仅页脚一个小点） | **P0 修** |
| Chat SSE 首错即断无重连 | `es.onerror = () => { es.close(); setStreaming(false) }`——传输抖动后流死亡，且未完成的回复被标成完成 | **P0 修** |
| Chat 无停止按钮 | 流式期间发送按钮仅禁用；daemon 无 stop 端点；acp/chat.rs 的 prompt 在命令循环 inline await，Stop 命令进不来 | **P0 修（委派 impl）** |
| `common.loading` 键缺失 | Home 加载时裸露键名（zh/en 均缺） | 顺手修 |

## 已达标（不动）

- 收件箱 5s 自动刷新 + 徽标 2s 轮询；run 取消/重试/落盘按钮齐全；成本展示（stats + timeline）。
- 首启引导（Home 无数据时 intro guide）、暗色模式、命令面板、空态渲染（空库 e2e 覆盖）。
- Wiki 作为知识库标签页可达（设计如此，非缺陷）。

## 本轮修复方案

1. **响应式外壳**（我做）：Sider 断点自动折叠（<992px 收成 64px 图标栏，手动开关保留）+ 折叠态页脚（连接点/主题/语言图标化）+ `.dash-grid`/`.kanban` 子项 `min-width:0` + hero 行可换行 + 窄屏内容边距收紧。e2e 新增 responsive spec（390/768 无横向溢出 + sider 折叠断言，空库/真库双环境可跑）。
2. **全局离线横幅**（我做）：App 层 `daemonUp` 已有 2s 轮询——离线时 Content 顶部一条 Alert（"daemon 不可达，正在重试"），恢复自动消失。视图级空态不动（横幅已解释"为什么是空的"）。
3. **Chat SSE 有界重连**（我做）：onerror 时若仍在流式 → 清空重挂（transcript replay 重建状态，含 missed Stopped）退避重试 ≤5 次；EventSource CLOSED（404，daemon 重启后会话死亡）→ 快速失败，提示"会话已中断"。
4. **Chat 停止**（委派 impl，Rust only）：`ChatCommand::Stop` + prompt 驱动改 `SentRequest` 句柄 + `tokio::select!`；`$.cancel_request` 走 SDK 原生 `SentRequest::cancel()`（2.1.0 文档核实：对端以正常数据或 -32800 响应）；取消 → `Stopped{Cancelled}`，会话存活可继续；`POST /api/v1/chat/{id}/stop`（幂等，未知 404）；mock 新增 slowreply 行为（10s 延迟窗口）；e2e 断言 3s 内收到 Cancelled 且会话仍可对话。panel 按钮由我在合入后补（避免 Chat.tsx 并发冲突）。

## 明确不做（本轮）

- 权限审批审计日志（approver 决策事后不可见）——记录为后续项。
- RunTimeline SSE 重连——仅活跃 run 挂载，daemon 重启后 run 本身已死，刷新即终态，无真实痛点。

## 验收

- responsive e2e 双环境绿；真实 daemon 上 390px 视口逐页 DOM 断言无溢出。
- 离线横幅：kill daemon → 横幅出现；起 daemon → 消失（面板不刷新）。
- Chat 停止：真实会话流式中点停 → 秒级 Stopped，后续消息正常。
