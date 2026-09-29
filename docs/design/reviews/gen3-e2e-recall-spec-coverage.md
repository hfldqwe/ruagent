# `recall.spec.ts` 的「永不执行」覆盖：自带夹具（t113）

- 单号：**t113**（work）· attempt 1 · 实现改动**只有** `panel/e2e/recall.spec.ts`（`git diff --numstat` = `126 53`）
- 与 `t110` 的边界：**点名机制**（`.github/workflows/e2e.yml` 的证据段）归 `t110`，本单**没有**碰那个文件；本单只回答「这条 spec 到底有没有覆盖」。

## 1 现状（诚实的结论）

```ts
// 改前 panel/e2e/recall.spec.ts:31-33
const stubs = page.locator(".recall-stub-head").filter({ hasText: /#\d+/ });
if ((await stubs.count()) === 0) {
  test.skip(true, "recall matched no memories in this database");
}
```

- **它此前覆盖到的东西 = 没有（在 CI 上）**。`test.skip` 是**无条件**的：只要页面上没有带 `#id` 的记忆 stub 就跳过，而 CI 的守护进程是**全新 root**（`memories=0`，本单实测 `READING memories_before=0`）⇒ 这条 spec 在 CI 里**每一次**都以 `1 skipped` 结束，从未执行过任何断言。
- 它原注释断言「doctor seeds at least one memory in CI」——**不成立**：本单在干净 root 上实测 `GET /api/v1/memory/list?namespace=user` 的 `hits=0`。
- 文件内唯一的 `test()` 就是这一条 ⇒ 该 spec 的**全部**断言（stub 可见、`stub-afford`、`#id`、内联展开、**`memory_get` 交叉核对**、折叠）在 CI 上**从未运行**。
- 本地（operator 的活库）它**偶尔真跑**：那就是 t102/t103 时期「focused e2e 绿」的来源之一，也正是本代反复出现的「**本地绿 = 环境恰好如此**」家族。

## 2 修法：自带夹具（选项 a），走真实路径

| 步骤 | 做法 |
| --- | --- |
| 闸门 | `writeAccess()`（`./write-guard`）⇒ 未武装时 `test.skip(reason)` —— **具名跳过**，且在 t110 的证据表里有名字，不是静默跳过 |
| 写入 | `POST /api/v1/memory/write`，body `{store:"observation", namespace:"user", content:"t113recallprobe …"}`（矩阵里合法的组合） |
| 取 id | 写入响应**只有** `{"outcome":"Inserted(1)"}`（实测），**不带 id** ⇒ 用 `GET /api/v1/memory/search?q=<PROBE>` 取 `hits[0].id`，顺带断言「写完即**可见**」 |
| 真实路径 | 面板 `/#memory` → 召回 → **保守** → 填 `PROBE` → 点召回 → 断言**记忆 stub 出现**（`await expect(stubs.first()).toBeVisible()`，**不再是 skip**）→ `#id` → 内联展开 → `GET /api/v1/memory/{id}` 交叉核对正文 → 再次点击折叠 |
| 清理 | `finally` 里 `DELETE /api/v1/memory/{id}`（**清理也是写**，在同一闸门下；**不走 UI**） + 两条后置断言：该 id 不再 **LISTED**、不再 **SEARCHABLE** |

## 3 读数（干净守护进程 + 临时端口 8848/8849；**显式 `E2E_BASE_URL`**；不碰 pid 79984）

| 情形 | 读数 |
| --- | --- |
| 改前（干净 root） | `1 skipped`（无条件跳过；`memories_before=0`） |
| **改后 run 1** | **`1 passed` / exit 0**（2.5s） |
| **改后 run 2** | **`1 passed` / exit 0**（2.5s） ⇒ **连续两次失败集合相同（都为空）** |
| 清理后置读数 | `DOM READING t113 cleanup: delete=200 list=200 search=200 listed_ids=[] search_ids=[]` ⇒ **零残留**；root 侧 `memories_after=0` |
| `RUAGENT_E2E_ALLOW_WRITES=0`（闸门关） | **`1 skipped`**（带 write-guard 的原因文本） |
| **能红的负控（修正后）**：保持写入不变，**只把查询**改成 `PROBE + "-nomatch"` | **`1 failed` / exit 1**，断言原文：`the fixture memory t113recallprobe must be recalled: … locator('.recall-stub-head').filter({ hasText: /#\d+/ }).first() with timeout 10000ms` ⇒ 是**有意义地红**（不是崩在别处）；恢复后 **`1 passed` / exit 0** |

**过程中的两次自我更正（如实登记）**：① 我先假定写入响应带 `id` ⇒ 断言失败（实测响应是 `{"outcome":"Inserted(1)"}`）⇒ 改用 search 取 id；② 我接着假定删除后 `GET /api/v1/memory/{id}` 是 **404** ⇒ 又失败 ⇒ 实测是 **软删除**（`delete=200`，`GET` 仍应答），遂把后置条件改成「不再 LISTED / 不再 SEARCHABLE」（并把三个原始状态码打进读数）。**两处都是我的假设错，不是产品错**。

## 4 判据（可机械核对）

1. **CI 证据表的 `distinct skipped SPECS (named)` 不再包含 `recall.spec.ts`**：改后它在干净 root 上 **1 passed**（出现在 passed 里）；只有在闸门关闭时才 skip，那时它是**具名**跳过（write-guard 的原因），由 `t110` 的机制点名。
2. **新的失败集合连续两次运行相同**（都为空）：run1/run2 均 `1 passed / exit 0`（见 §3）。

## 5 不覆盖什么（第 19 条）

- **只覆盖「保守召回在真实记忆上的 stub→展开→`memory_get` 交叉核对→折叠」这一条链**；**不覆盖** aggressive 策略、不覆盖知识/实体 stub 的形态、不覆盖召回排序/评分质量（那是有自己 gold set 的 Rust 侧工具）、不覆盖并发或索引延迟。
- **不覆盖「CI 的证据表会被正确渲染」** —— 那正是 `t110` 的机制，本单**故意不碰** `.github/workflows/e2e.yml`。
- **不覆盖闸门本身**：`writeAccess()` 的行为由 `write-guard.ts` + `registry.spec.ts` 负责；本单只用它，未改它。
- **已知代价**：闸门关闭时该 spec 仍会 skip（具名）⇒ 若某人用裸 `npx playwright test` 跑，它不执行；这是**设计**（那次会把它变成一次写入用户真实配置的操作），不是遗漏。
