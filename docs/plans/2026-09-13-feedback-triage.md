# 2026-09-13 用户反馈梳理与分方向推进计划

来源：用户全面评审后的反馈。按方向整理，含现状事实（已实测）、建议方案、优先级。

## A. UI / 体验

**用户反馈**：没有设计感；组件缩在一起、不随分辨率自适应；对话页选择器（智能体/模型/权限模式/…）应贴左下角（跟随输入框）而不是悬在左侧中间；夜间比日间丑；大量 i18n 漏翻。

**现状事实**：
- 自定义 CSS 固定 padding（24px 28px），无 clamp/断点缩放；1440px 以下开始挤
- 对话页 pickers 在视图顶部（chat-bar 在 chat-log 上方）
- 实测漏翻：记忆页 Browse/Audit log/profile/observation/procedure/lesson/user + 知识页 Delete + 图谱页全部英文 placeholder（Search entities…/Alice/works_at/As of date…）+ 收件箱 Allow/Reject

**方案**：
1. A1 对话页重构：pickers 移到底部 composer 上方一行（紧凑、贴左），顶部只留标题+新对话
2. A2 响应式：padding 用 clamp() 随视口缩放；补断点；侧栏在窄屏可折叠（antd Sider collapsible/自适配 breakpoint）
3. A3 i18n 清扫：扫全部视图的硬编码英文 → 补 zh/en 词条（本次做）
4. A4 设计感：布局密度调优 + 日间微调对比（P1，先解决结构问题）

## B. 知识库（用户最大质疑，实测证实）

**用户质疑**：摄入是不是只是简单切块？用什么 embedding 模型？召回怎么做的？没有内置模型怎么摄入？建议参考前沿知识图谱/RAG 论文；提到「召回时只读很少一段话 + 图谱方式召回」。

**现状事实（今日实测）**：
- 摄入 = 段落切块（800 字符 + 100 overlap），无结构感知（无标题层级感知）
- 嵌入 = **hash-embedder 回退正在线上运行**（词袋哈希，零语义）。fastembed(bge-small-en-v1.5) 代码存在但需 RUAGENT_EMBEDDER=fastembed 且首次下载模型
- 召回 = LanceDB ANN + SQLite FTS5 + RRF 融合，无重排、无图谱
- 实测语义查询 "how to undo a release" 分数 0.016（Rollback 段落没被语义命中）
- **图谱与知识库完全断连**：图谱只有手动建实体/事实，无 LLM 抽取，无参与召回

**方案**：
1. B1（P0）真实嵌入默认开启：fastembed 为默认，下载失败才回退 hash；面板显示当前嵌入器状态
2. B2（P1）结构感知切块：markdown 标题树切块（段落聚合到标题下），代码块不切断
3. B3（P1）图谱参与召回（用户的方向 = HippoRAG 2 / LightRAG 路线）：
   - 检索时先图谱定位（实体→邻域子图）→ 只取相关子图的文档块（「只读很少一段话」）
   - 调研仓库：HippoRAG 2（OSU-NLP-Group）、LightRAG（HKUDS）、GraphRAG（microsoft）——借模式不借码（注意许可证）
4. B4（P1）加 reranker 腿（bge-reroner / ONNX），RRF 三路融合
5. B5 面板知识库页显示：嵌入器、模型、切块数、召回质量自检按钮

## C. 记忆与对话历史整合（OpenViking 方向）

**用户反馈**：不同 agent 能同步记忆吗？提到一个同步 claude code/codex/dsh 记忆的 dsh 插件；OpenViking 能整合每个 agent 的对话历史，我们能做到吗？

**现状事实**：
- 记忆 = 平台中心库（4 store × 命名空间），MCP 工具 memory_search/memory_write 已暴露给所有 agent → **共享记忆读写本来就通**（设计如此），但检索只有 FTS（实测 "programming language preferences" 搜不到 "User prefers Rust"，0 命中）
- 对话历史：chat/run 转录（JSONL）存在但**没有任何提取管道** → 不产生记忆/图谱
- 未接入任何外部 agent 的历史（~/.claude/projects/*.jsonl、dsh session 存储、codex 历史都没摄入）

**调研发现**（GitHub）：
- volcengine/OpenViking（36.9k★，AGPLv3——**只借鉴模式，不能抄代码**）：viking:// 虚拟文件系统、L0/L1/L2 上下文分层、会话提交后台抽取（会话→记忆候选→与既有记忆比对 增/并/跳过）
- Rxiain/dsh-openviking：dsh 的 OpenViking 检索/自动召回/会话记忆插件
- Castor6/openviking-plugins：Claude Code 等 CLI 的记忆插件
- iamtouchskyer/memex（141★）：Zettelkasten 跨 Claude Code/Cursor 持久记忆

**方案**：
1. C1（P1）记忆检索升级：记忆也接嵌入（与知识库共用 embedder）+ 语义召回
2. C2（P1）会话提交管道（OpenViking 模式）：对话结束/用户点「沉淀」→ 后台用 LLM 从转录抽取 → 记忆候选（去重/合并/跳过 策略）+ 图谱实体/事实候选 → 人审（收件箱模式）或自动
3. C3（P2）外部历史摄入：importers 读 ~/.claude/projects/*/*.jsonl、dsh storages、codex → 同一提取管道（这就是「同步不同 agent 记忆」的实现路径）
4. C4 MCP 工具补全：给 agent 的工具加 graph_search / memory_supersede / knowledge 检索带命名空间

## D. 智能体 vs 运行时 概念重构（架构缺陷，用户判断正确）

**用户反馈**：智能体=功能角色（架构 agent、插件开发 agent），提示词应可跨运行时；运行时=dsh/opencode/claude-code；agent 可绑定运行时也可脱离；agent 可组合；dsh 有权限模式没接入；希望统一配置（思考等级/权限）屏蔽差异，又担心丢失运行时特有功能（如 dsh 创造模式）。

**现状事实**：agents.toml 的 [agent.X] 把两者焊死（每个条目既是角色又是运行时实例）；权限模式/思考力度已通过 ACP session options 通用机制接入（chat 页），但任务运行（runs）没用上；dsh 原生 presets 不走 ACP（已确认）。

**方案（两层模型）**：
```
[runtime.dsh]        # 运行时：怎么启动、原生能力面
command = "dsh --profile acp"
[runtime.opencode]
[runtime.claude-code]

[agent.architect]    # 智能体：角色 + 可移植提示词 + 偏好
prompt = "You are the architecture reviewer…"
runtimes = ["dsh", "claude-code", "opencode"]   # 或 any
default_runtime = "dsh"
model = null        # 继承运行时默认
effort = "high"     # 通用层 → 适配到各运行时原生选项
permission = "default"

[agent.plugin-dev]
prompt = "…"
runtimes = ["dsh"]                # 只在 dsh（需要 preset 时）
runtime_overrides.dsh = { preset = "cordis" }   # 运行时特有逃生舱
```
- **统一层**（model/effort/permission）经我们已有的 ACP session-options 机制映射到各运行时原生旋钮 → 可移植
- **运行时特有功能**（dsh preset/创造模式）放 runtime_overrides 逃生舱 + 面板「高级」区展示 → 不屏蔽可能性
- agent 组合：现有 pipeline/fanout 已支持按 agent 编排，重构后语义更清晰
- 迁移：agents.toml 向后兼容（旧格式自动升级为 runtime+agent 各一）

## E. 测试欠账（用户点名）

**用户反馈**：记忆/知识库/图谱/对话/看板没完整测过；「不能仅仅是看上去做了」。

**现状**：单测存在（memory/graph/knowledge 各自 crate），但端到端功能链路没验证过；本次实测发现知识库实际跑在 hash 回退、记忆语义检索为 0 命中——正是缺测试的代价。

**方案**：
1. E1（P0）功能自检命令/端点：`ruagent doctor` —— 嵌入器状态、语义召回抽检（内置 3 组必须命中的查询）、记忆生命周期、图谱遍历、chat 链路（mock agent）
2. E2（P1）e2e 集成测试扩展：mock-agent 驱动全链路（已有 e2e_daemon.rs，补 knowledge 质量/记忆生命周期/图谱遍历断言）
3. E3（P1）面板 E2E（playwright，CI 可选）

## F. 工具

**用户要求**：装 websearch/webfetch 工具。现状：本会话自带 WebSearch/WebFetch（WebFetch 对 github.com 被域校验拦截；WebSearch 间歇异常），curl + GitHub API 稳定可用；已用其完成 OpenViking/记忆同步生态调研。可再评估装一个搜索类 MCP（如 Tavily/Brave MCP）作为补充。

## 执行顺序

| 优先 | 项 | 说明 |
|---|---|---|
| P0 | A1 A2 A3 + B1 | 对话页布局、响应式、i18n 清扫、真实嵌入默认 |
| P1 | E1 E2 | doctor 自检 + e2e 补测（先让「做了」变成「验证过」）|
| P1 | D 设计文档 | agent/runtime 两层模型（大改，先评审再动手）|
| P1 | B2 B3 B4 + C1 | 知识库/记忆召回升级（图谱检索是核心方向）|
| P2 | C2 C3 | 会话沉淀管道 + 外部历史摄入 |
