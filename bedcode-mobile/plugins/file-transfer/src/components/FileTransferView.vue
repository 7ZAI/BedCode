<script setup lang="ts">
/**
 * FileTransferView — 文件传输浏览主页面 (Mobile)
 *
 * 结构（spec 9.2）：
 *   顶栏（对端名+在线点+设置）→ 面包屑 → Material 文件列表（图标+名称+元信息+多选勾选）
 *   → 多选时底部主按钮「下载到手机（N 项 · 总大小）」→ 迷你传输条（常驻）
 *   → 点击展开 bottom sheet 完整队列。
 *
 * 业务逻辑全部在 composables（useTasks / useRemoteFs / useSettings），本组件只做 UI。
 * 上传入口为占位：移动 SDK 无文件选择 API，经 dialogs.showPrompt 手动输入本地绝对路径。
 *
 * 经宿主 PluginViewHost 渲染（provide pluginContext），故此处直接断言非空。
 */
import { inject, ref, onMounted, onUnmounted, computed } from 'vue'
import type { PluginContext } from '@bedcode/plugin-sdk-mobile'
import { useTasks } from '../composables/useTasks'
import { useRemoteFs } from '../composables/useRemoteFs'
import { useSettings } from '../composables/useSettings'
import { formatBytes, formatSpeed, progressPercent } from '../utils/format'
import TaskQueueSheet from './TaskQueueSheet.vue'
import SettingsSection from './SettingsSection.vue'

const context = inject<PluginContext>('pluginContext')!
const t = (key: string, params?: Record<string, any>) => context.i18n.t(key, params)

const tasks = useTasks(context)
const fs = useRemoteFs(context)
const settingsApi = useSettings(context)

/** 页面模式：浏览 / 设置 */
const viewMode = ref<'browse' | 'settings'>('browse')
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
 * 上传占位入口：移动 SDK 无文件选择 API，经 dialogs.showPrompt 手动输入本地路径。
 * 目标远端目录为当前面包屑目录，文件名取本地路径 basename。
 */
async function uploadFile(): Promise<void> {
  const localPath = await context.dialogs.showPrompt({
    title: t('transfer.dialog.uploadTitle'),
    message: t('transfer.settings.addRootHint'),
    inputPlaceholder: t('transfer.dialog.localPathPlaceholder'),
    confirmText: t('transfer.task.upload'),
    cancelText: t('transfer.dialog.cancel'),
  })
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
  void settingsApi.load()
})

onUnmounted(() => {
  tasks.stop()
})
</script>

<template>
  <div class="ft-view h-full flex flex-col bg-[var(--mobile-bg-primary)]">
    <!-- ==================== 设置模式 ==================== -->
    <template v-if="viewMode === 'settings'">
      <div class="flex items-center gap-2 px-4 pt-3 pb-2 flex-shrink-0">
        <button
          class="flex-shrink-0 p-1.5 -ml-1.5 text-[var(--mobile-text-secondary)] active:opacity-80 transition-opacity"
          @click="viewMode = 'browse'"
        >
          <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 19l-7-7 7-7" />
          </svg>
        </button>
        <h2 class="flex-1 text-base font-semibold text-[var(--mobile-text-primary)] truncate">
          {{ t('transfer.settings.title') }}
        </h2>
      </div>
      <div class="flex-1 overflow-y-auto min-h-0">
        <SettingsSection :settings-api="settingsApi" :t="t" />
      </div>
    </template>

    <!-- ==================== 浏览模式 ==================== -->
    <template v-else>
      <!-- 顶栏：对端名 + 在线点 + 设置 + 上传 -->
      <div class="flex items-center gap-2 px-4 pt-3 pb-2 flex-shrink-0">
        <div class="flex items-center gap-1.5 flex-1 min-w-0">
          <span
            class="w-2 h-2 rounded-full flex-shrink-0"
            :class="tasks.peerOnline.value ? 'bg-[var(--mobile-success)] shadow-[0_0_6px_rgba(16,185,129,0.6)]' : 'bg-[var(--mobile-text-disabled)]'"
          ></span>
          <span class="text-sm font-medium text-[var(--mobile-text-primary)] truncate">
            {{ peerLabel }}
          </span>
          <span class="text-xs text-[var(--mobile-text-muted)] flex-shrink-0">
            {{ tasks.peerOnline.value ? t('transfer.peer.online') : t('transfer.peer.offline') }}
          </span>
        </div>
        <button
          class="flex-shrink-0 p-1.5 text-[var(--mobile-text-secondary)] active:opacity-80 transition-opacity"
          @click="uploadFile()"
        >
          <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-8l-4-4m0 0L8 8m4-4v12" />
          </svg>
        </button>
        <button
          class="flex-shrink-0 p-1.5 text-[var(--mobile-text-secondary)] active:opacity-80 transition-opacity"
          @click="viewMode = 'settings'"
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
      <div class="flex items-center gap-1 px-4 py-2 flex-shrink-0 overflow-x-auto">
        <button
          class="flex-shrink-0 text-sm text-[var(--mobile-accent)] active:opacity-80"
          @click="fs.goRoot()"
        >
          {{ t('transfer.breadcrumb.home') }}
        </button>
        <template v-for="(seg, i) in fs.crumbs.value" :key="seg">
          <svg class="w-3.5 h-3.5 flex-shrink-0 text-[var(--mobile-text-disabled)]" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5l7 7-7 7" />
          </svg>
          <button
            class="flex-shrink-0 text-sm max-w-[7rem] truncate"
            :class="i === fs.crumbs.value.length - 1 ? 'text-[var(--mobile-text-primary)] font-medium' : 'text-[var(--mobile-accent)]'"
            @click="fs.goTo(i)"
          >
            {{ seg }}
          </button>
        </template>
      </div>

      <!-- 文件列表 -->
      <div class="flex-1 overflow-y-auto min-h-0 overscroll-behavior-none">
        <!-- 加载态 -->
        <div v-if="fs.loading.value" class="px-4 py-10 text-center">
          <p class="text-sm text-[var(--mobile-text-muted)]">{{ t('transfer.table.loading') }}</p>
        </div>

        <!-- 空态 -->
        <div v-else-if="fs.entries.value.length === 0" class="px-4 py-10 text-center">
          <p class="text-sm text-[var(--mobile-text-muted)]">
            {{ fs.error.value ? t('transfer.table.dirUnavailable') : t('transfer.table.empty') }}
          </p>
        </div>

        <!-- 目录/文件行 -->
        <button
          v-for="entry in fs.entries.value"
          :key="entry.name"
          class="w-full flex items-center gap-3 px-4 py-3 text-left active:bg-[var(--mobile-bg-tertiary)] transition-colors"
          @click="onRowTap(entry)"
        >
          <!-- 类型图标 -->
          <div
            class="w-10 h-10 rounded-lg flex items-center justify-center flex-shrink-0"
            :class="entry.isDir ? 'bg-[var(--mobile-accent-muted)] text-[var(--mobile-accent)]' : 'bg-[var(--mobile-bg-elevated)] text-[var(--mobile-text-muted)]'"
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
          </div>

          <!-- 名称 + 元信息 -->
          <div class="flex-1 min-w-0">
            <p class="text-[0.9375rem] text-[var(--mobile-text-primary)] truncate">{{ entry.name }}</p>
            <p class="text-xs text-[var(--mobile-text-muted)] mt-0.5 truncate">
              {{ entry.isDir ? '—' : formatBytes(entry.size, t) }}
            </p>
          </div>

          <!-- 勾选框（仅文件） -->
          <span
            v-if="!entry.isDir"
            class="flex-shrink-0 w-6 h-6 rounded-full border-2 flex items-center justify-center transition-colors"
            :class="fs.selected.value.has(entry.name) ? 'bg-[var(--mobile-accent)] border-[var(--mobile-accent)] text-white' : 'border-[var(--mobile-border-hover)]'"
          >
            <svg v-if="fs.selected.value.has(entry.name)" class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="3" d="M5 13l4 4L19 7" />
            </svg>
          </span>
          <svg v-else class="w-5 h-5 flex-shrink-0 text-[var(--mobile-text-disabled)]" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5l7 7-7 7" />
          </svg>
        </button>
      </div>

      <!-- 底部：多选操作条 + 迷你传输条 -->
      <div class="flex-shrink-0 border-t border-[var(--mobile-border)] bg-[var(--mobile-bg-secondary)]/90 backdrop-blur-xl">
        <!-- 多选操作条 -->
        <div v-if="fs.selectedCount.value > 0" class="flex items-center gap-3 px-4 py-2.5">
          <button
            class="flex-shrink-0 ft-touch-btn px-3 rounded-lg text-sm text-[var(--mobile-text-secondary)] border border-[var(--mobile-border)] active:opacity-80"
            @click="fs.clearSelection()"
          >
            {{ t('transfer.table.clearSelection') }}
          </button>
          <button
            class="flex-1 ft-touch-btn px-3 rounded-xl text-sm font-medium text-white bg-[var(--mobile-accent)] active:opacity-80 transition-opacity gap-1.5"
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
              <p class="text-sm text-[var(--mobile-text-primary)] truncate">
                {{ primaryTask.remotePath.split('/').pop() }}
              </p>
              <span class="text-xs text-[var(--mobile-text-muted)] flex-shrink-0">
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
    </template>
  </div>
</template>
