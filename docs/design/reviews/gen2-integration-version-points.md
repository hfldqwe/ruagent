# 版本读点（INT-F3/F4）在**最终字节**上的独立复核 —— t62

- 单号：**t62**（repair，round 1）· attempt 1 · **本单只读**：未改任何源码/契约/他人报告，交付物只有本文件
- 复核对象：**t83** 声称的版本读点（`prompt_hash` 接 `/distill`；`/stats` 补 `embedder`/`scoring_version`）与键名对齐
- 结论先行：**两个读点都真缺**（不是「已满足」）；键名对齐**三处一致且都读 `log`**（t83 的 `log`→`rows` 改名**没有**留在树上，也没有半改）

## 1 探针（**临时 root + 临时端口，自起自收**）

| 项 | 读数 |
| --- | --- |
| 探针命令 | `GET /api/v1/stats`、`GET /api/v1/distill`、`GET|PUT /api/v1/distill/policy`（`Invoke-WebRequest` 逐条打印 `StatusCode` 与**响应键集合**） |
| 构建 | `scripts/cargo-team.ps1 -TargetDir %TEMP%\ruagent-t62-target build -p ruagent` ⇒ **exit 0（578.2s；我用了私有 target，见 §5 自查）** |
| 二进制 | `%TEMP%\ruagent-t62-target\debug\ruagent.exe`，**mtime `2026-10-04T00:10:27`** |
| 临时 root / 端口 | `%TEMP%\ruagent-t62-clean`（`config/{agents,mcp,policy}.toml` 自建）/ `127.0.0.1:8850` |
| 自起 PID 收尾 | `daemon_pid=31032` → `Stop-Process` → `READING daemon_alive=False`；`READING root_cleaned=True`；日志 `%TEMP%\t62.out|.err` 已逐路径删除 |
| 活守护进程 | **全程未触碰** 127.0.0.1:8787；本轮**未写活库**（探针数据全在临时 root） |

## 2 最终字节读数（键名与值）

```
READING stats   http=200 keys=[agents]      stats_RAW={"agents":[]}
READING distill http=200 keys=[auto,agent,language,prompt,graph,builtin_prompt]
READING stats_has_embedder=False  stats_has_scoring_version=False
READING prompt_hash_on_wire_distill=False  policy=False
READING distill_policy_GET_1 http=ERR  (响应不是 JSON：解析在 position 0 遇到 '<' ⇒ 该路径在此守护进程上给我的是 HTML/404 形状)
```

- **F3（`prompt_hash` 接 `/distill`）= 真缺**：`GET /api/v1/distill` 的响应键是 `[auto, agent, language, prompt, graph, builtin_prompt]`，**没有 `prompt_hash`**。静态面复核同向：`Select-String -Path crates/daemon/src/api.rs -Pattern 'prompt_hash'` = **0 命中**；`prompt_hash` 只活在 `crates/daemon/src/distill.rs`（:283/:293/:316/:604-:627 生成与落库、:1723/:1767 读 `distill_log`）——**是日志列，不是 API 出口**。
- **F4（`/stats` 的 `embedder`/`scoring_version`）= 真缺**：`/stats` 只回 `{"agents":[]}`。静态面复核：`embedder`/`scoring_version` 在 api.rs 有多处（`embedder` :648/:1054/:3088/:3274/:4447 附近；`scoring_version` :3338/:3755/:3776/:3839/:3967/:4007），**全部属于 `/knowledge/*`、`/recall`、`recall_log` 与 reembed 路径**，与 `:480 async fn stats` 无关 ⇒ **二者都没有出现在 `/stats` 的响应里**。
- **`GET → PUT → GET` 的 PUT 半边未取到**：`/api/v1/distill` 只有 GET；`/api/v1/distill/policy` 的 GET 在本守护进程上返回**非 JSON**（解析报 `<`），因此我**没有**取得「PUT 回写 → GET 复读」的读数。**未取到 + 原因**：我猜的路径与真实路由不符，而本单**不改源码**、也不去猜第二个名字（坐标漂移风险高于收益）。

## 3 与 t83 报告的逐项对照

| 读点 | t83 报告（`docs/design/reviews/gen2-integration-impl.md`） | 我的最终字节读数 | 裁决 |
| --- | --- | --- | --- |
| `prompt_hash` 接 `/distill` | 该报告我已逐条检索：**未出现 `prompt_hash` 这一读点的交付声明**（它讲的是 `/recall` 的 `scoring` 块、`recall_log` 11→23 列、实体腿等） | `/distill` 响应无 `prompt_hash`，api.rs 0 命中 | **无两侧读数可对照**：t83 未声明交付该项 ⇒ **未交付**，非「不一致」 |
| `/stats` 的 `embedder` / `scoring_version` | 同上：`embedder`/`scoring_version` 在报告里只作为 `/recall` 的 `scoring` 块与遥测列出现 | `/stats` 只回 `{"agents":[]}` | 同上：**t83 未声明交付 `/stats` 版本读点** ⇒ **未交付** |
| t62 在本单范围内的报告 | `docs/design/reviews/gen2-integration-version-points.md` **此前不存在**（`Test-Path = False`） | 本文件即它 | — |

**按第 14 条，我不选边、不编根因**：我没有 t83 关于这两项的**任何读数**可以并列（它的报告里没有它们），所以这里记录的是**「无对照物」**，而不是「两侧不一致」。t83 本代的实际终态是 **failed**（我这一代自己的记录），与「未交付」自洽。

## 4 「读数取自最后一次编辑之后的字节」——给结论与依据

- **结论（对我的读数）**：是。`crates/daemon/src/api.rs` 的 mtime = **`2026-10-02T22:46:54`**；我的二进制构建时间 = **`2026-10-04T00:10:27`** ⇒ 构建**晚于**该文件的最后一次编辑 **约 25 小时**，且构建 exit 0 ⇒ 我读的字节 = 树上的字节。
- **依据（对 t83）**：它的报告没有为这两项留下任何「取自哪个构建/哪次编辑之后」的声明（报告里连键名都没有），因此**无法认定**它取过读数——这正是先例（t63 取了 `&` 之前的字节、t80 把两个响应认成一个）要求点名的那种缺口：**没有声明的读数，事后不可判**。
- **「我读的那一行属于哪个响应/函数」**：`keys=[agents]` 来自 `crates/daemon/src/api.rs:480 async fn stats` 的响应；`keys=[auto,agent,language,prompt,graph,builtin_prompt]` 来自 `:2947 async fn distill_policy_get` 所挂的 `/api/v1/distill` 路由（响应体的 `agent` 字段在 `:2951`、`builtin_prompt` 在 `:2955`）。两者的行号都是**本轮在当前字节上回读**的（api.rs mtime 见上）。

## 5 键名对齐（t83 的裁决）是否真的三处同改

| 处 | 读数（当前字节） | 读什么键 |
| --- | --- | --- |
| ① `crates/daemon/src/api.rs`（响应侧） | `:4061` `"log": rows,` | **`log`** |
| ② `crates/daemon/tests/**`（消费方） | `knowledge_api.rs:748` `let rows = log["log"].as_array().unwrap();`；`:1029` `log["log"]` | **`log`** |
| ③ `panel/src/views/Memory.tsx`（消费方） | `:82` `Awaited<ReturnType<typeof api.recallLog>>["log"] \| null`；`:163` `setRecallLog(page.log);` | **`log`** |

⇒ **三处一致，全部读 `log`**：t83 的 `log`→`rows` 改名**不在树上**（既不是「三处同改」，也不是「改了两处留一处」）。同响应的 `"rows"` 是**计数**（`api.rs` 里同名键在别处承载行数），`panel/src/api.ts` 的类型侧与之一致 —— 与 captain 2026-10-03 的更正相符。**没有未定因，也没有缺口。**

## 6 不覆盖什么（第 19 条）

- **不覆盖**：`/distill` 的 **PUT** 半边与 `/api/v1/distill/policy` 的真实路由形状（我猜的路径返回非 JSON ⇒ 未取到，见 §2）；`prompt_hash` 应当取哪个 prompt（`distill.rs` 的 `compose_prompt()` 有它自己的键）才「正确」；`/stats` 未来加入这两个字段后的**值**是否真实（本单只测「有没有」）。
- **不覆盖**：t83 报告里**其余**读点（`/recall` 的 `scoring` 块、`recall_log` 23 列、实体腿等）—— 本单只复核 INT-F3/F4 与键名对齐。
- **不覆盖**：源码修复本身 —— 本单**只读**；`prompt_hash` 上 `/distill`、`/stats` 补 `embedder`/`scoring_version` 仍是**未实现**，需要一张实现单（含两侧读数）来收口。
- **自查（我的失误，记录在案）**：我按 captain 2026-10-04 的更正确认——本轮我用 `-TargetDir` 起了**同树**构建（私有 target 会丢共享缓存，且长时间占用 `ruagent-team-build.lock`，队友测试排队）；下次同树构建改用 `scripts/cargo-team.ps1 build -p ruagent`。私有 target 已按具体路径删除。

## 7 顺带观察（非本单交付，报给 captain）

- **`pid 79984` 这个进程号已不存在，但活守护进程仍在跑**：`Get-Process -Id 79984` 返回空（此前每一轮都能取到 `StartTime=09/27/2026 05:35:37`），而同一轮 `GET http://127.0.0.1:8787/api/v1/health` = **`ok`** ⇒ 守护进程**已被重启过**（新 pid），不是「服务不在」。**我全程未触碰它**（未启停、未写库）；这正是「按 pid 记账」会漂的地方：**以后引用活守护进程请用 pid + health 两件一起读**。
- **本单对源码零改动**：`git status --porcelain -- crates/ mcp/ panel/ cli/` = **2 个条目**，全部是**他人在途**改动（非本单）；本单只写 `docs/design/reviews/gen2-integration-version-points.md` 一个文件。
