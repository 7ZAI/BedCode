/**
 * 终端功能栏域（TerminalView 拆分产物）
 *
 * 一块职责：**标题栏 / 侧边栏 / 弹窗这些「功能栏」的开关状态与动作分发**：
 * 设置弹窗、清屏确认、侧边栏（文件树）、任务选择器、快捷键配置、帮助、新手引导，
 * 以及标题栏工具栏动作的分发表。
 *
 * 为什么工具栏分发也在这里：它本质是一张「动作键 → 状态迁移」映射表，与这些开关
 * 状态同源；留在组件里会让编排层同时持有状态定义与分发逻辑两类职责。
 * 刷新动作由外部注入（显示域，需 resize/subscription/bufferStore 协作），本域只分发。
 */
import { ref } from 'vue'
import { useInputAssistantStore } from '@/stores/inputAssistant'

export interface TerminalPanelsDeps {
  /** 手动刷新终端（显示域：渲染层恢复 + 尺寸仲裁重跑 + 数据层续传重拼接） */
  refreshTerminal: () => void | Promise<void>
}

export function useTerminalPanels(deps: TerminalPanelsDeps) {
  const assistStore = useInputAssistantStore()

  const showSettings = ref(false)
  const showClearConfirm = ref(false)
  const showSidebar = ref(false)
  const showTaskPicker = ref(false)
  const showShortcutConfig = ref(false)
  /** 标题栏 ? 按钮：终端输入组件便捷功能教程弹窗 */
  const showHelp = ref(false)
  /**
   * 新手引导：首次进入终端页自动展示（terminalOnboardingPending），展示后清除；
   * 设置里可重新开启（下次进入再显示）
   */
  const showOnboarding = ref(false)

  /** 标题栏工具栏动作分发 */
  function handleToolbarAction(key: string) {
    switch (key) {
      case 'task':
        showTaskPicker.value = true
        break
      case 'shortcut':
        showShortcutConfig.value = true
        break
      case 'clear':
        showClearConfirm.value = true
        break
      case 'refresh':
        void deps.refreshTerminal()
        break
      case 'settings':
        showSettings.value = true
        break
      case 'folder':
        showSidebar.value = !showSidebar.value
        break
      case 'help':
        showHelp.value = true
        break
    }
  }

  /** 侧栏「插入引用」后收起侧栏，露出输入区（路径填充由输入域承担） */
  function closeSidebar() {
    showSidebar.value = false
  }

  /** 清除「待展示新手引导」标记（本设备已展示过一次） */
  function clearOnboardingPending() {
    if (assistStore.settings.terminalOnboardingPending) {
      assistStore.saveSettings({ terminalOnboardingPending: false })
    }
  }

  /** 新手引导关闭 */
  function handleOnboardingClose() {
    showOnboarding.value = false
    clearOnboardingPending()
  }

  /** 新手引导跳转完整教程：清除标记并打开帮助弹窗 */
  function handleOnboardingOpenHelp() {
    showOnboarding.value = false
    clearOnboardingPending()
    showHelp.value = true
  }

  return {
    showSettings,
    showClearConfirm,
    showSidebar,
    showTaskPicker,
    showShortcutConfig,
    showHelp,
    showOnboarding,
    handleToolbarAction,
    closeSidebar,
    handleOnboardingClose,
    handleOnboardingOpenHelp,
  }
}
