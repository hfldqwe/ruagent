# t346 — 轴一六条落成工具行 72–77（判据 + 自检对 + §12/§11 同步）

**结论**：六条全部落地并有读数。**--self-test 495/495**（原 483 + 12）· **check-contract PASS**（§12 表 **77 行 / 最大 77 / 缺口 []**）· 覆盖守恒：**5 pass + 1 named not_measured**（行 77），**没有一格是无名的空**。

## 1 六条的最终读数（chat/task 两条路由，1440×900，dark）

| 行 | 判定 | 读数（来自 --out=C:/tmp/t346/out 的 metrics.json） |
| --- | --- | --- |
| 72 lang | **PASS** | `lang="zh-CN"`（两条路由相同） |
| 73 缩放 | **PASS** | `content="width=device-width, initial-scale=1.0"`：不含 `user-scalable=no`，不含 `maximum-scale` |
| 74 减少动效 | **PASS** | 具名集 3/3 可见（`.view-bar` / `.ant-layout-content` / `.sidebar-foot`），`transitionDuration = animationDuration = 0.00001s`（`emulateMedia({reducedMotion:reduce})` 之下） |
| 75 Tab 陷阱 | **PASS** | chat：40 次 Tab / 可聚焦 44 / 19 次离开 `main` · task：40 / 28 / 20（**离开 `main` 是信息**）· 元素身份唯一 ⇒ 无卡住 |
| 76 状态语义 | **PASS** | chat 命中 `.chat-group-more`（`aria-expanded`）· task 命中 `.tool-head`（`aria-expanded`）；其余选择器「本路由没有该元素」 |
| 77 横向滚动 | **NOT_MEASURED（带名字）** | `no scrollable container in the named set on this route`（两条路由上 `.tool-output pre` / `.diff-body` 都不存在） |

**与 t344 §5 的预期对账**：t344 预期 N3/N6 是 named not_measured。**读数更正了这份预期**：N3（行 74）实测是 **PASS**（具名集元素在 reduce 下确实为 0s）；N6（行 77）确实是 named not_measured ✓。预期是预期，读数才是读数。

## 2 反向（六条各自必须能红）—— 12 条自检用例

每条一对，且**都调用 CHECKS 里那一行的 judge**（`CHECKS.find((r) => r.n === n).judge`），因此自检证明的不只是判据函数，而是**登记 + parse + 包装**这一整条链：

| 行 | must-FAIL 构造 | must-PASS 构造 |
| --- | --- | --- |
| 72 | `lang=""` ⇒ FAIL | `lang="zh-CN"` ⇒ PASS |
| 73 | `user-scalable=no, maximum-scale=1` ⇒ FAIL | `width=device-width, initial-scale=1` ⇒ PASS |
| 74 | 具名集内一个元素 `transition: 200ms` ⇒ FAIL | 全部 0s ⇒ PASS |
| 75 | 40 次 Tab 全落在同一个元素（真陷阱）⇒ FAIL | 焦点在移动、且**离开 `main` 不算失败** ⇒ PASS |
| 76 | 可见控件既无 `aria-expanded` 也无 `aria-pressed` ⇒ FAIL | 只有 `aria-pressed` ⇒ PASS |
| 77 | 溢出容器没有 `tabindex` ⇒ FAIL | `tabindex="0"` 且 `touch-action` 非 `none` ⇒ PASS |

## 3 一处判据修正（必须点名）：t344 的 N4 判据不是 WCAG 2.1.2

t344 §1 写的是「连续按 Tab 40 次：焦点**始终留在** `main` 子树内」。**按原文实测 chat 19/40、task 20/40 离开 `main`** —— 而这两条路由上**没有任何键盘陷阱**。WCAG 2.1.2 禁止的是**陷阱**（焦点出不去），不是「焦点不许离开某个容器」；正常页面 Tab 本来就会走到侧栏、顶栏。

⇒ 本行改为判**陷阱的真实形状**：① 走过的不同元素 ≥2；② 同一元素连续 <5 次；③ 落点元素可见（rect ≥1px + computed style）。**离开 `main` 的次数作为信息输出，不参与判定**。反向构造随之改为「给 `.content` 加一个吞掉 Tab 的 `keydown`」（焦点永不移动 ⇒ FAIL）。

## 4 构建过程中实测到的两类**假红**（写进代码注释）

1. **元素身份太粗**：第一版用 `tag#id.class` 当身份 ⇒ **12 个 `.nav-link` 折叠成一个键**，被读成「同一元素连续 12 次」⇒ 判红 ✗。改成 tag+class+**可见文本**后，**10 个 `.ant-checkbox-input`（无 id、无文本）又折叠成一个** ⇒ 再次假红 ✗。最终用 **WeakMap 逐个编号** ⇒ 身份唯一 ✓。
2. **`body` 不是陷阱**：焦点跑出页面末尾时 `activeElement` 会回到 `body`，连续命中会被当成「卡住」✗ ⇒ 统计陷阱时**排除 `body`**，它单独作为信息输出 ✓。

## 5 三条口径的落点（每条判据都写了）

* **采样面写死**：1440×900 + `scrollTo(0,0)`；行 74 **必须先 `emulateMedia({reducedMotion:"reduce"})`** 再读 computed style（探针在读完立刻还原）。
* **「在 DOM 里」≠「看得见」**：行 76 用 rect ≥1px + `display`/`visibility`；行 74/75 同规则；行 72/73 读的是**属性**，判据里写明「与可见性无关」。
* **量元素盒还是视口内可见区**：行 77 明写**量元素盒**（`scrollWidth`/`clientWidth`，与可见区无关，并写明若改成命中测试要按 t306 口径写 `box`/`eff`）；行 75 明写**用 `activeElement` + rect**、不涉及命中区。

## 6 诚实标注的未覆盖

* **行 72 只实装了一半**：判「已声明且属于 {zh-CN, en}」✓；判「与 `#app` 首个正文文本的语种一致」✗（需要应用的配置语言，探针今天没有这个来源）—— 已写进 MASTER 该行的「判定方式」列与 primitives §11。
* **本次是子集运行**（`--routes=chat,task`）：行 77 的 named not_measured 只说明**这两条路由**上没有该具名集；全站结论要等默认全量重跑（审计自己打了这条 warning）。
* 六条都只在 **dark** 模式判（行 72–77 的 `modes` 均为 `["dark"]`）：这几条判据与配色无关，两模式判会得到同一结论。

## 7 验证命令与读数

```
$ node panel/tools/design-audit.mjs --self-test
self-test: 495/495 pass            exit=0
$ node docs/design/check-contract.mjs
§12 表：77 行，最大 77，缺口 []
CONTRACT SELF-CHECK: PASS           exit=0
$ node panel/tools/design-audit.mjs --routes=chat,task --check --out=C:/tmp/t346/out
exit=0；行 72-76 PASS，行 77 NOT_MEASURED（带名字）
```

**注意 `--out` 的形式**：`--out <dir>` 会被解析成 `--out=true` 并被拒（会往工作树里写 `true/`）；必须用 `--out=<绝对目录>`。
