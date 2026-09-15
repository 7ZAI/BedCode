# System 模块单元测试审计

**审计日期:** 2026-07-21  
**模块路径:** `bedcode-desktop/src-tauri/src/system/`  
**命令基线:** `cargo test --lib system::` → **38 passed / 0 failed / 0.21s**

---

## 1. 统计总览

| 文件 | 行数 | 测试数 | 覆盖率判定 |
|---|---|---|---|
| `config.rs` | 1176 | 17 | ✅ 充分（覆盖默认值、解析、roundtrip、注释、已删 key 兼容、终端背景图等） |
| `logging.rs` | 1198 | 15 | ✅ 充分（dev/release 语义、level reload、JSON/文本格式、span chain、trim、bootstrap channel） |
| `power.rs` | 266 | 2 | ⚠️ 不足（仅测试 linux_logind `build_inhibit_message()` 纯函数，PowerManager 行为未覆盖） |
| `power_wake.rs` | 301 | 3 | ✅ 充分（`resume_kind()` 和 `should_skip_recovery()` 纯函数覆盖正反例；Windows-only `recovery/imp` 模块无法单测） |
| `info.rs` | 88 | 1 | ⚠️ 不足（仅 1 个 smoke test，`desktop_device_name()` fallback 分支未覆盖） |
| `app_context.rs` | 247 | 0 | ❌ 无测试（builder 验证逻辑、默认值、global() panic 行为未覆盖） |
| `lifecycle.rs` | 327 | 0 | ❌ 无测试（**高风险**：async 钩子注册、优先级排序、超时保护、窗口关闭阻止逻辑全部未覆盖） |
| `error_boundary.rs` | 43 | 0 | ❌ 无测试（panic 恢复基础设施未验证） |
| `error.rs` | 94 | 0 | ⚠️ 低风险（简单 From 转换，可加但优先级低） |
| `constants/` (10 文件) | 227 合计 | 0 | ⚠️ 可忽略（纯常量定义，无逻辑） |
| `constants.rs` | 13 | 0 | ⚠️ 可忽略 |

**总计:** 19 文件 / 3207 行 / 38 测试

---

## 2. 断言强度分析（有测试的文件）

### config.rs — 强 ✅
- 68 个 assert 断言，17 个测试函数
- 覆盖正反例：默认值、覆盖值、roundtrip 对称性、注释忽略、空白 trim、已删 key 兼容性
- 断言具体值（`assert_eq!`），无恒真断言

### logging.rs — 强 ✅
- 60 个 assert 断言，15 个测试函数
- 覆盖 dev/release 双环境语义、日志级别过滤、span chain 完整性、trim 目录边界、bootstrap channel 生命周期
- 断言 JSON 格式可解析、结构化字段正确

### power.rs — 中 ⚠️
- 2 个测试，2 个断言
- 仅验证 D-Bus 消息构造的正确性（纯函数，好的测试目标）
- **缺口**: `PowerManager::enable/disable/is_active/set_enabled` 状态转换未测试

### power_wake.rs — 强 ✅
- 3 个测试，6 个断言
- 覆盖正反例：两种唤醒消息类型、非唤醒消息忽略、0 值边界、隐藏/最小化跳过逻辑
- 测试目标选择正确（纯函数独立于平台）

### info.rs — 弱 ⚠️
- 1 个测试，3 个断言
- 仅验证非空和版本号匹配
- **缺口**: `desktop_device_name()` fallback 分支（COMPUTERNAME 缺失、hostname 为空）、`fallback_os_ip_name()` 多 IP/无 IP 情况

---

## 3. 零测试文件风险分析

### lifecycle.rs — 🔴 P0 高风险
- **327 行，零测试**
- 关键逻辑：
  1. **优先级排序** (`sort_by_key(|e| e.priority)`) — 错误排序导致关闭顺序错乱，如 PluginHost 在 SessionManager 之前清理
  2. **超时保护** (`tokio::time::timeout`) — 超时时钩子被 cancel 但状态可能不一致
  3. **窗口关闭阻止** (`run_window_close_hooks` 任一 hook 返回 false 即阻止) — 逻辑简单但核心安全语义
  4. **并发安全** (`RwLock` + 克隆后释放锁) — 锁竞争/死锁
- 可测试性：纯内存操作，无外部依赖，容易构造集成测试

### error_boundary.rs — 🟡 P1 中风险
- **43 行，零测试**
- `spawn_with_error_boundary` 是所有 `tokio::spawn` 任务的 panic 防护网
- 断言目标：panic 被捕获、日志输出正确、非 panic 正常完成
- 可测试性：需要 `#[tokio::test]` + panic 模拟

### app_context.rs — 🟡 P1 中风险
- **247 行，零测试**
- Builder 验证逻辑（`expect()` panic 路径）未测试
- 默认值（`biometric_challenges` 有默认值，`app_handle` 允许 None）未验证
- 可测试性：构造 mock 依赖即可测试 builder

### error.rs — 🟢 P3 低风险
- 简单 From 实现和 Serialize，逻辑透明
- 测试价值低，可跳过

---

## 4. 修复优先级

| 优先级 | 文件 | 行 | 测试数 | 缺口 | 修复类型 |
|---|---|---|---|---|---|
| **P0** | `lifecycle.rs` | 327 | 0 | 优先级排序、超时保护、窗口关闭阻止 | 新增集成测试（可全 mock） |
| **P1** | `error_boundary.rs` | 43 | 0 | panic 捕获验证 | 新增 `#[tokio::test]` |
| **P1** | `power.rs` | 266 | 2 | PowerManager 状态转换 | 新增纯状态机测试 |
| **P2** | `app_context.rs` | 247 | 0 | builder 验证 + 默认值 | 新增 builder 测试 |
| **P2** | `info.rs` | 88 | 1 | fallback 分支 | 新增单元测试 |
| **P3** | `error.rs` | 94 | 0 | From 转换 | 可选，低风险 |

---

## 5. 结论

- **整体状况:** system 模块测试密度合理（config/logging 两个大文件覆盖充分），但 lifecycle/error_boundary 两个关键基础设施文件零测试，是最大的风险点。
- **38 个测试全部通过，断言强度高，无恒真断言。**
- **建议优先补充 lifecycle.rs 测试**（327 行零测试，关键异步逻辑），其次是 error_boundary.rs 和 power.rs 的状态机测试。
