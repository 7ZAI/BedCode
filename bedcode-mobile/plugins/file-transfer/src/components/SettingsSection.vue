<script setup lang="ts">
/**
 * SettingsSection — 文件传输设置区 (Mobile)
 *
 * 共享目录管理：移动端无目录选择器（context.fileService.pickDirectory 在移动端
 * reject），改为手动输入绝对路径 + 列表增删。
 * 下载目录只读展示（下载固定落系统 AppDownloadsDir，pick-download-dir 不适用）。
 * 并发数 1–8 步进；底部常驻明文传输安全告知（spec §10 transfer.settings.plainWarning）。
 *
 * 同时注册为宿主 SettingsSection（registerSettingsSection），并作为插件内设置页复用。
 */
import { ref } from 'vue'
import type { useSettings } from '../composables/useSettings'
import { CONCURRENCY_MAX } from '../composables/useSettings'

type SettingsApi = ReturnType<typeof useSettings>

const props = defineProps<{
  settingsApi: SettingsApi
  t: (key: string, params?: Record<string, any>) => string
}>()

const t = props.t

/** 手动输入的新共享目录路径 */
const newRoot = ref('')
const adding = ref(false)

async function handleAddRoot(): Promise<void> {
  const path = newRoot.value
  if (!path.trim()) return
  adding.value = true
  try {
    const ok = await props.settingsApi.addRoot(path)
    if (ok) newRoot.value = ''
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
  <div class="px-4 py-3 space-y-6">
    <!-- ==================== 共享目录 ==================== -->
    <section>
      <p class="text-sm font-semibold text-[var(--mobile-text-primary)]">
        {{ t('transfer.settings.sharedRoots') }}
      </p>
      <p class="text-xs text-[var(--mobile-text-muted)] mt-0.5 mb-2.5">
        {{ t('transfer.settings.addRootHint') }}
      </p>

      <!-- 手动输入 -->
      <div class="flex gap-2 mb-3">
        <input
          v-model="newRoot"
          type="text"
          :placeholder="t('transfer.dialog.localPathPlaceholder')"
          class="flex-1 min-w-0 rounded-xl border border-[var(--mobile-input-border)] bg-[var(--mobile-input-bg)] px-3.5 py-2.5 text-sm text-[var(--mobile-text-primary)] outline-none focus:border-[var(--mobile-accent)]"
          @keydown.enter="handleAddRoot()"
        />
        <button
          class="flex-shrink-0 px-4 py-2.5 rounded-xl text-sm font-medium bg-[var(--mobile-accent)] text-white active:opacity-80 transition-opacity"
          :disabled="adding || !newRoot.trim()"
          :class="{ 'opacity-50': adding || !newRoot.trim() }"
          @click="handleAddRoot()"
        >
          {{ t('transfer.settings.addRoot') }}
        </button>
      </div>

      <!-- 目录列表 -->
      <div v-if="(settingsApi?.settings.value.roots.length ?? 0) === 0" class="text-sm text-[var(--mobile-text-muted)] py-2">
        {{ t('transfer.settings.noRoots') }}
      </div>
      <div v-else class="space-y-2">
        <div
          v-for="root in settingsApi?.settings.value.roots ?? []"
          :key="root"
          class="flex items-center gap-2 px-3.5 py-2.5 rounded-xl border border-[var(--mobile-border)] bg-[var(--mobile-bg-secondary)]"
        >
          <svg class="w-4 h-4 flex-shrink-0 text-[var(--mobile-accent)]" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z" />
          </svg>
          <span class="flex-1 min-w-0 text-sm text-[var(--mobile-text-primary)] truncate">{{ root }}</span>
          <button
            class="flex-shrink-0 px-2.5 py-1 rounded-lg text-xs text-[var(--mobile-error)] border border-[var(--mobile-error-muted)] active:opacity-80"
            @click="handleRemoveRoot(root)"
          >
            {{ t('transfer.settings.removeRoot') }}
          </button>
        </div>
      </div>
    </section>

    <!-- ==================== 下载目录（只读） ==================== -->
    <section>
      <p class="text-sm font-semibold text-[var(--mobile-text-primary)]">
        {{ t('transfer.settings.downloadDir') }}
      </p>
      <p class="text-xs text-[var(--mobile-text-muted)] mt-0.5 mb-2">
        {{ t('transfer.settings.downloadDirHint') }}
      </p>
      <div class="flex items-center gap-2 px-3.5 py-2.5 rounded-xl border border-[var(--mobile-border)] bg-[var(--mobile-bg-secondary)]">
        <svg class="w-4 h-4 flex-shrink-0 text-[var(--mobile-text-muted)]" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4" />
        </svg>
        <span class="flex-1 min-w-0 text-sm text-[var(--mobile-text-muted)] truncate">
          {{ settingsApi?.settings.value.downloadDir || t('transfer.settings.noDownloadDir') }}
        </span>
      </div>
    </section>

    <!-- ==================== 并发数 ==================== -->
    <section>
      <p class="text-sm font-semibold text-[var(--mobile-text-primary)]">
        {{ t('transfer.settings.concurrency') }}
      </p>
      <p class="text-xs text-[var(--mobile-text-muted)] mt-0.5 mb-2">
        {{ t('transfer.settings.concurrencyHint') }}
      </p>
      <div class="flex items-center gap-3">
        <button
          class="flex-shrink-0 w-10 h-10 rounded-xl border border-[var(--mobile-border)] text-lg text-[var(--mobile-text-primary)] active:opacity-80 flex items-center justify-center"
          @click="decConcurrency()"
        >
          −
        </button>
        <span class="w-8 text-center text-lg font-semibold text-[var(--mobile-text-primary)]">
          {{ settingsApi?.settings.value.concurrency ?? 3 }}
        </span>
        <button
          class="flex-shrink-0 w-10 h-10 rounded-xl border border-[var(--mobile-border)] text-lg text-[var(--mobile-text-primary)] active:opacity-80 flex items-center justify-center"
          @click="incConcurrency()"
        >
          +
        </button>
      </div>
    </section>

    <!-- ==================== 明文安全告知（spec §10） ==================== -->
    <div class="rounded-xl border border-[var(--mobile-warning-muted)] bg-[var(--mobile-warning-muted)]/40 px-3.5 py-3">
      <p class="text-xs leading-relaxed text-[var(--mobile-warning)]">
        {{ t('transfer.settings.plainWarning') }}
      </p>
    </div>
  </div>
</template>
