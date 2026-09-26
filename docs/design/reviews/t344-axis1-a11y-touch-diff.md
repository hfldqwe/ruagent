# t344 — 轴一（可访问性与触控交互）与现有契约行的差集

**结论摘要**
1. 差集清单（下面 §1）—— 6 条**可被浏览器探针判定**的候选行，每条带**一个选择器 + 一个可判定判据 + 一次反向构造**。
2. **一条阻塞性发现**（§2）：**验收②「每一行必须能被现有审计工具量出来」与本单 inScope（不含 `panel/tools/`）互相冲突** —— 现有审计工具是**按契约行号硬编码**的（`row5`/`row16`/`row23`/`row32`/`row45` …），**新增行号在工具里没有对应检查** ⇒ 要么放宽 inScope 到 `panel/tools/`，要么把②改成「给出判据 + 说明需要工具新增哪个检查」。本单**因此未执行 ③④**（反向读数与覆盖守恒需要工具侧改动）。

## §1 差集清单（现有 71 行里**没有**的 a11y/触控行）

已有行（对照，避免重复）：13/14 文本对比度 · 15 焦点环对比度 · 17 无名称可交互元素 · 18 命中目标 <32/<24px · 21 命令面板 ARIA 缺口 · 23 `aria-expanded` 覆盖率 · 31 图谱边对比度 · 33 图谱分类非文本对比度 · 44 字面三点号。

| # | 候选新行 | 选择器 | 可判定判据 | 反向构造（必须变红） | 采样面/可见性口径 |
| --- | --- | --- | --- | --- | --- |
| N1 | 页面语言声明 | `document.documentElement` | `lang` 非空且 ∈ {zh-CN, en} 之一，且与 `#app` 首个正文文本的语种一致（探针只判「非空且与配置语言一致」） | 删掉 `lang` 或写成 `lang=""` ⇒ FAIL | 1440×900，`scrollTo(0,0)`；读的是**属性**（与可见性无关，须在判据里写明） |
| N2 | 缩放不被禁用 | `meta[name=viewport]` | 内容里**不含** `user-scalable=no`，且 `maximum-scale` 缺省或 ≥5 | 注入 `user-scalable=no, maximum-scale=1` ⇒ FAIL | 同上；读 `content` 属性 |
| N3 | 减少动效偏好被尊重 | 具名集：`.view-bar`、`.ant-layout-content`、`.sidebar-foot`（闭集，随实现登记） | `prefers-reduced-motion: reduce` 下，具名集内元素的 `computedStyle.animationDuration/transitionDuration` 全为 `0s`（或元素 `display:none`） | 给具名集内任一元素加 `transition: 200ms` ⇒ FAIL | 同视口；**必须 `emulateMedia({reducedMotion:'reduce'})`** 后才算；读 **computed style**（不是 class 名） |
| N4 | Tab 顺序无陷阱 | 具名集：`main.ant-layout-content.content` 内的可聚焦元素 | 连续按 Tab 40 次：焦点**始终留在** `main` 子树内，且 `document.activeElement` 不为 `body` 连续 ≥3 次 | 给 `.content` 加一个 `onkeydown` 吞掉 Tab（或加 `tabindex=-1` 到全部可聚焦元素）⇒ FAIL | 1440×900；判定用 `activeElement` + `getBoundingClientRect()` **rect ≥1px** 且 `visibility !== hidden`（DOM 里 ≠ 看得见） |
| N5 | 图标按钮的状态语义 | 具名集：`.recall-stub-head`、`.tool-head`、`.chat-group-more`（与行 23 同一批手写控件，但**不同属性**） | 每个手写**切换类**控件必须有 `aria-expanded` **或** `aria-pressed`（行 23 只查 expanded；本行查「二者至少其一」，覆盖「用 pressed 表达的开关」） | 把 `.tool-head` 的 `aria-expanded` 换成无 ARIA 的 `class="open"` ⇒ FAIL | 同视口；**不得**用全站 `querySelectorAll('[aria-expanded]')` 计数（antd `Select` 会造成虚假通过，见 §12.2）——只数具名集 |
| N6 | 横向滚动容器的触控归属 | 具名集：`.tool-output pre`、`.diff-body`（闭集） | 该容器 `scrollWidth > clientWidth` 时必须有 `tabindex="0"`（键盘可滚动，WCAG 2.1.1）**且** `touch-action` 不为 `none`（触控不被吞） | 去掉 `tabindex` ⇒ FAIL | 同视口；`scrollWidth/clientWidth` 是**元素盒**量，与可见区无关 ⇒ 判据写明「量元素盒」 |

**未列入的（诚实标注）**：WCAG 2.5.1（手势的单点替代）、2.5.4（动作触发）、1.4.4（200% 文字放大不丢内容）、2.3.3 的「动画是否必要」—— 这些**没有单一选择器 + 可判定判据**，写成行就是原则 ✗ ⇒ 若要收，必须先定**具名对象集**（哪几个手势/哪几个动画），否则不予收录。

## §2 阻塞性发现：② 与 inScope 冲突（证据）

- 审计工具是**按契约行号硬编码**的：`panel/tools/design-audit.mjs` 里出现 `row5ExcludeSelectors`（:97/:333/:655）、`row32CriterionLanded`（:4056/:5310/:5492）、`row.n === 32`（:5492）等；其自检也是按行号写的：`--self-test`（:417）的用例形如 `check("row16: outline-style:auto + shadow ring -> FAIL", …)`（:6183-6215）。
- ⇒ **新增行（例如 72+）在工具里没有任何对应检查** ⇒ 「每一行必须能被现有审计工具量出来」在**不改 `panel/tools/`** 的前提下**无法满足** ✗。
- 本单 inScope = `docs/design/`，**out of scope 明确含 `panel/tools/`** ✗ ⇒ ②③④ 不可执行。
- **队长已 amend 验收②**：允许两条路 —— (a) 用工具**现有**行种类表达；或 (b) 需要新行种类时**写出测量规格（选择器 + 判据 + 反向）并标注【需要工具行】**，工具行另行派单（panel/tools/ 由 t345 占用，不重叠）。**本清单六条全部属于 (b)** —— 逐条已给选择器 + 可判定判据 + 反向构造 ⇒ ②按 (b) 满足；**本单未改工具** ✓。

- **原建议**：二选一 —— (a) 放宽 inScope 到 `panel/tools/design-audit.mjs`（每个新行加一个 `row<N>` 检查 + 一条 `--self-test` 反向用例 ✓，正好对上 ③④）；或 (b) 保留 inScope，把②改成「给出判据 + 写明需要工具新增哪个检查」，③④ 改为下一单执行。

## §3 三条口径（本单的每条候选行都按它们写判据）

1. **采样面写死**：视口 1440×900、`scrollTo(0, 0)`、`emulateMedia` 按行指定（N3 必须 `reducedMotion: reduce`）。
2. **「在 DOM 里」≠「看得见」**：凡判据涉及可见性，必须 `getBoundingClientRect()` **rect ≥1px** 且 `computedStyle.visibility !== 'hidden'` 且 `display !== 'none'`。
3. **量的是「元素盒」还是「视口内可见区」必须写明**（t306 教训：`reachW/reachH = allHit===true ? box : eff`）：本清单里 N6 明写**量元素盒**（`scrollWidth/clientWidth`）；N4 明写**用 activeElement + rect**，不涉及命中区；若将来把 N6 改成命中测试，必须按 t306 的口径写清 `box` 与 `eff` 的取法。

## §4 未执行的部分（如实）

③ 反向读数、④ 覆盖守恒（pass/fail/not_measured 对账）、⑤ 的新行号一致性 —— 全部**依赖工具侧改动**（§2）⇒ 本单未执行，也未向 §12 添加任何行 ⇒ `check-contract.mjs` 读数保持不变（`§12 表：71 行，最大 71，缺口 []` · `CONTRACT SELF-CHECK: PASS`）。


## §5 ③ 反向与 ④ 覆盖守恒：机制读数（本单未加行，故给的是机制证据 + 逐行规格）

### ③ 反向（每条候选行的反向构造已在 §1 表里逐条给出）+ 机制读数

审计工具的 `--self-test` 就是「构造违规 ⇒ 必须变红」的现成机制，**同一条判据函数同时供自检与该行使用**（自检输出里有一句直接这么说）：

```
$ node panel/tools/design-audit.mjs --self-test
  ok  row35 must-FAIL: 5 CSS breakpoints FAIL (the <=4 budget is NOT relaxed)   got=false want=false
  ok  row35 must-PASS again once the injected 700 is removed (the removal direction)  got=true want=true
  ok  row35: the judge is single-sourced (the self-test and the row share one function)  got=true want=true
  ...
self-test: 483/483 pass      exit=0
```

⇒ **机制证据**：483 条自检里每一对都是 must-FAIL + must-PASS 两个方向（`--self-test` 的用途见 `panel/tools/design-audit.mjs:316/417/454`）✓。**每一条新行的反向**（§1 表第 5 列）落地方式 = 给该行写一条 `check("row<N> must-FAIL: …", judge(...), want)` 用例 + 一条 must-PASS ✓ —— **这正是工具行（t345 名下）要做的第一件事** ✓。

### ④ 覆盖守恒（7.98）：新行的三格账怎么写

工具自己已经定下规矩（`panel/tools/design-audit.mjs:2768` 注释原文）：

> Every way of NOT measuring gets a NAME. An unnamed empty result is what the panel team keeps having to reject: it reads like a pass.

⇒ 六条新行各自必须落在 **pass / fail / not_measured 之一**，且 `not_measured` **必须带名字**（例如「该路由没有可聚焦元素」「本捕获没有 meta 标签」）✓。对每一条的**预期落格**（工具行落地后应逐条对上账）：

| 行 | 预期格（当前实现下） | 名字（not_measured 时） |
| --- | --- | --- |
| N1 lang | pass（`lang` 已声明） | —— |
| N2 缩放 | pass（viewport 未禁用缩放） | —— |
| N3 减少动效 | **not_measured** 直到工具 `emulateMedia({reducedMotion:'reduce'})` 落地 | `no reduced-motion capture on this run` |
| N4 Tab 陷阱 | pass | —— |
| N5 状态语义 | pass（三个手写控件都有 `aria-expanded`） | —— |
| N6 横向滚动容器 | **not_measured** 直到具名集在该路由存在 | `no scrollable container in the named set on this route` |

（这些是**待核对的预期**，不是读数 —— 真正的三计数要等工具行落地后跑一次才有 ✓。本单不伪造它们 ✗。）

## §6 ⑤ check-contract 读数（未加行 ⇒ 计数保持一致）

```
$ node docs/design/check-contract.mjs
§12 表：71 行，最大 71，缺口 []
CONTRACT SELF-CHECK: PASS
```

⇒ 本单**未向 §12 添加行**（六条都属于 (b)【需要工具行】）⇒ 行数与表一致 ✓、自检 PASS ✓。**工具行落地时**，六条应以**连续行号 72–77** 加入 §12（`缺口 []` 要求连续 ✓），并同步 `primitives.md §11` 的判据条目（现有行 23 的写法即为此模式：「判据定义见 primitives.md §11 行 23」）✓。
