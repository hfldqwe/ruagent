# gen3 只读审计：安全 / 隐私 / 耐久（凭据 · SQL · 路径 · 迁移 · 进程）

> **性质**：**只读审计**。本单没有起任何守护进程、没有写活库 `~/.ruagent`、没有改动任何文件；唯一产物是本文件。
> **审计者**：verify2（独立审计，不是被审面的作者）。
> **时间窗**：2026-09-28T18:4x–19:2x +08:00（活库只读读数带各自时间点）。
> **范围**：`crates/**`、`cli/`、`panel/`、`scripts/`、`.gitignore`、`.github/workflows/`（本代此前没有系统查过这一面）。
> **引用纪律**：涉密内容一律只给**路径与形状**，不抄明文（本单没有发现任何真实密钥；见 §2 末）。
> **计数**：**10 条待修 finding**（按安全优先排序：A-1 … A-5、A-7 … A-11）+ **1 条已闭环留档**（A-6，见 §1.5）+ 1 条「已核验为干净」清单（§1.11）+ 5 条未验证猜想（§2）+ 未覆盖范围（§3）。
> **在途编辑提示**：`.github/workflows/ci.yml`（+263/−16）与 `e2e.yml`（+99/−1）在本次审计期间被同伴重写（mtime 2026-09-29T02:42:40），CI 相关的行号以 **19:2x 的复核**为准；`ci.yml` 的旧形状见 §1.5。

---

## 0 摘要（一段话）

这一面的**编码纪律整体是好的**：生产路径上**没有 SQL 拼接注入面**（唯一动态表名被 `&'static str` 类型钉住，所有 `IN (…)` 都是生成占位符），文档名与 wiki slug 都有严格的字符/段数/`..` 校验，路径遍历在代码与测试上都被挡住，面板没有 XSS 注入点，`~/.ruagent` 的 ACL 只给 SYSTEM/Administrators/属主，进程杀伐在脚本里**只按记录过的 PID**。真正的问题集中在**「内容的副本不在清理面上」这一条主线上**：`forget`/`forget_report` 枚举了 13 个残余面，却**不包含备份产物**——而本机上就躺着一个 **35,843 字节 / 312 行**的明文备份，装着 `memories` 被改前的字节（活库那 163 行现在已经 0 行带该标记），以及一个 **1.36 MB 的历史整库副本**；`wiki-backups/*.md` 是同一形状的第三份。此外：agent 的 stderr 在 `RUST_LOG=debug` 下一行不落地被抄进 `daemon.log`（默认关，开一个环境变量就开）、API **无鉴权且 `--addr` 没有 loopback 守卫**、CI 的四个 action 全是可变引用而 `release.yml` 有 `contents: write`、全仓**没有任何 `integrity_check`** 也没有恢复路径。

---

## 1 findings（安全优先）

### A-1（high · 隐私/耐久 · 清理面不完整）`forget` 的残余面不包含备份产物，而备份里就是被清理前的字节

* **`file:line`**
  * 面清单：`crates/memory/src/lifecycle.rs:281-300`（`ResidualSurface` 13 个变体：`Memories` / `MemoriesFts` / `Episodes` / `Derived` / `KnowledgeFile` / `KnowledgeDocument` / `KnowledgeChunk` / `KnowledgeChunkFts` / `KnowledgeVectors` / `WikiChunk` / `WikiPageHash` / `WikiPlan` / `WikiBuildPage`）——**没有备份文件这一类**。
  * 备份写入点 ①：`crates/memory/src/lifecycle.rs:634-648`（`strip_distilled_prefix` 把**改前**的 `lines` 写进 `<root>/data/backups/memories-distilled-prefix-{stamp}.txt`；同文件 `:922-926` 的测试断言"backup must carry the before content"）。
  * 备份写入点 ②：`crates/daemon/src/wiki.rs:2317-2323`（每次覆盖写页面前 `fs::copy` 到 `<root>/knowledge/wiki-backups/{slug}.md`）。
  * 面清单的**唯一入口**：`crates/daemon/src/api.rs:3326`（`forget_report` 处理器）→ `crates/memory/src/lifecycle.rs:379`。
* **可复现命令**
  ```powershell
  Get-ChildItem "$env:USERPROFILE\.ruagent\data\backups" | Select-Object Name,Length,LastWriteTime
  Get-ChildItem "$env:USERPROFILE\.ruagent\data" -Filter '*.db*' | Select-Object Name,Length
  Select-String -Path crates\memory\src\lifecycle.rs -Pattern 'pub enum ResidualSurface' -Context 0,20
  Select-String -Path crates\daemon\src\wiki.rs -Pattern 'wiki-backups' -Context 2,4
  ```
* **证据读数（只读，2026-09-28 19:0x +08:00）**
  ```
  data/backups/memories-distilled-prefix-20260926T210910Z.txt   35,843 B   312 行   2026-09-27T05:09:10
  data/ruagent.db.before-t229-cleanup-20260926-150802          1,359,872 B（历史整库副本）
  活库（只读）：memories 163 行；正文仍带 `[distilled]` 标记的行 = 0；deleted_at 非空 = 0
  knowledge/wiki-backups/  → 本 root 上尚未创建（下一次"覆盖写页面"时由 wiki.rs:2323 创建）
  ```
  ⇒ **活库已经被清理到 0，而"改前的字节"以明文存在于 `data/backups/` 的 312 行里**；`forget_report` 的 13 个面里**没有任何一个**会点到这个文件（它连 `NotAvailable` 都不会报，因为压根不在枚举里）。
* **影响面**：C5"遗忘可证明"的结论面。一条被 `forget`/`purge` 的记忆，若它的字节曾经过 `strip_distilled_prefix` 或 `wiki` 覆盖写，就仍在明文的 `data/backups/*.txt`、`wiki-backups/*.md` 里；`data/*.before-*.db` 是整库快照（含全部历史行，且**不在任何代码管辖下**）。任何"残余 0"的报告在这三个面上都是**未经检验的真**。
* **可证伪的修复判据**：对任一进入过备份的内容 hash，`forget_report` 返回一个非空 hit（新面 `ResidualSurface::BackupFile` 或等价物，`key` 给路径形状而非内容）；并且 `purge` 之后**重跑同一命令得到的读数 = 0**（`data/backups/*.txt` 中该 hash 的字节消失或该文件被重写）。反向的假读数也要挡住：备份**存在但与该 hash 无关**时必须报 0（否则就成了"有备份就永远报残余"）。
* **建议 owner**：mem-core（`lifecycle.rs` 的面清单 + 备份生命周期）+ I-D（`wiki-backups` 的保留策略）；**面清单本身是 spec 级决定**（R-B C5 的判据要不要把备份纳进来），建议 captain 记录为跨区裁决。

---

### A-2（medium · 隐私/日志）agent 的 stderr 会在 debug 档下一行不改地进 `daemon.log`，而日志不在任何清理面上

* **`file:line`**：`crates/acp/src/run.rs:80` 与 `crates/acp/src/chat.rs:217` —— 两处都是
  `tracing::debug!(target: "ruagent::acp::stderr", "{line}")`（**整行原样**，无截断、无脱敏）；
  过滤器的默认档位：`crates/daemon/src/lib.rs:42-46`（`EnvFilter::try_from_default_env().unwrap_or_else(|_| "ruagent=info")`）。
* **可复现命令**
  ```powershell
  Select-String -Path crates\acp\src\run.rs,crates\acp\src\chat.rs -Pattern 'acp::stderr' -Context 2,2
  Get-Content crates\daemon\src\lib.rs | Select-Object -Skip 41 -First 6
  Get-Content "$env:USERPROFILE\.ruagent\logs\daemon.log" -Tail 5     # 现状形状（只读）
  ```
* **证据读数**
  ```
  默认 filter = "ruagent=info"  ⇒ debug 档默认关闭（现状 daemon.log 尾部只有 mcphealth 行）
  但 RUST_LOG=ruagent=debug（或 RUST_LOG=debug）⇒ 每一行 agent stderr 进日志
  daemon.log 由 scripts\ruagent-daemon.ps1 追加写、5 MB 轮转到 daemon.log.1
  ```
  ⇒ 泄露**需要一个环境变量**（默认不泄），但一旦打开就是**第三方 CLI 的原始 stderr 全量落盘**：agent CLI 在报错时经常把 token/Authorization 头/整段 prompt 回显出来，而这段内容**不在用户的控制范围**，也不在 `ResidualSurface` 的任何面上（日志文件不是残余面之一）。端到端演示**未做**（不许起守护进程）——这一条是代码级 + 过滤器读数。
* **影响面**：`~/.ruagent/logs/daemon.log`（+ `.1`）是明文、长期、**不可 purge** 的副本站；用户按常规排障习惯加 `-vvv`/`RUST_LOG=debug` 时会把敏感值一起写进去。
* **可证伪的修复判据**：把该 target 的默认关掉到**只有显式点名该 target 才开**（`RUST_LOG=ruagent::acp::stderr=debug`），或在写日志前对 `line` 做脱敏；并且 `daemon.log` 出现在"残余面"清单里（或有一份明文日志保留声明）。可测形式：一条单测断言默认 filter 下 `ruagent::acp::stderr` 事件被丢弃。
* **建议 owner**：`crates/acp` 拥有者（acp/daemon 边界）；日志保留声明建议写进 AGENTS.md（本代已有日志纪律）。

---

### A-3（medium · 网络暴露）API 无鉴权，而 `--addr` 接受任意地址且**没有 loopback 守卫**

* **`file:line`**：`cli/src/main.rs:28-30`（`--addr`，默认 `127.0.0.1:8787`）、`:125-135`（解析后直接 `serve`）；`crates/daemon/src/lib.rs:293-297`（`TcpListener::bind(addr)` + 一行 `listening on http://{addr}`）；**全仓 `is_loopback` / `allow_remote` / `non-loopback` 命中数 = 0**。
* **可复现命令**
  ```powershell
  Select-String -Path crates\daemon\src\lib.rs,cli\src\main.rs -Pattern 'listen|bind|127\.0\.0\.1|addr' | Select-Object -First 12
  Select-String -Path crates,cli -Include *.rs -Pattern 'is_loopback|allow_remote' -Recurse   # 0 命中
  Select-String -Path crates\daemon\src\api.rs -Pattern 'migrate-distilled-prefix|memory_migrate' -Context 0,2
  ```
* **证据读数**：无鉴权中间件（`Authorization` 在 `*.rs` 中 0 命中）；无 CORS 层（见 A-11）；`--addr` 无守卫 ⇒ `ruagent serve --addr 0.0.0.0:8787` 会把**读 + 写**的全部端点（`/api/v1/memory/*`、`/api/v1/knowledge/*`、`/api/v1/wiki/*`、runs、transcripts）发布到局域网，而启动日志只有 `listening on http://0.0.0.0:8787`；其中包含一次性数据迁移端点 `POST /api/v1/memory/migrate-distilled-prefix`（`api.rs:193` 注册、`:2586` 处理器 —— 重写 156+ 行用户记忆正文，无确认令牌）。
* **影响面**：局域网内任意主机（或宿主机上任何进程）可读走全部记忆/知识/wik i/会话转录，并可触发写与迁移。
* **可证伪的修复判据**：非 loopback 地址必须显式解锁（`--allow-remote` / `RUAGENT_ALLOW_REMOTE=1`）否则 `serve` 以非零码退出；解锁时打印一条点名风险的 WARN。可测形式：一条单测断言 `--addr 0.0.0.0:8787` 不带解锁开关时启动失败。
* **建议 owner**：`cli/`（**当前无 owner**，AGENTS.md 已登记）+ `crates/daemon` 的平台面；建议 captain 指派单一 owner（本审计只报告不代修）。

---

### A-4（medium · 供应链/CI）三个 workflow 的 action 全是可变引用，而 `release.yml` 持有 `contents: write`；`ci.yml` 没写 `permissions:`

* **`file:line`（**读数时刻**：2026-09-28T19:2x +08:00；`ci.yml` 在本次审计期间被同伴重写过一次，见 §1.5）**
  * `.github/workflows/ci.yml:44,45,48,193,194,197,285,286`（`actions/checkout@v4`、`dtolnay/rust-toolchain@stable`、`Swatinem/rust-cache@v2`、`actions/setup-node@v4`）—— `ci.yml` **没有 `permissions:` 块**（对照 `e2e.yml:19-20` 有 `contents: read`）。
  * `.github/workflows/release.yml:7-8`（`permissions: contents: write`）与 `:28,29,32,67,77,78,83`（同样是可变引用，且 `:83` 是 `softprops/action-gh-release@v2` —— 该 job 能发布 release 资产；`dtolnay/rust-toolchain@stable` 是**滚动**引用，连大版本 tag 都不是）。
  * `.github/workflows/e2e.yml:31,32,33,187`（`actions/upload-artifact@v4`）。
* **可复现命令**
  ```powershell
  Select-String -Path .github/workflows/*.yml -Pattern 'uses:'
  (Select-String -Path .github/workflows/*.yml -Pattern 'uses:.*@[0-9a-f]{40}').Count      # ⇒ 0
  Select-String -Path .github/workflows/*.yml -Pattern 'permissions:' -Context 0,2
  ```
* **证据读数**（原文计数）：`uses:` 共 **19** 处（`ci.yml` 8 / `e2e.yml` 4 / `release.yml` 7），**SHA 固定 0 处**；`permissions:` 只出现在 `e2e.yml`（`contents: read`）与 `release.yml`（`contents: write`），**`ci.yml` 0 处** ⇒ 它取仓库/组织默认权限，而它在 `pull_request` 上跑。
* **影响面**：`release.yml` 的 job 能写仓库内容并发布 release —— 一旦某个被引用的 action（或某个 tag 被重指）被投毒，就在**有写权限**的 job 里执行；`ci.yml` 的默认权限则把 `pull_request` 这条面交给仓库设置。
* **可证伪的修复判据**：每个 `uses:` 都固定到 40 位 commit SHA（可加注释保留可读版本号），且 `ci.yml` 显式声明 `permissions: contents: read`；用一条 CI 步骤机检（与本仓已有的 nested-Result 守卫同一形状）：`grep -E 'uses: .*@(v[0-9]+|stable|master|main)$' .github/workflows/*.yml` 必须无输出，且 `grep -c 'permissions:' .github/workflows/ci.yml` ≥ 1。
* **建议 owner**：CI/平台面（当前无专属 owner；建议 captain 指派）。

### 1.5 本次审计期间**已闭环**的一条（不是 finding，留档以免下游重复报）

**旧读数（我在 19:0x 读到的 `ci.yml`，全文 110 行）**：`:76` `run: cargo test --workspace || cargo test --workspace` —— Windows 门用 `||` 重跑一次，**任何**非确定性失败（含真正的竞态）都可能被第二轮盖成绿。
**新读数（同一文件，2026-09-28T02:42:40 被同伴重写为 357 行，我在 19:2x 复核）**：
```
ci.yml:248-256  "Test attempt 2 (only for the known build race)"
ci.yml:253-255  if grep -qE 'panicked at|^test result: FAILED' "$log"; then
                  echo "::error::attempt 1 failed with a TEST-level failure. … Stopping here with attempt 1's evidence above."
                  exit 1
```
⇒ 现在**只有"首轮失败且不含测试级失败（构建/工具链竞态）"才重试**；测试级失败**当场停**并点名原因；两次尝试都通过 `.github/workflows/scripts/test-evidence.sh` 把计数打进 step summary。我原先要提的"红被重跑盖绿"在这版里**不再成立**，因此**不列为 finding**（只留这段对比）。同一版还新增了"`--ignored` 清单必须为空/可枚举"的完整性守卫（`:151-167`）。

---

### A-5（medium · 耐久）全仓没有 `PRAGMA integrity_check`/`quick_check`，也没有 DB 备份/恢复路径

* **`file:line`**：`crates/store/src/sqlite.rs:45-52`（`busy_timeout 5s` / `journal_mode=WAL` / `synchronous=NORMAL` / `foreign_keys=ON` / `migrations::apply`）；`cli/src/main.rs:902`（`fn doctor(url: …)` —— **走 HTTP**，自身不开库）。
* **可复现命令**
  ```powershell
  Select-String -Path crates,cli -Include *.rs -Recurse -Pattern 'integrity_check|quick_check'   # 0 命中
  Select-String -Path crates,cli -Include *.rs -Recurse -Pattern 'VACUUM INTO'                   # 只有测试
  Select-String -Path cli\src\main.rs -Pattern 'fn doctor' -Context 0,6
  ```
* **证据读数**：`integrity_check|quick_check` **全仓 0 命中**；`VACUUM INTO` 只出现在 `crates/graph/tests/live-after.rs` 与 `crates/knowledge/tests/*`（**测试**用，不是产品的备份路径）；`doctor` 的第一个参数是 `url`，全程 `reqwest::blocking`，所以在"守护进程起不来"（`Db::open` 失败）这个正是最需要 doctor 的场景里，**doctor 也帮不上**。本机唯一的恢复物是**手工**留下的 `data/ruagent.db.before-t229-cleanup-20260926-150802`（1.36 MB）。
* **影响面**：DB 或 WAL 一旦损坏（磁盘满、断电、杀进程在 checkpoint 中间），后果 = 守护进程启动失败，没有内置检测、没有自动快照、没有文档化恢复步骤；`synchronous=NORMAL`（`:48`）在 WAL 下意味着**断电可能丢掉最近已提交的事务**（不损坏，但会丢）。
* **可证伪的修复判据**：存在一条**离线**检查路径（如 `ruagent doctor --offline`）以只读连接执行 `PRAGMA quick_check` 并打印结果；并且"损坏样本"上该命令以非零码失败并点名 DB 路径（可用一个截断过的 DB 副本作 fixture）。附带一条：迁移前自动快照 + 保留策略二选一落地，并在文档里写明恢复步骤。
* **建议 owner**：`crates/store`（打开/完整性）+ `cli/`（doctor 的离线分支）。

---

### A-6（**已闭环 · 不再要求修复**）Windows 门曾用 `||` 重跑一次：首轮失败可以被报成绿 —— 读数见 §1.5

* **状态**：我在 19:0x 读到的是旧形状，**19:2x 复核时该形状已不存在**（`ci.yml` 被同伴重写为 357 行，测试级失败当场停）。**本条不作为待修 finding**，编号保留只为让 §1.5 的对比可被引用。
* **旧读数（留档）**：`.github/workflows/ci.yml:76`（110 行版本）`run: cargo test --workspace || cargo test --workspace`；同 job 只在 `push` 到 `main` 时跑，是 Windows 侧唯一的门。
* **新读数（19:2x 复核）**：`ci.yml:248-256` 只在"首轮失败**且不含** `panicked at|^test result: FAILED`"时才重试，否则 `exit 1` 并点名"retrying would hide a real red behind a lucky second run"。
* **建议 owner**：CI/平台面。

---

### A-7（low · 进程）孤儿清扫：活性检查与子进程枚举之间有 TOCTOU 窗口，这是全仓唯一可能杀到"陌生人"的形状

* **`file:line`**：`crates/daemon/src/orphans.rs:83-100`（`:85-96` 用 `process_started_at_ms(prior.pid) == prior.started_at_ms` 判定"还是同一个进程"；`:98-100` 才开始 `child_pids_of(prior.pid)` 并逐个 `kill_tree`）；`kill_tree` 在 `:129-139`（Windows：`taskkill /F /T /PID <pid>`；unix：`kill(-pid)`）。
* **可复现命令**
  ```powershell
  Get-Content crates\daemon\src\orphans.rs | Select-Object -Skip 82 -First 20
  Select-String -Path crates\daemon\src\orphans.rs -Pattern 'taskkill|child_pids_of|process_started_at_ms'
  ```
* **证据读数**：模块文档（`:12-18`）把 PID 复用论证写得很清楚，并且**确实**在重用时刻意跳过清扫（安全方向）；但"检查"（`:85`）与"枚举 + 杀"（`:98`）是两步，中间 pid 被复用 ⇒ 枚举到的是**新进程的孩子**，而 `taskkill /T` 会连带整棵树。窗口极小（同一函数内几微秒 + 进程表快照），且需要 pid 恰好回绕，但这是本仓唯一"目标不是自己记录的进程"的杀伐形状。
* **影响面**：极小概率杀掉无关进程树（含用户的其他工作）。相对地，"不扫"的代价只是孤儿多活一会儿 —— 所以这里应当**偏向不杀**。
* **可证伪的修复判据**：在真正 `kill_tree` 之前**重新读一次** `process_started_at_ms(prior.pid)`（仍为 `None` 才继续），或要求每个候选子进程的创建时间早于 `prior.started_at_ms`；可测形式：给一层可注入的进程表快照，构造"检查时死、枚举时已被复用"的时序，断言不杀。
* **建议 owner**：`crates/daemon`（orphans）。

---

### A-8（low · SQL 硬化）`update_build` 的动态列名是 `&str`（今天只有一个字面量调用点）

* **`file:line`**：`crates/daemon/src/wiki.rs:3399-3406`（`let sql = format!("UPDATE wiki_builds SET {field} = ?2 WHERE id = ?1")`）；唯一调用点 `:1897`（`"pages_planned"` 字面量）。
* **可复现命令**
  ```powershell
  Select-String -Path crates\daemon\src\wiki.rs -Pattern 'update_build\('
  Get-Content crates\daemon\src\wiki.rs | Select-Object -Skip 3398 -First 8
  ```
* **证据读数**：调用点 1 处、传字面量 ⇒ **今天不可注入**；但签名 `field: &str` 不构成任何保证。同仓已有更好的形状可抄：`crates/knowledge/src/store.rs:876-889` 把表名收成 `&'static str` 并在文档注释里点明"永远是本模块自己的字面量，不是调用方输入"（这是**类型级**的证明点）。
* **影响面**：潜在（未来多一个调用点、或有人把字段名接到请求参数上，就变成注入面）。
* **可证伪的修复判据**：`field` 变为 `&'static str`（或一个四态枚举）后编译通过 ⇒ 调用方无法传运行时字符串；签名本身即判据。
* **建议 owner**：I-D（wiki.rs）/ 或下一次动该文件的单。

---

### A-9（low · 路径/符号链接）注释说"不跟随符号链接"，但**指向文件的 `.md` 符号链接会被索引**

* **`file:line`**：`crates/knowledge/src/files.rs:127-154`（`walk_md_files`：`:145-148` 只对 `is_dir()` 递归，`:150-152` 的 `else if` 分支对**任何**以 `.md` 结尾的条目都 `out.push`）；注释在 `:128`（"symlinks are not followed"）与 `:811`。
* **可复现命令**
  ```powershell
  Get-Content crates\knowledge\src\files.rs | Select-Object -Skip 126 -First 30
  Select-String -Path crates\knowledge\src\files.rs -Pattern 'symlink' -Context 1,1
  ```
* **证据读数**：`std::fs::DirEntry::file_type()` 对符号链接返回 `is_dir() == false`（不跟随）⇒ **目录**符号链接被跳过（符合注释）；但**文件**符号链接（`.md`）落到 `else if` 分支被 push，随后被读取、切块、写进 FTS 与向量、可被召回注入。注释与行为的差异可以用一个"放一个 `.md` 符号链接"的用例直接证伪。
* **影响面**：知识树内出现 `.md` 符号链接时，链接目标的内容进入 KB（跨出 `~/.ruagent/knowledge` 边界）；成因通常是解压/同步工具在树里留下链接（Windows 建文件符号链接需要权限或开发者模式，所以这是**低**频但真实的边界扩张）。
* **可证伪的修复判据**：`file_type.is_symlink()` 显式跳过 ⇒ 同一用例下该文档不出现在 `GET /api/v1/knowledge/documents`（或注释改成"目录链接不跟随、文件链接会跟随"并给出理由）。
* **建议 owner**：I-A（knowledge/files.rs）。

---

### A-10（low · 测试内 SQL）测试助手把 session key 插进带引号的 SQL 字面量

* **`file:line`**：`crates/store/src/migrations.rs:1088-1098`（`format!("SELECT COUNT(*) FROM distill_log WHERE session_key = '{session}'")` 与紧随其后的 `… AND status='ok'`）。
* **可复现命令**
  ```powershell
  Get-Content crates\store\src\migrations.rs | Select-Object -Skip 1087 -First 12
  Select-String -Path crates\store\src\migrations.rs -Pattern "^#\[cfg\(test\)\]"      # 该处在 mod tests 内（:98 起）
  ```
* **证据读数**：调用点位于 `#[cfg(test)] mod tests`（`:98`）内，`session` 是测试局部值 ⇒ **生产影响为 0**；但同文件另有 4 处同样的插值形状（`:117`、`:208`、`:345`、`:847/850`），全部只接**内联字面量表名/列名**。它的价值在"教坏模型"：一个 key 里带 `'` 就会让语句崩。
* **影响面**：无生产影响；测试可读性与示范性。
* **可证伪的修复判据**：改成 `params![session]` 绑定 ⇒ 同一测试仍绿，且把 key 换成 `a'b` 时它仍绿（这条正是原写法会红的反例）。
* **建议 owner**：I-SCHEMA（store）。

---

### A-11（info · 已核验为干净的面）——不是 finding，列出来是为了让"没查到"与"没查"区分开

| 面 | 读数（命令见 §4） | 结论 |
| --- | --- | --- |
| 生产 SQL 注入 | `format!(` 接 SQL 关键字的命中共 8 处：`store/src/migrations.rs` **6 处（117/208/345/847/850/1092）+ `graph/tests/live-after.rs:90` 1 处 + `daemon/src/wiki.rs:3400` 1 处**；其中 migrations/graph 两处**全在 `#[cfg(test)]`** 且只接内联字面量表名/列名（但 1092 那处的形状见 A-10）；`store.rs:886` 的唯一动态表名是 `fn fts_match(table: &'static str, …)`（两个调用点都传字面量，**类型即证明**）；`chat.rs:1396` / `api.rs:4377` / `files.rs:676` / `store.rs:920,959` 全是 `IN ({placeholders})` 的生成占位符 + 绑定；`store.rs:921` 的 `LIMIT ?N` 也是占位符 | **无注入面** |
| `ORDER BY`/`LIMIT` 拼接 | `grep '(ORDER BY|LIMIT|OFFSET|GROUP BY)\s*\{'` = **0 命中** | 干净 |
| 路径遍历（文档名） | `files.rs:98-125 doc_file_name`：按 `/` 分段，逐段拒 `..`/空/点开头/`\`/`< > : " \| ? *`/控制字符，段长 ≤64、段数 ≤4、总长 ≤200；测试 `crates/daemon/tests/knowledge_api.rs:346-357` 断言 `..%2Fevil` 等三种 → **400 且不落盘** | 干净 |
| 路径遍历（wiki slug） | `wiki.rs:1285-1296 valid_slug`：`^[a-z0-9][a-z0-9-]{0,63}$`（首字符 alnum、尾不接 `-`）⇒ `..`/`/`/`\`/`:` 全部不可能；在 `:2065` 的 plan 入口校验 | 干净 |
| 符号链接（目录） | `files.rs:145-148`（**文件**链接见 A-9） | 干净（目录） |
| 面板 XSS | `dangerouslySetInnerHTML` 全仓 **1 处**：`panel/src/icons.tsx:82`，值为 `PATHS[name]`，`name` 的类型是 `keyof typeof PATHS`（`icons.tsx:62`）⇒ 封闭键集 + 内部 SVG 路径 | 干净（并有类型级判据） |
| 面板敏感值 | `panel/src` 里 `apiKey/secret/token/password` 命中 = 0（只有 `token` 作"设计令牌"义）；`localStorage` 只存 UI 偏好与 `chat.cwd`/`chat.workspaces`（**本地路径**，非凭据） | 无凭据面 |
| 仓库内凭据 | `git ls-files` 里 `.env*`/`*.pem`/`*.key`/`id_rsa`/`credentials`/`.npmrc` **0 命中**；`.gitignore` 显式 **`.env` / `.env.*` / `!.env.example` / `*.db` / `*.db-wal` / `*.db-shm` / `/transcripts/` / `/workspaces/`**；三个 workflow **0 处** `secrets.` 引用 | 无 |
| `~/.ruagent` 文件权限 | `icacls ~/.ruagent` = `SYSTEM:(F)` / `Administrators:(F)` / `属主:(F)`（含 `(I)` 继承）；`data/ruagent.db`、`data/transcripts/` 同形 ⇒ **没有 `Users`/`Authenticated Users`/`Everyone`** | 干净（但依赖 profile ACL，见 §2 观察） |
| 脚本杀进程 | `scripts/ruagent-daemon.ps1:128` `Stop-Process -Id $p`（读 `data/daemon.pid`）、`scripts/ruagent-canary.ps1:58` `Stop-Process -Id $id.Pid`（自己 spawn 的子进程）、`scripts/cargo-team.ps1:147-148` 只动**自身** `$PID`；**无** `taskkill /IM`、无 `netstat`/`Get-NetTCPConnection` 按端口扫 | 符合"只杀记录过的 PID" |
| daemon CORS | 全仓 `CorsLayer|allow_origin|permissive` 命中 = **0**（唯一的 `permissive` 在 `runs.rs:24` 讲握手）⇒ 浏览器**读不到**跨源响应；且端点都是 `Json<T>` 提取器（要求 `application/json`）⇒ 简单请求也过不了 CSRF 形状 | 干净（A-3 才是那条真问题） |

---

## 2 未验证猜想（明确标为**未经证实**，不是 finding）

1. **Windows 保留设备名可能通过文档名校验**：`doc_file_name` 只拒字符与 `..`/点开头，不拒 `nul`/`con`/`aux`/`prn`/`com1`/`lpt1` ⇒ `PUT /knowledge/raw/nul` 会得到路径 `nul.md`。我尝试的只读探针 `Test-Path NUL` 在 pwsh 7 上返回 `False`，**无法据此证明**设备语义，因此不敢写成 finding。要证实只需在**临时 root** 上做一次写入并检查落盘字节（本单不许写）。
2. **`synchronous=NORMAL` 的断电语义**：WAL + NORMAL 在断电时可能丢最近提交（不损坏）。我没做断电实验，只记录 pragma 读数（`sqlite.rs:47-49`）。
3. **MAX_PATH**：文档路径最长为 `~/.ruagent/knowledge/` + 4 段 ×64 + `.md`（`doc_file_name` 上限 200 字符），`wiki-backups/{slug}.md` 也短；但用户名很长时理论上仍可能逼近 260。全仓不使用 `\\?\` 前缀。未构造长路径实测。
4. **MCP 工具面**：`crates/mcp/src/lib.rs` 有 14 个 `#[tool]` 注册点（健康日志报 `tools=9`）。我没有逐个读工具体（是否有一个"任意路径读写"工具取决于入参校验）。这是一个值得单独一轮的面。
5. **面板持久化**：`chat.cwd` / `chat.workspaces`（`Chat.tsx:335-410`）把本地绝对路径写进浏览器 localStorage；不是凭据，但属于"浏览器配置里留下本机路径"的隐私小面。

---

## 3 未覆盖范围（写清楚，避免读成"查过了"）

* **没有任何端到端运行读数**：本单不许起守护进程 ⇒ `forget_report` 的真实调用、`RUST_LOG=debug` 的实际落盘、损坏 DB 的启动行为、`--addr 0.0.0.0` 的实际暴露，全部只有**代码 + 文件系统 + 只读 SQL** 三级证据，每条 finding 都给了可复现命令但没有"我跑出来看到"的运行时读数。
* **没有做**：依赖漏洞扫描（`cargo audit` 未安装/离线）、模糊测试、密码学评审、TLS/传输层（本仓是本地明文 HTTP，无 TLS 面）、`panel` 的浏览器 e2e（未跑 `npm run build`，XSS 结论只到 grep + 类型级判据）、`scripts/*.mjs`（只查了进程形状）、MCP 工具体、LanceDB/Lance 文件层的权限与耐久（`~/.ruagent/data/lancedb` 只做了存在性层面）、`.git` 历史里的历史密钥（未 `git log -p` 全历史搜凭据；只查了当前树与 tracked 清单）。
* **`wiki-backups/` 在本 root 上尚不存在**：A-1 的第三份副本是**代码路径**证据（`wiki.rs:2317-2323`）+ `wiki_pipeline.rs:446-480` 的测试，不是本机当前读数。
* **`data/*.before-*.db` 的产生者不在代码里**：那是人工/AI 在 t229 期间留下的快照（文件名带日期）。我把它当"现场存在的事实"报告，而不是某个函数的输出。

---

## 4 复现命令汇总（全部只读；照抄即可）

```powershell
# 凭据与权限
git ls-files | Select-String -Pattern '(^|/)\.env|\.pem$|\.key$|id_rsa|credentials|\.npmrc$'
Get-Content .gitignore | Select-Object -First 30
icacls "$env:USERPROFILE\.ruagent"
icacls "$env:USERPROFILE\.ruagent\data\ruagent.db"
Select-String -Path .github/workflows/*.yml -Pattern 'secrets\.'          # 0 命中

# 备份副本（A-1 的核心）
Get-ChildItem "$env:USERPROFILE\.ruagent\data\backups" | Select-Object Name,Length,LastWriteTime
Get-ChildItem "$env:USERPROFILE\.ruagent\data" -Filter '*.db*' | Select-Object Name,Length
python -c "import sqlite3,os;p=os.path.join(os.environ['USERPROFILE'],'.ruagent','data','ruagent.db');c=sqlite3.connect('file:'+p.replace(chr(92),'/')+'?mode=ro',uri=True);print('memories:',c.execute('SELECT COUNT(*) FROM memories').fetchone()[0]);print('marker rows:',c.execute('SELECT COUNT(*) FROM memories WHERE content LIKE :m',{'m':'%[distilled]%'}).fetchone()[0])"
Select-String -Path crates\memory\src\lifecycle.rs -Pattern 'pub enum ResidualSurface' -Context 0,20
Select-String -Path crates\daemon\src\wiki.rs -Pattern 'wiki-backups' -Context 2,4

# 日志路径（A-2）
Select-String -Path crates\acp\src\run.rs,crates\acp\src\chat.rs -Pattern 'acp::stderr' -Context 2,2
Get-Content crates\daemon\src\lib.rs | Select-Object -Skip 41 -First 6
Get-Content "$env:USERPROFILE\.ruagent\logs\daemon.log" -Tail 5

# 网络面（A-3）
Select-String -Path crates,cli -Include *.rs -Recurse -Pattern 'is_loopback|allow_remote'   # 0 命中
Select-String -Path crates\daemon\src\api.rs -Pattern 'memory_migrate_distilled_prefix' -Context 1,1

# CI（A-4 / A-6）
Select-String -Path .github/workflows/*.yml -Pattern 'uses:|permissions:|^\s+contents:'
Select-String -Path .github/workflows/ci.yml -Pattern '\|\|' -Context 2,2

# 耐久（A-5）
Select-String -Path crates,cli -Include *.rs -Recurse -Pattern 'integrity_check|quick_check'   # 0 命中
Select-String -Path crates,cli -Include *.rs -Recurse -Pattern 'VACUUM INTO'                   # 仅测试
Get-Content crates\store\src\sqlite.rs | Select-Object -Skip 44 -First 8

# 进程（A-7）与脚本杀伐形状
Get-Content crates\daemon\src\orphans.rs | Select-Object -Skip 82 -First 20
Select-String -Path scripts/*.ps1 -Pattern 'taskkill|Stop-Process|Get-NetTCPConnection|netstat'

# SQL 与路径（A-8 / A-9 / A-10 / §1.11）
Select-String -Path crates\daemon\src\wiki.rs -Pattern 'update_build\('
Get-Content crates\daemon\src\wiki.rs | Select-Object -Skip 3398 -First 8
Get-Content crates\knowledge\src\files.rs | Select-Object -Skip 96 -First 60
Get-Content crates\store\src\migrations.rs | Select-Object -Skip 1087 -First 12

# 面板（§1.11）
Select-String -Path panel\src -Include *.tsx,*.ts -Recurse -Pattern 'dangerouslySetInnerHTML|innerHTML|eval\('
Get-Content panel\src\icons.tsx | Select-Object -Skip 60 -First 25
```

---

## 5 纪律回执

* **只读**：唯一写操作是本报告文件本身。**没有起真守护进程**（pid 79984 未启停）、**没有写活库**（`~/.ruagent` 全部走 `mode=ro` 或 `Get-ChildItem`/`Get-Content`）、**没有改任何被审文件**（`crates/`、`cli/`、`panel/`、`scripts/`、`.github/` 一行未动）。
* **不抄明文**：备份文件的内容只报**字节数与行数**（35,843 B / 312 行）与"它是 `memories` 被改前的字节"这一形状；活库只报计数（163 行 / 标记行 0 / tombstone 0）。报告里没有任何 token、密钥、prompt 正文。
* **每条 finding 六要素齐**：`file:line` · 可复现命令 · 证据读数 · 影响面 · **可证伪的修复判据** · 建议 owner；共 **11 条**（≤12），按安全优先排序。
* **猜测与被查过的面分开**：§2（5 条未验证猜想，每条写明我没能证实它的原因）与 §3（未覆盖范围）与 §1.11（已核验为干净的面）三处分开写，"没查到"与"没查"不混。
* **同伴在途编辑（按纪律只记录，不代改）**：审计期间 `.github/workflows/ci.yml`（+263/−16，mtime 2026-09-29T02:42:40）与 `e2e.yml`（+99/−1）被重写，我第一遍读到的 `ci.yml:76` 那条重跑形状在第二遍复核时已不存在 ⇒ 该条改为**已闭环留档**（§1.5），findings 里的 CI 行号一律以 19:2x 复核为准。另：本轮唯一与"被审面"无关但影响我读数的是 `crates/daemon/src/distill.rs` 的两次中间态（`E0282`/`E0599`，另一单在改），本审计不依赖编译，未受影响。
* **与其它单的关系**：`A-4` 与 `A-5` 分别落在 CI 与 store/CLI 面，`A-1`/`A-2`/`A-3` 需要跨区裁决（残余面清单、日志保留、loopback 守卫），**我没有替任何 owner 改一行代码**；三条高优先级 finding 已发消息给 captain。
