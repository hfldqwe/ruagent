# issue #78 决策材料：Intel macOS（`x86_64-apple-darwin`）

调研单，不改产品代码、不改 `release.yml`、不改 `Cargo.toml`/`Cargo.lock`。所有读数在本文件 §6 里逐条可复现。

标注约定：**[实测]** = 我自己跑出来/读出来的原始输出；**[二手]** = 别人给的事实（上游 issue、发布页、本仓已有评审记录），我看了原始载体但没有在本机复跑；**[推断]** = 我根据前两者 + 源码做的判断，没有独立读数。

---

## 1. 结论

**推荐维持现状：Intel macOS 继续不进发布矩阵。** 而且 issue #78 现有的三个候选方向里，**第 2 个（换一个预编译集包含 `x86_64-apple-darwin` 的 `ort` 版本）可以被判定为「已死」，不是「等一等」**——`ort` 的预编译目标集在 rc.13 之后**又发了两个版本**（rc.14、rc.15）并且到了 `main`，**三者都没有这个 target**，上游维护者还明确说了跟随 ONNX Runtime 弃掉 x64 macOS（§2.2）。所以 issue 现在挂着的「等一个产品决策」里，真正需要决策的只剩「要不要为一个正在退场的平台出降级包」，而不是「有没有一条便宜的路」。

一句话给决策者的形态：**成本最低的可行方案（§2.4 候选 A，约 3 行 Cargo）确实能让 Intel Mac 拿到一个能跑的包，但那个包里没有语义搜索，而且现有代码只会用 `hash-embedder` 这个名字说出来、不会说「为什么」；成本最低的完整方案（§2.4 候选 B，社区预编译 dylib）会把发布流水线的一个构建输入挂到**别人**的 blob 上。**

---

## 2. 三个选项

### 2.1 选项 1：该 target 编译时把 ONNX / embedder 编掉（功能降级）

**成本** —— [推断] 按现有代码统计，需要动的地方是**一个 crate 内部 + 一处启停分支**，不是全仓：

| 位置 | 现在的作用 | 编掉之后要变成什么 |
|---|---|---|
| `crates/knowledge/Cargo.toml:22`（`fastembed = "6"`） | 唯一的 `fastembed`/`ort` 声明点 | 变成可选依赖 + feature |
| `crates/knowledge/src/fast.rs`（84 行，全文） | `FastEmbedder`：`embed`/`embed_query`/`name`/`dim` | 整个模块 `#[cfg(feature = ...)]`；`name()` 返回 `fastembed:multilingual-e5-small` 的路径消失 |
| `crates/knowledge/src/lib.rs:11,18`（`pub mod fast;` / `pub use fast::FastEmbedder;`） | 导出 | 加 `cfg` |
| `crates/daemon/src/lib.rs:573-601` | 启动时 `FastEmbedder::try_new()`，失败回落 hash，并打 WARN | 变成无条件走 `Knowledge::open()`（hash），并**换一条说得出原因**的日志 |
| `crates/knowledge/tests/retrieval-quality.rs:412`、`retrieval-gold-live.rs:46`、`retrieval-gold-copy.rs:55` | 三个 harness 在 `RUAGENT_T245_REAL=1` 下调 `FastEmbedder::try_new()` | 加 `cfg`，否则编不过 |

`Embedder` trait 本身（`crates/knowledge/src/embed.rs:7-26`）**不用改**：它已经有 `is_fallback()`（`embed.rs:19`）和 `HashEmbedder`（`embed.rs:38-96`），降级所需的类型早就存在。[实测]

**收益** —— [推断] Intel Mac 会拿到一个能启动、能服务面板、能跑 agent 的包；语义搜索退化为词袋哈希（`HashEmbedder`，256 维，`embed.rs:52`）。

**风险** —— 三条，其中第一条是**实测证据**，不是担心：

1. **现有降级路径不会禁用语义腿，只会用一个无意义的向量继续跑。** `store.rs:949-950` 的语义腿只受 `LegConfig.semantic` 门控，**不查 `is_fallback()`**：hash 向量照样去 LanceDB 做 ANN。也就是说 Intel Mac 上的检索**不报错、但排序近乎随机**；唯一信号是 `/api/v1/stats` 的 `embedder` 字段和 smoke 的 `embedder in use: hash-embedder`（`.github/scripts/smoke-archive.sh:219`）。这一点本仓已有实测记录：[二手] `docs/design/reviews/gen2-recall-spec.md:41` —— hash embedder 按空白切词，**一整句无空格中文 = 1 个词 = 1 个哈希桶**。
2. **数据目录从别的平台搬过来会直接 400。** [二手] `docs/design/reviews/gen2-integration-verify.md:24,117`：`RUAGENT_EMBEDDER=hash` 打真模型表（384 维）时 23/23 全是 `400 lancedb: … query vector dimension: 256`。而 `store.rs:685-697` 在「表是真模型、当前是 fallback」时**只 WARN 不迁移**（这是对的，否则离线一次就毁掉真向量）——于是 Intel Mac 上「用户把老 `data/` 拷过来」= 每次召回 400。
3. **「能力缺失」在编译期不可见。** 降级包和完整包的功能差别不会体现在任何构建读数里，只能靠下载者自己去 `/api/v1/stats` 看。这与 `release.yml:64-68` 自己写的原则（「honestly missing 好过 download button 失败」）**方向相反**：这次是「下载按钮成功、功能静默缩水」。

**依据** —— 上面每条都给了文件/行号或本仓评审路径；`embedder in use:` 这个自述形式确实是本仓已有的先例（`smoke-archive.sh:219`，`gen2-recall-*`、`t323-memory-quality-readings.md:15` 都用它），但**先例只覆盖「说出来现在是哪个 embedder」，没有覆盖「为什么不是另一个」**——要沿用这个模式，得新写一句原因，而不是照抄 `daemon/src/lib.rs:596` 的 `fastembed unavailable (offline?)`。

### 2.2 选项 2：换到一个预编译集包含 `x86_64-apple-darwin` 的 `ort` 版本 —— **判定：没有这样的版本，且不会再等来**

**成本** —— 0（不存在可升的版本，成本无从谈起）。**真正需要记下来的是「最小升级面」长什么样**，以防将来出现：[实测]

* `ort` / `ort-sys` **不是本仓的直接依赖**：对全部 `Cargo.toml` 做词边界 grep（`\bort(-sys)?\b`）**0 命中**，以 `ort = ` / `ort-sys = ` 起头的行也 0 命中；`fastembed = "6"`（`crates/knowledge/Cargo.toml:22`）是唯一相关声明，两者都是它的传递依赖；`Cargo.lock` 把两者都钉在 `2.0.0-rc.13`（`Cargo.lock:5209-5211`、`5222-5225`）。
* 唯一声明点：`crates/knowledge/Cargo.toml:22` `fastembed = "6"`（锁到 `6.0.3`，`Cargo.lock:2597-2600`）。
* 因此**想动 `ort` 只有两条路**：把 `fastembed` 升到要求更高 `ort` 的版本，或直接加一个 `ort`/`ort-sys` 直接依赖（或用 `[patch.crates-io]`）。
* `fastembed` 的**可达上限**：`7.7.1`（2026-11-16 发布）对 `ort` 的约束是 **`=2.0.0-rc.15`（精确等于，optional）**；`7.0.0` 与 `6.0.3` 都是 `=2.0.0-rc.13`。[实测，crates.io dependencies API] 也就是说**通过 `fastembed` 能到的最远点就是 rc.15**。

**收益** —— 0：rc.14、rc.15、以及 `pykeio/ort` 的 `main` 分支，预编译目标集里都**没有** `x86_64-apple-darwin`（`dist.tsv` 逐行核过，§6 读数 2/3/4）。rc.14 相对 rc.13 唯一新增的 target 是 **`x86_64-linux-android`**——这说明上游**在加 target**，只是不加这一个。[实测]

**风险** —— 无新增；反而消掉了「再等一个 rc」这个选项，免得下次有人重开这张单。

**依据**（逐条给了载体）：

* [实测] 本机 registry 里的 `ort-sys-2.0.0-rc.13/build/download/dist.tsv`：15 行，target 列只有 `aarch64-apple-darwin`、`aarch64-apple-ios`、`aarch64-apple-ios-sim`、`aarch64-linux-android`、`aarch64-pc-windows-msvc`、`aarch64-unknown-linux-gnu`、`x86_64-pc-windows-msvc`、`x86_64-unknown-linux-gnu`。**没有任何 `x86_64-apple-*`。**
* [实测] 我从 crates.io 下的 `ort-sys 2.0.0-rc.14`（20 行）与 `2.0.0-rc.15`（20 行）两个 `.crate`，解包读同一路径的 `dist.tsv`：两份都**不含** `x86_64-apple-darwin`；新增的是 `x86_64-linux-android`。上游 ORT 版本从 `ms@1.28.0` 抬到 `ms@1.31.0`。
* [实测] `https://raw.githubusercontent.com/pykeio/ort/main/ort-sys/build/download/dist.tsv`：20 行，仍然没有 `x86_64-apple-darwin`（新增了 `openvino`/`migraphx` 等 feature_set，target 集不变）。
* [二手] `https://github.com/pykeio/ort/issues/556`（标题就是 *Build error for target x86_64-apple-darwin*，已 closed）：维护者 `decahedron1` 两次表态——「ONNX Runtime no longer supports x64 macOS as of 1.24.1」，以及「MS 停发二进制的信号读作不再想支持它……`ort` 跟随 MS 的脚步现在就把这个 target 丢掉，比等上游哪天编不动更合理」。这条是**方向性**证据：不是「还没做」，是「决定不做」。

**结论**：issue #78 的选项 2 应当从候选表里**划掉并写明原因**，而不是继续挂着等。

### 2.3 选项 3：在 workflow 里为该 target 从源码构建 ONNX Runtime

**成本** —— [推断] 无法给出可信数字，我**没有**任何一处实测时长（见 §4）。可核到的构件要求是：`cmake` + Python（ORT 的 `build.sh` 驱动）+ Xcode CLT + 一个 macOS job 里的独立步骤；`ort.pyke.io` 的链接文档给出的路径是「跑 ORT 的 `build.sh`，然后 `ORT_LIB_PATH` 指向 `onnxruntime/build/<os>/<profile>`」。**规模参照（[实测]，Windows 侧）**：本机 `%LOCALAPPDATA%\ort.pyke.io\dfbin\x86_64-pc-windows-msvc\<hash>\onnxruntime.lib` 单个文件 **341,152,186 字节**，同目录 `DirectML.dll` 18,527,776 字节，整目录 **343.0 MB**——静态链接进产品的那一层就是这个量级，源码构建的中间产物只会更大。

**收益** —— [推断] 唯一能拿到**完整能力**（ONNX 语义检索）的官方路径。

**风险** ——

1. [推断] 这是 `release.yml:254-264` 已经算过账的那类风险：`Build release binary` 在 Windows 冷跑就 **7243 s（约 2 h）**，job 上限 180 min。在同一 job 里再加一次 ORT 源码构建，会把 macOS leg 推到上限附近；而默认 `fail-fast: false` 意味着失败只是一个红 leg，不会保护你。
2. [推断] 产物体积：静态链接后 Intel Mac 包会与今天的 Windows 包同量级（[二手] `docs/design/release-pipeline.md:152` 记了 v0.1.0 资产 97,274,294 字节）。
3. [实测] **签名/公证这一项在这个仓库是零成本**，因为这里根本没有签名步骤：`release.yml` 全文 grep `codesign|notariz|CODESIGN|certificate` **0 命中**，macOS 打包是 `tar czf`（`release.yml:396-415`）。所以「macOS 签名/公证问题」在这个仓里不是新增风险；真正新增的是**上面两条**。
4. [推断] 上游已停发 x64 macOS 二进制（§2.2），意味着源码构建要在一个上游不再测试的平台上自证可用——出问题没人接。

**依据** —— 构件的环境变量名是从 `ort-sys` 源码读出来的，不是猜的：[实测] `ort-sys-2.0.0-rc.13/build/vars.rs`：`SYSTEM_LIB_PATH = ["ORT_LIB_PATH", "ORT_LIB_LOCATION"]`、`SYSTEM_LIB_PROFILE = "ORT_LIB_PROFILE"`、`PREFER_DYNAMIC_LINK = "ORT_PREFER_DYNAMIC_LINK"`；`build/main.rs:47-68` 是「有 `ORT_LIB_{PATH,LOCATION}` 就走用户库」的分支。

### 2.4 我在核对过程中发现的两个候选（原 issue / `release.yml` 注释里都没有）

这两个不改变 §1 的推荐，但**它们各自比选项 1 便宜得多**，写进决策材料是为了让「维持现状」是一个**知情**的选择。

**候选 A：只对 Intel macOS 打开 `ort` 的 `load-dynamic`，让现有回落路径兜住。**
`ort` 的 `load-dynamic` = `[std, preload-dylibs, ort-sys/disable-linking]`（[实测] crates.io 的 `ort 2.0.0-rc.15` features JSON）。`ort-sys` 的 build script 第一条就是这个短路：

```rust
if env::var("DOCS_RS").is_ok() || cfg!(feature = "disable-linking") { return; }   // build/main.rs:25-33
```

于是**不下载、不链接、也不发 `link_error_generic`**，[推断] `x86_64-apple-darwin` 的链接期死亡会直接消失，改动面约 3 行（一个 `[target.'cfg(all(target_os="macos", target_arch="x86_64"))'.dependencies]` 表里给 `fastembed` 打开 `ort-load-dynamic`；`fastembed 6.0.3` 确实暴露了这个 feature：`ort-load-dynamic = ["ort/load-dynamic"]`）。运行时没有 dylib → `ort` 在第一次调 API 时 `expect("Failed to load ONNX Runtime dylib")` 直接 panic（[实测] `ort-2.0.0-rc.13/src/lib.rs:217-247`），**而这个 panic 落在一个已经被兜住的位置**：`crates/knowledge/src/fast.rs:23-34` 把整段放进 `tokio::task::spawn_blocking`，并且 `map_err(|e| EmbedError::Other(format!("join: {e}")))`——阻塞任务的 panic 以 `JoinError` 返回，于是 `try_new()` 返回 `Err`，走到 `daemon/src/lib.rs:594-598` 的回落。**注意**：`[profile.release]` 没有 `panic = "abort"`（`Cargo.toml:27-29` 只有 `lto`/`strip`），所以这条链成立。副作用是**每次启动多一条 panic 噪音**，且日志说「(offline?)」——要沿用本仓「说出来」的规矩，这句得改成说得出原因的话。功能结局与选项 1 相同（hash embedder），但**数据目录搬迁的 400 效应一模一样**（§2.1 风险 2）。

**候选 B：用社区预编译的 Intel macOS ONNX Runtime + 编译期动态链接。**
[二手但可核到发布物] `https://github.com/pykeio/ort/issues/556` 里 `cjpais` 提供了一个自己编的 `onnxruntime-osx-x86_64-1.24.2`，并说自己 CI 在用；`ghenry` 照它设 `ORT_LIB_LOCATION` 却报 `could not link to the ONNX Runtime build`。**缺的那一步在 `cjpais` 的 CI 里看得到**（[二手] `https://github.com/cjpais/Handy/pull/1041` 的 `.github/workflows/build.yml` 补丁）：`ORT_LIB_LOCATION=$ORT_DIR/lib` **加上 `ORT_PREFER_DYNAMIC_LINK=1`**，然后把 `libonnxruntime.1.24.2.dylib` 一起打进产物。
代价（[实测]/[推断] 混合，逐条标）：dylib 得进包——[实测] 该 tarball 的 `Content-Length` 是 **7,264,274 字节**（HTTP HEAD），且它是**动态链接**，所以要按 `ort.pyke.io/setup/linking` 的做法加 rpath（macOS: `-Clink-args=-Wl,-rpath,@loader_path`）或者把 dylib 放对位置；[二手] Handy `v0.10.1` 确实发布了 `Handy_x64.dmg`（15,314,274 字节），说明这条链在真实产品里跑通过。**风险**：这个 tarball 是个人云盘（`blob.handy.computer`）、无签名、非可复现来源，把发布流水线的一个构建输入挂到它上面，是这次所有选项里唯一**引入新的供应链依赖**的一个。收益是**唯一能给出完整能力**的低价路径。

---

## 3. 若维持现状：什么变化发生时应当重新评估（可判定）

每条都写成「看哪个 URL / 看哪个字段 / 什么算满足」，不写「关注一下」。

1. **`ort-sys` 的预编译目标集里出现了 `x86_64-apple-darwin`。**
   判定（发布版，权威）：`https://crates.io/api/v1/crates/ort-sys` 取 `versions[].num` 里比 `2.0.0-rc.15` 新的版本 → 下载 `https://crates.io/api/v1/crates/ort-sys/<版本>/download` → 解包 → 读 `ort-sys-<版本>/build/download/dist.tsv` → 第一列（TSV 第 1 字段）存在**恰好等于** `x86_64-apple-darwin` 的行。
   判定（前瞻，先行指标）：`https://raw.githubusercontent.com/pykeio/ort/main/ort-sys/build/download/dist.tsv`，同一条件。
   **只看这个文件，不要看 issue 标题**：rc.14 的 `x86_64-linux-android` 就是这样加进来的，加 target 的动作**只**体现在这张表里。
2. **上游 ONNX Runtime 恢复发布 macOS x64 二进制。**
   判定：`https://github.com/microsoft/onnxruntime/releases` 最新 release 的 assets 里出现文件名含 `osx-x86_64` 的条目。（注意：本机上 `gh api repos/microsoft/onnxruntime` 整个仓库返回 404，见 §4，所以这条只能靠人眼看网页。）
3. **本仓出现真实需求证据。** 判定：issue 列表里出现**不是** `hfldqwe` 自己开的、要求 x86_64 macOS 归档的 issue（本次核过：现有 issue 里没有任何一条是用户提出的 Intel Mac 需求，`#78` 与 `#74` 都是同一位作者在收拾发布流水线）。
4. **Rust 侧把该 target 再降一级**（tier 2 → tier 3 / 移除 host tools）。判定：`https://blog.rust-lang.org/` 上关于 `x86_64-apple-darwin` 的公告；出现了就说明连「能编」都保不住，三个选项全部作废。
5. **本仓把 embedder 变成可选的 feature**（即有人真去做了 §2.1）。判定：`crates/knowledge/Cargo.toml` 里 `fastembed` 从 `[dependencies]` 移进 `[features]`；出现这条，§2.4 候选 A 就不再是「改 Cargo」而是「加一行 matrix」。

---

## 4. 我没能证实的

* **没有在 Intel macOS 上跑过任何构建。** 本机是 Windows，没有 `x86_64-apple-darwin` 的链接器。§2.4 候选 A 的「~3 行 Cargo 就能编过」是**读源码推出来的**（`build/main.rs:25-33` 的短路 + `ort/src/lib.rs:217-247` 的 `expect` + `fast.rs:23-34` 的 `spawn_blocking`），**不是**构建读数。要证实，只需要一次 `workflow_dispatch`（`release.yml:76-77` 已经写明 dispatch 不发布）。
* **选项 3 的时长我一处都没查到。** 我找过：`ort.pyke.io/setup/linking`（只讲链接，不讲时长）、`pykeio/ort#556`（只有「I compiled it last night」，无时长）、`cjpais/Handy` 的 PR #1041（它们的 CI 是**下载**预编译包，不编 ORT，所以没有可借的读数）。**我没有把任何「大概 20 分钟/1 小时」之类的数字写进本报告，因为没有出处。**
* **`microsoft/onnxruntime` 这个仓库在本机 API 上不可见**：`gh api repos/microsoft/onnxruntime` 与 `.../releases` 都返回 `404 Not Found`（同一次会话里 `repos/pykeio/ort/...` 正常），`gh api search/repositories -f q='onnxruntime org:microsoft'` 也搜不到它。所以「MS 在 1.24.1 停发 x64 macOS 二进制」我**只有 `ort` 维护者的转述**（[二手] #556），**没有** MS 自己的公告原文。
* **[二手未独立核实]** #556 里提到的两条外部事实我只看到引用、没打开原文：Rust 把 `x86_64-apple-darwin` 降为 tier 2 with host tools（`https://blog.rust-lang.org/2025/08/19/demoting-x86-64-apple-darwin-to-tier-2-with-host-tools/`）、Apple 在 macOS Tahoe 之后停 Intel 支持。
* **`fastembed` 6 → 7 的 breaking change 我没有评估。** 因为没有任何 `ort` 版本能解决本单（§2.2），这条不构成决策输入；顺带记一笔：`fastembed` 已到 `7.7.1`（2026-11-16），它对 `ort` 的约束是 `=2.0.0-rc.15`（精确等于）——**将来谁想升 `fastembed`，`ort` 是被它钉死的，不是我们自己能选的。**
* **体积数字里只有两个是真的**：`onnxruntime.lib` 341,152,186 字节（[实测]，Windows 预编译静态库）、社区 Intel macOS tarball 7,264,274 字节（[实测]，HTTP HEAD）。**「Intel Mac 包会是多大」我没有实测**，只能给「与今天 Windows 包同量级」的量级判断（[二手] 97 MB 那条取自 `release-pipeline.md:152`）。

---

## 5. 引用的 URL 全表

上游 / 外部：

1. `https://crates.io/api/v1/crates/ort` —— 版本列表（最新 `2.0.0-rc.15`，2026-10-19）。
2. `https://crates.io/api/v1/crates/ort-sys` —— 同上（`2.0.0-rc.15`，2026-10-19）。
3. `https://crates.io/api/v1/crates/ort-sys/<版本>/download` —— `.crate` 包本体；`dist.tsv` 的权威来源（rc.13/rc.14/rc.15 三份都下过）。
4. `https://raw.githubusercontent.com/pykeio/ort/main/ort-sys/build/download/dist.tsv` —— 上游 `main` 的预编译目标集。
5. `https://github.com/pykeio/ort/issues/556` —— *Build error for target x86_64-apple-darwin*（closed）；维护者决定弃掉 x64 macOS；社区预编译包在这里首次出现。
6. `https://ort.pyke.io/setup/linking` —— `ORT_LIB_PATH`、`ORT_PREFER_DYNAMIC_LINK=1`、`load-dynamic`、rpath（`-Wl,-rpath,@loader_path`）的官方说明。
7. `https://crates.io/api/v1/crates/ort/2.0.0-rc.15` —— features JSON（`load-dynamic = [std, preload-dylibs, ort-sys/disable-linking]`、`alternative-backend = [ort-sys/disable-linking]`、`default = [… download-binaries …]`）。
8. `https://crates.io/api/v1/crates/fastembed/7.7.1/dependencies`（及 `/7.0.0/`、`/6.0.3/`）—— `ort` 的精确约束。
9. `https://blob.handy.computer/onnxruntime-osx-x86_64-1.24.2.tgz` —— 社区 Intel macOS 预编译包（HEAD：`Content-Length: 7264274`）。
10. `https://github.com/cjpais/Handy/pull/1041` —— 上述预编译包的真实用法（`ORT_LIB_LOCATION` + `ORT_PREFER_DYNAMIC_LINK=1` + 打包 dylib）。
11. `https://api.github.com/repos/cjpais/Handy/releases` —— `v0.10.1` 的 `Handy_x64.dmg`（15,314,274 字节），Intel macOS 产物确实在发。
12. `https://blog.rust-lang.org/2025/08/19/demoting-x86-64-apple-darwin-to-tier-2-with-host-tools/` —— [二手，未打开原文] Rust 侧降级公告。

本仓：

13. `.github/workflows/release.yml:37-77`（长注释：为什么 Intel macOS 故意缺席、三个候选方向）、`:226-234`（`Targets` 的 `rows` 表，加一行就在这）、`:254-264`（180 min 上限与 `7243 s` 冷跑读数）、`:305-336`（`cargo build --release --target`，以及「archive 里没有 onnxruntime 共享库」的四条实测）、`:396-415`（macOS 打包，无签名）。
14. `docs/design/release-pipeline.md:91-106`（ONNX 的四个实测）、`:136-144`（Intel Mac 缺口、三个候选方向、dispatch run 43238636506）。**注意**：该文件在我做这一单期间有并发的未提交改动（`git diff` 显示 +18/−7），我引用的行号对应当前**工作树字节**；其中 `:138-144` 那段 Intel Mac 说明正是那批并发改动写入的，不是我写的。
15. `crates/knowledge/Cargo.toml:22`、`crates/knowledge/src/{fast.rs,embed.rs,lib.rs}`、`crates/knowledge/src/store.rs:650-700,942-979`、`crates/daemon/src/lib.rs:557-623`、`crates/daemon/src/memembed.rs:114-138`、`crates/daemon/src/distill.rs:1165-1180`、`crates/daemon/src/api.rs:685-700,874`、`Cargo.lock:5209-5230,2597-2611`、`Cargo.toml:27-35`、`.github/scripts/smoke-archive.sh:183-219`。
16. `docs/design/reviews/gen2-recall-spec.md:41`（hash embedder 对 CJK 近乎无信息）、`docs/design/reviews/gen2-integration-verify.md:24,117`（256 vs 384 维导致 23/23 → 400）、`docs/design/reviews/t323-memory-quality-readings.md:15`（活库 embedder 读数）。

---

## 6. 我实际执行过的读数（可按序复跑）

```powershell
$env:NO_PROXY='*'   # 这台机器的系统代理是坏的（127.0.0.1:7890 无人监听）

# 1) 版本面
(Invoke-WebRequest -UseBasicParsing 'https://crates.io/api/v1/crates/ort'     -Headers @{'User-Agent'='ruagent-research'}).Content | ConvertFrom-Json | % { $_.versions | % num }
(Invoke-WebRequest -UseBasicParsing 'https://crates.io/api/v1/crates/ort-sys' -Headers @{'User-Agent'='ruagent-research'}).Content | ConvertFrom-Json | % { $_.versions | % num }

# 2) rc.14 / rc.15 的预编译目标集（rc.13 用本地 registry 里那份即可）
Invoke-WebRequest -UseBasicParsing 'https://crates.io/api/v1/crates/ort-sys/2.0.0-rc.15/download' -OutFile ort-sys.crate
tar -xzf ort-sys.crate -C .\x ; Get-Content .\x\ort-sys-2.0.0-rc.15\build\download\dist.tsv | % { ($_ -split "`t")[0] }

# 3) 上游 main
(Invoke-WebRequest -UseBasicParsing 'https://raw.githubusercontent.com/pykeio/ort/main/ort-sys/build/download/dist.tsv').Content

# 4) 依赖面
cargo tree -i ort-sys -e features -p ruagent      # → ort → fastembed "default" → ort-download-binaries-native-tls

# 5) 本机 ONNX 静态库体量
Get-ChildItem "$env:LOCALAPPDATA\ort.pyke.io" -Recurse -File | Sort-Object Length -Desc | Select -First 3 Length,FullName
```

读到的关键数字（本报告引用处均可回指本节）：`ort`/`ort-sys` 最新 `2.0.0-rc.15`（2026-10-19）；rc.13 `dist.tsv` 15 行、rc.14/rc.15 各 20 行、上游 `main` 20 行，**三者都不含 `x86_64-apple-darwin`**；`fastembed` 7.7.1 → `ort =2.0.0-rc.15`；`onnxruntime.lib` 341,152,186 字节；社区 Intel macOS tarball 7,264,274 字节。
