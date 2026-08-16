<script setup lang="ts">
/**
 * SharedDirSheet — 共享目录上传页（底部抽屉，M1 上传页 / M3 流直传）
 *
 * 两视图：
 *   1. 共享目录列表（SAF 授权条目 + 免授权特殊条目；失效条目提示重新授权）
 *   2. 目录树浏览（App 内遍历，免系统选择器；点文件即直接入队上传——
 *      M3 上传 SAF 流直传，无中转复制步骤，进度由任务队列展示）
 *
 * 视觉语言与 TaskQueueSheet 一致（抓把 / ft-sheet-panel / group-card / group-row），
 * 字号 clamp() 流式缩放；业务逻辑全部在 useSharedUpload。
 */
import { watch, computed } from 'vue'
import { inject } from 'vue'
import type { PluginContext } from '@bedcode/plugin-sdk-mobile'
import type { SharedRoot } from '../types'
import { KIND_PRIVATE_DOWNLOADS } from '../types'
import type { useSharedUpload } from '../composables/useSharedUpload'
import type { SharedEntry } from '../types'
import { formatBytes } from '../utils/format'
import FileTypeIcon from './FileTypeIcon.vue'

type UploadApi = ReturnType<typeof useSharedUpload>

const props = defineProps<{
  open: boolean
  upload: UploadApi
  t: (key: string, params?: Record<string, any>) => string
}>()

const emit = defineEmits<{
  (e: 'close'): void
  (e: 'open-settings'): void
}>()

const context = inject<PluginContext>('pluginContext')!
const t = props.t

/** 当前展示的共享目录根集合（来自 useSettings，含派生特殊条目） */
const roots = computed<SharedRoot[]>(() => props.upload.settings.settings.value.roots ?? [])

/** 视图模式：roots（目录列表）| tree（目录树） */
const view = computed<'roots' | 'tree'>(() =>
  props.upload.currentRoot.value ? 'tree' : 'roots',
)

/** 面包屑展示（根名 + 子目录名） */
const crumbs = computed(() => {
  const root = props.upload.currentRoot.value
  if (!root) return []
  return [root.name, ...props.upload.crumbs.value.map((c) => c.name)]
})

/** 打开时刷新（进入目录列表视图） */
watch(
  () => props.open,
  (open) => {
    if (open) void props.upload.openSheet()
    else props.upload.close()
  },
  { immediate: true },
)

/** 重新授权失效条目（重新选择目录树替换） */
async function reauthorize(root: SharedRoot): Promise<void> {
  const ok = await props.upload.settings.reauthorizeRoot(root)
  if (ok) {
    context.dialogs.showToast(t('transfer.upload.reauthorized'), 'success')
  }
}

/** 点击目录行进入浏览；点击文件直接入队上传 */
function onRowTap(entry: SharedEntry): void {
  if (entry.isDir) {
    void props.upload.cd(entry)
  } else {
    void props.upload.uploadFile(entry)
  }
}
</script>

<template>
  <Teleport to="body">
    <Transition name="ft-sheet">
      <div v-if="open" class="fixed inset-0 z-[100] flex items-end justify-center mobile-ui">
        <!-- Backdrop -->
        <div class="absolute inset-0 bg-[var(--mobile-overlay-heavy)]" @click="emit('close')"></div>

        <!-- Panel -->
        <div class="ft-sheet-panel relative w-full flex flex-col bg-[var(--mobile-bg-card)] border-t border-[var(--mobile-border)] rounded-t-2xl shadow-xl">
          <!-- 抓把 -->
          <div class="flex-shrink-0 flex justify-center pt-2.5 pb-1">
            <div class="w-10 h-1 rounded-full bg-[var(--mobile-border-hover)]"></div>
          </div>

          <!-- 标题行 -->
          <div class="flex-shrink-0 flex items-center gap-2 px-4 py-2">
            <!-- 树视图返回按钮：目录栈逐级返回，根目录回目录列表 -->
            <button
              v-if="view === 'tree'"
              class="flex-shrink-0 ft-nav-btn"
              @click="upload.crumbs.value.length > 0 ? upload.up() : upload.goRoots()"
            >
              <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 19l-7-7 7-7" />
              </svg>
            </button>
            <h3 class="flex-1 ft-sheet-title text-[var(--mobile-text-primary)] truncate">
              {{ t('transfer.upload.title') }}
            </h3>
            <button
              class="flex-shrink-0 ft-close-btn"
              :title="t('transfer.dialog.cancel')"
              @click="emit('close')"
            >
              <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
              </svg>
            </button>
          </div>

          <!-- 面包屑（树视图） -->
          <div v-if="view === 'tree'" class="flex-shrink-0 flex items-center gap-1 px-4 pb-2 overflow-x-auto">
            <template v-for="(seg, i) in crumbs" :key="i">
              <svg v-if="i > 0" class="w-3.5 h-3.5 flex-shrink-0 text-[var(--mobile-text-disabled)]" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5l7 7-7 7" />
              </svg>
              <span
                class="ft-crumb flex-shrink-0 max-w-[7rem] truncate"
                :class="{ 'ft-crumb-current': i === crumbs.length - 1 }"
              >
                {{ seg }}
              </span>
            </template>
          </div>

          <!-- ==================== 视图内容 ==================== -->
          <div class="flex-1 overflow-y-auto min-h-0 px-4 pb-[calc(var(--safe-area-bottom,0px)+12px)]">
            <!-- 共享目录列表（视图：roots） -->
            <div v-if="view === 'roots'">
              <!-- 空态：未配置共享目录 → 引导去设置 -->
              <div v-if="roots.length === 0" class="py-10 text-center">
                <p class="ft-empty-text">{{ t('transfer.upload.noRoots') }}</p>
                <button
                  class="ft-touch-btn ft-cta-btn mt-4"
                  @click="emit('open-settings')"
                >
                  {{ t('transfer.upload.openSettings') }}
                </button>
              </div>

              <p v-else class="ft-section-label">{{ t('transfer.upload.chooseRoot') }}</p>
              <div v-if="roots.length > 0" class="group-card">
                <button
                  v-for="(root, idx) in roots"
                  :key="root.id"
                  class="group-row group-row-btn"
                  :class="{ 'ft-row-last': idx === roots.length - 1 }"
                  @click="upload.enterRoot(root)"
                >
                  <!-- 目录图标 -->
                  <span class="icon-chip flex-shrink-0 chip-zinc">
                    <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                      <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z" />
                    </svg>
                  </span>
                  <div class="flex-1 min-w-0">
                    <p class="ft-root-name text-[var(--mobile-text-primary)] truncate">{{ root.name }}</p>
                    <p class="ft-root-meta mt-0.5 truncate">
                      {{ root.kind === KIND_PRIVATE_DOWNLOADS ? t('transfer.upload.specialEntryHint') : root.id }}
                    </p>
                  </div>
                  <!-- 失效提示 / 免授权徽标 / 进入箭头 -->
                  <button
                    v-if="root.kind !== KIND_PRIVATE_DOWNLOADS && !root.authorized"
                    class="flex-shrink-0 ft-invalid-btn"
                    @click.stop="reauthorize(root)"
                  >
                    {{ t('transfer.upload.reauthorize') }}
                  </button>
                  <span
                    v-else-if="root.kind === KIND_PRIVATE_DOWNLOADS"
                    class="flex-shrink-0 ft-badge"
                  >
                    {{ t('transfer.upload.specialBadge') }}
                  </span>
                  <svg class="w-4 h-4 flex-shrink-0" style="color: var(--mobile-row-sub)" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5l7 7-7 7" />
                  </svg>
                </button>
              </div>
            </div>

            <!-- 目录树浏览（视图：tree） -->
            <div v-else-if="view === 'tree'">
              <!-- 加载态 -->
              <div v-if="upload.loading.value" class="py-10 text-center">
                <span class="ft-spinner"></span>
                <p class="ft-body-text text-[var(--mobile-text-muted)] mt-2">{{ t('transfer.upload.loading') }}</p>
              </div>

              <!-- 列表错误（授权失效等） -->
              <div v-else-if="upload.listError.value" class="py-10 text-center">
                <p class="ft-empty-text">{{ upload.listError.value }}</p>
                <p v-if="upload.rootInvalid.value" class="ft-body-text text-[var(--mobile-warning)] mt-2">
                  {{ t('transfer.upload.rootInvalid') }}
                </p>
                <button class="ft-touch-btn ft-cta-btn mt-4" @click="upload.goRoots()">
                  {{ t('transfer.upload.backToRoots') }}
                </button>
              </div>

              <!-- 空目录 -->
              <div v-else-if="upload.entries.value.length === 0" class="py-10 text-center">
                <p class="ft-empty-text">{{ t('transfer.upload.emptyDir') }}</p>
              </div>

              <!-- 条目列表 -->
              <div v-else class="group-card">
                <button
                  v-for="(entry, idx) in upload.entries.value"
                  :key="entry.uri || entry.name"
                  class="group-row group-row-btn"
                  :class="{ 'ft-row-last': idx === upload.entries.value.length - 1 }"
                  @click="onRowTap(entry)"
                >
                  <FileTypeIcon :name="entry.name" :is-dir="entry.isDir" />
                  <div class="flex-1 min-w-0">
                    <p class="group-row-title truncate">{{ entry.name }}</p>
                    <p v-if="!entry.isDir" class="group-row-sub mt-0.5 truncate">
                      {{ formatBytes(entry.size, t) }}
                    </p>
                  </div>
                  <svg v-if="entry.isDir" class="w-4 h-4 flex-shrink-0" style="color: var(--mobile-row-sub)" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5l7 7-7 7" />
                  </svg>
                </button>
              </div>
            </div>
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<style scoped>
/* 面板最大高度：小屏防溢出，平板展示更多目录 */
.ft-sheet-panel {
  max-height: 78dvh;
}

/* 标题字号 */
.ft-sheet-title {
  font-size: clamp(0.9375rem, 1rem + (100vw - 360px) / 800, 1.0625rem);
  font-weight: 600;
}

/* 返回 / 关闭按钮：44px 触控目标，纯图标 */
.ft-nav-btn,
.ft-close-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 2.75rem;
  height: 2.75rem;
  border-radius: 0.625rem;
  color: var(--mobile-text-secondary);
  transition: background-color 0.15s ease;
}

.ft-nav-btn:active,
.ft-close-btn:active {
  background: var(--mobile-bg-tertiary);
}

/* 面包屑 */
.ft-crumb {
  font-size: clamp(0.75rem, 0.8125rem + (100vw - 360px) / 800, 0.875rem);
  color: var(--mobile-accent);
}

.ft-crumb-current {
  color: var(--mobile-text-primary);
  font-weight: 500;
}

/* 分区标签 */
.ft-section-label {
  font-size: clamp(0.8125rem, 0.875rem + (100vw - 360px) / 800, 0.9375rem);
  font-weight: 500;
  color: var(--mobile-text-primary);
  margin-bottom: 0.5rem;
}

/* 根条目名称 / meta */
.ft-root-name {
  font-size: clamp(0.8125rem, 0.875rem + (100vw - 360px) / 800, 0.9375rem);
}

.ft-root-meta {
  font-size: clamp(0.6875rem, 0.75rem + (100vw - 360px) / 800, 0.8125rem);
  color: var(--mobile-text-muted);
}

/* 免授权徽标（特殊条目） */
.ft-badge {
  flex-shrink: 0;
  padding: 0.125rem 0.5rem;
  border-radius: 9999px;
  font-size: clamp(0.625rem, 0.6875rem + (100vw - 360px) / 800, 0.75rem);
  font-weight: 500;
  background: var(--mobile-bg-tertiary);
  border: 1px solid var(--mobile-border);
  color: var(--mobile-text-secondary);
}

/* 重新授权按钮：警示色描边，44px 触控目标 */
.ft-invalid-btn {
  flex-shrink: 0;
  min-height: 2.25rem;
  padding: 0 0.75rem;
  border-radius: 0.5rem;
  font-size: clamp(0.6875rem, 0.75rem + (100vw - 360px) / 800, 0.8125rem);
  font-weight: 500;
  color: var(--mobile-warning);
  border: 1px solid var(--mobile-warning-muted);
  background: color-mix(in srgb, var(--mobile-warning) 8%, transparent);
}

.ft-invalid-btn:active {
  opacity: 0.8;
}

/* 空态 / 错误文案 */
.ft-empty-text {
  font-size: clamp(0.8125rem, 0.875rem + (100vw - 360px) / 800, 0.9375rem);
  color: var(--mobile-text-secondary);
}

/* CTA 按钮 */
.ft-cta-btn {
  min-height: 2.75rem;
  padding: 0 1.25rem;
  border-radius: 0.75rem;
  font-size: clamp(0.75rem, 0.8125rem + (100vw - 360px) / 800, 0.875rem);
  font-weight: 500;
  color: var(--mobile-text-on-accent);
  background: var(--mobile-accent);
}

.ft-cta-btn:active {
  opacity: 0.8;
}
</style>
