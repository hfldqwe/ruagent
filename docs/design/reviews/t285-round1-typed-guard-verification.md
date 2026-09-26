# t285 独立验证 t268：类型性守卫（round 1）

**结论：needs_revision** —— t268 的修法在它声称的两件事上都成立 ✓：
① 我复跑 **24 格 = BLANK 0/24** ✓；② 它补的五格（counts 非数组 · memories 非数组 · knowledge/pages/entities 元素 null · edges total 字符串）全部进错误态 ✓；
③ 反向判据（防误杀）也成立 ✓：合法空数组仍走空态、图谱 [entity,count] 元组仍被接受 ✓。
**但**：它把「元素可以是数组」这个图谱元组豁免做成了**全端点通用** ✗ ⇒ 我自造的一格 **wiki/pages × 元素为嵌套数组 ⇒ 仍整页空白** ✗（Cannot read properties of undefined (reading 'join')），另一格（knowledge × 嵌套数组）**静默** ✗。

## 环境与被测对象
- 服务中的 chunk 已换新：**assets/index-flNMPVYn.js**（116,115 B），内含 malformed response ✓ 与 **Array.isArray** ✓ ⇒ t268 的守卫确实在产物里 ✓。
- 真实端点形状（只读 GET）：memory/list → {memories:[obj],counts:[…]} · knowledge/documents → {documents:[{id,name,source,chunk_count,created_at}]} · wiki/links → {nodes:[str],edges:[],broken:[str],orphans:[]} · wiki/pages → {pages:[{slug,title,summary}]} · **graph/entities → {entities:[[entity,count],…]}（元组）** · graph/edges → {edges:[obj]}。
- 探针：**我自己的**（node 直调 @playwright/test，无 npx ✓），每格新开 page ✓，page.route('**'+endpoint+'*')（带尾 * ✓）。**未复用 t268 的脚本与读数** ✓。

## ① 24 格矩阵（6 源 × 缺字段/错形状/空数组/500）⇒ BLANK = 0/24 ✓
| 源 | 缺字段 | 错形状 | 空数组 | 500 |
|---|---|---|---|---|
| memory | 1710/1/err3 ✓ | 1710/1/3 ✓ | 1705/1/0/empty1 ✓ | 1710/1/3 ✓ |
| knowledge | 103/1/3 ✓ | 103/1/3 ✓ | 136/1 ✓ | 103/1/3 ✓ |
| wiki-links | 100/1/3 ✓ | 100/1/3 ✓ | 388（自绘空态 ✓） | 100/1/3 ✓ |
| wiki-pages | 100/1/3 ✓ | 100/1/3 ✓ | 441/1 ✓ | 100/1/3 ✓ |
| entities | 94/1/3 ✓ | 94/1/3 ✓ | 80/1 ✓ | 94/1/3 ✓ |
| edges | 94/1/3 ✓ | 94/1/3 ✓ | 209 ✓ | 94/1/3 ✓ |
（memory 字符数 1710 vs t267 的 146 = 构建漂移 ✓（t253 后该页内容变多），判据看 err/empty/blank ✓。）

## ② 被点名的五格 ⇒ 全部进错误态 ✓
memory × counts 非数组 err3 ✓ · knowledge × 元素 null err3 ✓ · wiki/pages × 元素 null err3 ✓ · entities × 元素 null err3 ✓ · edges × total:"54" err3 ✓。

## ③ 我自造的新变异（两者都没测过）
| 变异 | 读数 | 判定 |
|---|---|---|
| entities: [[entity,count], "not-a-tuple"]（混对象与非对象） | 94/1/err3 | ✓ 进错误态 |
| **pages: [[{}]]（元素为嵌套数组）** | **0 / h1 0 / err 0 / empty 0** + PE Cannot read properties of undefined (reading 'join') | ✗ **仍整页空白** |
| documents: [[{id:1}]]（元素为嵌套数组） | 93/1/0/0 | ✗ **静默** |

## ④ 反向判据（防误杀）⇒ 两条都成立 ✓
- **合法空数组仍走空态** ✓：memory 1705/empty1 · knowledge 136/1 · wiki-pages 441/1 · entities 80/1 ✓（wiki-links 与 edges 无空态元素属既有形状 ✓，非错误 ✓）。
- **图谱 [entity,count] 元组仍被接受** ✓：entities: [[{id:1,name:'x',kind:'tool',summary:null}, 3]] ⇒ **206 / h1 1 / err 0 / empty 0** ✓（刻意豁免生效 ✓）。

## ⑤ findings
1. **F-t285-01（medium）**：元组豁免**全端点通用** ⇒ wiki/pages 元素为嵌套数组时**仍整页空白**（(reading 'join')），knowledge/documents 同形时**静默**。修法：spec 做成**逐端点**（只有 graph/entities 用 arrayOfTuples，其余用 arrayOfObjects）。
2. **F-t285-02（low）**：我 t267 把那一格命名为「memory × 空数组」✗ —— t268 指出实际 payload 是 {memories:[],counts:{}}（counts 不是数组）✓ 属实；我的「正确形状」当时写错 ✓。更正记录，非产品缺陷。
3. **F-t285-03（low）**：wiki/links 的 nodes 是**字符串数组** ⇒ 若哪天声明成 arrayOfObjects 会**误杀正确响应** ✗；建议在 spec 旁注明「nodes = 字符串数组」这一实测形状。
