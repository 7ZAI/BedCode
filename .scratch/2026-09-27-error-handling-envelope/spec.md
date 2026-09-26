# 错误处理优雅化专项（Error Envelope & User Prompt Boundary）

> 状态：ready-for-agent（设计已定稿，ADR 0030 为契约单一事实源；本文件为实施拆分）
> 日期：2026-09-27 · 分支要求：dev / feature

## 1. 目标与范围

用户指令：**具体的错误信息与错误调用堆栈不得出现在前端任何界面；界面只给用户友好提示**。

- 范围：桌面端全部程序代码（宿主 `src-tauri/` + `src/`、wasm-apps 四应用）。
- 契约：`docs/adr/0030-error-envelope-and-user-prompt-boundary.md`（信封形状 / 分类规则 /
  硬不变量 / 注册表 v0 / UI 呈现规则）。
- 词汇表：`CONTEXT.md` § 错误处理（错误信封 / 错误码 / 用户提示 / 技术详情 / 兜底错误 / 追踪号）。
- 不在范围：移动端（零影响，error.rs 独立副本）；跨端 HTTP 错误信封（独立任务，两端同步部署，§9）；
  `plugin:notify` 通道（维持现状 + 约定）。

## 2. 核心契约速览

- 信封：`{ code, request_id, params? }`。code 即 i18n key（`errors.<code>`）；request_id 每次失败
  生成（边界转换器，短随机 hex，同条 tracing 日志带出）；params 只收用户安全值。
- 硬不变量：技术详情（错误原文 / anyhow 链 / 堆栈）**不出产生方进程**——信封永不携带。
- 分类：默认 `host.internal`；需 UI 特定文案/参数 → 调用点显式 `AppError::UserFacing{code, params, detail}`。
- 插件：SDK `bail_with_code(code, params)` 产标记 JSON `{"__bedcode_error__":true,"code":…,"params":…}`；
  宿主桥只做机制检测 + 形状校验 + 透传，不解释语义；畸形 → `host.internal` + plugin_id 参数。
- UI：纯友好文案，不显示错误码；v1 不自动重试，仅 `host.invoke.timeout` 有重试按钮。

## 3. 泄漏点全量清单（清剿对象）

### A. i18n key 带 `{error}` 插值（12 组 × zh/en，`src/locales/`）
| key | 位置 | 处理 |
|---|---|---|
| `desktop.server.startFailed/stopFailed/restartFailed` | desktop.ts:80-82 | 去 `{error}`，改静态文案或参数化 |
| `desktop.plugin.activateFailed/deactivateFailed` | :135-136 | 去 `{error}` |
| `desktop.plugin.selfCheckFailed` | :140 | 改 code `host.plugin.self-check-failed` 模板 |
| `desktop.plugin.runtimePanic/runtimeRecoveryFailed` | :141/:143 | 改 code `host.plugin.trap` / `recovery-failed` 模板（`{name}`） |
| `desktop.plugin.installFailed/uninstallFailed` | :289/:298 | 去 `{error}` → 兜底或场景码 |
| `desktop.plugin.approve.failed` | :250 | 去 `{error}` |
| `desktop.degradedReason` | :123 | 退役（改 `errors.host.plugin.degraded` 通用文案） |
| en 对应 | en/desktop.ts 同键 | 同步 |

### B. 裸错误字符串 → toast（9 处）
| 位置 | 现状 |
|---|---|
| `ServerView.vue:496/507/533/667` | `toast.error(e.message)` ×4 → `showUserError(e)` |
| `PluginsView.vue:428` | `installFailed {error: e.message||String(e)}` |
| `PluginDetailView.vue:570` | `errKey {error: e.message||'Unknown error'}` |
| `PluginDetailView.vue:612` | `uninstallFailed {error}` |
| `PluginApprovalDialog.vue:176` | `approve.failed {error: e?.message||String(e)}` |
| `usePluginManager.ts:109` | `{error: msg||'Unknown error'}` |

### C. `state.error` 原文渲染（最严重，mark_error 完整错误串上 UI）
| 位置 | 现状 |
|---|---|
| `contributionKinds.ts::getErrorMessage/getDegradedMessage` | 返回 `state.error` 原文 — 停止渲染消费（函数保留供日志？或删除） |
| `PluginsView.vue:91` | tooltip `getDegradedMessage` → 移除 |
| `PluginsView.vue:245/262` | 列表渲染 `getErrorMessage` → 改 `getStateKey` 通用文案 |
| `PluginDetailView.vue:92-94` | `degradedReason + getDegradedMessage` → 改 `errors.host.plugin.degraded`（`{name}`） |
| 源头 | `errors.rs::mark_error`、`commands.rs`（"auto reload after trap failed: {e}"）→ detail 只进日志 |

### D. updater 错误渲染
| 位置 | 现状 |
|---|---|
| `useUpdateChecker.ts:42/75` → `SettingsAboutSection.vue:54` | `{{ errorMessage }}` 渲染 `e.message` → 通用文案「检查更新失败，请稍后重试」+ `logger.error` |

### E. composable 抛带插值 Error
| 位置 | 现状 |
|---|---|
| `useServer.ts:100/113/126` | `throw new Error(i18n(...,{error:e}))` → 改为 reject UserError 或调用点 `showUserError` |

### F. 超时错误
| 位置 | 现状 |
|---|---|
| `utils/invoke.ts::InvokeTimeoutError` | message=key 含 `{cmd}` → 改 reject `host.invoke.timeout` UserError（params `{seconds}`），类可退役 |

### G. 事件通道 toast
| 位置 | 现状 |
|---|---|
| `runtime-listeners.ts` `plugin:error` | `selfCheckFailed {error}` → `host.plugin.self-check-failed`（`{plugin}`），detail 只日志 |
| `runtime-listeners.ts` `plugin:runtime-error` | `runtimePanic/RecoveryFailed {error}`（注释自述 toast 截断展示回溯）→ trap/recovery-failed 信封模板，detail 只日志 |
| `runtime-listeners.ts` `plugin:notify` | 维持现状（业务通知），约定不携带技术详情 |

## 4. 阶段拆分

### P1 Rust 边界（宿主侧核心）
- [ ] `system/error.rs`：新增 `AppError::UserFacing { code: String, params: serde_json::Value, detail: String }`；
  `impl Serialize` 改输出信封对象 `{code, request_id, params}`（request_id 序列化点生成 + 同条
  `tracing::error` 带 `request_id`/`error=%detail`）。`Display` 不变。
  注意：`Serialize` 产生副作用（生成 id + 记日志）需在 impl 内注释说明；Tauri IPC 恰好调用一次。
- [ ] `wasm_core/manager/host/errors.rs`：`notify_plugin_runtime_error` 载荷 → `{code, request_id, params:{name}}`
  （kind→code：panic/trap→`host.plugin.trap`，recovery_failed→`host.plugin.recovery-failed`）；detail 只留日志。
- [ ] `wasm_core/manager/host/commands.rs`：`invoke_wasm_command` 插件错误标记检测
  （`__bedcode_error__` marker）→ 透传 `{code, params}` 合并为 UserFacing；畸形/未标记 → `host.internal`
  兜底 + 原文日志。`schedule_plugin_reload_after_trap` 通知改信封。
- [ ] `wasm_core/manager/host/api_bridge.rs`：`plugin_mark_error` 通道（plugin:error）→ 信封；
  调用点 `not-activated` / `not found` / FileScan TS-only → UserFacing 场景码。
- [ ] 宿主外壳场景码：`host.plugin.*` 显式 UserFacing 于既有调用点（activate/deactivate/approve/install 失败路径）。
- [ ] 测试：信封序列化测试（形状 / 无 detail 字段 / request_id 存在 / code 稳定）；UserFacing 变体；
  marker 透传 + 畸形兜底；事件载荷信封。改在 `src-tauri` 跑 `cargo test` 过滤。

### P2 SDK 插件侧（零 ABI）
- [ ] `packages/plugin-sdk-desktop/rust`：`bail_with_code(code, params)` / `PluginError::business(...)`
  辅助（产标记 JSON 错误字符串）+ 常量 `ENVELOPE_MARKER`；doc 注释契约。
- [ ] SDK 插件模板（template/）与 4 个 wasm-app：`errors.*` 文案骨架（zh/en）在各自 `src/locales/`；
  抽首批业务码（如 `com.bedcode.terminal-session.session-not-found` 等，由各插件维护清单）。
- [ ] WASM 导出签名不变（witness：`invoke_command` 仍是 `Result<Value, String>`），ABI 不 bump。

### P3 前端消费层
- [ ] `src/utils/userError.ts`：`UserError{code, request_id, params?}`；`parseInvokeError(e)`（对象 → UserError，
  未知形状 → `host.internal`）；`showUserError(err, fallbackCode?)`（集中 toast 友好文案；次要行不显示码；
  `logger.error(code, request_id, err)`）；`userErrorFromUnknown(e)` → `frontend.internal`；
  重试按钮（仅 `host.invoke.timeout`，回调重发原命令）。
- [ ] `src/utils/invoke.ts`：超时 → reject `UserError('host.invoke.timeout', {seconds})`；`InvokeTimeoutError` 退役。
- [ ] `src/utils/frontendLogger.ts`：release 下 error/warn 级仍转发 `report_frontend_log`
  （info/debug 裁剪；复用攒批/节流；`configureLogger` 增加级别策略）。
- [ ] i18n `errors.*` 注册表 v0 全量 key（zh + en 同步），兜底文案「操作未完成，请稍后重试」。
- [ ] 测试：`parseInvokeError` 形状矩阵；`showUserError` logger 断言；超时码；release 转发（mock invoke）。

### P4 全量清扫 + 回归
- [ ] A 组：12 组 i18n key 去 `{error}`（文案按 §3 表）。
- [ ] B 组：9 处改 `showUserError`。
- [ ] C 组：`state.error` 渲染退役（PluginsView tooltip/列表、PluginDetailView 降级段），
  `getErrorMessage/getDegradedMessage` 消费路径清理（函数若仅日志用则改签名或删除）。
- [ ] D/E/F/G 组：updater 通用文案、useServer 模式改造、invoke 超时、runtime-listeners 三处。
- [ ] 测试更新：受影响的现有用例（`plugin-loader-gating` 等）；新增防泄漏回归：
  ① 信封序列化断言不含 detail；② i18n 键无 `{error}` 残留的扫描测试；
  ③ `showUserError` 永不渲染 code/detail 的断言（防回接锁式）。
- [ ] 收尾：`pnpm run test:run` 全量 + `cargo test` 全量 + `pnpm exec eslint .` 0 error；
  `cargo fmt`/`clippy` 自查；lens_diagnostics 无 blocker。

## 5. 验收标准
- 前端任何界面（toast / 页面 / tooltip）不再出现错误原文、堆栈、错误码、命令名。
- 技术详情（含前端 release 侧）全部可追溯落地（Rust tracing / frontend logger error-warn）。
- 任一 invoke / 事件失败在前端得到友好提示且不抛异常吞错。
- ADR 0030 注册表 v0 全部 code 有 i18n 文案（zh/en）与测试。
- grep 复核：`src/` 无 `{error}` 插值、无 `e.message` 直显、无 `state.error` 渲染。

## 6. 备注
- 依赖：P3 依赖 P1 的信封形状；P2 与 P1 可并行；P4 依赖 P1-P3。
- 相关文档：ADR 0030 / CONTEXT.md 错误处理词汇 / plugin-development-checklist 错误推送约定 /
  日志红线 AGENTS §8（凭据只记长度，禁止凭据进 params/文案）。