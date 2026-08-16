<script setup lang="ts">
/**
 * FileTransferView — 文件传输双栏工作台（原型 Variant A）
 *
 * 顶栏（对端 pill + 下载所选/刷新/设置）+ 左栏 RemoteFileTable + 右栏
 * TaskPanel（360px 常驻）。空态分级：未配共享目录 → 未配对 → 未设下载目录。
 * 对端上/下线（filesrv:peer_changed）驱动目录自动加载与清空。
 */
import { ref, computed, watch, inject, onMounted, onUnmounted } from 'vue'
import type { PluginContext } from '@binblink/plugin-sdk-desktop'
import RemoteFileTable from './RemoteFileTable.vue'
import TaskPanel from './TaskPanel.vue'
import SettingsPanel from './SettingsPanel.vue'
import BatchRequestDialog from './BatchRequestDialog.vue'
import { useTasks } from '../composables/useTasks'
import { useReceiving } from '../composables/useReceiving'
import { useRemoteFs } from '../composables/useRemoteFs'
import { useSettings } from '../composables/useSettings'
import { usePeer } from '../composables/usePeer'
import { formatBytes } from '../utils/format'

const context = inject<PluginContext>('pluginContext')!
const t = (key: string, params?: Record<string, any>) => context.i18n.t(key, params)

const {
  peer,
  peers,
  activePeerId,
  connOnline,
  switchPeer,
  start: startPeer,
  stop: stopPeer,
} = usePeer(context)
const { tasks, speedMap, summary, resumableCount, totalSpeed, enqueueDownload, enqueueUpload, queryPeer, refresh: refreshTasks, pause, resume, cancel, retry, removeTask, openInDir, resumeAll, start: startTasks, stop: stopTasks } = useTasks(context)
const { batches, receiving, history, toasts, approveBatch, rejectBatch, cancelReceiving, clearHistory, dismissToast, start: startReceiving, stop: stopReceiving } = useReceiving(context)
const { settings, hasRoots, load: loadSettings, addRoot, removeRoot, pickDownloadDir, setConcurrency, setReceivingPolicy, setApprovalTimeoutSec } = useSettings(context)
const {
  entries,
  loading,
  errorKey,
  notice,
  breadcrumb,
  selectedNames,
  currentPath,
  selectedEntries,
  clearSelection,
  load: loadDir,
  enterDir,
  navigateTo,
  toggleSelect,
  toggleAll,
  refresh: refreshDir,
  stop: stopRemote,
} = useRemoteFs(context, () => peer.value.id)

const showSettings = ref(false)

/** 传输队列面板是否展开（默认收起，顶栏按钮切换） */
const queueVisible = ref(false)

/**
 * 对端显示名：device-connected 缓存 → 任务快照 peer.name → peerId → IP。
 * 详见 usePeer 内设备名说明。
 */
const peerDisplayName = computed(() => {
  if (peer.value.name) return peer.value.name
  const withName = tasks.value.find(x => x.peer?.name)
  if (withName?.peer?.name) return withName.peer.name
  // 无设备名时 IP 比原始 peerId 更可辨识（内网传输场景），再退到 peerId
  if (peer.value.ip || peer.value.id) return peer.value.ip || peer.value.id
  // 已连接但尚未收到对端公告（未共享）：无可辨识信息时用占位符，
  // 避免与「未连接设备」文案混用
  if (connOnline.value) return '—'
  return t('transfer.peer.unpaired')
})

const selectedCount = computed(() => selectedNames.value.length)

/** v2：对端名映射（peerId → 展示名，批卡/接收任务展示用） */
const peerNames = computed<Record<string, string>>(() => {
  const map: Record<string, string> = {}
  for (const p of peers.value) map[p.id] = p.name || p.ip || p.id
  // 任务快照里的 peer.name 兜底（设备列表可能未含任务绑定的对端）
  for (const t of tasks.value) {
    if (t.peer?.deviceId && t.peer.name && !map[t.peer.deviceId]) {
      map[t.peer.deviceId] = t.peer.name
    }
  }
  return map
})

/** v2：历史条目打开所在文件夹（localPath 直接可用）
 * 下载方向历史 local_path 为 .part 临时名（文件完成后已 rename 到最终路径），
 * 需去后缀后才存在；兼容旧库数据的同时与任务卡 openInDir 保持一致 */
function openHistoryDir(localPath: string): void {
  if (!localPath) return
  // 与 openInDir 相同的 .part 剥离（历史库可能存旧 .part 路径，见 wasm 归档逻辑）
  const finalPath = localPath.endsWith('.part')
    ? localPath.slice(0, -'.part'.length)
    : localPath
  // 诊断：点击历史「打开所在文件夹」时打印实际解析出的定位路径
  console.log(`[File Transfer] openHistoryDir raw=${localPath} -> ${finalPath}`)
  void context.system.revealInDir(finalPath).catch((err: unknown) => {
    console.error(`[File Transfer] reveal failed for "${finalPath}":`, err)
  })
}

/** 批请求应答（fire-and-forget；批卡消失由 resolved 快照驱动，失败仅记日志） */
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

/** 顶栏状态文案：未连接 / 已连接但对端未共享 / 已连接 */
const peerStatusLabel = computed(() => {
  if (!connOnline.value) return t('transfer.peer.offline')
  if (!peer.value.online) return t('transfer.peer.notSharing')
  return t('transfer.peer.online')
})

/** 主下载按钮可用性：有选择 + 对端已共享 + 已配下载目录 */
const canDownload = computed(
  () => selectedCount.value > 0 && peer.value.online && settings.value.downloadDir !== '',
)

/** 空态分支优先级：共享目录 → 对端 → 下载目录 */
const showNoRoots = computed(() => !hasRoots.value)
/** 无法浏览对端目录：未连接（提示未连接）或已连接但未共享（提示对端未共享） */
const showNoPeer = computed(() => !peer.value.online)
const noPeerLabel = computed(() =>
  connOnline.value ? t('transfer.peer.notSharing') : t('transfer.empty.noPeer'),
)

/** 批量下载所选文件（remotePath 拼接当前目录路径）；入队成功后展开队列面板便于查看进度 */
async function handleDownload(): Promise<void> {
  if (!canDownload.value) return
  const base = currentPath.value
  const paths = selectedEntries.value.map(e => (base ? `${base}/${e.name}` : e.name))
  const ok = await enqueueDownload(paths, { id: peer.value.id, name: peerDisplayName.value })
  clearSelection()
  if (ok > 0) queueVisible.value = true
}

/** 顶栏刷新：任务列表 + 当前目录 + 主动探测对端状态 */
async function handleRefresh(): Promise<void> {
  await Promise.all([refreshTasks(), refreshDir(), queryPeer()])
}

/** 发送到手机：弹本地多文件选择 → 入队上传（对端根目录）；入队成功后展开队列面板便于查看进度 */
async function handleUpload(): Promise<void> {
  if (!peer.value.online) return
  const files = await context.fileService.pickFiles()
  if (!files.length) return
  const ok = await enqueueUpload(files, { id: peer.value.id, name: peerDisplayName.value })
  if (ok > 0) queueVisible.value = true
  if (ok < files.length) {
    // 部分失败（如对端同名拒绝）时刷新任务列表让用户看到 rejected 原因
    void refreshTasks()
  }
}

/** 设备切换菜单开合（多对端切换入口） */
const peerMenuOpen = ref(false)

/** 切换激活设备：关菜单 + 调插件命令；成功由 activePeerId 变化驱动目录重载 */
async function handleSwitchPeer(id: string): Promise<void> {
  peerMenuOpen.value = false
  await switchPeer(id)
}

/** 激活设备变化（上线自动激活 / 手动切换）驱动目录加载/清空 */
watch(
  () => peer.value.id,
  (id) => {
    if (id) {
      // 重置到根目录：切换设备后旧面包屑路径可能在新对端不存在
      navigateTo(0)
    } else {
      stopRemote()
      clearSelection()
    }
  },
)

onMounted(async () => {
  startPeer()
  startTasks()
  startReceiving()
  await Promise.all([loadSettings(), refreshTasks()])
  // 主动探测对端状态（防止先挂载后连接/广播丢失导致状态未同步）
  void queryPeer()
  if (peer.value.online) await loadDir()
})

onUnmounted(() => {
  stopPeer()
  stopTasks()
  stopReceiving()
  stopRemote()
})
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

    <!-- 顶栏 -->
    <div class="ft-topbar">
      <div class="ft-peer-pill">
        <span
          class="ft-dot"
          :class="
            connOnline
              ? peer.online
                ? 'ft-dot--online'
                : 'ft-dot--partial'
              : 'ft-dot--offline'
          "
        ></span>
        <span class="ft-peer-name">{{ peerDisplayName }}</span>
        <span class="ft-peer-meta">
          {{ peerStatusLabel }}
        </span>
      </div>
      <!-- 设备切换：多对端场景点击弹出在线设备列表 -->
      <div class="ft-peer-switch-wrap">
        <button
          class="ft-btn ft-peer-switch-btn"
          :class="{ 'ft-peer-switch-btn--open': peerMenuOpen }"
          :disabled="peers.length === 0"
          :title="t('transfer.peer.switchTitle')"
          @click="peerMenuOpen = !peerMenuOpen"
        >
          <svg class="w-3.5 h-3.5 flex-shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M17 20h5v-2a3 3 0 00-5.356-1.857M17 20H7m10 0v-2c0-.656-.126-1.283-.356-1.857M7 20H2v-2a3 3 0 015.356-1.857M7 20v-2c0-.656.126-1.283.356-1.857m0 0a5.002 5.002 0 019.288 0M15 7a3 3 0 11-6 0 3 3 0 016 0zm6 3a2 2 0 11-4 0 2 2 0 014 0zM7 10a2 2 0 11-4 0 2 2 0 014 0z" />
          </svg>
          <span class="ft-btn-text">{{ peers.length }}</span>
        </button>
        <!-- 设备列表下拉（自绘，禁原生 select） -->
        <Transition name="ft-drop">
          <div v-if="peerMenuOpen" class="ft-peer-menu">
            <div class="ft-peer-menu-title">{{ t('transfer.peer.switchTitle') }}</div>
            <button
              v-for="p in peers"
              :key="p.id"
              class="ft-peer-menu-item"
              :class="{ 'ft-peer-menu-item--active': p.id === activePeerId }"
              @click="handleSwitchPeer(p.id)"
            >
              <!-- 列表内对端均为在线（peer_changed online 才入列），统一绿点，激活项以高亮+勾标识 -->
              <span class="ft-dot ft-dot--online"></span>
              <span class="ft-peer-menu-name">{{ p.name || p.ip || p.id }}</span>
              <span v-if="p.id === activePeerId" class="ft-peer-menu-check">
                <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="3" d="M5 13l4 4L19 7" />
                </svg>
              </span>
            </button>
          </div>
        </Transition>
      </div>
      <div class="ft-spacer"></div>
      <button class="ft-btn" :disabled="!peer.online" @click="handleUpload" :title="t('transfer.topbar.sendToPhone')">
        <svg class="ft-ico-btn" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M22 2L11 13M22 2l-7 20-4-9-9-4 20-7z" />
        </svg>
        <span class="ft-btn-text">{{ t('transfer.topbar.sendToPhone') }}</span>
      </button>
      <button class="ft-btn ft-btn--primary" :disabled="!canDownload" @click="handleDownload">
        <svg class="ft-ico-btn" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M21 15v4a2 2 0 01-2 2H5a2 2 0 01-2-2v-4M7 10l5 5 5-5M12 15V3" />
        </svg>
        <span class="ft-btn-text">{{ t('transfer.topbar.downloadSelected', { count: selectedCount }) }}</span>
      </button>
      <button class="ft-btn" :disabled="!peer.online" @click="handleRefresh" :title="t('transfer.topbar.refresh')">
        <svg class="ft-ico-btn" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M23 4v6h-6M1 20v-6h6" />
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M3.51 9a9 9 0 0114.85-3.36L23 10M1 14l4.64 4.36A9 9 0 0020.49 15" />
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
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 7h16M4 12h16M4 17h10" />
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
      <button class="ft-btn" @click="showSettings = true" :title="t('transfer.topbar.settings')">
        <svg class="ft-ico-btn" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 21v-7M4 10V3M12 21v-9M12 8V3M20 21v-5M20 12V3M1 14h6M9 8h6M17 16h6" />
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
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M22 19a2 2 0 01-2 2H4a2 2 0 01-2-2V5a2 2 0 012-2h5l2 3h9a2 2 0 012 2z" />
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
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M11 18h2" />
          </svg>
        </div>
        <div class="ft-empty-title">{{ noPeerLabel }}</div>
        <div class="ft-empty-desc">{{ t('transfer.empty.noPeerHint') }}</div>
      </div>

      <!-- 空态：未设置下载目录 -->
      <div v-else-if="settings.downloadDir === ''" class="ft-empty">
        <div class="ft-empty-ico">
          <svg fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M12 3v12M5 12l7 7 7-7" />
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
            :entries="entries"
            :loading="loading"
            :error-key="errorKey"
            :breadcrumb="breadcrumb"
            :selected-names="selectedNames"
            @enter="enterDir"
            @navigate="navigateTo"
            @toggle="toggleSelect"
            @toggle-all="toggleAll"
          />
        </div>
      </Transition>

      <!-- 传输队列：默认收起，顶栏「传输队列」按钮展开；随网格列宽同步滑入/滑出 -->
      <Transition name="ft-queue-panel">
        <TaskPanel
          v-if="queueVisible"
          :tasks="tasks"
          :speed-map="speedMap"
          :summary="summary"
          :resumable-count="resumableCount"
          :total-speed="totalSpeed"
          :receiving="receiving"
          :history="history"
          :peer-names="peerNames"
          @pause="pause"
          @resume="resume"
          @cancel="cancel"
          @retry="retry"
          @remove="removeTask"
          @open-dir="openInDir"
          @resume-all="resumeAll"
          @cancel-receiving="cancelReceiving"
          @clear-history="clearHistory"
          @open-history-dir="openHistoryDir"
        />
      </Transition>
    </div>

    <!-- v2：接收中 toast（batch 立即弹；per-file 3s 窗口合并去重由 composable 处理） -->
    <Teleport to="body">
      <TransitionGroup name="ft-toast" tag="div" class="ft-toasts">
        <div v-for="toast in toasts" :key="toast.id" class="ft-toast">
          <svg class="ft-toast-ico" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 19V5M5 12l7-7 7 7" />
          </svg>
          <span class="ft-toast-text">
            {{ t('transfer.toast.receiving', { name: toast.name || '—', count: toast.count }) }}
          </span>
          <button class="ft-mini-btn ft-toast-close" :title="t('transfer.task.cancel')" @click="dismissToast(toast.id)">
            <svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M18 6L6 18M6 6l12 12" /></svg>
          </button>
        </div>
      </TransitionGroup>
    </Teleport>

    <!-- 设置覆盖层（淡入 + 上滑） -->
    <Transition name="ft-settings">
      <SettingsPanel
        v-if="showSettings"
        :settings="settings"
        @add-root="addRoot"
        @remove-root="removeRoot"
        @pick-download-dir="pickDownloadDir"
        @set-concurrency="setConcurrency"
        @set-receiving-policy="setReceivingPolicy"
        @set-approval-timeout-sec="setApprovalTimeoutSec"
        @close="showSettings = false"
      />
    </Transition>
  </div>
</template>
