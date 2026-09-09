<script setup lang="ts">
/**
 * FileTransferView — 文件传输双栏工作台（host-peer 契约版）
 *
 * 顶栏（对端 pill + 发送/下载所选/刷新/设置）+ 左栏 RemoteFileTable
 * （对端共享根 → 根内目录两级浏览）+ 右栏 TaskPanel（批级队列）。
 * 发送 = 系统选择器多选直发；接收 = 浏览勾选拉取；设备列表由
 * devices-changed 事件驱动。
 */
import { ref, computed, watch, inject, onMounted, onUnmounted } from 'vue'
import type { PluginContext } from '@binblink/bedcode-plugin-sdk-desktop'
import RemoteFileTable from './RemoteFileTable.vue'
import TaskPanel from './TaskPanel.vue'
import SettingsPanel from './SettingsPanel.vue'
import BatchRequestDialog from './BatchRequestDialog.vue'
import ConsentDialog from './ConsentDialog.vue'
import PeerDevicesPanel from './PeerDevicesPanel.vue'
import { useTasks } from '../composables/useTasks'
import { useReceiving } from '../composables/useReceiving'
import { useRemoteFs } from '../composables/useRemoteFs'
import { useSettings } from '../composables/useSettings'
import { usePeerDevices } from '../composables/usePeerDevices'

const context = inject<PluginContext>('pluginContext')!
const t = (key: string, params?: Record<string, any>) => context.i18n.t(key, params)

const {
  rows: deviceRows,
  peers,
  peer,
  connectedIds,
  connect: connectDevice,
  disconnect: disconnectDevice,
  switchPeer,
  refresh: refreshDevices,
  start: startPeer,
  stop: stopPeer,
} = usePeerDevices(context)
const {
  tasks,
  totalSpeed,
  refresh: refreshTasks,
  sendPickedFiles,
  cancel,
  retry,
  start: startTasks,
  stop: stopTasks,
} = useTasks(context)
const {
  batches,
  receiving,
  history,
  toasts,
  approveBatch,
  rejectBatch,
  cancelReceiving,
  clearHistory: clearHistoryEntries,
  dismissToast,
  start: startReceiving,
  stop: stopReceiving,
} = useReceiving(context)
const {
  settings,
  rootItems,
  hasRoots,
  load: loadSettings,
  addRoot,
  removeRoot,
  pickDownloadDir,
  setReceivingPolicy,
  setApprovalTimeoutSec,
  setEncryption,
} = useSettings(context)
/** 插件自身对等连接：存在任一已建立的对等连接（≠ 宿主主连接） */
const peerConnected = computed(() => connectedIds.value.size > 0)

const fs = useRemoteFs(context, () => peer.value.id)

const showSettings = ref(false)

/** 传输队列面板是否展开 */
const queueVisible = ref(false)

/** 对端显示名 */
const peerDisplayName = computed(() => {
  if (peer.value.name) return peer.value.name
  const withName = tasks.value.find((x) => x.peer?.name)
  if (withName?.peer?.name) return withName.peer.name
  if (peer.value.id) return peer.value.id
  if (peerConnected.value) return '—'
  return t('transfer.peer.unpaired')
})

const selectedCount = computed(() => fs.selectedNames.value.length)

/** 对端名映射（peerId → 展示名，批卡/接收任务展示用） */
const peerNames = computed<Record<string, string>>(() => {
  const map: Record<string, string> = {}
  for (const p of peers.value) map[p.id] = p.name || p.id
  for (const tk of tasks.value) {
    if (tk.peer?.deviceId && tk.peer.name && !map[tk.peer.deviceId]) {
      map[tk.peer.deviceId] = tk.peer.name
    }
  }
  return map
})

/** 批请求应答（fire-and-forget） */
function handleBatchApprove(batchId: string): void {
  approveBatch(batchId).catch((e: unknown) => {
    console.error(`[File Transfer] approve-batch failed for "${batchId}":`, e)
  })
}
function handleBatchReject(batchId: string): void {
  rejectBatch(batchId).catch((e: unknown) => {
    console.error(`[File Transfer] reject-batch failed for "${batchId}":`, e)
  })
}

const peerStatusLabel = computed(() => {
  if (!peerConnected.value) return t('transfer.peer.offline')
  if (!peer.value.online) return t('transfer.peer.notSharing')
  return t('transfer.peer.online')
})

/** 主下载按钮可用性：当前处于共享根内且有勾选 */
const canDownload = computed(() => fs.currentRoot.value !== null && selectedCount.value > 0)

/** 空态分支优先级：共享目录 → 对端 */
const showNoRoots = computed(() => !hasRoots.value)
const showNoPeer = computed(() => !peer.value.online)
const noPeerLabel = computed(() =>
  peerConnected.value ? t('transfer.peer.notSharing') : t('transfer.empty.noPeer'),
)

/** 拉取所选文件（一次 pull_files 批调用）；成功后展开队列面板 */
async function handleDownload(): Promise<void> {
  if (!canDownload.value || !fs.currentRoot.value) return
  try {
    await context.commands.execute('file-transfer.pull-files', {
      dirId: fs.currentRoot.value.id,
      // 当前所在根内相对路径：嵌套目录勾选下载必须带上，否则对端按根目录
      // 解析文件名 → not-found（根清单层不可勾选，relPath 恒有 currentRoot 伴生）
      path: fs.relPath.value,
      files: fs.selectedNames.value,
    })
    fs.clearSelection()
    queueVisible.value = true
  } catch (e) {
    console.error('[File Transfer] pull-files failed:', e)
  }
}

/** 顶栏刷新：任务 + 当前目录层级 + 设备缓存清扫 */
async function handleRefresh(): Promise<void> {
  await Promise.all([refreshTasks(), fs.refresh(), refreshDevices()])
}

/** 发送到手机：系统多文件选择器直发活跃对端 */
async function handleUpload(): Promise<void> {
  if (!peer.value.online) return
  const ok = await sendPickedFiles()
  if (ok > 0) queueVisible.value = true
}

/** 下载完成 → 打开本地所在目录（system.revealInDir） */
async function handleOpenFolder(path: string): Promise<void> {
  try {
    await context.system.revealInDir(path)
  } catch (e) {
    console.error('[File Transfer] open folder failed:', e)
  }
}

/** 附近设备面板开合（点击外部关闭，见 onMounted 文档监听） */
const devPanelOpen = ref(false)
const devPanelWrap = ref<HTMLElement | null>(null)

/** 已连接设备数（顶栏入口角标语义：可互传的设备数） */
const connectedCount = computed(
  () => deviceRows.value.filter((r) => r.status === 'connected').length,
)

function handleConnectDevice(nodeId: string): void {
  void connectDevice(nodeId)
}

function handleDisconnectDevice(nodeId: string): void {
  void disconnectDevice(nodeId)
}

/** 探索发现进行中（面板扫描按钮 spinner 态） */
const deviceScanning = ref(false)

/** 探索发现：重新拉取发现快照（query-peer），完成后恢复按钮态 */
async function handleScanDevices(): Promise<void> {
  if (deviceScanning.value) return
  deviceScanning.value = true
  try {
    await refreshDevices()
  } finally {
    deviceScanning.value = false
  }
}

async function handleSetActiveDevice(nodeId: string): Promise<void> {
  await switchPeer(nodeId)
}

/** 点击面板外部时收起（capture 阶段拦截，避免先触发内部点击） */
function handleDocClick(e: MouseEvent): void {
  if (!devPanelOpen.value) return
  if (devPanelWrap.value && !devPanelWrap.value.contains(e.target as Node)) {
    devPanelOpen.value = false
  }
}

onMounted(() => {
  document.addEventListener('click', handleDocClick, true)
})

onUnmounted(() => {
  document.removeEventListener('click', handleDocClick, true)
})

onMounted(async () => {
  startPeer()
  startTasks()
  startReceiving()
  await Promise.all([loadSettings(), refreshTasks()])
})

onUnmounted(() => {
  document.removeEventListener('click', handleDocClick, true)
  stopPeer()
  stopTasks()
  stopReceiving()
})

/** 激活设备变化驱动目录加载/清空 */
watch(
  () => peer.value.id,
  (id) => {
    if (id) {
      void fs.loadRoots()
    } else {
      fs.reset()
    }
  },
)
</script>

<template>
  <div class="ft-view">
    <!-- v2：批量传输请求全局弹窗（排队 + 倒计时超时默认拒绝；批 resolved 自动切换下一批） -->
    <BatchRequestDialog
      :batches="batches"
      :approval-timeout-sec="settings.approvalTimeoutSec"
      @approve="handleBatchApprove"
      @reject="handleBatchReject"
    />

    <!-- 首连确认弹窗：编排常驻于插件激活期（useConsent），此处仅渲染当前待确认项；
         不在面板时经状态栏项跳转过来后即可见可操作 -->
    <ConsentDialog />

    <!-- 顶栏 -->
    <div class="ft-topbar">
      <div class="ft-peer-pill">
        <span
          class="ft-dot"
          :class="
            peerConnected ? (peer.online ? 'ft-dot--online' : 'ft-dot--partial') : 'ft-dot--offline'
          "
        ></span>
        <span class="ft-peer-name">{{ peerDisplayName }}</span>
        <span class="ft-peer-meta">
          {{ peerStatusLabel }}
        </span>
      </div>
      <!-- 附近设备面板：三态连接管理（自绘，禁原生 select；点击外部收起） -->
      <div ref="devPanelWrap" class="ft-peer-switch-wrap">
        <button
          class="ft-btn ft-peer-switch-btn"
          :class="{ 'ft-peer-switch-btn--open': devPanelOpen }"
          :title="t('transfer.peer.switchTitle')"
          @click="devPanelOpen = !devPanelOpen"
        >
          <svg
            class="w-3.5 h-3.5 flex-shrink-0"
            fill="none"
            stroke="currentColor"
            viewBox="0 0 24 24"
          >
            <path
              stroke-linecap="round"
              stroke-linejoin="round"
              stroke-width="2"
              d="M17 20h5v-2a3 3 0 00-5.356-1.857M17 20H7m10 0v-2c0-.656-.126-1.283-.356-1.857M7 20H2v-2a3 3 0 015.356-1.857M7 20v-2c0-.656.126-1.283.356-1.857m0 0a5.002 5.002 0 019.288 0M15 7a3 3 0 11-6 0 3 3 0 016 0zm6 3a2 2 0 11-4 0 2 2 0 014 0zM7 10a2 2 0 11-4 0 2 2 0 014 0z"
            />
          </svg>
          <span class="ft-btn-text">{{ connectedCount }}</span>
        </button>
        <Transition name="ft-drop">
          <PeerDevicesPanel
            v-if="devPanelOpen"
            :rows="deviceRows"
            :scanning="deviceScanning"
            @connect="handleConnectDevice"
            @disconnect="handleDisconnectDevice"
            @set-active="handleSetActiveDevice"
            @scan="handleScanDevices"
          />
        </Transition>
      </div>
      <div class="ft-spacer"></div>
      <button
        class="ft-btn"
        :disabled="!peer.online"
        :title="t('transfer.topbar.sendToPhone')"
        @click="handleUpload"
      >
        <svg class="ft-ico-btn" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path
            stroke-linecap="round"
            stroke-linejoin="round"
            stroke-width="2"
            d="M22 2L11 13M22 2l-7 20-4-9-9-4 20-7z"
          />
        </svg>
        <span class="ft-btn-text">{{ t('transfer.topbar.sendToPhone') }}</span>
      </button>
      <button class="ft-btn ft-btn--primary" :disabled="!canDownload" @click="handleDownload">
        <svg class="ft-ico-btn" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path
            stroke-linecap="round"
            stroke-linejoin="round"
            stroke-width="2"
            d="M21 15v4a2 2 0 01-2 2H5a2 2 0 01-2-2v-4M7 10l5 5 5-5M12 15V3"
          />
        </svg>
        <span class="ft-btn-text">{{
          t('transfer.topbar.downloadSelected', { count: selectedCount })
        }}</span>
      </button>
      <button
        class="ft-btn"
        :disabled="!peer.online"
        :title="t('transfer.topbar.refresh')"
        @click="handleRefresh"
      >
        <svg class="ft-ico-btn" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path
            stroke-linecap="round"
            stroke-linejoin="round"
            stroke-width="2"
            d="M23 4v6h-6M1 20v-6h6"
          />
          <path
            stroke-linecap="round"
            stroke-linejoin="round"
            stroke-width="2"
            d="M3.51 9a9 9 0 0114.85-3.36L23 10M1 14l4.64 4.36A9 9 0 0020.49 15"
          />
        </svg>
        <span class="ft-btn-text">{{ t('transfer.topbar.refresh') }}</span>
      </button>
      <!-- 传输队列开关：面板默认收起，点击展开/收起（带任务数角标） -->
      <button
        class="ft-btn"
        :class="{ 'ft-btn--queue-open': queueVisible }"
        :title="t('transfer.queue.title')"
        @click="queueVisible = !queueVisible"
      >
        <svg class="ft-ico-btn" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path
            stroke-linecap="round"
            stroke-linejoin="round"
            stroke-width="2"
            d="M4 7h16M4 12h16M4 17h10"
          />
        </svg>
        <span class="ft-btn-text">{{ t('transfer.queue.title') }}</span>
        <span
          v-if="tasks.length > 0"
          class="ft-queue-count"
          :title="t('transfer.queue.count', { count: tasks.length })"
        >
          {{ tasks.length }}
        </span>
      </button>
      <button class="ft-btn" :title="t('transfer.topbar.settings')" @click="showSettings = true">
        <svg class="ft-ico-btn" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path
            stroke-linecap="round"
            stroke-linejoin="round"
            stroke-width="2"
            d="M4 21v-7M4 10V3M12 21v-9M12 8V3M20 21v-5M20 12V3M1 14h6M9 8h6M17 16h6"
          />
        </svg>
        <span class="ft-btn-text">{{ t('transfer.topbar.settings') }}</span>
      </button>
    </div>

    <!-- 双栏工作台：左栏随状态切换（空态提示 / 文件表格）；右栏传输队列默认收起，顶栏按钮展开 -->
    <div class="ft-main" :class="{ 'ft-main--queue': queueVisible }">
      <!-- 页面切换：空态 ↔ 工作台 out-in 交叉过渡，避免状态跳变闪烁 -->
      <Transition name="ft-page" mode="out-in">
        <!-- 空态：未配置共享目录 -->
        <div v-if="showNoRoots" class="ft-empty">
          <div class="ft-empty-ico">
            <svg fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path
                stroke-linecap="round"
                stroke-linejoin="round"
                stroke-width="1.5"
                d="M22 19a2 2 0 01-2 2H4a2 2 0 01-2-2V5a2 2 0 012-2h5l2 3h9a2 2 0 012 2z"
              />
            </svg>
          </div>
          <div class="ft-empty-title">{{ t('transfer.empty.noRoots') }}</div>
          <div class="ft-empty-desc">{{ t('transfer.empty.noRootsHint') }}</div>
          <button class="ft-btn ft-btn--primary ft-empty-action" @click="showSettings = true">
            {{ t('transfer.topbar.settings') }}
          </button>
        </div>

        <!-- 空态：对端未连接 / 已连接但未共享 -->
        <div v-else-if="showNoPeer" class="ft-empty">
          <div class="ft-empty-ico">
            <svg fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <rect x="6" y="2" width="12" height="20" rx="2" ry="2" />
              <path
                stroke-linecap="round"
                stroke-linejoin="round"
                stroke-width="1.5"
                d="M11 18h2"
              />
            </svg>
          </div>
          <div class="ft-empty-title">{{ noPeerLabel }}</div>
          <div class="ft-empty-desc">{{ t('transfer.empty.noPeerHint') }}</div>
        </div>

        <!-- 空态：未设置下载目录 -->
        <div v-else-if="settings.downloadDir === ''" class="ft-empty">
          <div class="ft-empty-ico">
            <svg fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path
                stroke-linecap="round"
                stroke-linejoin="round"
                stroke-width="1.5"
                d="M12 3v12M5 12l7 7 7-7"
              />
            </svg>
          </div>
          <div class="ft-empty-title">{{ t('transfer.empty.noDownloadDir') }}</div>
          <div class="ft-empty-desc">{{ t('transfer.empty.noDownloadDirHint') }}</div>
          <button class="ft-btn ft-btn--primary ft-empty-action" @click="showSettings = true">
            {{ t('transfer.topbar.settings') }}
          </button>
        </div>

        <!-- 工作态：远端文件表格（含对端存储权限提示） -->
        <div v-else class="ft-browse">
          <!-- 对端存储权限提示：列表为空且对端（移动端）可能未授予「所有文件访问权限」 -->
          <Transition name="ft-fade">
            <div v-if="notice === 'all_files_access_may_be_required'" class="ft-warning">
              <span class="ft-warning-ico">⚠</span>
              <span>{{ t('transfer.notice.storageAccess') }}</span>
            </div>
          </Transition>
          <RemoteFileTable
            :entries="fs.entries.value"
            :loading="fs.loading.value"
            :error-key="fs.errorKey.value"
            :breadcrumb="fs.breadcrumb.value"
            :selected-names="fs.selectedNames.value"
            @enter="fs.cd"
            @navigate="fs.goTo"
            @toggle="fs.toggle"
            @toggle-all="fs.toggleAll"
          />
        </div>
      </Transition>

      <!-- 传输队列：默认收起，顶栏「传输队列」按钮展开；随网格列宽同步滑入/滑出 -->
      <Transition name="ft-queue-panel">
        <TaskPanel
          v-if="queueVisible"
          :tasks="tasks"
          :total-speed="totalSpeed"
          :receiving="receiving"
          :history="history"
          :peer-names="peerNames"
          :download-dir="settings.downloadDir"
          @cancel="cancel"
          @retry="retry"
          @cancel-receiving="cancelReceiving"
          @clear-history="clearHistoryEntries"
          @open-folder="handleOpenFolder"
        />
      </Transition>
    </div>

    <!-- v2：接收中 toast（batch 立即弹；per-file 3s 窗口合并去重由 composable 处理） -->
    <Teleport to="body">
      <TransitionGroup name="ft-toast" tag="div" class="ft-toasts">
        <div v-for="toast in toasts" :key="toast.id" class="ft-toast">
          <svg class="ft-toast-ico" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path
              stroke-linecap="round"
              stroke-linejoin="round"
              stroke-width="2"
              d="M12 19V5M5 12l7-7 7 7"
            />
          </svg>
          <span class="ft-toast-text">
            {{ t('transfer.toast.receiving', { name: toast.name || '—', count: toast.count }) }}
          </span>
          <button
            class="ft-mini-btn ft-toast-close"
            :title="t('transfer.task.cancel')"
            @click="dismissToast(toast.id)"
          >
            <svg fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path
                stroke-linecap="round"
                stroke-linejoin="round"
                stroke-width="2"
                d="M18 6L6 18M6 6l12 12"
              />
            </svg>
          </button>
        </div>
      </TransitionGroup>
    </Teleport>

    <!-- 设置覆盖层（淡入 + 上滑）。注：SettingsPanel 曾因 defineEmits 缺失调用
         括号（宏未被展开，运行时 ReferenceError）导致半初始化组件毒化本层
         Transition 的更新路径（locateNonHydratedAsyncRoot 遍历遇 null subTree
         抛错），覆盖层永不出现；宏修复后 Transition 工作正常 -->
    <Transition name="ft-settings">
      <SettingsPanel
        v-if="showSettings"
        :settings="settings"
        :root-items="rootItems"
        @add-root="addRoot"
        @remove-root="removeRoot"
        @pick-download-dir="pickDownloadDir"
        @set-receiving-policy="setReceivingPolicy"
        @set-approval-timeout-sec="setApprovalTimeoutSec"
        @set-encryption="setEncryption"
        @close="showSettings = false"
      />
    </Transition>
  </div>
</template>
