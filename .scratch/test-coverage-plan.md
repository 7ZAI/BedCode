# 测试覆盖补齐计划（TDD）✅ 已完成

基线（已确认全绿）：
- 桌面 Rust: 251 passed（`bedcode-desktop/src-tauri`）
- 移动 Rust: 143 passed（`bedcode-mobile/src-tauri`）
- 桌面前端: 24 文件 / 252 tests（vitest，include 覆盖 SDK 前端）
- 移动前端: 9 文件 / 97 tests（vitest）

## 最终结果（全部绿）

| 套件 | 基线 | 最终 | 新增 |
|------|------|------|------|
| 桌面 Rust | 251 | **455** | +204（协议 35 + 前端相关 62 + 插件宿主 107） |
| 移动 Rust | 143 | **259** | +116（协议 43 + 连接 45 + 路由/文件服务 28） |
| 桌面前端（含 SDK） | 252 | **345** | +93（主项目 64 + SDK 前端 29） |
| 移动前端 | 97 | **154** | +57 |
| 桌面 SDK rust | 5 | **85 / 92(wasm)** | +80/+87 |
| 移动 SDK rust | 2 | **79 / 89(wasm)** | +77/+87 |
| 桌面 SDK 前端（独立） | 21 | **50** | +29 |
| 移动 SDK 前端 | 0 | **25** | +25（新增 vitest.config.ts） |

## Bug 台账（.scratch/test-coverage-bugs.md）
1. ✅ CloseFrame Display 混入关闭码（两端，已修复）
2. ✅ message_bus publish 静态订阅者消息被丢弃（已修复，测试去 ignore）
3. ✅ log.rs 编译错误条目（并行中间态，过时已解决）

## 插件宿主覆盖明细（本轮重点）
- message_bus 13 / registry 10 / types 9 / watcher 4 / host 27 / component 3
- host_impl/* 13 文件 104（bus/config/events/file_service/fs/lifecycle/log/mod/session/status/storage/terminal/timer）
- api_bridge.rs 不可单测（Tauri State 注入），已文档化
- file_service.rs 纯 re-export 壳，逻辑在子模块（已覆盖）

## 不可测部分（已在测试头注释说明）
- 桌面 `auth_service.rs`：BiometricRequest/BiometricVerify 依赖 `AppContext::global()`（Tauri 全局状态）
- 移动 `file_service/server.rs`：SAF 文件操作依赖 Android 插件（模块内已有端点集成测试）；上传 session 在 upload.rs；路径穿越校验在 sandbox.rs
- 移动 `connection/heartbeat.rs`：Instant 无注入缝，超时判定用最小真实等待

## 流程约定（原计划保留）

1. 每个任务 = 一个垂直切片（测试缝已预定义）
2. 风格：Rust 用文件末尾内联 `#[cfg(test)] mod tests`（仓库惯例，无 *_test.rs 文件）；前端用 `src/__tests__/` 镜像路径
3. **测试发现既有代码 bug（red 且非测试自身写错）→ 不现场修复，追加写入 `.scratch/test-coverage-bugs.md`（用 `cat >>` 追加，先 read 再写）**
4. 所有任务完成后统一汇总 bug，逐个修复（红 → 绿）
5. 禁止：改任务范围外的文件、重构既有代码、提交 git

## Round 1（4 并行，互不冲突）

| # | Agent | 范围 | 缝 |
|---|-------|------|-----|
| 1 | worker | 桌面 Rust 协议层 `server/ws/message.rs` | Message 构造器（~35 个）、message_type/message_id/expect_response、to_json/from_ws_message 往返、serde_json round-trip、request_id 关联 |
| 2 | worker | 移动 Rust 协议层 `model/message.rs` | 对称协议，同上 |
| 3 | worker | 桌面前端 stores + composables | `stores/i18n.ts`、`stores/inputAssistant.ts`、`stores/wsl.ts`、`composables/usePairing.ts`、`composables/useServer.ts`、`composables/useWsl.ts` |
| 4 | worker | 移动前端 stores + composables | `stores/codeViewer.ts`、`stores/settings.ts`、`stores/inputAssistant.ts`、`composables/useFileTree.ts` |

## Round 2（3 并行）

| # | Agent | 范围 | 缝 |
|---|-------|------|-----|
| 5 | worker | 桌面 Rust 安全层 | `utils/auth/pairing.rs`（PairingCode 生成/过期/验证/序列化 fallback 分支）、`server/services/auth_service.rs`（可测部分，重度耦合 Tauri 的记入报告说明） |
| 6 | worker | 移动 Rust 连接层 | `connection/request_response.rs`、`connection/reconnect.rs`、`connection/heartbeat.rs`、`connection/codec.rs` |
| 7 | worker | 移动 Rust 路由 + 文件服务 | `router/router.rs`、`router/registry.rs`、`router/context.rs`、`file_service/server.rs`（纯逻辑：路径校验/分块/范围请求头） |

## 完成后

- 汇总 `.scratch/test-coverage-bugs.md` 的 bug，逐个修复并补回归测试
- 复跑 4 个基线套件确认全绿
- 报告新增测试数量
