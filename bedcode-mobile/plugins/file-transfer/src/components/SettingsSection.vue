<script setup lang="ts">
/**
 * SettingsSection — 文件传输设置区 (Mobile)
 *
 * 共享目录管理：Android 优先用 SAF 系统目录选择器（fileService.pickDirectory，
 * 免存储权限）；不支持的 provider / iOS 降级为手动输入绝对路径 + 列表增删。
 * 下载目录只读展示（下载固定落系统 AppDownloadsDir，pick-download-dir 不适用）。
 * 并发数 1–8 步进；底部常驻明文传输安全告知（spec §10 transfer.settings.plainWarning）。
 *
 * 同时注册为宿主 SettingsSection（registerSettingsSection），并作为插件内设置页复用。
 *
 * 样式完全复用宿主 settings-group / settings-row / settings-section-title /
 * settings-label / settings-desc 设计语言，字号统一 clamp() 流式缩放。
 */
import { ref, inject } from 'vue'
import type { PluginContext } from '@bedcode/plugin-sdk-mobile'
import type { useSettings } from '../composables/useSettings'
import { CONCURRENCY_MAX } from '../composables/useSettings'

type SettingsApi = ReturnType<typeof useSettings>

const props = defineProps<{
  settingsApi: SettingsApi
  t: (key: string, params?: Record<string, any>) => string
}>()

/** 宿主经 PluginViewHost provide 的插件上下文（选择器与 Toast 用） */
const context = inject<PluginContext>('pluginContext')

const t = props.t

/** 手动输入的新共享目录路径 */
const newRoot = ref('')
const adding = ref(false)
const picking = ref(false)

/** 系统目录选择器选目录（取消/失败静默，失败 toast 提示降级手动输入） */
async function handlePickRoot(): Promise<void> {
  if (!context || picking.value) return
  picking.value = true
  try {
    const path = await context.fileService.pickDirectory()
    if (path) {
      const result = await props.settingsApi.addRoot(path)
      if (result === 'duplicate') {
        context.dialogs.showToast(t('transfer.settings.rootDuplicate'), 'warning')
      } else if (result === 'failed') {
        context.dialogs.showToast(t('transfer.settings.addRootFailed'), 'error')
      }
    }
    // 取消（null）静默
  } catch {
    context.dialogs.showToast(t('transfer.settings.pickFailed'), 'error')
  } finally {
    picking.value = false
  }
}

async function handleAddRoot(): Promise<void> {
  const path = newRoot.value
  if (!path.trim() || !context) return
  adding.value = true
  try {
    const result = await props.settingsApi.addRoot(path)
    if (result === 'ok') {
      newRoot.value = ''
    } else if (result === 'duplicate') {
      context.dialogs.showToast(t('transfer.settings.rootDuplicate'), 'warning')
    } else {
      context.dialogs.showToast(t('transfer.settings.addRootFailed'), 'error')
    }
  } finally {
    adding.value = false
  }
}

async function handleRemoveRoot(path: string): Promise<void> {
  await props.settingsApi.removeRoot(path)
}

function decConcurrency(): void {
  const cur = props.settingsApi.settings.value.concurrency
  if (cur > 1) void props.settingsApi.setConcurrency(cur - 1)
}

function incConcurrency(): void {
  const cur = props.settingsApi.settings.value.concurrency
  if (cur < CONCURRENCY_MAX) void props.settingsApi.setConcurrency(cur + 1)
}

/** 点击下载目录行：提示固定下载位置（只读，无选择交互） */
function showDownloadHint(): void {
  if (context) {
    context.dialogs.showToast(t('transfer.settings.downloadDirHint'), 'info')
  }
}
</script>

<template>
  <div class="ft-settings px-4 py-4 space-y-5">
    <!-- ==================== 共享目录 ==================== -->
    <section class="space-y-2">
      <h2 class="settings-section-title">{{ t('transfer.settings.sharedRoots') }}</h2>
      <p class="settings-desc ft-settings-hint">{{ t('transfer.settings.addRootHint') }}</p>

      <!-- 系统选择器 + 手动输入兜底 -->
      <div class="flex gap-2 mb-2">
        <button
          class="flex-shrink-0 ft-touch-btn px-4 rounded-xl text-white bg-[var(--mobile-accent)] active:opacity-80 transition-opacity disabled:opacity-50 ft-settings-btn"
          :disabled="picking"
          @click="handlePickRoot()"
        >
          {{ picking ? '…' : t('transfer.settings.pickRoot') }}
        </button>
        <input
          v-model="newRoot"
          type="text"
          :placeholder="t('transfer.dialog.localDirPlaceholder')"
          class="flex-1 min-w-0 ft-settings-input"
          @keydown.enter="handleAddRoot()"
        />
        <button
          class="flex-shrink-0 ft-touch-btn px-4 rounded-xl ft-btn-neutral active:opacity-80 transition-opacity disabled:opacity-50 ft-settings-btn"
          :disabled="adding || !newRoot.trim()"
          @click="handleAddRoot()"
        >
          {{ t('transfer.settings.addRoot') }}
        </button>
      </div>

      <!-- 目录列表 -->
      <div v-if="(settingsApi?.settings.value.roots.length ?? 0) === 0" class="settings-desc py-1">
        {{ t('transfer.settings.noRoots') }}
      </div>
      <div v-else class="settings-group">
        <div
          v-for="(root, idx) in settingsApi?.settings.value.roots ?? []"
          :key="root"
          class="settings-row"
        >
          <div class="flex items-center gap-2 flex-1 min-w-0">
            <svg class="w-4 h-4 flex-shrink-0 text-[var(--mobile-accent)]" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z" />
            </svg>
            <span class="settings-label flex-1 min-w-0 truncate">{{ root }}</span>
          </div>
          <button
            class="flex-shrink-0 ft-settings-remove-btn"
            @click="handleRemoveRoot(root)"
          >
            {{ t('transfer.settings.removeRoot') }}
          </button>
        </div>
      </div>
    </section>

    <!-- ==================== 下载目录（只读） ==================== -->
    <section class="space-y-2">
      <h2 class="settings-section-title">{{ t('transfer.settings.downloadDir') }}</h2>
      <div class="settings-group">
        <div
          class="settings-row"
          role="button"
          tabindex="0"
          @click="showDownloadHint()"
        >
          <div class="flex items-center gap-2 flex-1 min-w-0">
            <svg class="w-4 h-4 flex-shrink-0 text-[var(--mobile-text-muted)]" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4" />
            </svg>
            <span class="settings-value flex-1 min-w-0 truncate">
              {{ settingsApi?.settings.value.downloadDir || t('transfer.settings.noDownloadDir') }}
            </span>
          </div>
        </div>
      </div>
    </section>

    <!-- ==================== 并发数 ==================== -->
    <section class="space-y-2">
      <h2 class="settings-section-title">{{ t('transfer.settings.concurrency') }}</h2>
      <div class="settings-group">
        <div class="settings-row">
          <div class="min-w-0">
            <div class="settings-label">{{ t('transfer.settings.concurrency') }}</div>
            <div class="settings-desc">{{ t('transfer.settings.concurrencyHint') }}</div>
          </div>
          <div class="flex items-center gap-2 flex-shrink-0">
            <button
              class="ft-step-btn"
              @click="decConcurrency()"
            >
              −
            </button>
            <span class="ft-step-value">
              {{ settingsApi?.settings.value.concurrency ?? 3 }}
            </span>
            <button
              class="ft-step-btn"
              @click="incConcurrency()"
            >
              +
            </button>
          </div>
        </div>
      </div>
    </section>

    <!-- ==================== 明文安全告知（spec §10） ==================== -->
    <div class="ft-warning-box">
      <p class="ft-warning-text">
        {{ t('transfer.settings.plainWarning') }}
      </p>
    </div>
  </div>
</template>

<style scoped>
/* 设置提示文字 */
.ft-settings-hint {
  font-size: clamp(0.6875rem, 0.75rem + (100vw - 360px) / 800 * 0.0625rem, 0.8125rem);
  color: var(--mobile-text-muted);
  margin-bottom: 0.25rem;
}

/* 设置按钮流式字号 */
.ft-settings-btn {
  font-size: clamp(0.75rem, 0.8125rem + (100vw - 360px) / 800 * 0.0625rem, 0.875rem);
  font-weight: 500;
}

/* 输入框：复用宿主 settings-number-input 风格 */
.ft-settings-input {
  padding: 0.4375rem 0.75rem;
  font-size: clamp(0.75rem, 0.8125rem + (100vw - 360px) / 800 * 0.0625rem, 0.875rem);
  color: var(--mobile-text-primary);
  background: var(--mobile-input-bg);
  border: 1px solid var(--mobile-input-border);
  border-radius: 0.625rem;
  outline: none;
  transition: border-color 0.15s ease;
}

.ft-settings-input:focus {
  border-color: var(--mobile-accent);
}

/* 删除按钮 */
.ft-settings-remove-btn {
  padding: 0.25rem 0.625rem;
  border-radius: 0.5rem;
  font-size: clamp(0.6875rem, 0.75rem + (100vw - 360px) / 800 * 0.0625rem, 0.8125rem);
  color: var(--mobile-error);
  border: 1px solid var(--mobile-error-muted);
  background: transparent;
  transition: opacity 0.15s ease;
}

.ft-settings-remove-btn:active {
  opacity: 0.8;
}

/* 步进按钮（并发数） */
.ft-step-btn {
  width: 2.25rem;
  height: 2.25rem;
  border-radius: 0.625rem;
  border: 1px solid var(--mobile-border);
  background: var(--mobile-bg-elevated);
  color: var(--mobile-text-primary);
  font-size: 1.125rem;
  display: flex;
  align-items: center;
  justify-content: center;
  transition: opacity 0.15s ease;
}

.ft-step-btn:active {
  opacity: 0.8;
}

/* 步进数值 */
.ft-step-value {
  width: 2rem;
  text-align: center;
  font-size: clamp(1rem, 1.0625rem + (100vw - 360px) / 800 * 0.0625rem, 1.125rem);
  font-weight: 600;
  color: var(--mobile-text-primary);
}

/* 安全告知 */
.ft-warning-box {
  padding: 0.75rem 1rem;
  border-radius: 0.75rem;
  border: 1px solid var(--mobile-warning-muted);
  background: color-mix(in srgb, var(--mobile-warning) 6%, transparent);
}

.ft-warning-text {
  font-size: clamp(0.6875rem, 0.75rem + (100vw - 360px) / 800 * 0.0625rem, 0.8125rem);
  line-height: 1.5;
  color: var(--mobile-warning);
  margin: 0;
}
</style>
