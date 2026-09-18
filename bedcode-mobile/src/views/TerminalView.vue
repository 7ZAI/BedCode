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
      @back="router.back()"
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
      @confirm="handleClearConfirm"
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
 * 终端视图（移动端）— 编排层：Vue 生命周期接线（onMounted / onUnmounted / watch）
 *
 * 业务函数按**大颗粒度主题**拆分到 `src/composables/terminal/`（范式参考桌面端
 * `bedcode-desktop/src/composables/terminal/`），共享实例与跨域回调经 terminalKernel
 * 交换（回调在调用时解析，域创建顺序无关）：
 * - **useTerminalDisplay**（终端显示）：xterm 实例装配与销毁、主题解析与网格重排、
 *   清屏 / 手动刷新 / 合成层强制重绘、选择操作栏定位
 * - **useTerminalInput**（终端输入）：输入栏回传（文本/执行/特殊键）、预设任务
 *   发送与执行、命令面板预设识别、侧栏「插入引用」填充
 * - **useTerminalPanels**（功能栏）：标题栏/侧边栏/弹窗开关状态、工具栏动作分发、
 *   新手引导
 * - useTerminalRenderer（渲染器）/ useTerminalResize（PTY 尺寸裁决与正统端确认）/
 *   useTerminalKeyboardAvoidance（键盘避让）/ useTerminalSubscription（订阅重试与
 *   历史门控）——2026-09-18 首轮拆出，职责与边界见各自文件头
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
import '@xterm/xterm/css/xterm.css'
import '@/styles/terminal.css'
import { useMobileConnection } from '@/composables/useMobileConnection'
import { isMockSession, useMockTerminal } from '@/composables/useMockTerminal'
import { useTerminalBuffer } from '@/composables/useTerminalBuffer'
import ConfirmDialog from '@/components/ConfirmDialog.vue'
import { useOrientation } from '@/composables/useOrientation'
import { useInputAssistantStore } from '@/stores/inputAssistant'
import { useTerminalScroll } from '@/composables/useTerminalScroll'
import { useTuiCompat } from '@/composables/useTuiCompat'
import { createTerminalKernel } from '@/composables/terminal/terminalKernel'
import { useTerminalRenderer } from '@/composables/terminal/useTerminalRenderer'
import { useTerminalResize } from '@/composables/terminal/useTerminalResize'
import { useTerminalKeyboardAvoidance } from '@/composables/terminal/useTerminalKeyboardAvoidance'
import { useTerminalSubscription } from '@/composables/terminal/useTerminalSubscription'
import { useTerminalDisplay } from '@/composables/terminal/useTerminalDisplay'
import { useTerminalInput } from '@/composables/terminal/useTerminalInput'
import { useTerminalPanels } from '@/composables/terminal/useTerminalPanels'
import { nextPaintFrame } from '@/utils/nextPaintFrame'
import type { ToolbarItemConfig, TerminalSettings } from '@/components/TerminalSettingsModal.vue'
import TerminalHeader from '@/components/TerminalHeader.vue'
import TerminalSettingsModal from '@/components/TerminalSettingsModal.vue'
import TerminalConfirmModal from '@/components/TerminalConfirmModal.vue'
import TerminalInputBar from '@/components/TerminalInputBar.vue'
import FileSidebar from '@/components/FileSidebar.vue'
import TaskPickerModal from '@/components/TaskPickerModal.vue'
import ShortcutConfigModal from '@/components/ShortcutConfigModal.vue'
import TerminalHelpModal from '@/components/TerminalHelpModal.vue'
import TerminalOnboardingTour from '@/components/TerminalOnboardingTour.vue'
import { usePresetTasks } from '@/composables/usePresetTasks'

// ==================== Props & Route ====================

const router = useRouter()
const route = useRoute()
const { t } = useI18n()
const connection = useMobileConnection()
const mockTerminal = useMockTerminal()
const { isLandscape } = useOrientation()
const { store: bufferStore, registerRealtimeHandler, unregisterRealtimeHandler, subscribeSession, unsubscribeSession, forceReplay, handleDisconnect, handleSessionStopped, markSessionRunning, sendInput } = useTerminalBuffer()
const assistStore = useInputAssistantStore()
const sessionId = computed(() => route.params.id as string)
// 挂载时固定会话 ID：卸载时路由导航已完成、route.params 已失效（undefined），
// 若仍读 sessionId.value 会导致 ws_leave_session 调用失败 → 桌面端订阅泄漏 →
// 重进会话时旧订阅流干扰游标连续性（violation 循环，终端多次进入才渲染完整）
const mountedSessionId = sessionId.value

// 安全区域从 App.vue inject
const safeArea = inject<Ref<{ top: number; bottom: number }>>('safeArea')!

const { tasks: presetTasks } = usePresetTasks()

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

// ==================== State（模板 ref 与页面级开关） ====================

const xtermContainer = ref<HTMLDivElement | null>(null)
const scrollContainer = ref<HTMLDivElement | null>(null)
const isTerminalReady = ref(false)
// 根容器模板 ref：安全区 padding / 键盘避让高度收缩 / 页面 pan 守卫（keyboard 域消费）
const terminalViewRef = ref<HTMLElement | null>(null)

// ==================== Computed（会话与连接投影） ====================

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

// 会话的 config_id：WS 事件推送的会话对象为 camelCase（configId），
// HTTP /api/sessions 响应为 snake_case（config_id），两端来源需兼容（同 DevicesView 等）
const sessionConfigId = computed(() => session.value?.config_id ?? session.value?.configId)

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

// 弹窗安全区域样式
const settingsModalStyle = computed(() => ({
  paddingTop: `${safeArea.value.top}px`,
  paddingBottom: `${safeArea.value.bottom}px`,
}))

const confirmModalStyle = computed(() => ({
  paddingTop: `${safeArea.value.top}px`,
  paddingBottom: `${safeArea.value.bottom}px`,
}))

// ==================== 终端内核与域 ====================
// 共享内核：xterm 实例 / addon 实例 / 模板挂载点经 ctx 交换，跨域回调在调用时
// 解析（域创建顺序无关；详见 composables/terminal/terminalKernel.ts）
const kernel = createTerminalKernel(
  xtermContainer,
  () => sessionId.value,
  () => isConnected.value,
  () => isSessionActive.value,
)
const { terminalRef } = kernel
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

// TUI 兼容：alt screen + SGR 鼠标上报双条件门控（嗅探器持有检测/发送，
// useTerminalScroll 仅注入模式门控分流）
const { isTuiMode, attach: attachTuiCompat, feedOutput: feedTuiOutput, sendWheel: sendTuiWheel, dispose: disposeTuiCompat } = useTuiCompat(sessionId.value)

// 触摸滚动接管（含惯性/长按选择/自绘滚动条）
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

// 终端显示域（实例装配/销毁 + 主题 + 清屏/刷新/重绘 + 选择操作栏定位）
const {
  terminalSettings,
  resolvedTerminalTheme,
  applyTerminalTheme,
  initTerminal,
  disposeTerminal,
  forceCompositorRepaint,
  clearTerminal,
  refreshTerminal,
  selectionBarStyle,
} = useTerminalDisplay(kernel, {
  renderer,
  resize,
  subscription,
  bufferStore,
  registerRealtimeHandler,
  unregisterRealtimeHandler,
  setupViewportScroll,
  attachTuiCompat,
  feedTuiOutput,
  disposeTuiCompat,
  disposeScroll,
  forceReplay,
  currentLine,
  isUserScrolling,
  cellHeight,
  selectionViewportRange,
  longPressTriggerPos,
  scrollContainerRef: scrollContainer,
  mountedSessionId,
  setReady: (ready) => { isTerminalReady.value = ready },
})

// 终端输入域（输入回传 + 预设任务 + 命令面板预设识别 + 侧栏引用填充）
const {
  pendingRefPath,
  handleInputSubmit,
  handleInputExecute,
  handleSpecialKey,
  onTaskSend,
  onTaskExecute,
  applyAgentPreset,
  handleInsertRef: fillPendingRef,
} = useTerminalInput(kernel, { sendInput, getConfigId: () => sessionConfigId.value })

// 功能栏域（弹窗/侧边栏开关 + 工具栏动作分发 + 新手引导）
const {
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
} = useTerminalPanels({ refreshTerminal })

// 键盘避让域（移动端特有）：visualViewport 优先 + 插件 safeAreaChanged 兜底双通道
// 检测，terminal-view 根容器高度收缩承担避让（行数实时重算并同步 PTY）。
// onKeyboardHide 在键盘收起（偏移从可见归零）瞬间回调：先退出输入编辑态
// （光标消失、输入框收缩回单行、命令补全弹层关闭），再滚回最新行（键盘弹出期间
// 用户可能已上翻历史）。回调体内引用的 inputBarRef / scrollToBottomManual 由
// watch 在 setup 完成后触发解析，无 TDZ 问题
const inputBarRef = ref<InstanceType<typeof TerminalInputBar> | null>(null)
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

// ==================== 模板事件接线（薄封装：跨两个域的一步操作） ====================

/** 侧栏「插入引用」：填充输入条 + 收起侧栏露出输入区 */
function handleInsertRef(path: string) {
  fillPendingRef(path)
  closeSidebar()
}

/** 清屏确认：清屏 + 关闭确认弹窗 */
function handleClearConfirm() {
  clearTerminal()
  showClearConfirm.value = false
}

/** 设置确认：持久化 + 应用主题 + 字号变更后重排并同步 PTY */
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

/** 侧边栏设置面板输入框聚焦/失焦时，控制键盘避让 */
function handleSettingsInputFocus(focused: boolean) {
  keyboard.setSettingsInputFocused(focused)
}

// ==================== Watchers ====================

// 命令面板预设：session/config 任一就绪或切换即重新识别
// （deep：SyncConfigCreated push 也能触发）
watch(
  [() => sessionConfigId.value, () => connection.sessionConfigs.value],
  () => { applyAgentPreset() },
  { immediate: true, deep: true },
)

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
</script>
