<script setup lang="ts">
/**
 * FileTransferView — 文件传输浏览主页面 (Mobile)
 *
 * 结构（spec 9.2）：
 *   顶栏（对端名+在线点+设置）→ 面包屑 → Material 文件列表（图标+名称+元信息+多选勾选）
 *   → 多选时底部主按钮「下载到手机（N 项 · 总大小）」→ 迷你传输条（常驻）
 *   → 点击展开 bottom sheet 完整队列。
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
})

onUnmounted(() => {
  tasks.stop()
})
</script>

<template>
  <div class="ft-view h-full flex flex-col bg-[var(--mobile-bg-primary)]">
    <!-- 顶栏：对端名 + 在线状态 + 操作按钮 -->
    <div class="flex-shrink-0 flex items-center gap-3 px-4 pt-3 pb-2">
      <div class="flex items-center gap-2 flex-1 min-w-0">
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
      <button
        class="flex-shrink-0 p-1 text-[var(--mobile-text-secondary)] active:opacity-80 transition-colors"
        @click="uploadFile()"
      >
        <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-8l-4-4m0 0L8 8m4-4v12" />
        </svg>
      </button>
      <button
        class="flex-shrink-0 p-1 text-[var(--mobile-text-secondary)] active:opacity-80 transition-colors"
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

    <!-- 文件列表 -->
    <div class="flex-1 overflow-y-auto min-h-0 px-4 overscroll-behavior-none">
      <!-- 加载态 -->
      <div v-if="fs.loading.value" class="py-10 text-center">
        <p class="ft-body-text text-[var(--mobile-text-muted)]">{{ t('transfer.table.loading') }}</p>
      </div>

      <!-- 空态 -->
      <div v-else-if="fs.entries.value.length === 0" class="py-10 text-center">
        <p class="ft-body-text text-[var(--mobile-text-muted)]">
          {{ fs.error.value ? t('transfer.table.dirUnavailable') : t('transfer.table.empty') }}
        </p>
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
</style>
