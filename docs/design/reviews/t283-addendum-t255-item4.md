# t283 附录：t255 报告 ④ 的「改后 HTTP 层」补齐（ui-audit 提出的可证伪条件）

**结论：t255 ④ 的四条预期读数全部成立** ✓（切换后的 exe `4b66312a…`），**并顺带解决了 t283 的 F-t283-02** ✓。
**recall 调用次数 = 0**（本附录不需要 —— 逐腿证据已在 t283 正文里用 1 次 `?source=verify` 验过 ✓，不重复写 recall_log ✓）。

| # | t255 的预期（改后） | 旧 exe（改前实测） | 本轮实测 | 一致 |
|---|---|---|---|---|
| 1 | `memory/list?limit=5` 不带 store ⇒ **200** 且含 `total`/`matched` | **400 missing field store** | **200** ✓ keys=`[counts,limit,matched,memories,namespace,store,total]` · **total=5 · matched=161 · limit=5** · store=null · namespace=null · 5 行 | ✓ |
| 1b | `store=bogus` ⇒ **400 `unknown store …`** | 200 + **静默降级 observation** | **HTTP 400 `unknown store bogus: expected one of profile, observation, procedure, lesson`** ✓ | ✓ |
| 1c | `store=lesson` ⇒ 真实 current 行数 | **200 len=0** | **200 · matched=40** ✓（返回 5 行，limit=5） | ✓ |
| 2 | `recall/log` 每行带 `source`+`source_label`，且响应带 **retention 信封** | 两者都**没有** | **两者都有** ✓：顶层 keys=`[log,retention,source_filter]`；`retention={policy:"keep the newest rows; older rows are pruned on write", max_rows:5000, rows:600, oldest_ts:2026-09-16T13:05:24Z, newest_ts:2026-09-26T14:01:25Z, rows_without_source:593}` ✓；行 keys 含 `source`/`source_label` ✓；最近 6 行 **NULL source = 0** ✓，label 样本 `[probe, ui-chat, verify]` ✓ | ✓ |
| 3 | `knowledge/search` **仍无** legs ⇒ 腿看 `/recall` 的知识命中 | — | **无 legs** ✓（keys=`{chunk_id,document,content,score}`）；腿已在正文用 `/recall`（1 次，`?source=verify`）验到 ✓ | ✓ |

## F-t283-02 的结论（已解决）
`memory/list` 的 **limit 默认值 = 500** ✓：无参 ⇒ `limit=500 · matched=161 · 161 行`；`?limit=5` ⇒ `limit=5 · matched=161 · 5 行` ⇒ 即 **`limit` 是页大小、`matched` 是未分页真实数**（`total` 随页大小走）✓。t279 canary 片段里的 `limit:5` 应是显式传参 ✓ ⇒ **不再是差异** ✓。

## 一处值得记的口径（非缺陷）
`total` 与 `matched` 语义不同：`total` = 本次返回的条数（`limit=5` ⇒ 5；无参 ⇒ 161），`matched` = 过滤后的真实总数（两次都是 161）✓ —— 与 t251 的设计一致 ✓，但**读 `total` 会以为是总数** ✗ ⇒ 建议在端点文档/验收里写明这两个键的差别 ✓。
