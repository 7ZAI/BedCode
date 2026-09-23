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
          <span
            :class="['w-2 h-2 rounded-full shrink-0', statusColor]"
            data-tauri-drag-region
          ></span>
          <span
            class="wb-mono text-[calc(13px*var(--ui-scale))] font-semibold text-[var(--text-primary)] truncate"
            data-tauri-drag-region
          >
            {{ sessionName }}
          </span>
          <span
            class="text-[calc(10.5px*var(--ui-scale))] font-semibold tracking-[0.08em] uppercase shrink-0"
            :class="statusLabelClass"
            data-tauri-drag-region
          >
            {{ statusText }}
          </span>
        </div>

        <!-- 会话信息：cwd / 命令（mono 小字） -->
        <div
          v-if="config"
          class="hidden sm:flex items-center gap-2 min-w-0 wb-mono text-[calc(12.5px*var(--ui-scale))] text-[var(--text-secondary)]"
          data-tauri-drag-region
        >
          <span v-if="workingDir" class="truncate max-w-64" :title="workingDir">{{
            workingDir
          }}</span>
          <span v-if="workingDir && command" class="text-[var(--text-tertiary)]">·</span>
          <span v-if="command" class="truncate max-w-48" :title="command">{{ command }}</span>
        </div>
      </div>

      <div class="flex items-center gap-1.5">
        <!-- 插件页面工具栏项（target=terminal；宿主 registry 响应式数组注入） -->
        <template v-if="pageToolbarItems.length > 0">
          <div class="w-px h-4 bg-[var(--border)] mx-0.5"></div>
          <button
            v-for="item in pageToolbarItems"
            :key="`${item.pluginId}:${item.id}`"
            class="wb-btn-ghost"
            :title="item.label"
            @click="item.onClick?.()"
          >
            <span v-if="item.icon" class="w-3.5 h-3.5 plugin-icon">{{ item.icon }}</span>
            <span v-else class="text-xs">{{ item.label }}</span>
          </button>
        </template>

        <!-- 停止会话 -->
        <button
          class="wb-btn-primary !h-6 !px-2.5 !text-[calc(11px*var(--ui-scale))] uppercase"
          @click="stopSession"
        >
          {{ t('session.button.stop') }}
        </button>

        <!-- 设置 -->
        <button
          class="w-6 h-6 rounded-[6px] flex items-center justify-center text-[var(--text-secondary)] hover:bg-[var(--bg-hover)] transition-colors"
          :class="{ 'bg-[var(--bg-hover)]': isSettingsOpen }"
          :title="t('session.terminal.settings')"
          @click.stop="isSettingsOpen = !isSettingsOpen"
          @mousedown.stop
        >
          <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path
              stroke-linecap="round"
              stroke-linejoin="round"
              stroke-width="1.5"
              d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z"
            />
            <path
              stroke-linecap="round"
              stroke-linejoin="round"
              stroke-width="1.5"
              d="M15 12a3 3 0 11-6 0 3 3 0 016 0z"
            />
          </svg>
        </button>

        <!-- 清屏 -->
        <button
          class="w-6 h-6 rounded-[6px] flex items-center justify-center text-[var(--text-secondary)] hover:bg-[var(--bg-hover)] transition-colors"
          :title="t('session.terminal.clearScreen')"
          @click="terminalPreviewRef?.clearTerminal()"
        >
          <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path
              stroke-linecap="round"
              stroke-linejoin="round"
              stroke-width="1.5"
              d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16"
            />
          </svg>
        </button>

        <!-- 刷新格式 -->
        <button
          class="w-6 h-6 rounded-[6px] flex items-center justify-center text-[var(--text-secondary)] hover:bg-[var(--bg-hover)] transition-colors"
          :title="t('session.terminal.refreshFormat')"
          @click="terminalPreviewRef?.refreshTerminal()"
        >
          <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path
              stroke-linecap="round"
              stroke-linejoin="round"
              stroke-width="1.5"
              d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15"
            />
          </svg>
        </button>

        <!-- 插件扩展点（终端工具条 / 标题栏项；宿主 registry 注入） -->
        <template v-if="terminalToolbarItems.length > 0">
          <div class="w-px h-4 bg-slate-200 dark:bg-dark-600 mx-1"></div>
          <button
            v-for="item in terminalToolbarItems"
            :key="`${item.pluginId}:${item.id}`"
            class="wb-btn-ghost !h-6"
            :title="item.label"
            @click="item.onClick?.()"
          >
            <span v-if="item.icon" class="w-4 h-4 plugin-icon">{{ item.icon }}</span>
            <span v-else class="text-xs">{{ item.label }}</span>
          </button>
        </template>
        <div
          v-if="titleBarItems.length > 0"
          class="flex items-center gap-2 px-2"
          style="-webkit-app-region: no-drag"
        >
          <button
            v-for="item in titleBarItems"
            :key="`${item.pluginId}:${item.id}`"
            class="flex items-center gap-1 px-1.5 py-0.5 text-xs text-slate-500 dark:text-dark-400 hover:text-slate-700 dark:hover:text-dark-200 hover:bg-slate-100 dark:hover:bg-dark-700 rounded transition-colors"
            :title="item.label"
            @click="item.onClick?.()"
          >
            <span v-if="item.icon" class="plugin-icon">{{ item.icon }}</span>
            <span>{{ item.label }}</span>
          </button>
        </div>

        <!-- 分隔线 -->
        <div class="w-px h-4 bg-[var(--border)] mx-0.5"></div>

        <!-- 窗口控制 -->
        <button
          class="w-6 h-6 rounded-[6px] flex items-center justify-center text-[var(--text-secondary)] hover:bg-[var(--bg-hover)] transition-colors"
          :title="t('session.terminal.minimize')"
          @click="minimizeWindow"
        >
          <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M20 12H4" />
          </svg>
        </button>
        <button
          class="w-6 h-6 rounded-[6px] flex items-center justify-center text-[var(--text-secondary)] hover:bg-[var(--bg-hover)] transition-colors"
          :title="t('session.terminal.maximize')"
          @click="toggleMaximize"
        >
          <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path
              v-if="!isMaximized"
              stroke-linecap="round"
              stroke-linejoin="round"
              stroke-width="1.5"
              d="M4 8V4m0 0h4M4 4l5 5m11-1V4m0 0h-4m4 0l-5 5M4 16v4m0 0h4m-4 0l5-5m11 5l-5-5m5 5v-4m0 4h4"
            />
            <path
              v-else
              stroke-linecap="round"
              stroke-linejoin="round"
              stroke-width="1.5"
              d="M9 9V4.5M9 9H4.5M9 9L3.75 3.75M9 15v4.5M9 15H4.5M9 15l-5 5m5-5l5-5m-5 5v-4.5m0 4.5h4.5"
            />
          </svg>
        </button>
        <button
          class="w-6 h-6 rounded-[6px] flex items-center justify-center text-[var(--text-secondary)] hover:bg-[var(--color-danger)] hover:text-white transition-colors"
          :title="t('session.terminal.close')"
          @click="closeWindow"
        >
          <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path
              stroke-linecap="round"
              stroke-linejoin="round"
              stroke-width="1.5"
              d="M6 18L18 6M6 6l12 12"
            />
          </svg>
        </button>
      </div>
    </header>

    <!-- 加载态 -->
    <div v-if="isLoading" class="flex-1 flex items-center justify-center">
      <p class="wb-mono text-[calc(12px*var(--ui-scale))] text-[var(--text-secondary)]">
        {{ t('session.terminal.loadingSession') }}
      </p>
    </div>

    <!-- 终端区：flex-1 占据剩余空间，min-h-0 防止内容撑开容器 -->
    <TerminalPreview
      v-else
      ref="terminalPreviewRef"
      class="flex-1 min-h-0"
      :session="session"
      :show-input="true"
      :show-header="false"
    />

    <!-- 24px 状态条 -->
    <footer
      class="h-6 shrink-0 flex items-center justify-between px-3 border-t border-[var(--border)] bg-[var(--bg-card)]"
    >
      <div class="flex items-center gap-2">
        <span :class="['w-1.5 h-1.5 rounded-full', statusColor]"></span>
        <span
          class="text-[calc(10.5px*var(--ui-scale))] font-semibold tracking-[0.08em] uppercase"
          :class="statusLabelClass"
          >{{ statusText }}</span
        >
      </div>
      <div
        class="flex items-center gap-1.5 wb-mono text-[calc(11px*var(--ui-scale))] text-[var(--text-secondary)]"
      >
        <span
          class="text-[calc(10.5px*var(--ui-scale))] tracking-[0.08em] text-[var(--text-tertiary)]"
          >{{ t('session.terminal.uptime').toUpperCase() }}</span
        >
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
        <div
          class="h-10 shrink-0 px-4 flex items-center justify-between border-b border-[var(--border)]"
        >
          <span
            class="text-[calc(13px*var(--ui-scale))] font-semibold text-[var(--text-primary)]"
            >{{ t('session.terminal.settings') }}</span
          >
          <button
            class="w-6 h-6 rounded-[6px] flex items-center justify-center text-[var(--text-secondary)] hover:bg-[var(--bg-hover)] transition-colors"
            :title="t('session.terminal.close')"
            @click="isSettingsOpen = false"
          >
            <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path
                stroke-linecap="round"
                stroke-linejoin="round"
                stroke-width="1.5"
                d="M6 18L18 6M6 6l12 12"
              />
            </svg>
          </button>
        </div>

        <div class="flex-1 overflow-y-auto p-4 space-y-5">
          <!-- 终端主题 -->
          <div>
            <span
              class="block mb-1.5 text-[calc(11px*var(--ui-scale))] uppercase tracking-wider text-[var(--text-tertiary)]"
              >{{ t('session.terminal.theme') }}</span
            >
            <select
              class="w-full h-8 rounded-[6px] px-2 cursor-pointer text-[calc(12px*var(--ui-scale))] bg-[var(--bg-card)] border border-[var(--border)] text-[var(--text-primary)] focus:outline-none focus:border-brand"
              :value="settingsTheme"
              @change="settingsTheme = ($event.target as HTMLSelectElement).value"
              @click.stop
              @mousedown.stop
            >
              <option v-for="opt in themeSelectOptions" :key="opt.value" :value="opt.value">
                {{ opt.label }}
              </option>
            </select>
          </div>

          <!-- 字体大小 -->
          <div>
            <span
              class="block mb-1.5 text-[calc(11px*var(--ui-scale))] uppercase tracking-wider text-[var(--text-tertiary)]"
              >{{ t('session.terminal.fontSize') }}</span
            >
            <select
              class="w-full h-8 rounded-[6px] px-2 cursor-pointer text-[calc(12px*var(--ui-scale))] bg-[var(--bg-card)] border border-[var(--border)] text-[var(--text-primary)] focus:outline-none focus:border-brand"
              :value="settingsFontSize"
              @change="settingsFontSize = Number(($event.target as HTMLSelectElement).value)"
              @click.stop
              @mousedown.stop
            >
              <option v-for="opt in fontSizeSelectOptions" :key="opt.value" :value="opt.value">
                {{ opt.label }}
              </option>
            </select>
          </div>

          <!-- 背景图片 -->
          <div class="space-y-3">
            <div>
              <span
                class="block mb-1.5 text-[calc(11px*var(--ui-scale))] uppercase tracking-wider text-[var(--text-tertiary)]"
                >{{ t('session.terminal.bgImage') }}</span
              >
              <div class="flex items-center gap-2">
                <button
                  class="wb-btn-ghost !h-7 !px-2.5 text-[calc(11px*var(--ui-scale))]"
                  @click="pickBgImage"
                  @mousedown.stop
                >
                  {{ t('session.terminal.bgImageSelect') }}
                </button>
                <button
                  v-if="hasBgImage"
                  class="wb-btn-ghost !h-7 !px-2.5 text-[calc(11px*var(--ui-scale))]"
                  @click="removeBgImage"
                  @mousedown.stop
                >
                  {{ t('session.terminal.bgImageRemove') }}
                </button>
              </div>
              <span v-if="bgImageName" class="block mt-1 text-[calc(11px*var(--ui-scale))] text-[var(--text-secondary)] truncate">
                {{ bgImageName }}
              </span>
            </div>

            <div>
              <div class="flex items-center justify-between mb-1">
                <span
                  class="text-[calc(11px*var(--ui-scale))] uppercase tracking-wider text-[var(--text-tertiary)]"
                  >{{ t('session.terminal.bgImageOpacity') }}</span
                >
                <span class="wb-mono">{{ settingsBgOpacity }}%</span>
              </div>
              <input
                v-model.number="settingsBgOpacity"
                type="range"
                min="0"
                max="100"
                step="1"
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
 * 终端窗口视图（插件版，票 03a）— 独立终端窗口壳
 *
 * 自宿主 `TerminalWindowView.vue` 迁入：40px 工具条（mono 会话名 + 状态标签 +
 * cwd/命令 + 插件扩展点 + 窗口控制）+ 24px 状态条 + 设置面板（含背景图片）；
 * 保留贴靠/显示动画/设置面板与插件扩展点。
 *
 * 与宿主版本差异（方案 1 迁入适配）：
 * - 会话数据经 `context.session.get()`（宿主面向插件会话 API）+ 插件命令通道
 *   `session.config.list`（插件私有库真源）——不直调宿主领域命令；
 * - 终端设置/背景图经宿主能力注入（`terminalHostCapabilities`：settings accessor +
 *   bg image 命令桥；无注入环境回退内存版）；
 * - 插件扩展点（页面工具栏/终端工具栏/标题栏项）经宿主注入的 registry 响应式
 *   数组复刻渲染（宿主 Plugin*Toolbar 组件的等效按钮）；
 * - 窗口几何/贴靠/就绪事件与关闭由宿主 `context.session.openTerminal` 原语持有，
 *   本视图只做窗口内交互（贴靠跟随/最小化/最大化/关闭）——逻辑逐字迁入。
 */
import { ref, computed, watch, onMounted, onUnmounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { toast } from 'vue-sonner'
import { getCurrentWindow, PhysicalPosition } from '@tauri-apps/api/window'
import { emit, listen, type UnlistenFn } from '@tauri-apps/api/event'
import { inject } from 'vue'
import type { PluginContext } from '@binblink/bedcode-plugin-sdk-desktop'
import TerminalPreview from '../../components/terminal/TerminalPreview.vue'
import { currentSessionId } from '../../utils/route'
import {
  useTerminalHostCapabilities,
  createFallbackHostCapabilities,
  TERMINAL_PAGE_TOOLBAR_TARGET,
} from '../../components/terminal/terminalHostCapabilities'
import type { SessionInfo } from '../../composables/terminal/model'
import type { SessionConfigDto } from '../../composables/useSessionCenter'
import { TERMINAL_THEME_NAMES } from '../../utils/terminal/terminalThemes'

const { t } = useI18n()
const appWindow = getCurrentWindow()

// 插件上下文（PluginViewHost / dev-shell 注入）
const context = inject<PluginContext>('pluginContext')!
// 终端宿主能力（TerminalWindowHostView 注入；dev-shell/vitest 回退内存版）
const caps = useTerminalHostCapabilities() ?? createFallbackHostCapabilities()

const SNAP_THRESHOLD = 15 // 贴靠阈值（像素）

const sessionId = currentSessionId()
const sessionName = ref('')
const session = ref<SessionInfo | null>(null)
const config = ref<SessionConfigDto | null>(null)
const isMaximized = ref(false)
const isLoading = ref(true)
const isShown = ref(false) // 是否已允许显示（由主窗口在内容就绪后通知）
const revealDone = ref(false) // 进入动画是否已结束（结束后移除残留 transform）
const isSnapped = ref(false) // 是否已贴靠
const snapDirection = ref<'left' | 'right' | null>(null) // 贴靠方向
const nowTick = ref(Date.now())
let uptimeTimer: ReturnType<typeof setInterval> | null = null

// TerminalPreview 组件引用，访问暴露的 fontSize/terminalTheme 等
const terminalPreviewRef = ref<InstanceType<typeof TerminalPreview> | null>(null)

// 设置面板是否打开
const isSettingsOpen = ref(false)

const workingDir = computed(() => config.value?.workingDir || '')
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

// 设置面板主题/字号下拉选项（label 由共享 Select 的 label prop 渲染）
const themeSelectOptions = computed(() => Object.entries(TERMINAL_THEME_NAMES).map(([value, label]) => ({ value, label })))
const fontSizeSelectOptions = computed(() => {
  const sizes: number[] = []
  for (let size = 8; size <= 20; size++) {
    sizes.push(size)
  }
  return sizes.map((size) => ({ value: size, label: `${size}px` }))
})

// ==================== 插件扩展点（宿主 registry 注入，复刻渲染） ====================

const pageToolbarItems = computed(() =>
  caps.extensions.pageToolbarItems.value.filter((i) => i.id === TERMINAL_PAGE_TOOLBAR_TARGET),
)
const terminalToolbarItems = computed(() => caps.extensions.terminalToolbarItems.value)
const titleBarItems = computed(() => caps.extensions.titleBarItems.value)

// ==================== 状态展示 ====================

const isLive = computed(() => {
  const s = session.value?.status
  return s === 'running' || s === 'waitingInput' || s === 'starting'
})

const statusColor = computed(() => {
  if (!session.value) return 'bg-[var(--text-tertiary)]'
  switch (session.value.status) {
    case 'running':
      return 'bg-green-500'
    case 'waitingInput':
      return 'bg-yellow-500 animate-pulse'
    case 'error':
      return 'bg-red-500'
    case 'stopped':
      return 'bg-[var(--text-tertiary)]'
    case 'starting':
      return 'bg-blue-500 animate-pulse'
    default:
      return 'bg-[var(--text-tertiary)]'
  }
})

const statusText = computed(() => {
  switch (session.value?.status) {
    case 'starting':
      return t('session.status.starting')
    case 'running':
      return t('session.status.running')
    case 'waitingInput':
      return t('session.status.asking')
    case 'error':
      return t('session.status.error')
    case 'stopped':
      return t('session.status.stopped')
    default:
      return t('session.status.unknown')
  }
})

const statusLabelClass = computed(() => {
  switch (session.value?.status) {
    case 'running':
      return 'text-green-600 dark:text-green-400'
    case 'waitingInput':
      return 'text-yellow-600 dark:text-yellow-400'
    case 'error':
      return 'text-red-600 dark:text-red-400'
    case 'starting':
      return 'text-blue-500 dark:text-blue-400'
    default:
      return 'text-[var(--text-tertiary)]'
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

// ==================== 背景图片（经宿主能力桥） ====================

const hasBgImage = computed(() => caps.bgImage.hasImage)
/** 当前背景图片名（只回显最后路径分隔符后的内容） */
const bgImageName = computed(() => caps.bgImage.imageName)

// 不透明度滑块：本地 ref 实时预览（TerminalPreview 经 accessor 外部同步 watch
// 跟随），防抖后经 accessor 持久化（宿主桥 save 落 settingsStore）
const settingsBgOpacity = ref<number>(caps.settings.getBgOpacity() ?? 30)
let bgSaveTimeout: ReturnType<typeof setTimeout> | null = null
function scheduleBgSettingsSave() {
  if (bgSaveTimeout) clearTimeout(bgSaveTimeout)
  bgSaveTimeout = setTimeout(() => {
    caps.settings.save({ bgOpacity: settingsBgOpacity.value })
  }, 300)
}
watch(settingsBgOpacity, () => scheduleBgSettingsSave())

/** 选择系统图片文件并设为终端背景（宿主桥：dialog + set_terminal_bg_image 命令） */
async function pickBgImage() {
  const ok = await caps.bgImage.pickAndSet()
  if (!ok) return
  // 设置面板内的不透明度保持（背景图切换不重置滑块）
}

/** 移除终端背景图片 */
async function removeBgImage() {
  await caps.bgImage.remove()
}

// ==================== 会话加载 ====================

async function loadSessionInfo() {
  isLoading.value = true
  try {
    const result = await context.session.get(sessionId)
    session.value = result as SessionInfo
    sessionName.value = (result as SessionInfo).name

    // 只读拉取会话配置（插件私有库真源，经 session.config.list），用于工具条展示 cwd / 命令
    const configId = (result as SessionInfo).configId ?? (result as SessionInfo).config_id
    if (configId) {
      try {
        const raw = await context.commands.execute('session.config.list', {})
        const configs = Array.isArray(raw) ? (raw as SessionConfigDto[]) : []
        config.value = configs.find((c) => c.id === configId) ?? null
      } catch (e) {
        console.warn('[terminal-session] Failed to load session config:', e)
      }
    }

    // 加载完成后初始化位置
    await initWindowPosition()
  } catch (e) {
    console.error('[terminal-session] Failed to load session info:', e)
    sessionName.value = t('session.terminal.defaultName')
  } finally {
    isLoading.value = false
    // 通知主窗口内容已就绪，可显示窗口（避免加载闪屏）
    emit('terminal-ready', { sessionId }).catch(() => {})
  }
}

// ==================== 窗口逻辑 ====================

/** 初始化窗口位置和贴靠检测 */
async function initWindowPosition() {
  // 监听主窗口移动
  unlistenMainMoved = await listen<{ x: number; y: number; width: number; height: number }>(
    'main-window-moved',
    handleMainWindowMoved,
  )

  // 监听主窗口大小变化
  unlistenMainResized = await listen<{ width: number; height: number }>(
    'main-window-resized',
    handleMainWindowResized,
  )

  // 监听贴靠状态变化（从主窗口发出）
  unlistenSnapped = await listen<{ sessionId: string; direction: 'left' | 'right' }>(
    'terminal-window-snapped',
    (event) => {
      if (event.payload.sessionId === sessionId) {
        isSnapped.value = true
        snapDirection.value = event.payload.direction
      }
    },
  )
}

// 记录主窗口上一次的位置
let lastMainWindowPos = { x: 0, y: 0, width: 0, height: 0 }

let unlistenMainMoved: UnlistenFn | null = null
let unlistenMainResized: UnlistenFn | null = null
let unlistenSnapped: UnlistenFn | null = null
let unlistenShow: UnlistenFn | null = null
let unlistenFocus: UnlistenFn | null = null

/** 处理主窗口移动 - 贴靠时同步移动 */
async function handleMainWindowMoved(event: {
  payload: { x: number; y: number; width: number; height: number }
}) {
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
  const rightDistance = Math.abs(mainPos.x + mainPos.width - terminalPos.x)
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

/** 停止当前会话并关闭窗口（停止编排归本插件命令面 `session.close`） */
async function stopSession() {
  try {
    await context.commands.execute('session.close', { sessionId })
    await context.session.closeTerminal(sessionId)
    toast.info(t('session.terminal.stopped'))
    await appWindow.close()
  } catch (e) {
    toast.error(t('session.terminal.stopFailed', { error: (e as Error).message }))
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
    console.error('[terminal-session] Close error:', e)
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
    if (event.payload.sessionId === sessionId) {
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
  uptimeTimer = setInterval(() => {
    nowTick.value = Date.now()
  }, 1000)

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

/* 插件扩展点图标（页面工具栏 / 终端工具栏 / 标题栏项）
 *
 * 宿主三个 Plugin*Toolbar 组件各自 scoped 定义 .plugin-icon —— scoped 样式不外泄，
 * 本视图按宿主原样复刻这三个扩展点，故必须自带同款规则，否则图标丢失字号/行高归一化
 * （与宿主同位置图标不一致）。取值与 src/plugin/components/PluginPageToolbar.vue 一致。 */
.plugin-icon {
  font-size: calc(14px * var(--ui-scale));
  line-height: 1;
}
</style>
