# 25 — peer_net.rs 核心连接管理补测试

**What to build:** 为 `peer_net.rs` 的 17 个 pub 方法（16 async：dial/disconnect/consent/trust/shared_dirs）补测试，当前仅 1 个测试覆盖 1533 行代码。

**Blocked by:** 无

**Status:** done（2026-09-15：抽 2 个纯函数 + 13 测试——`trust_entry_to_dto`（list_trusted_peers DTO 转换）、`derive_display_name`（add_shared_directory 展示名派生），并直测 `bus_topic_for`、DiscoveredPeerDto/SharedDirDto From 转换、TrustStore 与 SharedDirStore 的 tempdir 隔离 CRUD；`cargo test --lib peer_net::` 14 绿。AppHandle 依赖的 dial/consent/生命周期仍无测试（需 mock Tauri runtime，未做））

- [ ] `dial_peer` / `dial_peer_endpoint`：成功连接 → DTO 含 connected=true；无效 node_id → 错误
- [ ] `disconnect_peer`：已连接 → 断开成功；未连接 → 返回 false
- [ ] `respond_peer_consent`：accepted=true → 信任建立；accepted=false → 拒绝
- [ ] `list_trusted_peers` / `revoke_trusted_peer`：增删查
- [ ] `add_shared_directory` / `remove_shared_directory` / `list_shared_directories`：CRUD
- [ ] `set_shared_roots`：批量设置
- [ ] `ensure_node_started` / `stop_node_for_plugin` / `sync_node_with_plugin_state`：生命周期
- [ ] `validate_concurrency`：边界值（0/1/255/256）
- [ ] `cargo test --lib peer_net::` 通过

## 证据

`peer_net.rs` 1533 行仅 1 个测试（`dial_connected_payload_carries_connected_true`）。17 个 pub 方法几乎零覆盖。这些函数是 P2P 网络连接管理的核心——dial/consent/trust 是安全边界。

## 修复方向

多数函数依赖 `AppHandle`（Tauri runtime），需先抽取纯逻辑：
1. consent 逻辑：accept/reject 决策为纯函数
2. trust 管理：增删查为纯函数
3. shared_dirs CRUD：纯函数 + DTO 转换分离
4. handler 层保留 actix 接线，核心逻辑走纯函数单测

## 影响面

仅新增测试 + 小范围重构（纯函数提取）。

## Comments

- 2026-09-14 审计发现，见 `../peer-spec.md` §3
