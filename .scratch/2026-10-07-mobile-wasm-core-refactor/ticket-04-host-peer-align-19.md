# 票 04 · host-peer 对齐 19（阶段 1 第二票）

> 状态：**已完成（2026-10-07）**。本票只做「增 5 函数 + 引擎支撑」；`resume-all-transfers`
> 删除按 spec 随票 06（file-transfer 迁移后）同批执行。

## 1. 目标与本票实际范围

spec 票 04：「删 `resume-all-transfers`（先随票 06 迁移再删，破坏性）、增 5 函数
（`set-download-dir` / `start-node` / `stop-node` / `active-transfers` /
`collect-outgoing`，纯增量先加）；宿主命令面 `dial_peer` 显式化」。

**本票执行**：
- ✅ 增 5 函数（WIT → SDK → 宿主 component → host_impl 四层）+ 引擎支撑
- ⏳ `resume-all-transfers` 删除：**未做**（spec 明确随票 06 迁移后删，破坏性批次）
- ⏳ 命令面 `dial_peer` 显式化：移动端已有 `dial_peer_endpoint`（peer_net.rs:375，按
  endpoint 拨号引擎入口），WIT `dial-peer` 已是 endpoint 形状——命令面收窄属票 09
  （设备列表投影下沉）范畴，本票不重叠

## 2. 改动清单（12 文件）

| 层 | 文件 | 改动 |
| --- | --- | --- |
| WIT | `packages/plugin-sdk-mobile/rust/wit/bedcode.wit` | host-peer 增 5 函数（复制桌面 v34 签名 + 注释） |
| SDK | `src/host/peer.rs` | trait HostPeer 增 5 方法（对齐桌面 SDK 签名） |
| SDK | `src/wasm_host.rs` | impl 增 5 转调（host_peer::set_download_dir 等） |
| 宿主绑定 | `plugin/wasm_runtime/component.rs` | `impl bedcode::plugin::host_peer::Host` 增 5 方法 |
| host_impl | `plugin/wasm_runtime/host_impl/peer.rs` | 5 函数（permission 门 + run 桥 + 引擎转调） |
| 引擎 | `peer_net.rs` | **属主记账**：PeerNetState 加 `node_owner` + `start_node_owned` / `stop_node_owned` / `release_node_for` + `stop_locked` 清账（对齐桌面审计票 12） |
| 引擎 | `peer_transfer.rs` | `collect_outgoing_files` 改 pub(crate) + `CollectedSources` pub(crate) + `collect_outgoing_for_plugin`（spawn_blocking）+ `active_send_transfer_rows` |
| 引擎 | `peer_receive.rs` | PeerTransferSettings 加 `download_dir` 字段（serde default 兼容旧文件）+ `set_peer_download_dir`（create_dir_all + 热生效）+ `active_receive_rows` + apply_settings 扩展落点 |
| 引擎 | `peer_remote.rs` | `active_pull_rows` + `now_ms` |
| 测试 | `peer_receive.rs` 内联 tests + `peer_receive/tests/settings_default_is_ask.rs` | 构造补 `download_dir: None` |

## 3. 语义对齐与移动端差异（点名）

| 函数 | 桌面语义 | 移动端落地 | 差异 |
| --- | --- | --- | --- |
| `start-node` / `stop-node` | 属主记账（谁起谁停，内核不猜产品 id） | 同款属主记账完整实现 | 无（对齐） |
| `active-transfers` | 三表投影（send 句柄表有 total/transferred） | 三表投影（sessions + pending + pulls） | **移动端句柄表不持有字节计数**（桌面 v31 句柄表有 total_bytes/transferred_bytes；移动端字节在事件流与任务侧）→ totalBytes/transferredBytes 显性给 0，行形状一致 |
| `collect-outgoing` | 目录递归 + 同名去重 | 复用既有 `collect_outgoing_files`（移动端发送路径已用它） | 无（对齐） |
| `set-download-dir` | 持久化 + 热生效 | 同款（settings.download_dir 持久化 + apply_settings 热生效） | 无（对齐） |
| `resume-all-transfers` | 已退役 | **保留**（票 06 删） | 双端 host-peer 现为 19 vs 20（差此 1） |

## 4. 五同步点验证（票 04 门禁）

| 同步点 | 状态 |
| --- | --- |
| ① SDK 常量 | ✅ `PERMISSION_PEER`（"peer"）既有，5 新函数复用统一 PEER 门禁（桌面为函数级 check_permission；移动端为统一门禁——既有设计，5 函数同覆盖） |
| ② 打包 CLI 合法集合 | ✅ 移动端无函数级白名单形态：manifest `permissions` 声明（loader.rs 收集进 granted_permissions）+ 宿主运行时门禁（require_peer_permission） |
| ③ 前端合法集合 | ✅ host-peer 是 WASM 插件面，前端不直调，无前端 capability 变更 |
| ④ 宿主能力清单 | ✅ component.rs `impl host_peer::Host` 19 方法齐备 |
| ⑤ 权限门 | ✅ host_impl 5 函数全部经 `require_peer_permission` |

## 5. 门禁结果

| 门禁 | 结果 |
| --- | --- |
| SDK `cargo check` | ✅ 0 error（crate 单编译单元：wasm_host.rs 编辑触发 bindgen 宏重跑读新 WIT） |
| 宿主 `cargo check` | ✅ 0 error 0 warning（22.53s） |
| 全量 lib 测试 `cargo test --lib` | ✅ **366 passed / 0 failed**（139.60s；含 peer 36 个：collect/去重/历史/闸门/设置往返等） |
| `cargo fmt --check`（本任务文件） | ✅ 干净（全仓 Diff 仅并行会话在途：auth/manager、dev_logs、event_ws、connection/manager——未碰） |
| `cargo clippy --lib` | ✅ 无本任务新增（修了 1 个我引入的：CollectedSources 可见性；余为既有基线） |
| 插件调用点零漂移 | ✅ 未迁移插件（file-transfer 等）无 host-peer 新函数调用点——纯增量，零漂移 |
| 双端 WIT 对照 | ✅ 桌面 19 vs 移动 20（差 = resume-all-transfers，票 06 删后对齐 19） |

## 6. 遗留与接线状态

- `release_node_for` 已实现，**未接线**：插件停用外壳（plugin/manager.rs:806 现调
  `stop_node_for_plugin`）切换随票 06 插件下沉同批（对齐桌面审计票 12「谁起谁停」终态）。
- `sync_node_with_plugin_state`（boot 对账，按 `FILE_TRANSFER_PLUGIN_ID` 硬编码）保留现状
  ——移动端渐进项，桌面已退役，移动端随票 06 对齐。
- 前端命令面 `dial_peer(node_id)`（发现缓存解析）收窄留票 09。
