# 票 05 — 锁与索引：browse 单例源码锁 + 依赖方向锁扩展 + 日志核验 + code-map

Status: resolved
Type: task
Blocked by: 04

## ① 源码锁（新）`packages/bedcode-discovery-engine/src/mdns_browse_singleton_lock.rs`（src 内模块）

> **实施修订（2026-10-16）**：原定 `tests/` 路径与治理锁冲突——`bedcode-desktop/src-tauri/tests/capability_crates_unit_tests_only.rs` 禁止**所有** `packages/bedcode-*` crate 出现 crate 根 `tests/`（例外桶现仅 `bedcode-host-kit`，不得新增）。改按先例 `packages/bedcode-server-peer-net/src/dependency_direction_lock.rs`：锁住 `src/` 内模块，`src/lib.rs` 接线 `mod mdns_browse_singleton_lock;`。

扫描面：`packages/**/src` + 双端 `src-tauri/src` 的 `.rs` 源文件（排除 target 与生成物；**锁自身文件不参与扫描**，同既有锁的自锁规避口径）：

- `.browse(` / `.stop_browse(`：**仅允许** `packages/bedcode-discovery-engine/src/type_hub.rs`——白名单精确到文件，条目注释写明理由
- `ServiceDaemon::new`：**仅允许** `packages/bedcode-discovery-engine/src/engine.rs`（守护唯一创建点红线的文本锁延伸）
- 实现口径照抄既有锁模式（如 `bedcode-mobile/src-tauri/tests/retired_mobile_mdns_advertise_lock.rs`）：**跳过纯注释行**——否则 `packages/bedcode-wasm-core/src/manager/host/owner.rs:122` 文档注释里的 `host-mdns.browse(service-type)` 会误报
- 哨兵文件 + 文件数下限，防「枚举为空 → 断言恒真」
- 门禁：锁测试绿 + **变异自检**（删白名单条目必红；在别处加 `.browse(` 必红）

## ② 依赖方向锁扩展 `packages/bedcode-server-peer-net/src/dependency_direction_lock.rs`

- `FORBIDDEN_DEPS` 增 `bedcode-discovery-engine`（server-peer-net 不得引——中枢句柄以纯值注入）
- 核对 / 补齐「engine→peer-net」方向白名单口径（`disable_virtual_interfaces` 为既有合法调用，spec §0.4、§3）

## ③ 中枢生命周期日志核验

`created` / `reused` / `join-rebrowse` / `requery` / `drained-stop` 齐备（实现已在票 02，如缺补之）

## ④ 两端 code-map 防回接锁索引登记

`bedcode-desktop/docs/code-map.md` + `bedcode-mobile/docs/code-map.md` 文末防回接锁索引各加新锁条目

## 门禁

- `packages/bedcode-discovery-engine`、`packages/bedcode-server-peer-net` 各自 `cargo test` 绿（含新锁）
- 变异自检结果记入票面 Comments

## Comments

### 改动摘要

- **① 源码锁（新）**：`packages/bedcode-discovery-engine/src/mdns_browse_singleton_lock.rs`（`src/` 内模块，按实施修订；`lib.rs` 以 `#[cfg(test)] mod mdns_browse_singleton_lock;` 接线，照 `server-peer-net` 先例）。扫描面 = 仓库根 `packages/**/src`（只收 `src` 目录内文件）+ 双端 `src-tauri/src` 全树，排除 `target/` 与锁自身；两条判据各自白名单精确到文件（`type_hub.rs` 唯一 `.browse(` / `.stop_browse(`；`engine.rs` 唯一 `ServiceDaemon::new`），白名单条目带 doc 注释写明理由；**跳过纯注释行**（`trim_start().starts_with("//")`，覆盖 `//` / `///` / `//!`）；哨兵文件 4 个 + 文件数下限 400（实测 484）；两条判据各带「白名单非悬空」正向断言（白名单文件必须真有命中），防「枚举为空 → 断言恒真」。
- **② 依赖方向锁扩展**：`FORBIDDEN_DEPS` 增 `bedcode-discovery-engine`（注明中枢句柄以纯值经 `MdnsPort` 注入的理由）；新增 `SHARED_ENGINE_MANIFEST = "../peer-net/Cargo.toml"` 与反向断言——`bedcode-peer-net` 清单不得出现 `bedcode-discovery-engine`，把「engine→peer-net 单向」这条既有合法边（`disable_virtual_interfaces`，spec §0.4）钉死（反向会成环）；模块头断言清单由「三条」改「四条」。
- **③ 中枢生命周期日志核验**：五个 token 在 `type_hub.rs` 全部齐备，**无需改动**——`created`（:335 info）/ `reused`（:298 debug）/ `drained-stop`（:449 info）为日志 message；`requery` / `join-rebrowse` 为 `mdns type hub rebrowse` 日志的 `reason` 字段值（`RebrowseReason::as_str()`，:649-654），均结构化字段（AGENTS §8）。
- **④ code-map 索引**：两端 `docs/code-map.md` 防回接锁索引各加一行（新锁 `mdns_browse_singleton_lock.rs` + 被扩展的 `dependency_direction_lock.rs`）；桌面既有「拆分 crate 边界 … 各面 `dependency_direction_lock.rs`」条目是泛化表述，无需改动。

### 门禁与变异自检结果

- `packages/bedcode-discovery-engine` `CARGO_BUILD_JOBS=2 cargo test`：**45 passed; 0 failed**（含新锁 3 例）。
- `packages/bedcode-server-peer-net` `CARGO_BUILD_JOBS=2 cargo test`：**56 passed; 0 failed**（含扩展后 `dependency_direction_lock` 3 例）。
- 变异自检（自检后已恢复原样，`git diff` 确认无残留）：
  - A 删 `BROWSE_ALLOWLIST` 条目 → `browse_and_stop_browse_are_only_called_by_the_type_hub` **红**（「白名单悬空」）。
  - B 在 `engine.rs` 加一处代码行 `.browse(` → 同例 **红**（预期外命中，定位 `engine.rs:60`）。
  - C 把 `SHARED_ENGINE_MANIFEST` 临时指向确实声明 `bedcode-discovery-engine` 的清单 → `peer_net_stays_a_leaf_below_base_and_shared_engine` **红**（证明反向断言非空转）。

### 偏差与理由

- 扫描面实现：`packages/` 下按「路径含 `/src/` 段」收文件（等价 `packages/**/src`，把 `bedcode-host-kit/fixtures/forced-link-probe/src` 一并纳入），双端 `src-tauri/src` 整树；`src-tauri/gen/` 不在 `src` 下，天然排除。
- ② 的「engine→peer-net 方向白名单口径」落为**反向缺席断言**（peer-net 清单不得引 discovery-engine）+ 模块头说明；未新增可被绕过的「允许边」常量表（该文件既有结构无此类表，最小改动优先）。
- ③ 按票面「如缺补之」核验，五个 token 均在，未改代码。

### 未决问题

- 无。

