<script setup lang="ts">
/**
 * ResultPage — 识别结果页（spec §6.2 第 4 条）
 *
 * - 文本行列表：点击某行复制该行（Toast 反馈）
 * - 低置信度行弱化（confidence < 0.6 次要色 + 「低置信度」标签）
 * - 底部「复制全文」；空结果给「未识别到文字」空态（CTA 返回主页）
 * - 离开页面（宿主 header 返回 / 返回主页）时清空共享结果，避免陈旧展示
 *
 * 结果数据经 useOcr 的 module 级共享状态（OcrView 写入），不经路由参数。
 */
import { inject, onUnmounted } from 'vue'
import type { PluginContext } from '@bedcode/plugin-sdk-mobile'
import { useOcr, clearOcrResult, LOW_CONFIDENCE_THRESHOLD } from '../composables/useOcr'

const context = inject<PluginContext>('pluginContext')!
const t = (key: string, params?: Record<string, any>) => context.i18n.t(key, params)

const ocr = useOcr(context)

/** 复制文本并 Toast 反馈（navigator.clipboard 为既有插件模式；失败静默） */
async function copyText(text: string, toastKey: string): Promise<void> {
  try {
    await navigator.clipboard.writeText(text)
    context.dialogs.showToast(t(toastKey), 'success')
  } catch {
    // WebView 剪贴板不可用时静默降级（不打扰用户）
  }
}

/** 点击某行：复制该行文本 */
async function handleCopyLine(text: string): Promise<void> {
  await copyText(text, 'ocr.result.lineCopied')
}

/** 底部「复制全文」：行间换行拼接 */
async function handleCopyAll(): Promise<void> {
  const text = (ocr.lines.value ?? []).map((l) => l.text).join('\n')
  if (!text) return
  await copyText(text, 'ocr.result.copied')
}

onUnmounted(() => {
  clearOcrResult()
})
</script>

<template>
  <div class="flex flex-col gap-3 p-4 min-h-full">
    <!-- 空结果空态 -->
    <div v-if="!ocr.lines.value || ocr.lines.value.length === 0" class="ocr-empty-state">
      <div class="ocr-empty-ico ocr-empty-ico--neutral">
        <svg class="w-9 h-9" fill="none" stroke="currentColor" viewBox="0 0 24 24" aria-hidden="true">
          <path
            stroke-linecap="round"
            stroke-linejoin="round"
            stroke-width="1.6"
            d="M3.375 19.5h17.25m-17.25 0a1.125 1.125 0 01-1.125-1.125M3.375 19.5h7.5c.621 0 1.125-.504 1.125-1.125m-9.75 0V5.625m0 12.75v-1.5c0-.621.504-1.125 1.125-1.125m18.375 2.625V5.625m0 12.75c0 .621-.504 1.125-1.125 1.125m1.125-1.125v-1.5c0-.621-.504-1.125-1.125-1.125m0 3.75h-7.5A1.125 1.125 0 0112 18.375m9.75-12.75c0-.621-.504-1.125-1.125-1.125H3.375c-.621 0-1.125.504-1.125 1.125m19.5 0v1.5c0 .621-.504 1.125-1.125 1.125M2.25 5.625v1.5c0 .621.504 1.125 1.125 1.125m0 0h17.25m-17.25 0h7.5c.621 0 1.125.504 1.125 1.125M3.375 8.25c-.621 0-1.125.504-1.125 1.125v1.5c0 .621.504 1.125 1.125 1.125m17.25-3.75h-7.5c-.621 0-1.125.504-1.125 1.125m8.625-1.125c.621 0 1.125.504 1.125 1.125v1.5c0 .621-.504 1.125-1.125 1.125m-17.25 0h7.5m-7.5 0c-.621 0-1.125.504-1.125 1.125v1.5c0 .621.504 1.125 1.125 1.125M12 10.875v-1.5m0 1.5c0 .621-.504 1.125-1.125 1.125M12 10.875c0 .621.504 1.125 1.125 1.125m-2.25 0c.621 0 1.125.504 1.125 1.125M13.125 12h7.5m-7.5 0c-.621 0-1.125.504-1.125 1.125M20.625 12c.621 0 1.125.504 1.125 1.125v1.5c0 .621-.504 1.125-1.125 1.125m-17.25 0h7.5M12 14.625v-1.5m0 1.5c0 .621-.504 1.125-1.125 1.125M12 14.625c0 .621.504 1.125 1.125 1.125m-2.25 0c.621 0 1.125.504 1.125 1.125m0 1.5v-1.5m0 0c0-.621.504-1.125 1.125-1.125m0 0h7.5"
          />
        </svg>
      </div>
      <div class="text-[var(--font-size-m)] text-[var(--mobile-text-secondary)] font-medium">
        {{ t('ocr.result.empty') }}
      </div>
      <div class="text-[var(--font-size-xs)] text-[var(--mobile-text-muted)]">
        {{ t('ocr.result.emptyDesc') }}
      </div>
      <!-- 空态操作入口：回主页重新选图（避免空态只有文案无行动点） -->
      <button
        class="ocr-btn-accent mt-3 h-11 px-5 rounded-xl text-[var(--font-size-s)] font-medium transition-colors duration-200 active:opacity-80"
        @click="context.ui.goBack()"
      >
        {{ t('ocr.result.backToHome') }}
      </button>
    </div>

    <!-- 识别元信息 + 行列表 -->
    <template v-else>
      <div class="text-[var(--font-size-xs)] text-[var(--mobile-text-muted)] px-1 tabular-nums">
        {{ t('ocr.result.meta', { count: ocr.lines.value.length, duration: ocr.durationMs.value }) }}
      </div>

      <div class="flex flex-col gap-2">
        <button
          v-for="(line, idx) in ocr.lines.value"
          :key="idx"
          class="ocr-line-card group text-left rounded-xl px-3.5 py-3 bg-[var(--mobile-bg-card)] shadow-[var(--mobile-card-shadow)] transition-[color,background-color,transform] duration-200 active:scale-[0.98]"
          :class="{ 'ocr-line-card--weak': line.confidence < LOW_CONFIDENCE_THRESHOLD }"
          @click="handleCopyLine(line.text)"
        >
          <div class="flex items-start gap-2">
            <span
              class="ocr-line-no flex-shrink-0 mt-0.5 text-[var(--font-size-xs)] text-[var(--mobile-text-muted)] tabular-nums"
            >
              {{ idx + 1 }}
            </span>
            <span
              class="flex-1 min-w-0 text-[var(--font-size-s)] text-[var(--mobile-text-primary)] leading-relaxed break-words"
            >
              {{ line.text }}
            </span>
            <span
              v-if="line.confidence < LOW_CONFIDENCE_THRESHOLD"
              class="ocr-low-chip flex-shrink-0 mt-0.5"
            >
              {{ t('ocr.result.lowConfidence') }}
            </span>
          </div>
        </button>
      </div>
    </template>

    <!-- 底部复制全文（仅结果非空时） -->
    <div v-if="ocr.lines.value && ocr.lines.value.length > 0" class="mt-auto pt-2 pb-[env(safe-area-inset-bottom)]">
      <button
        class="w-full h-11 rounded-xl bg-[var(--mobile-accent)] text-[var(--mobile-text-on-accent)] font-medium transition-colors duration-200 active:opacity-80"
        @click="handleCopyAll"
      >
        {{ t('ocr.result.copyAll') }}
      </button>
    </div>
  </div>
</template>

<style scoped>
/* 空态：flex:1 撑满宿主滚动容器，内容垂直居中（同 ft-empty-state 手法） */
.ocr-empty-state {
  flex: 1 0 auto;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  padding: 2rem 1.25rem 2.5rem;
  gap: 0.375rem;
  text-align: center;
}

.ocr-empty-ico {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 4.5rem;
  height: 4.5rem;
  border-radius: 1.5rem;
  margin-bottom: 0.625rem;
  background: color-mix(in srgb, currentColor 10%, transparent);
}

.ocr-empty-ico--neutral {
  color: var(--mobile-text-muted);
}

/* 低置信度行：次要色 + 弱化透明度（hover 仅桌面鼠标时叠加，触摸靠 active:scale） */
.ocr-line-card--weak {
  background: var(--mobile-bg-tertiary);
}

.ocr-line-card--weak .flex-1 {
  color: var(--mobile-text-secondary);
  opacity: 0.75;
}

.ocr-line-card:hover {
  background: color-mix(in srgb, var(--mobile-text-primary) 5%, var(--mobile-bg-card));
}

.ocr-line-card.ocr-line-card--weak:hover {
  background: color-mix(in srgb, var(--mobile-text-primary) 4%, var(--mobile-bg-tertiary));
}

.ocr-low-chip {
  display: inline-flex;
  align-items: center;
  height: 1.5rem;
  padding: 0 0.5rem;
  border-radius: 9999px;
  font-size: var(--font-size-xs);
  font-weight: 600;
  color: var(--mobile-warning);
  background: color-mix(in srgb, var(--mobile-warning) 12%, transparent);
}
</style>
