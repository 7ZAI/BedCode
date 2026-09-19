# 08: trust 模块（B3）

**What to build:** 设备信任列表统一视图（DB pairings + peer trust_store 映射），列表 / 撤销行为与宿主实现等价，数据持久化。

**Blocked by:** 07

**Status:** done（2026-09-19）

- [x] 列表 / 撤销行为与宿主实现等价（对照测试）
- [x] 数据持久化，重启后一致；撤销后立即生效
- [x] 插件 cargo test 全绿

## 语义基准（与宿主实现逐条等价）

| 宿主概念 | 宿主实现（行为基准） | 插件 trust 模块 |
| --- | --- | --- |
| 已配对设备 | `pairings` 表 `get_pairings`：仅 `is_active=1`、`ORDER BY paired_at DESC`；`remove_pairing`：`UPDATE ... SET is_active=0` 软删 | `PairingRecord`（host-storage 键 `trust.pairings` 持久化）：active 过滤 + `paired_at` DESC 排序；撤销 = 软删 `active=false` 保留记录 |
| 可信对端 | peer trust_store `list_trusted_peers` → `TrustedPeerDto`（nodeId/displayName/fingerprintShort/addedAt）；`revoke_trusted_peer` → bool | 经 host-peer `list-trusted` / `revoke-trusted` 原语原样映射（条目序保留；`TrustedDeviceDto::from_peer` 字段对齐） |

**统一视图**：`{ devices: TrustedDeviceDto[], peerError: string|null }`——pairing 段
（active 过滤 + paired_at DESC）+ peer 段（宿主条目序）合并，每条带 `kind` 判别
（`pairing` | `peer`）；pairing 字段 `name/fingerprint/addedAt/lastSeen/connectCount/active`
对齐宿主 `Pairing` 公开视图。

**撤销分派**：pairing 目标软删（宿主 `remove_pairing` 语义，未命中幂等 `removed=false`
不报错——对应宿主 UPDATE 影响 0 行）；peer 目标转 host-peer `revoke-trusted`（宿主
`revoke_trusted_peer` 语义，返回是否删除）。返回 `{ removed, kind }`。

**持久化**：pairing 记录真源 = 宿主 `plugin_storage` 表（host-storage 原语，按插件
属主隔离），写入即持久化、撤销软删即时写回 → 重启一致、撤销立即生效。

**降级（peer 侧不可用）**：无头/peer-net 未启动时 host-peer `list-trusted` 报错（宿主
原语显性失败），`list` 不静默吞错——pairing 段照常返回 + `peerError` 透出；`revoke`
对 peer 目标直接上抛错误（不做「看似成功」的假撤销）。

## 验证证据（2026-09-19）

- **插件单测**：`bedcode-desktop/plugins/devices/rust` 全量 **45 passed / 0 failed**
  （trust 17：model 3 + store 7 + ops 7；pairing 28 零回归；0 warning）
  - store：持久化 roundtrip（模拟重启读回一致）、撤销软删跨重启保持、未知 id 幂等、
    重复撤销幂等、损坏存储显性失败
  - ops：active 过滤 + paired_at DESC（对照宿主 `get_pairings`）、peer 映射字段对齐
    （对照宿主 `TrustedPeerDto`）、pairing/peer 合并统一数组、peer 不可用降级透出
    peerError、撤销立即生效、未命中 removed=false、peer 二次撤销 removed=false
  - model：`PairingRecord` serde 缺省（旧数据无 active 字段读回默认受信任，向后兼容）
- **宿主闭环**：`test_devices_plugin_artifact_lifecycle` 扩展（真实 wasip3 产物经宿主
  async 运行时）：`devices.trust.list` 空列表 → `add-pairing` → list 可见（kind/id/name/
  fingerprint/connectCount=1/addedAt RFC3339）→ **持久化断言**（plugin_storage 表
  `trust.pairings` 落库 1 条 active=true）→ `revoke` → list 立即消失 → 未命中 id
  幂等 removed=false → **撤销后落库断言**（记录保留 active=false，重启一致）；无头
  上下文 peerError 透出（`unavailable`/`headless` 字样）断言通过
- **全量**：桌面 `cargo test --lib -- --skip pty` **862 passed / 0 failed**（17 个失败
  全部在并发 pty 线 `pty::*`，在途文件 `pty_process.rs` 等，handoff 明示禁止触碰，
  非本票回归）；`test_devices_plugin_artifact_lifecycle` 单独跑 ok

## 产物

- `plugins/devices/rust/src/trust/`：`mod.rs`（模块文档 + 命令面入口 re-export）+
  `model.rs`（`TrustKind` / `PairingRecord` / `TrustedDeviceDto`，字段对齐宿主
  `Pairing` / `TrustedPeerDto`）+ `store.rs`（`HostAccess` trait + host-storage 持久化
  层 + 对照测试）+ `ops.rs`（`list` / `revoke` / `add_pairing` 编排 + `*_via_host`
  cfg 分流入口，native 显性失败同 keys.rs 模式）
- `plugins/devices/rust/src/lib.rs`：命令面 `devices.trust.list` / `devices.trust.revoke`
  / `devices.trust.add-pairing`（pairing 完成流写入入口；互调 api 面归票 09）
- 宿主 `wasm_runtime.rs`：`test_devices_plugin_artifact_lifecycle` trust 闭环扩展 +
  `parse_rfc3339_for_test` helper
- 产物：`resources/plugins/desktop/com.bedcode.devices/bedcode_plugin_devices.wasm`
  重建（wasip3 Component）

## 后续

- 票 09 consent + 互调 API：`auth.list-trusted-devices` / `auth.revoke-device` 经
  `trust::list_via_host` / `revoke_via_host` 暴露（manifest.api 同步声明）
- 票 11 宿主命令面桥接：`list_paired_devices` / `remove_paired_device` 转发本命令面
- 工作区仍含并发 pty 线在途文件（pty_process.rs 等），勿整仓提交
