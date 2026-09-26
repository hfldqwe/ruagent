# t281 独立验证 t260（知识/wiki 命中进入 agent 上下文）—— 部分完成，结论 needs_revision（验证者侧未完成，非产物缺陷）

验证者：ui-audit · 任务 t281（verification round 1）· reviewedTaskId = t260
**诚实前置**：本单我只独立完成了 5 条 acceptance 中的 **第 4 条**，并用既有测试拿到了第 1 条的**单元级 + 真 run（memory 块）**证据；**第 1 条的 knowledge/wiki 运行时读数、第 2 条（软删后知识块仍在）、第 3 条（压力下丢弃顺序）我没有独立测量** —— 原因与补齐配方写在最后。**这不是对 t260 的否证，是验证未完成。**

## ① 已验证（我自己跑）

### 第 4 条：grep 读数 ✓（与预期一致）

```
render_context 生产调用点 = 2：
  crates/daemon/src/chat.rs:518   let out = render_context(&items, &InjectionBudget::default());
  crates/daemon/src/runs.rs:1894  render_context(&items, &InjectionBudget::default())
（定义在 crates/memory/src/inject.rs:325；包装器内部调用 inject.rs:404）
render_injection 生产调用点 = 0：
  inject.rs:399 定义（pub fn render_injection … { render_context(&items, budget) }）
  其余命中全部在 #[cfg(test)]：inject.rs:425/449/461/478/488
```

⇒ `render_injection` 已成 `render_context` 的薄包装，生产路径上无人调用（只被自己的单测调用）。

### 第 1 条（部分）：注入渲染的单测 + 真 run 的 context_injected

`CARGO_TARGET_DIR=D:/rust_cache cargo test -p ruagent-memory inject` ⇒ 全部 ok，其中与 t260 直接相关的：
`knowledge_and_wiki_are_separate_blocks_and_knowledge_lines_are_undated`（断言 `out.contains("<knowledge>")` / `out.contains("<wiki>")`，inject.rs:620-622）·
`knowledge_selection_rule_is_the_top_n_per_kind` · `the_total_bound_holds_with_knowledge_items_too` · `per_block_truncation_is_visible` · `empty_hits_render_no_block_at_all` · `duplicate_tags_merge` · `golden_render` · `empty_renders_empty`。

`cargo test -p ruagent-mock-agent --test e2e_daemon memories_are_injected_into_run_prompts` ⇒ **1 passed**（临时 root + mock agent；断言注入的 memory 出现在 agent 的 prompt 回显里、SSE 流上有 `"type":"context_injected"`、且渲染出 `<relevant_memories>`）⇒ **真 run 确实产生 context_injected** ✓，但**这条只覆盖 memory 块**，不覆盖 knowledge/wiki。

## ② 未验证（不静默，附补齐配方）

| 未完成项 | 为什么没做 | 补齐配方（下一位验证者可直接照做） |
| --- | --- | --- |
| 第 1 条：真 chat + 真 run 各产生一次含 **knowledge + wiki** 块的 context_injected，并给逐字片段与 transcript 路径 | 需要一条**临时 root 的进程内 chat/run 探针**（外部 crate，路径依赖 crates/daemon + crates/mock-agent），我先去查既有测试与调用点、预算就耗尽了，没有动手写 | ① 仓外建探针（参考 t251/t255 的形态）：临时 root + `Db::open` + 真迁移；② 写入 1 条记忆 + 1 篇知识文档 + 1 个 wiki 页；③ 走 `crates/mock-agent` 的 echo mock 驱动一次 run（`e2e_daemon.rs:1014` 的 `drive_run` 同型）与一次 chat（`chat.rs:518` 路径）；④ 读 `<root>/transcripts/*.jsonl` 里 `context_injected` 事件的 `text`，逐字截 `<knowledge>` / `<wiki>` 块头与字符数 |
| 第 2 条：软删一条记忆后它不再进入注入，而知识块仍在（证明缺席来自 DELETE 而非渲染变空） | 同上，需要同一条探针的第二次 run | 在同一探针里：run#1 记录块头 → `delete_memory(id)` → run#2 断言记忆块消失、`<knowledge>`/`<wiki>` 仍在且字符数与 run#1 相同（**同时**给「不软删只清空知识」的对照，才能把「来自 DELETE」与「渲染变空」分开） |
| 第 3 条：五块逼近 per_block 上限时的渲染字节数、被丢的块、丢弃计数 | 需要调用 `render_context` 的探针（`inject.rs:325` 是 pub）或扩展注入单测 | 仓外探针直接调 `ruagent_memory::inject::render_context(&items, &InjectionBudget::default())`，构造 5 个块（每个 ~per_block 上限），打印总字节、每块字节、被丢块 tag、以及渲染里的丢弃/截断提示（单测 `per_block_truncation_is_visible` 已证明提示存在，但**没有**五块压力读数） |

## ③ findings（needs_revision）

| id | severity | problem | requiredFix |
| --- | --- | --- | --- |
| F-t281-01 | low | t260 的产物（知识/wiki 命中进入上下文）**没有端到端运行时测试**：知识/wiki 块只在 `crates/memory/src/inject.rs` 的单测里被断言（`:620-622`），而唯一的真 run `context_injected` 测试（`crates/mock-agent/tests/e2e_daemon.rs:1014`）只断言 memory 块 | 在 `e2e_daemon` 里加一条用例：写入 1 篇知识文档 + 1 个 wiki 页后驱动一次 run，断言回显/transcript 里出现 `<knowledge>` 与 `<wiki>` 块头 |
| F-t281-02 | low | `render_injection` 生产调用点为 **0**（只剩定义 `inject.rs:399` 与它自己的单测 5 处）⇒ 生产路径上的死表面；它已是 `render_context` 的薄包装（`:404`） | 要么删掉它、把 5 条单测改指 `render_context`，要么在注释里写明为何保留（例如外部调用方契约） |

## ④ 结论

**needs_revision** —— 原因是**本单验证未完成**（第 1 条的 knowledge/wiki 运行时读数、第 2 条、第 3 条缺独立测量），**不是**对 t260 的否证：我已验的部分（第 4 条 grep、知识/wiki 块的渲染单测、真 run 的 context_injected）都与 t260 的声明一致。建议把本单以**同一 reviewedTaskId 重新派发**，或按 ② 的配方直接指派给下一位验证者（配方里的每一步都是仓外探针 + 临时 root，不需要守护进程、不写活库）。

## ⑤ 纪律

未改 crates/ · 未调用 `/api/v1/recall`（本单 0 次）· 未启停守护进程 · 未写活库 · 只 add 本报告 · 未 push · 清理：上一单中断留下的 `C:/tmp/t289` 临时树已删（删前按命令行核对孤儿 = 0 个属于我）· 用到的测试全是临时 root。
