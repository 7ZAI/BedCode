<template>
  <div
    class="h-screen relative overflow-hidden flex flex-col bg-[var(--bg-page)]"
    :class="isShown ? (revealDone ? 'opacity-100' : 'animate-fade-slide-up') : 'opacity-0'"
    @animationend="onRevealEnd"
  >
    <!-- ==================== 40px 工具条：左信息，右操作 ==================== -->
    <header
      class="h-10 shrink-0 flex items-center justify-between px-3 border-b border-[var(--border)] bg-[var(--bg-card)]"
      data-tauri-drag-region
    >
      <div class="flex items-center gap-3 min-w-0" data-tauri-drag-region>
        <div class="flex items-center gap-2 min-w-0 shrink-0" data-tauri-drag-region>
          <span :class="['w-2 h-2 rounded-full shrink-0', statusColor]" data-tauri-drag-region></span>
          <span class="wb-mono text-[calc(13px*var(--ui-scale))] font-semibold text-[var(--text-primary)] truncate" data-tauri-drag-region>
            {{ sessionName }}
          </span>
          <span class="text-[calc(10.5px*var(--ui-scale))] font-semibold tracking-[0.08em] uppercase shrink-0" :class="statusLabelClass" data-tauri-drag-region>
            {{ statusText }}
          </span>
        </div>

        <!-- 会话信息：cwd / 命令（mono 小字） -->
        <div
          v-if="config"
          class="hidden sm:flex items-center gap-2 min-w-0 wb-mono text-[calc(12.5px*var(--ui-scale))] text-[var(--text-secondary)]"
          data-tauri-drag-region
        >
          <span v-if="workingDir" class="truncate max-w-64" :title="workingDir">{{ workingDir }}</span>
          <span v-if="workingDir && command" class="text-[var(--text-tertiary)]">·</span>
          <span v-if="command" class="truncate max-w-48" :title="command">{{ command }}</span>
        </div>
      </div>

      <div class="flex items-center gap-1.5">
        <PluginPageToolbar target="terminal" />
        <!-- 停止会话 -->
        <button
          class="wb-btn-primary !h-6 !px-2.5 !text-[calc(11px*var(--ui-scale))] uppercase"
          @click="stopSession"
        >
          {{ t('common.button.stop') }}
        </button>

        <!-- 设置 -->
        <button
          @click.stop="isSettingsOpen = !isSettingsOpen"
          class="w-6 h-6 rounded-[6px] flex items-center justify-center text-[var(--text-secondary)] hover:bg-[var(--bg-hover)] transition-colors"
          :class="{ 'bg-[var(--bg-hover)]': isSettingsOpen }"
          :title="t('desktop.terminal.settings')"
          @mousedown.stop
        >
          <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z" />
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
          </svg>
        </button>

        <!-- 清屏 -->
        <button @click="terminalPreviewRef?.clearTerminal()" class="w-6 h-6 rounded-[6px] flex items-center justify-center text-[var(--text-secondary)] hover:bg-[var(--bg-hover)] transition-colors" :title="t('desktop.terminal.clearScreen')">
          <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
          </svg>
        </button>

        <!-- 刷新格式 -->
        <button @click="terminalPreviewRef?.refreshTerminal()" class="w-6 h-6 rounded-[6px] flex items-center justify-center text-[var(--text-secondary)] hover:bg-[var(--bg-hover)] transition-colors" :title="t('desktop.terminal.refreshFormat')">
          <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
          </svg>
        </button>

        <!-- 插件扩展点 -->
        <PluginTerminalToolbar />
        <PluginTitleBarItems />

        <!-- 分隔线 -->
        <div class="w-px h-4 bg-[var(--border)] mx-0.5"></div>

        <!-- 窗口控制 -->
        <button @click="minimizeWindow" class="w-6 h-6 rounded-[6px] flex items-center justify-center text-[var(--text-secondary)] hover:bg-[var(--bg-hover)] transition-colors" :title="t('desktop.terminal.minimize')">
          <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M20 12H4" />
          </svg>
        </button>
        <button @click="toggleMaximize" class="w-6 h-6 rounded-[6px] flex items-center justify-center text-[var(--text-secondary)] hover:bg-[var(--bg-hover)] transition-colors" :title="t('desktop.terminal.maximize')">
          <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path v-if="!isMaximized" stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M4 8V4m0 0h4M4 4l5 5m11-1V4m0 0h-4m4 0l-5 5M4 16v4m0 0h4m-4 0l5-5m11 5l-5-5m5 5v-4m0 4h4" />
            <path v-else stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M9 9V4.5M9 9H4.5M9 9L3.75 3.75M9 15v4.5M9 15H4.5M9 15l-5 5m5-5l5-5m-5 5v-4.5m0 4.5h4.5" />
          </svg>
        </button>
        <button @click="closeWindow" class="w-6 h-6 rounded-[6px] flex items-center justify-center text-[var(--text-secondary)] hover:bg-[var(--color-danger)] hover:text-white transition-colors" :title="t('desktop.terminal.close')">
          <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M6 18L18 6M6 6l12 12" />
          </svg>
        </button>
      </div>
    </header>

    <!-- 加载态 -->
    <div v-if="isLoading" class="flex-1 flex items-center justify-center">
      <p class="wb-mono text-[calc(12px*var(--ui-scale))] text-[var(--text-secondary)]">{{ t('desktop.terminal.loadingSession') }}</p>
    </div>

    <!-- 终端区：flex-1 占据剩余空间，min-h-0 防止内容撑开容器 -->
    <TerminalPreview v-else ref="terminalPreviewRef" class="flex-1 min-h-0" :session="session" :show-input="true" :show-header="false" />

    <!-- 24px 状态条 -->
    <footer class="h-6 shrink-0 flex items-center justify-between px-3 border-t border-[var(--border)] bg-[var(--bg-card)]">
      <div class="flex items-center gap-2">
        <span :class="['w-1.5 h-1.5 rounded-full', statusColor]"></span>
        <span class="text-[calc(10.5px*var(--ui-scale))] font-semibold tracking-[0.08em] uppercase" :class="statusLabelClass">{{ statusText }}</span>
      </div>
      <div class="flex items-center gap-1.5 wb-mono text-[calc(11px*var(--ui-scale))] text-[var(--text-secondary)]">
        <span class="text-[calc(10.5px*var(--ui-scale))] tracking-[0.08em] text-[var(--text-tertiary)]">{{ t('desktop.server.uptime').toUpperCase() }}</span>
        <span class="text-[var(--text-primary)]">{{ uptimeText }}</span>
      </div>
    </footer>

    <!-- 设置面板遮罩：点击关闭 -->
    <transition name="settings-backdrop">
      <div
        v-if="isSettingsOpen"
        class="absolute inset-0 top-10 z-20 bg-black/25"
        @click="isSettingsOpen = false"
      ></div>
    </transition>

    <!-- 设置面板：从右侧滑出 -->
    <transition name="settings-panel">
      <aside
        v-if="isSettingsOpen"
        class="absolute top-10 right-0 bottom-0 z-30 w-64 flex flex-col bg-[var(--bg-card)] border-l border-[var(--border)] shadow-xl"
      >
        <div class="h-10 shrink-0 px-4 flex items-center justify-between border-b border-[var(--border)]">
          <span class="text-[calc(13px*var(--ui-scale))] font-semibold text-[var(--text-primary)]">{{ t('desktop.terminal.settings') }}</span>
          <button
            class="w-6 h-6 rounded-[6px] flex items-center justify-center text-[var(--text-secondary)] hover:bg-[var(--bg-hover)] transition-colors"
            :title="t('desktop.terminal.close')"
            @click="isSettingsOpen = false"
          >
            <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        </div>

        <div class="flex-1 overflow-y-auto p-4 space-y-5">
          <!-- 终端主题 -->
          <Select
            v-model="settingsTheme"
            :options="themeSelectOptions"
            :label="t('desktop.terminal.theme')"
            size="sm"
            @click.stop
            @mousedown.stop
          />

          <!-- 字体大小 -->
          <Select
            v-model="settingsFontSize"
            :options="fontSizeSelectOptions"
            :label="t('desktop.terminal.fontSize')"
            size="sm"
            @click.stop
            @mousedown.stop
          />

          <!-- 背景图片 -->
          <div>
            <label class="block text-xs font-medium mb-1.5 text-[var(--text-secondary)]">{{ t('desktop.terminal.bgImage') }}</label>
            <div class="flex items-center gap-1.5">
              <button
                @click.stop="pickBgImage"
                class="wb-btn-ghost !h-7 flex-1 justify-center"
              >
                {{ t('desktop.terminal.bgImageSelect') }}
              </button>
              <button
                v-if="hasBgImage"
                @click.stop="removeBgImage"
                class="wb-btn-ghost !h-7 !text-red-600 dark:!text-red-400"
                :title="t('desktop.terminal.bgImageRemove')"
              >
                {{ t('desktop.terminal.bgImageRemove') }}
              </button>
            </div>

            <!-- 当前图片回显：只显示文件名 -->
            <div
              v-if="hasBgImage"
              class="mt-1.5 px-2 py-1 rounded-[6px] bg-[var(--bg-hover)] border border-[var(--border)] text-xs text-[var(--text-secondary)] truncate"
              :title="bgImageName"
            >
              {{ bgImageName }}
            </div>

            <!-- 图片不透明度：实时预览，防抖持久化 -->
            <div v-if="hasBgImage" class="mt-2">
              <div class="flex items-center justify-between text-xs text-[var(--text-secondary)] mb-1">
                <span>{{ t('desktop.terminal.bgImageOpacity') }}</span>
                <span class="wb-mono">{{ settingsBgOpacity }}%</span>
              </div>
              <input
                type="range"
                min="0"
                max="100"
                step="1"
                v-model.number="settingsBgOpacity"
                class="w-full h-1 appearance-none bg-[var(--border-strong)] cursor-pointer accent-[var(--color-primary)]"
                @click.stop
              />
            </div>
          </div>
        </div>
      </aside>
    </transition>
  </div>
</template>

<script setup lang="ts">
/**
 * 终端窗口视图 — 独立终端窗口
 * Warm Workbench 风格：40px 工具条（mono 会话名 + 状态标签 + cwd/命令）+ 24px 状态条；
 * 保留贴靠/显示动画/设置面板（含背景图片）与插件扩展点
 */
import { ref, computed, onMounted, onUnmounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRoute } from 'vue-router'
import { getCurrentWindow, PhysicalPosition } from '@tauri-apps/api/window'
import { invoke } from '@tauri-apps/api/core'
import { emit, listen, type UnlistenFn } from '@tauri-apps/api/event'
import { open } from '@tauri-apps/plugin-dialog'
import { useSettingsStore } from '@/stores/settings'
import { useToast } from '@/composables/useToast'
import TerminalPreview from '@/components/TerminalPreview.vue'
import { Select } from '@/components'
import PluginTerminalToolbar from '@/plugin/components/PluginTerminalToolbar.vue'
import PluginTitleBarItems from '@/plugin/components/PluginTitleBarItems.vue'
import PluginPageToolbar from '@/plugin/components/PluginPageToolbar.vue'
import { useSessionStore } from '@/stores/session'
import { getSessionConfig } from '@/composables/useDesktopCommands'
import type { SessionInfo, SessionConfig } from '@/composables/useTauri'

const { t } = useI18n()
const appWindow = getCurrentWindow()
const settingsStore = useSettingsStore()
const sessionStore = useSessionStore()
const toast = useToast()

const SNAP_THRESHOLD = 15  // 贴靠阈值（像素）

const route = useRoute()
const sessionId = ref(route.params.id as string)
const sessionName = ref('')
const session = ref<SessionInfo | null>(null)
const config = ref<SessionConfig | null>(null)
const isMaximized = ref(false)
const isLoading = ref(true)
const isShown = ref(false)  // 是否已允许显示（由主窗口在内容就绪后通知）
const revealDone = ref(false)  // 进入动画是否已结束（结束后移除残留 transform）
const isSnapped = ref(false)  // 是否已贴靠
const snapDirection = ref<'left' | 'right' | null>(null)  // 贴靠方向
const nowTick = ref(Date.now())
let uptimeTimer: ReturnType<typeof setInterval> | null = null

// TerminalPreview 组件引用，访问暴露的 fontSize/terminalTheme 等
const terminalPreviewRef = ref<InstanceType<typeof TerminalPreview> | null>(null)

// 设置面板是否打开
const isSettingsOpen = ref(false)

const workingDir = computed(() => config.value?.working_dir || config.value?.workingDir || '')
const command = computed(() => config.value?.command || '')

// 设置面板绑定的主题/字体大小（读写 TerminalPreview 暴露的 ref，与终端实时同步）
const settingsTheme = computed({
  get: () => terminalPreviewRef.value?.terminalTheme ?? 'dracula',
  set: (value: string) => {
    if (terminalPreviewRef.value) terminalPreviewRef.value.terminalTheme = value
  },
})

const settingsFontSize = computed({
  get: () => terminalPreviewRef.value?.fontSize ?? 12,
  set: (value: number) => {
    if (terminalPreviewRef.value) terminalPreviewRef.value.fontSize = value
  },
})

const themeOptions = computed(() => terminalPreviewRef.value?.themeNames ?? {})

// 设置面板主题/字号下拉选项（label 由共享 Select 的 label prop 渲染）
const themeSelectOptions = computed(() =>
  Object.entries(themeOptions.value).map(([value, label]) => ({ value, label })),
)
const fontSizeSelectOptions = computed(() =>
  [8, 10, 12, 14, 16, 18, 20].map(size => ({ value: size, label: `${size}px` })),
)

// ==================== 状态展示 ====================

const isLive = computed(() => {
  const s = session.value?.status
  return s === 'running' || s === 'waitingInput' || s === 'starting'
})

const statusColor = computed(() => {
  if (!session.value) return 'bg-[var(--text-tertiary)]'
  switch (session.value.status) {
    case 'running': return 'bg-green-500'
    case 'waitingInput': return 'bg-yellow-500 animate-pulse'
    case 'error': return 'bg-red-500'
    case 'stopped': return 'bg-[var(--text-tertiary)]'
    case 'starting': return 'bg-blue-500 animate-pulse'
    default: return 'bg-[var(--text-tertiary)]'
  }
})

const statusText = computed(() => {
  switch (session.value?.status) {
    case 'starting': return t('common.status.starting')
    case 'running': return t('common.status.running')
    case 'waitingInput': return t('common.status.asking')
    case 'error': return t('common.status.error')
    case 'stopped': return t('common.status.stopped')
    default: return t('common.status.unknown')
  }
})

const statusLabelClass = computed(() => {
  switch (session.value?.status) {
    case 'running': return 'text-green-600 dark:text-green-400'
    case 'waitingInput': return 'text-yellow-600 dark:text-yellow-400'
    case 'error': return 'text-red-600 dark:text-red-400'
    case 'starting': return 'text-blue-500 dark:text-blue-400'
    default: return 'text-[var(--text-tertiary)]'
  }
})

// 运行时长：从 startedAt 起算，每秒刷新
const uptimeText = computed(() => {
  const start = session.value?.startedAt
  if (!start || !isLive.value) return '--:--:--'
  const diff = Math.floor((nowTick.value - new Date(start).getTime()) / 1000)
  if (diff < 0) return '--:--:--'
  const h = Math.floor(diff / 3600)
  const m = Math.floor((diff % 3600) / 60)
  const s = diff % 60
  return `${h}:${m.toString().padStart(2, '0')}:${s.toString().padStart(2, '0')}`
})

// ==================== Background Image ====================

/** 选择背景图片时允许的图片扩展名 */
const BG_IMAGE_EXTENSIONS = ['png', 'jpg', 'jpeg', 'gif', 'webp', 'bmp', 'svg', 'ico']

const hasBgImage = computed(() => !!settingsStore.settings.ui.terminal_bg_image)

/** 当前背景图片名（只回显最后一个路径分隔符后的内容） */
const bgImageName = computed(() => {
  const v = settingsStore.settings.ui.terminal_bg_image
  if (!v) return ''
  return v.split(/[\\/]/).pop() || v
})

// 不透明度滑块：直接变更 store 状态实时预览（TerminalPreview 监听 store），防抖后持久化
const settingsBgOpacity = computed({
  get: () => settingsStore.settings.ui.terminal_bg_opacity ?? 30,
  set: (value: number) => {
    settingsStore.settings.ui.terminal_bg_opacity = value
    scheduleBgSettingsSave()
  },
})

let bgSaveTimeout: ReturnType<typeof setTimeout> | null = null
function scheduleBgSettingsSave() {
  if (bgSaveTimeout) clearTimeout(bgSaveTimeout)
  bgSaveTimeout = setTimeout(() => {
    settingsStore.saveSettings({ ui: { ...settingsStore.settings.ui } })
  }, 300)
}

/** 选择系统图片文件并设为终端背景（复制到应用数据目录，避免原图移动/删除后失效） */
async function pickBgImage() {
  try {
    const selected = await open({
      multiple: false,
      filters: [{ name: t('desktop.terminal.bgImage'), extensions: BG_IMAGE_EXTENSIONS }],
    })
    if (!selected || typeof selected !== 'string') return
    const fileName = await invoke<string | null>('set_terminal_bg_image', { sourcePath: selected })
    if (fileName) {
      // 设置中存原始文件名用于回显，实际复制文件由后端统一命名为 terminal_bg.<ext>
      const displayName = selected.split(/[\\/]/).pop() || fileName
      await settingsStore.saveSettings({
        ui: { ...settingsStore.settings.ui, terminal_bg_image: displayName },
      })
    }
  } catch (e) {
    console.error('[TerminalWindowView] Failed to set background image:', e)
    toast.error(t('desktop.terminal.bgImageSetFailed'))
  }
}

/** 移除终端背景图片 */
async function removeBgImage() {
  try {
    await invoke('set_terminal_bg_image', { sourcePath: null })
    await settingsStore.saveSettings({
      ui: { ...settingsStore.settings.ui, terminal_bg_image: '' },
    })
  } catch (e) {
    console.error('[TerminalWindowView] Failed to remove background image:', e)
    toast.error(t('desktop.terminal.bgImageSetFailed'))
  }
}

// ==================== 窗口逻辑 ====================

// 记录主窗口上一次的位置
let lastMainWindowPos = { x: 0, y: 0, width: 0, height: 0 }
// 记录本窗口上一次的位置
let lastTerminalWindowPos = { x: 0, y: 0 }

let unlistenMainMoved: UnlistenFn | null = null
let unlistenMainResized: UnlistenFn | null = null
let unlistenSnapped: UnlistenFn | null = null
let unlistenShow: UnlistenFn | null = null
let unlistenFocus: UnlistenFn | null = null

async function loadSessionInfo() {
  isLoading.value = true
  try {
    const result = await invoke<SessionInfo>('get_session', { sessionId: sessionId.value })
    session.value = result
    sessionName.value = result.name

    // 只读拉取会话配置，用于工具条展示 cwd / 命令
    if (result.config_id) {
      try {
        config.value = await getSessionConfig(result.config_id)
      } catch (e) {
        console.error('[TerminalWindowView] Failed to load session config:', e)
      }
    }

    // 加载完成后初始化位置
    await initWindowPosition()
  } catch (e) {
    console.error('[TerminalWindow] Failed to load session info:', e)
    sessionName.value = t('desktop.terminal.defaultName')
  } finally {
    isLoading.value = false
    // 通知主窗口内容已就绪，可显示窗口（避免加载闪屏）
    emit('terminal-ready', { sessionId: sessionId.value }).catch(() => {})
  }
}

/** 初始化窗口位置和贴靠检测 */
async function initWindowPosition() {
  const win = appWindow

  // 获取本窗口当前位置
  const pos = await win.outerPosition()
  lastTerminalWindowPos = { x: pos.x, y: pos.y }

  // 监听主窗口移动
  unlistenMainMoved = await listen<{ x: number; y: number; width: number; height: number }>(
    'main-window-moved',
    handleMainWindowMoved
  )

  // 监听主窗口大小变化
  unlistenMainResized = await listen<{ width: number; height: number }>(
    'main-window-resized',
    handleMainWindowResized
  )

  // 监听贴靠状态变化（从主窗口发出）
  unlistenSnapped = await listen<{ sessionId: string; direction: 'left' | 'right' }>(
    'terminal-window-snapped',
    (event) => {
      if (event.payload.sessionId === sessionId.value) {
        isSnapped.value = true
        snapDirection.value = event.payload.direction
      }
    }
  )
}

/** 处理主窗口移动 - 贴靠时同步移动 */
async function handleMainWindowMoved(event: { payload: { x: number; y: number; width: number; height: number } }) {
  const mainPos = event.payload

  // 更新本窗口记录的位置
  lastMainWindowPos = mainPos

  if (!isSnapped.value) {
    // 未贴靠时，检测是否需要贴靠
    await checkAndSnap(mainPos)
    return
  }

  // 已贴靠：跟随主窗口移动
  const win = appWindow
  const terminalPos = await win.outerPosition()
  const terminalSize = await win.outerSize()

  let newX = terminalPos.x

  if (snapDirection.value === 'right') {
    // 贴靠右侧
    newX = mainPos.x + mainPos.width
  } else if (snapDirection.value === 'left') {
    // 贴靠左侧
    newX = mainPos.x - terminalSize.width
  }

  // 计算移动差值
  const dx = newX - terminalPos.x

  // 仅当有实际移动时才更新
  if (dx !== 0) {
    await win.setPosition(new PhysicalPosition(newX, terminalPos.y))
  }

  lastTerminalWindowPos = { x: newX, y: terminalPos.y }
}

/** 处理主窗口大小变化 - 调整贴靠位置 */
async function handleMainWindowResized(event: { payload: { width: number; height: number } }) {
  if (!isSnapped.value) return

  const mainSize = event.payload
  const win = appWindow
  const terminalPos = await win.outerPosition()
  const terminalSize = await win.outerSize()

  let newX = terminalPos.x

  if (snapDirection.value === 'right') {
    newX = mainSize.width + lastMainWindowPos.width - terminalSize.width + lastMainWindowPos.x
  } else if (snapDirection.value === 'left') {
    newX = lastMainWindowPos.x - terminalSize.width
  }

  if (newX !== terminalPos.x) {
    await win.setPosition(new PhysicalPosition(newX, terminalPos.y))
  }
}

/** 检测并执行贴靠 */
async function checkAndSnap(mainPos: { x: number; y: number; width: number; height: number }) {
  const win = appWindow
  const terminalPos = await win.outerPosition()
  const terminalSize = await win.outerSize()

  // 检测右侧贴靠
  const rightDistance = Math.abs((mainPos.x + mainPos.width) - terminalPos.x)
  if (rightDistance < SNAP_THRESHOLD) {
    isSnapped.value = true
    snapDirection.value = 'right'
    await win.setPosition(new PhysicalPosition(mainPos.x + mainPos.width, terminalPos.y))
    return
  }

  // 检测左侧贴靠
  const leftDistance = Math.abs(mainPos.x - (terminalPos.x + terminalSize.width))
  if (leftDistance < SNAP_THRESHOLD) {
    isSnapped.value = true
    snapDirection.value = 'left'
    await win.setPosition(new PhysicalPosition(mainPos.x - terminalSize.width, terminalPos.y))
    return
  }

  // 未贴靠
  isSnapped.value = false
  snapDirection.value = null
}

/** 停止当前会话并关闭窗口 */
async function stopSession() {
  try {
    await sessionStore.killSession(sessionId.value)
    toast.info(t('desktop.session.sessionStopped'))
    await appWindow.close()
  } catch (e) {
    toast.error(t('desktop.session.stopFailed', { error: (e as Error).message }))
  }
}

async function minimizeWindow() {
  const win = appWindow
  await win.minimize()
}

async function toggleMaximize() {
  const win = appWindow
  const maximized = await win.isMaximized()
  if (maximized) {
    await win.unmaximize()
    isMaximized.value = false
  } else {
    await win.maximize()
    isMaximized.value = true
  }
}

async function closeWindow() {
  try {
    await appWindow.close()
  } catch (e) {
    console.error('[TerminalWindowView] Close error:', e)
  }
}

function handleKeydown(e: KeyboardEvent) {
  // Esc 关闭设置面板
  if (e.key === 'Escape' && isSettingsOpen.value) {
    isSettingsOpen.value = false
  }
}

/**
 * 窗口进入动画结束后，切换到无 transform 状态（opacity-100）。
 * 动画 fill-mode:both 会让 transform: translateY(0) 永久残留在根节点，
 * 使包裹 WebGL 画布的外层长期处于独立合成层，WebView2 合成器滚动时可能
 * 缓存旧帧导致重影；动画结束后移除 transform 消除该触发点。
 */
function onRevealEnd(e: AnimationEvent) {
  if (e.animationName === 'fade-slide-up' && isShown.value) {
    revealDone.value = true
  }
}

onMounted(async () => {
  // Esc 关闭设置面板
  window.addEventListener('keydown', handleKeydown)
  // 先注册显示事件监听，再加载会话，避免与主窗口的显示通知产生竞态
  unlistenShow = await listen<{ sessionId: string }>('terminal-show', (event) => {
    if (event.payload.sessionId === sessionId.value) {
      isShown.value = true
    }
  })

  // 兜底：窗口获得焦点时也触发显现动画
  unlistenFocus = await appWindow.onFocusChanged(({ payload: focused }) => {
    if (focused) {
      isShown.value = true
    }
  })

  // 运行时长每秒刷新
  uptimeTimer = setInterval(() => { nowTick.value = Date.now() }, 1000)

  loadSessionInfo()
})

onUnmounted(() => {
  window.removeEventListener('keydown', handleKeydown)
  if (bgSaveTimeout) {
    clearTimeout(bgSaveTimeout)
    bgSaveTimeout = null
  }
  if (uptimeTimer) {
    clearInterval(uptimeTimer)
    uptimeTimer = null
  }
  if (unlistenMainMoved) unlistenMainMoved()
  if (unlistenMainResized) unlistenMainResized()
  if (unlistenSnapped) unlistenSnapped()
  if (unlistenShow) unlistenShow()
  if (unlistenFocus) unlistenFocus()
})
</script>

<style scoped>
/* 设置面板滑出过渡：will-change 提升为独立合成层，避免动画期间页面抖动 */
.settings-panel-enter-active,
.settings-panel-leave-active {
  transition: transform 0.25s ease;
  will-change: transform;
}

.settings-panel-enter-from,
.settings-panel-leave-to {
  transform: translateX(100%);
}

/* 设置面板遮罩淡入淡出 */
.settings-backdrop-enter-active,
.settings-backdrop-leave-active {
  transition: opacity 0.2s ease;
  will-change: opacity;
}
</style>
