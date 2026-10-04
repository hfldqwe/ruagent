# t152 把 Rust 工具链钉到 1.95.0（mem-core）

- 任务：t152（kind=repair，来源 = 用户 2026-10-04 的决定「钉到具体版本」）；attempt `ab8a9ea0-a92c-420c-a949-8b839ba69a8f`
- 一句话：三个载体都钉到 **`1.95.0`** 且**逐字相同**；本机已证明**文件就是决定编译器的那一处**；三道 Rust 门在钉住的工具链上全绿；代价与升版本路径写在 §6，未验证的两处写在 §7。

## 1 改了什么（只有这三个载体）

| 载体 | 改前 | 改后 |
| --- | --- | --- |
| `rust-toolchain.toml` | `channel = "stable"`（2 行） | `channel = "1.95.0"` + 注释块：钉住的实测版本、为什么钉、**升版本要改哪几处**、**「本机需先装」那一行**（captain 要求）、以及挡不住时的 per-process 出路 |
| `.github/workflows/ci.yml` `rust-linux`（`dtolnay/rust-toolchain`，原 52 行） | 注释 `# stable (branch head 2026-09-03; the TOOLCHAIN still floats -- see the report)`；`with:` 只有 `components` | 注释写明 SHA 保持 pin + 「action 从 @rev 取工具链，所以本输入才钉住 runner 的**安装**」；`with:` **新增 `toolchain: "1.95.0"`** |
| 同文件 `rust-windows`（原 297 行） | 同上（同一条浮动注释） | 同上，并写明「与 rust-linux 及 `rust-toolchain.toml` **同一个 pin**，否则两边的绿是**不同工具**的绿」 |

其它一律未动：**job 的 `if:` / `runs-on` / timeout / matrix 逐字未变**，步骤数仍 **`rust-linux` 13 / `rust-windows` 9 / `panel` 6**（PyYAML 读数，见 §4）。

## 2 「CI 会不会跟着变」：先验证，再决定写不写显式输入

**查到的原文**（`dtolnay/rust-toolchain` README，2026-10-04 取，<https://raw.githubusercontent.com/dtolnay/rust-toolchain/master/README.md>）：
> "The selection of Rust toolchain is made based on the particular **@rev** of this Action being requested. For example "dtolnay/rust-toolchain@nightly" pulls in the nightly Rust toolchain, while "dtolnay/rust-toolchain@1.89.0" pulls in 1.89.0."
> "**Important: the default is to match the @rev** as described above. When passing an explicit `toolchain` as an input instead of @rev, you'll want to use "dtolnay/rust-toolchain@master" as the revision of the action."

⇒ **这个 action 不读仓内的 `rust-toolchain.toml`**（工具链由 `@rev` 决定）⇒ 只改文件**不足以**钉住 runner 装什么 ⇒ **必须**显式写 `with: toolchain:`（已写）。
**但两个载体各自钉的是不同的东西，这一点是本单的核心**：

- **文件钉的是「命令跑什么」**：`rustup` 的目录覆盖优先于已安装的默认工具链。**本机读数（这行就是机制本身）**：
  - 改前：`rustup show active-toolchain` ⇒ `stable-x86_64-pc-windows-msvc **(overridden by 'C:\Users\19410\Documents\ai\ruagent\rust-toolchain.toml')**`
  - 改后：`1.95.0-x86_64-pc-windows-msvc **(overridden by '…\rust-toolchain.toml')**`
- **输入钉的是「runner 装什么」**：CI 里 `cargo` 仍在同一棵树里跑 ⇒ 即使输入被忽略，文件仍决定编译版本（代价只是 runner 多装一份 `stable`，并可能报 "Unexpected input" 警告）。**这不是狡辩**：它是「哪一处是承重的」的如实分配，也是 §7-② 那条未验证项的兜底。

## 3 为什么写 `1.95.0` 而不是带日期/哈希的完整标识

`channel` 接受的是 **rustup 的 channel/toolchain 说明符**（`stable` / `nightly` / `1.89.0` / `nightly-2025-01-01`），**已发布的 `1.95.0` 是不可变的**，所以 `1.95.0` 本身就是精确的钉；`1.95.0 (59807616e 2026-04-14)` 不是合法的 channel 值，**日期与 commit 写进注释**做溯源（这也是本仓记录 SHA pin 的一贯写法）。
实测值（本机、钉住后）：`rustc 1.95.0 (59807616e 2026-04-14)` · `cargo 1.95.0 (f2d3ce0bd 2026-03-21)` · `rustup 1.29.0`。
**顺带证明这不是卫生问题而是活的风险**：`rustup check`（2026-10-04）⇒ `stable-x86_64-pc-windows-msvc - update available: 1.95.0 (59807616e 2026-04-14) -> **1.99.0 (b940084d7 2026-09-28)**` ⇒ 浮动的话，**明天的一次 CI 就会用 1.99.0**，而 `clippy -D warnings` 会把新 lint 变成错误 ⇒ 同一个提交换一天可能红。

## 4 本地证明（三道门 + 形状 + 门禁）

| 命令 | 读数 |
| --- | --- |
| `cargo fmt --all --check` | **exit=0**，**0 行输出**（18:01:5x，钉住的工具链上） |
| `scripts/cargo-team.ps1 clippy --workspace --all-targets -DenyWarnings` | **exit=0**（18:02:16 → 18:02:30，14.2s；尾部含 `Checking ruagent-daemon` / `Checking ruagent` ⇒ 真查） |
| `scripts/cargo-team.ps1 test --workspace` | **exit=0**（18:02:38 → 18:05:51，3m13s）：**74 个 `test result` 行全部 `ok`，672 passed / 0 failed / 16 ignored**（16 ignored 与 t144 的 manifest 逐字对上，是个交叉印证） |
| 形状复核（PyYAML） | `rust-linux` 13 步 / `rust-windows` 9 步 / `panel` 6 步；`if`/`runs-on`/timeout 逐字未变；两个 carrier 的 `toolchain` 输入都是 `'1.95.0'` ⇒ `EVERY toolchain carrier says: {'1.95.0'} => SAME` |
| `bash .github/workflows/scripts/check-workflow-refs.sh`（本单 verify） | **exit=0**（14s）：`workflow path references checked: 22, not tracked/missing: 0` + 四份 workflow 全 `parses as YAML (PyYAML)` |

**captain 要求后的「钉版复核」（2026-10-04 18:06:58 → 18:08:14，同一条 shell、`$env:RUSTUP_TOOLCHAIN` 实测为空）**：

| 先打出的身份 | 读数 |
| --- | --- |
| `$env:RUSTUP_TOOLCHAIN` | `''`（**空** ⇒ 没有旁路） |
| `rustup show active-toolchain` | `1.95.0-x86_64-pc-windows-msvc (overridden by '…\rust-toolchain.toml')` |
| `rustc -V` / `cargo -V` / `rustfmt --version` / `cargo clippy --version` | `1.95.0 (59807616e 2026-04-14)` / `1.95.0 (f2d3ce0bd 2026-03-21)` / `1.9.0-stable (59807616e1 2026-04-14)` / `0.1.95 (59807616e1 2026-04-14)` |
| `cargo fmt --all --check` | **exit=0**，0 行 |
| `clippy --workspace --all-targets -DenyWarnings` | **exit=0** |
| `test --workspace` | **exit=0**，74 个 `test result` 行全 `ok`，**672 passed / 0 failed / 16 ignored** |

**诚实标注**：这次复核里 clippy 只花 **2.7s**、输出**没有 `Checking` 行** —— 因为**字节与编译器都没变**，cargo 的指纹命中了，它**本次没有重新 lint**；真正做过检查的是 18:02:16 那次（14.2s，含 `Checking ruagent-daemon`/`Checking ruagent`），两者跑的是同一份字节（下面的锚点证明没有文件移动）。`test --workspace` 这次是**真跑**（76s 内 74 个 target 全过）—— 它不受「指纹命中」影响，因为测试二进制总是要执行。

**读数归属（t20 形状）**：跑三道门前后各取一次四个文件的 sha256，**逐字节相同** ⇒ 没有文件在门运行期间移动：`rust-toolchain.toml` `240f110a…` · `ci.yml` `9d1c06ff…`（30773 B）· 同期树里还有同伴在途的 `crates/daemon/src/distill.rs` `aa018d6e…` 与 `registry.rs` `9932c493…`。**这两份是 graph 的 t154 落地方案（+39/−2、+25/−8，我读了 diff 确认是带理由的修复，不是变异窗口）**；我跑的三道门认证的是「树当时的样子」，包含它们。

## 5 我踩到的环境故障（如实写：原因不是「钉版有误」）

时间线（都是实测，含 captain 的一条**自我更正**）：
1. 我先按纪律显式安装：`rustup toolchain install 1.95.0 --profile default`（我只带了 loopback 的 `NO_PROXY`）⇒ 下载完 6 个组件后在收尾处 **失败**：`error: component download failed for rust-std-x86_64-pc-windows-msvc: could not rename 'downloaded' file from '…\.rustup\downloads\…partial' to '…': (os error 2)` + `rolling back changes`。**成因我只敢说没establish**：同一份 `~/.rustup/downloads` 上有**两个安装者**竞争（captain 的修复日志里也印着 `removing previous version of component clippy`），而且同一个 `os error 2` 在别处也出现过（见下）。
2. 于是本机出现**目录在、manifest 缺**的半个工具链；而 `rust-toolchain.toml` 的 pin 让 rustup **选中**它 ⇒ 树里任何 `cargo`/`rustc` 都报 `error: Missing manifest in toolchain '1.95.0-x86_64-pc-windows-msvc'` ⇒ **全队被挡约 20 分钟**（captain 独立观测到同一现象）。**这是「自动安装没完成」，不是「钉版有误」**：pin 之后的正确形态就该是这个工具链，缺的是那次安装的收尾。
3. **网络**：rustup **和 curl/git 一样读那台机器上的死代理 `127.0.0.1:7890`** —— captain 第一次不带 `NO_PROXY` 的安装**超时 600 s 且无输出**，带 `NO_PROXY='*'` 才走通。⇒ 本机的网络故障**不止影响 `git`/`gh`**，也影响 **rustup**（本代此前只记了前两者的形状）。
4. **captain 的自我更正（我按他的原文记，因为它本身是 C40 家族的实例）**：他先前说「带 `NO_PROXY='*'` 走通、**退出码 0**」，实测不对 —— 真实读数是工具链那行 `installed - rustc 1.95.0 (59807616e 2026-04-14)` **装成功**，随后 **rustup 自更新**（1.29.0 → 1.29.1）打印 `error: unable to run updater: … (os error 2)` 并以 **exit 1** 结束。**教训与 C40 同族：「谁退出了」与「退出的是哪一段」要分开读** —— 他把 harness 报的 job 退出码当成了命令的退出码。这条对我是**可复用的**：本报告里所有 exit code 我都用 `Tee-Object … | Out-Null` 在管道前取，就是为了不让同一件事发生在我的读数上。
5. 修复后我在 **18:01:41** 读到健康读数：`1.95.0-x86_64-pc-windows-msvc (active)` + `rustc 1.95.0 (59807616e 2026-04-14)`，exit 0；随后三道门都在这个钉版上跑（§4 的时间戳）。
6. **我没有使用 `RUSTUP_TOOLCHAIN=stable` 那条出路**（captain 批准的 per-process 捷径）：我第一次 post-pin 的 `rustc -V` 就是健康的钉版读数。为把这一点钉死，captain 要求后我又在**同一 shell** 里（`$env:RUSTUP_TOOLCHAIN` 为空）先打 `rustup show active-toolchain` 再跑三道门，读数在 §4 第二张表。

**归因（不替自己淡化）**：这次全队阻塞的**触发条件就是我的 pin 指向一个尚未装好的版本**；失败的那次安装是我的，修复是 captain 的。承重教训已按 captain 要求写进 `rust-toolchain.toml` 的注释：「**本机需先 `rustup toolchain install 1.95.0 …`**，否则整棵树跑不起来；挡不住时用 per-process `RUSTUP_TOOLCHAIN=stable`，**不要**改仓库文件」。

## 6 换来什么 / 付出什么 / 以后怎么升

- **换来**：同一个提交在不同日期不再可能得到不同的绿/红 —— 编译器身份从「跑的那天」变成「提交里写的字符串」；CI 两个 runner 与本地三处**同一个值**，而它们**都在同一个 diff 里**，所以「本地绿、CI 红因为工具链不同」这一类不可能再悄悄发生。
- **付出**：① 不再自动获得新 stable 的修复与新 lint（这是用户明确选择的代价）；② 升版本是一件**要人做**的事：**3 处 + 1 条安装命令**（`rust-toolchain.toml` 的 `channel`；`ci.yml` 两处 `toolchain:` 输入；每台机器先 `rustup toolchain install <版本>`）——owner = **提出升版本的人**（本代是 captain 指派；若无人指派，就是下一个需要新 lint/修复的人）；③ 钉住一个**本机没装**的版本会让整棵树暂时跑不起来（§5），所以「改文件」与「装工具链」要**同一件事**里做。

## 7 未测 / 未覆盖（点名）

1. **CI 本身没跑**（不许 push/dispatch）⇒ 「CI 上装的是 1.95.0」是**由文件的输入 + 该 action 的文档推出来的**，不是我读到的；要观测需要一次推送后的 run（看 `rustup show` 或 `rustc -V` 那一步的输出）。
2. **那个 SHA 上的 action 是否真的认 `toolchain:` 输入，我没有验证成功**：raw 取该 SHA 的 `action.yml` 两次超时、HTML 视图只回来导航栏 ⇒ **未读到**；README 只说「显式输入要用 `@master`」。⇒ **风险与兜底都写明**：若该 SHA 忽略输入，GitHub 会给 "Unexpected input" 警告、runner 多装一份 `stable`，而**文件仍决定编译版本**（§2 的机制读数）；若要保持「action 也绝对不浮动」，替代形态是把 ref 换成 `@1.95.0` —— **但那会放弃 SHA pin**（本仓所有 action 都 SHA-pinned），所以我没有替仓库做这个取舍。
3. **windows runner 的安装**：两个 carrier 都写了同一个值（形状读数），但 windows job 只在 main push 上跑 ⇒ 未观测。
4. `Swatinem/rust-cache` 的 key 会随工具链变化（缓存失效一次）——**未测量**，只是提醒下一个看到冷构建的人。
5. 本单**没有**跑 `--ignored` 那 16 条（t144 的 manifest 已说明它们各自的处境）；`672 passed` 是**默认跑**的读数。

## 8 残留

- 改动：`rust-toolchain.toml`、`.github/workflows/ci.yml`、本报告。`crates/**`、`panel/**`、`scripts/**`、其余三份 workflow 一行未动。
- 未 push / dispatch / rerun / cancel / 建 tag；未碰活守护进程。
- 复核材料：`%TEMP%\ruagent-t152\`（`install.log`（失败的那次，含 rename 错误原文）、`clippy.out`、`test.out`（672 passed 的逐 target 行）、`gate-log.txt`（三次运行的时间戳与 exit code）、`check_pin.py`（三 carrier 同值 + 形状复核））。

---

## 9 修订记录 R-1（2026-10-04，t152 交回之后；**纯追加**）

**为什么追加**：§7 里有两条只有**流水线**能回答的问题（② action 是否认输入；④ rust-cache 代价），t152 交回时都还是未测。captain 随后拿到了**真跑读数**（run `37194596072`），按本仓惯例把答案记进来，而不是让悬着的那句话留在记录里。**这是一次文档追加**：两个代码载体一字节未动 —— 追加前重取 sha256，`rust-toolchain.toml` 仍是 `240f110a…`、`ci.yml` 仍是 `9d1c06ff…`，**与 §4 三道门认证的那份字节逐字相同**（2026-10-04T18:36:12+08:00），所以 §4 的绿读数仍然描述当前字节。

### R-1a §7-② 结案：输入**被认了**（真跑读数，不是推断）

**旧文字（逐字，引自 §7-2）**：「**那个 SHA 上的 action 是否真的认 `toolchain:` 输入，我没有验证成功**：raw 取该 SHA 的 `action.yml` 两次超时、HTML 视图只回来导航栏 ⇒ **未读到**；README 只说「显式输入要用 `@master`」。」
**答案（captain 转来的 runner 原文，run `37194596072`；不是我的读数，归属写清）**：
```
toolchain: 1.95.0
##[start-action] Run rustup toolchain install 1.95.0 --component rustfmt --component clippy --profile minimal --no-self-update
info: syncing channel updates for 1.95.0-x86_64-unknown-linux-gnu
  1.95.0-x86_64-unknown-linux-gnu installed - rustc 1.95.0 (59807616e 2026-04-14)
##[start-action] Run rustup default 1.95.0  ⇒ default toolchain set to 1.95.0-x86_64-unknown-linux-gnu
```
⇒ **没有 `Unexpected input` 警告**；runner 装的是 `1.95.0`，其 `rustc` 与本地**逐字同一个 commit**（`59807616e 2026-04-14`）；`Format` 在该 runner 上 **✓**，`Rust (ubuntu)` / `Rust (windows)` / `Panel` 三个 job **全 success** ⇒ **钉版没有把 CI 弄坏，两个载体在 CI 上也一致了**（action 装 1.95.0 + 文件让命令用它）。§7-② 那条「替代形态 `@1.95.0`」**不再需要**（它要求放弃 SHA pin，现在没有理由付那个代价）。

### R-1b §7-④ 升级：从「未测量」到「**已测量**，归因待下一次跑判定」

**旧文字（逐字，引自 §7-4）**：「`Swatinem/rust-cache` 的 key 会随工具链变化（缓存失效一次）——**未测量**，只是提醒下一个看到冷构建的人。」

**实测（captain 给的两份 run 对照）**：

| job | 钉版前 `37192592194` | 钉版后 `37194596072` | 差 |
| --- | --- | --- | --- |
| **Rust (ubuntu)** | **281s** | **1183s** | **+902s（4.2×）** |
| Rust (windows) | 658s | 1125s | +467s |
| Panel | 31s | 39s | +8s |
| **整 run** | 11m2s | **19m54s** | +8m52s |
| E2E | 4m5s | 8m4s | +3m59s |

**机制（captain 明说他没有证据 ⇒ 我按纪律标成【未证实的假设】，不写成结论）**：最可能是 **`rust-cache` 的 key 随工具链版本变化 ⇒ 命中失败 ⇒ 依赖被整体重建**（工具链换了，`target/` 里旧编译器的产物作废），**叠加每个 job 各装一次工具链**。
**判据（下一次推送就能判）**：若下一次 run 回到 ~4–5 分钟 ⇒ **一次性**（新 key 已建立，缓存重新装上）；若仍在 ~19 分钟 ⇒ **每次跑的固定代价** ⇒ 那时才值得讨论别的形态（把工具链纳入 cache / 只在需要时钉 / 别的取舍）。
**给用户的一句话权衡**：这次钉版**用 CI 时长换可复现性** —— 目前看到的是**首次**缓存重建的代价（+8m52s，ubuntu 4.2×），**是否变成常态由下一次 run 判定**；若它成为常态，代价就从「一次性」变成「每次多 9 分钟」，届时需要重新取舍（而**现在还不该**按一次冷启动就回退：回退等于把 §3 里那条「新 stable 的新 lint 能让零改动提交变红」的风险再放回来）。
