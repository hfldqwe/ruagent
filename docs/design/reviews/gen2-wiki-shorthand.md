# t56：三处「读起来像 map」的索引简写改为明确措辞（纯措辞单）

> 单号 t56（repair round 1）· 成员 `wiki` · attempt 1 · 时间窗 2026-09-28 17:0x UTC（+08:00 01:0x）
> inScope：`docs/design/reviews/gen2-wiki-spec.md` · `docs/design/reviews/gen2-wiki-impl.md` · 本报告
> 本单**只改措辞**：不动任何判据、读数值、代码或行为；不写活库；未启停 pid 79984。

## 0 一句话

把两份**持久文档**里 `degrees[slug]` / `degrees[s]` / `links.degrees[slug]` 三处**索引简写**改成「`degrees` 数组中该 slug 的那一行」，并**保留**同一份规格早就写对的 `pub degrees: Vec<PageDegree>` / `struct PageDegree{…}` 类型声明。理由是这三处简写**已经真的误导过一次**（t51 我按 map 形状写了第一版断言，被测试当场打红；而契约 L78 从头就是数组）。

## 1 三处修改（旧文字逐字 → 新文字；行号因插入修订块而 +8 / +1）

| # | 位置（改前 → 改后） | 旧文字（逐字） | 新文字（逐字） |
| --- | --- | --- | --- |
| 1 | `gen2-wiki-spec.md` **:198 → :206**（G5 标定读数的判据列） | `` `links_out` 与 `degrees[slug].links_out` 对 4 页逐位相等（含 `links_out_broken` 分开报）`` | ``**逐页比较**：`pages` 里该页的 `links_out`，与 `degrees` **数组中该 slug 的那一行**（`PageDegree`，类型见 D 节的 `Vec<PageDegree>`）的 `links_out` 对 4 页逐位相等（含 `links_out_broken` 分开报）`` |
| 2 | `gen2-wiki-spec.md` **:342 → :350**（D.5 `link_graph` 的纯函数判据） | ``- `degrees[s].links_out == edges` 中以 s 为 src 的条数 + `links_out_broken[s]`；`` | ``- `degrees` **数组中 slug 为 s 的那一行**（`PageDegree`）满足：它自己的 `links_out == edges` 中以 s 为 src 的条数 + 它自己的 `links_out_broken`；`` |
| 3 | `gen2-wiki-impl.md` **:189 → :190**（§4 单源强制点表的判据列） | `` `the_two_graph_endpoints_cannot_disagree` 对每页断言 `pages[i].links_out == links.degrees[slug].links_out` 且 `links_in`/`links_out_broken` 同样逐位相等 `` | `` `the_two_graph_endpoints_cannot_disagree` 对每页断言：`pages` 里该页的 `links_out` == `degrees` **数组中该 slug 的那一行**（`PageDegree`；`degrees` 是 `Vec<PageDegree>`，**不是**以 slug 为键的 map）的 `links_out`，且 `links_in`/`links_out_broken` 同样逐位相等 `` |

**保留不动**（本来就是对的那一处，任务书要求保留）：`gen2-wiki-spec.md:331` `pub degrees: Vec<PageDegree>`、`:339` `pub struct PageDegree {`、`:340` `pub slug: String, pub links_in: usize, pub links_out: usize, pub links_out_broken: usize,`。

**行数读数**：`gen2-wiki-spec.md` **781 → 789**（+8 = 新增的 8 行修订记录；三处判据行仍是 3 行，只换措辞）；`gen2-wiki-impl.md` **502 → 503**（+1 = 新增的 1 行「修订记录 3」）。改后定位：`spec:206`（G5 行）、`spec:350`（D.5 判据）、`impl:190`（§4 表）。

## 2 grep 全仓 `degrees[` 逐条分类（验收要求）

**读数**（递归 grep `docs/ crates/ panel/ cli/ scripts/`，排除 `node_modules/dist/.git`）：命中 **12** 处、分布在 4 份 `.md`：

| 文件:行 | 内容 | 分类 | 处置 |
| --- | --- | --- | --- |
| `gen2-integration-contract.md:78` | `degrees:[{slug,links_in,links_out,links_out_broken}]` + `pages[i].links_out == degrees[i].links_out` | **合法**：前者是完整的数组类型声明，后者是 `[i]` **真索引** | 不动（且它不在本单 inScope） |
| `gen2-integration-contract.md:268` | K-8 的修法：`pages[i].links_out == degrees[i].links_out` | **合法**：`[i]` 真索引 | 不动 |
| `gen2-wiki-consumption.md:19` | `degrees[0] = {"slug":"e2e-consumption-probe",…}` | **合法**：HTTP 响应的**真实读数**，按位置取数组首元素 | 不动 |
| `gen2-wiki-consumption.md:36` | 「`degrees[slug]` 的四个数与 `/wiki/pages` 同名页逐位相等」 | **同类简写（要改性质）** —— 但 `gen2-wiki-consumption.md` **不在本单 inScope** | **只登记，未改**（见 §5 U-1） |
| `gen2-wiki-consumption.md:39/:128/:130` | 撤回/更正段里**逐字引用**旧措辞（`我最初按 map 写断言…`、`degrees[slug].links_out`…） | **合法**：引文必须保持逐字，否则更正段失去锚点 | 不动 |
| `gen2-wiki-spec.md:12/:13/:15/:16` | 本单新增修订块的**逐字引用**（旧文字 1、2） | **合法**：引文 | 不动 |
| `gen2-wiki-impl.md:10` | 本单新增「修订记录 3」的**逐字引用**（旧文字 3） | **合法**：引文 | 不动 |
| `gen2-wiki-spec.md:206`、`:350`、`gen2-wiki-impl.md:190` | 三处操作位（已改为「数组中…的那一行」） | **已改** | ✓ |

**`crates/` 与 `panel/` 的结论**：**全 0 命中**（另 `cli/`、`scripts/` 也是 0）。所以本单没有把任何**代码**里的合法用法当错误去"修"；`crates/daemon/src/wiki.rs` 里真实类型就是 `Vec<PageDegree>`，代码侧从未出现 map 形状（这也是"简写只在文档里"的旁证）。
**净读数**：改前 10 处 → 改后 12 处，多出的 2 处**全部是修订块里的逐字引文**；**操作位（非引文）的 `degrees[` 现在为 0**。

## 3 日期修订记录（已落在两份文件里，旧文字逐字引用）

- `gen2-wiki-spec.md` 顶部新增 **8 行**修订块（`:12`–`:19`），含日期（2026-09-28）、owner、**旧文字逐字引用两条**、**为什么改**（含"已误导过一次"的实证：t51 第一版断言写成 `links["degrees"]["deploy-pipeline"]["links_out"]`，被测试当场打红；契约 L78 从头是数组）、以及"**不改**判据/阈值/标定读数/字段名与语义"的声明。
- `gen2-wiki-impl.md` 顶部新增 **修订记录 3**（`:10`，单行），同样带日期、旧文字逐字引用、误导实证、与新文字。

## 4 「只做措辞精确化」的证明（以及一条仪器警告）

1. **数字序列逐位相同**（判据读数不许变）——把三对旧/新文字的数字按出现顺序抽出比较：

```
spec G5 行   : 旧数字=[4]  新数字=[4]  相同=True   （旧长度 75 → 新长度 154）
spec D.5 判据: 旧数字=[]   新数字=[]   相同=True   （旧长度 73 → 新长度 117）
impl §4 表   : 旧数字=[]   新数字=[]   相同=True   （旧长度 140 → 新长度 221）
ALL DIGIT-SEQUENCES IDENTICAL = True
```
   **这条检查抓到了一处我自己的失误**：第一版新文字把 `对 4 页逐位相等` 改成了 `逐位相等`（丢掉了采样面「4 页」），数字序列立刻变成 `[4]→[3]`（`3` 来自我顺手加的 `D.3` 引述）⇒ **被判红并已改回**（现在保留「对 4 页逐位相等」，把节号写成「D 节」以不引入新数字）。这正是本单存在的意义：措辞单也会碰上"顺手改掉读数"。
2. **判据关键词仍在**（计数，非读数）：`links_out_broken` 14 处、`self_links` 15 处、`Vec<PageDegree>` 6 处、`wanted[0]` 1 处。
3. **类型声明保留**：`spec:331/:339/:340` 逐字未动（见 §1 末）。
4. **代码未动**：本单写入集合只有两份 `.md` + 本报告；`crates/`、`panel/`、`cli/`、`scripts/` 我一个字节都没改（`changedPaths` 即证据）。
5. **仪器警告（值得登记）**：`gen2-wiki-spec.md` / `gen2-wiki-impl.md` **未被 git 跟踪** —— `git ls-files -v` 对两者**无输出**、`git status --porcelain` 把两者都报成 `??`、`git diff --numstat --` 它们**是空**；`git status --porcelain --ignored -- docs/design/reviews` 显示该目录下同类报告（graph/distill/…）**也基本都未跟踪**。所以 **`git diff` 在这些文档上不是可用仪器**：**空 diff 只说明未跟踪，不说明未改**。本单因此改用**内容级读数**（数字序列 + 行数增量 + 关键词计数 + 逐字引用），而不是 git。下一位改这些文档的人会需要这条。

## 5 未测 / 不做（不静默）

| # | 项 | 状态 | 原因 |
| --- | --- | --- | --- |
| U-1 | `gen2-wiki-consumption.md:36` 的同类简写（`degrees[slug]`） | **只登记，未改** | 该文件不在本单 inScope（t51 的报告，属我但归 t51 的产物）；一句话即可改，请 captain 决定是否并入后单 |
| U-2 | `crates/`、`panel/` 内的 `degrees[` | **无命中，无需改** | 递归 grep 读数 0；代码里的类型本就是 `Vec<PageDegree>` |
| U-3 | 契约 `gen2-integration-contract.md` 的 `[i]` 写法 | **判定合法，不动** | `[i]` 是循环变量索引，不读作 map；且该文件不在本单 inScope |
| U-4 | 任何代码行为/测试读数 | **本单不涉及** | 纯文档措辞单：没有跑 cargo（无代码改动可验），读数取自文件内容本身 |

## 6 读数命令与结果

```powershell
# 本单的 verify（合同命令）
powershell -NoProfile -Command "Select-String -Path docs/design/reviews/gen2-wiki-spec.md -Pattern '该 slug 的那一行' -CaseSensitive | Select-Object LineNumber"
# → LineNumber 206            （exit=0；G5 行已含新措辞）

# 另外两条我自己的读数
Select-String -Path docs/design/reviews/gen2-wiki-spec.md -Pattern 'slug 为 s 的那一行'   # → 350
Select-String -Path docs/design/reviews/gen2-wiki-impl.md -Pattern '数组中该 slug 的那一行' # → 190
```
