<template>
  <div
    ref="terminalViewRef"
    class="terminal-view"
    :style="terminalViewStyle"
  >
    <!-- Loading Overlay -->
    <transition name="loading-fade">
      <div v-if="!isTerminalReady" class="loading-overlay">
        <div class="loading-spinner"></div>
        <p class="loading-text">{{ t('mobile.terminal.preparing') }}</p>
      </div>
    </transition>

    <!-- Header - 固定位置，不随键盘移动 -->
    <TerminalHeader
      :session-name="sessionName"
      :is-selection-mode="isSelectionMode"
      :visible-items="visibleToolbarItems"
      :all-items="ALL_TOOLBAR_ITEMS"
      :show-sidebar="showSidebar"
      @back="handleBack"
      @action="handleToolbarAction"
    />

    <!-- 裁剪容器：限制上移区域不突破 Header 底部 -->
    <div class="movable-clip">
      <!-- 可移动区域：终端内容 + 输入栏，随根容器高度收缩（键盘避让由
           terminal-view bottom 收缩承担，见 terminalViewStyle） -->
      <div class="movable-area">
        <!-- Main Content: Terminal + Sidebar overlay -->
        <div class="main-content">
          <div class="terminal-output-area">
            <div
              ref="scrollContainer"
              class="terminal-scroll-container"
              :class="{ 'selection-mode': isSelectionMode }"
            >
              <div
                ref="xtermContainer"
                class="xterm-container"
                :style="xtermContainerStyle"
              ></div>
              <!-- TUI 模式下隐藏滚动条：alt buffer 无 scrollback，全满 thumb 是误导 -->
              <div v-if="!isTuiMode" class="scrollbar-track">
                <div
                  class="scrollbar-thumb"
                  :class="{ visible: scrollbarVisible }"
                  :style="scrollbarThumbStyle"
                ></div>
              </div>
              <transition name="scroll-indicator">
                <button
                  v-if="isUserScrolling && !isSelectionMode"
                  class="scroll-to-bottom-btn"
                  @click="scrollToBottomManual"
                  :title="t('mobile.terminal.scrollToBottom')"
                >
                  <svg class="scroll-to-bottom-icon" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 14l-7 7m0 0l-7-7m7 7V3" />
                  </svg>
                </button>
              </transition>
              <transition name="selection-bar">
                <div v-if="isSelectionMode && hasSelection && selectionTouchEnded" class="selection-action-bar" :style="selectionBarStyle">
                  <button class="selection-action-btn" @click="copySelection">
                    {{ t('common.button.copy') }}
                  </button>
                  <button class="selection-action-btn" @click="selectAllText">
                    {{ t('mobile.terminal.selectAll') }}
                  </button>
                  <button class="selection-action-btn cancel" @click="exitSelectionMode">
                    {{ t('common.button.cancel') }}
                  </button>
                </div>
              </transition>
            </div>
          </div>

          <FileSidebar
            class="sidebar-overlay"
            :class="{ 'sidebar-hidden': !showSidebar }"
            :session-id="sessionId"
            ref-insert
            @settings-input-focus="handleSettingsInputFocus"
            @insert-ref="handleInsertRef"
          />
          <div v-if="showSidebar" class="sidebar-backdrop" @click="showSidebar = false"></div>
        </div>

        <!-- Input Bar -->
        <TerminalInputBar
          ref="inputBarRef"
          :disabled="!isSessionActive"
          :is-connected="isConnected"
          :placeholder="inputPlaceholder"
          :is-landscape="isLandscape"
          :pending-ref="pendingRefPath"
          @submit="handleInputSubmit"
          @execute="handleInputExecute"
          @special-key="handleSpecialKey"
          @shortcuts-panel-toggle="handleShortcutsPanelToggle"
          @ref-consumed="pendingRefPath = null"
        />
      </div>
    </div>

    <!-- Settings Modal -->
    <TerminalSettingsModal
      :visible="showSettings"
      :font-size="terminalSettings.fontSize"
      :theme="terminalSettings.theme"
      :is-theme-user-set="terminalSettings.isThemeUserSet"
      :quick-bar-count="assistStore.settings.quickBarCount"
      :toolbar-items="assistStore.settings.headerToolbarItems || ['folder']"
      :all-toolbar-items="ALL_TOOLBAR_ITEMS"
      :onboarding-pending="assistStore.settings.terminalOnboardingPending"
      :safe-area-style="settingsModalStyle"
      @confirm="handleSettingsConfirm"
      @cancel="showSettings = false"
    />

    <!-- Clear Confirm Modal -->
    <TerminalConfirmModal
      :visible="showClearConfirm"
      :message="t('mobile.terminal.clearScreen') + '?'"
      :safe-area-style="confirmModalStyle"
      @confirm="clearTerminal"
      @cancel="showClearConfirm = false"
    />

    <!-- 正统渲染端覆盖确认：服务端裁决本端非正统（另一端正渲染输出）时弹出，
         确认后 force 重发覆盖，取消则抑制同尺寸后续请求 -->
    <ConfirmDialog
      v-model="showRendererOverrideDialog"
      :title="$t('mobile.terminal.rendererOverrideTitle')"
      :message="
        rendererOverrideTarget
          ? $t('mobile.terminal.rendererOverrideBody', {
              renderer: rendererOverrideTarget.rendererName,
            })
          : ''
      "
      :confirm-text="$t('mobile.terminal.rendererOverrideConfirm')"
      :cancel-text="$t('mobile.terminal.rendererOverrideCancel')"
      variant="warning"
      :close-on-backdrop="false"
      @confirm="confirmRendererOverride"
      @cancel="cancelRendererOverride"
    />
  </div>

  <!-- Task Picker -->
  <TaskPickerModal
    :visible="showTaskPicker"
    :tasks="presetTasks"
    :session-id="sessionId"
    @send="onTaskSend"
    @execute="onTaskExecute"
    @close="showTaskPicker = false"
  />

  <!-- Shortcut Config -->
  <ShortcutConfigModal :visible="showShortcutConfig" @close="showShortcutConfig = false" />
  <!-- 便捷功能教程弹窗（标题栏 ? 入口） -->
  <TerminalHelpModal :visible="showHelp" @close="showHelp = false" />
  <!-- 新手引导（聚光灯分步导览：首次进入自动展示；「查看完整教程」接帮助弹窗） -->
  <TerminalOnboardingTour
    :visible="showOnboarding"
    @close="handleOnboardingClose"
    @open-help="handleOnboardingOpenHelp"
  />
</template>

<script setup lang="ts">
/**
 * 终端视图（移动端）— 编排层：xterm 实例生命周期 + 移动端输入/工具栏/弹窗接线
 *
 * 复杂逻辑按域拆分到 `src/composables/terminal/`（范式参考桌面端
 * `bedcode-desktop/src/composables/terminal/`），共享状态经 terminalKernel 交换
 * （各域创建顺序无关，回调调用时解析）：
 * - useTerminalRenderer：网格测量/构造期预估、DPR 感知 fit（列 ±1 漂移钳制）、
 *   WebGL 可选加载与 context-loss 恢复、字符图集预热（仅 WebGL）、DPR 变化监听
 * - useTerminalResize：PTY 尺寸串行队列 + 服务端正统渲染端裁决（覆盖确认弹窗）
 * - useTerminalKeyboardAvoidance：双通道键盘检测 + 根容器高度收缩避让 + pan 守卫
 * - useTerminalSubscription：订阅失败重试 + 历史渲染就绪门控（加载遮罩放行）
 * 写入管线（useTerminalBuffer / writeCoalescer）与触摸滚动（useTerminalScroll）
 * 保持既有拆分不变。
 *
 * 渲染与滚动架构对齐桌面端 TerminalPreview.vue（VS Code 终端体验）：
 * - 写入管线：同帧输出经 rAF 合并 + 64KB 拆块（writeCoalescer），
 *   高频输出无撕裂/重影、超大块不卡主线程
 * - 渲染：默认 DOM 渲染器（xterm 内置 canvas，移动端 TUI 场景稳定无闪烁）；
 *   可选 WebGL addon（USE_WEBGL_RENDERER 开关，context loss 自动回退恢复）
 * - 滚动：onScroll 推导"是否在底部"（位置即状态），回到底部自动跟随输出
 * - 尺寸：ResizeObserver + rAF 节流 fit，cols/rows 实际变化才同步 PTY
 *
 * 移动端特殊处理：
 * - disableStdin：禁用 xterm 原生输入（桌面键盘输入流无法在移动端复现），
 *   输入统一由底部 TerminalInputBar 承担（命令/特殊键/快捷键面板）
 * - 触摸滚动接管：自定义触摸滚动 + 惯性 + 长按选择复制（useTerminalScroll）
 * - 键盘避让：visualViewport 优先 + 插件 safeAreaChanged 兜底双通道检测，
 *   terminal-view 根容器高度收缩压缩终端显示区高度（resize 语义，配合
 *   AndroidManifest adjustNothing）——行数实时重算并同步 PTY，TUI 完整
 *   重排可见；布局视口与可视区等高，无聚焦呈现视口 pan 空间
 * - Unicode11 addon：TUI 应用 box-drawing 字符列宽计算正确性
 */
defineOptions({ name: 'TerminalView' })

import { ref, computed, inject, type Ref, onMounted, onUnmounted, watch, nextTick } from 'vue'
import { logger } from '@/utils/frontendLogger'
import { useRouter, useRoute } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { Terminal } from '@xterm/xterm'
import { FitAddon } from '@xterm/addon-fit'
import { WebLinksAddon } from '@xterm/addon-web-links'
import { Unicode11Addon } from '@xterm/addon-unicode11'
import '@xterm/xterm/css/xterm.css'
import '@/styles/terminal.css'
import { useMobileConnection } from '@/composables/useMobileConnection'
import { isMockSession, useMockTerminal } from '@/composables/useMockTerminal'
import { useTerminalBuffer } from '@/composables/useTerminalBuffer'
import ConfirmDialog from '@/components/ConfirmDialog.vue'
import { useOrientation } from '@/composables/useOrientation'
import { useTheme } from '@/composables/useTheme'
import { useSettingsStore } from '@/stores/settings'
import { useInputAssistantStore } from '@/stores/inputAssistant'
import { useTerminalScroll } from '@/composables/useTerminalScroll'
import { FONT_FAMILY as METRICS_FONT_FAMILY, TERMINAL_LINE_HEIGHT, TERMINAL_SCROLLBAR_GUTTER_PX } from '@/utils/terminalMetrics'
import { TerminalResizeDebouncer } from '@/utils/terminalResizeDebouncer'
import { useTuiCompat } from '@/composables/useTuiCompat'
import { createTerminalKernel } from '@/composables/terminal/terminalKernel'
import { useTerminalRenderer } from '@/composables/terminal/useTerminalRenderer'
import { useTerminalResize } from '@/composables/terminal/useTerminalResize'
import { useTerminalKeyboardAvoidance } from '@/composables/terminal/useTerminalKeyboardAvoidance'
import { useTerminalSubscription } from '@/composables/terminal/useTerminalSubscription'
import { TERMINAL_SCROLLBACK } from '@/utils/terminalScrollback'
import TerminalHeader from '@/components/TerminalHeader.vue'
import TerminalSettingsModal from '@/components/TerminalSettingsModal.vue'
import type { ToolbarItemConfig, TerminalSettings } from '@/components/TerminalSettingsModal.vue'
import TerminalConfirmModal from '@/components/TerminalConfirmModal.vue'
import TerminalInputBar from '@/components/TerminalInputBar.vue'
import FileSidebar from '@/components/FileSidebar.vue'
import TaskPickerModal from '@/components/TaskPickerModal.vue'
import ShortcutConfigModal from '@/components/ShortcutConfigModal.vue'
import TerminalHelpModal from '@/components/TerminalHelpModal.vue'
import TerminalOnboardingTour from '@/components/TerminalOnboardingTour.vue'
import { useToast } from '@/composables/useToast'
import { usePresetTasks, executeTask, sendTask } from '@/composables/usePresetTasks'
import { resolveTerminalTheme } from '@/config/terminalThemes'
import type { PresetTask } from '@/composables/model'

// ====================================================================================
// 功能逻辑层（业务）：路由与状态 / 命令预设 / 键盘避让 / 输入与工具栏 / 设置 / 订阅与生命周期
// ====================================================================================
// ==================== Props & Route ====================

const router = useRouter()
const route = useRoute()
const { t } = useI18n()
const connection = useMobileConnection()
const mockTerminal = useMockTerminal()
const toast = useToast()
const { isLandscape } = useOrientation()
const { isSystemDark } = useTheme()
const { store: bufferStore, registerRealtimeHandler, unregisterRealtimeHandler, subscribeSession, unsubscribeSession, forceReplay, handleDisconnect, handleSessionStopped, markSessionRunning, sendInput } = useTerminalBuffer()
const settingsStore = useSettingsStore()
const assistStore = useInputAssistantStore()
const sessionId = computed(() => route.params.id as string)
// 挂载时固定会话 ID：卸载时路由导航已完成、route.params 已失效（undefined），
// 若仍读 sessionId.value 会导致 ws_leave_session 调用失败 → 桌面端订阅泄漏 →
// 重进会话时旧订阅流干扰游标连续性（violation 循环，终端多次进入才渲染完整）
const mountedSessionId = sessionId.value

// 安全区域从 App.vue inject
const safeArea = inject<Ref<{ top: number; bottom: number }>>('safeArea')!

// ==================== Task Picker ====================

const { tasks: presetTasks } = usePresetTasks()
const showTaskPicker = ref(false)

// ==================== Header Toolbar Config ====================

const ALL_TOOLBAR_ITEMS: ToolbarItemConfig[] = [
  { key: 'task', label: 'mobile.terminal.toolbarTask', icon: 'task' },
  { key: 'shortcut', label: 'mobile.terminal.toolbarShortcut', icon: 'shortcut' },
  { key: 'clear', label: 'mobile.terminal.toolbarClear', icon: 'clear' },
  { key: 'refresh', label: 'mobile.terminal.toolbarRefresh', icon: 'refresh' },
  { key: 'settings', label: 'mobile.terminal.toolbarSettings', icon: 'settings' },
  { key: 'folder', label: 'mobile.terminal.toolbarFolder', icon: 'folder' },
]

const visibleToolbarItems = computed(() => {
  const items = assistStore.settings.headerToolbarItems || ['folder']
  return ALL_TOOLBAR_ITEMS.filter(item => items.includes(item.key))
})

// ==================== State ====================

const xtermContainer = ref<HTMLDivElement | null>(null)
const scrollContainer = ref<HTMLDivElement | null>(null)
const isTerminalReady = ref(false)
// 根容器模板 ref：安全区 padding / 键盘避让高度收缩 / 页面 pan 守卫（keyboard 域消费）
const terminalViewRef = ref<HTMLElement | null>(null)
const resizeObserverRef = ref<ResizeObserver | null>(null)
// ResizeObserver rAF 节流句柄：同一帧内多次 fit 只执行一次
let resizeRaf = 0
// resize 分层防抖器（对齐桌面端 TerminalPreview）：垂直立即 / 水平 100ms 合并，
// onApply 接线 resize 域的 applyResize
let resizeDebouncer: TerminalResizeDebouncer | null = null

// ==================== History Render Gate ====================
// 门控三信号（本地回放完成 / 服务端历史段结束 / 首次 fit 校准）与超时兜底已迁入
// composables/terminal/useTerminalSubscription.ts
// （armGate / markReplayDone / markFirstFitDone / settleServerHistoryNow /
//   waitForHistoryGate）；此处仅保留渲染侧的一帧让出

/** 让出下一帧渲染（末批内容 commit 上屏后再撤遮罩；无 rAF 的测试环境立即继续） */
function nextPaintFrame(): Promise<void> {
  return new Promise((resolve) => {
    if (typeof requestAnimationFrame === 'function') {
      requestAnimationFrame(() => resolve())
    } else {
      resolve()
    }
  })
}

const showSettings = ref(false)
const showClearConfirm = ref(false)
const showSidebar = ref(false)
const showShortcutConfig = ref(false)
// 标题栏 ? 按钮：终端输入组件便捷功能教程弹窗
const showHelp = ref(false)
// 新手引导：首次进入终端页自动展示（terminalOnboardingPending），展示后清除；
// 设置里可重新开启（下次进入再显示）
const showOnboarding = ref(false)

// 侧栏「插入引用」待填入路径：TerminalInputBar 消费后置回 null
const pendingRefPath = ref<string | null>(null)

// 终端主题设置：theme 存储当前生效的主题名，isThemeUserSet 标记是否由用户手动指定
const terminalSettings = ref({
  fontSize: assistStore.settings.terminalFontSize,
  theme: assistStore.settings.terminalTheme
    ?? (settingsStore.settings.ui.theme === 'system'
      ? (isSystemDark.value ? 'dark' : 'light')
      : settingsStore.settings.ui.theme) as string,
  isThemeUserSet: assistStore.settings.isTerminalThemeUserSet,
})

/**
 * 当前生效的具体色板：'system' 解析为 dark/light（xterm 只接受可解析颜色，
 * var() 串会落回内置默认色）。同时供 xterm theme 选项与容器底色 CSS 变量
 * （--terminal-canvas-bg）共用：网格贴合后顶部 0~1 行余量、右侧行尾余量区
 * （约 1 列 + 滚动条预留宽）显示的是容器底色，必须与画布同色——此前容器
 * 用 App 主题 token（浅色取暖白），深色 TUI 下顶部/右侧露出浅色带。
 */
const resolvedTerminalTheme = computed(() =>
  resolveTerminalTheme(terminalSettings.value.theme, isSystemDark.value),
)

// 弹窗安全区域样式
const settingsModalStyle = computed(() => ({
  paddingTop: `${safeArea.value.top}px`,
  paddingBottom: `${safeArea.value.bottom}px`,
}))

const confirmModalStyle = computed(() => ({
  paddingTop: `${safeArea.value.top}px`,
  paddingBottom: `${safeArea.value.bottom}px`,
}))

// ==================== Computed ====================

const isConnected = computed(() =>
  connection.connectionStatus.value === 'connected' ||
  connection.connectionStatus.value === 'paired'
)

const session = computed(() => {
  if (isMockSession(sessionId.value)) {
    return { id: sessionId.value, name: t('mobile.session.mockName'), status: 'running', is_active: true }
  }
  return connection.activeSessions.value.find(s => s.id === sessionId.value)
})

// ==================== Agent CLI 预设（命令面板） ====================
// 预设识别需要会话的 config_id（activeSessions）与对应配置的启动命令
// （sessionConfigs）。两条数据源在通知跳转/路由恢复等直接进入终端页的路径上
// 都可能未就绪（loadActiveSessions 仅 DevicesView/SessionsView 调用），
// 故识别时按需补齐：会话缺失则按 sessionId 拉取会话列表反查 config_id，
// 配置缺失则现场拉取配置列表；识别结果需随会话切换更新，由 watch 响应式触发。
// 识别为 generic（未识别）时面板仅保留用户自定义命令。
// 会话的 config_id：WS 事件推送的会话对象为 camelCase（configId），
// HTTP /api/sessions 响应为 snake_case（config_id），两端来源需兼容（同 DevicesView 等）
const sessionConfigId = computed(() => session.value?.config_id ?? session.value?.configId)

let agentOverridesLoaded = false
// 按需拉取标记：并发触发（watch immediate + 数据到位）时只拉一次
let sessionsFetchStarted = false
let configsFetchStarted = false

/** 识别并应用当前会话的命令预设；数据未就绪时按需补齐（会话列表/配置列表）后重试 */
async function applyAgentPreset() {
  if (isMockSession(sessionId.value)) return // mock 会话无配置，不加载预设
  if (!agentOverridesLoaded) {
    await assistStore.loadAgentTypeOverrides()
    agentOverridesLoaded = true
  }
  // 会话未就绪：按会话 id 拉取会话列表（GET /api/sessions 自带 config_id）反查
  if (!sessionConfigId.value && !sessionsFetchStarted) {
    sessionsFetchStarted = true
    await connection.loadActiveSessions()
  }
  const configId = sessionConfigId.value
  if (!configId) {
    const found = connection.activeSessions.value.find(s => s.id === sessionId.value)
    logger.warn('[TerminalView] applyAgentPreset: 会话未就绪（无 config_id）', JSON.stringify({
      sessionId: sessionId.value,
      activeSessionsCount: connection.activeSessions.value.length,
      foundSession: found,
    }))
    return // 列表拉取失败或会话确实无配置，保留用户自定义命令
  }
  let config = connection.sessionConfigs.value.find(c => c.id === configId)
  // 配置列表未加载（DevicesView 之外的进入路径）：主动拉取一次，仍失败则等 watch 重触发
  if (!config && !configsFetchStarted) {
    configsFetchStarted = true
    await connection.loadSessionConfigs().catch(() => {})
    config = connection.sessionConfigs.value.find(c => c.id === configId)
  }
  if (!config) {
    logger.warn('[TerminalView] applyAgentPreset: 配置列表无匹配 config_id，预设不加载', { configId })
    return
  }
  const agentType = assistStore.getEffectiveAgentType(configId, config.command)
  assistStore.setAgentPreset(agentType)
}

// session/config 任一就绪或切换即重新识别（deep：SyncConfigCreated push 也能触发）
watch(
  [() => sessionConfigId.value, () => connection.sessionConfigs.value],
  () => { applyAgentPreset() },
  { immediate: true, deep: true },
)

const sessionName = computed(() => session.value?.name || sessionId.value || t('desktop.terminal.title'))

const isSessionActive = computed(() => isMockSession(sessionId.value) || (session.value?.status || 'stopped') === 'running')

const inputPlaceholder = computed(() => {
  // mock 会话与标题（mockName）不再重复：直接使用通用命令占位文案
  if (isMockSession(sessionId.value)) return t('mobile.input.commandPlaceholder')
  if (!isConnected.value) return t('mobile.input.disconnected') + '...'
  if (!isSessionActive.value) return t('mobile.connection.connectFailed')
  return t('mobile.input.commandPlaceholder')
})

const safeAreaTop = computed(() => safeArea.value.top || 0)

// ==================== 终端内核与域 ====================
// 共享内核：xterm 实例 / addon 实例 / 模板挂载点经 ctx 交换，跨域回调在调用时
// 解析（域创建顺序无关；详见 composables/terminal/terminalKernel.ts）
const kernel = createTerminalKernel(
  xtermContainer,
  () => sessionId.value,
  () => isConnected.value,
  () => isSessionActive.value,
)
const { terminalRef, fitAddonRef } = kernel
const renderer = useTerminalRenderer(kernel)
const resize = useTerminalResize(kernel)
const subscription = useTerminalSubscription(kernel, { bufferStore, subscribeSession })

// 模板同名绑定（域返回值解构，template 零改动）
const {
  rendererOverrideTarget,
  showRendererOverrideDialog,
  confirmRendererOverride,
  cancelRendererOverride,
} = resize

// TerminalInputBar 组件引用：键盘被系统收起时通知其退出编辑态（blur 输入框）
const inputBarRef = ref<InstanceType<typeof TerminalInputBar> | null>(null)

// 键盘避让域（移动端特有）：visualViewport 优先 + 插件 safeAreaChanged 兜底双通道
// 检测，terminal-view 根容器高度收缩承担避让（行数实时重算并同步 PTY）。
// onKeyboardHide 在键盘收起（偏移从可见归零）瞬间回调：先退出输入编辑态
// （光标消失、输入框收缩回单行、命令补全弹层关闭），再滚回最新行（键盘弹出期间
// 用户可能已上翻历史）。回调体内引用的 inputBarRef / scrollToBottomManual 由
// watch 在 setup 完成后触发解析，无 TDZ 问题
const keyboard = useTerminalKeyboardAvoidance({
  rootRef: terminalViewRef,
  safeAreaTop: () => safeAreaTop.value,
  canvasBackground: () => resolvedTerminalTheme.value.background,
  onKeyboardHide: () => {
    if (inputBarRef.value?.isFocused()) inputBarRef.value.blurInput()
    scrollToBottomManual()
  },
})
const { terminalViewStyle } = keyboard

// 可移动区域：终端内容 + 输入栏。键盘避让由 terminal-view 根容器高度收缩
// 承担（见上方 terminalViewStyle 注释）：终端区（flex:1）与输入栏随根容器
// 等比压缩/还原，ResizeObserver 触发重新 fit → 行数实时变化并同步 PTY。
// 此处的 movable-area 不再做任何避让变换/内边距——历史上先后用过
// translateY 整体平移与 padding-bottom 挤压，前者行数不变顶部被裁、后者
// 会与 WebView 聚焦呈现的视口 pan 叠加（双重补偿），均已废弃

/** 选择操作栏定位：避让选区和屏幕边界 */
const selectionBarStyle = computed(() => {
  const BAR_MARGIN = 10
  const EDGE_PADDING = 12

  const container = scrollContainer.value
  if (!container) return {}

  const rect = container.getBoundingClientRect()
  const estimatedBarWidth = 240
  const estimatedBarHeight = 40

  // 选区在容器内的像素范围（通过 viewport 行号 × 行高计算）
  let selTop = 0
  let selBottom = 0
  if (cellHeight.value > 0) {
    const topRow = Math.max(0, selectionViewportRange.topRow)
    const bottomRow = Math.min(terminalRef.value?.rows ?? topRow, selectionViewportRange.bottomRow + 1)
    selTop = topRow * cellHeight.value
    selBottom = bottomRow * cellHeight.value
  }

  // 水平：以长按位置为中心，限制不超出容器
  const relX = longPressTriggerPos.x - rect.left
  let left = relX - estimatedBarWidth / 2
  left = Math.max(EDGE_PADDING, Math.min(left, rect.width - estimatedBarWidth - EDGE_PADDING))

  // 垂直：优先选区上方，空间不足则选区下方，都不行则就近边缘
  let top: number
  const aboveTop = selTop - estimatedBarHeight - BAR_MARGIN
  const belowTop = selBottom + BAR_MARGIN

  if (aboveTop >= EDGE_PADDING) {
    top = aboveTop
  } else if (belowTop + estimatedBarHeight <= rect.height - EDGE_PADDING) {
    top = belowTop
  } else if (selTop < rect.height / 2) {
    // 选区偏上，操作栏放底部
    top = rect.height - estimatedBarHeight - EDGE_PADDING
  } else {
    // 选区偏下，操作栏放顶部
    top = EDGE_PADDING
  }

  return {
    top: `${top}px`,
    left: `${left}px`,
  }
})

// ==================== Input Handlers ====================
// 输入统一由 TerminalInputBar 承担（xterm 原生输入已禁用），
// 命令经终端 WS input 帧发送到主机会话（10 号票：替代旧 HTTP 输入路径），
// 特殊键以按键组合名形式发送

/**
 * 输入无法送达时的用户可见反馈。
 *
 * 此前仅在 `sendInput` 返回 false 时提示，而「未连接 / 会话非活跃」分支是静默
 * no-op——用户感知为「输入没反应」且没有任何线索。这里显式区分两种原因提示。
 */
function notifyInputUnavailable() {
  logger.warn(
    `[TerminalView] input dropped (${sessionId.value}): connected=${isConnected.value}, ` +
      `active=${isSessionActive.value}`,
  )
  toast.error(t(isConnected.value ? 'mobile.connection.connectFailed' : 'mobile.input.disconnected'))
}

function handleInputSubmit(text: string) {
  if (!terminalRef.value) return
  if (isMockSession(sessionId.value)) return
  if (isConnected.value && isSessionActive.value) {
    if (!sendInput(sessionId.value, text)) {
      toast.error(t('mobile.connection.connectFailed'))
    }
  } else {
    notifyInputUnavailable()
  }
}

async function handleInputExecute(text: string) {
  if (!terminalRef.value) return
  if (isMockSession(sessionId.value)) return
  if (isConnected.value && isSessionActive.value) {
    if (!sendInput(sessionId.value, text, 'enter')) {
      toast.error(t('mobile.connection.connectFailed'))
    }
  } else {
    notifyInputUnavailable()
  }
}

function handleSpecialKey(key: string) {
  if (isMockSession(sessionId.value)) return
  if (isConnected.value && isSessionActive.value) {
    if (!sendInput(sessionId.value, '', key)) {
      toast.error(t('mobile.connection.connectFailed'))
    }
  } else {
    notifyInputUnavailable()
  }
}

// ==================== Toolbar Actions ====================

function handleToolbarAction(key: string) {
  switch (key) {
    case 'task': showTaskPicker.value = true; break
    case 'shortcut': showShortcutConfig.value = true; break
    case 'clear': showClearConfirm.value = true; break
    case 'refresh': refreshTerminal(); break
    case 'settings': showSettings.value = true; break
    case 'folder': showSidebar.value = !showSidebar.value; break
    case 'help': showHelp.value = true; break
  }
}

// ==================== Settings ====================

function handleSettingsConfirm(settings: TerminalSettings) {
  terminalSettings.value.fontSize = settings.fontSize
  terminalSettings.value.theme = settings.theme
  terminalSettings.value.isThemeUserSet = settings.isThemeUserSet

  assistStore.saveSettings({
    quickBarCount: settings.quickBarCount,
    headerToolbarItems: settings.toolbarItems,
    terminalFontSize: terminalSettings.value.fontSize,
    terminalTheme: terminalSettings.value.isThemeUserSet ? terminalSettings.value.theme : null,
    isTerminalThemeUserSet: terminalSettings.value.isThemeUserSet,
    terminalOnboardingPending: settings.onboardingPending,
  })

  applyTerminalTheme()
  // 字号变更后重排：显式更新 xterm 字号 + 走统一口径 refit（fitWithMargin →
  // applyDprFit，DPR 感知 + 行尾安全余量），不再走裸 fitAddon.fit()；
  // 字体度量需重新测量，延迟与原实现一致；尺寸变化须同步 PTY 重排行宽
  if (terminalRef.value) {
    terminalRef.value.options.fontSize = settings.fontSize
  }
  setTimeout(() => {
    if (renderer.fitWithMargin()) resize.syncTerminalSizeToHost()
  }, 50)
  showSettings.value = false
}

// ==================== Misc Handlers ====================

/** 新手引导关闭：清除待展示标记（本设备已展示过一次） */
function handleOnboardingClose() {
  showOnboarding.value = false
  if (assistStore.settings.terminalOnboardingPending) {
    assistStore.saveSettings({ terminalOnboardingPending: false })
  }
}

/** 新手引导跳转完整教程：清除标记并打开帮助弹窗 */
function handleOnboardingOpenHelp() {
  showOnboarding.value = false
  if (assistStore.settings.terminalOnboardingPending) {
    assistStore.saveSettings({ terminalOnboardingPending: false })
  }
  showHelp.value = true
}

/** 侧边栏设置面板输入框聚焦/失焦时，控制键盘避让 */
function handleSettingsInputFocus(focused: boolean) {
  keyboard.setSettingsInputFocused(focused)
}

/** 侧栏「插入引用」：把 @路径 传给输入条填充，并收起侧栏露出输入区 */
function handleInsertRef(path: string) {
  pendingRefPath.value = path
  showSidebar.value = false
}

function handleBack() {
  router.back()
}

async function onTaskSend(task: PresetTask) {
  if (!isConnected.value || !isSessionActive.value) {
    toast.error(t('mobile.connection.connectFailed'))
    return
  }
  try {
    await sendTask(task, sessionId.value)
  } catch {
    toast.error(t('mobile.toolbox.sendFailed'))
  }
}

async function onTaskExecute(task: PresetTask) {
  if (!isConnected.value || !isSessionActive.value) {
    toast.error(t('mobile.connection.connectFailed'))
    return
  }
  try {
    await executeTask(task, sessionId.value)
  } catch {
    toast.error(t('mobile.toolbox.sendFailed'))
  }
}

// ==================== Subscribe with Retry ====================
// 订阅重试（失败 toast 一次 + 3s 定时重试，页面存活且会话活跃期间有效）已迁入
// composables/terminal/useTerminalSubscription.ts::subscribeWithRetry

// ==================== Lifecycle ====================

onMounted(async () => {
  // 链路调试（布局/渲染排查）：挂载起点 + 视口基线（与 fit 日志对照定位布局异常）
  logger.debug(
    `[TerminalView] mounted (session=${sessionId.value}): dpr=${window.devicePixelRatio}, ` +
      `viewport=${window.innerWidth}x${window.innerHeight}`,
  )
  // 键盘避让双通道监听（visualViewport / 插件 safeAreaChanged）+ 页面 pan 守卫
  keyboard.attach()

  // 兜底加载会话配置：DevicesView 之外的进入路径（通知跳转/路由恢复）从未调用过
  // loadSessionConfigs，预设识别需要其中的启动命令；加载完成后由上方 watch 触发识别。
  // 会话列表（activeSessions）不在此兜底——由 applyAgentPreset 按 sessionId 按需反查。
  if (!connection.hasLoadedConfigs.value && !connection.isLoadingConfigs.value) {
    connection.loadSessionConfigs().catch(() => {})
  }
  // 会话列表兜底：直接进入终端页（通知跳转/路由恢复）时 activeSessions 可能为空，
  // 会使 isSessionActive=false → 输入条禁用、输入被丢弃。主动拉一次会话列表。
  if (!connection.activeSessions.value.some(s => s.id === sessionId.value)) {
    connection.loadActiveSessions().catch(() => {})
  }

  await nextTick()

  // 历史渲染就绪门控布防：先于 initTerminal（回放完成信号在 handler 注册时
  // 即接线）与订阅路径（phase 监听需捕获 subscribe_ok 后 history 段全程）
  subscription.armGate()

  // 进入终端页 = 全量重播：xterm 每次进入都是全新实例，旧游标续传会丢失历史
  // （含后台期间已推进但从未渲染过的字节）。重置游标必须早于 initTerminal——
  // 其内部 registerRealtimeHandler 的 spliceHistory 会立即读取游标作为 from；
  // gap 自愈路径的 forceReplay 不重置游标（续传补缺口语义保持不变）
  bufferStore.resetCursor(sessionId.value)

  await initTerminal()

  // DEV 前缀：生产构建常量折叠为 false，整个 mock 分支（含 startOutput 调用）被 tree-shake
  if (import.meta.env.DEV && isMockSession(sessionId.value) && mockTerminal.isDev) {
    // mock 会话无服务端历史段：立即放行该门控条件（否则只能等超时兜底）
    subscription.settleServerHistoryNow()
    if (terminalRef.value) {
      mockTerminal.startOutput(terminalRef.value)
    }
  } else if (isSessionActive.value && isConnected.value) {
    // 会话页预加载已就绪（全量回放已在订阅期间缓冲，registerRealtimeHandler
    // 挂载时已写入 xterm）：跳过 forceReplay，避免清空已缓冲历史再次全量回放
    const prepared = bufferStore.consumePrepared() === sessionId.value
    if (!prepared) {
      // 全量重播已由上方 resetCursor（spliceHistory from=0）+ 下方重订阅承担；
      // forceReplay 在此仅为兜底（幂等：from=游标，若 spliceHistory 未及完成则再跑一次）
      forceReplay(sessionId.value)
    }
    await subscription.subscribeWithRetry()
  } else {
    // 非活跃/未连接：本次挂载不会发起订阅，无服务端历史段可等，立即放行；
    // 本地缓存仍由 replayDone 门控（重进展示最后已知内容）
    subscription.settleServerHistoryNow()
  }

  // 无条件同步一次尺寸（内部按 isConnected 门控）：会话状态 stale 时
  // 上方 isSessionActive 分支可能被跳过，不兜底会令 PTY 停留在桌面端
  // 宽度 → 移动端行尾截断；活跃时也由此处统一发送（避免重复调用）
  resize.syncTerminalSizeToHost()

  // 等历史输出渲染完成再撤遮罩：缓存回放 + 服务端历史段 + 首次 fit 三条件
  // 全部满足；任一环节卡死由 HISTORY_SETTLE_TIMEOUT_MS 超时兜底。
  // 放行后再让出一帧渲染，末批内容 commit 上屏后才淡出遮罩，
  // 避免遮罩半透明期间透出逐批写入的闪烁过程
  const gateResult = await subscription.waitForHistoryGate()
  await nextPaintFrame()
  isTerminalReady.value = true
  // 新手引导：终端就绪后按持久化标记展示一次（关闭或打开完整教程即清除）
  if (assistStore.settings.terminalOnboardingPending) {
    showOnboarding.value = true
  }
  // 链路调试（布局/渲染排查）：就绪路径（gate 正常 or 超时兜底）+ 终端网格尺寸，
  // 超时兜底说明订阅/历史/fit 某环节卡死（对照 terminalBuffer/useTerminalBuffer 日志）
  logger.debug(
    `[TerminalView] ready (session=${sessionId.value}): ` +
      `${gateResult === 'timeout' ? 'gate TIMEOUT fallback' : 'gate settled'}, ` +
      `term=${terminalRef.value?.cols ?? '?'}x${terminalRef.value?.rows ?? '?'}`,
  )

  // 入场渲染收尾（等价手动刷新按钮的渲染半段）：首次 fit/历史回放/遮罩淡出
  // 过渡期间真机 WebView 合成器可能缓存旧帧分块，表现为终端区底部与输入栏
  // 之间出现一段背景色空白间隔（点击刷新后消失的现场）。全量重绘 + 强制
  // 重合成一次清除（仅此一次，幂等低成本）
  if (terminalRef.value && terminalRef.value.rows > 0) {
    terminalRef.value.refresh(0, terminalRef.value.rows - 1)
  }
  forceCompositorRepaint()
})

onUnmounted(async () => {
  // 链路调试：渲染管线卸载（writeCoalescer dispose 汇总日志随后输出）
  logger.debug(`[TerminalView] unmounted (session=${mountedSessionId})`)
  // 订阅重试定时器 + 门控兜底定时器（遮罩已放行时为 null，防御未走完 onMounted 的卸载竞态）
  subscription.disposeSubscription()

  if (panelRepaintTimer) {
    clearTimeout(panelRepaintTimer)
    panelRepaintTimer = null
  }
  // 键盘避让双通道监听 + 页面 pan 守卫
  keyboard.dispose()

  if (isMockSession(mountedSessionId)) {
    mockTerminal.stopOutput()
  }
  disposeTerminal()

  // 页面卸载：停止前端消费 + 切 batch 传播（Rust 订阅保持，会话未停——
  // 后台期间的输出由服务端队列 + Rust 缓存保留）；重新进入时由
  // onMounted 的 resetCursor（全量重播）+ registerRealtimeHandler 拼接历史
  if (!isMockSession(mountedSessionId)) {
    await unsubscribeSession(mountedSessionId)
  }
})

watch(isSessionActive, async (active, prevActive) => {
  if (!sessionId.value || isMockSession(sessionId.value)) return
  if (active && !prevActive) {
    // 会话恢复运行（含同 id 重启）：复位 sessionStopped，否则 ws_output
    // 监听器会永久丢弃新流帧（事件路径 SyncSessionStatusChanged 已复位，
    // 此处兜底防事件丢失场景）
    markSessionRunning(sessionId.value)
    // 会话停止/重启后偏移空间从 0 重建，游标已被 markSessionStopped 重置，
    // 此处订阅即全量重播；页面存活场景走增量续传
    await subscription.subscribeWithRetry()
    // 会话激活（含重连后）时 PTY 可能仍是默认尺寸，主动同步一次
    resize.syncTerminalSizeToHost()
  } else if (!active && prevActive) {
    await handleSessionStopped(sessionId.value)
  }
})

watch(isConnected, async (connected) => {
  if (!sessionId.value || isMockSession(sessionId.value)) return
  if (!connected) {
    handleDisconnect()
    subscription.clearSubscribeRetry()
  } else {
    if (isSessionActive.value) {
      // 重连成功后 PTY 重建为默认 80x24，需主动同步当前尺寸
      await subscription.subscribeWithRetry()
    }
    // 无论会话状态是否 stale 都重发尺寸（服务端 404 无害），
    // 避免 PTY 停留在桌面端宽度导致移动端行尾截断
    resize.syncTerminalSizeToHost()
  }
})
// ====================================================================================
// 渲染输出层（终端显示）：xterm 实例 / 输出写入 / 触摸滚动 / 尺寸同步 / 清屏与重绘
// ====================================================================================
// ==================== TUI 兼容 ====================
// TUI 模式（alt screen + SGR 鼠标上报）下手势转滚轮事件转发给应用内部滚动；
// 由 useTuiCompat 持有检测/发送，useTerminalScroll 仅注入模式门控分流

const { isTuiMode, attach: attachTuiCompat, feedOutput: feedTuiOutput, sendWheel: sendTuiWheel, dispose: disposeTuiCompat } = useTuiCompat(sessionId.value)

// ==================== Terminal Scroll ====================

const {
  currentLine,
  isSelectionMode,
  hasSelection,
  selectionTouchEnded,
  scrollbarVisible,
  scrollbarThumbStyle,
  xtermContainerStyle,
  shortcutsPanelHeight,
  isUserScrolling,
  cellHeight,
  scrollToBottomManual,
  setupViewportScroll,
  exitSelectionMode,
  copySelection,
  selectAllText,
  handleShortcutsPanelToggle,
  dispose: disposeScroll,
  longPressTriggerPos,
  selectionViewportRange,
} = useTerminalScroll(terminalRef, scrollContainer, { isTuiMode, sendWheel: sendTuiWheel })

// ==================== Watchers ====================
// 键盘收起（偏移归零）的统一回调（退出输入编辑态 + 滚回最新行）由键盘避让域
// 内部 watch 触发，经 useTerminalKeyboardAvoidance 的 onKeyboardHide 接线

// 快捷键面板收起后强制重绘：xterm 容器经 translateY(-h) 上移后还原时，真机
// WebView 合成层会残留旧帧分块（错位/露出主题背景色，实测表现为终端区出现
// 米白横带与右侧竖带、底部“间隔”）。过渡动画（250ms）结束后强制 xterm 重绘
// 全部行 + 合成器重合成，清除残留（与入场渲染收尾同模式）
let panelRepaintTimer: ReturnType<typeof setTimeout> | null = null
watch(shortcutsPanelHeight, (height) => {
  // 仅面板收起（还原 transform）时需要清理；展开时上移由合成器处理
  if (height > 0) return
  if (panelRepaintTimer) clearTimeout(panelRepaintTimer)
  panelRepaintTimer = setTimeout(() => {
    panelRepaintTimer = null
    if (terminalRef.value && terminalRef.value.rows > 0) {
      terminalRef.value.refresh(0, terminalRef.value.rows - 1)
    }
    forceCompositorRepaint()
  }, 320)
})

watch(() => settingsStore.settings.ui.theme, (uiTheme) => {
  if (terminalSettings.value.isThemeUserSet) return
  const resolved = uiTheme === 'system'
    ? (isSystemDark.value ? 'dark' : 'light')
    : uiTheme
  if (terminalSettings.value.theme !== resolved) {
    terminalSettings.value.theme = resolved as string
    applyTerminalTheme()
  }
})

watch(isSystemDark, () => {
  if (terminalSettings.value.isThemeUserSet) return
  if (settingsStore.settings.ui.theme !== 'system') return
  terminalSettings.value.theme = isSystemDark.value ? 'dark' : 'light'
  applyTerminalTheme()
})

// ==================== Terminal Setup ====================
// 渲染器域（USE_WEBGL_RENDERER 开关 / WebGL addon 加载与 context-loss 恢复 /
// 网格测量与构造期预估 / DPR 感知 fit / atlas 预热 / DPR 变化监听）已迁入
// composables/terminal/useTerminalRenderer.ts

// 终端字体栈：唯一真源在 utils/terminalMetrics（构造选项与启动尺寸预估共用），此处仅导入
const FONT_FAMILY = METRICS_FONT_FAMILY

async function initTerminal() {
  if (!xtermContainer.value) return

  // 创建前预测量：直接以适配屏幕的行列值构造，不再经过默认 80x24 阶段
  const initial = renderer.computeInitialSize(terminalSettings.value.fontSize ?? 14)

  const term = new Terminal({
    // 渲染器：默认 DOM（xterm 内置 canvas）；USE_WEBGL_RENDERER 开启时
    // WebGL addon 加载成功后自动接管渲染，失败则保持 DOM
    // 字体与尺寸（对齐桌面端，VS Code 终端默认字体栈 + 跨平台回退）
    cols: initial.cols,
    rows: initial.rows,
    fontSize: terminalSettings.value.fontSize,
    fontFamily: FONT_FAMILY,
    // 行高倍率（唯一真源 TERMINAL_LINE_HEIGHT）：小屏 CJK 满屏输出行间呼吸感；
    // 与 measureCellSize/computeGridSize 同源，保证预估网格与渲染口径一致
    lineHeight: TERMINAL_LINE_HEIGHT,
    // 滚动历史行数（与桌面主机服务端事件队列容量对齐）
    scrollback: TERMINAL_SCROLLBACK,
    // 自绘滚动条预留宽（唯一真源 TERMINAL_SCROLLBAR_GUTTER_PX）：xterm 6 内部
    // verticalScrollbarSize 与 FitAddon 可用宽扣除同源取此值（缺省 14px，
    // 原生滚动条已被 CSS 隐藏却仍按 14px 预留 → 右侧固定空白竖条）。
    // 设为自绘指示线足迹后，画布右缘与滚动条零重叠且死区收窄到 6px
    overviewRuler: { width: TERMINAL_SCROLLBAR_GUTTER_PX },
    // 默认即时滚动：关闭平滑滚动，避免滚动动画期间合成器缓存旧帧导致重影；
    // 仅在惯性甩动时由 useTerminalScroll 临时开启（smoothScrollDuration）
    // 做单次平滑滑行，滑行结束立即复位为 0
    smoothScrollDuration: 0,
    // VS Code 风格块光标：移动端保留光标（标记输入落点与 TUI 光标位置），
    // DOM 渲染器自带光标层，无需额外处理
    cursorBlink: true,
    cursorStyle: 'block',
    cursorWidth: 1,
    drawBoldTextInBrightColors: true,
    // 移动端特殊处理：禁用 xterm 原生输入。
    // 桌面端键盘输入流（onData → PTY）无法在移动端复现，输入统一由底部
    // TerminalInputBar 承担，避免软键盘误弹与焦点抢占
    disableStdin: true,
    // 主题（'system' 已解析为具体色板，禁止把 var() 串传给 xterm）
    theme: resolvedTerminalTheme.value,
    allowProposedApi: true,
  })

  terminalRef.value = term

  // 挂载 addon（对齐桌面端顺序：addon 先于 open）
  const addon = new FitAddon()
  fitAddonRef.value = addon
  term.loadAddon(addon)
  term.loadAddon(new WebLinksAddon())

  // Unicode11 addon（移动端特殊处理）：启用 Unicode 11 字符宽度计算。
  // TUI 应用（opencode 等）大量使用 box-drawing 字符（╔═╗║╚╝）和 emoji，
  // 不加载此 addon 时 xterm 默认字符宽度表为 Unicode 5，
  // 部分新字符的列宽计算错误会导致光标位置漂移、上一个写入的字符部分残留（重影）
  const unicode11 = new Unicode11Addon()
  term.loadAddon(unicode11)
  term.unicode.activeVersion = '11'

  term.open(xtermContainer.value)

  // 渲染器初始化：按 USE_WEBGL_RENDERER 决策加载 WebGL（或保持 DOM），
  // WebGL 激活后隐藏 DOM 层光标（保留 WebGL 层光标，避免双光标）
  await renderer.initRenderer(term)

  // 注册实时 handler — 历史分片回放（高水位节流，见 useTerminalBuffer）与
  // 实时推送同通道写入；背压 ack 由 useTerminalBuffer 无条件回发（onWriteParsed
  // 即证明本端在消费，不依赖正统归属，见 composable 注释）
  // 回放完成信号接入加载遮罩门控：末批解析完成后才允许撤遮罩
  const { replayDone } = registerRealtimeHandler(sessionId.value, term, feedTuiOutput)
  void replayDone.then(() => subscription.markReplayDone())

  // 本地历史缓存曾被头部 LRU 裁剪（超 16MB）：本次回放起点非流首，可能切断
  // 转义序列，提示历史不完整（渲染残留由 composable 的回放静止全量重绘兜底）
  if (bufferStore.getBuffer(sessionId.value)?.headTrimmed) {
    toast.warning(t('mobile.terminal.historyTruncated'))
  }

  // TUI 兼容：挂接 onWriteParsed 检测备用屏幕（与嗅探器构成双条件门控）
  attachTuiCompat(term)

  // 触摸滚动接管 + 首帧校准 fit：
  // - setupViewportScroll 不依赖字体测量（viewport 在 open 后即存在于 DOM），
  //   必须无条件挂载，否则触摸滚动/历史查看永久失效
  // - fitWithMargin 幂等（FitAddon 在字体测量未就绪时无操作），轮询重试直至
  //   校准生效；尺寸变化经 onResize → 串行队列发送（自动合并最新值）
  //
  // 收敛语义（修顶部落差）：不“首次变化即停”，而是连续多次无变化才算稳定。
  // 原因：charMeasure 字体度量就绪晚于首次 fit（初始估值偏大 → 行数偏少），
  // 首次 fit 从默认 80×24 变化即停会锁定偏小网格；容器尺寸此后不变时
  // ResizeObserver 不再触发，顶部空带（标题栏与首行之间）无法自愈。
  // 网格贴底对齐后（.xterm bottom:0），行偏少的缺额全部暴露在顶部。
  // 配合 terminalResizePolicy 行双向即时生效，网格收敛到 floor(容器高/行高)。
  setTimeout(() => {
    setupViewportScroll()
    // 连续无变化次数达到阈值即视为收敛（字体度量已稳定）
    const STABLE_FITS = 3
    // 总重试上限：50ms 间隔 × 40 ≈ 2s，超时放行遮罩门控（防异常态无限循环）
    const MAX_TOTAL_ATTEMPTS = 40
    let stableCount = 0
    let totalAttempts = 0
    // 是否已发生过至少一次成功校准：字体度量未就绪时 fit 是 no-op（无变化），
    // 不能据此提前收敛（会把网格锁死在默认 80×24），必须先有一次真实校准
    let everChanged = false
    const tryInitialFit = () => {
      if (!terminalRef.value) return
      const changed = renderer.fitWithMargin()
      if (changed) {
        // 校准生效：补发一次实际尺寸（队列合并，防 onResize 门控漏发），
        // 并重置稳定计数——尺寸仍在变化（字体度量未稳），继续收敛
        resize.syncTerminalSizeToHost()
        everChanged = true
        stableCount = 0
      } else {
        stableCount++
        if (everChanged && stableCount >= STABLE_FITS) {
          // 已成功校准过且连续多次 fit 无变化：网格已收敛，放行遮罩门控
          subscription.markFirstFitDone()
          return
        }
      }
      if (++totalAttempts < MAX_TOTAL_ATTEMPTS) {
        setTimeout(tryInitialFit, 50)
      } else {
        // 收敛超时：放弃继续校准并放行遮罩门控（后续尺寸由 ResizeObserver 兜底）
        subscription.markFirstFitDone()
      }
    }
    tryInitialFit()
  }, 50)

  // ResizeObserver — 接入分层防抖（对齐桌面端 TerminalPreview / VS Code）：
  // 垂直 resize（行数变化）立即应用，仅宽度变化 100ms 防抖合并，避免旋转/键盘
  // 避让触发容器尺寸微调时每帧整屏 reflow；0/非法尺寸（隐藏/过渡中 RO 报 0）被
  // 防抖器忽略，避免把 PTY 缩成 1×1 打乱 shell。小缓冲（<200 行，VS Code
  // StartDebouncingThreshold）连宽度变化也立即应用。flush() 保证防抖窗口内最后
  // 一次尺寸必达。仅 cols/rows 实际变化才同步 PTY（见 applyResize）。
  resizeDebouncer = new TerminalResizeDebouncer({
    onApply: () => resize.applyResize(),
    getBufferLength: () => {
      const t = terminalRef.value
      return t ? t.buffer.active.length : null
    },
  })
  resizeObserverRef.value = new ResizeObserver((entries) => {
    const rect = entries[0]?.contentRect
    if (rect) {
      // rAF 聚合同一帧的多次回调，再喂给防抖器（采集侧节流，防抖器负责应用侧调度）
      if (resizeRaf) return
      resizeRaf = requestAnimationFrame(() => {
        resizeRaf = 0
        resizeDebouncer?.resize(rect.width, rect.height)
      })
    }
  })
  resizeObserverRef.value.observe(xtermContainer.value)

  // DPR 动态变化监听（跨 DPI 旋转 / 系统缩放变化时窗口尺寸可能不变，
  // ResizeObserver 不触发）：matchMedia 只能匹配固定 dppx 值，变化后按新值递归注册
  renderer.watchDprChanges()

  // PTY 尺寸同步：xterm 内部 resize（含 fit 触发）时同步到主机会话。
  // 统一走 HTTP 串行队列（resize 域 queueResize）：HTTP 与 WS 双通道并发会把不同
  // 尺寸的请求乱序送达服务端——fit 前的 80x24 默认值若后到会覆盖实际
  // 尺寸，PTY 停在 80x24 → opencode 按 24 行渲染，显示区下半黑（半屏黑）
  term.onResize(({ cols, rows }) => {
    // 调试验证：记录 xterm 每次尺寸变化（fit/容器变化/字号变化）
    logger.debug(`[TerminalView] onResize: ${cols}x${rows}`)
    resize.queueResize(cols, rows)
  })
}

// ==================== Resize 串行队列 / 正统渲染端裁决 ====================
// 已迁入 composables/terminal/useTerminalResize.ts
// （queueResize / syncTerminalSizeToHost / applyResize + 覆盖确认弹窗状态）

function disposeTerminal() {
  if (resizeObserverRef.value) {
    resizeObserverRef.value.disconnect()
    resizeObserverRef.value = null
  }
  if (resizeRaf) {
    cancelAnimationFrame(resizeRaf)
    resizeRaf = 0
  }
  // 清理 resize 分层防抖器（去不触发挂起应用）
  resizeDebouncer?.dispose()
  resizeDebouncer = null
  // 清理渲染器域资源（atlas 预热定时器 + DPR 变化监听）
  renderer.disposeRenderer()

  // 卸载时 route.params 已失效（undefined），须用挂载时固定的会话 ID，
  // 否则 handler 注销被守卫跳过 → 残留闭包引用已 dispose 的 xterm
  if (mountedSessionId) {
    unregisterRealtimeHandler(mountedSessionId)
  }

  disposeTuiCompat()
  disposeScroll()

  if (terminalRef.value) {
    terminalRef.value.dispose()
    terminalRef.value = null
    fitAddonRef.value = null
  }
  isTerminalReady.value = false
}

function applyTerminalTheme() {
  if (!terminalRef.value) return
  terminalRef.value.options.theme = resolvedTerminalTheme.value
  renderer.fitWithMargin()
}

// ==================== Clear Terminal ====================

function clearTerminal() {
  if (!terminalRef.value) return
  terminalRef.value.clear()
  currentLine.value = 0
  isUserScrolling.value = false
  showClearConfirm.value = false
}

// ==================== Refresh Terminal ====================
// 主动同步尺寸（syncTerminalSizeToHost）已迁入 resize 域

/** 合成层强制重绘：1px transform 往返抖动，迫使 WebView 合成器重新合成 canvas 层。
 * xterm 渲染管线挂起（脏区跳过等）时 refresh() 不生效，
 * DOM transform 变化能绕过渲染管线直接触发合成器重绘（实测：键盘避让后黑屏恢复） */
function forceCompositorRepaint() {
  const el = xtermContainer.value
  if (!el) return
  // 读取当前生效 transform（xtermContainerStyle 可能已有值），往返后还原
  const current = getComputedStyle(el).transform
  el.style.transform = 'translateY(1px)'
  requestAnimationFrame(() => {
    el.style.transform = current
  })
}

async function refreshTerminal() {
  if (!fitAddonRef.value || !terminalRef.value) return

  renderer.fitWithMargin()
  // 强制重绘可见区：fit 尺寸不变时不触发重排，WebGL 渲染残留需要手动刷新
  if (terminalRef.value.rows > 0) {
    terminalRef.value.refresh(0, terminalRef.value.rows - 1)
  }
  // 合成层强制重绘：xterm 渲染管线挂起时 refresh() 不生效，
  // transform 往返迫使合成器重新合成 canvas（渲染层恢复）
  forceCompositorRepaint()

  if (isConnected.value && isSessionActive.value) {
    // 用户显式刷新 = 明确意图：清空此前「拒绝覆盖尺寸」的记录，让尺寸仲裁重新
    // 走一遍（否则同尺寸请求被永久抑制，PTY 尺寸再也不会被纠正）
    resize.clearRejectedSize()
    // 统一走串行队列（过滤未校准默认值 + 单通道保序），失败仅 console.warn
    resize.queueResize(terminalRef.value.cols, terminalRef.value.rows)
    // 数据层兜底：渲染层恢复后内容仍缺失（violation 风暴期间帧被拒）时
    // 续传重拼接（forceReplay from=游标——同实例 scrollback 仍在，无需全量）
    if (!isMockSession(sessionId.value)) {
      // 订阅信念对账：Rust 幂等订阅不发状态事件，长时间未收事件的会话
      // 需主动拉状态收敛（否则刷新后输入仍可能被 subscribed 门控拒绝）
      await bufferStore.reconcileState(sessionId.value)
      forceReplay(sessionId.value)
      await subscription.subscribeWithRetry()
    }
  }
  toast.success(t('mobile.terminal.refreshed'))
}

</script>
