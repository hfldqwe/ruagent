# t291 复验 t260（第 2 轮）：知识/wiki 块的运行时读数 —— 结论 **pass**

验证者：ui-audit · 任务 t291（verification round 2）· reviewedTaskId = t260
仪器：作者留在 `C:/tmp/t260/t260_probe.rs`（541 行，4 个 tokio 测试），**放回** `crates/daemon/tests/t260_probe.rs` 运行，跑完删除（`crates/daemon/tests/` 现在只有 injection_e2e.rs / knowledge_api.rs / smoke.rs，其中 injection_e2e.rs 是队友新加的未跟踪文件，不是我的）。
命令：`CARGO_TARGET_DIR=D:/rust_cache cargo test -p ruagent-daemon --test t260_probe -- --nocapture --test-threads=1` ⇒ **4 passed / 0 failed**（3.89–4.45s）。mock 二进制在位（`D:/rust_cache/debug/ruagent-mock-agent.exe`，22:01）⇒ 没有静默跳过。

## ① 仪器有效性（先做，必须自己改一处输入）

| 对照 | 我改的输入 | 输出是否随之改变 |
| --- | --- | --- |
| A（弱） | 只换 `QUERY`：`where does the deploy script live…` → `whales migrating across the southern ocean` | **只有 RRF 分数变了**（`t260-src-0 0.032522→0.032787`、`wiki/t260-w0 0.030303→0.030077`），块与字符数**没变**（318/295）⇒ 该语料只有 2 篇文档，选择对查询不敏感 ⇒ **这条对照不足以证明仪器有效** |
| B（强，采用） | **换语料**：知识文档正文加标记 `# Deploy guide UI-AUDIT-CONTROL-K7` / `# Deploy notes UI-AUDIT-CONTROL-K7` | **逐字片段随之改变** ✓ 见下；字符数同步改变：chat 318→**358**、run 295→**335**、SOFTDEL before 263→**283**、after 173→**193** |

对照 B 的逐字读数（来自真 transcript 的注入块）：
```
CHAT knowledge verbatim >>>t260-runbook: # Deploy guide UI-AUDIT-CONTROL-K7
CHAT wiki      verbatim >>>wiki/t260-notes: # Deploy notes UI-AUDIT-CONTROL-K7
RUN  knowledge verbatim >>>t260-runbook: # Deploy guide UI-AUDIT-CONTROL-K7
RUN  wiki      verbatim >>>wiki/t260-notes: # Deploy notes UI-AUDIT-CONTROL-K7
```
⇒ **我的输入进入了产品的注入路径**，不是探针里写死的期望 ⇒ 仪器有效，下面的读数测的是产品。

## ② 真 chat 与真 run 各一次 context_injected（含 knowledge 与 wiki 块）

| 路径 | transcript 路径 | 块 | 字符数 |
| --- | --- | --- | --- |
| chat | `C:\Users\19410\AppData\Local\Temp\ruagent-t260-both-13888-0\data\transcripts\run-01a0de09-5ea5-7772-a79d-9c59a440787c.jsonl` | `["knowledge", "wiki"]` | **318**（budget_total=4096, per_block=1024, ratio=0.078） |
| run | `…\ruagent-t260-both-13888-0\data\transcripts\run-01a0de09-5f43-73bb-b8f5-ebb8145617ea.jsonl` | `["knowledge", "wiki"]` | **295**（ratio=0.072） |

逐字片段（同 ① 的 verbatim 行）· `KNOWLEDGE hits=2 docs=["t260-runbook", "wiki/t260-notes"]`。
**改前基线（只读真 transcript，未调 /api/v1/recall）**：`BASELINE files=146 context_injected=79 with_knowledge=0 with_wiki=0 with_entity=0 old_chat_header=54 contract_tags=25` ⇒ 与 t259 的「79 个 context_injected 里 0 个含 knowledge/wiki」**逐字一致** ✓ ⇒ 改动确实把 0 变成 2 篇命中、并让两条路径都渲染出块。

## ③ 软删一条记忆后：记忆消失、知识块仍在

```
SOFTDEL before: chat_marker=true  run_marker=true  run_blocks=["relevant_memories", "knowledge"] run_chars=263
SOFTDEL outcome=Deleted { id: 1, deleted_at: "2026-09-26T14:07:41.012352100+00:00" }
SOFTDEL after:  chat_marker=false chat_blocks=["knowledge"] chat_chars=173
SOFTDEL after:  run_marker=false  run_blocks=["knowledge"]  run_chars=173
```
⇒ 记忆标记从 true→false，**而 knowledge 块在两条路径上都仍在**（blocks 仍含 knowledge，字符数只少了记忆块那部分）⇒ **缺席来自 DELETE，不是渲染变空** ✓（同一探针同一语料，只有删除这一个变量）。

## ④ 压力：五块逼近 per_block 上限

```
RULE  chat chars=533 blocks=["knowledge","wiki"] knowledge=[src-0,src-1,src-2] wiki=[w0,w1] truncated=false dropped=false
BUDGET chat chars=3327 of total=4096 ratio=0.812 blocks=["user_profile","relevant_memories","knowledge","context_budget"] truncated=true dropped=true
       … [+2 items dropped: context budget reached]
BUDGET run  chars=3327 of total=4096 ratio=0.812 blocks=[同上] truncated=true dropped=true
```
⇒ 渲染字节数 **3327 / 4096**（0.812）、被丢的块在渲染里**具名**（`[+2 items dropped: context budget reached]`）+ `context_budget` 块出现、`dropped=true` ⇒ **不是静默丢弃** ✓。选择规则读数：knowledge 取 3 条（`t260-src-0/1/2`）、wiki 取 2 条（`w0/w1`），RRF 分数单调递减可核对。

## ⑤ 结论：**pass**

t260 的四条运行时声明（知识/wiki 进入 chat 与 run 的上下文 · 软删后记忆消失而知识块仍在 · 预算丢弃具名可见 · 选择规则=每类 top-n）在**我改过语料的仪器**上全部复现。findings（均为观察，不阻塞）：

| id | severity | problem | requiredFix |
| --- | --- | --- | --- |
| F-t291-01 | low | 仪器在 2 篇文档的语料上对**查询**不敏感（换 QUERY 只改分数不改块）⇒ 若将来只用「换查询」做对照，会得到假阴性 | 探针的对照建议固定用「换语料」；或在 boot 里放 ≥3 篇语义互斥的文档，让查询能改变选择集 |
| F-t291-02 | low | `render_injection` 生产调用点仍为 0（t281 的 F-t281-02 未变） | 删除并把单测改指 `render_context`，或写明保留理由 |
| F-t291-03 | info | t281 的 F-t281-01（t260 产物无端到端运行时测试）看起来正在被处理：`crates/daemon/tests/injection_e2e.rs` 已作为**未跟踪**新文件出现在盘上（不是我的） | 待其提交后再看是否覆盖 knowledge/wiki 块 |

## ⑥ 纪律

未改 crates/（探针是**临时**放入并在跑完后删除：`crates/daemon/tests/` 现无 t260_probe.rs）· `git status --short crates/daemon/` 只有队友的 `?? injection_e2e.rs`（非我的）· 未调 `/api/v1/recall`（0 次）· 未启停守护进程 · 未写活库（`~/.ruagent` 只被只读读过一次 transcript 目录）· 只 add 本报告 · 未 push · 测试全用临时 root（`ruagent-t260-both-<pid>-0`，探针自清）。
