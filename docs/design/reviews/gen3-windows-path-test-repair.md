# ubuntu 唯一失败用例：`gold-seed-idempotence.rs` 把 Windows 绝对路径当输入（t108）

> 状态：**完成，等 captain 推送**。改动面：`crates/knowledge/tests/gold-seed-idempotence.rs`（唯一代码改动）+ 本报告。
> `git diff --stat`：**1 file changed, 35 insertions(+), 14 deletions(-)**。
> 作者 recall，时间窗 2026-09-29T04:4x → 05:3x（+08:00）。

---

## 0 一句话

这个测试**不在验平台，而在验「逐组件包含」**；它把 Windows 字面量当夹具，于是在 Linux 上夹具退化成一个组件，判据随之失真。**改夹具，不改判据，不加 `#[cfg]`。**

## 1 改前 → 改后

**改前（CI ubuntu 原文，唯一的失败用例）**
```
test the_seed_guard_tells_a_copy_from_the_live_root ... FAILED
thread '…' panicked at crates/knowledge/tests/gold-seed-idempotence.rs:373:9:
assertion `left == right` failed: the live database itself: C:\Users\x\.ruagent\data\ruagent.db
test result: FAILED. 2 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.16s
```
```rust
let live = PathBuf::from(r"C:\Users\x\.ruagent");          // ← 平台相关夹具
… (r"C:\Users\x\.ruagent\data\ruagent.db", true, …)        // ← 四个 case 全是 Windows 字面量
common::is_inside(Path::new(path), &live)
```

**改后（平台中立夹具，四个 case 一条不少）**
```rust
let live = Path::new("home").join("x").join(".ruagent");   // 相对 + join：用平台自己的分隔符
let cases = [
    (live.join("data").join("ruagent.db"),            true,  "the live database itself"),
    (live.join("data").join("ra-t29.db"),             true,  "any file under the live root, not just the one name"),
    (Path::new("home").join("x").join(".ruagent-other").join("data").join("ruagent.db"),
                                                     false, "a SIBLING directory: component-wise containment must not confuse it (a string-prefix check would report this as inside, which is why the case exists)"),
    (Path::new("tmp").join("ra-t29-post").join("data").join("ruagent.db"),
                                                     false, "the copy this instrument uses (a different tree entirely)"),
];
for (path, inside, note) in cases {
    assert_eq!(common::is_inside(&path, &live), inside, "{note}: {path:?}");
}
```
- **为什么相对路径**：绝对前缀会把平台的**根语法**拖进来（Windows 盘符 / unix `/`），那正是让旧夹具平台相关的根；相对拼接让两平台逐组件语义**完全相同**。
- **没有 `#[cfg(windows)]`、没有 `#[ignore]`**（读数：文件里 `cfg(`/`ignore]` 零命中，仅注释里说明「故意不用」）——跳过 Linux 会把「平台分叉」变成「平台无视」，那正是要被修掉的形状。
- 被验的语义**一字未改**：`common::is_inside` = `target.starts_with(root)`（`crates/knowledge/tests/common/mod.rs:413`），即 `Path::starts_with` 的**逐组件**包含。

## 2 为什么它此前在本机一直是绿的（第 19 条一手实例）

**机制读数**（`%TEMP%\ra_t108_pathprobe.rs`，只用 `std::path`，即 `is_inside` 依赖的同一原语；host = **windows**）：
```
live          = "C:\\Users\\x\\.ruagent"                    live components    = 5
candidate     = "C:\\Users\\x\\.ruagent\\data\\ruagent.db"  candidate components = 7
Path::starts_with（组件式，is_inside 的行为）  = true        ← 这个 case 在本机成立 ⇒ 绿
string prefix（被变异的判据）                  = true
SIBLING: component-wise = false, string prefix = true        ← 对照：字符串前缀会把兄弟目录判成 inside
```
- 在 **Windows** 上反斜杠是分隔符 ⇒ 5 组件 vs 7 组件 ⇒ case ① 断言成立 ⇒ **本机绿**。
- 在 **Linux** 上反斜杠只是普通字符 ⇒ `C:\Users\x\.ruagent` 与 `…\data\ruagent.db` **各自坍缩成 1 个组件且互不相等** ⇒ case ① 期望 `true` 却得 `false` ⇒ 红，且报错文案就是 `the live database itself` —— **与 CI 原文逐字吻合**。
- ⇒ **CI ubuntu 是这个测试第一次在 Linux 上执行**。本机从来只有 Windows 这一侧在跑，而门禁**没有声明「哪一侧被跑过」**：这就是第 19 条的实例 —— 一条**局部平台**上长期为绿的门禁，其覆盖面比它看起来的小得多。**结论：跨平台断言的门禁必须显式声明「哪一侧被跑过（Windows / Linux / 两者）」**，否则「绿」只是「在我这台机器上绿」。

## 3 负控（能红，且在隔离树里做）

**隔离方式（C22 推荐路径，共享树零变异）**：`git stash create` 取快照 → `git archive` 导出到 `%TEMP%\ra-t108-pre` → **只在导出树里**把 `is_inside` 改成字符串前缀比较 → 用**独立 target dir**（`-TargetDir %TEMP%\ra-t108-target`）跑。共享树验证：导出前后 `git status --porcelain` 逐字相同、`git stash list` 为空（`create` 不入栈）、导出树内 27 MB、target 4.4 GB 跑完按精确路径删除。

**变异**（隔离树 `crates/knowledge/tests/common/mod.rs:416`）：
```rust
// t108 NEGATIVE CONTROL (isolated tree only): string-prefix instead of component-wise
target.to_string_lossy().starts_with(root.to_string_lossy().as_ref())
```

**读数（`%TEMP%\ra-t108-gates.log`）**
```
thread 'the_seed_guard_tells_a_copy_from_the_live_root' panicked at crates\knowledge\tests\gold-seed-idempotence.rs:398:9:
assertion `left == right` failed: a SIBLING directory: component-wise containment must not confuse it
(a string-prefix check would report this as inside, which is why the case exists): "home\\x\\.ruagent-other\\data\\ruagent.db"
test the_seed_guard_tells_a_copy_from_the_live_root ... FAILED
test result: FAILED. 2 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.18s
[cargo-team] exit=101 elapsed=130.1s
```
⇒ **恰好红在 case ③（兄弟目录）**，用例名与注释都在说明「这就是它存在的理由」；case ①②④ 仍通过（2 passed）⇒ 变异**只**打中了逐组件语义这一点 ✓。注意：这次红是在**改后的平台中立夹具**上产生的 ⇒ 新夹具**保留了**这条断言的判别力（这正是最关键的一条：修平台相关性**没有**把测试变弱）。
**恢复读数**：共享树从未被变异（`git status` 只有我的 inScope 文件 `M`），改后夹具在共享树上的真跑见 §4（`ok. 3 passed`）。

## 4 门禁读数（第 22 条形态）

| 门 | 读数 | 判 |
| --- | --- | --- |
| `rustfmt --edition 2024 --check crates\knowledge\tests\gold-seed-idempotence.rs` | **exit 0**（先报过 1 处 `Diff in`：`assert_eq!` 应单行；已按 rustfmt 结果改，复检 exit 0） | 通过 |
| `test -p ruagent-knowledge --test gold-seed-idempotence -Nocapture` | **exit 0**；`test result: ok. 3 passed; 0 failed; 0 ignored`；含 `the_seed_guard_tells_a_copy_from_the_live_root ... ok`（27.7s，Windows 侧） | 通过 |
| `test -p ruagent-knowledge` | **exit 0**（75.2s）；`Running` **9** 个 target + doc-tests，`test result:` **10** 行：lib `ok 35` · **gold-seed-idempotence `ok 3`** · residual-scan `ok 6` · retrieval-cjk `ok 9` · retrieval-gold-copy `ok 0/1 ignored` · retrieval-gold-live `ok 0/1 ignored` · retrieval-legs `ok 9` · retrieval-quality `ok 3` · score-kind-wire `ok 2` · doc-tests `ok 0` ⇒ `FAILED` **0**、`panicked` **0** | 通过 |
| `clippy -p ruagent-knowledge --all-targets -DenyWarnings -CleanFirst ruagent-knowledge` | **exit 0**（7.7s）；**证据 1（clean-first 真的重编了）**：`Checking ruagent-store` + `Checking ruagent-knowledge` 两行都出现（若走缓存就不会有）；**证据 2（零告警）**：输出里 `^warning`/`^error` **0 行** + `Finished dev profile … in 7.49s` | 通过 |

## 5 全仓同类扫描（精确正则，只报不改）

命令：`grep -E '"[A-Za-z]:[\\/]|r"[A-Za-z]:\\' --include=*.rs crates/ cli/` ⇒ **19 命中 / 7 文件**（`cli/` **0** 命中）。

| # | 位置 | 命中 | 类别 | 归属 / 风险 |
| --- | --- | --- | --- | --- |
| 1 | `knowledge/tests/gold-seed-idempotence.rs:350` | doc 注释里引用旧夹具 | **本单的说明文字** | 我（不是路径） |
| 2 | `mcp/src/health.rs` 191/208/209/247/254 | `r"C:\definitely-not-here-9f3a\ruagent.exe"`、`r"C:\nope-9f3a\…"` | **「必须不存在」夹具**（preflight/ping/describe 的失败路径） | mcp/integ · **低**：Linux 上坍缩成单组件反而更确定不存在；`:209` 的 `err.contains(r"C:\nope-9f3a")` 是**字符串回显**，中性 |
| 3 | `daemon/src/config.rs:969,982` | 同上 | 同 2 | integ · 低 |
| 4 | `daemon/src/config.rs:578` | `# args = ["--root", "C:/projects"]` | **文档示例（注释）** | integ · 无风险 |
| 5 | `daemon/src/sessions.rs` 1195/1200/1209/1223/1229 | JSONL 夹具 `"cwd":"C:\\work\\demo"` + 断言等值 | **往返（in==out）** | integ · **低但值得盯**：现在断言「原样回显」，若将来改成 `file_name()`/`parent()` 推导工程名，**同一夹具会在 Linux 上变成另一个答案**（`C:\work\demo` 是 1 个组件） |
| 6 | `acp/src/adapter.rs:128,129` | `resolve_program(r"C:\tools\some-agent.exe")` + 等值断言 | **往返** | integ/acp · 低；同 5 的「将来会变味」风险 |
| 7 | `acp/src/lib.rs:111,134` | `"D:/rust_cache/debug/ruagent.exe"` | **构建路径字面量（正斜杠）** | integ/acp · 低（正斜杠两平台都可解析） |
| 8 | `daemon/src/api.rs:4856` | `PathBuf::from("C:\\")` | **被 `if set_current_dir(&second).is_ok()` 守卫** | integ · **（唯一新发现）在 Linux 上 `C:\` 不是目录 ⇒ 该分支静默不执行 ⇒ 「答案不会随 cwd 移动」这条断言在 Linux 上从未被验过**（测试仍然绿）。属「静默平台跳过」形状，建议后续单：把第二工作目录换成平台中立路径，或在跳过时**显式声明**「linux 上未覆盖」 |

**为什么只有第 1 条真的红了**：这 19 处里，只有它把**断言的真值**建立在组件边界上（「包含」关系）。其余是「必须不存在」（坍缩只会更真）、「原样回显」（与分隔符无关）、注释/正斜杠路径。⇒ **同类硬编码的真正判据不是「有没有 `C:\`」，而是「断言的真值是否依赖宿主的分隔符语义」**；建议把这条写进平台门禁的口径（命中项只报不改，均已在上表归属）。

## 6 不覆盖什么（第 19 条）

1. **改后夹具的 Linux 侧我没有真跑**（本机是 Windows）：Linux 行为是**分析必然**（`join` 用平台自己的分隔符 ⇒ 两平台都是 3 组件 vs 4/5 组件，组件式包含语义相同）+ 旧夹具的 CI 原文提供失败侧的经验证据 ⇒ **确认它的是下一次 CI ubuntu 真跑**。本单的 `test -p ruagent-knowledge` 与负控都是 **Windows 侧**读数。
2. **负控是在隔离树里做的**（按契约推荐）：它证明「变异的判据 ⇒ case ③ 红」，但**不**证明「共享树上任何别的调用者未被影响」——`is_inside` 只在测试公共模块里被这个文件与 `common::assert_not_live` 使用（后者在真写库前拦一次，见 `common/mod.rs:403`），本单未改动它。
3. **上一节 8 类命中只报不改**：本单只动 1 个文件（inScope），其余 18 处未改、未验证其在 Linux 上的真实行为（其中 2 处我给了「将来会变味」的风险判断，1 处给了「Linux 上静默跳过」的发现）。
4. **`cli/` 未扫出命中**，但 `cli/` 无主（AGENTS.md 已记），本单不涉及。

## 7 纪律回执

- 写入集合：`crates/knowledge/tests/gold-seed-idempotence.rs` + 本报告（**都在 inScope**）。`git status` 里我的条目只有这一个文件；其余 `M`（`crates/store/**` 等）是同伴在途工作，未碰。
- **未** push / dispatch / rerun / cancel / tag；**未**写活库、**未**启停守护进程（pidfile 仍 `79984 …`，与开工前一致）。
- **活库读数（引用需带时间点）**：开工前 `~/.ruagent/data/ruagent.db` mtime = **2026-09-28T21:56:30**；收尾时 = **2026-09-29T04:54:56** ⇒ **它动了，但不是我**：我的所有测试走自建临时 root（`scratch_root()`），两个碰活库的 instrument（`retrieval-gold-live` / `retrieval-gold-copy`）在本轮读数里明确是 `0 passed; 1 ignored`（**没跑**），且 `common::assert_not_live` 就是拦这件事的守卫；变化来自那个一直在跑的**真守护进程 pid 79984** 自己写库。
- 临时残留（按精确路径自清）：**已删** `%TEMP%\ra-t108-pre`（27 MB 导出树）· `%TEMP%\ra-t108-target`（4,410 MB 独立 target）· `%TEMP%\ra-t108-pre.zip`（20 MB）；**保留**（证据）：`%TEMP%\ra-t108-gates.log`（16 KB，负控 + 两门全量）· `%TEMP%\ra-t108-win.log`（13 KB，Windows 侧目标测试）· `%TEMP%\ra_t108_nc.py`（驱动）· `%TEMP%\ra_t108_pathprobe.rs/.exe/.pdb`（机制探针）。仓库内**零**临时文件。
