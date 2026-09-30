// i18n domain: settings.*
//
// Copy for this domain only. New strings for a view belong in THAT view domain
// file (see panel/src/i18n/README.md and the header of panel/src/i18n.tsx): the point
// of the split is that two tasks editing two different views never touch one file.
// Keep both languages in step: a key added here must be added to zh AND en.

export const zh: Record<string, string> = {
  "settings.title": "设置",
  "settings.subtitle": "蒸馏策略、能力插件",
  "settings.noRoles": "没有可选角色",

  // ---- capability plane (docs/plans/capability-plugins-design.md §4) -------
  // The NAME of each capability is panel copy, so it is keyed here, one key per
  // registry id. The description, the `gates` phrase and a conflict's `reason`
  // are the daemon's own user-facing strings (design §6 says the API returns
  // them verbatim): they are rendered as data and deliberately NOT duplicated
  // into these dictionaries, which would create a second source of truth that
  // drifts from the registry. Ids are stable forever (design §4.2), so the
  // mapping is safe; an id this build has never heard of falls back to the id.
  "settings.capabilities.title": "能力插件",
  "settings.capabilities.hint":
    "每一项都即时生效并写入 policy.toml；关掉一条召回腿会立刻改变 /api/v1/recall 的返回。默认配置与今天完全一致。",
  "settings.capabilities.sourceDefault": "policy.toml 未写入 = 今天的行为",
  "settings.capabilities.sourceFile": "已写入 policy.toml",
  "settings.capabilities.tier.free": "零 token",
  "settings.capabilities.tier.freeHint": "零 token：确定性执行，不调用模型。",
  "settings.capabilities.tier.llm": "llm",
  "settings.capabilities.tier.llmHint": "llm 档：每次执行都会调用模型并消耗 token。",
  "settings.capabilities.notDefault": "非默认",
  "settings.capabilities.newBadge": "新增",
  "settings.capabilities.costWarning": "启用后会消耗模型 token。",
  "settings.capabilities.costTitle": "启用消耗 token 的能力？",
  "settings.capabilities.costBody":
    "「{name}」属于 llm 档。启用后，每次触发都会调用模型并消耗你的 token；关闭时不需要任何确认。",
  "settings.capabilities.costConfirm": "启用（消耗 token）",
  "settings.capabilities.saved": "已保存",
  "settings.capabilities.err": "无法读取能力配置",
  "settings.capabilities.errHint":
    "读不到 GET /api/v1/capabilities：daemon 不可达、它还是不含这个路由的旧二进制，或者响应不符合契约。",
  "settings.capabilities.saveFailed": "保存失败——开关保持服务端的值，daemon 的原文如下。",
  "settings.capabilities.conflictTitle": "有行为被你的能力配置抑制",

  // The option editor. The KEY names (weight, min_score, …) are the API's own
  // and are shown verbatim, like the daemon's description/gates phrases: they
  // are what the user would otherwise type into policy.toml. Only the panel's
  // own words live here.
  "settings.capabilities.options.title": "选项",
  "settings.capabilities.options.apply": "应用",
  "settings.capabilities.options.reset": "恢复默认值",
  "settings.capabilities.options.resetHint":
    "把这一行的选项键从 policy.toml 里删掉，让注册表默认值重新生效——不是把默认值写成你的意见。",
  "settings.capabilities.options.unapplied": "有未应用的修改",
  "settings.capabilities.options.notFinite": "必须是有限数字",
  "settings.capabilities.options.notInteger": "必须是整数",
  "settings.capabilities.options.outOfRange": "超出范围（{expectation}）",
  "settings.capabilities.options.refused": "有输入未通过校验，这一行没有发送任何请求。",

  "settings.capabilities.name.memory_inject_chat": "会话首轮记忆注入",
  "settings.capabilities.name.memory_inject_runs": "运行提示词注入",
  "settings.capabilities.name.recall_leg_memory_semantic": "召回腿 · 记忆语义",
  "settings.capabilities.name.recall_leg_memory_fts": "召回腿 · 记忆关键词",
  "settings.capabilities.name.recall_leg_knowledge_semantic": "召回腿 · 知识库语义",
  "settings.capabilities.name.recall_leg_knowledge_fts": "召回腿 · 知识库关键词",
  "settings.capabilities.name.recall_leg_wiki": "召回腿 · Wiki 页面",
  "settings.capabilities.name.recall_leg_graph": "召回腿 · 知识图谱",
  "settings.capabilities.name.session_extract_rules": "会话规则提取",
  "settings.capabilities.name.knowledge_ingest_graph": "知识库 → 图谱摄入",
  "settings.capabilities.name.distill_session": "会话自动蒸馏",
};

export const en: Record<string, string> = {
  "settings.title": "Settings",
  "settings.subtitle": "distillation policy, capabilities",
  "settings.noRoles": "No roles available",

  // ---- capability plane (docs/plans/capability-plugins-design.md §4) -------
  "settings.capabilities.title": "Capabilities",
  "settings.capabilities.hint":
    "Each one takes effect immediately and is written to policy.toml; turning a recall leg off changes what /api/v1/recall returns. The default configuration behaves exactly as before.",
  "settings.capabilities.sourceDefault": "nothing written to policy.toml = today's behaviour",
  "settings.capabilities.sourceFile": "written to policy.toml",
  "settings.capabilities.tier.free": "free",
  "settings.capabilities.tier.freeHint": "Zero tokens: deterministic, no model call.",
  "settings.capabilities.tier.llm": "llm",
  "settings.capabilities.tier.llmHint": "llm tier: every run calls the model and spends tokens.",
  "settings.capabilities.notDefault": "not default",
  "settings.capabilities.newBadge": "new",
  "settings.capabilities.costWarning": "Enabling this spends model tokens.",
  "settings.capabilities.costTitle": "Enable an llm-tier capability?",
  "settings.capabilities.costBody":
    "“{name}” is llm-tier. Once enabled, every trigger calls the model and spends your tokens; turning it off needs no confirmation.",
  "settings.capabilities.costConfirm": "Enable (spends tokens)",
  "settings.capabilities.saved": "Saved",
  "settings.capabilities.err": "Cannot read the capability plane",
  "settings.capabilities.errHint":
    "Cannot read GET /api/v1/capabilities: the daemon is unreachable, it is an older binary without this route, or the response does not match the contract.",
  "settings.capabilities.saveFailed":
    "Save failed — the switches keep the server's value; the daemon's own message is below.",
  "settings.capabilities.conflictTitle": "This capability config suppresses behaviour you had",

  // The option editor (see the zh block above for why the KEY names are not
  // translated: they are the API's own).
  "settings.capabilities.options.title": "Options",
  "settings.capabilities.options.apply": "Apply",
  "settings.capabilities.options.reset": "Reset to defaults",
  "settings.capabilities.options.resetHint":
    "Removes this row's option keys from policy.toml so the registry defaults apply again — it does not write the defaults in as your opinion.",
  "settings.capabilities.options.unapplied": "unapplied changes",
  "settings.capabilities.options.notFinite": "must be a finite number",
  "settings.capabilities.options.notInteger": "must be a whole number",
  "settings.capabilities.options.outOfRange": "out of range ({expectation})",
  "settings.capabilities.options.refused": "An input failed validation; no request was sent for this row.",

  "settings.capabilities.name.memory_inject_chat": "Chat first-prompt injection",
  "settings.capabilities.name.memory_inject_runs": "Run prompt injection",
  "settings.capabilities.name.recall_leg_memory_semantic": "Recall leg · memory semantic",
  "settings.capabilities.name.recall_leg_memory_fts": "Recall leg · memory keyword",
  "settings.capabilities.name.recall_leg_knowledge_semantic": "Recall leg · knowledge semantic",
  "settings.capabilities.name.recall_leg_knowledge_fts": "Recall leg · knowledge keyword",
  "settings.capabilities.name.recall_leg_wiki": "Recall leg · wiki pages",
  "settings.capabilities.name.recall_leg_graph": "Recall leg · knowledge graph",
  "settings.capabilities.name.session_extract_rules": "Session rule extraction",
  "settings.capabilities.name.knowledge_ingest_graph": "Knowledge → graph ingestion",
  "settings.capabilities.name.distill_session": "Session auto-distillation",
};
