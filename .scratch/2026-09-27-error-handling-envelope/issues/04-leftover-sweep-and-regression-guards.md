# 04：残留清扫 + 防泄漏回归防线收口

**Type:** task
**Spec:** `../spec.md`（§4 P4 / §3 D 组与遗留项）；契约单一事实源 `docs/adr/0030-error-envelope-and-user-prompt-boundary.md`（决定 12 / Consequences 回归防线）
**Blocked by:** 02, 03
**Status:** done（2026-09-27）

**What to build:** 兜住前三个切片未覆盖的剩余泄漏面（更新检查失败目前把原始错误渲染在设置页；以及任何遗落的 `{error}` 插值键、裸错误直显），并把「技术详情永不上界面」从依赖纪律升级为**机器防线**：新代码即使写错，构建/测试也会翻红提醒。具体三条防线：① 信封序列化契约测试（形状稳定、永不携带 detail 字段）；② i18n 键无 `{error}` 插值残留的扫描测试；③ 集中 toast 消费层永不渲染 code/detail 的断言。用户侧最终形态：所有失败一律友好提示，技术细节只存在于日志。

**集成测试约束（用户指令）**：本票**只编写**集成测试文件，**不执行**；统一在票 05 全量执行。

**Acceptance:**

- [x] 更新检查失败通用化：设置页不再渲染原始错误消息，显示通用文案 + 日志（error/warn 级 release 已落盘）
- [x] 遗留清扫：全仓 grep 复核——i18n 无 `{error}` 插值残留；视图/组件无裸错误消息直显；composable 不再抛「已插值 i18n」的错误对象
- [x] 前端消费层字符串容错收口：对遗留字符串形状的兼容按契约收紧（明确保留或移除，留档理由）
- [x] 防泄漏回归防线落地：① 信封序列化契约测试（形状 / 无 detail 字段 / request_id 存在）；② i18n `{error}` 扫描测试；③ toast 消费层永不渲染 code/detail 断言——三条均为单元或静态测试，随全量套件运行
- [x] 单元测试全绿（针对性过滤，不跑集成）；**编写**（不执行）覆盖「全链路失败 → 界面零技术详情」的集成测试
- [x] 交付说明：`src/` 范围 grep 结果为「零技术原文直显」（下附全仓 grep 证据）

## 交付说明（2026-09-27，票 04）

### A. 更新检查失败通用化（D 组）
- `composables/useUpdateChecker.ts`：删除对外暴露的裸错误字段（曾 `errorMessage = e.message`）；
  失败只置 `status='failed'`，**详情经 `logger.error` 落盘**（release 下 error/warn 仍转发，见 frontendLogger 票 01）。
- `components/settings/SettingsAboutSection.vue`：失败段改渲染 `getUpdateStatusText()` →
  `settings.about.checkFailed` 通用文案（zh「检查更新失败，请稍后重试」/ en「Failed to check for updates,
  please try again」）；不再消费 `errorMessage`。
- 验证：`useUpdateChecker.test.ts` 8 例（失败→failed 且组件对象**无 errorMessage 键**、详情只进日志、
  状态恢复、getUpdateStatusText 全状态映射）；`SettingsView.test.ts` 绿。

### B. 遗留清扫（全仓 grep 复核）
以下 grep 在**宿主 `src/` + wasm-apps 四应用**全仓执行，结果均为零命中：
1. `{error}` 插值残留：`src/locales/**` + `wasm-apps/*/src/i18n/**` 值中零命中（曾存在于
   terminal-session 8 键、agent-hub 4 键，本票改静态文案，zh/en 同步）。
2. 裸错误直显：`{{ e.message }}` / `{{ state.error }}` / `{{ speed?.error }}` / `{{ r.error }}` 等
   模板渲染路径零命中（唯一保留：`Input.vue` / `TextInput.vue` 的 `error` prop —— 表单**校验位**，
   无任何调用方绑定动态错误串，属产品预留呈现位）。
3. composable 抛「已插值 i18n」错误：`new Error(\`...\${t(...)}\`)` 零命中（useServer 票 01 已改
   parseInvokeError；context.ts 本票收口）。

### C. 前端消费层字符串容错收口
- `plugin/context.ts` `commands.execute` catch：移除「遗留字符串形状 raised 进 `.message`」的拼装
  （原文曾 `Command not found: ${id} (${raw})`）——现在只抛机制错误 `Command not found: ${id}`，
  **原始失败经 `logger.error` 落盘**。留档理由：旧字符串 rejection 已被信封化（ADO：AppError
  Serialize 输出对象），`typeof e === 'string'` 分支实为死代码；拼装原文存在「未来被展示时泄漏」风险。
- `userError.ts` 模块级 `t` 绑定改懒取（修复 i18n.test.ts 贫 mock `{global:{locale}}` 无 `t` 时
  模块加载即炸的隔离问题）。

### D. 防泄漏回归防线（三条机器防线齐备）
1. **信封序列化契约测试**（Rust，随 `cargo test` 运行，`system/error.rs`）：
   `envelope_never_carries_technical_detail`（字段白名单 code/request_id/params + 无 detail 断言）、
   `user_facing_preserves_code_and_safe_params`、`event_envelope_matches_ipc_envelope_shape`、
   `request_id_is_renewed_per_serialization` 等（票 01/03 落地，本次复核确认在册）。
2. **i18n `{error}` 扫描测试**（新增，host vitest）：`src/__tests__/locales/errorInterpolationGuard.test.ts`，
   递归扫描**全部 14 个 locale 文件**（宿主 6 + wasm-apps 四应用 8），断言：值不含 `{error}`、
   占位符成对且为合法具名参数（{name}/{plugin}/{count}/{pluginId}/{viewId} 等，白名单拒绝
   `{error}`/空/嵌套/数字索引）——28 tests。新增键若带 `{error}` 或拼写错误占位符即翻红。
3. **toast 消费层永不渲染 code/detail 断言**（`utils/userError.test.ts`）：`showUserError` toast
   文案不包含 code / request_id / 技术详情；`parseInvokeError` 形状矩阵（object→映射、字符串/
   Error/null/畸形→host.internal）；插件码裸 key 回退逻辑 —— 在册，本次复核不变。

### E. 单元测试（针对性过滤，均实际运行）
- 宿主：`useUpdateChecker` 8 + `errorInterpolationGuard` 28 + `userError` 30 + `invoke` + `frontendLogger`
  + `useServer` + `runtimeErrorEnvelope` 19 + `PluginApprovalDialog` + `SettingsView` + `plugin-flow` 6
  （常规套件内文件必须当次修好，故跑）。
- 全量宿主回归：`npx vitest run src/__tests__/` → **79 files / 792 passed / 17 skipped**（4 skipped =
  票 01-03 的 describe.skip 集成文件）。
- wasm-apps 插件套件：terminal-session 21 files + agent-hub 3 files → **24 files / 256 passed**。
- **只写不跑**（票 05 启用）：`src/__tests__/integration/update-checker-error-envelope.test.ts`
  （describe.skip，4 例：check() 抛错→设置页通用文案+DOM 零原文、失败后重试恢复、下载失败详情只进日志、
  GitHub 打开失败仅日志）。已验证文件可收集（4 skipped，不执行断言）。
- Rust：本票无 Rust 改动（信封测试在册于票 01/03，命令面 counts 见票 02）。`cargo test` 全量回归
  **留给票 05**（AGENTS §10 收尾统一执行；本票按 §3 只跑针对性单测，未动 Rust 代码）。

### F. wasm-apps 四应用清扫明细（spec §1 范围 + 票 04「全仓」acceptance）
| 应用 | 改动 |
|---|---|
| terminal-session | `SessionCenterView` 6 处 toast 去 `{error: e?.message}` + 补 console.error；`TerminalWindowView`
  stopFailed 去原文；`SessionConfigForm` wslError 原文改 i18n 布尔门控；`TaskHistoryView` 定时任务失败
  段只出 `task.scheduledError` 通用文案（详情经 guest 已有 host.log_error 落盘）；zh/en 8 键去 `{error}` |
| agent-hub | `SkillsTab` scanError/import.failed 去 `{error}`、entry.error 渲染移除；`ProviderApply` apply.failed
  原文只进日志 + 界面 i18n；`InstallTab` customError / lastRun.error / speed.error 原文收口（console.error +
  i18n）；`OverviewTab` speed.error 同；`CliCard` info.error 同；`SkillEditor` 移除对不存在字段
  `detail.error` 的死渲染（顺带消 TS2339）；`useUsage` add/removeSource 不再透传原文（console.error 后
  仅返回 ok）；zh/en 4 键去 `{error}` |
| ai-chatbox | `ProviderForm` fetchError/testResult 原文收口；`ChatView` messageErrorText/visibleError 由
  「desktop.plugin. 前缀才翻译」的遗留守卫改为**全量解析**（插件码经宿主 t() 解析，未注册原文→
  `requestFailed` 兜底文案），终结 lastError 码/原文上屏；新增 i18n 键 `testFailed` / `requestFailed`
  （zh/en 同步，messages.ts schema 同步） |
| file-transfer | 审计确认：useSettings/usePeerDevices 仅把原文用于关键词分类 + console.error，
  无渲染路径（UI 走枚举态 i18n）——无需改动，留档 |

### G. 遗留判定（明确保留，留档理由）
- `PluginState::Error(String)` 原因串：保留为宿主侧诊断事实（票 03 裁决），本票确认零渲染路径；
  彻底移除需改 SDK PluginState 形状（ABI 面），留待插件错误状态重构时评估——**不在本票放宽 UI 呈现**。
- `loader.ts` `pluginMarkError(pluginId, e.message)`：宿主诊断通道（即插件错误状态真源），非 UI 渲染。
- `Input/TextInput` 的 `error` prop：表单校验位，无动态绑定，保留。

### 边界说明（与 spec 的偏差，记录）
- 本票实际清扫面超出 `src/`：spec §1 范围明列 wasm-apps 四应用、票 04 acceptance 用「全仓」字眼，
  故 C/F/G 组含插件工程清扫（宿主侧三防线 + 全仓 grep 交付）。
- `cargo test` 全量、`pnpm run test:run` 全量、eslint 全仓 0 error 收口统一在票 05 执行
  （本票已验证宿主 vitest 全量 792 绿 + wasm-apps 256 绿 + eslint 改动文件 0 error）。