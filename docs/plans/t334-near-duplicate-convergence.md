# t334 — 近重复记忆的一次性收敛（历史数据）

**状态**：已执行（停机窗口内），verify 绿。
**仪器**：`scripts/converge-near-duplicates.mjs`（默认只读；`--dry-run` / `--execute --yes` / `--verify`）
**判据**：`crates/memory/src/dedupe.rs`（t329 交付）的**镜像**，按字符 + fail-closed。两份拷贝会漂移，所以脚本内置 Rust 单测用的**同一批 7 条向量**，每次运行先自检（不一致 exit 2）。本次自检：`7/7 vectors agree`。

## 为什么会有这次收敛

t329 让判据能看见中文、并把命中从「跳过」改成「supersede」，但**历史没有迁移**：`117 / 123 / 126 / 142` 四条同一事实的活行仍在。t329 改变的是**不再增长**，不是**已清干净** —— 这句话就是本单存在的理由。

## 阶段一（只读清单，过门）

- 扫描：`live rows total=161 · profile=44 · live rows with source_episode=4`
- 计划 **2 组**：`keep #142 ← superseded #117/#123/#126`（链 `#117←#123←#126←#142`）与 `keep #147 ← superseded #143`（链 `#143←#147`）
- 反向证据：契约那对（`用户偏好简体中文` vs `用户不使用简体中文`）判定 `Refused(a polarity token is in the difference)`，差异 `偏好 / 不使用`；活库中被拒的极性近邻按对称重叠率 79.6% / 70.7% / 64.4% / 63.2% / 60.7% / 56.5% ⇒ **全部不动**
- 余弦口径：脚本不嵌入（node 无模型）⇒ 引用 t329 的 hash 嵌入器余弦 **0.5000** 与 t325 的**真实模型**读数（同 namespace **33 对 ≥0.95**）
- **两个口径都要写**：我数出 **2 组**（我的可忽略词表含 `用户/偏好/进行` 等），队长数出 **1 组**（只按 `{偏好,进行,用户}` 归一化的更窄口径）⇒ 差的正是 `#143/#147` 那对（差异 `空 / 用户的`）。同一事实、两种度量、各自局限。

## 为什么必须停机窗口（不是兜底）

`POST /api/v1/memory/supersede` 的 body 是 `{id, new_content}`，它走 `write_memory(supersedes: Some(old.id))`；而 `crates/memory/src/write.rs` 的流水线**先做内容哈希去重（step 2）、再做 supersede（step 3）**：

- 要把 `#117` 标 superseded 且活行留 `#142`，`new_content` 只能是 `#142` 的文本 ⇒ **哈希命中现存活行 ⇒ `SkippedDuplicate(142)`，什么都不标**；
- 换任何别的文本 ⇒ **插入新行** ⇒ 存储增长，且留下两行近重复（`--verify` 会红）；
- `DELETE /api/v1/memory/{id}` 是软删 `deleted_at`，不是 `superseded_at`。

⇒ 收敛在**停机窗口**内由脚本直连 DB 完成：窗口内它是**唯一写者**，不违反「绝不打开第二个写连接」。
窗口证据：`stopped pid=20700` · stop notice 已追加（Reason: t330 数据收敛窗口）· 8787 连接被拒 · LISTENING 数 = 0。

## 执行与读数

```
$ node scripts/converge-near-duplicates.mjs --execute --yes
executed: 6 row(s) marked superseded (none deleted), 2 group(s) converged
```

| 读数 | 改前 | 改后 |
| --- | --- | --- |
| live duplicate groups | **2** | **0** ✓ |
| live profile rows | **44** | **40** ✓ |
| superseded rows（总） | 2 | **6**（+4） |
| memories 总行数 | 163 | **163**（一行未删）✓ |
| `--verify` 退出码 | 1（FAIL: 2 groups still live） | **0**（OK） |

链（**真写进 `supersedes` 列**，可查）：

```
#117 supersedes=NULL superseded=1  用户偏好使用简体中文交流。
#123 supersedes=117  superseded=1  用户使用简体中文交流。
#126 supersedes=123  superseded=1  用户使用简体中文进行交流。
#142 supersedes=126  superseded=0  偏好使用简体中文交流。      ← 活行
#143 supersedes=NULL superseded=1  主力项目是 ruagent——一…
#147 supersedes=143  superseded=0  用户的主力项目是 ruagent…   ← 活行
```

## source_episode：保持 NULL，并标注「历史行无来源」

收敛后的活行 `#142` / `#147` 的 `source_episode` **保持 NULL**。原因：t329 之前的写路径**恒传 `source_episode: None`**，这些行的真实来源已经丢失。队长的裁决写在这里作为本单的原则：

> **写一个假的来源比一个诚实的 NULL 更坏** —— 来源缺失是一个事实，不该被一个看起来更好的值覆盖。

（活库现有 4 条带 `source_episode` 的活行来自产品自己的摄取路径，不是本次收敛产生的。）

## 只做 UPDATE，不删任何行

`--execute` 只有两条 `UPDATE`（`superseded_at`/`supersedes`），没有任何 `DELETE`；单一事务（`BEGIN IMMEDIATE` … `COMMIT`，出错 `ROLLBACK`）。`memories` 总行数 163 → 163。

## 怎么复跑

```bash
node scripts/converge-near-duplicates.mjs --dry-run   # 只读：清单 + 反向证据
node scripts/converge-near-duplicates.mjs --verify    # 收敛后：重复组必须为 0，并打印链
```

`--execute` 需要 `--yes`，且按流程应先过「清单 → 队长确认 → 停机窗口」这道门。
