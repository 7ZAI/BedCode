# 桌面端 mdns 模块单元测试审查报告

> 状态: **审计完成，1 张修复票据待处理**（2026-09-14 22:45）
> 范围: `bedcode-desktop/src-tauri/src/mdns.rs` + `src/mdns/`（**3 文件 124 行**）
> 测试规模: **0 个**（无 `#[cfg(test)]` / `#[test]` / `#[tokio::test]`）
> 分支: `dev`（`src/mdns`、`src/mdns.rs` 审计前后均干净，无代码改动）

---

## 1. 摘要（Verdict）

**零测试，且存在「看似被覆盖」的假象，外加一个真实生产缺陷。**

- `advertiser.rs`（100 行）与 `types.rs`（18 行）**零测试**，加入口 `mdns.rs`（6 行）共 124 行无任何测试触达。
- **`cargo test --lib mdns::` 误报 `1 passed`**：那条测试是 `plugin::wasm_runtime::host_impl::mdns::tests::stop_unknown_browser_is_idempotent_false`，属**插件 host-mdns 能力实现**（另一模块），只因 `mdns::` 是完整测试路径的子串才被匹配。用其作为「mdns 有测试」的证据是**虚假覆盖**。
- **5 个集成测试构造了 `MdnsAdvertiser::new()` 但只做装配**（`tests/broadcast_shutdown.rs:147`、`http_auth_biometric.rs:102`、`pty_session_chain.rs:138`、`ws_auth_rules.rs:103`、`ws_session_route.rs:93`），**无一调用 `.start()` / `.stop()` / `.is_advertising()`**。
- **审计发现 1 个真实生产缺陷**（§4 R4）：`advertiser.rs:86` 的 unregister fullname 未经实例名转义，与注册侧不一致；`service_name` 含 `.` / `\` 时撤销静默失败、**僵尸 mDNS 记录泄漏到局域网**。
- **根因**：`ServiceDaemon::new()` 硬编码在 `advertiser.rs:39`，**无测试缝**。
- 违反 §8 红线：`advertiser.rs:87`、`:89` 两处 `let _ =` 静默忽略错误。

---

## 2. 审查基线（全部实跑）

```bash
cd bedcode-desktop/src-tauri
cargo test --lib mdns::
# → test plugin::wasm_runtime::host_impl::mdns::tests::stop_unknown_browser_is_idempotent_false ... ok
# → 1 passed; 0 failed; 613 filtered out; finished in 0.00s   （首次编译 31s，增量 1s）

cargo test --lib -- --list | grep -c ": test$"     # → 614（lib 全量）
cargo test --lib -- --list | grep -i "mdns"        # → 仅上面那 1 条，不在 src/mdns/ 内
```

**判定**：`src/mdns/` 测试数 = **0**。

---

## 3. 审查方法

1. **实跑基线**（§2）。
2. **引用图证明**（替代变异测试）：本任务约束「不修改生产代码」，故不执行「改坏 → 观察变红 → 回滚」。改用全仓 `grep -rn "mdns::advertiser|crate::mdns|mdns::types|MdnsAdvertiser|AdvertiseConfig"`：无任何 `#[cfg(test)]` 引用行为 API → 对 `advertiser.rs` / `types.rs` 做**任何不破坏编译**的行为变异（删 `advertiser.rs:65` 的 `*advertising = true`、反转 `:34` / `:79` 分支、删 `:87-89` 的 unregister 块），**614 个 lib 测试全绿**。5 个集成测试只能拦住签名/类型级变异。结论确定，无遗漏空间。
3. **第三方源码探针**：读 `mdns-sd` 0.20.1（Cargo.lock 实际锁定版本）确认 fullname 构造规则，定位 R4。
4. **模式扫描**：`let _ =`、输入校验缺失、锁持有范围、状态机回滚顺序。

> 未创建任何临时探针 / 变异体文件；`src/mdns` 全程零改动。所有行号由 `bash` 直接从磁盘读取。
>
> **自查纠正**：首轮 `find src/mdns -type f` 只返回 2 个文件，**漏掉模块入口 `src/mdns.rs`**（扁平结构下 `src/mdns.rs` 与 `src/mdns/` 并存）。最终口径 3 文件 124 行。同类作用域错误在 pty-spec.md §11 已复现过一次——审计自身也要校验统计口径。

---

## 4. 覆盖与风险

| 文件 | 行数 | 测试 | 判定 |
|---|---|---|---|
| `src/mdns.rs` | 6 | 0 | ⚪ 仅模块声明，无覆盖需求 |
| `src/mdns/advertiser.rs` | 100 | 0 | 🔴 零测试；无测试缝；2 处 `let _ =`；1 个生产缺陷 |
| `src/mdns/types.rs` | 18 | 0 | 🔴 零测试；跨端契约常量无回归锁；零输入校验 |

14 条行为契约（C-001~C-014，`new` 初始态 / `is_advertising` / `stop` 幂等 / 重复 `start` 早返回 / daemon 创建失败 / `ServiceInfo` 构造失败 / 注册成功三态置位 / 失败不留半状态 / 广播中 stop 撤销 / **unregister fullname 与注册一致** / unregister 失败可恢复 / `SERVICE_TYPE` 单一事实源 / `AdvertiseConfig` 输入合法性 / 调用方 TXT 组装）→ **覆盖率 0.0%**。

| ID | 等级 | 位置 | 风险 |
|---|---|---|---|
| R1 | 🔴 | `src/mdns/` 全部 | 124 行 0 测试，行为回归不会让 CI 变红 |
| R2 | 🔴 | 过滤器 + `tests/*.rs` ×5 | 虚假覆盖：误报 1 passed + 5 处构造-only |
| R3 | 🔴 | `advertiser.rs:39` | 无测试缝，成功路径测试必然开真实 multicast socket |
| R4 | 🔴 | `advertiser.rs:86-87` | **unregister fullname 未转义** → 撤销静默失败、僵尸记录泄漏 |
| R5 | 🔴 | `advertiser.rs:87`、`:89` | 2 处 `let _ =`，违反 §8「重要路径禁止 `let _ =`」 |
| R6 | 🟡 | `advertiser.rs:82`→`:84-90` | `advertising=false` 先于 unregister/shutdown 置位，失败后无法重试、daemon 泄漏 |
| R7 | 🟡 | `advertiser.rs:33`→`:65` | write guard 持有跨两次网络调用，`is_advertising()` 被阻塞 |
| R8 | 🟡 | `advertiser.rs:34-37` | 重复 `start()` 返回 `Ok(())`，调用方无法区分「已启动」与「被忽略」 |
| R9 | 🟢 | `advertiser.rs:45` | 注释称「TXT key 自动转小写」，实际 mdns-sd 对非 ASCII / 含 `=` key 直接 `Err`（`service_info.rs:188-201`），并拒绝单条 >255 字节（`:203-211`） |
| R10 | ⚪ | `types.rs:10` | `AdvertiseConfig` derive `Serialize/Deserialize` 全仓无调用点（仅 2 处构造） |

### R4 详情

mdns-sd 0.20.1 `service_info.rs:66-84,177-178`：

```rust
fn escape_instance_name(name: &str) -> String {
    match ch { '.' => { result.push('\\'); result.push('.'); }
               '\\' => { result.push('\\'); result.push('\\'); } _ => ... }
}
let fullname = format!("{}{ty_domain}", escape_instance_name(my_name));
```

注册（`advertiser.rs:48-51`）经库内部转义；撤销（`:86`）自行拼接**未转义**。`service_name = "BedCode-my.desktop"` 时注册 `BedCode-my\.desktop._bedcode._tcp.local.`、撤销查 `BedCode-my.desktop._bedcode._tcp.local.` → 不等 → `unregister` Err → 被 `:87` 的 `let _ =` 吞掉 → 状态置「已停止」但 PTR/SRV/TXT 永不被撤销，移动端（`peer_net.rs:1075-1081` 发现事件桥接）持续发现连不上的设备。

**可达**：`service_name = format!("{}{}", SERVICE_NAME_PREFIX, device_name)`（`server/supervisor.rs:407`、`commands/mdns.rs:33`），非 Windows 平台 `device_name` 直接取 `sysinfo::System::host_name()`（`system/info.rs:53-58`），macOS/Linux 主机名允许含点。Windows `COMPUTERNAME` 不允许含点，不可触发。

**零网络可测**：`ServiceInfo::new` 是纯构造不建 socket，`get_fullname()` 公开（`service_info.rs:303-308`）：

```rust
let info = ServiceInfo::new("_bedcode._tcp.local.", "my.desktop",
    "my.desktop.local.", "", 8080, Vec::new()).unwrap();
assert_eq!(info.get_fullname(), "my\\.desktop._bedcode._tcp.local.");
// advertiser.rs:86 当前拼法产出 "my.desktop._bedcode._tcp.local." → 不等 → 修复前必失败
```

---

## 5. 变异杀死分析

| 改坏点 | 结果 |
|---|---|
| 删 `:65` 的 `*advertising = true` | **0 变红** |
| 反转 `:34` 的 `if *advertising` | **0 变红** |
| 反转 `:79` 的 `if !*advertising` | **0 变红** |
| 拼错 `:86` 的 fullname | **0 变红** |
| 删 `:87-89` 整个 unregister/shutdown 块 | **0 变红** |
| `SERVICE_TYPE` 改 `_tcp` → `_udp` | **0 变红** |
| `:40` 的 `AppError::Internal` 改成 `Database` | **0 变红** |
| 删 `types.rs:10` 的 `#[derive(Clone)]` | 会变红（**仅**签名/类型级变异能被 5 个集成测试拦住） |

**结论**：所有行为级变异（分支反转、状态不置位、资源不回收、跨端契约漂移）全部免疫，当前防线强度 = 0。

**假设**：`service_name: ""` 的行为未实测——`escape_instance_name("")` 产出空串，fullname 变 `._bedcode._tcp.local.`，是否被 `register()` 的 `check_service_name` 拒绝未验证（`service_daemon.rs:484-485`）；补测试时应先探针确认真实行为再写断言。无 `CONFLICT`：本模块无 ADR 引用，契约全部从代码分支反推。

---

## 6. 修复方向（不修改生产代码，仅建议）

1. **测试缝**：抽象 `ServiceDaemon` 为可注入工厂（生产包 `mdns_sd::ServiceDaemon`，测试用 Fake 记录 `register` 入参、可注入 `unregister` 返回值），避免测试开真实 socket。
2. **转义回归锁**（无需 seam）：把 `:86` 拼接抽成纯函数，期望值取 `ServiceInfo::new(...).get_fullname()`，**两边同源**杜绝再漂移。
3. **无网络契约**：`new()` → `is_advertising()==false`；`stop()` 连续两次 `Ok(())`；`SERVICE_TYPE` 常量回归锁；注入失败工厂断言失败后状态未污染（C-008）。
4. **`let _ =` 整改**：`:87`、`:89` 改 `tracing::warn!` 带 `service_name` / `port` 结构化字段；状态置位挪到 unregister 成功之后（R6）。
5. **输入校验（P1）**：`AdvertiseConfig::validate()` 返回 `AppError::InvalidInput`（`system/error.rs:37-38` 已存在）：空 `service_name` / `port == 0` / 实例名 >255 字节 → Err，校验落 Rust 端。

---

## 7. 票据与清理

票据：[18-mdns-advertiser-contract-tests.md](./issues/18-mdns-advertiser-contract-tests.md)（P0：测试缝 + 14 条契约 + R4 转义回归锁 + `let _ =` 红线）

> **编号说明**：按任务指定用 `18-` 前缀，但 `18-commands-terminal-bg-path-validation.md` 已占用数字 18（README 声明「不回收已用编号」）。slug 不同故无重名冲突，但前缀重复，README 索引需同步声明；是否改号（如 20）由主会话决定。

```bash
cd bedcode-desktop/src-tauri && git status --porcelain src/mdns src/mdns.rs   # → (空)
ps -ef | grep -E "cargo|vitest" | grep -v grep                                # → 无残留
```

- 未创建临时探针 / 变异体文件；新增文件仅本 spec + 票据，均在 `.scratch/unit-test-audit/` 审计台账下。
- 观察到一个 **19:24 启动的 gradle daemon**（早于本会话 22:33，非本次测试所开），未清理。
- `target/` 为 **17G**（超 AGENTS.md §3 的 15GB 阈值）；本次未 `cargo clean`，理由是增量编译仅 1s，清理需重编 wasmtime/tauri/actix 全家桶；建议发布前择机清理。
