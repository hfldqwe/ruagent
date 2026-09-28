# 非 loopback 绑定必须显式解锁（A-3）—— t76 落地 + t87 重跑

- 单号：**t76**（实现，失败于同树破损）· **t87**（重跑，本轮）
- 归属：**integ**（INT；`cli/` 无属主，由集成面属主代管）
- 审计来源：`docs/design/reviews/gen3-audit-security.md` **A-3**（medium，网络暴露）
- 代码（t76 已落地，t87 未再改动）：`cli/src/main.rs` · `crates/daemon/src/lib.rs`

## 1 形状

| 位置 | 内容 |
| --- | --- |
| `crates/daemon/src/lib.rs` | 新增 `pub async fn serve_with_remote(root, addr, allow_remote)`；旧 `pub async fn serve(root, addr)` **保留为薄包装**（`serve_with_remote(root, addr, false)`）⇒ **全仓唯一调用者** `cli/src/main.rs` 之外无人受影响。绑定**之前**加守卫：`!addr.ip().is_loopback() && !allow_remote` ⇒ `tracing::warn!` + `eprintln!` WARN + **`anyhow::bail!`**（非零退出）。同意来源 `remote_bind_allowed()`：`RUAGENT_ALLOW_REMOTE ∈ {1,true,TRUE,yes}`。绑定**之后**日志打印**实际**地址（`listener.local_addr().unwrap_or(addr)`，端口 0 也会解析）并点名暴露面 |
| `cli/src/main.rs` | `Serve` 变体新增 `--allow-remote`（布尔 flag），改为调用 `serve_with_remote(root, addr, allow_remote)`。**默认值未动**：`#[arg(long, default_value = "127.0.0.1:8787")]` |

**本单不做鉴权体系**（captain 已裁定另议）——只加「显式同意」这一个开关。

## 2 四情形读数（最终字节，私有 `-TargetDir` 构建的二进制）

构建：`cargo-team.ps1 -TargetDir %TEMP%\ruagent-t87-target build -p ruagent` ⇒ **exit 0 / 12m03s**；产物 `…\ruagent-t87-target\debug\ruagent.exe` **存在、mtime `09/29/2026 04:35:58`、325,455,872 B**。（上一轮 479.9s 且**产物不存在**，本轮两件都补齐。）

| # | 情形 | 进程 | 退出码 / HTTP | 启动日志行（逐字） |
| --- | --- | --- | --- | --- |
| 1 | 默认 loopback `127.0.0.1:8831` | pid 54744 | alive，`/health` = **ok**（9s 就绪） | `INFO ruagent_daemon: ruagent daemon listening on http://127.0.0.1:8831 (loopback only, no auth)` |
| 2 | `--addr 0.0.0.0:8832` **无同意** | pid 27904 | **exit = 1**，8832 **refused** | `ruagent: WARN: refusing to bind 0.0.0.0:8832: this daemon has NO authentication, so a non-loopback bind publishes every endpoint -- including POST /api/v1/memory/migrate-distilled-prefix, which rewrites stored memory bodies -- to anyone who can reach the port. Pass --allow-remote or set RUAGENT_ALLOW_REMOTE=1 to accept that exposure (see audit A-3, docs/design/reviews/gen3-audit-security.md).` |
| 3 | `--addr 0.0.0.0:8833` + `RUAGENT_ALLOW_REMOTE=1` | pid 93712 | alive，`/health` = **ok**（5s） | `WARN ruagent_daemon: ruagent daemon listening on http://0.0.0.0:8833 -- EXPOSED to non-loopback (explicitly allowed by --allow-remote/RUAGENT_ALLOW_REMOTE): every read and write endpoint is reachable from the network and there is no authentication` |
| 4 | `--addr 0.0.0.0:8834 --allow-remote` | pid 92500 | alive，`/health` = **ok**（5s） | 同 3 的 EXPOSED 行（逐字一致） |

⇒ **默认只绑 loopback、非 loopback 需显式同意、两种同意入口都生效、日志点名实际地址与暴露面**，四件都在盘上有读数。四个进程已全部停掉（`alive_now=0`）。

**第一轮的失败是仪器错，不是功能错**：第一次探针把日志只从 **stderr** 取，而 `tracing::info!` 走 **stdout**；且 6s 就绪等待太短。改为「stdout+stderr 都扫 + 最多 90s 轮询 `/health`」后，三情形全部拿到。

## 3 `migrate-distilled-prefix` 一次 POST 会重写多少行（一手）

在**活库的只读副本**上做（`~/.ruagent/data/ruagent.db` → `%TEMP%\ruagent-t87-ro\data\ruagent.db`，15,149,600 B 逐字节相同；**活库只读、未写**），用同一个私有二进制起在 `127.0.0.1:8835` 后 POST 该端点：

```json
{"scanned":0,"stripped":0,"marker_inside_only":0,"backfilled":0,
 "backup":"…\\ruagent-t87-ro\\data\\backups\\memories-distilled-prefix-20260928T203855Z.txt",
 "samples":[]}
```

- **一手读数 = 今天一次 POST 重写 `0` 行**（`scanned/stripped/backfilled` 全 0），端点自己也落了备份文件 ⇒ 迁移**已经完成过**。
- **审计的「156+ 行」是历史数字**（它描述的是该端点**首次**运行时的规模）⇒ 本报告**不当一手读数引用**；WARN 文本因此走**定性**表述（点名该端点会 rewrite stored memory bodies），**不依赖任何数字**。
- 附带一手事实：库里存记忆的表叫 **`memories`**（不是 `memory`）——我的 python 探针第一次按 `memory` 查，得到 `ERR no such table: memory`（已如实保留在探测记录里）。

## 4 门禁（编译面 / 测试面分开，第 6 条）

| 面 | 命令 | 读数 |
| --- | --- | --- |
| 编译面 | `cargo-team.ps1 check --workspace --all-targets` | **exit 0**（树已从 t76 时的 `crates/graph` E0277 破损中恢复） |
| 测试面 | `cargo-team.ps1 test --workspace` | **TEST_EXIT=0**；`Running ` **45** · `test result:` **56** 行 · `ok` **56** · `FAILED` **0** · `panicked at` **0**；**日志中 `ruagent-graph` 的 error 行 = 0** ⇒ 上一轮的拦路错误已消失 |
| 第 16 条 ① | `clippy -p ruagent-daemon --all-targets -DenyWarnings -CleanFirst ruagent-daemon` | **exit 0 / 12.3s**（两件证据 = `-CleanFirst` 强制重查 + `Checking ruagent-daemon`） |
| 第 16 条 ② | `clippy -p ruagent --all-targets -DenyWarnings -CleanFirst ruagent` | **exit 0 / 12.8s**（CLI crate = `ruagent`；同上两件证据） |

## 5 收尾

- 临时 root `%TEMP%\ruagent-t87-bind` / 只读副本 `%TEMP%\ruagent-t87-ro` **已删**（`temp_cleaned=True`）；python 计数脚本已删。
- 私有 `-TargetDir` 实测 **15.55 GiB**，**用完已删**（`cleaned=True`）。
- 探针 PID 54744 / 93704 / 92500 / 27904 / 59284 **全部已停**；**未写活库**（只用只读副本，且副本上跑的是那个一次性端点）；**pid 79984 未触碰**（`StartTime = 09/27/2026 05:35:37`）。

## 6 交回 captain 的 AGENTS.md 建议文本（AGENTS.md 由 captain 维护）

> **绑定默认只走 loopback（t76）。** `ruagent serve` 默认 `127.0.0.1:8787`；非 loopback 地址**必须显式同意**（`--allow-remote` 或 `RUAGENT_ALLOW_REMOTE=1|true|TRUE|yes`），否则启动**非零退出**并打 WARN：点名**实际**绑定地址、说明本守护进程**没有任何鉴权**、且可达实例能改写记忆正文（`POST /api/v1/memory/migrate-distilled-prefix`）。`serve(root, addr)` 仍是 `serve_with_remote(root, addr, allow_remote)` 的薄包装。**改 `cli/`（至今无属主）或 `daemon/` 时不得放宽这条默认。**

## 7 与 t76 的交接

- t76 交付了**代码与形状**（`serve_with_remote` + 守卫 + 日志），但**全部行为读数与门禁都因同树破损未取得**，报告未写。
- t87 **补齐**：编译面绿、产物存在+mtime、四情形行为读数（含 WARN/EXPOSED 逐字）、两条第 16 条 clippy、workspace 测试四计数、只读副本上的迁移行数。
- **未交付/未声称**：鉴权体系（另议）；`--addr` 之外的绑定路径（如未来的配置文件绑定）未审查；四情形只用 IPv4 地址（IPv6 `[::]` 未测）。
