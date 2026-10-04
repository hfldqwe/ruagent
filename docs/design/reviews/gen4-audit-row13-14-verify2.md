# t181 · 窄单：补 t176 缺的四项工具读数 + `getComputedStyle`（C59）

**唯一写入**：本报告。`panel/**`、`crates/**`、`.github/**`、`scripts/**`、`tools/**` 一行未改（两臂全在**仓库外**）。未起真守护进程（8787 未触碰）。

## 1 两臂身份（成对记录，含构建产物）

| 臂 | 源码版本 | `panel/src/index.css` blob / sha256 | 构建产物 | asset sha256 |
| --- | --- | --- | --- | --- |
| **修前 pre** | `9f4eb9f`（=`8f86313^`） | `d3384f3f…` / **`FB575D60…`** | **`index-DH8EJgem.css`** | **`C94A9566236A42F0…`** |
| **修后 post** | `8f86313` | `d904a55a…` / **`D51A4134…`** | **`index-CQhHFKM-.css`** | **`46DDF76A81E3C152…`** |

⇒ 两臂的**asset 名与哈希前缀与作者申报逐字相同**（`index-DH8EJgem.css` / `C94A9566…`、`index-CQhHFKM-.css` / `46DDF76A…`）。**没有用 `HEAD^`**（今天 = `bb2d99a`，已含修后字节 ⇒ 会静默空转）。
**单变量证明**（captain 要求）：两臂源码树逐文件 SHA256 比对，`Compare-Object` **只有一个差异项 `src\index.css`**（其余文件全部相同）⇒ 两臂**只差那个 CSS** ⇒ 本单的对照比「提交到提交」更干净。**因此不覆盖**：「那两个提交之间别的文件有没有变」是另一个问题，不在本单。

## 2 两臂的工具读数（我自己的 daemon、临时 root、显式 base-url）

```
node panel/tools/design-audit.mjs --check --routes=home,settings --modes=dark,light --tabs=40 \
  --no-shots --no-pixels --allow-not-measured --base-url=http://127.0.0.1:<8871|8872> --out=<仓外目录>
E2E_BASE_URL=http://127.0.0.1:<同端口>   RUAGENT_PANEL_DIST=<该臂 dist>   （B29/C60：显式给出，不回落 8787；未设 CI）
```
| 读数 | 修前（8872 / `index-DH8EJgem.css`） | 修后（8871 / `index-CQhHFKM-.css`） |
| --- | --- | --- |
| 服务的是哪一臂（实测 HTML 里的 asset） | `index-DH8EJgem` ✓ | `index-CQhHFKM-` ✓（并确认**不是** pre） |
| `--check` 退出码 | **1** | **1**（行 7/12/18 仍 FAIL，预期） |
| **失败行集合** | **`7,12,13,14,18`** | **`7,12,18`** |
| row 13（暗色文本对比度） | **FAIL** | **PASS** |
| row 14（亮色文本对比度） | **FAIL** | **PASS** |
| row 4 / row 5 / row 30 / row 38 | PASS / PASS / PASS / PASS | PASS / PASS / PASS / PASS |

⇒ **① row 13/14 红→绿**（工具判定原文即上表的 FAIL→PASS）；⇒ **② 失败行集合只差 13 与 14**（`7,12,13,14,18 → 7,12,18`）——与作者申报逐字一致；⇒ **③ 行 4/5/30/38 两臂都 PASS，且其 `target`/`limits` 逐字相同**（`{"max":1}` · `{"max":40}` · `{"min":1.2}` · `{"max":1}`）；行 30 的 target 文本里就写着那条 `亮色 borderSecondary #e7e9ed → #dee0e4 ⇒ 对 canvas 1.2330` ⇒ 与作者报的 `ring #dee0e4 1px` 同源。

## 3 `getComputedStyle`（我自己的量，修后臂）

**第一次探针撞上了别人的进程**：我先按计划打 8871，但那一刻 8871 的属主是 `%TEMP%\t169\bin\ruagent.exe --root %TEMP%\t169\root`（**不是我的**，我未停它）⇒ 那次读数**不能归给我这一臂**，**已作废重取**。**干净的那次**：我自己的 daemon（`%TEMP%\t181\bin\ruagent.exe`，pid 40016，端口 8873，`--root %TEMP%\t181\root-post2`），并**先验证服务的 asset 是 `index-CQhHFKM-`**：
```
mode=light cls="step-num" borderWidth="0px 0px 0px 0px" borderStyle="none"
           boxShadow="rgb(138, 86, 0) 0px 0px 0px 1px inset"   color="rgb(138, 86, 0)"
mode=dark  cls="step-num" borderWidth="0px 0px 0px 0px" borderStyle="none"
           boxShadow="rgb(245, 180, 87) 0px 0px 0px 1px inset" color="rgb(240, 169, 59)"
```
⇒ **`inset … 1px` 的环在两档都在**、`border-width` 都是 `0px`/`none` ⇒ **C59 的机制未被换掉，不是 blocker**（与作者报的 `borderWidth=0px/none`、`boxShadow=rgb(138,86,0) 0px 0px 0px 1px inset` 逐字一致）。**附加读数**：亮档 `color` = `rgb(138,86,0)`（= `#8a5600`，正是新规则）而暗档 `color` = `rgb(240,169,59)`（= `#f0a93b`，未动）⇒ **「按档作用域、另一档没被带动」这次是我**量**出来的**，不只是字节推断。

## 4 未测 / 边界（C63、C47）

* **未测**：① 行 4/5/38 的**观测计数列**（作者报 `0` / `3` / `0`）与行 30 的**比值**我只拿到 `PASS` 与 `limits`，**没有单独提取数值列**；② `checked` 计数（作者报 `61/140/61/140`）**未测**（我未从日志提取；工具日志已按纪律删除）⇒ 这两格我写「未测」，**不写「通过」**。
* **边界**：两臂都用 `--no-shots --no-pixels` ⇒ 结论**只对计算样式成立**；**不**背书像素/感知层/屏幕阅读器。未做全量 `--check`（13 路由 × 两档），只按作者配方跑了 `home,settings` 两路由。
* **C60（更正后的用法）**：我每次都显式给 `E2E_BASE_URL` ⇒ 用它；未设 `CI` ⇒ 不会回落 8787；**8787 全程未被访问**（另：真守护进程未启停，未写活库）。
* **门禁**：`npm --prefix panel run build`（仓库自己的）⇒ **exit 0**（`✓ built in 5.38s`）；两臂各自的构建也 exit 0。

## 5 收尾

我记录的进程：`pid 41828`（8871，post，已自行退出）· `pid 34512`（8872，pre，按 PID 停止）· `pid 40016`（8873，post2，按 PID 停止）。**按具体路径删除**：`%TEMP%\t181` 下的 `src-pre/src-post/dist-pre/dist-post/bin/root-*/out-*/cs*.cjs/*.log`（junction 先 `rmdir`）⇒ 现在只剩一个**空目录**；端口 8872/8873 已释放；**8871 上仍有别人的 `t169` 进程，我未触碰**。
