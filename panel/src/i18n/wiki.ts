// i18n domain: wiki.*
//
// Copy for this domain only. New strings for a view belong in THAT view domain
// file (see panel/src/i18n/README.md and the header of panel/src/i18n.tsx): the point
// of the split is that two tasks editing two different views never touch one file.
// Keep both languages in step: a key added here must be added to zh AND en.

export const zh: Record<string, string> = {
  "wiki.stat": "{n} 页 · {stale} 失效 · {broken} 断链",
  "wiki.compile": "编译 Wiki",
  "wiki.runningHint": "已有构建在运行",
  "wiki.wanted": "想要的页：",
  "wiki.empty.title": "还没有 wiki 页",
  "wiki.empty.hint": "把源文档编译成互相链接的综述页——先预览计划，确认后执行",
  "wiki.staleTag": "源已更新",
  "wiki.editedTag": "手改",
  "wiki.orphanTag": "孤儿页",
  "wiki.links": "{out} 出 {in} 入",
  // 覆盖率三态（R-D D.2 / RV-D-1）。`unknown` 不是 0，也不是 1：这一页**从来没
  // 有构建记录过可验证读数**（手写的页、或记录值已不再适用）。键**不许省略** ——
  // 「不知道」必须显示成不知道，与 stale=unknown / edited=unknown 同一套词表。
  "wiki.coverage": "覆盖 {s}",
  "wiki.coverageUnknown": "覆盖 unknown",
  "wiki.coverageHint":
    "构建期记录的「有锚内容节 / 内容节」比例。unknown = 没有可用的构建期记录（从没构建过，或页面已失效 ⇒ 旧记录不再适用）；它既不是 0 也不是 1。",
  "wiki.freshUnknown": "新鲜度 unknown",
  "wiki.builds": "构建历史",
  "wiki.buildPages": "{written}/{planned} 页",
  "wiki.buildFailed": "{n} 失败",
  "wiki.action.create": "新建",
  "wiki.action.update": "更新",
  "wiki.action.delete": "删除",
  "wiki.action.keep": "保留",
  "wiki.compile.scope": "范围",
  "wiki.compile.scope.all": "全部源",
  "wiki.compile.scope.changed": "仅变更",
  "wiki.compile.agent": "智能体",
  "wiki.compile.agent.auto": "自动（dsh 优先）",
  "wiki.compile.preview": "预览计划",
  "wiki.compile.replan": "重新规划",
  "wiki.compile.confirm": "确认执行",
  "wiki.compile.planned": "计划就绪 — {n} 页",
  "wiki.compile.started": "构建 #{id} 已启动",
  "wiki.compile.gate": "确认后才会写入页面；手改过的页会被跳过。",
  "wiki.view.edit": "编辑此页",
  "wiki.err": "无法读取 Wiki",
  "wiki.buildsEmpty": "还没有构建记录",
  "wiki.buildsEmptyHint": "编译 Wiki 会先给出干跑计划，确认之后才写入页面。",
  "wiki.pageCap": "只列出前 {n} 页，请先收紧范围或删除孤儿页",
};

export const en: Record<string, string> = {
  "wiki.stat": "{n} pages · {stale} stale · {broken} wanted",
  "wiki.compile": "Compile wiki",
  "wiki.runningHint": "a build is already running",
  "wiki.wanted": "wanted pages:",
  "wiki.empty.title": "No wiki pages yet",
  "wiki.empty.hint": "Compile your source documents into interlinked overview pages — preview the plan, then confirm",
  "wiki.staleTag": "sources updated",
  "wiki.editedTag": "hand-edited",
  "wiki.orphanTag": "orphan",
  "wiki.links": "{out} out {in} in",
  // Coverage's third state (R-D D.2 / RV-D-1). `unknown` is neither 0 nor 1: no
  // build ever recorded a verifiability reading for this page (hand-written, or
  // the recorded one no longer applies). The key is NEVER omitted — "cannot tell"
  // must read as cannot-tell, the same vocabulary as stale=unknown.
  "wiki.coverage": "coverage {s}",
  "wiki.coverageUnknown": "coverage unknown",
  "wiki.coverageHint":
    "The build's own anchored-sections / content-sections ratio. unknown = no usable build reading (never built, or the page drifted so the old reading no longer applies); it is neither 0 nor 1.",
  "wiki.freshUnknown": "freshness unknown",
  "wiki.builds": "Build history",
  "wiki.buildPages": "{written}/{planned} pages",
  "wiki.buildFailed": "{n} failed",
  "wiki.action.create": "create",
  "wiki.action.update": "update",
  "wiki.action.delete": "delete",
  "wiki.action.keep": "keep",
  "wiki.compile.scope": "scope",
  "wiki.compile.scope.all": "all sources",
  "wiki.compile.scope.changed": "changed only",
  "wiki.compile.agent": "agent",
  "wiki.compile.agent.auto": "auto (dsh first)",
  "wiki.compile.preview": "Preview plan",
  "wiki.compile.replan": "Re-plan",
  "wiki.compile.confirm": "Confirm build",
  "wiki.compile.planned": "plan ready — {n} pages",
  "wiki.compile.started": "build #{id} started",
  "wiki.compile.gate": "Nothing is written until you confirm; hand-edited pages are skipped.",
  "wiki.view.edit": "Edit page",
  "wiki.err": "Cannot read the wiki",
  "wiki.buildsEmpty": "No builds yet",
  "wiki.buildsEmptyHint": "Compiling the wiki starts with a dry-run plan; pages are written only after you confirm.",
  "wiki.pageCap": "Only the first {n} pages are listed — narrow the scope or prune orphans first",
};
