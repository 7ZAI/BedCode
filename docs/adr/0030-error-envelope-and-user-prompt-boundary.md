# 错误信封 + 用户提示边界（Error Envelope & User Prompt Boundary）

## 背景

桌面端当前「错误 → 用户可见面」的路径是裸透传：`system/error.rs` 中 `AppError` 的
`Serialize` 实现为 `serialize_str(&self.to_string())` —— 完整技术文案（含 anyhow 错误链）
直接作为 Tauri invoke 的 rejection 进前端；前端多处再把错误原文插值或直显进用户可见面
（toast / 页面 / tooltip），最严重的三类：

- 插件 `state.error` 被 `mark_error` 写入完整错误串（如 `"auto reload after trap failed: …"`），
  `PluginsView.vue` 列表项与 tooltip、`PluginDetailView.vue` 降级原因段落直接渲染；
- 插件运行时异常事件（`PLUGIN_RUNTIME_ERROR`）把 panic 消息/回溯放进 toast
  （`runtime-listeners.ts` 注释自述「toast 截断展示」）；
- 12 组 i18n key 带 `{error}` 插值，`ServerView.vue` 等 9 处 `toast.error(e.message)` 直显。

用户指令（2026-09-27）：**具体的错误信息与错误调用堆栈不得出现在前端任何界面上；界面只
提供用户友好的提示**。

范围界定（与用户确认）：覆盖桌面端全部程序代码（宿主 `src-tauri/` + `src/` 与 wasm-apps
四应用）；跨端 HTTP 错误响应复用同一信封形状、作为独立任务两端同步部署（AGENTS §9）；
前端内部错误走约定（统一 logger + 友好提示），不建错误码体系之外的机制。
修改面不含移动端（`system/error.rs` 是桌面端独立副本；WIT/ABI 零变更）。

## 决定

1. **边界信封形状**：跨进程失败一律承载为 `{ code, request_id, params? }`。`code` 为机器
   可读稳定标识；`request_id` 为单次失败事件在产生方日志中的关联键（边界转换器生成短随机
   hex，随详情同条 tracing 日志带出）；`params` 为**已消毒具名参数**（显示名/端口/秒数/文件名），
  禁止携带技术文案、调用堆栈、凭据。用户文案唯一真源 = 前端 i18n，前端按 code 映射模板插值 params。
2. **硬不变量：技术详情不出产生方进程**。信封永不携带错误原文/堆栈字符串——前端「没有」
   就「不会泄露」，比「展示了再规避」更强。技术详情只存在于产生方进程内的日志
   （Rust tracing 结构化字段 / 前端 logger）。
3. **分类规则：默认兜底 + 显式覆盖**。所有变体默认映射 `host.internal`（detail 仅日志）；
   **凡需 UI 特定文案或参数**，调用点显式构造 `AppError::UserFacing { code, params, detail }`
   （detail 仅日志）。不做「变体 → 码」全量映射表（避免表格漂移），调用点显式为唯一入口。
4. **双命名空间错误码**：宿主域 `host.*`、前端域 `frontend.*`、插件域 `<plugin_id>.*`
   （插件自定、宿主不预埋文案、不解释语义——同一「容器透传、语义归插件」哲学，见
   `utils/session_gateway.rs`）。**code 即 i18n key**（`host.invoke.timeout` ↔
   `errors.host.invoke.timeout`）：零映射层、零漂移，插件文案在插件自己的 `src/locales/`。
   未映射 / 畸形形状一律兜底 `host.internal` / `frontend.internal`，文案：
   「操作未完成，请稍后重试」。
5. **序列化单点改造**：`impl Serialize for AppError` 改为输出信封 JSON 对象（`Display`
   不变，日志/错误链照旧全量）；`request_id` 在序列化点生成并落同条 tracing 日志。
   全部 Tauri 命令、`api_bridge`、事件通道自动覆盖，无逐命令改造。
6. **插件错误标记信封（零 ABI 变更）**：WASM 插件命令面错误签名不变（仍是字符串），SDK 提供
   `bail_with_code(code, params)` / `PluginError::business(...)`，序列化为标记 JSON
   （`{"__bedcode_error__":true, "code":…, "params":…}`）；宿主桥（`invoke_wasm_command`）
   只做**机制判断**——检测标记：是 → 校验形状后透传给前端（宿主不解释业务语义）；
   否/畸形 → 按 `host.internal` + 插件 id 参数处理，原文进日志。
7. **事件通道同信封**：`PLUGIN_RUNTIME_ERROR`（trap / recovery_failed）与 `plugin:error`
   （自检失败）载荷改为 `{ code, request_id, params: { name/plugin } }`，detail 只留日志；
   前端监听器 toast 友好文案。`plugin:notify` 维持现状（业务通知），checklist 加约定：
   通知与错误推送不得携带技术详情。
8. **前端消费层收敛**：新增 `UserError { code, request_id, params? }` 与三个收敛函数：
   `parseInvokeError(e)`（invoke rejection 对象 → UserError；未知形状 → `host.internal`）、
   `showUserError(err, fallbackCode?)`（集中 toast 友好文案 + `logger.error(code, request_id, err)`）、
   `userErrorFromUnknown(e)`（renderer 侧未知异常 → `frontend.internal`）。composable 不再
   `throw new Error(i18n(...))`，view 不再 `e.message`。`invokeWithTimeout` 超时 → reject
   `host.invoke.timeout` 的 UserError（去掉 `{cmd}` 可见文本）。
9. **UI 呈现规则**：toast / 页面只显示友好文案（固定模板 + params 插值），**不显示任何错误码**
   ——用户侧不需要「看懂」内部标识；如产品需「反馈可对号」，可选展示 request_id 尾号
   （6 hex，支持人员可精确 grep 到那一次失败），v1 默认不展示。**v1 不自动重试**（宿主命令面多
   为副作用操作，自动重跑幂等代价高）；仅 `host.invoke.timeout` 在 toast 提供「重试」按钮。
10. **前端日志兜底（现状 gap 修复）**：`frontendLogger` release 构建由空函数改为
    **error/warn 级仍转发 Rust `report_frontend_log` 落盘**（info/debug 裁剪，复用攒批机制），
    保证「详情只进日志」在前端侧同样成立。
11. **退役 UI 直显**：插件列表/详情的 `state.error` / 降级原因原文（`getErrorMessage` /
    `getDegradedMessage` 渲染路径）从 UI 移除，改徽标 + 通用文案
    （`desktop.plugin.error` / `degraded`，`host.plugin.degraded` 带 `{name}` 参数）；
    `SettingsAboutSection` 更新检查失败显示通用文案 + 日志。
12. **错误码注册表 v0**（宿主域 / 前端域；插件域由各插件自定）：

    | code | 触发 | zh | en | 重试 |
    |---|---|---|---|---|
    | `host.internal` | 未映射/引擎类/畸形信封兜底 | 操作未完成，请稍后重试 | Operation failed, please try again | — |
    | `host.invoke.timeout` | IPC 超时（30 s） | 操作超时，请重试 | Operation timed out, please retry | 有 |
    | `host.plugin.not-activated` | 未激活调用 | 该应用未启用，无法执行此操作 | This app is not enabled, this action is unavailable | — |
    | `host.plugin.not-found` | 插件不存在 / TS-only | 应用不存在或已卸载 | App not found or removed | — |
    | `host.plugin.trap` | WASM trap（自动恢复中） | 应用「{name}」运行异常，已尝试自动恢复 | App "{name}" misbehaved, auto-recovery attempted | — |
    | `host.plugin.recovery-failed` | 自动恢复失败进 Error 态 | 应用「{name}」运行异常且未能自动恢复，请到应用中心处理 | App "{name}" failed and could not recover; check the app center | — |
    | `host.plugin.self-check-failed` | 启动自检失败 | 应用「{plugin}」启动自检失败，请检查配置 | App "{plugin}" self-check failed, check its configuration | — |
    | `host.plugin.degraded` | 降级运行 | 应用「{name}」降级运行，部分功能不可用 | App "{name}" is running degraded, some features unavailable | — |
    | `host.not-found` / `host.invalid-input` / `host.config` / `host.auth` | 按场景显式映射（默认仍 internal） | 内容不存在 / 输入无效，请检查后重试 / 配置保存失败，请稍后重试 / 认证未通过，请重新连接 | …（对应英文） | — |
    | `frontend.internal` | renderer 侧未知异常 | 操作未完成，请稍后重试 | Operation failed, please try again | — |

    **code 稳定性规则**：code 是公开契约，改名 = 破坏性变更，由回归测试钉死（见 Consequences）。

## Considered Options

- **语义码 vs 数字码**（用户 2026-09-27 裁决）：语义码是 i18n key 本体、双命名空间天然隔离
  第三方/插件/双端（ADR 0018/0019 契约分叉下数字码必撞号）、日志/测试/支持三方可读、
  「稳定性」靠纪律（改码=破坏性变更）不靠格式。数字码额外需要「码→key」第二张映射表
  （双真源漂移）与中央发号机构。展示层则反过来：**用户面不显示任何码**（语义码上 UI 是
  技术外观且无实义）。
- **信封 vs 维持字符串（+前缀分类）**：字符串前缀方案「看起来最小」，但文案仍可能夹带
  细节、无参数通道、无法区分语义与内部——否决。
- **插件标记信封 vs ABI bump**：WIT/invoke_command 导出签名不变（标记 JSON 字符串 + 宿主
  机制性检测透传），零 ABI 破坏、零插件重建成本；结构化错误类型留给未来真有需要时演进。
- **code 即 i18n key vs 独立映射表**：前者少一层真源、词汇可被双语文案 lint 覆盖；后者
  多一层维护且必漂移——否决。

## Consequences

- **破坏面**：`AppError` 序列化行为变更 → 所有 Tauri 命令 rejection 形状从字符串变信封对象、
  相关测试更新；12 组 i18n key 移除 `{error}` 插值；约 30 处调用点改造（A–G 全量清单见
  `.scratch/2026-09-27-error-handling-envelope/spec.md`，实施任务拆 P1–P4）。
- **正面**：用户可见面从此只会出现友好文案；技术详情（含堆栈、anyhow 链）只在产生方进程
  日志；支持排障凭 request_id + 时间精确定位；前端 release 侧错误也落盘（此前为盲区）。
- **无 ABI 变更**：WIT 不动、插件零重建；移动端零影响（独立 error.rs 副本）；HTTP 跨端
  信封任务单独立项、两端同步部署。
- **回归防线**：① 信封序列化单元测试（形状稳定、不含 detail 字段断言、request_id 存在）；
  ② i18n 键无 `{error}` 残留的测试（或 lint）；③ 前端消费层 `parseInvokeError` 形状矩阵、
  `showUserError` 日志断言、release 转发测试；④ `plugin-loader-gating` 等既有断言随清扫更新。

## 修订记录

- 2026-09-27 初稿（grill-with-docs 会话定稿：边界信封 + 语义码 + 用户面纯文案 + 四阶段实施
  拆分 P1 Rust 边界 / P2 SDK 插件侧 / P3 前端消费层 / P4 全量清扫）。