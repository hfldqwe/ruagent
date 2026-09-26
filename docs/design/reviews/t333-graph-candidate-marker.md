# t333 — 图搜索回落 loose 后，「候选」必须看得见

- 产物：`panel/dist` 服务中入口 chunk `assets/index-CF2TOUf5.js`（本轮 `npm run build` 后）
- 改动：`panel/src/api.ts`（graphSearch 返回整个信封而不是只取 entities）· `panel/src/views/Graph.tsx`（新状态 searchMatch + 查询级标记）· `panel/src/i18n/graph.ts`（zh/en 两条键）
- commit：`3284d6f0658903ddac035a5098212c958d038964`

## 为什么在这里修
t320 的回落把响应标成 `match ∈ exact | candidate | none`（三值：只有两值时「回落了但没找到」会被读成「有候选」）。
而面板的 `graphSearch` 只取 `r.entities` ⇒ 放宽这件事在界面上不存在 ⇒ **候选与精确命中长得一模一样**，用户会把放宽结果当成精确命中。

## 读数（1440×900 · dark · 同一视口）
探针先断言自己的输入：`GET /api/v1/graph/search` 的 `match` 实测
`{"autohotkey-v2":"candidate","AutoHotkey":"exact"}`（否则「标记缺席」会被误读成「没有候选」）。

对两组结果区取【元素多重集（tag+class 序列）】做差（不是只比文本，只比文本会放过「标记存在但不可见」）：

| 组 | 独有元素种类 |
|---|---|
| candidate（q=autohotkey-v2） | `["div.search-match-candidate"]` |
| exact（q=AutoHotkey） | `[]`（该标记**不出现** ⇒ 标记确实区分两种值） |

candidate 组独有的逐字 outerHTML：

```html
<div class="search-match-candidate" role="status" title="精确匹配没有结果，后端放宽了查询——这些是候选，不是精确命中。">候选：放宽匹配（非精确命中）</div>
```

可见性：`getBoundingClientRect` = **182×42 px** · `display: block` · `visibility: visible` · `opacity: 1` ⇒ 不是隐藏节点。

## 设计选择：查询级，不是命中级
`match` 是**整条响应**的属性（与 t316 的 `query_keyword_stage` 同类）⇒ 标记只渲染一次、在结果之上，
**不挂在单条结果旁边**（那会把整个查询的放宽归因到某一条实体上）。

## 门禁对账（#graph，改前 → 改后）
```
改前： pass 53 · fail 0 · not_measured 6（行 15 16 25 28 46 49）
改后： pass 53 · fail 0 · not_measured 6（行 15 16 25 28 46 49）
```
⇒ 逐行相同、逐段相同：本单**没有让任何判据变绿**，所以不存在「原来那一格没人接手」的情况（7.98）。

## 反向与回归
- exact 组不出现该标记（见上表）⇒ 标记不退化。
- `npm run build` 通过（i18n 闸门 VERDICT: PASS）· e2e **38 passed / 3 skipped**，无新增失败。
- i18n：中文由运行期 DOM 读数确认（上表 outerHTML 即其文本）；英文键 `graph.match.candidate` 已随产物发布（`Candidate: relaxed match (not an exact hit)`，在 `index-CF2TOUf5.js` 内实测到），走同一个 `t()` 调用。
