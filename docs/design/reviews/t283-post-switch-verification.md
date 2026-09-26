# t283 切换后的独立复核（t279 的 HTTP 层 + PRAGMA + chunk + exe）

**结论：pass** ✓ —— t279 声称的每一条**都独立复现** ✓，且 **PRAGMA 里能看到「声明来源」机制在队里被真正用起来**（历史行全 NULL，切换后写入的行带 source ✓）。
两条低严重度的差异/措辞问题如实登记（F-t283-01 / -02）。

**/api/v1/recall 调用次数 = 1**（按裁决声明 `?source=verify`，读回 `source:"verify"` ✓，见 ② 的 PRAGMA 行 598）。

## ① GET 组（切换后活体 8787）与 t279 报告逐条对照
| 项 | t279 声称 | 我的实测 | 一致 |
|---|---|---|---|
| memory/list **不带 store** | 200 + total（canary：`total:0,matched:0,limit:5`） | **200** ✓，keys = `[counts,limit,matched,memories,namespace,store,total]`，**total=165 · matched=165 · limit=500** · store=null · namespace=null · counts=14 组 | ✓（limit 数值差异见 F-t283-02） |
| recall/log 每行带 source/source_label | 是（历史行 `unknown (pre-0018)`） | **是** ✓：首行 `"source":"switch-verify","source_label":"switch-verify"` ✓ | ✓ |
| knowledge/search 的 hit 带逐腿字段 | （它自己⑦已如实标注：**没有**） | **没有** ✗：keys = `{chunk_id,document,content,score}` ⇒ 见 F-t283-01 | ✓（与它的标注一致） |
| **recall 响应的知识命中**带逐腿字段 | （它说腿在这里） | **是** ✓：hit keys = `{chunk_id,content,document,excerpt,keyword_rank,keyword_score,kind,legs,score,score_kind,semantic_rank,semantic_score}`；样本 `legs:["semantic","keyword"] · score_kind:"rrf_rank" · semantic_score:0.3671 · keyword_score:-1.6779` ✓；`memory_legs={semantic:2,keyword:0,keyword_new:0,returned:2,dropped_by_top_n:0,top_semantic_score:0.8464}` ✓ | ✓ |
| DELETE graph/entity/999999 | 404 entity not found | **404 `entity not found`** ✓ | ✓ |
| DELETE memory/999999?purge=true | 404 memory not found | **404 `memory not found`** ✓ | ✓ |

## ② PRAGMA（python 内置 sqlite3，mode=ro）
- **`memories.deleted_at` 存在** ✓ · **`recall_log.source` 存在** ✓ · **`schema_migrations` 最新 = 18** ✓（与 t279 一致）
- **该列出现之后写入的行是否带 source** ✓：`recall_log` 598 行 · **带 source = 5** · NULL = **593**（全部历史行，最后一条 NULL 的 ts = 13:43:23Z，早于切换 ~13:54Z ✓）· 取值分布 `[[null,593],["probe",2],["verify",1],["ui-chat",1],["switch-verify",1]]` · 末尾三行 `598 verify / 597 ui-chat / 596 probe` ✓
- **NULL 不是 user** ✓：`unknown (pre-0018)` 是**历史行的标记**，与「谁调的」无关 ✓；切换后每一行都带声明来源 ✓
- `memories.deleted_at` 非空行数 = **0** ✓（本轮无软删残留）

## ③ chunk 对照
- 我观察到的序列：`index-Bhzy9OHC.js`（14:44 广播那轮）→ `index-flNMPVYn.js`（t285/t288 两轮）→ **`index-CO4JFzrW.js`（现服务 ✓，dist mtime Sep 26 21:42，116,415 B）**
- **切换没有重建面板** ✓ ⇒ 切换前后**都是 `index-CO4JFzrW.js`** ✓（与 t279 ⑥ 一致 ✓）
- **对应提交** ✓：最后一个动 `panel/src/` 的提交 = **`e8cb2f7`（t287：spec 逐端点）** ✓；`git status --short panel/src panel/dist` **无输出** ⇒ 无未提交改动 ⇒ **该 chunk = `e8cb2f7` 那棵树构建的** ✓（`panel/dist` 未被 git 跟踪，产物本身不在版本控制里 ✓）

## ④ exe
| | 切换前 | 切换后 |
|---|---|---|
| md5 | `3763f910c4e18085201cf77db577bada` | **`4b66312ad4a689f386fe576aaf9c66d7`** ✓ 不同 |
| 大小/时间 | 321,812,480 B / Sep 26 14:44 | 322,337,792 B / **Sep 26 21:54** ✓ |

## ⑤ findings
1. **F-t283-01（low）**：验收措辞「knowledge/search 的 hit 带逐腿字段」**对该端点不成立** ✗（keys = `{chunk_id,document,content,score}`）—— 逐腿字段在 **`/api/v1/recall` 的知识命中**上 ✓，而面板 #knowledge 页走的正是窄端点 ⇒ 那一页今天看不到腿 ✓（t279 已如实标注并立单 ✓）。修法：把该条验收措辞改成「recall 响应的知识命中带逐腿字段」，或在窄端点补腿。
2. **F-t283-02（low）**：`memory/list` 的 **limit 默认值**在 t279 的 canary 片段里是 **5**、在我活体无参调用里是 **500** ✗ ⇒ 请确认默认值（两种情况下「200 + total」这条判据都成立 ✓，故仅记账）。
3. **F-t283-03（low，信息）**：`knowledge/search` 的 `score` 是 **RRF 名次分**（样本 `0.032786883413791656` = 2/61 ✓），与余弦/bm25 不同量纲 ⇒ 与 t247 已登记的同一族问题 ✓（此处不重复立案）。
