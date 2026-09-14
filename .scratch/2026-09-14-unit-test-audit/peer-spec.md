# 桌面端 Peer 网络模块单元测试审查报告

> 状态: **审计完成，1 张修复票据待处理**（2026-09-14，23:40）
> 范围: `bedcode-desktop/src-tauri/src/peer_*.rs`（5 文件，4970 行）
> 测试规模: **21 个**（peer_transfer.rs 13 + peer_receive.rs 7 + peer_net.rs 1）
> 分支: `dev`

---

## 1. 摘要（Verdict）

**测试集中在 `peer_transfer.rs` 和 `peer_receive.rs`，`peer_net.rs`（1533 行）和 `peer_remote.rs`（459 行）几乎无覆盖。**

- `peer_transfer.rs`（1731 行，13 测试）：覆盖 pump 调度、terminal 淘汰、pause 路由、history 持久化、DTO 序列化。质量尚可。
- `peer_receive.rs`（1088 行，7 测试）：覆盖 unique_remote_path、collect 展开/去重、配置往返。
- `peer_net.rs`（1533 行，1 测试）：仅 1 个测试（`dial_connected_payload_carries_connected_true`）。20+ pub async fn（dial/disconnect/consent/trust/shared_dirs）几乎零覆盖。
- `peer_remote.rs`（459 行，0 测试）：零测试。
- `peer_migration.rs`（159 行，0 测试）：零测试。

---

## 2. 审查基线

```bash
cargo test --lib peer_   # → 21 passed
```

---

## 3. 总判定表

| 文件 | 行数 | 测试 | 判定 | 关键问题 |
|---|---|---|---|---|
| `peer_transfer.rs` | 1731 | 13 | 🟢 有效 | pump/淘汰/history 覆盖全面 |
| `peer_receive.rs` | 1088 | 7 | 🟡 部分 | 路径处理+配置有覆盖；接收逻辑未测 |
| `peer_net.rs` | 1533 | 1 | 🔴 **形同虚设** | 20+ pub async fn 仅 1 测试 |
| `peer_remote.rs` | 459 | 0 | 🔴 零测试 | 远程设备管理零覆盖 |
| `peer_migration.rs` | 159 | 0 | 🔴 零测试 | 迁移逻辑零覆盖 |

---

## 4. 修复优先级

| 优先级 | 票据 | 内容 |
|---|---|---|
| P1 | 25 | peer_net.rs 核心连接管理补测试（dial/consent/trust） |

---

## 5. 审计纪律记录

- 测试计数：21（grep 精确匹配）
- 零测试文件：2（peer_remote.rs + peer_migration.rs）
