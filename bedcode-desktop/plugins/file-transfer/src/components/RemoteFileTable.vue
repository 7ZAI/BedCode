<script setup lang="ts">
/**
 * RemoteFileTable — 远端目录表格（左侧栏）
 *
 * 面包屑导航 + 复选框多选 + 类型图标 + 名称/大小/修改时间。
 * 目录不可勾选（双击进入），仅文件参与下载选择。纯展示组件，
 * 目录状态全部来自 props，交互经 emit 交给父级 composable。
 */
import { computed, inject } from 'vue'
import type { PluginContext } from '@bedcode/plugin-sdk-desktop'
import type { RemoteEntry } from '../types'
import type { Crumb } from '../composables/useRemoteFs'
import { formatBytes, formatModified } from '../utils/format'

const context = inject<PluginContext>('pluginContext')!
const t = (key: string, params?: Record<string, any>) => context.i18n.t(key, params)

const props = defineProps<{
  entries: RemoteEntry[]
  loading: boolean
  /** 目录不可用的 i18n key（空 = 无错误） */
  errorKey: string
  breadcrumb: Crumb[]
  selectedNames: string[]
}>()

const emit = defineEmits<{
  (e: 'enter', entry: RemoteEntry): void
  (e: 'navigate', index: number): void
  (e: 'toggle', name: string): void
  (e: 'toggleAll'): void
}>()

/** 是否全部文件已被选中（表头全选框状态） */
const allSelected = computed(() => {
  const fileNames = props.entries.filter(e => !e.isDir)
  return fileNames.length > 0 && fileNames.every(e => props.selectedNames.includes(e.name))
})

/** 部分文件被选中（表头半选态：驱动原生 checkbox 的 indeterminate 属性） */
const someSelected = computed(() => {
  const fileNames = props.entries.filter(e => !e.isDir)
  const selectedCount = fileNames.filter(e => props.selectedNames.includes(e.name)).length
  return selectedCount > 0 && !allSelected.value
})

/** 当前是否根目录（面包屑仅剩根节点） */
const isRoot = computed(() => props.breadcrumb.length <= 1)

/** 文件类型 → 图标 SVG path 数据（目录/音频/视频/图片/压缩包/文档/通用） */
const FILE_ICONS: Record<string, string> = {
  dir: 'M22 19a2 2 0 01-2 2H4a2 2 0 01-2-2V5a2 2 0 012-2h5l2 3h9a2 2 0 012 2z',
  audio: 'M9 18V5l12-2v13',
  video: 'M15 10l4.553-2.276A1 1 0 0121 8.618v6.764a1 1 0 01-1.447.894L15 14M5 18h14a2 2 0 002-2V8a2 2 0 00-2-2H5a2 2 0 00-2 2v8a2 2 0 002 2z',
  image: 'M4 15l4-4 5 5M20 4l-7.5 7.5M18 20l-7.5-7.5M3 3l18 18',
  archive: 'M21 8l-9-5-9 5v8l9 5 9-5V8zM3 8l9 5 9-5M12 13v8',
  doc: 'M14 2H6a2 2 0 00-2 2v16a2 2 0 002 2h12a2 2 0 002-2V8zM14 2v6h6M16 13H8M16 17H8M10 9H8',
  file: 'M14 2H6a2 2 0 00-2 2v16a2 2 0 002 2h12a2 2 0 002-2V8zM14 2v6h6',
}

/** 分类 → 展示色（遵循宿主功能色，柔和区分；目录用品牌色） */
const FILE_ICON_COLORS: Record<string, string> = {
  dir: 'var(--color-primary)',
  audio: 'var(--color-warning)',
  video: 'var(--color-success)',
  image: 'var(--color-primary)',
  archive: 'var(--color-warning)',
  doc: 'var(--text-secondary)',
  file: 'var(--text-tertiary)',
}

/** 文件类型 → 分类（未知扩展名归类通用文件） */
function fileKind(entry: RemoteEntry): string {
  if (entry.isDir) return 'dir'
  const name = entry.name.toLowerCase()
  if (/\.(mp3|flac|wav|m4a|aac|ogg|wma)$/.test(name)) return 'audio'
  if (/\.(mp4|mkv|avi|mov|wmv|flv|webm)$/.test(name)) return 'video'
  if (/\.(jpg|jpeg|png|gif|webp|bmp|svg|heic)$/.test(name)) return 'image'
  if (/\.(zip|rar|7z|tar|gz|bz2|xz)$/.test(name)) return 'archive'
  if (/\.(txt|md|pdf|docx?|xlsx?|pptx?)$/.test(name)) return 'doc'
  return 'file'
}

/** 文件类型图标（目录用文件夹；其余按扩展名分类，未知归类通用文件） */
function fileIcon(entry: RemoteEntry): string {
  return FILE_ICONS[fileKind(entry)]
}

/** 文件类型图标展示色 */
function fileIconColor(entry: RemoteEntry): string {
  return FILE_ICON_COLORS[fileKind(entry)]
}

/** 双击行：目录进入，文件无操作 */
function onRowDblClick(entry: RemoteEntry): void {
  if (entry.isDir) emit('enter', entry)
}
</script>

<template>
  <div class="ft-browse">
    <!-- 面包屑导航（分隔符仅出现在非末位节点之后） -->
    <div class="ft-crumbbar">
      <template v-for="(crumb, i) in breadcrumb" :key="crumb.path">
        <button
          class="ft-crumb"
          :class="{ 'ft-crumb--current': i === breadcrumb.length - 1 }"
          @click="emit('navigate', i)"
        >
          {{ i === 0 ? t(crumb.name) : crumb.name }}
        </button>
        <span v-if="i < breadcrumb.length - 1" class="ft-crumb-sep">/</span>
      </template>
    </div>

    <!-- 加载 / 错误 / 空态 -->
    <div v-if="loading" class="ft-loading">{{ t('transfer.table.loading') }}</div>
    <div v-else-if="errorKey" class="ft-empty">{{ t(errorKey) }}</div>
    <div v-else-if="entries.length === 0" class="ft-empty">
      <!-- 根目录空 = 对方尚未设置共享目录（spec §8 默认安全），子目录空 = 普通空目录 -->
      {{ isRoot ? t('transfer.peer.noSharedRoots') : t('transfer.table.empty') }}
    </div>

    <!-- 目录表格 -->
    <div v-else class="ft-table-wrap">
      <table class="ft-table">
        <thead>
          <tr>
            <th style="width: 32px">
              <label class="ft-checkbox">
                <!-- 原生 checkbox 仅作交互内核（绝对定位覆盖 + 透明），change/indeterminate 语义不变 -->
                <input
                  type="checkbox"
                  class="ft-checkbox-input"
                  :checked="allSelected"
                  :indeterminate="someSelected"
                  @change="emit('toggleAll')"
                />
                <span class="ft-checkbox-box" aria-hidden="true">
                  <svg class="ft-checkbox-mark" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3.5" stroke-linecap="round" stroke-linejoin="round">
                    <path d="M5 13l4 4L19 7" />
                  </svg>
                  <span class="ft-checkbox-indet"></span>
                </span>
              </label>
            </th>
            <th>{{ t('transfer.table.name') }}</th>
            <th style="width: 90px">{{ t('transfer.table.size') }}</th>
            <th style="width: 130px">{{ t('transfer.table.modified') }}</th>
          </tr>
        </thead>
        <tbody>
          <tr
            v-for="entry in entries"
            :key="entry.name"
            :class="{ 'ft-row--sel': !entry.isDir && selectedNames.includes(entry.name) }"
            @dblclick="onRowDblClick(entry)"
          >
            <td>
              <label
                class="ft-checkbox"
                :class="{ 'ft-checkbox--disabled': entry.isDir }"
              >
                <!-- 原生 checkbox 仅作交互内核；目录行隐藏（与目录不可勾选语义一致） -->
                <input
                  type="checkbox"
                  class="ft-checkbox-input"
                  :disabled="entry.isDir"
                  :checked="!entry.isDir && selectedNames.includes(entry.name)"
                  @change="emit('toggle', entry.name)"
                />
                <span class="ft-checkbox-box" aria-hidden="true">
                  <svg class="ft-checkbox-mark" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3.5" stroke-linecap="round" stroke-linejoin="round">
                    <path d="M5 13l4 4L19 7" />
                  </svg>
                </span>
              </label>
            </td>
            <td>
              <div class="ft-fname">
                <span class="ft-ico" :style="{ color: fileIconColor(entry) }">
                  <svg fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" :d="fileIcon(entry)" />
                  </svg>
                </span>
                <span class="ft-fname-text">{{ entry.name }}</span>
              </div>
            </td>
            <td class="ft-dim">{{ entry.isDir ? '—' : formatBytes(entry.size) }}</td>
            <td class="ft-dim">{{ formatModified(entry.mtime) }}</td>
          </tr>
        </tbody>
      </table>
    </div>
  </div>
</template>
