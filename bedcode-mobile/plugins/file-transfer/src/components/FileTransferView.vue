<script setup lang="ts">
/**
 * FileTransferView — 文件传输浏览主页面 (Mobile) — host-peer 契约版
 *
 * 结构：
 *   顶栏（对端名+连接状态）→ 面包屑 → 共享根/目录列表（多选勾选）
 *   → 多选时底部「下载到手机」→ 迷你传输条 → 顶栏右侧上传（系统选择器直发）。
 *
 * 发送 = 系统文件选择器（SAF 多选，宿主换算真实路径后 send_files）；
 * 接收 = 浏览对端共享根 → 勾选拉取（pull_files 批量）；
 * 队列/接收应答/历史见 TaskQueueSheet 与 BatchRequestDialog。
 */
import { inject, ref, onMounted, onUnmounted, computed, watch } from 'vue'
import type { PluginContext, Disposable } from '@binblink/plugin-sdk-mobile'
import { useTasks } from '../composables/useTasks'
import { usePeerDevices } from '../composables/usePeerDevices'
import { useRemoteFs } from '../composables/useRemoteFs'
import { useSettings } from '../composables/useSettings'
import { formatBytes, formatSpeed, progressPercent } from '../utils/format'
import FileTypeIcon from './FileTypeIcon.vue'
import TaskQueueSheet from './TaskQueueSheet.vue'
import PeerDevicesSheet from './PeerDevicesSheet.vue'
import BatchRequestDialog from './BatchRequestDialog.vue'

const context = inject<PluginContext>('pluginContext')!
const t = (key: string, params?: Record<string, any>) => context.i18n.t(key, params)

const tasks = useTasks(context)
const devices = usePeerDevices(context)
const fs = useRemoteFs(context)
const settings = useSettings(context)
// 解构 Ref：模板需直接读 approvalTimeoutSec（settings 对象顶层是 settings Ref）
const { settings: transferSettings } = settings

/** 队列 / 附近设备两个 bottom sheet 是否展开 */
const queueOpen = ref(false)
const devicesOpen = ref(false)

/** 设备操作 → 编排 composable 命令路由（fire-and-forget，失败已在内部上报） */
function handleDeviceConnect(nodeId: string): void {
  void devices.connect(nodeId)
}
function handleDeviceDisconnect(nodeId: string): void {
  void devices.disconnect(nodeId)
}
function handleDeviceSetActive(nodeId: string): void {
  void devices.switchPeer(nodeId)
}

/** 探索发现进行中（附近设备 sheet 头部扫描按钮 spinner 态） */
const deviceScanning = ref(false)

/** 探索发现：重新拉取发现快照（query-peer），完成后恢复按钮态 */
async function handleScanDevices(): Promise<void> {
  if (deviceScanning.value) return
  deviceScanning.value = true
  try {
    await devices.refresh()
  } finally {
    deviceScanning.value = false
  }
}

/** 批请求应答（fire-and-forget；批卡消失由 resolved 快照驱动） */
function handleBatchApprove(batchId: string): void {
  tasks.approveBatch(batchId).catch((e: unknown) => {
    console.error(`[File Transfer] approve-batch failed for "${batchId}":`, e)
  })
}
function handleBatchReject(batchId: string): void {
  tasks.rejectBatch(batchId).catch((e: unknown) => {
    console.error(`[File Transfer] reject-batch failed for "${batchId}":`, e)
  })
}

// ==================== 下拉刷新 ====================
const PULL_TRIGGER = 56
const PULL_MAX = 96
const PULL_RESISTANCE = 0.45

const scrollEl = ref<HTMLElement | null>(null)
const pullDistance = ref(0)
const pullState = ref<'idle' | 'pulling' | 'ready' | 'refreshing'>('idle')
const pullingActive = ref(false)

let pullStartY = 0

function onPullStart(e: TouchEvent): void {
  const el = scrollEl.value
  if (!el || el.scrollTop > 0 || fs.loading.value || pullState.value === 'refreshing') return
  pullingActive.value = true
  pullStartY = e.touches[0].clientY
}

function onPullMove(e: TouchEvent): void {
  if (!pullingActive.value) return
  const dy = e.touches[0].clientY - pullStartY
  if (dy <= 0) {
    if (pullDistance.value !== 0) {
      pullDistance.value = 0
      pullState.value = 'idle'
    }
    return
  }
  pullDistance.value = Math.min(dy * PULL_RESISTANCE, PULL_MAX)
  pullState.value = pullDistance.value >= PULL_TRIGGER ? 'ready' : 'pulling'
}

function onPullEnd(): void {
  if (!pullingActive.value) return
  pullingActive.value = false
  if (pullState.value === 'ready') {
    pullState.value = 'refreshing'
    pullDistance.value = PULL_TRIGGER
    void fs.refresh().finally(() => {
      pullState.value = 'idle'
      pullDistance.value = 0
    })
  } else {
    pullState.value = 'idle'
    pullDistance.value = 0
  }
}

let disposeBackPress: Disposable | null = null

/** 对端展示名：优先活跃对端名（三态真相），回落旧任务快照/占位链 */
const peerLabel = computed(() => devices.peer.value.name || tasks.displayPeerName.value)

const peerStatusLabel = computed(() =>
  tasks.connOnline.value ? t('transfer.peer.online') : t('transfer.peer.offline'),
)

const peerStatusClass = computed(() =>
  tasks.connOnline.value ? 'ft-peer-status--online' : 'ft-peer-status--offline',
)

type EmptyKind = 'empty' | 'notSharing' | 'error'

const emptyKind = computed<EmptyKind>(() => {
  if (fs.error.value) return 'error'
  if (!tasks.peerOnline.value && !fs.currentRoot.value) return 'notSharing'
  return 'empty'
})

const emptyTitle = computed(() => {
  if (fs.notice.value) return t('transfer.notice.storageAccessTitle')
  switch (emptyKind.value) {
    case 'error': return t('transfer.table.dirUnavailable')
    case 'notSharing': return t('transfer.peer.notSharing')
    default: return t('transfer.table.empty')
  }
})

const emptyHint = computed(() => {
  if (fs.notice.value) return t('transfer.notice.storageAccess')
  switch (emptyKind.value) {
    case 'error': return t('transfer.empty.unavailableHint')
    case 'notSharing': return t('transfer.empty.notSharingHint')
    default: return t('transfer.empty.emptyDirHint')
  }
})

const emptyIcoClass = computed(() => {
  switch (emptyKind.value) {
    case 'error': return 'ft-empty-ico--error'
    case 'notSharing': return 'ft-empty-ico--warning'
    default: return 'ft-empty-ico--neutral'
  }
})

const emptyIcoPath = computed(() => {
  switch (emptyKind.value) {
    case 'error':
      return 'M12 8v4m0 4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z'
    case 'notSharing':
      return 'M4.72 4.39a9.5 9.5 0 0114.56 0M7.82 5.52a6.5 6.5 0 018.36 0M10.8 7.21a3.5 3.5 0 012.4 0M12 20h.01'
    default:
      return 'M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z'
  }
})

const emptyCta = computed(() => {
  if (emptyKind.value === 'empty') {
    return {
      label: t('transfer.topbar.refresh'),
      primary: false,
      icon: 'M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15',
      action: () => fs.refresh(),
    }
  }
  return {
    label: t('transfer.topbar.queryPeer'),
    primary: true,
    icon: 'M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15',
    action: () => tasks.queryPeer(),
  }
})

const primaryTask = computed(() => tasks.primaryTask.value)

const downloadLabel = computed(() =>
  t('transfer.topbar.downloadSelected', {
    count: fs.selectedCount.value,
    size: formatBytes(fs.selectedTotalSize.value, t),
  }),
)

/** 点击行：目录进入（根清单层=进入共享根），文件勾选 */
function onRowTap(entry: { name: string; isDir: boolean }): void {
  if (!fs.currentRoot.value && entry.isDir) {
    void fs.cd(entry.name)
    return
  }
  if (entry.isDir) {
    void fs.cd(entry.name)
  } else {
    fs.toggle(entry.name)
  }
}

/** 批量拉取勾选文件（当前根内相对路径拼装，一次 pull_files 调用） */
async function downloadSelected(): Promise<void> {
  if (!tasks.peerOnline.value || !fs.currentRoot.value) return
  const names = fs.selectedFiles.value
  if (names.length === 0) return
  try {
    const result = await context.commands.execute('file-transfer.pull-files', {
      dirId: fs.currentRoot.value.id,
      path: fs.currentPath.value,
      files: names,
    })
    if ((result?.count ?? 0) > 0) {
      context.dialogs.showToast(t('transfer.saveTo.enqueued'), 'success')
      fs.clearSelection()
    }
  } catch (e) {
    console.error('[File Transfer] pull-files failed:', e)
    context.dialogs.showToast(String(e), 'error')
  }
}

/**
 * 上传入口：系统文件选择器多选 → 直发活跃对端
 * （宿主 SAF 选图换算真实路径；取消静默返回）
 */
async function uploadFile(): Promise<void> {
  if (!tasks.peerOnline.value) {
    context.dialogs.showToast(t('transfer.upload.offline'), 'error')
    return
  }
  try {
    const raw = await context.commands.execute('file-transfer.pick-files', {})
    const paths: string[] = Array.isArray(raw) ? raw : []
    if (paths.length === 0) return
    const ok = await tasks.sendFiles(paths)
    if (ok > 0) {
      context.dialogs.showToast(t('transfer.upload.enqueued'), 'success')
    }
  } catch (e) {
    const msg = String(e)
    if (msg.includes('cancel')) return
    console.error('[File Transfer] pick/send failed:', e)
  }
}

/** 对端就绪状态变化驱动目录加载（上线加载共享根，下线清空） */
watch(
  () => tasks.peerOnline.value,
  (online) => {
    if (online) {
      void fs.loadRoots()
    } else {
      fs.reset()
    }
  },
)

onMounted(() => {
  tasks.start()
  devices.start()
  disposeBackPress = context.ui.onBackPressed(({ canGoBack }) => {
    // 返回键接管顺序：最上层弹层先关（附近设备 sheet → 队列 sheet）→ 退目录
    if (devicesOpen.value) {
      devicesOpen.value = false
      return
    }
    if (queueOpen.value) {
      queueOpen.value = false
      return
    }
    if (fs.crumbs.value.length > 1) {
      void fs.up()
      return
    }
    if (fs.crumbs.value.length === 1) {
      void fs.goRoot()
      return
    }
    if (canGoBack) history.back()
  })
  // 挂载晚于设备事件时主动探测一次
  if (!tasks.peerOnline.value) void tasks.queryPeer()
  void fs.loadRoots()
})

onUnmounted(() => {
  disposeBackPress?.dispose()
  devices.stop()
  tasks.stop()
})
</script>

<template>
  <div class="ft-view h-full flex flex-col bg-[var(--mobile-bg-primary)]">
    <!-- v2：批量传输请求全局弹窗（排队 + 倒计时超时默认拒绝；批 resolved 自动切换下一批） -->
    <BatchRequestDialog
      :batches="tasks.batches.value"
      :approval-timeout-sec="transferSettings.approvalTimeoutSec"
      @approve="handleBatchApprove"
      @reject="handleBatchReject"
    />

    <!-- 顶栏：对端名（可点开附近设备 sheet）+ 连接状态 + 右上操作（上传 / 设置） -->
    <div class="flex-shrink-0 flex items-center gap-2 px-4 pt-2.5 pb-2">
      <!-- 对端名区域：整块可点（44px 触控目标），chevron 提示可展开设备面板 -->
      <button
        class="ft-peer-trigger min-w-0 max-w-[55%] flex items-center gap-1"
        @click="devicesOpen = true"
      >
        <span class="ft-peer-name min-w-0 text-[var(--mobile-text-primary)] truncate">
          {{ peerLabel }}
        </span>
        <svg class="w-3.5 h-3.5 flex-shrink-0 text-[var(--mobile-text-muted)]" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 9l-7 7-7-7" />
        </svg>
      </button>
      <!-- 状态胶囊紧跟对端名（连接态 success tint 底 / 断开态中性底） -->
      <span class="ft-peer-status flex-shrink-0" :class="peerStatusClass">
        {{ peerStatusLabel }}
      </span>
      <!-- 弹性空隙：把操作按钮推到行尾 -->
      <div class="flex-1 min-w-2"></div>
      <!-- 操作按钮（主动发起连接 / 上传 / 设置）：纯图标，置于顶栏右侧，避免悬浮于列表数据之上造成遮挡 -->
      <button
        class="ft-topbar-btn ft-topbar-btn-primary flex-shrink-0"
        :title="t('transfer.topbar.connectDevice')"
        @click="devicesOpen = true"
      >
        <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M17 20h5v-2a3 3 0 00-5.356-1.857M17 20H7m10 0v-2c0-.656-.126-1.283-.356-1.857M7 20H2v-2a3 3 0 015.356-1.857M7 20v-2c0-.656.126-1.283.356-1.857m0 0a5.002 5.002 0 019.288 0M15 7a3 3 0 11-6 0 3 3 0 016 0zm6 3a2 2 0 11-4 0 2 2 0 014 0zM7 10a2 2 0 11-4 0 2 2 0 014 0z" />
        </svg>
      </button>
      <button
        class="ft-topbar-btn ft-topbar-btn-primary flex-shrink-0"
        :title="t('transfer.topbar.uploadFile')"
        @click="uploadFile()"
      >
        <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-8l-4-4m0 0L8 8m4-4v12" />
        </svg>
      </button>
      <button
        class="ft-topbar-btn flex-shrink-0"
        :title="t('transfer.topbar.settings')"
        @click="context.ui.openPage('settings')"
      >
        <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path
            stroke-linecap="round"
            stroke-linejoin="round"
            stroke-width="2"
            d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z"
          />
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
        </svg>
      </button>
    </div>

    <!-- 面包屑 -->
    <div class="flex-shrink-0 flex items-center gap-1 px-4 py-2 overflow-x-auto">
      <button
        class="ft-breadcrumb-item flex-shrink-0"
        @click="fs.goRoot()"
      >
        {{ t('transfer.breadcrumb.home') }}
      </button>
      <template v-for="(seg, i) in fs.crumbs.value" :key="i">
        <svg class="w-3.5 h-3.5 flex-shrink-0 text-[var(--mobile-text-disabled)]" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5l7 7-7 7" />
        </svg>
        <button
          class="ft-breadcrumb-item flex-shrink-0 max-w-[7rem] truncate"
          :class="{ 'ft-breadcrumb-current': i === fs.crumbs.value.length - 1 }"
          @click="fs.goTo(i)"
        >
          {{ seg }}
        </button>
      </template>
    </div>

    <!-- 文件列表：滚动容器（空态/加载态在可视区内垂直居中，列表态顶部对齐） -->
    <div
      ref="scrollEl"
      class="flex-1 overflow-y-auto min-h-0 overscroll-behavior-none"
      @touchstart.passive="onPullStart"
      @touchmove.passive="onPullMove"
      @touchend="onPullEnd"
      @touchcancel="onPullEnd"
    >
      <!-- 下拉刷新指示器：下拉时随位移露出；刷新中常驻直至加载完成 -->
      <div
        class="ft-pull"
        :class="{ 'ft-pull-anim': !pullingActive }"
        :style="{ height: pullDistance + 'px' }"
      >
        <span v-if="pullState === 'refreshing'" class="ft-spinner ft-pull-spinner"></span>
        <svg
          v-else
          class="ft-pull-arrow"
          :class="{ 'ft-pull-arrow--ready': pullState === 'ready' }"
          fill="none"
          stroke="currentColor"
          viewBox="0 0 24 24"
        >
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 14l-7 7m0 0l-7-7m7 7V3" />
        </svg>
        <span class="ft-pull-text">
          {{ pullState === 'ready' ? t('transfer.pull.ready') : pullState === 'refreshing' ? t('transfer.pull.refreshing') : t('transfer.pull.pull') }}
        </span>
      </div>

      <!-- min-h-full + flex-col：空态/加载态在可视区内垂直居中，列表态保持顶部对齐 -->
      <div class="px-4 min-h-full flex flex-col">
        <!-- 加载态：扁平细线 spinner + 文案 -->
        <div v-if="fs.loading.value" class="ft-empty-state">
          <span class="ft-spinner"></span>
          <p class="ft-body-text text-[var(--mobile-text-muted)] mt-2">{{ t('transfer.table.loading') }}</p>
        </div>

        <!-- 空态（按场景区分：空目录 / 未连接 / 对端未共享 / 目录不可用） -->
        <div v-else-if="fs.entries.value.length === 0" class="ft-empty-state">
          <div class="ft-empty-ico" :class="emptyIcoClass">
            <svg fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" :d="emptyIcoPath" />
            </svg>
          </div>
          <p class="ft-empty-title">{{ emptyTitle }}</p>
          <p class="ft-empty-hint">{{ emptyHint }}</p>
          <button
            class="ft-touch-btn ft-empty-cta"
            :class="emptyCta.primary ? 'ft-empty-cta--primary' : 'ft-empty-cta--neutral'"
            @click="emptyCta.action()"
          >
            <svg class="w-4 h-4 flex-shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" :d="emptyCta.icon" />
            </svg>
            {{ emptyCta.label }}
          </button>
        </div>

        <!-- 目录/文件行：复用宿主 group-card / group-row 视觉语言 -->
        <div v-else class="group-card">
          <button
            v-for="(entry, idx) in fs.entries.value"
            :key="entry.name"
            class="group-row group-row-btn"
            :class="{
              'ft-row-last': idx === fs.entries.value.length - 1,
              'ft-row-selected': !entry.isDir && fs.selected.value.has(entry.name),
            }"
            @click="onRowTap(entry)"
          >
            <!-- 类型图标（按扩展名匹配：音乐/视频/图片/PDF/文档等，未知回退通用文件） -->
            <FileTypeIcon :name="entry.name" :is-dir="entry.isDir" />

            <!-- 名称 + 元信息（目录无元信息行，仅文件显示大小） -->
            <div class="flex-1 min-w-0">
              <p class="group-row-title truncate">{{ entry.name }}</p>
              <p v-if="!entry.isDir" class="group-row-sub mt-0.5 truncate">
                {{ formatBytes(entry.size, t) }}
              </p>
            </div>

            <!-- 勾选框（仅文件） / 箭头（目录） -->
            <span
              v-if="!entry.isDir"
              class="ft-checkbox flex-shrink-0"
              :class="{ 'ft-checkbox-checked': fs.selected.value.has(entry.name) }"
            >
              <svg v-if="fs.selected.value.has(entry.name)" class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="3" d="M5 13l4 4L19 7" />
              </svg>
            </span>
            <svg v-else class="w-4 h-4 flex-shrink-0" style="color: var(--mobile-row-sub)" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5l7 7-7 7" />
            </svg>
          </button>
        </div>
      </div>
    </div>

    <!-- 底部：多选操作条 + 迷你传输条 -->
    <div class="flex-shrink-0 border-t border-[var(--mobile-border)] bg-[var(--mobile-bg-secondary)]/90 backdrop-blur-xl">
      <!-- 多选操作条 -->
      <div v-if="fs.selectedCount.value > 0" class="flex items-center gap-3 px-4 py-2.5">
        <button
          class="flex-shrink-0 ft-touch-btn px-3 rounded-lg ft-btn-neutral text-[var(--mobile-text-secondary)] active:opacity-80"
          :style="{ fontSize: 'clamp(0.75rem, 0.8125rem + (100vw - 360px) / 800, 0.875rem)' }"
          @click="fs.clearSelection()"
        >
          {{ t('transfer.table.clearSelection') }}
        </button>
        <button
          class="flex-1 ft-touch-btn px-3 rounded-xl text-[var(--mobile-text-on-accent)] bg-[var(--mobile-accent)] active:opacity-80 transition-opacity flex items-center justify-center gap-1.5"
          :style="{ fontSize: 'clamp(0.75rem, 0.8125rem + (100vw - 360px) / 800, 0.875rem)' }"
          @click="downloadSelected()"
        >
          <svg class="w-4 h-4 flex-shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4" />
          </svg>
          <span class="truncate">{{ downloadLabel }}</span>
        </button>
      </div>

      <!-- 迷你传输条：活跃任务显示进度；空闲时显示队列入口（常驻，保证队列始终可达） -->
      <button
        v-if="primaryTask"
        class="w-full flex items-center gap-3 px-4 py-2.5 active:bg-[var(--mobile-bg-tertiary)] transition-colors"
        @click="queueOpen = true"
      >
        <svg class="w-5 h-5 flex-shrink-0 text-[var(--mobile-accent)]" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13 5l7 7-7 7M5 5l7 7-7 7" />
        </svg>
        <div class="flex-1 min-w-0">
          <div class="flex items-center justify-between gap-2">
            <p class="ft-mini-text text-[var(--mobile-text-primary)] truncate">
              {{ primaryTask.remotePath.split('/').pop() }}
            </p>
            <span class="ft-mini-text text-[var(--mobile-text-muted)] flex-shrink-0">
              {{ formatSpeed(tasks.totalSpeed.value, t) }}
            </span>
          </div>
          <div class="mt-1 h-1 rounded-full bg-[var(--mobile-bg-tertiary)] overflow-hidden">
            <div
              class="h-full rounded-full bg-[var(--mobile-accent)] transition-all duration-300"
              :style="{ width: (progressPercent(primaryTask.offset, primaryTask.size) ?? 0) + '%' }"
            ></div>
          </div>
        </div>
        <svg class="w-4 h-4 flex-shrink-0 text-[var(--mobile-text-disabled)]" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5l7 7-7 7" />
        </svg>
      </button>

      <!-- 空闲态队列入口：无活跃任务时保持可进入传输队列（含历史/暂停/失败任务） -->
      <button
        v-else
        class="w-full flex items-center gap-3 px-4 py-2.5 active:bg-[var(--mobile-bg-tertiary)] transition-colors"
        @click="queueOpen = true"
      >
        <svg class="w-5 h-5 flex-shrink-0 text-[var(--mobile-accent)]" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 7h16M4 12h16M4 17h10" />
        </svg>
        <span class="flex-1 min-w-0 text-left truncate ft-mini-text text-[var(--mobile-text-primary)]">
          {{ tasks.tasks.value.length > 0 ? t('transfer.queue.entry', { count: tasks.tasks.value.length }) : t('transfer.minibar.noActive') }}
        </span>
        <svg class="w-4 h-4 flex-shrink-0 text-[var(--mobile-text-disabled)]" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5l7 7-7 7" />
        </svg>
      </button>
    </div>

    <!-- 附近设备 bottom sheet（三态列表 + 探索发现重扫 + 连接/断开/切换活跃对端） -->
    <PeerDevicesSheet
      :open="devicesOpen"
      :rows="devices.rows.value"
      :scanning="deviceScanning"
      :t="t"
      @close="devicesOpen = false"
      @connect="handleDeviceConnect"
      @disconnect="handleDeviceDisconnect"
      @set-active="handleDeviceSetActive"
      @scan="handleScanDevices"
    />

    <!-- 队列 bottom sheet（四 tab：发送/接收/历史） -->
    <TaskQueueSheet
      :open="queueOpen"
      :tasks="tasks.tasks.value"
      :receiving="tasks.receivingTasks.value"
      :history="tasks.history.value"
      :total-speed="tasks.totalSpeed.value"
      :t="t"
      @close="queueOpen = false"
      @cancel="(id) => tasks.cancel(id)"
      @retry="(id) => tasks.retry(id)"
      @cancel-receiving="(sessionId) => tasks.cancelReceiving(sessionId)"
      @clear-history="() => tasks.clearHistory()"
    />
  </div>
</template>

<style scoped>
/* 对端名称触控区：44px 最小触控目标，按压反馈底色；名称流式字号 */
.ft-peer-trigger {
  min-height: 2.75rem;
  border-radius: 0.5rem;
  transition: background-color 0.15s ease;
  -webkit-tap-highlight-color: transparent;
}

.ft-peer-trigger:active {
  background: var(--mobile-bg-tertiary);
}

.ft-peer-name {
  font-size: clamp(0.875rem, 0.9375rem + (100vw - 360px) / 800, 1rem);
  font-weight: 500;
}

/* 顶栏连接状态胶囊：小号 pill，底色 tint 随状态（与插件 ft-chip 同语言） */
.ft-peer-status {
  display: inline-flex;
  align-items: center;
  height: 1.25rem;
  padding: 0 0.5rem;
  border-radius: 9999px;
  font-size: clamp(0.625rem, 0.6875rem + (100vw - 360px) / 800, 0.75rem);
  font-weight: 500;
}

/* 顶栏连接状态胶囊语义色（连接态 success tint / 断开态中性） */
.ft-peer-status--online {
  background: var(--mobile-success-muted);
  border: 1px solid var(--mobile-success-connected-border);
  color: var(--mobile-success);
}
.ft-peer-status--offline {
  background: var(--mobile-bg-tertiary);
  border: 1px solid var(--mobile-border);
  color: var(--mobile-text-secondary);
}

/* 面包屑文字 */
.ft-breadcrumb-item {
  font-size: clamp(0.75rem, 0.8125rem + (100vw - 360px) / 800, 0.875rem);
  color: var(--mobile-accent);
}

.ft-breadcrumb-current {
  color: var(--mobile-text-primary);
  font-weight: 500;
}

/* 文件列表正文 */
.ft-body-text {
  font-size: clamp(0.8125rem, 0.875rem + (100vw - 360px) / 800, 0.9375rem);
}

/* 迷你传输条文字（数字等宽对齐） */
.ft-mini-text {
  font-size: clamp(0.75rem, 0.8125rem + (100vw - 360px) / 800, 0.875rem);
  font-variant-numeric: tabular-nums;
}

/* 多选勾选框：只过渡受影响的属性（frontend-styles：禁止 blanket transition-all） */
.ft-checkbox {
  width: 1.375rem;
  height: 1.375rem;
  border-radius: 9999px;
  border: 2px solid var(--mobile-border-hover);
  display: flex;
  align-items: center;
  justify-content: center;
  transition: border-color 0.15s ease, background-color 0.15s ease;
}

.ft-checkbox-checked {
  background: var(--mobile-accent);
  border-color: var(--mobile-accent);
  color: var(--mobile-text-on-accent);
}

/* 多选勾选行底色：accent 8% tint（与勾选框同色系）；按压反馈 :active 优先级更高，不冲突 */
.ft-row-selected {
  background: color-mix(in srgb, var(--mobile-accent) 8%, transparent);
}

/* ==================== 下拉刷新 ==================== */
/* 指示器：贴滚动容器顶部，高度随下拉位移露出（内容整体下移，与原生下拉同语义） */
.ft-pull {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 0.5rem;
  overflow: hidden;
  color: var(--mobile-text-secondary);
}

/* 回弹过渡：仅在不跟手（手指抬起后 / 刷新结束）时启用 */
.ft-pull-anim {
  transition: height 0.25s cubic-bezier(0.4, 0, 0.2, 1);
}

.ft-pull-arrow {
  width: 1.25rem;
  height: 1.25rem;
  flex-shrink: 0;
  transition: transform 0.2s ease;
}

/* 达阈值：箭头翻转提示「释放立即刷新」 */
.ft-pull-arrow--ready {
  transform: rotate(180deg);
  color: var(--mobile-accent);
}

.ft-pull-text {
  font-size: clamp(0.75rem, 0.8125rem + (100vw - 360px) / 800, 0.875rem);
}

/* 刷新中小号 spinner（复用全局 .ft-spinner 圆环，缩小尺寸） */
.ft-pull-spinner {
  width: 1.125rem;
  height: 1.125rem;
  border-width: 2px;
  flex-shrink: 0;
}

/* 顶栏操作按钮（上传 / 设置）：纯图标（无圆形底），置于行尾。
   44px 触控目标，按压时仅底色反馈（透明 → 中性底） */
.ft-topbar-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 2.75rem;
  height: 2.75rem;
  border-radius: 0.625rem;
  color: var(--mobile-text-secondary);
  transition: background-color 0.15s ease, color 0.15s ease;
  -webkit-tap-highlight-color: transparent;
}

.ft-topbar-btn:active {
  background: var(--mobile-bg-tertiary);
  color: var(--mobile-text-primary);
}

/* 主操作（上传）：图标用品牌色，保留可发现性 */
.ft-topbar-btn-primary {
  color: var(--mobile-accent);
}

.ft-topbar-btn-primary:active {
  color: var(--mobile-accent);
}
</style>
