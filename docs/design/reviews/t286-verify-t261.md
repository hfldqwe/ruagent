# t286 独立验证 t261（keyword 腿三段降级）

验证者：ui-audit · 任务 t286（verification round 1）· 被验证对象：提交 **23add4e**（t261）
结论：**needs_revision**（3 条 findings；代码本身未发现缺陷，问题在自报数字与判据口径）

## 0 改前取法（本单必须写明）与它的证明

**取法：改前提交的树 + 当前 harness 原样拷入（仪器恒定）+ 链接证明。**

```bash
git archive 880764b | tar -x -C C:/tmp/t286/before          # 23add4e 的父提交
cp crates/knowledge/tests/retrieval-quality.rs C:/tmp/t286/before/crates/knowledge/tests/
sed -i 's/^version.workspace = true$/version = "0.3.1"/' 该树/crates/{knowledge,store}/Cargo.toml   # 避开 t256 登记的跨树产物串扰
```

| 证明 | 读数 |
| --- | --- |
| 仪器两棵树逐字节相同 | `retrieval-quality.rs` md5 = **ea8698092c7fc0011ccd31f26739f777**（改前树 == 仓库树） |
| 改前产物确实是那个提交的字节 | 改前树 `crates/knowledge/src/store.rs` md5 = **24b97e90ecf3e70a4f4631861628cd2d** == `git show 880764b:crates/knowledge/src/store.rs` 的 md5 |
| 改后链接的是含 t261 的库（t256 的告警：同一 CARGO_TARGET_DIR 会把别人的产物交给你） | 改后运行 `-v` 链接 `libruagent_knowledge-a54df93ae6f94ec5.rlib`，其中 `KeywordStage` 出现 **5** 次；同目录 `libruagent_knowledge-7ed966b33dcbcd68.rlib` = **0** 次（t261 之前的库） |
| 仓库树未被我的操作改动 | 只读 `git archive` + 临时树；仓库 `git status --short crates/` 全程干净 |

## ① 改前 / 改后 两组数（我自己跑）

| 指标 | 改前（880764b + 当前 harness） | 改后（仓库树 HEAD） | t261 自报 | 判定 |
| --- | --- | --- | --- | --- |
| keyword 非空 | **8/18** | **17/18** | 8/18 → **18/18** | 改前复现 ✓ · 改后**否证**（17，不是 18） |
| fused recall@1 | 0.6667 | 1.0000 | 0.6667 → 1.0000 | 复现 ✓ |
| fused recall@5 | 0.8667 | 1.0000 | （自报同） | 复现 ✓ |
| fused MRR | 0.7500 | 1.0000 | （自报同） | 复现 ✓ |
| n（可答查询） | 15 | 15 | — | 一致 |

改后唯一的空腿查询 = **`how do I change a bicycle tyre`**（class = no-answer）。

## ② 我自己的逐词 verdict（不复用 t261 的 coverage.out）

对象集 = harness 的 `CORPUS`（我从源码解析：**16 篇 / 1925 字符**）与 18 条查询；采样面 = 我自己的切词器（按 unicode61：非字母数字分隔、汉字整段一个 term）+ 子串存在性判定；判据 = 「term 存在于语料 ⇒ keyword 腿必须非空」。

| 读数 | 值 |
| --- | --- |
| 可达条数（至少一个 term 存在于语料） | **18 / 18** |
| 非空条数（harness 逐查询 `keyword_hits` 非空） | **17 / 18** |
| 违例 | **1 条：`how do I change a bicycle tyre`**（在语料里只有 `do` / `i` / `a` 这类命中） |

口径说明（诚实标注）：这 1 条违例只在**子串读法**下成立。产品按设计**不给单字符 ASCII 走 LIKE**（`fts.rs` 的 like_patterns 明确跳过），而 `a`/`i` 是单字符、`do` 只在别的词内部出现（不是 token 前缀）⇒ 按「token 或 token 前缀」读法它是**不可达**的。
⇒ 判据本身没写清「存在」的定义，两种读法给出 18 或 17。

## ③ 旧构造的重实现是否还在

```
grep -rln 'legacy_match|legacy_keyword_hits' crates/*/tests/      → 0 个文件
ls crates/knowledge/tests/                                        → retrieval-legs.rs  retrieval-quality.rs（t261-live-reading.rs 已不存在）
grep -n 'legacy_|keyword_stage|Like' crates/knowledge/tests/retrieval-quality.rs → 无命中
```

⇒ 测试里**没有**对旧构造的重实现，也没有 t261-live-reading.rs 之类的旁路探针 ✓（对比：t254 时工作区还有 legacy_keyword_hits，现已清除）。

## ④ LIKE 段的分数（不许有编出来的分数）

| 证据 | 读数 |
| --- | --- |
| 源码 | `store.rs:599` `Ok((r.get(0)?, 0.0f32))`（substring 段硬编码 0.0）· `store.rs:60/95-98` `raw_score` 文档 + `keyword_stage` 字段（`KeywordStage`） |
| 我跑的测试（7 passed / 0 failed） | `keyword_leg_stage_is_substring_for_a_han_substring` ⇒ `assert_eq!(h.raw_score, 0.0, "the substring stage has no bm25")` **通过** ✓；另有 precision / prefix-only / empty 三条 stage 断言通过 |
| harness 语料上的实测 | 18 行里 **没有任何一行 raw_score = 0.0** ⇒ LIKE 段在这套语料上**从未触发**（`泡茶 水温`、`手冲咖啡 研磨度` 由 prefix 段以 bm25 −4.11 / −3.43 命中） |

⇒ 结论：LIKE 段分数**确实是 0.0 且带 stage 标注**（测试级实测），但**harness 的逐查询表里看不到它**（findings F-286c）。

## 结论：needs_revision + findings

| id | severity | problem | requiredFix |
| --- | --- | --- | --- |
| F-286a | low | t261 自报 keyword 非空 **18/18**，同一仪器实测 **17/18**（差的那条是 no-answer 查询 `how do I change a bicycle tyre`） | 把 output/文档里的数字改成 17/18；或写明该条为何算可达 |
| F-286b | low | 判据「凡 term 存在于语料的查询，keyword 腿必须非空」没写「存在」的判定口径（token/前缀 vs 子串）。按子串读法有 1 条违例，按 token-前缀读法 0 条 | 把判据写成「term 作为 token 或 token 前缀存在」并补一条反例断言（单字符 ASCII 不保证命中） |
| F-286c | low | LIKE 段在这套语料上从不触发（18 行 raw_score 无 0.0），只在单元测试里被覆盖 ⇒ harness 的逐查询表看不到 0.0 行 | 给 harness 加一条汉字子串查询（如 `潜艇` 对 `蓝鲸潜艇`），让逐查询表出现一行 raw_score = 0.0 + stage=Substring |

## 未取到 / 跳过（不静默）

* 我另写的仓外 LIKE 探针（`C:/tmp/t286/probe`，只用 knowledge+store+tokio）**链接失败**（共享 target dir 里 lancedb/fastembed 的链接冲突，输出被截断），没有重试 —— LIKE 段因此改用「我跑 t261 自己的 stage 测试」这条路径取证（上面 ④）。可证伪：把探针放到独立 target dir 重跑，应打印 `stage=Substring` 且 `raw_score=0.0`。
* 未跑全 workspace 测试（不在本单判据内，且工作区有队友在途改动）。


---

# 更正（同日，接到 retrieval 的时间线说明后重测）—— 撤回 F-286a，结论改为 pass

**retrieval 是对的：我那条 17/18 是「工作区在途树（t261 + 未提交的 t270 守卫）」的读数，不是 t261 提交的读数。**

## 重测：把改后侧钉到提交 23add4e 本身

```bash
git archive 23add4e | tar -x -C C:/tmp/t286b/after        # t261 自己的提交
sed -i version 0.3.2 该树/crates/{knowledge,store}/Cargo.toml   # 避开跨树产物串扰
cargo test -p ruagent-knowledge --test retrieval-quality -- --nocapture
```

| 运行 | 产物 | 仪器 | keyword 非空 | fused |
| --- | --- | --- | --- | --- |
| 钉住提交 23add4e | 23add4e | 23add4e 自带（md5 6f0ffb09…） | **18/18**（空腿 = 无） | 1.0000 / 1.0000 / 1.0000 |
| 钉住提交 23add4e | 23add4e | **当前 harness**（md5 ea869809…，与仓库一致） | **18/18** | 1.0000 / 1.0000 / 1.0000 |
| 我 21:49 的那次 | **工作区**（含未提交的 t270 守卫） | 当前 harness | 17/18 | 1.0000 / 1.0000 / 1.0000 |
| 改前 880764b | 880764b | 当前 harness | 8/18 | 0.6667 / 0.8667 / 0.7500 |

⇒ **仪器被排除**（同一仪器在钉住提交上给 18/18），17 与 18 的差 **100% 来自产物**，即 t270 的 `MIN_RECALL_ASCII=3` 守卫。
⇒ **t261 自报的 8/18 → 18/18 与 fused 四组数在其自身提交上全部复现** ⇒ **F-286a 撤回**。

## 身份证明的补强（这次做对了）

钉住树的链接证明：`libruagent_knowledge-7832acd6bb5bc968.rlib` 里 `KeywordStage` = **5**（t261 的符号）而 `MIN_RECALL_ASCII` = **0**（**没有** t270 的守卫）⇒ 一次 grep 就同时证明了「是 t261」与「不是 t270」。
我上次只证明了「是 post-t261 的库」（`KeywordStage`=5），**没有**证明「等于 23add4e 的字节」——**把「链接证明」当成了「提交证明」，这是本单的方法学错误**。工作区在途改动对验证者的杀伤面与对实现者一样大；t257/t252 登记的「被测改动必须先进提交再派验证单」这条规则，这次是**我**踩了。

## 那条「违例」其实是 t270 的预期效果（独立复现）

钉住提交上 `how do I change a bicycle tyre` 的 keyword 腿 = **10 条命中**，raw_score = `[-2.065353, -3e-06, -3e-06, -3e-06, -3e-06, -2e-06, -2e-06, -2e-06, -2e-06, -2e-06]` ⇒ 除首条外全是 **bm25 ≈ 0 的 1–2 字符前缀噪声**（`"a"*` / `"do"*` / `"i"*`）；t270 的守卫上线后同一查询变 **0 命中**。⇒ 我的「违例」= 独立测到了 t270 的修复效果，不是 t261 的缺陷。

## findings 的修订

| id | 修订后状态 |
| --- | --- |
| F-286a | **撤回**（t261 的 18/18 在 23add4e 上复现） |
| F-286b | **保留并改写**（low）：判据「凡 term 存在于语料的查询，keyword 腿必须非空」在纯 t261 下成立**只是因为 1–2 字符前缀命中噪声**（raw ≈ −3e-06），t270 守卫上线后同一查询即变空 ⇒ 它不是稳定不变量。判据必须写明「term 存在」的口径（token / token 前缀）**以及最小长度守卫**，并补一条反例断言。 |
| F-286c | **保留**（low）：钉住提交上 harness 的 18 行里**仍无任何 raw_score = 0.0** ⇒ substring 段在这套语料上从不触发（只在 `retrieval-legs` 的单测里被覆盖：钉住提交上我跑 = 7 passed，含 `assert_eq!(h.raw_score, 0.0)`）。建议给 harness 加一条汉字子串查询（`潜艇` 对 `蓝鲸潜艇`）。 |

## 更正后的结论

**pass** —— t261 的三条声明（keyword 8/18→18/18 · fused recall@1/@5/MRR 0.6667/0.8667/0.7500 → 1.0000/1.0000/1.0000 · 逐腿 stage 标注与 substring 段 0.0 分数）在其提交 23add4e 上全部复现；两条 low 观察（判据口径 / harness 未覆盖 substring 段）不是缺陷。
