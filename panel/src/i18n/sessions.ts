// i18n domain: sessions.*
//
// Copy for this domain only. New strings for a view belong in THAT view domain
// file (see panel/src/i18n/README.md and the header of panel/src/i18n.tsx): the point
// of the split is that two tasks editing two different views never touch one file.
// Keep both languages in step: a key added here must be added to zh AND en.

export const zh: Record<string, string> = {
  "sessions.title": "会话",
  "sessions.subtitle": "所有智能体的对话历史，自动同步（{n} 条）",
  "sessions.all": "全部",
  "sessions.empty": "还没有同步到会话",
  "sessions.emptyHint": "claude code、dsh、ruagent 的会话会自动索引到这里。",
  "sessions.untitled": "未命名会话",
  "sessions.messages": "条",
  "sessions.noMessages": "该会话没有可显示的消息",
  "sessions.distillHint": "蒸馏这条会话 → 记忆 + 图谱",
  "sessions.err": "无法加载会话索引",
  "sessions.err.hint": "daemon 不可达或接口出错。这不等于「还没有会话」。",
  "sessions.stale.hint": "显示的是上一次成功读取的索引，10 秒轮询正在重试。",
  "sessions.filterEmpty": "该来源没有会话——换一个来源，或选「全部」。",
  "sessions.distilled": "蒸馏完成 · 记忆 {m} 写入 / {k} 跳过 · 实体 {e} · 关系 {r} · 智能体 {a}",
  "sessions.msgCap": "只渲染最近 {n} 条消息（更早的未加载）",
  "sessions.archive": "归档",
  "sessions.archiveHint": "归档：只是从 ruagent 的列表里藏起来，不动任何文件，可随时取消",
  "sessions.unarchive": "取消归档",
  "sessions.unarchiveHint": "取消归档：让它重新出现在列表里",
  "sessions.archived": "已归档",
  "sessions.hideArchived": "隐藏已归档",
  "sessions.showArchived": "显示已归档（{n}）",
  "sessions.onlyArchived": "只看已归档",
  "sessions.archivedEntry": "归档（{n}）",
  "sessions.sort": "排序",
  "sessions.sortUpdated": "最近对话",
  "sessions.sortCreated": "最近创建",
  "sessions.archivedTitle": "已归档的会话",
  "sessions.archivedNote": "已归档的会话只在这里出现，不会混回会话列表。",
  "sessions.archivedEmpty": "还没有已归档的会话",
  "sessions.system": "系统会话",
  "sessions.hideSystem": "隐藏系统会话",
  "sessions.showSystem": "显示系统会话（{n}）",
  "sessions.onlySystem": "只看系统会话",
  "sessions.systemName": "（含平台上下文的历史会话）· {d}",
  "sessions.more": "更多操作（{name}）",
  "sessions.archivedToast": "已归档 · 文件未被改动，可随时取消归档",
  "sessions.unarchivedToast": "已取消归档",
  "sessions.deletedToast": "已从索引中删除",
  "sessions.delete": "删除",
  "sessions.deleteHint": "删除：从 ruagent 的索引里移除这条记录（仅限 ruagent 自己的会话）",
  "sessions.deleteConfirm": "删除这条会话？",
  "sessions.deleteConfirmHint": "只从 ruagent 的会话索引里移除，不动任何文件。列表里不会再出现，也无法撤销。",
  "sessions.deleteNotOurs": "这是 {s} 自己的历史文件，ruagent 只做了索引，无权删除。要删除请到 {s} 里操作。",
  "sessions.filter.source": "来源",
  "sessions.filter.workspace": "工作区",
  "sessions.filter.workspaceAll": "全部工作区",
  "sessions.filter.time": "时间",
  "sessions.filter.timeAll": "全部时间",
  "sessions.filter.time7d": "近 7 天",
  "sessions.filter.time30d": "近 30 天",
  "sessions.search": "搜索标题、工作区、预览…",
  "sessions.filteredEmpty": "没有符合筛选条件的会话。",
  "sessions.filterClear": "清除筛选",

  // ---- distillation standing on the list (t8 / #47) ----
  // The daemon decides these from the LEDGER (distill_log.status = 'ok'), so
  // the badge names the ledger's reading, not a guess over the visible rows.
  "sessions.distill.state.ok": "已蒸馏",
  "sessions.distill.state.failed": "蒸馏失败",
  "sessions.distill.state.empty": "无内容",
  "sessions.distill.state.none": "未蒸馏",
  "sessions.distill.state.norecord": "结果未记录",
  "sessions.distill.hint": "账本判定：{state}；最近一次尝试 {when}",
  "sessions.distill.never": "从未尝试",
  "sessions.distill.unsupported": "这个 daemon 不返回蒸馏状态（字段是后来加的），因此无法按它筛选。",
  "sessions.filter.distill": "蒸馏状态",
  "sessions.filter.distillAll": "全部蒸馏状态",
  "sessions.filter.distillNone": "只看未蒸馏",
  "sessions.filter.distillOk": "只看已蒸馏",
  "sessions.batchFailed": "批量蒸馏失败：{e}",

  // ---- the distillation TIER (ruagent-issues-56-59 t6 / issue #57) ----
  // These live in THIS view's domain file because the control they label is in
  // SessionsView.tsx. The batch modal's older copy sits under `distill.batch.*`
  // in i18n/distill.ts; new strings for this view follow the rule at the top of
  // i18n/README.md (one file per view domain) rather than that prefix.
  //
  // Why the strings are this explicit: the tier is the difference between a
  // free pass and one model call PER WINDOW over up to twenty sessions, and the
  // route's own default (an empty body) is the paid one. Every place the tier
  // is chosen shows what it costs, and every place a pass reports back says
  // which tier ran and whether it called the model.
  "sessions.tier.label": "蒸馏档位",
  "sessions.tier.rules": "rules（免费）",
  "sessions.tier.acp": "acp（调用模型）",
  "sessions.tier.hint": "rules＝本地规则抽取，不调用模型（零 token）；acp＝智能体抽取，每个窗口一次模型调用（会计费）。本页的批量蒸馏与单条蒸馏都用这个档位。",
  "sessions.tier.note.rules": "本档不调用模型：零 token、零成本。",
  "sessions.tier.note.acp": "本档会调用模型：每个窗口一次模型调用，按用量计费。",
  "sessions.tier.plan": "这份预估所用档位（daemon 回报）：{x}",
  "sessions.tier.planStale": "预估还在按所选档位重算——重算回来之前不会开始。",
  "sessions.tier.acpBasis": "（acp 档对照）",
  "sessions.tier.windows.rules": "分窗数（非模型调用）",
  "sessions.tier.run": "本次实跑档位（daemon 回报）：{x}",
  "sessions.tier.asked": "本次请求的档位：{x}",
  "sessions.tier.run.calls.rules": "本档不调用模型：0 次模型调用。账本记录的 {n} 个窗口是分窗数（按字节预算切分），不是模型调用。",
  "sessions.tier.run.calls.acp": "本档会调用模型：账本记录了 {n} 个窗口，即 {n} 次模型调用。",
  "sessions.tier.run.mismatch": "⚠ 实跑回报的档位是 {x}，而请求的档位是 {y}——以 daemon 的回报为准。",

  // ---- a REFUSED distillation is its own fact (#62) ----
  // The daemon answers 409 when the write vocabulary (the store x namespace
  // matrix, design 6.2) refused EVERY memory candidate a pass proposed. That is
  // NOT "nothing to distil": the pass ran, measured and proposed; what it
  // proposed cannot land. The copy below keeps those two readings apart on
  // purpose, states nothing was written, and prints the store's writable
  // vocabulary beside the refused pair -- without that list "lesson cannot write
  // user" is not actionable.
  "sessions.refused.title": "蒸馏被拒绝——本次什么都没写",
  "sessions.refused.hint": "这与「没有可蒸馏的内容」是两件事：这一趟确实跑了、也提出了记忆候选，但写入词表（store × namespace 矩阵）把每一条都拒了，所以一条都没落库。ledger 里这行的状态仍是「成功」，原因写在「原因」列。",
  "sessions.refused.pairs": "被拒的 store × namespace",
  "sessions.refused.allowed": "可写：{list}",
  "sessions.refused.allowedEmpty": "（该 store 没有可写的 namespace）",
  "sessions.refused.measure": "本次计数：被拒 {r} · 写入 {w} · 重复跳过 {s}——被拒不是重复，词表拒绝与「已存在」是两件事。",
  "sessions.refused.session": "会话",
  "sessions.refused.toast": "蒸馏被拒绝：{n} 个 store × namespace 被写入词表拒绝，什么都没写。",
  "sessions.refused.batch": "其中 {n} 条被写入词表全数拒绝（状态仍是「成功」，但一条记忆都没写）。",
  "sessions.refused.batchSum": "成功 {ok} + 无内容 {empty} + 失败 {failed} = {sum} = 尝试 {attempted} 条——三态没有变，「被拒」不是第四种状态，而是「成功」里的一个子集标记。",
  "sessions.refused.batchNotReported": "这个 daemon 不报告被拒计数（字段是后来加的），所以这里不写 0。",
  "sessions.refused.result": "被拒 {n}",

  // ---- t22 / #67: the "待重跑" list ----
  // A refused pass is recorded as ok, so its session leaves the undistilled
  // backlog and never comes back by itself. This is the only way back to those
  // sessions, and it is a FILTER over the ledger's refusal column rather than a
  // guess from the session list's own status word.
  "sessions.refused.onlyTag": "只看上次被拒（{n}）",
  "sessions.refused.onlyHint": "筛出「最近一次蒸馏被写入词表拒绝」的会话：它们在账本上仍是「成功」，所以不会自动回到未蒸馏队列——这就是 #67 说的「待重跑」清单。数字是会话数（账本里的被拒「尝试」数可能更多）。",
  "sessions.refused.onlyUnsupported": "这个 daemon 不报告拒绝计数（字段是后来加的），所以没有这个清单——这里显示的是「没有这个读数」而不是「一条都没有」。",
  "sessions.refused.rowTag": "上次被拒",
  "sessions.refused.rowHint": "这个会话最近一次蒸馏被写入词表拒绝，可能什么都没写；用本行的蒸馏按钮按当前档位重跑。",
  "sessions.refused.batchNote": "{n} 个会话的上次蒸馏被写入词表拒绝：账本上仍是「成功」，所以它们不在本批的未蒸馏队列里，不会自动重跑。",
  "sessions.inconsistent.list": "会话响应里没有可用的 sessions 数组——这是自相矛盾的响应（守护进程出错或被中间层改写），所以这里显示的是「读数不可用」而不是「没有会话」。",
};

export const en: Record<string, string> = {
  "sessions.title": "Sessions",
  "sessions.subtitle": "Conversation history from every agent, auto-synced ({n})",
  "sessions.all": "All",
  "sessions.empty": "No sessions synced yet",
  "sessions.emptyHint": "claude code, dsh and ruagent sessions are indexed here automatically.",
  "sessions.untitled": "Untitled session",
  "sessions.messages": "msgs",
  "sessions.noMessages": "No displayable messages in this session",
  "sessions.distillHint": "Distill this session → memories + graph",
  "sessions.err": "Cannot load the session index",
  "sessions.err.hint": "The daemon is unreachable or the endpoint errored. That is not the same as “no sessions yet”.",
  "sessions.stale.hint": "Showing the last successful read; the 10s poll is retrying.",
  "sessions.filterEmpty": "No sessions from this source — pick another, or “All”.",
  "sessions.distilled": "Distilled · memories {m} written / {k} skipped · entities {e} · relations {r} · agent {a}",
  "sessions.msgCap": "Only the newest {n} messages are rendered",
  "sessions.archive": "Archive",
  "sessions.archiveHint": "Archive: hides it from ruagent's list only. No file is touched, and it is reversible",
  "sessions.unarchive": "Unarchive",
  "sessions.unarchiveHint": "Unarchive: put it back in the list",
  "sessions.archived": "Archived",
  "sessions.hideArchived": "Hide archived",
  "sessions.showArchived": "Show archived ({n})",
  "sessions.onlyArchived": "Archived only",
  "sessions.archivedEntry": "Archived ({n})",
  "sessions.sort": "Sort",
  "sessions.sortUpdated": "Recently active",
  "sessions.sortCreated": "Recently created",
  "sessions.archivedTitle": "Archived sessions",
  "sessions.archivedNote": "Archived sessions appear only here — they are never mixed back into the list.",
  "sessions.archivedEmpty": "Nothing is archived yet",
  "sessions.system": "System sessions",
  "sessions.hideSystem": "Hide system sessions",
  "sessions.showSystem": "Show system sessions ({n})",
  "sessions.onlySystem": "System sessions only",
  "sessions.systemName": "(historical session with platform context) · {d}",
  "sessions.more": "More actions ({name})",
  "sessions.archivedToast": "Archived · no file was changed, and you can undo this",
  "sessions.unarchivedToast": "Unarchived",
  "sessions.deletedToast": "Removed from the index",
  "sessions.delete": "Delete",
  "sessions.deleteHint": "Delete: remove this record from ruagent's index (ruagent's own sessions only)",
  "sessions.deleteConfirm": "Delete this session?",
  "sessions.deleteConfirmHint": "Removes it from ruagent's session index and touches no file. It will not come back, and this cannot be undone.",
  "sessions.deleteNotOurs": "This is {s}'s own history file — ruagent only indexes it and has no right to delete it. Delete it in {s} instead.",
  "sessions.filter.source": "Source",
  "sessions.filter.workspace": "Workspace",
  "sessions.filter.workspaceAll": "All workspaces",
  "sessions.filter.time": "Time",
  "sessions.filter.timeAll": "All time",
  "sessions.filter.time7d": "Last 7 days",
  "sessions.filter.time30d": "Last 30 days",
  "sessions.search": "Search title, workspace, preview…",
  "sessions.filteredEmpty": "No sessions match these filters.",
  "sessions.filterClear": "Clear filters",

  // ---- distillation standing on the list (t8 / #47) ----
  "sessions.distill.state.ok": "Distilled",
  "sessions.distill.state.failed": "Distill failed",
  "sessions.distill.state.empty": "No content",
  "sessions.distill.state.none": "Not distilled",
  "sessions.distill.state.norecord": "Outcome not recorded",
  "sessions.distill.hint": "Ledger says: {state}; last attempt {when}",
  "sessions.distill.never": "never attempted",
  "sessions.distill.unsupported": "This daemon does not answer the distillation standing (the field was added later), so it cannot be filtered on.",
  "sessions.filter.distill": "Distillation",
  "sessions.filter.distillAll": "Any distillation state",
  "sessions.filter.distillNone": "Undistilled only",
  "sessions.filter.distillOk": "Distilled only",
  "sessions.batchFailed": "Batch distillation failed: {e}",

  // ---- the distillation TIER (ruagent-issues-56-59 t6 / issue #57) ----
  // Keys live in this view's domain file (see the zh block above). The tier is
  // the difference between a free pass and one model call PER WINDOW over up to
  // twenty sessions, so the choice states its cost and the receipt states which
  // tier ran and whether it called the model.
  "sessions.tier.label": "Distillation tier",
  "sessions.tier.rules": "rules (free)",
  "sessions.tier.acp": "acp (calls the model)",
  "sessions.tier.hint": "rules = local rule extraction, no model call (zero tokens); acp = agent extraction, one model call per window (billed). Both the batch pass and the single-row distill on this page use this tier.",
  "sessions.tier.note.rules": "This tier calls no model: zero tokens, zero cost.",
  "sessions.tier.note.acp": "This tier CALLS the model: one call per window, billed by usage.",
  "sessions.tier.plan": "Tier this estimate was planned with (the daemon's own answer): {x}",
  "sessions.tier.planStale": "The estimate is being recomputed for the selected tier — starting stays disabled until it arrives.",
  "sessions.tier.acpBasis": " (acp-tier basis)",
  "sessions.tier.windows.rules": "windows (not model calls)",
  "sessions.tier.run": "Tier this run used (the daemon's own answer): {x}",
  "sessions.tier.asked": "Tier this request asked for: {x}",
  "sessions.tier.run.calls.rules": "This tier calls no model: 0 model calls. The {n} windows in the ledger are window SPLITS (cut by the byte budget), not model calls.",
  "sessions.tier.run.calls.acp": "This tier CALLS the model: the ledger recorded {n} windows, i.e. {n} model calls.",
  "sessions.tier.run.mismatch": "⚠ the run reported tier {x} while the request asked for {y} — the daemon's answer is authoritative.",

  // ---- a REFUSED distillation is its own fact (#62) ----
  // The daemon answers 409 when the write vocabulary refused EVERY candidate a
  // pass proposed. That is not "nothing to distil" -- the pass ran, measured and
  // proposed -- so the copy keeps the two apart, says nothing was written, and
  // prints the store's writable vocabulary beside each refused pair.
  "sessions.refused.title": "Distillation refused — nothing was written",
  "sessions.refused.hint": "This is not ‘nothing to distil’: the pass did run and did propose memory candidates, but the write vocabulary (the store x namespace matrix) refused every one of them, so nothing landed. The ledger row still reads ‘ok’, with the reason in the Reason column.",
  "sessions.refused.pairs": "refused store x namespace",
  "sessions.refused.allowed": "accepts: {list}",
  "sessions.refused.allowedEmpty": "(this store has no writable namespace)",
  "sessions.refused.measure": "This pass: {r} refused · {w} written · {s} skipped as duplicates — a refusal is NOT a duplicate, and the vocabulary rejecting a pair is a different fact from the row already existing.",
  "sessions.refused.session": "session",
  "sessions.refused.toast": "Distillation refused: {n} store x namespace pair(s) rejected by the write vocabulary, nothing was written.",
  "sessions.refused.batch": "{n} of them were refused by the write vocabulary outright (their status is still ‘ok’, but not one memory was written).",
  "sessions.refused.batchSum": "ok {ok} + empty {empty} + failed {failed} = {sum} = {attempted} attempted — the three states are unchanged: ‘refused’ is not a fourth one, it is a subset marker inside ‘ok’.",
  "sessions.refused.batchNotReported": "This daemon does not report a refused count (the field was added later), so this line does not claim 0.",
  "sessions.refused.result": "refused {n}",

  // ---- t22 / #67: the refused-last-time list ----
  "sessions.refused.onlyTag": "Refused last time only ({n})",
  "sessions.refused.onlyHint": "Selects the sessions whose MOST RECENT distillation was refused by the write vocabulary: they still read ok in the ledger, so they never return to the undistilled queue -- this is the refused-and-waiting-for-a-re-run list. The number counts SESSIONS (the ledger's refused ATTEMPT count can be higher).",
  "sessions.refused.onlyUnsupported": "This daemon does not report refusal counts (the column was added later), so there is no such list here -- that is a missing reading, not an empty one.",
  "sessions.refused.rowTag": "refused last",
  "sessions.refused.rowHint": "This session's most recent distillation was refused by the write vocabulary and may have written nothing; use the row's distill button to re-run it at the current tier.",
  "sessions.refused.batchNote": "{n} session(s) were refused by the write vocabulary last time: they still read ok in the ledger, so they are NOT in this batch's undistilled queue and will not be retried automatically.",
  "sessions.inconsistent.list": "The sessions response carries no usable sessions array -- a self-contradictory response (a daemon that failed halfway, or a middle layer that rewrote it), so this is an unusable reading rather than an empty list.",
};
