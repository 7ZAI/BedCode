<script setup lang="ts">
/**
 * FileTransferView — 文件传输主页面（三段式布局）
 *
 * 旧版架构：浏览为主页 + 传输队列/附近设备收进 bottom sheet。
 * 当前架构：传输 / 浏览 / 设备三段式主内容区，传输列表**直接铺出来**（默认页），
 *         底部行动条按「多选 / 活跃传输 / 上传」三态互斥收敛唯一入口。
 * 领域逻辑（useTasks / usePeerDevices / useRemoteFs / useSettings）完全复用，
 * 本文件只做编排与布局。
 */
import { inject, ref, onMounted, onUnmounted, computed, watch } from 'vue'
import type { PluginContext, Disposable } from '@binblink/bedcode-plugin-sdk-mobile'
import { useSwipeTabs } from '@binblink/bedcode-plugin-sdk-mobile/ui/swipe-tabs'
import { useTasks } from '../composables/useTasks'
import { usePeerDevices } from '../composables/usePeerDevices'
import { useRemoteFs } from '../composables/useRemoteFs'
import { useSettings } from '../composables/useSettings'
import { formatBytes, formatSpeed, progressPercent } from '../utils/format'
import { isTerminalState } from '../types'
import type { MainTab, FooterMode } from '../types'
import PeerHeader from './PeerHeader.vue'
import TabBar from './TabBar.vue'
import TransfersTab from './TransfersTab.vue'
import BrowseTab from './BrowseTab.vue'
import DevicesTab from './DevicesTab.vue'
import SummaryBar from './SummaryBar.vue'
import BatchRequestDialog from './BatchRequestDialog.vue'

const context = inject<PluginContext>('pluginContext')!
const t = (key: string, params?: Record<string, any>) => context.i18n.t(key, params)

const tasks = useTasks(context)
const devices = usePeerDevices(context)
const fs = useRemoteFs(context)
const settings = useSettings(context)
const { settings: transferSettings } = settings

/** 主分段（传输列表默认可见——传输是第一目标） */
const tab = ref<MainTab>('transfers')

/** 可发送性：自建设备缓存中存在具备传输能力的节点（与旧版同构） */
watch(
  () => devices.devices.value,
  (list) => {
    tasks.peerOnline.value = list.some((d) => d.fileTransfer !== false && !!d.nodeId)
  },
  { deep: false, immediate: true },
)

/** 左右滑切换主分段（内容区手势，不与浏览 tab 内列表滚动冲突） */
function switchTab(dir: 'left' | 'right'): void {
  const order: MainTab[] = ['transfers', 'browse', 'devices']
  const next = order.indexOf(tab.value) + (dir === 'left' ? 1 : -1)
  if (next >= 0 && next < order.length) tab.value = order[next]!
}
const { onTouchStart, onTouchMove, onTouchEnd } = useSwipeTabs(switchTab)

// dev-shell / 桌面预览回退：鼠标指针拖动驱动同一切换逻辑（浏览器无触摸事件）。
// 真机触摸走上方 touch 事件；此处按 pointerType 只认 mouse，避免双触发。
let mouseStartX = 0
let mouseStartY = 0
let mouseDX = 0
let mouseTracking = false
function onMouseDown(e: PointerEvent): void {
  if (e.pointerType !== 'mouse') return
  mouseStartX = e.clientX
  mouseStartY = e.clientY
  mouseDX = 0
  mouseTracking = true
}
function onMouseMove(e: PointerEvent): void {
  if (!mouseTracking || e.pointerType !== 'mouse') return
  const dx = e.clientX - mouseStartX
  const dy = e.clientY - mouseStartY
  mouseDX = Math.abs(dx) > Math.abs(dy) ? dx : 0
}
function onMouseUp(): void {
  if (!mouseTracking) return
  mouseTracking = false
  if (mouseDX < -48) switchTab('left')
  else if (mouseDX > 48) switchTab('right')
  mouseDX = 0
}

// ==================== 对端 / 设备 ====================

/** 对端展示名：优先活跃对端名（三态真相），回落旧任务快照/占位链 */
const peerLabel = computed(() => devices.peer.value.name || tasks.displayPeerName.value)

/** 探索发现进行中（设备 tab 头部扫描按钮 spinner 态） */
const deviceScanning = ref(false)

async function handleScanDevices(): Promise<void> {
  if (deviceScanning.value) return
  deviceScanning.value = true
  try {
    await devices.refresh()
  } finally {
    deviceScanning.value = false
  }
}

function handleDeviceConnect(nodeId: string): void {
  void devices.connect(nodeId)
}
function handleDeviceDisconnect(nodeId: string): void {
  void devices.disconnect(nodeId)
}
function handleDeviceSetActive(nodeId: string): void {
  void devices.switchPeer(nodeId)
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

/** 历史「打开所在文件夹」：优先本地路径（wire 有 localPath，dev-shell 演示链路），
 * 否则真机凭文件名经宿主 MediaStore 按名解析（接收落点不在 wire 上） */
async function handleOpenLocation(id: string): Promise<void> {
  const entry = tasks.history.value.find((e) => e.id === id)
  if (!entry) return
  try {
    if (entry.localPath) await context.system.revealInDir(entry.localPath)
    else await context.system.revealReceivedFileLocation(entry.fileName)
  } catch (e) {
    console.error('[File Transfer] open location failed:', e)
    context.dialogs.showToast(t('transfer.v2.history.noLocalFile'), 'error')
  }
}

// ==================== 浏览（选择 / 下载） ====================

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
      tab.value = 'transfers'
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
    if (online) void fs.loadRoots()
    else fs.reset()
  },
)

// ==================== 底部行动条 ====================

/** 接收侧终态（与 TransfersTab 口径一致；ReceivingTask.state 为宿主透传字符串） */
const RECEIVING_TERMINAL = new Set(['completed', 'failed', 'rejected', 'cancelled'])

/** 活跃传输（发送队列 + 接收中；底栏「总体」与角标口径包含双向） */
const activeTasks = computed(() => [
  ...tasks.tasks.value.filter((tk) => !isTerminalState(tk.state)),
  ...tasks.receivingTasks.value.filter((rt) => !RECEIVING_TERMINAL.has(rt.state)),
])

/** 总体进度：各活跃批已传字节 / 总字节 */
const overallPercent = computed(() => {
  const list = activeTasks.value
  const total = list.reduce((sum, tk) => sum + (tk.size > 0 ? tk.size : 0), 0)
  if (total <= 0) return 0
  const done = list.reduce((sum, tk) => sum + Math.min(tk.offset, tk.size), 0)
  return Math.min(100, Math.round((done / total) * 100))
})

/** 底栏三态：多选 > 活跃传输 > 上传 CTA */
const footerMode = computed<FooterMode>(() => {
  if (tab.value === 'browse' && fs.selectedCount.value > 0) return 'select'
  if (activeTasks.value.length > 0) return 'active'
  return 'upload'
})

/** 队列残留条数（上传态提示：终态但仍可清理/重试的任务） */
const queuedCount = computed(() => tasks.tasks.value.length)

const downloadLabel = computed(() =>
  t('transfer.topbar.downloadSelected', {
    count: fs.selectedCount.value,
    size: formatBytes(fs.selectedTotalSize.value, t),
  }),
)

const activeLabel = computed(() => {
  const count = activeTasks.value.length
  return count > 0 ? t('transfer.queue.active', { count }) : t('transfer.queue.title')
})

// ==================== 生命周期 ====================

let disposeBackPress: Disposable | null = null

onMounted(() => {
  tasks.start()
  devices.start()
  void settings.load()
  disposeBackPress = context.ui.onBackPressed(({ canGoBack }) => {
    // 返回键接管顺序：浏览 tab 逐级退目录 → 回到传输 tab → 交还宿主
    if (tab.value === 'browse' && fs.crumbs.value.length > 1) {
      void fs.up()
      return
    }
    if (tab.value === 'browse' && fs.crumbs.value.length === 1) {
      void fs.goRoot()
      return
    }
    if (tab.value !== 'transfers') {
      tab.value = 'transfers'
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
  <div class="fv2-view mobile-ui">
    <!-- v2 批量传输请求全局弹窗（排队 + 倒计时超时默认拒绝） -->
    <BatchRequestDialog
      :batches="tasks.batches.value"
      :approval-timeout-sec="transferSettings.approvalTimeoutSec"
      @approve="handleBatchApprove"
      @reject="handleBatchReject"
    />

    <!-- 顶栏：活跃对端 + 连接状态 + 设置入口 -->
    <PeerHeader
      :peer-name="peerLabel"
      :online="devices.peer.value.online"
      :online-label="t('transfer.peer.online')"
      :offline-label="t('transfer.peer.offline')"
      :upload-label="t('transfer.v2.upload.cta')"
      :upload-disabled="!tasks.peerOnline.value"
      @peer-tap="tab = 'devices'"
      @upload="uploadFile()"
      @settings="context.ui.openPage('settings')"
    />

    <!-- 主分段（传输 / 浏览 / 设备） -->
    <TabBar
      v-model="tab"
      :transfers="t('transfer.v2.tab.transfers')"
      :browse="t('transfer.v2.tab.browse')"
      :devices="t('transfer.v2.tab.devices')"
      :active-count="activeTasks.length"
      :device-count="devices.rows.value.length"
    />

    <!-- 主内容区（左右滑切换分段）：data-swipe-zone 声明横滑区供宿主仲裁边界，
         区在首尾边界时交还外层翻页（MobileSwipeContainer 协议，与 auto-task 同构） -->
    <div
      class="flex-1 min-h-0 flex flex-col fv2-swipe-zone"
      data-swipe-zone
      :data-zone-at-start="tab === 'transfers'"
      :data-zone-at-end="tab === 'devices'"
      @touchstart.passive="onTouchStart"
      @touchmove.passive="onTouchMove"
      @touchend="onTouchEnd"
      @touchcancel="onTouchEnd"
      @pointerdown="onMouseDown"
      @pointermove="onMouseMove"
      @pointerup="onMouseUp"
      @pointercancel="onMouseUp"
    >
      <Transition name="fv2-fade" mode="out-in">
        <!-- 传输列表（默认，直接铺出） -->
        <TransfersTab
          v-if="tab === 'transfers'"
          key="transfers"
          :tasks="tasks.tasks.value"
          :receiving="tasks.receivingTasks.value"
          :history="tasks.history.value"
          :t="t"
          @cancel="(id) => tasks.cancel(id)"
          @retry="(id) => tasks.retry(id)"
          @cancel-receiving="(sessionId) => tasks.cancelReceiving(sessionId)"
          @clear-history="() => tasks.clearHistory()"
          @open-location="handleOpenLocation"
        />

        <!-- 浏览对端共享目录 -->
        <BrowseTab
          v-else-if="tab === 'browse'"
          key="browse"
          :entries="fs.entries.value"
          :loading="fs.loading.value"
          :error="fs.error.value"
          :notice="fs.notice.value"
          :crumbs="fs.crumbs.value"
          :current-root="fs.currentRoot.value"
          :selected="fs.selected.value"
          :selected-count="fs.selectedCount.value"
          :peer-online="tasks.peerOnline.value"
          :t="t"
          @cd="(name) => fs.cd(name)"
          @up="() => fs.up()"
          @go-root="() => fs.goRoot()"
          @go-to="(i) => fs.goTo(i)"
          @toggle="(name) => fs.toggle(name)"
          @toggle-all="() => fs.toggleAll()"
          @clear-selection="() => fs.clearSelection()"
          @refresh="() => fs.refresh()"
          @query-peer="() => tasks.queryPeer()"
        />

        <!-- 附近设备 -->
        <DevicesTab
          v-else
          key="devices"
          :rows="devices.rows.value"
          :scanning="deviceScanning"
          :t="t"
          @connect="handleDeviceConnect"
          @disconnect="handleDeviceDisconnect"
          @set-active="handleDeviceSetActive"
          @scan="handleScanDevices"
        />
      </Transition>
    </div>

    <!-- 底部行动条（多选 / 活跃传输 / 上传 CTA 三态互斥） -->
    <SummaryBar
      :mode="footerMode"
      :upload-label="t('transfer.v2.upload.cta')"
      :queue-hint="queuedCount > 0 ? t('transfer.v2.queue.hint', { count: queuedCount }) : ''"
      :download-label="downloadLabel"
      :clear-label="t('transfer.table.clearSelection')"
      :active-label="activeLabel"
      :speed-label="formatSpeed(tasks.totalSpeed.value, t)"
      :overall-label="t('transfer.v2.active.overall', { percent: overallPercent })"
      :overall-percent="overallPercent"
      :view-queue-label="t('transfer.v2.active.viewQueue')"
      :offline="!tasks.peerOnline.value"
      @upload="uploadFile()"
      @download="downloadSelected()"
      @clear-selection="() => fs.clearSelection()"
      @open-queue="tab = 'transfers'"
    />
  </div>
</template>
