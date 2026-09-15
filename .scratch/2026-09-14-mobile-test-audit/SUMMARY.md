# 移动端单元测试分模块审核 — 最终汇总

审核时间：2026-09-14
审核标准：unit-test-discipline G1-G6 硬性门禁
Workflow：4241958a（6 lane，并发 2，分 3 波，模型 sensenova/sensenova-6.8-flash-lite）

---

## 1. 评分总览

| Lane | 模块 | 文件 | 用例 | 行数 | 总分 | 判定 |
|---|---|---|---|---|---|---|
| fe-utils-config | utils/config/plugin/services | 12 | ~54 | 877 | **71.7** | ⚠️ 需重写（部分） |
| fe-stores-views | stores/components/views | 10 | 60 | 2038 | **81.8** | ✓ 通过 |
| fe-composables | 组合式函数 | 11 | ~87 | 2247 | **79** | ⚠️ 需局部重写 |
| fe-integration-ft | 集成测试 + file-transfer | 15 | 107 | 3748 | **84** | ✓ 通过 |
| plugin-sdk | ai-chatbox 插件 + SDK | 10 | ~179 | 2418 | **81.8** | ✓ 通过 |
| rust-tests | Rust 集成 + 内联 #[test] | ~35 | ~270 | ~4000 | **76.7** | ⚠️ 需局部重写 |
| **合计** | — | **73** | **~657** | **~15300** | **77.5** | — |

**通过 3/6**：fe-stores-views、fe-integration-ft、plugin-sdk
**需重写 3/6**：fe-utils-config、fe-composables、rust-tests

---

## 2. P0 必修项（安全/关键契约）

### 2.1 前端 — 安全敏感路径零覆盖

| 文件:行号 | 问题 | 违反门禁 |
|---|---|---|
| `config/terminalOnboardingSteps.test.ts:26-34` | 恒真 `toBeDefined` + `if(!key) continue` 静默放行缺失 key | G3/G4/G6 |
| `plugin/pluginIcon.test.ts:35-37` | SVG 消毒分支无负例（`<script>`/`onclick`/`javascript:` 注入未覆盖） | G2/G6 |
| `plugin/dialogHost.test.ts:22-28` | `resolveById` 完全无测（30s 超时定点结算安全关键路径） | G1/G2 |
| `services/linkCrypto.test.ts:79-116` | encrypt-only 无 decrypt 对称验证（反解异常分支缺失） | G2/G6 |
| `adapters/usage.ts` | 完全无单测（usage 合并入口零覆盖，NaN/undefined 传播） | G1 |
| `plugin/permission.ts` | 权限仲裁完全无测（错配即插件越权） | G1 |
| `plugin/events.ts` | 事件订阅/触发/clearPluginEvents 清理完全无测 | G1 |

### 2.2 前端 — 实现 bug 被锁死 / 为绿改期望

| 文件:行号 | 问题 | 违反门禁 |
|---|---|---|
| `plugin-loader-gating.test.ts` 全文 | isEnabled=false 意图门禁完全未测（spec §3.5 核心裁决） | G1/G2 |

### 2.3 Rust — 测试锁死实现 bug

| 文件:行号 | 问题 | 违反门禁 |
|---|---|---|
| `wasm_host.rs:497-500` | `test_sanitize_plugin_id` 复制实现逻辑为预期，不引用被测代码 | G4/G6 |
| `wasm_host.rs:559-562` | `test_extract_table_names_quoted` 断言 `my-table`→`"my"`（正则 `\w+` 不匹配 `-`，测试锁死 bug） | G4/G6 |
| `system/info.rs:121-124` | `test_local_ip_addresses` 只有 `let _ =` 无断言 | G3/G4 |
| `http_proxy_flow.rs:279` | 遗留 `eprintln!("DEBUG ...")` 在 CI 刷脏日志 | G4 |

### 2.4 Rust — 核心模块零覆盖

| 模块 | 行数 | 测试数 | 缺口 |
|---|---|---|---|
| `auth/manager.rs` | 725 | 1 | authenticate/refresh/biometric_sign 生产路径无覆盖 |
| `plugin/manager.rs` | 1421 | 4 | init_wasm_runtime 失败、权限闸门、uninstall 副作用无覆盖 |
| `peer_net.rs` | 1915 | 4 | connect/disconnect/握手/超时/消息路由主链路 0 覆盖 |

---

## 3. 高风险未覆盖清单

### 3.1 前端完全无测试的关键路径

- `src/plugin/permission.ts`（权限仲裁，18 权限 × N API）
- `src/plugin/events.ts`（事件订阅/清理）
- `src/plugin/routes.ts`（路由注册）
- `src/plugin/context.ts`（426 行插件 API 注入）
- `src/plugin/loader.ts`（275 行加载流程）
- `src/utils/terminalMetrics.ts`（网格计算上游）
- `linkCrypto.generateEphemeral` / `parseCryptoEcho`（握手安全）
- `pluginIcon.sanitizedSvg`（XSS 防御负例）

### 3.2 前端有测试但缺失关键场景

- `terminalOnboardingSteps`：`tryHintKey` optional 静默放行
- `terminalDimensions`：DPR=0 兜底、可用宽=0 边界
- `terminalIdle`：提示符带用户输入、`$` 中段负例
- `useTuiCompat`：超大 chunk（>64KB）CSI sniffer 内存、MAX_PENDING_DELTA 恰好等于边界
- `useTerminalBuffer`：`terminalGetHistory` 空/失败降级
- `useFileTree`：refresh 失败错误处理
- `writeCoalescer`：dispose 后竞态、MAX_WRITE_CHUNK 恰好等于阈值
- `usePeerDevices`：2.1s 真实 sleep 违反 G4
- `plugin-loader-gating`：isEnabled=false 意图门禁

### 3.3 Rust 有测试但缺失关键场景

- `model/message.rs`：无并发访问测试（Send/Sync 语义）
- `plugin/wasm_host.rs`：UPDATE/DELETE/ALTER/DROP 关键字独立覆盖缺失
- `egress.rs`：X-Forwarded-For / 307 相对跳转 / 空 body redirect
- `connection/heartbeat.rs`：时钟漂移（Pong 后超时仍判定断连）
- `plugin/approval.rs`：approved_permissions 空 + version mismatch 组合拒绝
- `tests/ws_protocol_integration.rs`：多客户端并发连接

---

## 4. 改进优先级

### P0（本次 CI 前必须修）

1. **删除/重写锁死 bug 的测试**：`wasm_host.rs:497-500`、`wasm_host.rs:559-562`
2. **补安全敏感路径测试**：`pluginIcon` SVG 消毒负例、`linkCrypto` decrypt 对称验证、`dialogHost.resolveById`
3. **补核心模块主路径**：`auth/manager.rs`（biometric_sign/refresh）、`plugin/manager.rs`（init 失败/权限闸门）、`peer_net.rs`（握手超时/非法 peer 拒绝）
4. **补零覆盖入口**：`adapters/usage.ts`、`plugin/permission.ts`
5. **清理弱断言**：`system/info.rs:121-124`（无断言）、`terminalOnboardingSteps:26-34`（恒真）

### P1（建议下个迭代修）

6. `plugin-loader-gating` isEnabled=false 门禁
7. `usePeerDevices` 真实 sleep → fake timers
8. `useAiChat` 早返回路径（switchConversation/regenerate/stopGeneration）
9. `terminalDimensions` 精确边界断言
10. `plugin/events.ts`、`plugin/routes.ts` 基础测试

### P2（可选）

11. `terminalMetrics.ts`、`clipboard.ts` 等低风险工具
12. `heartbeat.rs` 时钟注入缝
13. `ws_protocol_integration.rs` 多客户端并发

---

## 5. 整体评价

- **上游水平**：composables / integration-ft / plugin-sdk 三个 lane 评分 79-84，契约追溯、mock 边界、异步确定性处理扎实
- **主要短板**：基础层（utils/config）安全敏感路径覆盖不足、Rust 核心模块（auth/plugin/peer）测试稀疏、adapters 早返回/异常路径缺失
- **修复后预期**：P0 项落地后，fe-utils-config 可从 71.7→82+，fe-composables 79→88-90，rust-tests 76.7→82+，整体均分可达 82+

---

## 6. 详细报告

| Lane | 报告文件 |
|---|---|
| fe-utils-config | `fe-utils-config.md` |
| fe-stores-views | `fe-stores-views.md` |
| fe-composables | `fe-composables.md` |
| fe-integration-ft | `fe-integration-ft.md` |
| plugin-sdk | `plugin-sdk.md` |
| rust-tests | `rust-tests.md` |

所有报告位于 `.scratch/mobile-test-audit/`，含具体文件:行号、违反门禁标识、变异分析、可操作建议。
