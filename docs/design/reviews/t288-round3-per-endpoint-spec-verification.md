# t288 独立验证 t287：逐端点 spec（round 3）

**结论：needs_revision** —— 逐端点 spec **修好了 t285 点名的三格** ✓、**反向判据 6/6 成立** ✓（真实响应逐字回放 = 不拦截，结构签名逐项相等 ✓），
**但我自造的一格找到了新的空白** ✗：`wiki/pages: [{slug:42,title:null}]`（**对象内部字段类型错**）⇒ **整页空白** ✗；
另有两处**读数与 t268/t287 不一致**，如实登记（见 ④）。**本轮 500 那一列未取得**（原因见 ⑤：探针跳过 + 维护窗口）。

## 被测产物（先证明测的是修好的产物）
- 服务中 chunk = **assets/index-CO4JFzrW.js**（105,061 B）
- 产物内标记计数：**arrayOfTuples ×4 · arrayOfStrings ×4 · arrayOfObjects ×8 · malformed response ×1** ⇒ **逐端点 spec 确实在产物里** ✓
- 六端点真实响应（逐字捕获，长度）：memory/list 5013 · documents 566 · wiki/links 115 · wiki/pages 374 · graph/entities 7517 · graph/edges 12944

## ① 24 格矩阵 ⇒ 18/24 取得，BLANK = 0/18 ✓（**500 那一列 6 格未取得** ✗，见 ⑤）
| 源 | 缺字段 | 错形状 | 空数组 | 500 |
|---|---|---|---|---|
| memory | 1714/1/**err3** ✓ | 1714/1/3 ✓ | 1709/1/0/**empty1** ✓ | 未取得 ✗ |
| knowledge | 103/1/3 ✓ | 103/1/3 ✓ | 136/1/0/**1** ✓ | 未取得 ✗ |
| wiki-links | 100/1/3 ✓ | 100/1/3 ✓ | 388/1/0/0（自绘 ✓） | 未取得 ✗ |
| wiki-pages | 100/1/3 ✓ | 100/1/3 ✓ | 441/1/0/**1** ✓ | 未取得 ✗ |
| entities | 94/1/3 ✓ | 94/1/3 ✓ | 80/1/0/**1** ✓ | 未取得 ✗ |
| edges | 94/1/3 ✓ | 94/1/3 ✓ | 209/1/0/0 ✓ | 未取得 ✗ |

## ② 自造三格（t285 点名的）⇒ 全部进错误态 ✓
| 格 | t285（改前） | 本轮 |
|---|---|---|
| wiki/pages × `[[{}]]` | 0/0/0/0 + PE `reading 'join'` ✗ | **100 / 1 / err 3 / 0** ✓ |
| knowledge × `[[{id:1}]]` | 93/1/0/0 静默 ✗ | **103 / 1 / err 3 / 0** ✓ |
| entities × 混对象与非对象 | 94/1/err3 ✓ | **94 / 1 / err 3 / 0** ✓ |

## ③ 反向判据（最强形式）⇒ **6/6 结构签名逐项相等** ✓
| 源 | 不拦截（h1/err/empty/sider/main） | 逐字回放真实响应 | 字符数（只作读数） |
|---|---|---|---|
| memory | 1/0/0/true/true | 1/0/0/true/true ✓ | 3904 vs 3904 |
| knowledge | 1/0/0/true/true | 同 ✓ | 240 vs 240 |
| wiki-links | 1/0/0/true/true | 同 ✓ | 444 vs 444 |
| wiki-pages | 1/0/0/true/true | 同 ✓ | 444 vs 444 |
| **entities** | 1/0/0/true/true · **canvas=true · aria="力导向实体图：65 个实体、46 条关系"** | 同 ✓ | 188 vs 188 |
| edges | 1/0/0/true/true | 同 ✓ | 188 vs 188 |
⇒ **误杀 0 处** ✓（`wiki/links` 的**字符串数组 nodes** ✓ 与 `graph/entities` 的**元组** ✓ 都被接受 ✓）；**#graph 只断言 DOM**（canvas 存在 + aria-label 数字 ✓），**未比对像素** ✓。**回放确实生效**的对照证据 = 变异格会改变页面 ✓（例如 memory 缺字段 3904→1714 ✓）。

## ④ 我自造的新变异 + 两处不一致（如实登记）
| 变异 | 读数 | 判定 |
|---|---|---|
| **wiki/pages × `[{slug:42,title:null}]`（对象内部字段类型错）** | **0 / h1 0 / err 0 / empty 0** + PE `reading …` | ✗ **仍整页空白**（守卫只看元素 kind，不看对象内部） |
| knowledge × `{documents:null}`（顶层字段 null 而非缺失） | 103/1/**err3** | ✓ 被拒 |
| memory × `{memories:[],counts:null}`（同上） | 1714/1/**err3** | ✓ 被拒 |
| **memory × `{memories:[null],counts:[]}`** | **3904/1/0/0（plain，与基线同）** | ✗ **静默**：`null` 元素未触发 arrayOfObjects 拒绝 |
| **knowledge × `{documents:[null]}`** | **240/1/0/0（plain）** | ✗ **静默** |
| **wiki-links × `{nodes:[null],…}`** | **444/1/0/0（plain）** | ✗ **静默** |
| **wiki-pages × `{pages:[null]}`** | **444/1/0/0（plain）** | ✗ **静默** |
| entities × `{entities:[null]}` | 94/1/**err3** ✓ | ✓ 被拒 |
| edges × `{edges:[null]}` | 94/1/**err3** ✓ | ✓ 被拒 |
⇒ **与 t268/t287 的读数不一致** ✗：它们报「knowledge/pages/entities × 元素 null ⇒ 进错误态」✓，而我实测 **entities/edges 被拒、memory/knowledge/wiki-links/wiki-pages 未被拒** ✗。**我不猜原因** ✓（可能是构建漂移、或两轮探针的 payload 形状不同 ✓）⇒ 列为待查 ✓。

## ⑤ 未取得的部分（不猜）
- **500 那一列 6 格** ✗：我的探针把 500 的 body 写成 `null` ⇒ 被「无 body 就跳过」的逻辑吃掉 ⇒ **这 6 格本轮没跑** ✓（探针缺陷，不是产品缺陷 ✓）。
- **维护窗口**：本轮读数全部取自**同一个 daemon 实例**（一次跑完 ✓）；期间收到 captain 广播（t279 重建+切换 8787 ✓）⇒ **我按广播停止继续取 HTTP 读数** ✓ ⇒ 补跑 500 列须在「切换完成」之后 ✓，且届时**产物可能已变**（需重新做 ⑥ 的产物证明 ✓）。

## ⑥ findings
1. **F-t288-01（medium）**：守卫只检查**元素的 kind**，不检查**对象内部字段类型** ⇒ `pages:[{slug:42,title:null}]` **整页空白** ✗。修法：对已知行对象做最小字段断言（至少 slug/title 为字符串），或让视图对 `undefined` 做兜底。
2. **F-t288-02（medium）**：`null` 元素在 memory/knowledge/wiki-links/wiki-pages 上**未触发拒绝**（页面正常渲染 ✗），而 entities/edges 上**被拒** ✗ —— 同一 spec 形状两种行为 ⇒ 需查清 arrayOfObjects 对 `null` 的实际判定（是否被 `typeof null === 'object'` 放过）。
3. **F-t288-03（low）**：本轮 **500 列未取得**（探针缺陷）⇒ 需补跑。
4. **F-t288-04（low）**：与 t268/t287 的「元素 null ⇒ 错误态」读数不一致 ⇒ 待查（构建漂移或 payload 形状差异）。
