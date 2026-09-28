# gen3 测量面审计（只读）— 静默跳过 / 恒真断言 / 陈旧期望 / CI 覆盖缺口

> 任务：t70。范围：全仓 `crates/**`、`cli/`、`panel/`、`scripts/`、`.github/workflows/`。
> **本单只读**：我没有改 `crates/`、`cli/`、`panel/`、`scripts/`、`.github/` 的任何字节；我唯一写入的文件就是本报告。
> 审计者：verify（独立验证员，不是这些文件的作者）。
> 计数纪律：每条清点都写明**工具与开关**；「读到的」与「猜的」分开（§4）。
> 归因：本单在一次并发波次里做（`git status --porcelain` 期间 66→112 项在动）。**凡是引用 `Cargo.toml` 之类的同名文件我都用全路径复核**——我自己先用 `Select-String` 的 `Filename`（只是 basename）把 `crates/daemon/Cargo.toml` 的 `smoke` 特性误记成根 `Cargo.toml`，已用全路径改正（见 §3.5）。
> 时间窗：2026-09-29T02:40–03:10+08:00；`HEAD=0a39e5b801`（本代绝大多数字节未提交，这本身是 F1 的核心）。

## 0 结论（按价值排序，全部有读数）

**这一仓的「测量纪律」文本（ci.yml 头注释、AGENTS.md、各报告）已经写得比实现跑得远。**
三个最值钱的事实：

1. **F1（high）测量硬化根本没进 tracked 树，而且依赖一个未跟踪脚本。** 提交版 `ci.yml` = **110 行 / 4,353 B，`test-evidence` 命中 0**；工作区版 = **357 行 / 18,998 B，5 处调用**（`ci.yml:111/131/243/264/274`）。提交版 `e2e.yml` = 96 行，**`Entry-point` 命中 0**。而 `bash .github/workflows/scripts/test-evidence.sh` 指向的脚本 **`git ls-files` 里不存在（未跟踪）**。⇒ 一个 fresh checkout 跑的是**旧 CI**（没有逐 target 计数、没有 ignored 清单、没有 coverage declaration、没有 skip 闸门）；而如果把工作区照原样提交，evidence 步骤会因为找不到脚本直接红。
2. **F2（high）提交版 CI 的 Windows 测试步骤把首次失败吞掉了**：`cargo test --workspace || cargo test --workspace`（提交版 `ci.yml`）——重试通过即绿，且**没有任何 attempt 记录**。工作区版已换成"Test attempt 1/2 + evidence"，但那只活在未提交的字节里。
3. **F6（medium）两条恒真断言就长在「证伪性」测试里**：`crates/store/src/migrations.rs:1285-1286` 是同一个绑定的自比较（`assert!(distill_rows >= distill_rows)` / `assert_eq!(distill_rows, distill_rows)`），注释写着本意是 after/before。全仓**只有这两条**自比较（我做了精确清点，见 §2 F6）。

其余 9 条见 §2；**已查但确认不是缺陷**的 5 项见 §3（含 t53 踩过的 `npm run check` 陷阱、`e2e_daemon.rs` 那条**已经被修好**的陈旧期望、`recall.spec.ts` 的条件跳过）。

## 1 方法与可复现仪器

| 仪器 | 命令 | 产出 |
| --- | --- | --- |
| 清点 1（跳过/恒真/字面量） | `python %TEMP%\t70-census.py` | §2 F6/F8、20 个 panel `test.skip` 点、15 个 `#[ignore]`、4 个 `env::var` 门 |
| 清点 2（引用关系/env/schema） | `python %TEMP%\t70-census2.py` | §2 F3/F7、脚本→调用者矩阵、env 变量归属 |
| tracked 对照 | `git show HEAD:.github/workflows/ci.yml \| grep -c test-evidence`；`git ls-files --error-unmatch .github/workflows/scripts/test-evidence.sh` | §2 F1/F2 |
| 逐行读数 | `Select-String -CaseSensitive` / 直接读文件（行号见每条 finding） | §2 每条 |
| 未完成 | `cargo test --workspace -- --ignored --list`（想复现 ci.yml:152 的 `found`）**在 26 分钟窗口内没跑完**（共享 cargo 锁被并发波次持着，包装脚本一直等），我 kill 了自己的那个 job | §5 未覆盖项 |

**我读到的**（可复现）：上表前三行 + §2 每条的行号；**我引用的"作者/文档声称"**都在同一条里标出来源，不当作我的读数。

## 2 findings（12 条，每条：file:line · 复现 · 证据 · 可证伪判据 · owner）

### F1（high · 声称的能力 vs 实际）测量硬化只活在未提交字节里，且启动脚本未跟踪

- **file:line**：`.github/workflows/ci.yml`（提交版 110 行 vs 工作区 357 行）、`.github/workflows/scripts/test-evidence.sh`（整文件）、`.github/workflows/e2e.yml`（提交版 96 行）。
- **复现**：
  ```bash
  git show HEAD:.github/workflows/ci.yml | wc -l                        # 110
  git show HEAD:.github/workflows/ci.yml | grep -c test-evidence         # 0
  grep -c test-evidence .github/workflows/ci.yml                         # 5
  git ls-files --error-unmatch .github/workflows/scripts/test-evidence.sh # 未跟踪 -> 非零退出
  git show HEAD:.github/workflows/e2e.yml | grep -c 'Entry-point'        # 0
  grep -c 'Entry-point' .github/workflows/e2e.yml                        # 1（工作区）
  ```
- **证据读数**：提交版 `ci.yml` 的 Rust 步骤只有 `cargo fmt --all --check`、`cargo clippy … -D warnings`、`cargo test --workspace`；**没有** test-evidence 调用、**没有** ignored 清单步骤、**没有** coverage declaration（那三段注释与步骤都只出现在工作区版）。提交版 `e2e.yml` 的 Playwright 步骤是 `npm run test:e2e`（入口本身是对的），但**没有** entry-point guard、**没有** skip 证据步骤。`test-evidence.sh` 文件在磁盘上存在（2,580 B）但 `git ls-files` 不含它。
- **可证伪的修复判据**：`git show <被提交的 sha>:.github/workflows/ci.yml | grep -c test-evidence` ≥ 1 **且** `git ls-files .github/workflows/scripts/test-evidence.sh` 非空 **且** 新增一条 CI 自检：workflow 里每个 `bash <path>` / `node <path>` 引用的文件都必须在 `git ls-files` 里（今天这条自检不存在，所以"引用了未跟踪文件"可以一路绿）。
- **owner**：captain / CI owner（本单不改）。

### F2（high · 空跑绿）提交版 CI 的 Windows 步骤 `cargo test --workspace || cargo test --workspace` 吞掉首次失败

- **file:line**：提交版 `.github/workflows/ci.yml`（`rust-windows` job 的 Test 步骤，`git show` 里唯一的 `|| cargo test` 形态）。
- **复现**：`git show HEAD:.github/workflows/ci.yml | grep -n 'cargo test'` ⇒ 三处：ubuntu `cargo test --workspace`、windows `cargo test --workspace || cargo test --workspace`。
- **证据读数**：`||` 只在 shell 层；若第一次失败、第二次通过，**步骤退出码为 0**，且日志里没有任何"attempt 1 失败过"的机械记录（工作区版为此加了 `Test attempt 1` / `Test attempt 1 evidence` / `Test attempt 2 (only for the known build race)` 三段；提交版没有）。⇒ 一个**真失败**只要重试时因为缓存/时序通过，就永久绿；一个真 flake 也永远不被计数。
- **可证伪的修复判据**：任何 workflow 的 `run:` 里不含 `|| cargo` 形态（或：重试必须把第一次的退出码/摘要写进 `$GITHUB_STEP_SUMMARY` 并让"第一次失败过"成为可见读数）；自检：`grep -rn 'cargo test.*||' .github/workflows/` 为空。
- **owner**：CI owner。

### F3（medium · 静默跳过）`env::var` 门控的**测量分支**不进 skip 记账（`#[ignore]` 清单管不到它）

- **file:line**：`crates/knowledge/tests/retrieval-quality.rs:927`（`let real = std::env::var("RUAGENT_T245_REAL").as_deref() == Ok("1");` → `run_once("a", real)`）；同文件 `:933`（`CARGO_TARGET_DIR` 只决定产物落哪，良性）；三个 `expect()` 型（`live-after.rs:39`、`retrieval-gold-copy.rs:41`、`retrieval-gold-live.rs:33`）**会大声失败**，良性。
- **复现**：`python %TEMP%\t70-census2.py`（env 归属段）；或
  `grep -rn 'RUAGENT_T245_REAL' --include='*' -l .` ⇒ **只有 2 个文件**：该测试 + `docs/design/reviews/gen2-recall-spec.md`；`.github/`、`scripts/` 里 **0 次**。
- **证据读数**：默认（CI、本地裸跑）`real=false` ⇒ 用 hash embedder 跑；**真 e5 那一侧没有任何门禁/脚本会跑**（它既不是 `#[ignore]`，也没有 CI 分支）。ci.yml:155-162 的清单算术是 `covered=7 + declared=8 = 15`（对应 15 个 `#[ignore]` **属性**，我用清点 1 逐条数过：injection_e2e 7 + live-after 3 + retrieval-gold-copy 1 + retrieval-gold-live 1 + store/src/lib.rs 1 + migrations 2）——**这个门**只覆盖 `#[ignore]`，env 门在门外。
- **可证伪的修复判据**：每个"改变被测对象"的环境变量必须出现在清单里（跑 / 声明不跑 + 理由），或该分支必须打印一行机器可核对的 skip 形状（含变量名与取值），并让清单的 `found` 把它算进去；反例检查：把 `RUAGENT_T245_REAL=1` 加入 CI 后 `retrieval-quality` 的输出必须与默认不同（今天无人检查这一点）。
- **owner**：knowledge/recall owner + CI owner。

### F4（medium · 声称的能力 vs 实际）设计闸门只跑"定理证明器"，§12 的真正测量只在本地

- **file:line**：`.github/workflows/ci.yml:324-326`（`node tools/design-audit.mjs --self-test`）；`panel/tools/design-audit.mjs:14`（`--check` = 判 §12，失败 exit 1）、`:6595`（`--self-test` 不启浏览器）、`:8064-8070`（分支）；唯一的 `--check` 调用者 `panel/tools/audit-shards.mjs:45`。
- **复现**：`grep -rn 'design-audit' .github/workflows/ scripts/ panel/package.json` ⇒ 只有 `ci.yml:326 … --self-test`；`grep -rn 'design-audit.mjs --check' panel/tools scripts .github` ⇒ 命中 `audit-shards.mjs`（本地分片工具）与工具自身的 HELP。
- **证据读数**：CI 的 panel job 只跑 `--self-test`（作者注释也诚实：它防的是"锚点掉回内置默认"和"reconcile 漂移"两种**判据代码**的错，433 ms、无浏览器）。而 §12 的门槛（材质阶梯、文字阶梯、信号色像素预算…13 路由 × 暗/亮）**只有在 `--check`/`--json` 对着服务中的 bundle 跑时**才被测量；`ci.yml:292-310` 的 coverage declaration 写了"runtime behaviour … is the E2E workflow's job"，但**没有一句**说 §12 阈值不在 CI 里测。
- **可证伪的修复判据**：要么 e2e.yml 里（panel 已启动）加 `node tools/design-audit.mjs --check` 并在 evidence 里记 exit code + 失败行数，要么 coverage declaration 明确点名"§12 设计阈值为本地闸门"。二者都做到的检验：CI 里 `--check` 的 exit code 出现在 `$GITHUB_STEP_SUMMARY`。
- **owner**：panel / design owner。

### F5（medium · CI 覆盖缺口）Playwright 套件没有 PR 闸门；且除 `registry.spec.ts` 外任何 skip 都不失败

- **file:line**：`.github/workflows/e2e.yml:15-17`（`on: push: branches: [main]`，提交版与工作区版相同）；`.github/workflows/e2e.yml:139-171`（skip 只对 `registry.spec.ts` 报错：`:168`）。
- **复现**：`sed -n '15,17p' .github/workflows/e2e.yml`；`sed -n '139,176p' .github/workflows/e2e.yml`。
- **证据读数**：`on:` 只有 main push ⇒ **合并前**面板的唯一闸门是 typecheck+build（F4 已说设计阈值也不在），DOM/交互/写路径全部在**合并之后**才被测。套件内部：skip 计数只打印（`:152-166`），唯一失败条件是 `- .*registry\.spec\.ts` 出现（`:168`）。我自己的两次运行（t20 §3.7，两个不同 root）都出现 `- 16 e2e\recall.spec.ts:10 › conservative recall stubs…`（数据依赖跳过，本地日志 `%TEMP%\vint-e2e*.log`）——**在 CI 它同样只会是一行 skip，不会红**。
- **可证伪的修复判据**：① PR 触发一次 e2e（或契约里写明"仅 main 后验"的理由）；② skip 要有**每 spec 名**的预算：某个 spec 上一轮绿、这一轮 skip ⇒ 失败（今天只对 registry 一个名字硬编码）。
- **owner**：CI owner + panel owner。

### F6（medium · 恒真断言）`migrations.rs:1285-1286` 两条自比较断言

- **file:line**：`crates/store/src/migrations.rs:1285`、`:1286`（同文件 `:1287` 的 `assert_eq!(distinct_ids, 0)` 是真检查）。
- **复现**：
  ```bash
  grep -rnE 'assert(_eq)?!\(\s*([A-Za-z_][A-Za-z0-9_]*)\s*(==|>=|<=|!=|,)\s*\2\s*[,)]' crates cli
  # 全仓只命中这两行
  ```
- **证据读数**（我读的原文）：
  ```
  1285: assert!(distill_rows >= distill_rows);  // after >= before, 0 >= 0
  1286: assert_eq!(distill_rows, distill_rows); // after == before, 0 == 0
  ```
  两行两边是**同一个绑定**⇒**按构造必真**；注释显示本意是 `after` 与 `before`。这一段还带 `println!` 自述"每条 PRE-t33 谓词都成立"，即**它就是用来证明"谓词空转"的那半个测试**，结果自己带了两条空转断言。
- **可证伪的修复判据**：改成两个不同绑定（`assert!(after >= before)` / `assert_eq!(after, before)`）后，把 `before` 人为加 1 必须变红；今天把 `distill_rows` 任意改值都**不可能**让这两行失败。
- **owner**：store owner。

### F7（medium · 声称的能力 vs 实际）4 个 tracked 仪器全仓 0 提及

- **file:line**：`panel/tools/capture-readiness.mjs`、`panel/tools/interaction-probe.mjs`、`panel/tools/memory-kb-readings.mjs`、`panel/tools/t150-blank-control.mjs`（四个都 `git ls-files` 跟踪）。
- **复现**：
  ```bash
  for s in capture-readiness interaction-probe memory-kb-readings t150-blank-control; do
    echo -n "$s: "; grep -rIl --exclude-dir=node_modules --exclude-dir=target "$s" crates cli panel scripts .github docs | grep -v "$s.mjs" | wc -l
  done   # 全 0
  ```
- **证据读数**：四个都在 tracked 树里、都没有任何文件（代码/脚本/CI/文档）提到它们。对照：`design-audit.mjs` 被 63 个文件提到、`audit-shards.mjs`/`merge-audit.mjs`/`thresholds.mjs` 都有调用者（本地工具链）。⇒ 这 4 个是**无法被发现的仪器**（要么死代码，要么"能力"只存在于作者记忆里）。
- **可证伪的修复判据**：每个仪器必须被某个 tracked 文档/driver 以命令 + 期望读数的形式点名，或被删除；自检：上表 5 行输出全非 0（或在删除清单里）。
- **owner**：panel owner（若判定为死码则 owner = 最后一个改动者）。

### F8（low-medium · 陈旧期望的"面"）83 条把 `len()`/`count()` 钉成字面量的断言，分布在 31 个文件

- **file:line**：清单（我给的 top，不是全部）— `crates/daemon/src/sessions.rs:1051,1052,1132`；`crates/daemon/src/api.rs:5633,5634,5646`；`crates/store/src/lib.rs:917,955,993`；`crates/memory/src/inject.rs:1020,1268,1588`；`crates/mock-agent/tests/e2e_daemon.rs:694,813,1043`；`crates/orchestrator/src/lib.rs:245,267,296`；`crates/memory/src/write.rs:345,414,467`；`crates/knowledge/src/chunk.rs:256,315`；`crates/knowledge/src/files.rs:1345,1377`。
- **复现**：`python %TEMP%\t70-census2.py` 第 4 段（正则 `assert(_eq)!\([^,]*(len\(\)|count\(\)|_count|_len)[^,]*, *\d+\)`）⇒ **83 条 / 31 文件**。
- **证据读数**：这类断言的失败条件只有一个——"集合长大了"，所以它**既是守卫也是陈旧期望的温床**（本代在 `e2e_daemon.rs` 抓到的那条就是这个形状）。我**逐条读过**那条的现状：`crates/mock-agent/tests/e2e_daemon.rs:2161-2179` 已经改成读行的字段/文本（作者注释写明 t347 移除了 `[distilled]` 前缀、断言改为"行里带蒸馏文本"）⇒ **那处已经是修好的形态，不是缺陷**。⇒ 本节是"面"的读数，不是"点"的指控。
- **可证伪的修复判据**：对最可能漂移的前 ~10 条，期望值必须从**同一个真相源**导出（例：迁移数用 `MIGRATIONS.len()`、头部数用 `INJECTED_HEADERS.len()`，而不是同一行的字面量），或加注释把"增长时我要改"写清；抽查判据：任意改动集合大小的 mutation 必须让对应断言变红（今天对导出的那部分不成立）。
- **owner**：各文件 owner（列表按文件给出）。

### F9（low · 陈旧引用）审计/backlog 用自己的行号引用自己会烂

- **file:line**：`docs/design/reviews/gen3-backlog.md:53`（写 `e2e_daemon.rs:2161` panic）；今天的 `crates/mock-agent/tests/e2e_daemon.rs:2161` 是**解释 t347 修复的注释**，断言在 `:2167-2179`。
- **复现**：`sed -n '53p' docs/design/reviews/gen3-backlog.md`；`sed -n '2161p;2167,2179p' crates/mock-agent/tests/e2e_daemon.rs`。
- **证据读数**：同一条缺陷在 `gen2-memory-verify.md:295` 被记成 `:2166`（也是旧行号）。⇒ 行号式引用在同一次修复后立刻过期，而审计条目恰恰是"下一轮去哪看"的索引。
- **可证伪的修复判据**：审计/backlog 条目引用**符号名 + 提交 sha**（或 `file:fn_name`），并在复跑脚本里按符号重定位；抽查判据：任一历史条目的 `file:line` 在今天的树上仍指向同类缺陷（或条目里带 sha）。
- **owner**：docs/audit owner（写 backlog 的人）。

### F10（low · 死守卫风险）写路径闸门挂在**文件名字面量**上，改名即失去闸门

- **file:line**：`.github/workflows/e2e.yml:168`（`grep -qE '^ *- .*registry\.spec\.ts'` ⇒ error）。
- **复现**：`sed -n '166,176p' .github/workflows/e2e.yml`。
- **证据读数**：闸门语义是"武装了 write guard 的那个 spec 必须真的跑"，但实现是认字符串。把 `registry.spec.ts` 改名（或拆成两个 spec）后：`writeAccess()` 仍然会正确 skip，而 CI 的这条 gate **同时失效**（匹配不到就通过）——正是这条 gate 存在要防的形态。
- **可证伪的修复判据**：闸门改挂在属性上（例：run-e2e 输出"write-guarded specs: N，ran: N"，CI 断言两者相等），抽查判据：把 spec 改名后 CI 仍必须红（今天不红）。
- **owner**：CI owner。

### F11（low · 恒真式守卫）ignored 清单是"总数机器核对 + 逐名散文声明"

- **file:line**：`.github/workflows/ci.yml:152`（`found=$(grep -cE ': test$' …)`）、`:155`（`covered=7`）、`:161`（`declared_not_covered=8`）、`:162`（`expected=$((covered + declared_not_covered))`）、`:180`（只比总数）。
- **复现**：`sed -n '145,184p' .github/workflows/ci.yml`。
- **证据读数**：我今天用清点 1 独立数到 **15 个 `#[ignore]` 属性**（7+8 的构成见 F3），`covered+declared = 15` ⇒ **今天对得上**，而且"新增一个 `#[ignore]` 不更新清单"会红（设计正确）。剩下的洞是：**改名/挪窝**（7 个名字里有谁换名，总数不变，机器只检查总数）与 8 个"declared"的名字只是散文，没人核对清单打印的名字与 `--ignored --list` 的名字逐一相等。我**没跑** `--ignored --list`（见 §5），所以这一条我**只给静态读数**。
- **可证伪的修复判据**：清单步骤打印**逐名** disposition 表（从 `--ignored --list` 的输出直接生成），并断言"每个 ignored 名字恰好出现在 RUN 或 DECLARED 一次"；抽查判据：把一个 ignored 测试改名不动总数，CI 必须红（今天不红）。
- **owner**：CI owner。

### F12（low · 已声明的覆盖缺口，登记以免下一代当成"漏了"）PR 从不在 Windows 上跑 Rust

- **file:line**：`.github/workflows/ci.yml:190`（`if: github.event_name == 'push' && github.ref == 'refs/heads/main'`）、`:191`（`runs-on: windows-latest`）。
- **复现**：`sed -n '186,196p' .github/workflows/ci.yml`；`sed -n '57,74p' .github/workflows/ci.yml`（ubuntu job 的 coverage declaration 第 3 条自陈 "Windows-specific paths … the `Rust (windows)` job（main pushes only）"）。
- **证据读数**：AGENTS.md 写"Windows is a first-class platform: no Unix-only assumptions"；而 PR 阶段只跑 ubuntu（+ panel），Windows 只在 main push 后跑 ⇒ **一个 Windows-only 的编译/路径错误可以合并**。这条**被 CI 自己声明了**（所以不是 silent gap），我登记它是因为"声明了"≠"有人决定过"。
- **可证伪的修复判据**：契约里为它写一行裁决（接受/加 `workflow_dispatch`/加 nightly），抽查判据：能指出裁决所在的行。
- **owner**：captain（契约层）。

### 观察（不是 finding，但读数完整）

- **O1** `scripts/cargo-team.ps1`（全队强制的编译包装：单一编译锁、CPU 0-11、`-CleanFirst`、`-TargetDir`）**未被 git 跟踪**（`git ls-files --error-unmatch` 非零），`scripts/daemon-identity.ps1` 同样；它们只被未跟踪的评审报告引用 ⇒ 本代资源纪律的**可发现性**依赖于未提交文件。
- **O2** `RUAGENT_EMBEDDER` 出现在 CI 两处（`ci.yml:37` 环境级、`e2e.yml`）——这是**被声明**的（"offline CI uses the deterministic hash embedder fallback"），不是漏洞。
- **O3** panel 有 **20 个 `test.skip(...)` 点**（清点 1），全部带理由字符串；我逐条看过最危险的两个：`recall.spec.ts:32` 是**运行时条件**（`if (stubs.count()===0) test.skip(true, …)`，不是无条件跳过）、`registry.spec.ts:22` 走 `writeAccess()` 属性（正确形态）。⇒ 不是 finding。
- **O4** `crates/daemon/tests/smoke.rs` 存在（5,140 B，`#![cfg(feature = "smoke")]` 在 `:13`），`crates/daemon/Cargo.toml:12` 声明 `smoke = []`，且它"never in CI"是被声明的（root 注释 / design §12.5）⇒ AGENTS.md 的"feature-gated smoke tests"是真的。
- **O5** 全仓只有 **2 条**自比较断言（F6 的那两条）；`assert!(x >= 0)`、`assert!(true)`、`if false`、`todo!`、`unimplemented!` 这些经典形态我扫了，**0 命中**（清点 1）⇒ 恒真断言不是"一片"，是"两点"。
- **O6** `crates/knowledge/tests/retrieval-quality.rs:933` 用 `CARGO_TARGET_DIR`（缺省落到 crate 内 `target/`）决定 JSON 落点 ⇒ 与"`-CleanFirst` 会清掉它"叠加时，读数的**持久性**依赖构建目录策略（不是测量错误，登记）。

## 3 已查但**不是**缺陷（防伪证 / 免得下一轮重复劳动）

1. **`npm run check`（panel/package.json）不是覆盖缺口**：它的两步 `tsc -b --noEmit` / `tsc -p e2e/tsconfig.json --noEmit` 与 `npm run build` 内部的步骤**同一套**（t55 已更正 t53 的历史误判）；我在 t20 两条都跑过，都 exit 0。⇒ 不再报。
2. **`e2e_daemon.rs` 那条陈旧期望已经修好**：我读了 `:2161-2179`，断言现在检查"行里带蒸馏文本"而不是旧前缀（t347 之后正文前缀被移除）。⇒ 只作为 F8 的"面"举例，不作为"点"的缺陷。
3. **`recall.spec.ts:32` 的 `test.skip(true, …)` 是无条件跳过吗？不是**：它在 `if ((await stubs.count()) === 0)` 里，是数据依赖跳过，且理由字符串完整。（但见 F5：这种跳过在 CI 里不失败。）
4. **`registry.spec.ts` 在 npm 入口下会跑**（`RUAGENT_E2E_ALLOW_WRITES=1` 由 `run-e2e.mjs` 置位），契约里"它会跳过"的期望只对裸 `npx playwright test` 成立；e2e.yml 的 entry-point guard（工作区版）正是把这件事变成持续属性。⇒ 不算缺陷，但 guard **还没提交**（F1）。
5. **`smoke` 特性/测试目标存在**（`crates/daemon/Cargo.toml:12`、`crates/daemon/tests/smoke.rs:13`）。我先前用 `Select-String` 的 `Filename`（basename）把它误记成根 `Cargo.toml`：全路径复核后确认。⇒ 记录这个教训：**同名文件的引用必须用全路径**。

## 4 未验证猜想（**不是** finding；没有读数，只有形状）

- **G1** 我怀疑 e2e 的**跨 spec 干扰**（并行 + 共享 root）会让"数据依赖的 skip"变成 flaky：t20 里 `wiki.spec.ts` 的三条在**裸根**上全绿，而同一根的 `consumption.spec.ts:77`（建一个无读数页）超时红——看起来像别的 spec 先写了页。**我没有做隔离重跑**（单 spec `--workers=1`）来定因。读数形状：`npx playwright test wiki.spec.ts --workers=1` vs 全套对比。
- **G2** 我怀疑 `ci.yml:152` 的 `grep -cE ': test$'` 对某些 target 名字会漏（例如 lib test 的名字以 `::` 结尾都能匹配，但若某 target 的 `--list` 输出带尾随空格/CRLF 就漏一行 ⇒ 总数变小 ⇒ **代数上会红**，fail-safe，所以只是猜想不是缺陷）。**未跑**（§5）。
- **G3** 我怀疑 `test-evidence.sh` 的必填名匹配 `^test ${name}( .*)? \.\.\. ok$` 对**参数化**测试名（带 `[...]` 或空格）会漏配 ⇒ 该粒度的"证明它跑了"可能过严或过松。**未跑** `--include-ignored` 复核（§5）。
- **G4** `panel/tools/repro-ci-e2e.sh` 声称复现 CI 的 e2e（工作目录 panel），我没执行它，因此"本地复现 == CI"这条**未验证**（它有 1 处引用，来自 docs）。

## 5 未覆盖范围（每条写原因）

| 未覆盖 | 原因 |
| --- | --- |
| `cargo test --workspace -- --ignored --list`（复现 ci.yml:152 的 `found`） | 共享 cargo 锁被并发波次持有：我的 job 起于 02:44:40，到审计窗口结束（03:10+）仍只有 "start" 一行；我 kill 了自己的 job（**没有**别人的进程）。F11 因此只给静态读数（15 个 `#[ignore]` 属性 vs `covered+declared=15`）。 |
| **任何 CI 的真实运行**（GitHub Actions 侧） | 我只读了 workflow 文本 + `git show HEAD:` 的提交版；**没有**CI 运行日志可读（本机不跑 Actions）。所以 F1/F2/F5 的"提交版会不会绿"是**由文本推断**，不是运行读数——严格说 F1 的"fresh checkout 跑旧 CI"是文本事实，"红"部分是推断（`bash` 找不到文件必然非零退出）。 |
| `design-audit.mjs --check` / `--json` 的实测 | 需要浏览器 + 服务中的 panel（工具自己写"no browser ⇒ self-test 分支"）；本单不做面板启动。F4 的读数是**调用关系**（谁在 CI 里跑），不是阈值是否达标。 |
| panel e2e 的**隔离重跑**（G1）与 `repro-ci-e2e.sh`（G4） | 预算/时间：本单是"清点型"审计，e2e 全套 ≥5 分钟且需要 boot daemon；我用了 t20 已产出的两次运行日志作为 e2e 数据的来源（在 §2 F5 里标为 t20 读数）。 |
| `docs/**` 的散文审计（MASTER.md 的 54 条判据是否与 §12 逐条一致） | 这是另一个量级的工作（design-audit 自己就是它的机器读数）；我只审计了"谁跑它"。 |
| Windows/Linux 运行时差异 | 本机是 Windows，无 Linux runner（F12 的作用域）。 |
| `crates/**` 里"断言是否与实现漂移"的**逐条**判定（83 条） | 逐条漂移判定需要每条的意图；我只验证了那条已知 exemplar（已修好）并给出面清单 + 判据。 |

## 6 我的足迹

唯一的写入：本文件 `docs/design/reviews/gen3-audit-instruments.md`。`crates/`、`cli/`、`panel/`、`scripts/`、`.github/` **零字节改动**（本单只读）。
辅助仪器（`%TEMP%`，不进仓库）：`t70-census.py`、`t70-census2.py`、`t70-ignored.ps1`（已 kill 的 job）、`t70-tracked.txt`。
收尾核对：真守护进程 `pid 79984 alive StartTime=09/27/2026 05:35:37`（未触碰）；我自己的临时 root 均已删除（`%TEMP%\ruagent-vint-f6` = False）；被杀 job 的日志留在 `%TEMP%\t70-ignored-list.log`（只有 start 一行，作为"未完成"的证据）。
