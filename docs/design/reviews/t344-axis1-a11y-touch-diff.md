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
- **建议**：二选一 —— (a) 放宽 inScope 到 `panel/tools/design-audit.mjs`（每个新行加一个 `row<N>` 检查 + 一条 `--self-test` 反向用例 ✓，正好对上 ③④）；或 (b) 保留 inScope，把②改成「给出判据 + 写明需要工具新增哪个检查」，③④ 改为下一单执行。

## §3 三条口径（本单的每条候选行都按它们写判据）

1. **采样面写死**：视口 1440×900、`scrollTo(0, 0)`、`emulateMedia` 按行指定（N3 必须 `reducedMotion: reduce`）。
2. **「在 DOM 里」≠「看得见」**：凡判据涉及可见性，必须 `getBoundingClientRect()` **rect ≥1px** 且 `computedStyle.visibility !== 'hidden'` 且 `display !== 'none'`。
3. **量的是「元素盒」还是「视口内可见区」必须写明**（t306 教训：`reachW/reachH = allHit===true ? box : eff`）：本清单里 N6 明写**量元素盒**（`scrollWidth/clientWidth`）；N4 明写**用 activeElement + rect**，不涉及命中区；若将来把 N6 改成命中测试，必须按 t306 的口径写清 `box` 与 `eff` 的取法。

## §4 未执行的部分（如实）

③ 反向读数、④ 覆盖守恒（pass/fail/not_measured 对账）、⑤ 的新行号一致性 —— 全部**依赖工具侧改动**（§2）⇒ 本单未执行，也未向 §12 添加任何行 ⇒ `check-contract.mjs` 读数保持不变（`§12 表：71 行，最大 71，缺口 []` · `CONTRACT SELF-CHECK: PASS`）。
