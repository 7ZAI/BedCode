<script setup lang="ts">
/**
 * FileTransferView — 文件传输浏览主页面 (Mobile)
 *
 * 结构（spec 9.2）：
 *   顶栏（对端名+在线状态，纯信息不带操作按钮）→ 面包屑 → Material 文件列表
 *   （图标+名称+元信息+多选勾选）→ 多选时底部主按钮「下载到手机（N 项 · 总大小）」
 *   → 迷你传输条（常驻）→ 右下角 FAB（上传 + 设置，悬浮于底部栏上方）。
 *   对端重测入口下沉到空态（离线/目录不可用时展示重试按钮）。
 *
 * 业务逻辑全部在 composables（useTasks / useRemoteFs），本组件只做 UI；
 * 设置页经 context.ui.openPage('settings') 整体路由跳转（SettingsPage 包装 useSettings）。
 * 上传入口为占位：移动 SDK 无文件选择 API，经 dialogs.showPrompt 手动输入本地绝对路径。
 *
 * 经宿主 PluginViewHost 渲染（provide pluginContext），故此处直接断言非空。
 */
import { inject, ref, onMounted, onUnmounted, computed } from 'vue'
import type { PluginContext } from '@bedcode/plugin-sdk-mobile'
import { useTasks } from '../composables/useTasks'
import { useRemoteFs } from '../composables/useRemoteFs'
import { formatBytes, formatSpeed, progressPercent } from '../utils/format'
import TaskQueueSheet from './TaskQueueSheet.vue'

const context = inject<PluginContext>('pluginContext')!
const t = (key: string, params?: Record<string, any>) => context.i18n.t(key, params)

const tasks = useTasks(context)
const fs = useRemoteFs(context)

/** 队列 bottom sheet 是否展开 */
const queueOpen = ref(false)

/** 对端展示名（实际名字或未连接文案） */
const peerLabel = computed(() => tasks.displayPeerName.value)

/** 迷你传输条主任务（null 表示无活跃任务） */
const primaryTask = computed(() => tasks.primaryTask.value)

/** 多选下载按钮文案：下载到手机 (N 项 · 总大小) */
const downloadLabel = computed(() =>
  t('transfer.topbar.downloadSelected', {
    count: fs.selectedCount.value,
    size: formatBytes(fs.selectedTotalSize.value, t),
  }),
)

/** 点击行：目录进入，文件勾选 */
function onRowTap(entry: { name: string; isDir: boolean }): void {
  if (entry.isDir) {
    void fs.cd(entry.name)
  } else {
    fs.toggle(entry.name)
  }
}

/** 批量下载勾选文件 */
async function downloadSelected(): Promise<void> {
  const paths = fs.selectedFiles.value
  if (paths.length === 0) return
  const ok = await tasks.enqueueDownload(paths, {
    id: tasks.peerId.value,
    name: tasks.displayPeerName.value,
  })
  if (ok > 0) fs.clearSelection()
}

/**
 * 上传入口：优先系统文件选择器（fileService.pickFile，Android SAF 免权限）；
 * 选择器不可用/路径解析失败降级 dialogs.showPrompt 手动输入绝对路径。
 * 目标远端目录为当前面包屑目录，文件名取本地路径 basename。
 */
async function uploadFile(): Promise<void> {
  let localPath: string | null = null
  try {
    localPath = await context.fileService.pickFile()
  } catch {
    // SAF 选择器不可用（不支持的 provider / iOS）→ 手动输入兜底
    localPath = await context.dialogs.showPrompt({
      title: t('transfer.dialog.uploadTitle'),
      message: t('transfer.settings.addRootHint'),
      inputPlaceholder: t('transfer.dialog.localPathPlaceholder'),
      confirmText: t('transfer.task.upload'),
      cancelText: t('transfer.dialog.cancel'),
    })
  }
  if (!localPath) return
  const fileName = localPath.split(/[\\/]/).pop() || localPath
  const base = fs.currentPath.value
  const remotePath = base ? `${base}/${fileName}` : fileName
  await tasks.enqueueUpload({
    peerId: tasks.peerId.value,
    peerName: tasks.displayPeerName.value,
    remotePath,
    localPath,
  })
}

onMounted(() => {
  tasks.start()
  void fs.load('')
  // 主动探测对端状态（防止先挂载后连接/广播丢失导致状态未同步）
  void tasks.queryPeer()
})

onUnmounted(() => {
  tasks.stop()
})
</script>

<template>
  <div class="ft-view h-full flex flex-col bg-[var(--mobile-bg-primary)]">
    <!-- 顶栏：对端名 + 在线状态（纯信息，操作按钮在右下角 FAB / 空态） -->
    <div class="flex-shrink-0 flex items-center gap-2 px-4 pt-3 pb-2">
      <span
        class="status-dot flex-shrink-0"
        :class="tasks.peerOnline.value ? 'dot-emerald' : 'dot-zinc'"
      ></span>
      <span class="ft-peer-name text-[var(--mobile-text-primary)] truncate">
        {{ peerLabel }}
      </span>
      <span class="ft-peer-status flex-shrink-0">
        {{ tasks.peerOnline.value ? t('transfer.peer.online') : t('transfer.peer.offline') }}
      </span>
    </div>

    <!-- 面包屑 -->
    <div class="flex-shrink-0 flex items-center gap-1 px-4 py-2 overflow-x-auto">
      <button
        class="ft-breadcrumb-item flex-shrink-0"
        @click="fs.goRoot()"
      >
        {{ t('transfer.breadcrumb.home') }}
      </button>
      <template v-for="(seg, i) in fs.crumbs.value" :key="seg">
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

    <!-- 文件列表：相对定位容器，右下角悬浮 FAB 锚定于此（不随滚动移动） -->
    <div class="flex-1 relative overflow-y-auto min-h-0 overscroll-behavior-none">
      <div class="px-4">
        <!-- 加载态 -->
        <div v-if="fs.loading.value" class="py-10 text-center">
          <p class="ft-body-text text-[var(--mobile-text-muted)]">{{ t('transfer.table.loading') }}</p>
        </div>

        <!-- 空态 -->
        <div v-else-if="fs.entries.value.length === 0" class="py-10 text-center px-4">
          <p class="ft-body-text text-[var(--mobile-text-muted)]">
            {{ fs.error.value ? t('transfer.table.dirUnavailable') : t('transfer.table.empty') }}
          </p>
          <!-- 离线/目录不可用：对端重测下沉到空态，避免顶栏塞操作按钮 -->
          <button
            v-if="!tasks.peerOnline.value || fs.error.value"
            class="mt-4 inline-flex items-center gap-1.5 ft-touch-btn px-4 rounded-xl ft-btn-neutral active:opacity-80 transition-opacity ft-mini-text"
            @click="tasks.queryPeer()"
          >
            <svg class="w-4 h-4 flex-shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M21 21l-4.35-4.35M17 11a6 6 0 11-12 0 6 6 0 0112 0z" />
            </svg>
            {{ t('transfer.topbar.queryPeer') }}
          </button>
        </div>

        <!-- 目录/文件行：复用宿主 group-card / group-row 视觉语言 -->
        <div v-else class="group-card">
          <button
            v-for="(entry, idx) in fs.entries.value"
            :key="entry.name"
            class="group-row group-row-btn"
            :class="{ 'ft-row-last': idx === fs.entries.value.length - 1 }"
            @click="onRowTap(entry)"
          >
            <!-- 类型图标 -->
            <span
              class="icon-chip flex-shrink-0"
              :class="entry.isDir ? 'chip-cyan' : 'chip-zinc'"
            >
              <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path
                  v-if="entry.isDir"
                  stroke-linecap="round"
                  stroke-linejoin="round"
                  stroke-width="2"
                  d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z"
                />
                <path
                  v-else
                  stroke-linecap="round"
                  stroke-linejoin="round"
                  stroke-width="2"
                  d="M7 21h10a2 2 0 002-2V9.414a1 1 0 00-.293-.707l-5.414-5.414A1 1 0 0012.586 3H7a2 2 0 00-2 2v14a2 2 0 002 2z"
                />
              </svg>
            </span>

            <!-- 名称 + 元信息 -->
            <div class="flex-1 min-w-0">
              <p class="group-row-title truncate">{{ entry.name }}</p>
              <p class="group-row-sub mt-0.5 truncate">
                {{ entry.isDir ? '—' : formatBytes(entry.size, t) }}
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

      <!-- 悬浮操作按钮（设置 / 上传）：锚定列表底部右下角；多选时隐藏避免遮挡 -->
      <div
        v-if="fs.selectedCount.value === 0"
        class="absolute right-4 bottom-4 flex flex-col items-center gap-3"
      >
        <button
          class="ft-fab ft-fab-secondary"
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
        <button
          class="ft-fab ft-fab-primary"
          :title="t('transfer.topbar.uploadFile')"
          @click="uploadFile()"
        >
          <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-8l-4-4m0 0L8 8m4-4v12" />
          </svg>
        </button>
      </div>

      <!-- 为 FAB 让出底部滚动空间 -->
      <div class="h-20 flex-shrink-0"></div>
    </div>

    <!-- 底部：多选操作条 + 迷你传输条 -->
    <div class="flex-shrink-0 border-t border-[var(--mobile-border)] bg-[var(--mobile-bg-secondary)]/90 backdrop-blur-xl">
      <!-- 多选操作条 -->
      <div v-if="fs.selectedCount.value > 0" class="flex items-center gap-3 px-4 py-2.5">
        <button
          class="flex-shrink-0 ft-touch-btn px-3 rounded-lg ft-btn-neutral text-[var(--mobile-text-secondary)] active:opacity-80"
          :style="{ fontSize: 'clamp(0.75rem, 0.8125rem + (100vw - 360px) / 800 * 0.0625rem, 0.875rem)' }"
          @click="fs.clearSelection()"
        >
          {{ t('transfer.table.clearSelection') }}
        </button>
        <button
          class="flex-1 ft-touch-btn px-3 rounded-xl text-white bg-[var(--mobile-accent)] active:opacity-80 transition-opacity flex items-center justify-center gap-1.5"
          :style="{ fontSize: 'clamp(0.75rem, 0.8125rem + (100vw - 360px) / 800 * 0.0625rem, 0.875rem)' }"
          @click="downloadSelected()"
        >
          <svg class="w-4 h-4 flex-shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4" />
          </svg>
          <span class="truncate">{{ downloadLabel }}</span>
        </button>
      </div>

      <!-- 迷你传输条 -->
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
    </div>

    <!-- 队列 bottom sheet -->
    <TaskQueueSheet
      :open="queueOpen"
      :tasks="tasks.tasks.value"
      :speed-map="tasks.speedMap.value"
      :total-speed="tasks.totalSpeed.value"
      :resumable-count="tasks.resumableCount.value"
      :t="t"
      @close="queueOpen = false"
      @pause="(id) => tasks.pause(id)"
      @resume="(id) => tasks.resume(id)"
      @cancel="(id) => tasks.cancel(id)"
      @retry="(id) => tasks.retry(id)"
      @resume-all="() => tasks.resumeAll()"
    />
  </div>
</template>

<style scoped>
/* 对端名称：流式字号 */
.ft-peer-name {
  font-size: clamp(0.875rem, 0.9375rem + (100vw - 360px) / 800 * 0.0625rem, 1rem);
  font-weight: 500;
}

/* 在线/离线状态文字 */
.ft-peer-status {
  font-size: clamp(0.6875rem, 0.75rem + (100vw - 360px) / 800 * 0.0625rem, 0.8125rem);
  color: var(--mobile-text-muted);
}

/* 面包屑文字 */
.ft-breadcrumb-item {
  font-size: clamp(0.75rem, 0.8125rem + (100vw - 360px) / 800 * 0.0625rem, 0.875rem);
  color: var(--mobile-accent);
}

.ft-breadcrumb-current {
  color: var(--mobile-text-primary);
  font-weight: 500;
}

/* 文件列表正文 */
.ft-body-text {
  font-size: clamp(0.8125rem, 0.875rem + (100vw - 360px) / 800 * 0.0625rem, 0.9375rem);
}

/* 迷你传输条文字 */
.ft-mini-text {
  font-size: clamp(0.75rem, 0.8125rem + (100vw - 360px) / 800 * 0.0625rem, 0.875rem);
}

/* 多选勾选框 */
.ft-checkbox {
  width: 1.375rem;
  height: 1.375rem;
  border-radius: 9999px;
  border: 2px solid var(--mobile-border-hover);
  display: flex;
  align-items: center;
  justify-content: center;
  transition: all 0.15s ease;
}

.ft-checkbox-checked {
  background: var(--mobile-accent);
  border-color: var(--mobile-accent);
  color: var(--mobile-text-on-accent);
}

/* 离线状态点 */
.dot-zinc {
  width: 6px;
  height: 6px;
  border-radius: 9999px;
  background: var(--mobile-text-disabled);
}

/* 悬浮操作按钮（FAB）：上传主按钮 + 设置次按钮，圆形 + 悬浮阴影 */
.ft-fab {
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: 9999px;
  box-shadow: var(--mobile-card-shadow-hover);
  transition: opacity 0.15s ease;
  -webkit-tap-highlight-color: transparent;
}

.ft-fab:active {
  opacity: 0.8;
}

.ft-fab-primary {
  width: clamp(3.25rem, 3.5rem + (100vw - 400px) / 800 * 0.25rem, 3.75rem);
  height: clamp(3.25rem, 3.5rem + (100vw - 400px) / 800 * 0.25rem, 3.75rem);
  background: var(--mobile-accent);
  color: var(--mobile-text-on-accent);
}

.ft-fab-secondary {
  width: 2.75rem;
  height: 2.75rem;
  background: var(--mobile-bg-elevated);
  border: 1px solid var(--mobile-border);
  color: var(--mobile-text-secondary);
}
</style>
