<script setup lang="ts">
/**
 * FileTransferView — 文件传输双栏工作台（原型 Variant A）
 *
 * 顶栏（对端 pill + 下载所选/刷新/设置）+ 左栏 RemoteFileTable + 右栏
 * TaskPanel（360px 常驻）。空态分级：未配共享目录 → 未配对 → 未设下载目录。
 * 对端上/下线（filesrv:peer_changed）驱动目录自动加载与清空。
 */
import { ref, computed, watch, inject, onMounted, onUnmounted } from 'vue'
import type { PluginContext } from '@bedcode/plugin-sdk-desktop'
import RemoteFileTable from './RemoteFileTable.vue'
import TaskPanel from './TaskPanel.vue'
import SettingsPanel from './SettingsPanel.vue'
import { useTasks } from '../composables/useTasks'
import { useRemoteFs } from '../composables/useRemoteFs'
import { useSettings } from '../composables/useSettings'
import { usePeer } from '../composables/usePeer'

const context = inject<PluginContext>('pluginContext')!
const t = (key: string, params?: Record<string, any>) => context.i18n.t(key, params)

const { peer, connOnline, start: startPeer, stop: stopPeer } = usePeer(context)
const { tasks, speedMap, summary, resumableCount, totalSpeed, enqueueDownload, enqueueUpload, queryPeer, refresh: refreshTasks, pause, resume, cancel, retry, resumeAll, start: startTasks, stop: stopTasks } = useTasks(context)
const { settings, hasRoots, load: loadSettings, addRoot, removeRoot, pickDownloadDir, setConcurrency } = useSettings(context)
const {
  entries,
  loading,
  errorKey,
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

/** 批量下载所选文件（remotePath 拼接当前目录路径） */
async function handleDownload(): Promise<void> {
  if (!canDownload.value) return
  const base = currentPath.value
  const paths = selectedEntries.value.map(e => (base ? `${base}/${e.name}` : e.name))
  await enqueueDownload(paths, { id: peer.value.id, name: peerDisplayName.value })
  clearSelection()
}

/** 顶栏刷新：任务列表 + 当前目录 + 主动探测对端状态 */
async function handleRefresh(): Promise<void> {
  await Promise.all([refreshTasks(), refreshDir(), queryPeer()])
}

/** 发送到手机：弹本地多文件选择 → 入队上传（对端根目录） */
async function handleUpload(): Promise<void> {
  if (!peer.value.online) return
  const files = await context.fileService.pickFiles()
  if (!files.length) return
  const ok = await enqueueUpload(files, { id: peer.value.id, name: peerDisplayName.value })
  if (ok < files.length) {
    // 部分失败（如对端同名拒绝）时刷新任务列表让用户看到 rejected 原因
    void refreshTasks()
  }
}

/** 对端上/下线驱动目录加载/清空 */
watch(
  () => peer.value.online,
  (online) => {
    if (online) {
      void loadDir()
    } else {
      stopRemote()
      clearSelection()
    }
  },
)

onMounted(async () => {
  startPeer()
  startTasks()
  await Promise.all([loadSettings(), refreshTasks()])
  // 主动探测对端状态（防止先挂载后连接/广播丢失导致状态未同步）
  void queryPeer()
  if (peer.value.online) await loadDir()
})

onUnmounted(() => {
  stopPeer()
  stopTasks()
  stopRemote()
})
</script>

<template>
  <div class="ft-view">
    <!-- 顶栏 -->
    <div class="ft-topbar">
      <div class="ft-peer-pill">
        <span
          class="ft-dot"
          :class="
            connOnline.value
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
      <div class="ft-spacer"></div>
      <button class="ft-btn" :disabled="!peer.online" @click="handleUpload">
        📤<span class="ft-btn-text">{{ t('transfer.topbar.sendToPhone') }}</span>
      </button>
      <button class="ft-btn ft-btn--primary" :disabled="!canDownload" @click="handleDownload">
        {{ t('transfer.topbar.downloadSelected', { count: selectedCount }) }}
      </button>
      <button class="ft-btn" :disabled="!peer.online" @click="handleRefresh">
        ⟳<span class="ft-btn-text">{{ t('transfer.topbar.refresh') }}</span>
      </button>
      <button class="ft-btn" @click="showSettings = true">
        ⚙<span class="ft-btn-text">{{ t('transfer.topbar.settings') }}</span>
      </button>
    </div>

    <!-- 空态：未配置共享目录 -->
    <div v-if="showNoRoots" class="ft-empty">
      <div class="ft-empty-ico">📂</div>
      <div>{{ t('transfer.empty.noRoots') }}</div>
      <button class="ft-btn ft-empty-action" @click="showSettings = true">
        {{ t('transfer.topbar.settings') }}
      </button>
    </div>

    <!-- 空态：对端未连接 / 已连接但未共享 -->
    <div v-else-if="showNoPeer" class="ft-empty">
      <div class="ft-empty-ico">📱</div>
      <div>{{ noPeerLabel }}</div>
    </div>

    <!-- 空态：未设置下载目录 -->
    <div v-else-if="settings.downloadDir === ''" class="ft-empty">
      <div class="ft-empty-ico">⬇️</div>
      <div>{{ t('transfer.empty.noDownloadDir') }}</div>
      <button class="ft-btn ft-empty-action" @click="showSettings = true">
        {{ t('transfer.topbar.settings') }}
      </button>
    </div>

    <!-- 双栏工作台 -->
    <div v-else class="ft-main">
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
      <TaskPanel
        :tasks="tasks"
        :speed-map="speedMap"
        :summary="summary"
        :resumable-count="resumableCount"
        :total-speed="totalSpeed"
        @pause="pause"
        @resume="resume"
        @cancel="cancel"
        @retry="retry"
        @resume-all="resumeAll"
      />
    </div>

    <!-- 设置覆盖层 -->
    <SettingsPanel
      v-if="showSettings"
      :settings="settings"
      @add-root="addRoot"
      @remove-root="removeRoot"
      @pick-download-dir="pickDownloadDir"
      @set-concurrency="setConcurrency"
      @close="showSettings = false"
    />
  </div>
</template>
