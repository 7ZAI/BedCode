/**
 * 终端宿主能力注入契约（宿主侧，票 03a；票 05 收口）
 *
 * 与插件侧真源 `plugins/terminal-session/src/components/terminal/
 * terminalHostCapabilities.ts` 结构镜像（宿主不 import 插件包，双份定义的
 * 脆弱点以插件侧为真源）。TerminalWindowHostView 按此契约 provide，
 * 插件终端视图（TerminalPreview / TerminalWindowView）inject。
 *
 * 仅类型与注入键（值在 TerminalWindowHostView.vue 内实现）；输出面已不属
 * 契约（票 05）：输出改经插件 WASM 命令面 `session.output.pull` 轮询拉取
 * `host-session.output-ring-fetch` 原语，宿主 Channel 桥随前端 composable 摘除。
 */

/** 注入键（与插件侧 TERMINAL_HOST_CAPABILITIES_KEY 同字符串） */
export const TERMINAL_HOST_CAPABILITIES_KEY = 'terminalHostCapabilities'

/** 终端设置访问器（结构镜像插件侧 TerminalSettingsAccessor） */
export interface TerminalSettingsAccessor {
  getFontSize(): number
  getTheme(): string
  getBgImage(): string
  getBgOpacity(): number
  getServerPort(): number
  save(patch: {
    fontSize?: number
    theme?: string
    bgImage?: string
    bgOpacity?: number
  }): void
  onChange(listener: () => void): () => void
}
