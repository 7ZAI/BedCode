/**
 * 壳内公共组件库（UI 原语）
 * -----------------------------------------------------------------------------
 * 这批组件是**旧前端公共组件的壳内副本**：契约（props / emits / 插槽）与旧实现
 * 保持逐字一致，旧页面迁移时只换 import 路径，零调用面改动。
 *
 * 为什么复制而不是直接复用旧目录：
 *   旧界面（`/mobile/**`）与宿主壳（`/mobile/shell`）并存期，两条线各自演进。
 *   壳若直接 import `@/components/**`，旧目录就成了新界面的隐式真源——将来
 *   退役旧页面时会牵动壳，且任何人「顺手改旧组件」都会改变新界面行为。复制
 *   一份、由壳自持，替换期才能各走各的；旧目录退役后这批副本即新真源。
 *   方向与纪律见 `.scratch/2026-10-07-mobile-wasm-platform/ui-mechanism-port.md`
 *   与 AGENTS.md §6「移动端前端重构优先对接宿主壳」。
 *
 * 迁移映射（旧 → 壳内）：
 *   @/components/Button.vue           → ./Button.vue
 *   @/components/Toggle.vue           → ./Toggle.vue
 *   @/components/Modal.vue            → ./Modal.vue
 *   @/components/ConfirmDialog.vue    → ./ConfirmDialog.vue
 *   @/components/BottomSheet.vue      → ./PromptDialog.vue（仅重命名：旧名与实现不符）
 *   @/components/LoadingDialog.vue    → ./LoadingDialog.vue
 *   @/components/CollapseSection.vue  → ./CollapseSection.vue
 *   @/components/QuickActionButton.vue→ ./QuickActionButton.vue
 *   @/components/LetterAvatar.vue     → ./LetterAvatar.vue
 *
 * 三处刻意的差异（其余逐字复制，已在各文件内注明理由）：
 *   · ConfirmDialog 的 loading 转圈白边 → `--mobile-text-on-accent` 派生（token-bound）
 *   · QuickActionButton 的 hover 光晕 → `--shell-action-glow`（随主题/色板变化）
 *   · LetterAvatar 的六组渐变色 → `styles/shell.css` 的 `.shell-avatar-g*`（颜色只落样式表）
 *
 * 待迁（本批未含，属业务组件或需先引机制，见落地说明的「待迁清单」）：
 *   终端域（Terminal*）、会话/设备域（DeviceCard / Session*）、文件域（FileExplorer
 *   及 icons/*）、任务域（Task* / RepeatableToggle）、插件域（PluginIcon）、
 *   启动域（SplashScreen*）、设置脚手架（SettingsSubPage，需壳内导航语义确定后落）
 */

export { default as Button } from './Button.vue'
export { default as Toggle } from './Toggle.vue'
export { default as Modal } from './Modal.vue'
export { default as ConfirmDialog } from './ConfirmDialog.vue'
export { default as PromptDialog } from './PromptDialog.vue'
export { default as LoadingDialog } from './LoadingDialog.vue'
export { default as CollapseSection } from './CollapseSection.vue'
export { default as QuickActionButton } from './QuickActionButton.vue'
export { default as LetterAvatar } from './LetterAvatar.vue'
