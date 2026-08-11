<script setup lang="ts">
/**
 * SettingsSection — 文件传输设置区 (Mobile)
 *
 * 共享目录管理：Android 优先用 SAF 系统目录选择器（fileService.pickDirectory，
 * 免存储权限）；不支持的 provider / iOS 降级为手动输入绝对路径 + 列表增删。
 * 下载目录只读展示（未显式配置时经 get-settings 解析宿主默认下载目录 AppDownloadsDir）。
 * 并发数 1–8 步进；底部常驻明文传输安全告知（spec §10 transfer.settings.plainWarning）。
 *
 * 同时注册为宿主 SettingsSection（registerSettingsSection），并作为插件内设置页复用。
 *
 * 样式完全复用宿主 settings-group / settings-row / settings-section-title /
 * settings-label / settings-desc 设计语言，字号统一 clamp() 流式缩放；
 * 提示与安全告知统一使用黄色提醒框（ft-warning-box）。
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

/** 「所有文件访问权限」一键授权跳转中 */
const granting = ref(false)

/**
 * 一键跳转系统「所有文件访问权限」授权页（Android 11+ 分区存储下读取
 * 顶层自定义目录必需；无运行时弹窗，只能经系统设置手动开启）。
 * 已授权时宿主直接返回 true（不跳转），失败（非 Android / 未激活）toast 提示。
 */
async function handleGrantAllFilesAccess(): Promise<void> {
  if (!context || granting.value) return
  granting.value = true
  try {
    const granted = await context.fileService.requestAllFilesAccess()
    if (granted) {
      context.dialogs.showToast(t('transfer.settings.allFilesAccessGranted'), 'success')
    }
    // 未授权：宿主已跳转系统设置页，回到 App 后用户手动开启
  } catch {
    context.dialogs.showToast(t('transfer.settings.allFilesAccessUnavailable'), 'error')
  } finally {
    granting.value = false
  }
}

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
</script>

<template>
  <div class="ft-settings px-4 py-4 pb-8 space-y-6">
    <!-- ==================== 共享目录 ==================== -->
    <section class="space-y-3">
      <h2 class="settings-section-title">{{ t('transfer.settings.sharedRoots') }}</h2>

      <!-- 使用说明：黄色提醒框（与底部明文安全告知同款视觉） -->
      <div class="ft-warning-box">
        <p class="ft-warning-text">{{ t('transfer.settings.addRootHint') }}</p>
        <p class="ft-warning-text mt-2">{{ t('transfer.settings.scopedStorageHint') }}</p>
        <button
          class="ft-grant-btn mt-2.5"
          :disabled="granting"
          @click="handleGrantAllFilesAccess()"
        >
          {{ granting ? t('transfer.settings.granting') : t('transfer.settings.grantAllFilesAccess') }}
        </button>
      </div>

      <!-- 系统选择器：通栏主按钮（图标 + 文案，44px+ 触控目标） -->
      <button
        class="ft-touch-btn w-full gap-2 rounded-xl text-[var(--mobile-text-on-accent)] bg-[var(--mobile-accent)] active:opacity-80 transition-opacity disabled:opacity-50 ft-settings-btn"
        :disabled="picking"
        @click="handlePickRoot()"
      >
        <svg class="w-4 h-4 flex-shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z" />
        </svg>
        {{ picking ? t('transfer.settings.picking') : t('transfer.settings.pickRoot') }}
      </button>

      <!-- 手动输入兜底：添加按钮在上（与「选择目录」主按钮并列成组），下方输入路径 -->
      <div class="space-y-2.5">
        <button
          class="ft-touch-btn w-full gap-2 rounded-xl ft-btn-accent active:opacity-80 transition-opacity disabled:opacity-50 ft-settings-btn"
          :disabled="adding || !newRoot.trim()"
          @click="handleAddRoot()"
        >
          <svg class="w-4 h-4 flex-shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 4v16m8-8H4" />
          </svg>
          {{ t('transfer.settings.addRoot') }}
        </button>
        <input
          v-model="newRoot"
          type="text"
          :placeholder="t('transfer.dialog.localDirPlaceholder')"
          class="w-full ft-settings-input"
          @keydown.enter="handleAddRoot()"
        />
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
            <span class="settings-label flex-1 min-w-0 truncate" :title="root">{{ root }}</span>
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

    <!-- ==================== 下载目录（只读，未配置时展示默认落盘地址） ==================== -->
    <section class="space-y-2">
      <h2 class="settings-section-title">{{ t('transfer.settings.downloadDir') }}</h2>
      <div class="settings-group">
        <div class="settings-row">
          <div class="flex items-center gap-2 flex-1 min-w-0">
            <svg class="w-4 h-4 flex-shrink-0 text-[var(--mobile-text-muted)]" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4" />
            </svg>
            <span class="settings-value flex-1 min-w-0 truncate" :class="{ 'ft-settings-unset': !(settingsApi?.settings.value.downloadDir) }">
              {{ settingsApi?.settings.value.downloadDir || t('transfer.settings.noDownloadDir') }}
            </span>
          </div>
        </div>
      </div>
      <p class="settings-desc ft-settings-hint">{{ t('transfer.settings.downloadDirHint') }}</p>
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
/* 设置区次级说明文字 */
.ft-settings-hint {
  font-size: clamp(0.6875rem, 0.75rem + (100vw - 360px) / 800, 0.8125rem);
  color: var(--mobile-text-muted);
}

/* 设置按钮流式字号 */
.ft-settings-btn {
  font-size: clamp(0.75rem, 0.8125rem + (100vw - 360px) / 800, 0.875rem);
  font-weight: 500;
}

/* 输入框：独立一行通栏，高度对齐触控按钮（44px+），placeholder 走 token */
.ft-settings-input {
  min-height: clamp(2.75rem, 2.75rem + (100vw - 400px) / 800 * 4, 3rem);
  padding: 0.5rem 0.875rem;
  font-size: clamp(0.75rem, 0.8125rem + (100vw - 360px) / 800, 0.875rem);
  color: var(--mobile-text-primary);
  background: var(--mobile-input-bg);
  border: 1px solid var(--mobile-input-border);
  border-radius: 0.75rem;
  outline: none;
  transition: border-color 0.15s ease;
}

.ft-settings-input::placeholder {
  color: var(--mobile-input-placeholder);
}

.ft-settings-input:focus {
  border-color: var(--mobile-input-focus);
}

/* 删除按钮 */
.ft-settings-remove-btn {
  padding: 0.25rem 0.625rem;
  border-radius: 0.5rem;
  font-size: clamp(0.6875rem, 0.75rem + (100vw - 360px) / 800, 0.8125rem);
  color: var(--mobile-error);
  border: 1px solid var(--mobile-error-muted);
  background: transparent;
  transition: opacity 0.15s ease;
}

.ft-settings-remove-btn:active {
  opacity: 0.8;
}

/* 步进按钮（并发数）：44px 触控目标 */
.ft-step-btn {
  width: 2.75rem;
  height: 2.75rem;
  border-radius: 0.625rem;
  border: 1px solid var(--mobile-border);
  background: var(--mobile-bg-elevated);
  color: var(--mobile-text-primary);
  font-size: var(--font-size-xl);
  display: flex;
  align-items: center;
  justify-content: center;
  transition: opacity 0.15s ease;
}

.ft-step-btn:active {
  opacity: 0.8;
}

/* 步进数值：独立 chip 背景，避免「孤儿数字」感 */
.ft-step-value {
  width: 2.5rem;
  height: 2.25rem;
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: 0.625rem;
  background: var(--mobile-bg-tertiary);
  text-align: center;
  font-size: clamp(1rem, 1.0625rem + (100vw - 360px) / 800, 1.125rem);
  font-weight: 600;
  color: var(--mobile-text-primary);
  font-variant-numeric: tabular-nums;
}

/* 下载目录未设置：占位 chip 样式，明确「尚未配置」而非可编辑输入 */
.ft-settings-unset {
  display: inline-flex;
  align-items: center;
  padding: 0.25rem 0.625rem;
  border-radius: 0.5rem;
  background: var(--mobile-bg-tertiary);
  font-size: clamp(0.6875rem, 0.75rem + (100vw - 360px) / 800, 0.8125rem);
  color: var(--mobile-text-muted);
}

/* 黄色提醒框（使用说明 / 安全告知共用） */
.ft-warning-box {
  padding: 0.75rem 1rem;
  border-radius: 0.75rem;
  border: 1px solid var(--mobile-warning-muted);
  background: color-mix(in srgb, var(--mobile-warning) 6%, transparent);
}

.ft-warning-text {
  font-size: clamp(0.6875rem, 0.75rem + (100vw - 360px) / 800, 0.8125rem);
  line-height: 1.5;
  color: var(--mobile-warning);
  margin: 0;
}

/* 「去授权」按钮：警告框内次要操作，主色文字 + 警示边框，44px 触控目标 */
.ft-grant-btn {
  min-height: clamp(2.5rem, 2.5rem + (100vw - 400px) / 800 * 2, 2.75rem);
  padding: 0 1rem;
  border-radius: 0.625rem;
  border: 1px solid var(--mobile-warning-muted);
  background: color-mix(in srgb, var(--mobile-warning) 10%, transparent);
  color: var(--mobile-warning);
  font-size: clamp(0.75rem, 0.8125rem + (100vw - 360px) / 800, 0.875rem);
  font-weight: 600;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  transition: opacity 0.15s ease;
}

.ft-grant-btn:active {
  opacity: 0.8;
}

.ft-grant-btn:disabled {
  opacity: 0.5;
}
</style>
