<script setup lang="ts">
/**
 * FileTransferView — 文件传输浏览主页面 (Mobile)
 *
 * 结构（spec 9.2）：
 *   顶栏（对端名+连接状态，纯信息不带操作按钮）→ 面包屑 → Material 文件列表
 *   （图标+名称+元信息+多选勾选）→ 多选时底部主按钮「下载到手机（N 项 · 总大小）」
 *   → 迷你传输条（常驻）→ 右下角 FAB（上传 + 设置，悬浮于底部栏上方）。
 *   对端重测入口下沉到空态（对端未共享/目录不可用时展示重试按钮）。
 *
 * 语义分层（避免连接与文件共享混杂）：
 *   - 顶栏：只反映「连接」维度（已连接 / 未连接），与对端是否共享文件无关。
 *   - 页面正文：只反映「对端文件共享」维度（空目录 / 对端未共享 / 目录不可用），
 *     无论连接是否已建立，看不到对端文件统一归到「对端未共享」空态。
 *
 * 业务逻辑全部在 composables（useTasks / useRemoteFs / useSharedUpload），本组件只做 UI；
 * 设置页经 context.ui.openPage('settings') 整体路由跳转（SettingsPage 包装 useSettings）。
 * 上传入口为共享目录上传页（底部抽屉）：App 内遍历共享目录（SAF URI 存储）→
 * 「准备中」中转复制进度 + 取消 → 完成后自动入队（M1 上传页）。
 *
 * 经宿主 PluginViewHost 渲染（provide pluginContext），故此处直接断言非空。
 */
import { inject, ref, onMounted, onUnmounted, computed, watch } from 'vue'
import type { PluginContext, Disposable } from '@bedcode/plugin-sdk-mobile'
import { useTasks } from '../composables/useTasks'
import { useRemoteFs } from '../composables/useRemoteFs'
import { useSettings } from '../composables/useSettings'
import { useSharedUpload } from '../composables/useSharedUpload'
import { formatBytes, formatSpeed, progressPercent } from '../utils/format'
import FileTypeIcon from './FileTypeIcon.vue'
import TaskQueueSheet from './TaskQueueSheet.vue'
import SharedDirSheet from './SharedDirSheet.vue'
import BatchRequestDialog from './BatchRequestDialog.vue'

const context = inject<PluginContext>('pluginContext')!
const t = (key: string, params?: Record<string, any>) => context.i18n.t(key, params)

const tasks = useTasks(context)
const fs = useRemoteFs(context)
const settings = useSettings(context)
// 解构 Ref：模板需直接读 approvalTimeoutSec（settings 对象顶层是 settings Ref）
const { settings: transferSettings } = settings
const upload = useSharedUpload(context, tasks, settings)

/** 队列 bottom sheet 是否展开 */
const queueOpen = ref(false)

// ==================== v2 批量传输请求应答（全局弹窗，spec 14.4） ====================
//
// 前台 = BatchRequestDialog（排队 + 倒计时 + 必须明确选择接收/拒绝；超时自动
// 关闭、默认拒绝由宿主 pending TTL 执行）；后台/锁屏 = 系统通知 action 按钮
// （Kotlin 侧，见 TaskNotificationManager）。数据源为 batches-changed 事件 +
// list-batches 初始拉取；应答后批卡经 batches-changed 消失（宿主 resolved 事件驱动）。

/** 批请求应答（fire-and-forget；批卡消失由 resolved 快照驱动，失败仅记日志） */
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

// ==================== 下拉刷新（与原生下拉刷新同语义） ====================
/** 释放触发刷新的阈值（px） */
const PULL_TRIGGER = 56
/** 下拉最大位移（px） */
const PULL_MAX = 96
/** 阻尼系数：手指位移 → 指示器位移 */
const PULL_RESISTANCE = 0.45

/** 文件列表滚动容器（下拉手势作用域） */
const scrollEl = ref<HTMLElement | null>(null)
/** 指示器可见高度（随下拉位移变化；刷新中常驻阈值高度） */
const pullDistance = ref(0)
/** 下拉状态机：idle → pulling（未达阈值）/ ready（达阈值）→ refreshing → idle */
const pullState = ref<'idle' | 'pulling' | 'ready' | 'refreshing'>('idle')
/** 手指按住期间关闭回弹过渡（跟手），抬起后开启平滑回弹 */
const pullingActive = ref(false)

let pullStartY = 0

/** 仅在容器位于顶部且非加载/刷新中时接管触摸 */
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
    // 释放刷新：指示器常驻刷新态，目录加载完成后回弹
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

/**
 * 系统返回拦截：队列面板 → 先关闭面板；目录栈内 → 逐级返回上级目录；
 * 根目录 → 恢复默认后退（退出页面回上一页）。
 * 经宿主 ui.onBackPressed 注册（Tauri AppPlugin 将 Android 系统返回转发到 JS）。
 */
let disposeBackPress: Disposable | null = null

/** 对端展示名（实际名字或未连接文案） */
const peerLabel = computed(() => tasks.displayPeerName.value)

/**
 * 顶栏连接状态文案：只反映连接层，不掺杂对端共享语义。
 * 「对端未共享」是业务层信息，仅在页面正文空态出现，与顶栏分离。
 */
const peerStatusLabel = computed(() =>
  tasks.connOnline.value ? t('transfer.peer.online') : t('transfer.peer.offline'),
)

/** 顶栏连接状态文字语义色：成功 / 静默 */
const peerStatusClass = computed(() =>
  tasks.connOnline.value ? 'ft-peer-status--online' : 'ft-peer-status--offline',
)

/**
 * 空态类型：只表达「对端文件共享」维度，连接状态由顶栏承载。
 * 「对端未共享」统一兜底未连接 + 已连接但未共享两种看不到文件的情况。
 */
type EmptyKind = 'empty' | 'notSharing' | 'error'

const emptyKind = computed<EmptyKind>(() => {
  if (fs.error.value) return 'error'
  if (!tasks.peerOnline.value) return 'notSharing'
  return 'empty'
})

/** 空态主标题（按场景区分，避免无差别展示「此目录为空」） */
const emptyTitle = computed(() => {
  if (fs.notice.value) return t('transfer.notice.storageAccessTitle')
  switch (emptyKind.value) {
    case 'error': return t('transfer.table.dirUnavailable')
    case 'notSharing': return t('transfer.peer.notSharing')
    default: return t('transfer.table.empty')
  }
})

/** 空态说明文案（引导用户下一步操作） */
const emptyHint = computed(() => {
  if (fs.notice.value) return t('transfer.notice.storageAccess')
  switch (emptyKind.value) {
    case 'error': return t('transfer.empty.unavailableHint')
    case 'notSharing': return t('transfer.empty.notSharingHint')
    default: return t('transfer.empty.emptyDirHint')
  }
})

/** 空态图标底色（扁平 tint，随状态色） */
const emptyIcoClass = computed(() => {
  switch (emptyKind.value) {
    case 'error': return 'ft-empty-ico--error'
    case 'notSharing': return 'ft-empty-ico--warning'
    default: return 'ft-empty-ico--neutral'
  }
})

/** 空态图标路径（24 线性描边） */
const emptyIcoPath = computed(() => {
  switch (emptyKind.value) {
    case 'error':
      return 'M12 8v4m0 4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z'
    case 'notSharing':
      // 同心圆弧 wifi：顶部最宽 → 往下依次变短 → 底部圆点；
      // 不用 heroicons 原版 4 弧路径（其中 r5.25 弧会压在最长弧上方，视觉错乱）
      return 'M4.72 4.39a9.5 9.5 0 0114.56 0M7.82 5.52a6.5 6.5 0 018.36 0M10.8 7.21a3.5 3.5 0 012.4 0M12 20h.01'
    default:
      return 'M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z'
  }
})

/** 空态 CTA：异常场景重测对端；正常空目录刷新 */
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
    // 循环箭头 = 重新探测/重试语义（放大镜是搜索语义，不匹配）
    icon: 'M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15',
    action: () => tasks.queryPeer(),
  }
})

/** 迷你传输条主任务（null 表示无活跃任务） */
const primaryTask = computed(() => tasks.primaryTask.value)

/** 多选下载按钮文案：下载到手机 (N 项 · 总大小) */
const downloadLabel = computed(() =>
  t('transfer.topbar.downloadSelected', {
    count: fs.selectedCount.value,
    size: formatBytes(fs.selectedTotalSize.value, t),
  }),
)

// ==================== 「保存到…」（M3 单文件目标） ====================

/** 长按触发阈值（ms）：与原生长按菜单语义对齐 */
const LONG_PRESS_MS = 500
/** 长按定时器（触发后清除） */
let longPressTimer: ReturnType<typeof setTimeout> | null = null
/** 长按已触发标记：抑制随后 touchend 派生的 click（避免误勾选） */
let longPressFired = false

/** 长按文件行：启动定时器；目录长按不处理 */
function onRowTouchStart(_e: TouchEvent, entry: { isDir: boolean; name: string }): void {
  if (entry.isDir) return
  longPressFired = false
  longPressTimer = setTimeout(() => {
    longPressTimer = null
    longPressFired = true
    void saveToFile(entry.name)
  }, LONG_PRESS_MS)
}

/** 手指移动/抬起/取消：取消未达阈值的长按 */
function onRowTouchCancel(): void {
  if (longPressTimer) {
    clearTimeout(longPressTimer)
    longPressTimer = null
  }
}

/** 桌面 dev 兜底：右键菜单触发「保存到…」（真机走长按） */
function onRowContextMenu(entry: { isDir: boolean; name: string }): void {
  if (entry.isDir) return
  void saveToFile(entry.name)
}

/**
 * 「保存到…」：入队下载（中转目录唯一名）→ 完成时 WASM 弹系统保存对话框
 * （ACTION_CREATE_DOCUMENT，用户选位置）→ 流拷贝即达 → 删除中转副本
 */
async function saveToFile(name: string): Promise<void> {
  if (!tasks.peerOnline.value) {
    context.dialogs.showToast(t('transfer.upload.offline'), 'error')
    return
  }
  const base = fs.currentPath.value
  const path = base ? `${base}/${name}` : name
  const ok = await tasks.enqueueDownload(
    [path],
    { id: tasks.peerId.value, name: tasks.displayPeerName.value },
    { saveTo: true },
  )
  if (ok > 0) {
    context.dialogs.showToast(t('transfer.saveTo.enqueued'), 'success')
  }
}

/** 点击行：目录进入，文件勾选（长按已触发时抑制，避免误勾选） */
function onRowTap(entry: { name: string; isDir: boolean }): void {
  if (longPressFired) {
    longPressFired = false
    return
  }
  if (entry.isDir) {
    void fs.cd(entry.name)
  } else {
    fs.toggle(entry.name)
  }
}

/** 批量下载勾选文件（remotePath 拼接当前目录路径，与桌面端 handleDownload 对齐） */
async function downloadSelected(): Promise<void> {
  if (!tasks.peerOnline.value) return
  const base = fs.currentPath.value
  const paths = fs.selectedFiles.value.map(name => (base ? `${base}/${name}` : name))
  if (paths.length === 0) return
  const ok = await tasks.enqueueDownload(paths, {
    id: tasks.peerId.value,
    name: tasks.displayPeerName.value,
  })
  if (ok > 0) fs.clearSelection()
}

/**
 * 上传入口：打开共享目录上传页（底部抽屉）
 *
 * M1 上传源严格限于共享目录（SAF 目录树授权，见 spec）：App 内遍历
 * 共享目录文件列表 → 「准备中」中转复制 → 完成后自动入队。
 */
async function uploadFile(): Promise<void> {
  if (!tasks.peerOnline.value) {
    context.dialogs.showToast(t('transfer.upload.offline'), 'error')
    return
  }
  await upload.openSheet()
}

/**
 * 对端共享状态变化驱动目录加载（与桌面端 watch(peer.online) 对齐）：
 * queryPeer/peer_changed 置位 peerOnline 后自动刷新目录，
 * 避免首次进入停留在「对端未共享」空态需手动点重测。
 */
watch(
  () => tasks.peerOnline.value,
  (online) => {
    if (online) {
      void fs.refresh()
    } else {
      // 对端下线：清空残留目录条目与勾选，避免对离线对端入队
      fs.reset()
    }
  },
)

onMounted(() => {
  tasks.start()
  void tasks.refreshV2()
  disposeBackPress = context.ui.onBackPressed(({ canGoBack }) => {
    if (queueOpen.value) {
      queueOpen.value = false
      return
    }
    if (fs.crumbs.value.length > 0) {
      void fs.up()
      return
    }
    if (canGoBack) history.back()
  })
  if (tasks.peerOnline.value) {
    // 对端已就绪（视图挂载晚于事件）：直接加载
    void fs.load('')
  } else {
    // 主动探测对端状态（防止先挂载后连接/广播丢失导致状态未同步）；
    // 探测回复后 peerOnline 置位，由上方 watch 自动加载目录
    void tasks.queryPeer()
  }
})

onUnmounted(() => {
  disposeBackPress?.dispose()
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

    <!-- 顶栏：对端名 + 连接状态 + 右上操作（上传 / 设置） -->
    <div class="flex-shrink-0 flex items-center gap-2 px-4 pt-2.5 pb-2">
      <span class="ft-peer-name min-w-0 max-w-[45%] text-[var(--mobile-text-primary)] truncate">
        {{ peerLabel }}
      </span>
      <!-- 状态胶囊紧跟对端名（连接态 success tint 底 / 断开态中性底） -->
      <span class="ft-peer-status flex-shrink-0" :class="peerStatusClass">
        {{ peerStatusLabel }}
      </span>
      <!-- 弹性空隙：把操作按钮推到行尾 -->
      <div class="flex-1 min-w-2"></div>
      <!-- 操作按钮（上传 / 设置）：纯图标，置于顶栏右侧，避免悬浮于列表数据之上造成遮挡 -->
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
            @touchstart.passive="(e) => onRowTouchStart(e, entry)"
            @touchmove.passive="onRowTouchCancel"
            @touchend="onRowTouchCancel"
            @touchcancel="onRowTouchCancel"
            @contextmenu.prevent="onRowContextMenu(entry)"
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

    <!-- 队列 bottom sheet（v2 四 tab：全部/发送/接收/历史） -->
    <TaskQueueSheet
      :open="queueOpen"
      :tasks="tasks.tasks.value"
      :receiving="tasks.receivingTasks.value"
      :history="tasks.history.value"
      :speed-map="tasks.speedMap.value"
      :total-speed="tasks.totalSpeed.value"
      :resumable-count="tasks.resumableCount.value"
      :t="t"
      @close="queueOpen = false"
      @pause="(id) => tasks.pause(id)"
      @resume="(id) => tasks.resume(id)"
      @cancel="(id) => tasks.cancel(id)"
      @retry="(id) => tasks.retry(id)"
      @remove="(id) => tasks.removeTask(id)"
      @open="(id) => tasks.openTask(id)"
      @resume-all="() => tasks.resumeAll()"
      @cancel-receiving="(sessionId) => tasks.cancelReceiving(sessionId)"
      @clear-history="() => tasks.clearHistory()"
      @open-history="(entry) => tasks.openHistoryEntry(entry)"
    />

    <!-- 共享目录上传页（M1：共享目录文件列表 + 准备中进度 + 取消） -->
    <SharedDirSheet
      :open="upload.open.value"
      :upload="upload"
      :t="t"
      @close="upload.close()"
      @open-settings="() => { upload.close(); context.ui.openPage('settings') }"
    />
  </div>
</template>

<style scoped>
/* 对端名称：流式字号 */
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
