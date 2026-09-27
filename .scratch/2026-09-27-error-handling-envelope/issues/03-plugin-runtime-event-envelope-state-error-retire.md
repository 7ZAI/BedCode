# 03：插件运行时异常/自检事件信封 + 插件错误状态原文直显退役

**Type:** task
**Spec:** `../spec.md`（§4 P1 事件子集 + P4 C 组 / §3 C·G 组）；契约单一事实源 `docs/adr/0030-error-envelope-and-user-prompt-boundary.md`（决定 7 / 11）
**Blocked by:** 01
**Status:** done（2026-09-27）

**What to build:** 插件运行出问题（WASM 陷阱、自动恢复失败、启动自检失败、降级运行）时，用户只在 toast / 列表 / 详情页看到友好模板（如「应用「X」运行异常，已尝试自动恢复」「应用「X」降级运行，部分功能不可用」），**任何技术原文（panic 消息、回溯、错误链、恢复失败原因）不再出现在界面**——它们全部只进日志。当前最严重的泄漏面（插件错误状态的完整错误串被列表 tooltip、列表项、详情页直接渲染）在本票内彻底退役。

**集成测试约束（用户指令）**：本票**只编写**集成测试文件，**不执行**；统一在票 05 全量执行。

**Acceptance:**

- [x] 插件运行时异常事件载荷信封化：trap / 自动恢复失败 / 自检失败 / 降级各映射稳定语义码 + 具名参数（应用显示名）；技术详情只留日志（含 request_id）
- [x] 前端事件监听接入统一消费层：三类运行时提示按码渲染友好模板；不再出现 panic 消息/回溯展示（含注释自述的「toast 截断展示」行为）
- [x] 插件错误状态原文渲染退役：列表项、tooltip、详情页降级原因段落不再渲染 `state.error` 原文；改徽标 + 通用文案（降级带应用名参数）
- [x] 状态 i18n 组迁移：自检失败/运行时异常/恢复失败/降级原因相关键去 `{error}`、改用语义码模板（zh + en 同步）
- [x] 单元测试全绿（针对性过滤，不跑集成）；**编写**（不执行）覆盖「事件 → 友好 toast + 详情仅日志」的集成测试
- [x] 交付说明：grep 复核本票范围内插件错误状态零原文直显

## 交付说明（2026-09-27）

**Rust（宿主事件通道）**
- `system/error.rs`：新增 `EventEnvelope { code, request_id, params }` + `payload()`（与 IPC 信封同形状，结构上无字段可装详情）；`new_request_id()` 提为 `pub` 供两侧共用同一生成器；3 个信封单测。
- `host/errors.rs`：kind → 语义码映射 `runtime_error_code`（`panic|trap` → `host.plugin.trap`，`recovery_failed` → `host.plugin.recovery-failed`，未知 → `host.internal` 兜底）；纯函数 `runtime_error_envelope` / `self_check_envelope`（形状可独立回归）；新增 `display_name()`（params 只带 manifest 显示名，查不到退回 id）；`notify_plugin_runtime_error` 载荷改信封、日志带 `request_id`；新增 `notify_plugin_self_check_error`（`PLUGIN_ERROR` 通道）。顺手删掉该函数上重复的 doc 块。
- `host/services.rs::mark_plugin_error` 改为委托上面的自检上报（`AppContext::global()` → `try_global()`，无头/测试不再有 panic 风险）。
- `system/constants.rs`：两个事件常量的 doc 钉住载荷形状与码映射来源。
- `host/commands.rs`：仅注释——`mark_error` 串明确为「宿主侧诊断事实，UI 不渲染」。

**前端**
- `plugin/runtime-listeners.ts`：两个错误通道改走 `parseInvokeError` + `showUserError`（与 invoke 面同一消费层）；删掉按 kind 手挑 i18n 键与 120 字截断展示；`plugin:notify` 维持现状。
- `plugin/contributionKinds.ts`：**删除** `getErrorMessage` / `getDegradedMessage`（原文取值函数无消费者即删，不留「备用」出口）。
- `views/PluginsView.vue`：降级徽章 tooltip → `errors.host.plugin.degraded`（带应用名）；错误态状态位改通用 `getStateKey` 文案；简介行错误态 → `⚠ ` + 通用提示。
- `views/PluginDetailView.vue`：降级段落 → `errors.host.plugin.degraded`（带应用名）。
- i18n：`errors.host.plugin.{trap,recovery-failed,self-check-failed,degraded}` 新增（zh + en 同步）；退役 `degradedReason` / `selfCheckFailed` / `runtimePanic` / `runtimeTrap` / `runtimeRecoveryFailed`（双语言）。

**测试**
- Rust 单测（`host/tests/runtime_preauth_test.rs`）：kind→码映射表、载荷字段白名单 + 无详情、自检载荷参数名、显示名兜底 —— 4 个，与既有节流用例一起 5/5 绿。
- 前端单测（`__tests__/plugin/runtimeErrorEnvelope.test.ts`，19 例）：注册表 v0 四码 zh/en 文案 + 无未替换占位符 + 文案不含码字面量；退役锁（已下线 i18n 键、两个取值函数、两个视图、监听器不得再截断/直译）。
- 既有 `__tests__/integration/plugin-flow.test.ts`：降级 tooltip 断言按新契约改写；新增「错误态插件不渲染原文」一例（该文件本就在常规套件内，改坏必须当次修，故跑了它：6/6 绿）。
- **只写不跑**：`__tests__/integration/plugin-error-envelope.test.ts`（`describe.skip`，7 例：事件 → toast 友好文案 / 详情只落日志 / 畸形载荷兜底 / notify 不变 / 详情页降级段）。仅验证了收集与 fixture 可解析，未执行断言——票 05 统一启用。

**与 spec 的两处偏差（记录）**
1. spec P1 写「`api_bridge.rs` 的 `plugin_mark_error` 通道（plugin:error）」：`plugin:error` 事件的真实产出点是 `host/services.rs` 的 `PluginServices::mark_plugin_error`（guest 侧 host 函数）；`api_bridge.rs::plugin_mark_error` 是前端命令，只置状态不发事件，故按真实链路改 services.rs。
2. `PluginState::Error(String)` 的原因串**保留**（宿主侧诊断事实、既有生命周期用例断言它）。它仍会随插件清单 DTO 过 IPC 到 renderer，但本票后已无任何渲染路径；要彻底不出产生方进程需改 SDK 的 `PluginState` 形状（ABI 面），留给票 02/04 裁决。

**grep 复核**：`getErrorMessage|getDegradedMessage|state.error` 在 `src/`（除测试与 loader.ts 的日志行）零命中；退役的 5 个 i18n 键只在「已退役」注释里出现。
