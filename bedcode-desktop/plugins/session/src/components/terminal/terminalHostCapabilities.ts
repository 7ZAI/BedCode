/**
 * 终端视图的宿主能力注入契约（票 03a）
 *
 * 插件终端视图（TerminalPreview / TerminalWindowView）经 inject 取用宿主能力。
 * 宿主侧实现：`src/views/TerminalWindowHostView.vue`（provide 同结构对象；
 * 契约类型宿主侧不 import 插件，就地定义同构结构——双份定义的脆弱点，
 * 以本文件的导出为真源）。
 *
 * 设计边界（方案 1 + ADR 0022 裁剪线）：终端渲染/写入/IME 已整体下沉插件，
 * 但三块宿主存储/框架面不随之下沉，经注入桥接（与 context.session.openTerminal
 * 原语同构——宿主留引擎原语，插件持编排）：
 * - 终端设置持久化（宿主 settingsStore，terminal_* 字段）——TerminalSettingsAccessor
 * - 背景图文件命令（宿主 set_terminal_bg_image 复制文件 + 设置持久化）
 * - 输出流（票 04 起插件已撤桥：输出改经插件 WASM 命令面轮询拉取
 *   `host-session.output-ring-fetch` 原语，WIT list<u8> 二进制直传；本字段为票 05
 *   宿主摘除前的兼容残留，插件不再调用 attachSink）
 * - 插件扩展点（宿主 registry 响应式数组；壳复刻渲染宿主 Plugin*Toolbar 组件
 *   的等效按钮）
 *
 * dev-shell / vitest 无宿主注入时由 createFallbackHostCapabilities() 提供内存版
 * （settings 存内存、output no-op、扩展点空数组），保证组件可独立渲染测试。
 */
import { inject, ref, type Ref } from 'vue'
import type { TerminalSettingsAccessor } from '../../composables/terminal/useTerminalSettingsSync'
import type { TerminalOutputSink } from '../../composables/terminal/useTerminalWritePipeline'

/** 插件扩展点项（宿主 registry Registered*ToolbarItem 的结构镜像，真源在宿主） */
export interface TerminalExtensionItem {
  pluginId: string
  id: string
  label: string
  icon?: string
  onClick?: () => void
}

/** 终端窗口模式的页面工具栏目标（对齐宿主 PluginPageToolbar target 语义） */
export const TERMINAL_PAGE_TOOLBAR_TARGET = 'terminal'

/** 宿主注入的终端宿主能力 */
export interface TerminalHostCapabilities {
  /** 终端设置访问器（宿主 settingsStore 桥） */
  settings: TerminalSettingsAccessor
  /** 背景图片命令桥（宿主 set_terminal_bg_image 命令 + 设置持久化） */
  bgImage: {
    /** 选择系统图片并设为终端背景；用户取消返回 false，成功返回 true */
    pickAndSet(): Promise<boolean>
    /** 移除终端背景图片 */
    remove(): Promise<void>
    /** 当前背景图片名（回显用，最后路径分隔符后内容） */
    imageName: string
    /** 是否已启用背景图片 */
    hasImage: boolean
  }
  /**
   * 输出流桥（票 04 起插件不再调用——输出改经插件 WASM 命令面轮询拉取
   * `host-session.output-ring-fetch` 原语；字段保留至票 05 宿主摘除）
   */
  output: {
    /** 接入输出源：订阅宿主 Channel，三回调映射到 sink；返回断开函数 */
    attachSink(sink: TerminalOutputSink): () => void
  }
  /** 插件扩展点（宿主 registry 响应式数组，壳复刻渲染） */
  extensions: {
    terminalToolbarItems: Ref<TerminalExtensionItem[]>
    titleBarItems: Ref<TerminalExtensionItem[]>
    pageToolbarItems: Ref<TerminalExtensionItem[]>
  }
}

/** provide / inject 键 */
export const TERMINAL_HOST_CAPABILITIES_KEY = 'terminalHostCapabilities'

/** 读取宿主注入（组件 setup 内调用）；无注入返回 null（dev-shell / vitest） */
export function useTerminalHostCapabilities(): TerminalHostCapabilities | null {
  return inject<TerminalHostCapabilities | null>(TERMINAL_HOST_CAPABILITIES_KEY, null)
}

/** dev-shell / vitest 回退：内存版 settings + no-op 输出桥 + 空扩展点 */
export function createFallbackHostCapabilities(): TerminalHostCapabilities {
  let fontSize = 12
  let theme = 'dracula'
  let bgImage = ''
  let bgOpacity = 30
  const accessor: TerminalSettingsAccessor = {
    getFontSize: () => fontSize,
    getTheme: () => theme,
    getBgImage: () => bgImage,
    getBgOpacity: () => bgOpacity,
    getServerPort: () => 8080,
    save: (patch) => {
      if (patch.fontSize != null) fontSize = patch.fontSize
      if (patch.theme != null) theme = patch.theme
      if (patch.bgImage != null) bgImage = patch.bgImage
      if (patch.bgOpacity != null) bgOpacity = patch.bgOpacity
    },
    onChange: () => () => {},
  }
  return {
    settings: accessor,
    bgImage: {
      pickAndSet: async () => false,
      remove: async () => {},
      imageName: '',
      hasImage: false,
    },
    output: {
      attachSink: () => () => {},
    },
    extensions: {
      terminalToolbarItems: ref([]),
      titleBarItems: ref([]),
      pageToolbarItems: ref([]),
    },
  }
}
