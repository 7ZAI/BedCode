// Common UI Components
// 统一导出所有公共组件

export { default as Button } from './Button.vue'
export { default as Input } from './Input.vue'
// Select 单源化：宿主与插件共用 SDK 组件，宿主不再维护副本
export { default as Select } from '@binblink/plugin-sdk-desktop/ui'
export { default as Toggle } from './Toggle.vue'
export { default as Modal } from './Modal.vue'

// 新增组件
export { default as Spinner } from './Spinner.vue'
export { default as SplashLoading } from './SplashLoading.vue'
export { default as LoadingOverlay } from './LoadingOverlay.vue'
export { default as Tooltip } from './Tooltip.vue'
export { default as NotificationBadge } from './NotificationBadge.vue'
export { default as TerminalInputRail } from '@binblink/plugin-sdk-desktop/ui/terminal-input-rail'
