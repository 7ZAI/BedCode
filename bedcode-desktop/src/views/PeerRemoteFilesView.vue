<template>
  <div class="h-full flex flex-col bg-[var(--bg-page)]">
    <!-- ==================== 工具栏页头：返回 + 标题 + 刷新 ==================== -->
    <div class="wb-toolbar">
      <div class="flex items-center gap-3 min-w-0">
        <button class="wb-btn-ghost flex-shrink-0" @click="goBack">
          <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path
              stroke-linecap="round"
              stroke-linejoin="round"
              stroke-width="1.75"
              d="M10.5 19.5L3 12m0 0l7.5-7.5M3 12h18"
            />
          </svg>
          {{ t('peers.files.back') }}
        </button>
        <div class="min-w-0">
          <h2 class="text-[calc(13px*var(--ui-scale))] font-semibold text-[var(--text-primary)] truncate">
            {{ t('peers.files.title') }}
          </h2>
          <span class="text-[calc(11px*var(--ui-scale))] text-[var(--text-tertiary)] truncate">
            {{ deviceName }}
          </span>
        </div>
      </div>
      <div class="flex items-center gap-2 flex-shrink-0">
        <template v-if="activeRoot">
          <button class="wb-btn-ghost" :disabled="loading || pulling" @click="refresh">
            {{ t('peers.files.refresh') }}
          </button>
          <button
            v-if="fileEntries.length > 0"
            class="wb-btn-ghost"
            :disabled="loading || pulling"
            @click="toggleAll"
          >
            {{ allFilesSelected ? t('peers.files.clearSelect') : t('peers.files.selectAll') }}
          </button>
        </template>
      </div>
    </div>

    <div class="flex-1 overflow-auto px-6 py-4">
      <!-- ==================== 共享根选择（对端暴露多个共享目录时） ==================== -->
      <template v-if="!activeRoot">
        <p class="mb-3 text-[calc(12px*var(--ui-scale))] text-[var(--text-secondary)]">
          {{ t('peers.files.rootsTitle', { name: deviceName }) }}
        </p>
        <div
          v-if="!loading && errorKey === '' && roots.length === 0"
          class="rounded-[10px] border border-[var(--border)] bg-[var(--bg-card)] px-4 py-10 text-center"
        >
          <p class="text-[calc(13px*var(--ui-scale))] text-[var(--text-secondary)]">
            {{ t('peers.files.noRoots') }}
          </p>
        </div>
        <div v-else class="space-y-1.5 max-w-3xl">
          <button
            v-for="root in roots"
            :key="root.id"
            class="w-full rounded-[10px] bg-[var(--bg-card)] px-4 transition-colors duration-200 hover:border-[var(--border)] border border-transparent active:opacity-90"
            :disabled="loading"
            @click="selectRoot(root)"
          >
            <span class="flex items-center gap-3 h-11">
              <svg
                class="w-4 h-4 flex-shrink-0 text-[var(--color-primary)]"
                fill="none"
                stroke="currentColor"
                viewBox="0 0 24 24"
              >
                <path
                  stroke-linecap="round"
                  stroke-linejoin="round"
                  stroke-width="1.75"
                  d="M2.25 12.75V12A2.25 2.25 0 014.5 9.75h15A2.25 2.25 0 0121.75 12v.75m-8.69-6.44l-2.12-2.12a1.5 1.5 0 00-1.061-.44H4.5A2.25 2.25 0 002.25 6v12a2.25 2.25 0 002.25 2.25h15A2.25 2.25 0 0021.75 18V9a2.25 2.25 0 00-2.25-2.25h-5.379a1.5 1.5 0 01-1.06-.44z"
                />
              </svg>
              <span class="flex-1 min-w-0 truncate text-left text-[calc(13px*var(--ui-scale))] text-[var(--text-primary)]">
                {{ root.name }}
              </span>
            </span>
          </button>
        </div>
      </template>

      <!-- ==================== 已进入共享根：权限提示 / 面包屑 / 条目列表 ==================== -->
      <template v-else>
        <!-- 权限过滤提示（沿用既有 notice 语义） -->
        <div
          v-if="filteredNotice"
          class="mb-3 rounded-[10px] border border-amber-500/40 bg-amber-500/10 px-4 py-2.5 flex items-center gap-2.5"
        >
          <svg class="w-4 h-4 flex-shrink-0 text-amber-600 dark:text-amber-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path
              stroke-linecap="round"
              stroke-linejoin="round"
              stroke-width="1.75"
              d="M12 9v3.75m-9.303 3.376c-.866 1.5.217 3.374 1.948 3.374h14.71c1.73 0 2.813-1.874 1.948-3.374L13.949 3.378c-.866-1.5-3.032-1.5-3.898 0L2.697 16.126zM12 15.75h.007v.008H12v-.008z"
            />
          </svg>
          <span class="text-[calc(12px*var(--ui-scale))] text-[var(--text-secondary)]">
            {{ t('peers.files.noticeFiltered') }}
          </span>
        </div>

        <!-- 面包屑导航 -->
        <nav class="mb-3 flex items-center gap-1 flex-wrap text-[calc(12px*var(--ui-scale))] min-w-0">
          <template v-for="(crumb, index) in breadcrumb" :key="crumb.path">
            <span v-if="index > 0" class="text-[var(--text-tertiary)]">/</span>
            <button
              class="max-w-[220px] truncate rounded px-1 py-0.5 transition-colors duration-200"
              :class="
                index === breadcrumb.length - 1
                  ? 'font-medium text-[var(--text-primary)]'
                  : 'text-[var(--color-primary)] hover:bg-[var(--bg-hover)] active:opacity-80'
              "
              :disabled="loading || pulling"
              @click="navigateTo(index)"
            >
              {{ index === 0 ? t('peers.files.breadcrumbHome') : crumb.name }}
            </button>
          </template>
        </nav>

        <!-- 错误 / 空态 / 加载 -->
        <div
          v-if="errorKey"
          class="rounded-[10px] border border-[var(--border)] bg-[var(--bg-card)] px-4 py-10 text-center"
        >
          <p class="text-[calc(13px*var(--ui-scale))] text-[var(--text-secondary)]">
            {{ t(errorKey) }}
          </p>
        </div>
        <div
          v-else-if="!loading && entries.length === 0"
          class="rounded-[10px] border border-[var(--border)] bg-[var(--bg-card)] px-4 py-10 text-center"
        >
          <p class="text-[calc(13px*var(--ui-scale))] text-[var(--text-secondary)]">
            {{ t('peers.files.empty') }}
          </p>
        </div>

        <!-- 条目列表：目录行进入 + 尾部勾选钮；文件行点选 -->
        <div v-else class="space-y-1.5 max-w-3xl">
          <div
            v-for="entry in entries"
            :key="entry.name"
            class="group rounded-[10px] border border-transparent bg-[var(--bg-card)] px-4 transition-colors duration-200 hover:border-[var(--border)]"
            :class="{ 'ring-1 ring-[var(--color-primary)]': isSelected(entry.name) }"
          >
            <div class="flex items-center gap-3 h-11">
              <!-- 主区：目录点击进入，文件点击切换选中 -->
              <button
                class="flex-1 min-w-0 flex items-center gap-3 h-full text-left"
                :disabled="loading || pulling"
                @click="entry.isDir ? enterDir(entry) : toggleSelect(entry.name)"
              >
                <!-- 目录/文件图标 -->
                <svg
                  class="w-4 h-4 flex-shrink-0"
                  :class="entry.isDir ? 'text-[var(--color-primary)]' : 'text-[var(--text-tertiary)]'"
                  fill="none"
                  stroke="currentColor"
                  viewBox="0 0 24 24"
                >
                  <path
                    v-if="entry.isDir"
                    stroke-linecap="round"
                    stroke-linejoin="round"
                    stroke-width="1.75"
                    d="M2.25 12.75V12A2.25 2.25 0 014.5 9.75h15A2.25 2.25 0 0121.75 12v.75m-8.69-6.44l-2.12-2.12a1.5 1.5 0 00-1.061-.44H4.5A2.25 2.25 0 002.25 6v12a2.25 2.25 0 002.25 2.25h15A2.25 2.25 0 0021.75 18V9a2.25 2.25 0 00-2.25-2.25h-5.379a1.5 1.5 0 01-1.06-.44z"
                  />
                  <path
                    v-else
                    stroke-linecap="round"
                    stroke-linejoin="round"
                    stroke-width="1.75"
                    d="M19.5 14.25v-2.625a3.375 3.375 0 00-3.375-3.375h-1.5A1.125 1.125 0 0113.5 7.125v-1.5a3.375 3.375 0 00-3.375-3.375H8.25m2.25 0H5.625c-.621 0-1.125.504-1.125 1.125v17.25c0 .621.504 1.125 1.125 1.125h12.75c.621 0 1.125-.504 1.125-1.125V11.25a9 9 0 00-9-9z"
                  />
                </svg>
                <span class="flex-1 min-w-0 truncate text-[calc(13px*var(--ui-scale))] text-[var(--text-primary)]">
                  {{ entry.name }}
                </span>
                <span
                  v-if="!entry.isDir"
                  class="flex-shrink-0 text-[calc(11px*var(--ui-scale))] text-[var(--text-tertiary)] wb-mono"
                >
                  {{ formatBytes(entry.size) }}
                </span>
              </button>

              <!-- 勾选钮（目录亦可整选）：自绘视觉，无原生控件外观 -->
              <button
                class="flex-shrink-0 w-5 h-5 rounded border flex items-center justify-center transition-colors duration-200"
                :class="
                  isSelected(entry.name)
                    ? 'bg-[var(--color-primary)] border-[var(--color-primary)]'
                    : 'border-[var(--border)] bg-transparent hover:border-[var(--color-primary)] active:opacity-80'
                "
                :aria-label="t('peers.files.toggleSelect')"
                :disabled="loading || pulling"
                @click="toggleSelect(entry.name)"
              >
                <svg
                  v-if="isSelected(entry.name)"
                  class="w-3 h-3 text-[var(--color-primary-contrast)]"
                  fill="none"
                  stroke="currentColor"
                  viewBox="0 0 24 24"
                >
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2.5" d="M4.5 12.75l6 6 9-13.5" />
                </svg>
              </button>
            </div>
          </div>
        </div>
      </template>
    </div>

    <!-- ==================== 底部选择操作条：计数 + 字节量 + 拉取 ==================== -->
    <Transition name="pf-slide-up">
      <div
        v-if="hasSelection"
        class="flex-shrink-0 border-t border-[var(--border)] bg-[var(--bg-card)] px-6 py-3 flex items-center gap-4"
      >
        <span class="text-[calc(12px*var(--ui-scale))] text-[var(--text-secondary)] min-w-0 truncate">
          {{
            pulling
              ? t('peers.files.enumerating')
              : t('peers.files.selectedCount', { count: selectedNames.length, size: formatBytes(selectedBytes) })
          }}
        </span>
        <div class="flex-1"></div>
        <button class="wb-btn-ghost flex-shrink-0" :disabled="pulling" @click="clearSelection">
          {{ t('peers.files.clearSelect') }}
        </button>
        <button
          class="wb-btn-primary flex-shrink-0"
          :disabled="pulling || loading"
          @click="handlePull"
        >
          {{ pulling ? t('peers.files.pulling') : t('peers.files.pull') }}
        </button>
      </div>
    </Transition>

    <!-- ==================== 入队结果轻提示 ==================== -->
    <Transition name="pf-fade">
      <div
        v-if="queuedHint"
        class="fixed bottom-20 left-1/2 -translate-x-1/2 z-30 rounded-md bg-[var(--bg-card)] border border-[var(--border)] shadow-lg px-4 py-2 text-[calc(12px*var(--ui-scale))] text-[var(--text-primary)]"
      >
        {{ queuedHint }}
      </div>
    </Transition>
  </div>
</template>

<script setup lang="ts">
/**
 * 远端共享目录浏览页 — 可信对端文件拉取入口（issue 11）
 *
 * 进入已连接对端的共享目录逐级下钻（目录优先、按名排序由引擎保证）；勾选
 * 文件或整个目录后一次性入队拉取到本机下载目录——进度与取消在「传输任务」
 * 页的正在接收分列呈现（复用任务体系）。只读：本页面无任何写操作入口。
 */
import { computed, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRoute, useRouter } from 'vue-router'
import {
  usePeerRemoteFiles,
  formatBytes,
} from '@/composables/usePeerRemoteFiles'

const { t } = useI18n()
const route = useRoute()
const router = useRouter()

const {
  deviceName,
  roots,
  activeRoot,
  breadcrumb,
  entries,
  loading,
  errorKey,
  filteredNotice,
  selectedNames,
  pulling,
  hasSelection,
  selectedBytes,
  open,
  selectRoot,
  enterDir,
  navigateTo,
  refresh,
  toggleSelect,
  toggleAll,
  clearSelection,
  pullSelection,
} = usePeerRemoteFiles()

/** 拉取入队成功提示（数秒后自动消失） */
const queuedHint = ref('')
let hintTimer: ReturnType<typeof setTimeout> | null = null

const fileEntries = computed(() => entries.value.filter((e) => !e.isDir))
const allFilesSelected = computed(
  () =>
    fileEntries.value.length > 0 &&
    fileEntries.value.every((e) => selectedNames.value.includes(e.name)),
)

function isSelected(name: string): boolean {
  return selectedNames.value.includes(name)
}

function goBack(): void {
  void router.push({ name: 'peer-devices' })
}

function showHint(key: string, count?: number): void {
  if (hintTimer) clearTimeout(hintTimer)
  queuedHint.value = count === undefined ? t(key) : t(key, { count })
  hintTimer = setTimeout(() => {
    queuedHint.value = ''
  }, 3200)
}

async function handlePull(): Promise<void> {
  try {
    const count = await pullSelection()
    if (count > 0) showHint('peers.files.pullQueued', count)
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error)
    console.error('[PeerRemoteFiles] pull failed:', error)
    if (message.includes('too-many-files')) showHint('peers.files.tooMany')
    else showHint('peers.files.pullFailed')
  }
}

onMounted(() => {
  const nodeId = String(route.params.nodeId ?? '')
  if (!nodeId) {
    void goBack()
    return
  }
  const name = String(route.query.name ?? nodeId.slice(0, 8))
  void open(nodeId, name)
})
</script>

<style scoped>
/* 选择条滑入：仅 transform/opacity，GPU 合成 */
.pf-slide-up-enter-active,
.pf-slide-up-leave-active {
  transition:
    transform 0.2s ease,
    opacity 0.2s ease;
}
.pf-slide-up-enter-from,
.pf-slide-up-leave-to {
  transform: translateY(100%);
  opacity: 0;
}

.pf-fade-enter-active,
.pf-fade-leave-active {
  transition: opacity 0.2s ease;
}
.pf-fade-enter-from,
.pf-fade-leave-to {
  opacity: 0;
}
</style>
