<script setup lang="ts">
/**
 * BrowseTab — 浏览对端共享目录 tab
 *
 * 两级结构：共享根清单 → 根内目录树（useRemoteFs 提供）。文件行可勾选，
 * 多选后的「下载」动作交由父级底栏执行（本组件只负责选择与导航）。
 * 空态按场景区分（空目录 / 未共享 / 目录不可用 / 存储权限提示），
 * 下拉刷新沿用旧版手势参数（触发 56px、阻尼 0.45）。
 */
import { computed, ref } from 'vue'
import type { RemoteEntry } from '../types'
import { formatBytes } from '../utils/format'
import FileTypeIcon from './FileTypeIcon.vue'
import EmptyState from './EmptyState.vue'

type Translate = (key: string, params?: Record<string, any>) => string

const props = defineProps<{
  entries: RemoteEntry[]
  loading: boolean
  error: string | null
  notice: string | null
  crumbs: string[]
  /** 当前所在共享根（null = 处于根清单层） */
  currentRoot: { id: string; name: string } | null
  selected: Set<string>
  selectedCount: number
  /** 是否存在具备传输能力的对端（决定空态与可发送性） */
  peerOnline: boolean
  t: Translate
}>()

const emit = defineEmits<{
  (e: 'cd', name: string): void
  (e: 'up'): void
  (e: 'go-root'): void
  (e: 'go-to', index: number): void
  (e: 'toggle', name: string): void
  (e: 'toggle-all'): void
  (e: 'clear-selection'): void
  (e: 'refresh'): void
  (e: 'query-peer'): void
}>()

const t = props.t

/** 当前目录内文件数（勾选框只对文件生效） */
const fileEntries = computed(() => props.entries.filter((e) => !e.isDir))

/** 全选态（仅当前目录文件） */
const allFilesSelected = computed(() =>
  fileEntries.value.length > 0 && fileEntries.value.every((e) => props.selected.has(e.name)),
)

/** 当前目录文件总大小（勾选态底栏复用） */
const dirTotalSize = computed(() =>
  fileEntries.value.reduce((sum, e) => sum + (e.size > 0 ? e.size : 0), 0),
)

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
  if (!el || el.scrollTop > 0 || props.loading || pullState.value === 'refreshing') return
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

async function onPullEnd(): Promise<void> {
  if (!pullingActive.value) return
  pullingActive.value = false
  if (pullState.value === 'refreshing') return
  if (pullState.value === 'ready') {
    pullState.value = 'refreshing'
    pullDistance.value = PULL_TRIGGER
    emit('refresh')
  } else {
    pullState.value = 'idle'
    pullDistance.value = 0
  }
}

function onRefreshDone(): void {
  if (pullState.value === 'refreshing') {
    pullState.value = 'idle'
    pullDistance.value = 0
  }
}

// ==================== 空态 ====================

type EmptyKind = 'empty' | 'notSharing' | 'error'

const emptyKind = computed<EmptyKind>(() => {
  if (props.error) return 'error'
  if (!props.peerOnline && !props.currentRoot) return 'notSharing'
  return 'empty'
})

const emptyTitle = computed(() => {
  if (props.notice) return t('transfer.notice.storageAccessTitle')
  switch (emptyKind.value) {
    case 'error': return t('transfer.table.dirUnavailable')
    case 'notSharing': return t('transfer.peer.notSharing')
    default: return t('transfer.table.empty')
  }
})

const emptyHint = computed(() => {
  if (props.notice) return t('transfer.notice.storageAccess')
  switch (emptyKind.value) {
    case 'error': return t('transfer.empty.unavailableHint')
    case 'notSharing': return t('transfer.empty.notSharingHint')
    default: return t('transfer.empty.emptyDirHint')
  }
})

const emptyTone = computed<'neutral' | 'warning' | 'error'>(() => {
  if (props.notice) return 'warning'
  return emptyKind.value === 'error' ? 'error' : 'neutral'
})

const emptyIconPath = computed(() => {
  if (props.notice) return 'M12 9v2m0 4h.01M12 3l9.5 16.5H2.5L12 3z'
  switch (emptyKind.value) {
    case 'error': return 'M12 8v4m0 4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z'
    case 'notSharing': return 'M4.72 4.39a9.5 9.5 0 0114.56 0M7.82 5.52a6.5 6.5 0 018.36 0M10.8 7.21a3.5 3.5 0 012.4 0M12 20h.01'
    default: return 'M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z'
  }
})

/** 点击行：目录进入；文件勾选 */
function onRowTap(entry: RemoteEntry): void {
  if (entry.isDir) emit('cd', entry.name)
  else emit('toggle', entry.name)
}

/** 下拉箭头文案 */
const pullLabel = computed(() => {
  if (pullState.value === 'refreshing') return t('transfer.pull.refreshing')
  if (pullState.value === 'ready') return t('transfer.pull.ready')
  return t('transfer.pull.pull')
})
</script>

<template>
  <div class="flex-1 min-h-0 flex flex-col">
    <!-- 存储权限提示横幅（对端共享目录位于 Android 顶层存储时） -->
    <div v-if="notice" class="fv2-notice">
      <svg fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 9v2m0 4h.01M12 3l9.5 16.5H2.5L12 3z" />
      </svg>
      <div class="fv2-notice-body">
        <p class="fv2-notice-title">{{ t('transfer.notice.storageAccessTitle') }}</p>
        <p class="fv2-notice-text">{{ t('transfer.notice.storageAccess') }}</p>
      </div>
    </div>

    <!-- 面包屑（水平滚动，末段高亮当前） -->
    <div class="fv2-crumbs">
      <button class="fv2-crumb" :class="{ 'fv2-crumb--current': crumbs.length === 0 }" @click="emit('go-root')">
        {{ t('transfer.breadcrumb.home') }}
      </button>
      <template v-for="(seg, i) in crumbs" :key="i">
        <svg class="fv2-crumb-sep" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5l7 7-7 7" />
        </svg>
        <button
          class="fv2-crumb"
          :class="{ 'fv2-crumb--current': i === crumbs.length - 1 }"
          @click="emit('go-to', i)"
        >
          <span class="truncate">{{ seg }}</span>
        </button>
      </template>
    </div>

    <!-- 目录工具行：文件数 + 全选切换（仅根内目录且有文件时显示） -->
    <div v-if="currentRoot && fileEntries.length > 0" class="flex items-center justify-between px-5 py-1">
      <span class="fv2-crumb">{{ fileEntries.length }} · {{ formatBytes(dirTotalSize, t) }}</span>
      <button class="fv2-btn-text" @click="emit('toggle-all')">
        <svg class="w-4 h-4 flex-shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7" />
        </svg>
        {{ allFilesSelected ? t('transfer.table.clearSelection') : t('transfer.table.selectAll') }}
      </button>
    </div>

    <!-- 滚动区：下拉刷新 + 列表/空态 -->
    <div
      ref="scrollEl"
      class="flex-1 overflow-y-auto min-h-0 overscroll-behavior-none"
      @touchstart.passive="onPullStart"
      @touchmove.passive="onPullMove"
      @touchend="onPullEnd"
      @touchcancel="onPullEnd"
    >
      <!-- 下拉刷新指示器：随位移露出，刷新中常驻 -->
      <div
        class="fv2-pull"
        :style="{ height: pullDistance + 'px' }"
      >
        <span v-if="pullState === 'refreshing'" class="fv2-spinner fv2-spinner--sm"></span>
        <svg
          v-else
          class="w-5 h-5 flex-shrink-0"
          :style="{ transform: pullState === 'ready' ? 'rotate(180deg)' : 'rotate(0deg)', transition: 'transform 0.2s ease', color: pullState === 'ready' ? 'var(--mobile-accent)' : 'var(--mobile-text-secondary)' }"
          fill="none"
          stroke="currentColor"
          viewBox="0 0 24 24"
        >
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 14l-7 7m0 0l-7-7m7 7V3" />
        </svg>
        <span class="fv2-mini-speed">{{ pullLabel }}</span>
      </div>

      <!-- 加载态 -->
      <div v-if="loading" class="h-full flex flex-col">
        <EmptyState :icon="''" :title="t('transfer.table.loading')" loading />
      </div>

      <!-- 空态 -->
      <div v-else-if="entries.length === 0" class="h-full flex flex-col">
        <EmptyState
          :icon="emptyIconPath"
          :title="emptyTitle"
          :hint="emptyHint"
          :tone="emptyTone"
          :action-label="emptyKind === 'empty' ? t('transfer.topbar.refresh') : t('transfer.topbar.queryPeer')"
          :action-icon="'M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15'"
          :action-variant="emptyKind === 'notSharing' ? 'primary' : 'neutral'"
          @action="emptyKind === 'empty' ? emit('refresh') : emit('query-peer')"
        />
      </div>

      <!-- 文件 / 目录行 -->
      <div v-else class="group-card">
        <button
          v-for="(entry, idx) in entries"
          :key="entry.name"
          class="fv2-file"
          :class="{
            'fv2-file--selected': !entry.isDir && selected.has(entry.name),
            'group-card-divider': false,
          }"
          :style="idx > 0 ? { borderTop: '1px solid var(--mobile-group-divider)' } : undefined"
          @click="onRowTap(entry)"
        >
          <!-- 类型图标（按扩展名匹配，目录/未知回退） -->
          <FileTypeIcon :name="entry.name" :is-dir="entry.isDir" />

          <div class="flex-1 min-w-0">
            <p class="fv2-file-name truncate">{{ entry.name }}</p>
            <p v-if="!entry.isDir && entry.size > 0" class="fv2-task-meta">{{ formatBytes(entry.size, t) }}</p>
          </div>

          <!-- 勾选框（仅文件） / 进入箭头（目录） -->
          <span v-if="!entry.isDir" class="fv2-check" :class="{ 'fv2-check--checked': selected.has(entry.name) }">
            <svg v-if="selected.has(entry.name)" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="3" d="M5 13l4 4L19 7" />
            </svg>
          </span>
          <svg
            v-else
            class="w-4 h-4 flex-shrink-0"
            style="color: var(--mobile-row-sub)"
            fill="none"
            stroke="currentColor"
            viewBox="0 0 24 24"
          >
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5l7 7-7 7" />
          </svg>
        </button>
      </div>
    </div>
  </div>
</template>

<style scoped>
/* 下拉刷新指示器：贴滚动容器顶部，高度随位移露出 */
.fv2-pull {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 0.5rem;
  overflow: hidden;
  color: var(--mobile-text-secondary);
  transition: height 0.25s cubic-bezier(0.4, 0, 0.2, 1);
}
</style>
