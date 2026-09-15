# 20-mdns-advertiser-contract-tests

> Status: `done`（2026-09-15 修复）
> Blocked by: 无
> 关联 spec: [mdns-spec.md](../mdns-spec.md) §6 R1-R5、§7 变异分析、§8 修复方向

## What to build

为 `src/mdns/` 补测试缝 + 契约测试，锁定 14 条行为契约（当前 **0 条覆盖**），并修复审计发现的 unregister 转义缺陷。

## 根因

`src/mdns/` 两个文件（`advertiser.rs` 100 行、`types.rs` 18 行）+ 入口 `src/mdns.rs`（6 行）合计 124 行，**零测试**（无 `#[cfg(test)]`）。

`cargo test --lib mdns::` 会误报 `1 passed`，但那条测试是 `plugin::wasm_runtime::host_impl::mdns::tests::stop_unknown_browser_is_idempotent_false`——**另一个模块**（插件 host-mdns 能力实现），只因过滤器 `mdns::` 是完整路径子串才被匹配。

5 个集成测试构造了 `MdnsAdvertiser::new()` 但**只做装配、无行为断言**：

```
tests/broadcast_shutdown.rs:147   tests/http_auth_biometric.rs:102
tests/pty_session_chain.rs:138    tests/ws_auth_rules.rs:103
tests/ws_session_route.rs:93
```

引用图证明：全仓无任何 `#[cfg(test)]` 引用 `crate::mdns::` 的行为 API → 对 `advertiser.rs` / `types.rs` 做任何不破坏编译的行为变异（删 `advertiser.rs:65` 的 `*advertising = true`、反转 `:34`/`:79` 的分支、删 `:87-89` 的 unregister/shutdown 块），**615 个 lib 测试全部保持绿色**（复核：当前 lib 单测实测 615 个）。

## 生产缺陷（本次审计发现，P0）

`advertiser.rs:86` 拼接的 unregister fullname 未经实例名转义：

```rust
let fullname = format!("{}.{}", name, SERVICE_TYPE);
```

而注册时 mdns-sd 内部会转义（`mdns-sd-0.20.1/src/service_info.rs:66-84,177-178`）：`.` → `\.`、`\` → `\\`，fullname = `{escape(instance_name)}.{ty_domain}`。

`service_name = "BedCode-my.desktop"` 时：注册 `BedCode-my\.desktop._bedcode._tcp.local.`，撤销却查 `BedCode-my.desktop._bedcode._tcp.local.` → `unregister` 返回 Err → 被 `advertiser.rs:87` 的 `let _ =` 吞掉 → **僵尸 mDNS 记录永久泄漏到局域网**，移动端持续发现连不上的设备。

**可达**：非 Windows 平台 `device_name` 直接取 `sysinfo::System::host_name()`（`system/info.rs:53-58`），macOS/Linux 主机名允许含点。Windows `COMPUTERNAME` 不允许含点，不可触发。

## 修复方向

1. **测试缝**：`ServiceDaemon::new()` 硬编码在 `advertiser.rs:39`，抽象为可注入工厂（生产实现包 `mdns_sd::ServiceDaemon`，测试用 Fake 记录 `register` 收到的 `ServiceInfo`、可注入 `unregister` 返回值）。避免测试开真实 multicast socket。
2. **转义回归锁（无需 seam，零网络，可立即做）**：`ServiceInfo::new` 是纯构造、`get_fullname()` 公开（`service_info.rs:303-308`）。把 `advertiser.rs:86` 的拼接抽成纯函数，期望值直接取 `ServiceInfo::new(SERVICE_TYPE, name, ...).get_fullname()`——**两边同源**，杜绝再次漂移。
3. **无网络可测契约**（纯收益）：
   - `new()` → `is_advertising() == false`
   - `stop()` 幂等：连续两次 `Ok(())` 且状态仍 false
   - `SERVICE_TYPE == "_bedcode._tcp.local."` 常量回归锁（跨端发现契约，改 `_tcp` → `_udp` 当前不会让任何测试变红）
   - 注入失败工厂 → 断言 `Err` 后 `is_advertising()` 仍 false（失败不留半状态）
4. **`let _ =` 红线整改**：`advertiser.rs:87`、`:89` 两处静默忽略错误，违反 AGENTS.md §8「重要路径禁止 `let _ =`」。改为 `tracing::warn!` 带 `service_name` / `port` 结构化字段。
5. **状态机回滚顺序**：`advertiser.rs:82` 在 unregister/shutdown 之前就把 `advertising` 置 false，失败后无法重试、daemon 泄漏。把状态置位挪到 unregister 成功之后。
6. **输入校验（P1）**：`AdvertiseConfig` 增加 `validate()` 返回 `AppError::InvalidInput`（`system/error.rs:37-38` 已存在）：`service_name.trim().is_empty()` / `port == 0` / 实例名 >255 字节 → Err。校验必须落 Rust 端。

## 影响面

- 新增 trait seam 改动 `advertiser.rs` 内部结构，`start` / `stop` / `is_advertising` 公开签名不变，`commands/mdns.rs`、`server/supervisor.rs` 调用方零改动。
- 转义修复只在 `service_name` 含 `.` / `\` 时改变行为（macOS/Linux dotted hostname），Windows 侧行为不变。
- 修复后僵尸记录问题消除；撤销失败的错误会进入 `warn!` 日志，可观测。

## 验收清单

- [ ] `advertiser.rs` 引入 daemon 工厂 seam，测试不创建真实 `ServiceDaemon`、不联网
- [ ] 转义回归锁断言在**当前代码下失败**（证明守卫力），修复后通过
- [ ] `new()` / `is_advertising()` / `stop()` 幂等 3 个无网络契约测试通过
- [ ] 注入失败工厂 → 失败后状态未污染、`stop()` 不触碰 Fake
- [ ] `advertiser.rs:87`、`:89` 的 `let _ =` 替换为带结构化字段的 `tracing::warn!`
- [ ] `SERVICE_TYPE` 常量回归锁
- [ ] `cargo test --lib mdns::advertiser` 全绿，且**不再**只匹配到 `host_impl::mdns` 那 1 条
- [ ] `cargo test --lib` 全量 615+ 通过（实测基线 615）

## Comments

2026-09-14 审计创建。P0：转义缺陷导致僵尸 mDNS 记录泄漏（跨端发现破坏）+ 2 处 `let _ =` 违反 §8 红线。

**编号已更正**：由 18 改为 20（与 `18-commands-terminal-bg-path-validation.md` 去重），README 索引已同步。
