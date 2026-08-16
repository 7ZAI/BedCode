<script setup lang="ts">
/**
 * OcrSettings — OCR 设置区：模型管理（spec §6.2 第 5 条）
 *
 * - 显示模型占用（modelsBytes，MB 格式化）
 * - 模型在位：「删除模型」（先确认弹窗；删除后入口禁用）
 * - 模型缺失：「恢复模型」（恢复后解禁）
 * - 全程 busy 防重入；结果 Toast 反馈；与主页共享 enginePhase 状态源
 */
import { inject, onMounted } from 'vue'
import type { PluginContext } from '@binblink/plugin-sdk-mobile'
import { useOcr } from '../composables/useOcr'

const context = inject<PluginContext>('pluginContext')!
const t = (key: string, params?: Record<string, any>) => context.i18n.t(key, params)

const ocr = useOcr(context)

/** 字节 → 人类可读（MB，保留 1 位） */
function formatBytes(bytes: number): string {
  if (!bytes) return '0 MB'
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`
}

/** 删除模型：确认弹窗 → 删除 → 反馈 */
async function handleDelete(): Promise<void> {
  if (ocr.modelBusy.value) return
  const ok = await context.dialogs.showConfirm({
    title: t('ocr.settings.deleteModels'),
    message: t('ocr.settings.deleteConfirm'),
  })
  if (!ok) return
  try {
    await ocr.deleteModels()
    context.dialogs.showToast(t('ocr.settings.deleted'), 'success')
  } catch {
    context.dialogs.showToast(t('ocr.settings.deleteFailed'), 'error')
  }
}

/** 恢复模型（幂等） */
async function handleRestore(): Promise<void> {
  if (ocr.modelBusy.value) return
  try {
    await ocr.restoreModels()
    context.dialogs.showToast(t('ocr.settings.restored'), 'success')
  } catch {
    context.dialogs.showToast(t('ocr.settings.restoreFailed'), 'error')
  }
}

onMounted(() => {
  void ocr.refreshEngineStatus()
})
</script>

<template>
  <div class="p-4 flex flex-col gap-3">
    <div class="settings-group">
      <div class="settings-section-title">{{ t('ocr.settings.modelTitle') }}</div>
      <div class="settings-desc">{{ t('ocr.settings.modelDesc') }}</div>

      <div class="settings-row">
        <span class="settings-label">{{ t('ocr.settings.modelsBytes') }}</span>
        <span class="text-[var(--font-size-s)] text-[var(--mobile-text-secondary)] tabular-nums">
          {{ formatBytes(ocr.engineStatus.value?.modelsBytes ?? 0) }}
        </span>
      </div>

      <!-- 模型在位：删除入口；模型缺失：恢复入口（互斥，busy 时均禁用） -->
      <button
        v-if="ocr.engineStatus.value?.modelsPresent"
        class="ocr-settings-btn ocr-btn-danger w-full mt-2 transition-colors duration-200 active:opacity-80"
        :disabled="ocr.modelBusy.value"
        @click="handleDelete"
      >
        {{
          ocr.modelBusy.value ? t('ocr.settings.deleting') : t('ocr.settings.deleteModels')
        }}
      </button>
      <button
        v-else
        class="ocr-settings-btn ocr-btn-accent w-full mt-2 transition-colors duration-200 active:opacity-80"
        :disabled="ocr.modelBusy.value"
        @click="handleRestore"
      >
        {{
          ocr.modelBusy.value ? t('ocr.settings.restoring') : t('ocr.settings.restoreModels')
        }}
      </button>
    </div>
  </div>
</template>

<style scoped>
/* 操作按钮样式在全局 styles.css（.ocr-settings-btn / .ocr-btn-accent / .ocr-btn-danger） */
</style>
