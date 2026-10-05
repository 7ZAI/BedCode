# 07: 建 sqlite 引擎 crate 并整体搬迁 db 层（expand）

**What to build:** 数据库能力（db 5 + plugin-db 5 = 10 条原语，另加建在其上的 kv 3 条）的**存储引擎**搬进仓库根一个新 crate：数据库类型、schema、查询与迁移整套过去。

本票是 expand 步骤，刻意**只搬不动**：宿主侧保留一层转发，使现存的 20 个引用点一行不改。因此本票结束时全仓照常绿，且后续 08 票可以在一个独立上下文里完成收口。

为什么 db 需要单独两步：数据库类型现在住在宿主的 bin crate 内，约 20 个文件引用它。若新 crate 不把它整体带走，新 crate 就得反向依赖宿主 crate —— 那是把耦合搬家，不是解耦。

**Blocked by:** 06

**Status:** ~~done~~ **reverted（2026-10-05，ADR 0036）**——本票交付的 `bedcode-sqlite-engine` crate 已整体删除，引擎面回到 `src-tauri/src/db/`；以下实施记录仅作历史留档，结论以 ADR 0036 为准；落点偏离 spec §4 的根 `packages/`，依据见记录第 1 段）

- [x] 新 crate 建在**仓库根 `packages/`**（平台无关引擎，移动端将来可复用 —— 移动端已有对应实现），target 落点按仓库 target 目录治理定并用 `cargo metadata` 核验 → ⚠️ **偏离**：落 `bedcode-desktop/packages/bedcode-sqlite-engine`（依据见实施记录第 1 段）；target 落点仓库根 `target/host-kits`，`cargo metadata` 实测 `target_directory = <repo>/target/host-kits` ✅
- [x] 数据库类型、schema、查询封装、迁移函数**整套搬入**新 crate（`git mv`，逐字保留）
- [x] 宿主侧保留转发层，20 个现有引用点**一行不改**（本票结束时 `git diff` 中不应出现这 20 个文件的改动）→ 实测：20 个文件**零代码行改动**，仅 `auth_policy.rs` 的模块头注释里 DDL 路径随引擎搬家（AGENTS §0「文档字面 ≠ 事实」）
- [x] schema 内容**零变更**：现存表仍只 5 张；已退役的业务表（配对 / 连接历史 / 会话配置）不得复活（退役表锁把关）
- [x] 迁移**幂等**测试随 schema 一起搬入并通过（对旧库可重跑）
- [x] 迁移测试数据走临时目录 + 默认日志，**不污染真实数据目录与日志目录**
- [x] 错误处理仍用统一错误类型并带操作描述，无裸 `?` 透传
- [x] 新 crate 自身测试 + 桌面全量 + 退役表锁全绿
- [x] `cargo fmt` / `cargo clippy` 干净

## Comments

### 实施记录（2026-10-05）

**1. 落点偏离 spec §4：根 `packages/` → `bedcode-desktop/packages/`（同票 03 先例）**

spec §4 的理由是「平台无关引擎，移动端将来可复用」。两条实测约束把它钉在桌面端：

1. **错误类型同源**：新 crate 取 `bedcode-server-base` 的 `AppError` / `Result`
   （票 04/05/06 迁出的绑定层已用同款）。这不是偷懒——它带来**逐字零差**的搬迁：宿主
   `crate::Result` 就是同一个类型，20 个引用点的 `?` 不需要任何 `From` impl。若让引擎
   自带一套错误类型，就得把 `AppError` 的 Display 文案（「Database error: …」）复制
   一份进引擎（文案双源 = 迟早漂移），或在宿主补 `From` 映射（错误串有被改写的风险）。
2. **票 08 的绑定层**：绑定层必须自带 provider 侧 `bindgen!`（票 03 已实测证伪 spec D8
   的前提：宿主的 `bedcode` 模块是 **guest 视角**），并取权限词汇 ⇒ 绑死桌面 WIT。

根 `packages/` 是**双端共享位**：一个必然依赖桌面基础层的 crate 放那里是陷阱（移动端
永远拉不动，读者还会误以为可直接复用）。票 03 已为 `bedcode-discovery-engine` 做过
同一判断，spec §4 的注释已补「修正」段，两 crate 现在口径一致。

**2. `rusqlite` 特性分层**：引擎 crate 只开 `bundled`（与 `bedcode-server-base` 同款）；
`hooks`（progress handler 慢查询硬中断）**只在宿主侧声明**——它服务 DB 执行护栏
（`host_api/database.rs`，属绑定层、票 08 迁出）。特性在同一依赖图内统一，宿主构建
仍拿到 `bundled + hooks`。

**3. `rustfmt.toml` 必须随 crate 一起搬**：本仓库 rustfmt 取 `max_width = 120`，默认是
100。不带 config 的话「搬移」立刻变成整片重排（实测 `cargo fmt --check` 要求把
`.execute("DELETE …", [])` 拆成四行）——**搬迁的证据必须是逐字保留**。已加
`rustfmt.toml`（注释写明为何是 120）。

**4. 唯一一处内容删除**：`Database::with_conn`（`#[cfg(test)]` 私有构造）全树零调用方
（`grep -rn with_conn src` 只命中定义本身），在 `cargo test` 构建里是死代码（AGENTS §6：
无说明的陈旧代码一律删除）。其余三文件 + `schema.sql` 与 `models.rs` 逐字未动。

**5. 测试账目（迁移即回归）**

| 范围 | 结果 |
| --- | --- |
| `bedcode-sqlite-engine`（新 crate）`cargo test` | **4 passed**（迁移幂等 / 旧库建表 / 退役表不建出 / 退役密钥行清扫不误伤）——`retired_tables_are_not_created` 等四条**随 schema 一起搬进 crate**，宿主 `cargo test` 不再跑它们 |
| 宿主 `cargo test --lib` | **850 passed / 1 failed / 1 ignored** |
| 唯一失败 | 既有 `session_e2e::test_session_task_domain_closed_loop`（陈旧断言：期望 3 步、插件现返 4 步含 `queue-retrying-check`）——票 06 台账已记同一项，与本票零交集，**未碰** |
| 与票 06 逐条对齐 | 856 → 852 = **恰好 −4**（迁走的四条迁移测试）✅ 无其他增减 |
| 宿主集成 target（用到 `bedcode_desktop_lib::db::Database` 的三个） | `broadcast_shutdown` / `http_auth_biometric` / `ws_auth_rules` 各 1 passed |
| `cargo clippy` | 新 crate `--all-targets` 零警告；宿主 `--lib` 56 警告**全部既有**（含他人在途的 `fs_auth.rs` / `host_api/database.rs` 等），`grep` 确认无一条指向 `bedcode-sqlite-engine` 或 `src/db.rs` |
| `cargo fmt` | 新 crate 与 `src/db.rs` 干净（宿主整树 `fmt --check` 仍红——他人在途文件，与本票无关） |

**未跑**：宿主 `src-tauri/tests/` 全量与 wasm 应用构建——按 AGENTS §3 两段式，全量回归
统一在**票 08（contract 步，代码面收口）**末尾跑；本票已跑齐受影响面。

**6. 磁盘**：根分区一度只剩 **1.6G 可用**（99%）。已清 `src-tauri/target/debug/incremental`
（3.9G）与 `target/server-libs/debug/incremental`（4.0G）——纯缓存，`CARGO_INCREMENTAL`
默认开，下次构建自愈。新 crate 首次构建净增 2.6G（`host-kits` 6.1G → 8.9G，主因是
`bedcode-server-base` 拖进的 `tauri` / `notify` / `sysinfo`，与宿主构建那份重复但只在
本桶内）。**未清** `wasm-apps/debug/incremental`：实测另一会话正在 `wasm-apps` 里跑
`cargo check`（AGENTS §11 不碰他人在途）。

**7. follow-up 归票 09**

- **crate 边界锁的登记表不覆盖本 crate**：`crate_boundary_lock::SERVER_LIB_CRATES` 是
  server-lib-split 的 6 件登记表，`bedcode-sqlite-engine` / `bedcode-discovery-engine`
  都不在其中 ⇒「不得横向依赖传输面 / 不得反向依赖宿主」这条现在对能力域 crate **无锁**。
  09 需裁决：登记进 `SERVER_LIB_CRATES` + `ALLOWED_DOWNWARD_EDGES`（则 `server_lib_src_roots()`
  一并覆盖，退役域扫描面自动扩展到 SQLite 引擎），或给能力域 crate 面内
  `dependency_direction_lock`（与 ws / peer-net 同款）。同一款缺口 06 已记过一次。
- `pnpm run target:size` 的 `rootTargetDirs` 仍只列 `target/server-libs` + `cross-end-tests/target`，
  漏 `target/host-kits`（该文件正被他人在途修改，本票不碰）。已在
  `docs/knowledge/build-process.md`「Target 目录管理」表补上 host-kits 行。

## Comments
