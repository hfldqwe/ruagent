# t299 复验 t298：指纹表 + 真实响应回放 + 断言面三值审查

**结论：pass** —— 目标修复成立（memory 与 wiki-pages 的 {} 格不再 BLANK）、真实响应回放零假红 6/6、null 元素六端点全拒。
两条如实登记的偏离（均已在 t298 披露）+ 一条**我的前置线索被证伪**（自我更正）。

## 产物证明（先于读数）
chunk = **assets/index-DQtMkr-V.js**（107,038 B）· 标记计数 **arrayOfTuples x4 · arrayOfStrings x4 · arrayOfObjects x9**（t296 时 8 ⇒ 新增 spec）· **present x5**（新增 kind）· malformed response x1 ⇒ **t298 的修复在产物里**。

## ① 指纹表（null / 0 / "str" / {} / [] x 六端点）
| 源 | null | 0 | "str" | {} | [] | 与 t296 基线 |
|---|---|---|---|---|---|---|
| memory | REJECT | REJECT | REJECT | **REJECT(1690)** | REJECT | {} 由 **BLANK(0)** → REJECT，目标达成 |
| knowledge | REJECT | REJECT | REJECT | **REJECT(103)** | REJECT | {} 由 PASS(95 静默) → REJECT（t298 偏离①，已披露） |
| wiki-links | REJECT | REJECT | **PASS(610)** | REJECT | REJECT | **一格不变** |
| wiki-pages | REJECT | REJECT | REJECT | **REJECT(100)** | REJECT | {} 由 **BLANK(0)** → REJECT，目标达成 |
| entities | REJECT | REJECT | REJECT | REJECT | **REJECT(94)** | [] 由 PASS(206) → REJECT（t298 偏离②：元组收紧，且借此发现并修掉一个新缺陷） |
| edges | REJECT | REJECT | REJECT | **REJECT(94)** | REJECT | {} 由 PASS(209) → REJECT（新增行字段断言的必然结果） |

**「只有 {} 那一格变、其余一格不动」这条形状【没有字面成立】**：另有 4 格通过集变化，每一处都可解释且方向正确 —— 根因见 F-t299-01。

## ② 真实响应逐字回放 ⇒ 6/6 结构签名逐项相等、零假红
memory 1/0/0/true/true（3790）· knowledge 同（366）· wiki-links 同（687）· wiki-pages 同（687）· **entities 同（188，canvas=true，aria="力导向实体图：63 个实体、46 条关系"）** · edges 同（188）。
⇒ wiki/links 的字符串数组与 graph/entities 的真实元组**都被接受**（后者证明元组收紧没误杀真响应）。null 元素六端点全 REJECT（hit=1）。

## ③ 断言面三值审查
**不更严**：memory id(number) · supersedes(present，null 合法且语料见过) · wiki/links nodes+broken(arrayOfStrings) · edges+orphans(array，**样本为空故不声明元素**) · entities arrayOfTuples(50 个长度 2) · edges id/src/dst(number) · relation/fact_text(string) · invalid_at(present，60 行全 null) · total/limit/offset(number)。
**不可判定（语料无 NULL 样本，第三态保留）**：memory 行 store/namespace · documents created_at · wiki/pages slug —— 共 **4 项**。
**我的前置线索被证伪（自我更正）**：我切换前说「若 spec 断言**顶层** store/namespace 会更严」—— 读到字面量后：spec 只有 {memories:"arrayOfObjects", counts:"array"} 加**行字段**，**顶层根本不在 spec 里** ⇒ 那条线索不成立（我把 MemoryListQuery 的 Option<String> 误当成行字段来源）。
**「哪个字段是元凶」有答案**：t298 **只断言 slug**（视图真正做字符串操作的那个），title/summary 不断言 ⇒ 我 t296 payload 里的 **title:null 不是元凶**，**slug:42 才是**。

## ④ Rust 侧 ground truth（只读 crates/daemon/src/api.rs）
MemoryListQuery{store:Option<String>, namespace:Option<String>, limit:Option<u32>}（**查询**结构）· MemoryWriteRequest.supersedes:Option<i64> · CreateEntityRequest.kind/summary:Option<String> —— 三者都是**请求**结构 ⇒ 解释了顶层回显可为 null 与 supersedes 的 null 合法性，但**不能**证明响应行结构。

## ⑤ findings
**F-t299-01（low，方法）**：回归面形状字面不成立 —— 指纹表的非 {} 格用的是**通用元素值**（{id:1,name:'x',kind:'tool',summary:null} / [1,2]），而 t298 新增的是**行字段断言** ⇒ 那些值必然被判不合契约。修法：回归面改成**逐端点合法行**；真实响应回放（②）已是这个更强形式。
**F-t299-02（low，自我更正）**：切换前那条「顶层 store/namespace 可能更严」的线索**被字面量证伪**。
**F-t299-03（low，登记）**：三值审查中「不可判定」**4 项** —— 要消掉需读**响应行结构**（DB 行类型 / Option<T>），本单只读到请求结构 ⇒ 建议另派一格。
