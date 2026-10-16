# 票 05 — 锁与索引：browse 单例源码锁 + 依赖方向锁扩展 + 日志核验 + code-map

Status: ready-for-agent
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
