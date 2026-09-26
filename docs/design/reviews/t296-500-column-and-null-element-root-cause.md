# t296 补跑 500 列 + null 元素分裂的根因 + F-t288-01 判定

**结论：pass** ✓ —— ① **500 列 6 格全部取得**（都进错误态 ✓）· ② **F-t288-02 不是产品缺陷，是我 t288/t285 探针的 key 错配** ✓（可证伪 ✓）· ③ **F-t288-01 是真缺陷**（低严重度的硬化缺口 ✓），修法建议见 ⑤。

## 产物证明（先于读数）
- 服务中 chunk = **assets/index-BEfa_XOT.js**（105,313 B）—— **又换新了**（t288 时是 `index-CO4JFzrW`）⇒ 期间有人重建过面板 ✓
- 产物内标记：**arrayOfTuples ×4 · arrayOfStrings ×4 · arrayOfObjects ×8 · malformed response ×1** ✓ ⇒ **逐端点 spec 在产物里** ✓
- 本单探针**不再用共享 keyed map**（那正是 t288 的 bug ✓），每个源用**显式分支**构造 payload ✓；每格打印 `hit`（路由命中次数）⇒ **全部 hit=1** ✓

## ① 500 列 6 格（上一轮被吃掉的原因）
| 源 | hit | main | h1 | err | empty | pageerror |
|---|---|---|---|---|---|---|
| memory | 1 | 1730 | 1 | **3** | 0 | null |
| knowledge | 1 | 103 | 1 | **3** | 0 | null |
| wiki-links | 1 | 100 | 1 | **3** | 0 | null |
| wiki-pages | 1 | 100 | 1 | **3** | 0 | null |
| entities | 1 | 94 | 1 | **3** | 0 | null |
| edges | 1 | 94 | 1 | **3** | 0 | null |
⇒ **6/6 进错误态** ✓。**上一轮为什么没有** ✗：t288 的探针里 `M.http500 = () => null` ✓ 而 `load()` 有「无 body 就跳过」的分支 ⇒ **这 6 格根本没跑** ✓（不是跑出来空白 ✓）—— 探针缺陷 ✓，与产品无关 ✓。

## ② F-t288-02 的根因：**探针 key 错配，不是产品行为** ✓
**用正确 payload 重测**（`{memories:[null],counts:[]}` · `{documents:[null]}` · `{nodes:[null],…}` · `{pages:[null]}` · `{entities:[null]}` · `{edges:[null],…}`）⇒ **六个源全部 err=3（REJECT）** ✓✓
⇒ **`null` 元素在每个端点都被拒** ✓ ⇒ **t288 报的「分裂」不存在** ✓。

**t288/t285 的 bug** ✓：`elemNull` 用的是**共享 keyed map**，键写成端点字段名（`memories/documents/pages/nodes/…`）而**索引用的是源名**（`memory/knowledge/wiki-links/wiki-pages/…`）⇒ 查表得 **`undefined`** ⇒ `JSON.stringify(undefined)` = `undefined` ⇒ fulfill 拿到空 body ⇒ **该格实际没有拦截** ⇒ 真实响应到达 ⇒ 页面**正常渲染**（字符数等于基线 ✓）⇒ 被读成「静默放过」✗。
⇒ 这也解释了与 **t268/t287 读数不一致**的原因 ✓：它们发的是**正确 payload**，测到的是**真的被拒** ✓ —— **它们是对的，我是错的** ✓。

## ③ 元素 kind 指纹（可证伪的谓词形状）
| 源 | null | 0 | "str" | {} 对象 | [] 数组 | 与声明的 spec |
|---|---|---|---|---|---|---|
| memory | REJECT | REJECT | REJECT | **BLANK(0)** | REJECT | arrayOfObjects ✓（对象通过，但视图崩 —— 见 ④） |
| knowledge | REJECT | REJECT | REJECT | PASS(95) | REJECT | arrayOfObjects ✓ |
| wiki-links | REJECT | REJECT | **PASS(567)** | REJECT | REJECT | **arrayOfStrings ✓**（只有字符串通过 ✓） |
| wiki-pages | REJECT | REJECT | REJECT | **BLANK(0)** | REJECT | arrayOfObjects ✓ |
| entities | REJECT | REJECT | REJECT | REJECT | **PASS(206)** | **arrayOfTuples ✓**（只有数组通过 ✓） |
| edges | REJECT | REJECT | REJECT | PASS(209) | REJECT | arrayOfObjects ✓ |
⇒ **没有 `typeof null === 'object'` 的洞** ✓（null 全被拒 ✓）· **每端点的通过集与它声明的 spec 完全一致** ✓ ⇒ 逐端点 spec 的**判据层**是正确的 ✓。

## ④ F-t288-01 判定：**真缺陷（低严重度的硬化缺口）** ✗
用正确 payload 复核 ✓（hit=1 ✓）：
| 格 | 读数 | 判定 |
|---|---|---|
| `wiki/pages: [{slug:42,title:null}]` | **0 / h1 0 / err 0 / empty 0 + PE** | ✗ **整页空白** |
| `memory: [{id:1,name:'x',kind:'tool',summary:null}]`（即 ③ 的 `obj` 格） | **0 / h1 0 / err 0 / empty 0** | ✗ **整页空白** |
| `knowledge: [{id:'x',name:7}]` | 95 / 1 / err 0 / empty 0 | ✗ **静默降级** |
| `edges: [{id:'x',src:'a',dst:'b'}]` | 209 / 1 / err 0 / empty 0 | ✗ **静默** |
**判为缺陷的理由** ✓：**「200 但不合契约 ⇒ 不得空白」是本队已确立的判据**（t264/t267/t285 一路沿用它 ✓），而这里**两个页真的空白** ✗ ⇒ 落在判据内 ✓。**但严重度低** ✓：该 payload **深度不合契约**（`slug:42` / 把图谱实体塞进记忆行 ✓），**没有已知的真实后端路径会产出它** ✓。
**修法建议** ✓（两条，都不必新增 UI ✓）：**(a) 对已知行对象做最小字段断言**（如 `pages` 行的 `slug`/`title` 为字符串 ✓，`memories` 行的 `content` 为字符串 ✓）⇒ 走同一条 catch ✓；**(b) 或让视图对 `undefined` 兜底**（`memory` 与 `wiki-pages` 现在会崩 ✓ 而 `knowledge`/`edges` 不会 ✓ ⇒ 说明**崩的是视图的取字段方式**，不是守卫 ✓）⇒ (b) 更便宜、且能一并覆盖「对象内部任何未声明字段」这一类 ✓。
**若判为边界**（我不建议 ✓）则需要这条判据：「视图不得在**对象内部字段**上崩」—— 而这正是它现在违反的 ✓。

## ⑤ findings
1. **F-t296-01（medium，产品）**：守卫只检查元素 kind、不检查对象内部字段 ⇒ `wiki/pages` 与 `memory` 在「元素是对象但字段类型错」时**整页空白** ✗（`knowledge`/`edges` 只静默降级 ✓）。修法见 ④。
2. **F-t296-02（low，方法）**：**t285/t288 的 `elemNull` 探针 key 错配** ⇒ 那一行读数作废 ✓ ⇒ 我在两单里据此报的「`null` 元素静默」是**我自己的错** ✗，t268/t287 的读数是对的 ✓（本条为更正记录 ✓）。
3. **F-t296-03（low，产品）**：`knowledge` 与 `edges` 在元素字段类型错时**静默**（95/209 字符、无错误、无空态 ✓）⇒ 同族但更轻 ✓，建议与 F-t296-01 一并处理 ✓。
