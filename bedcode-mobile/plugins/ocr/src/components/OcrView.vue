<script setup lang="ts">
/**
 * OcrView — OCR 主页（spec §6.2）
 *
 * 两个主按钮「相册选图」「拍照」+ 引擎状态条：
 * - ready/loading：按钮可用（首次识别触发引擎惰性加载）
 * - missing：按钮禁用，状态条引导「恢复模型」
 * - unavailable：按钮禁用，状态条展示不可用
 * 识别中 loading 禁用按钮防重入；触发按钮图标换 spinner（activeAction 区分）；
 * 成功后跳转结果页；失败 Toast 映射错误。
 */
import { computed, inject, onMounted, ref } from 'vue'
import type { PluginContext } from '@binblink/plugin-sdk-mobile'
import { useOcr, mapRecognizeError, RESULT_ROUTE_ID } from '../composables/useOcr'

const context = inject<PluginContext>('pluginContext')!
const t = (key: string, params?: Record<string, any>) => context.i18n.t(key, params)

const ocr = useOcr(context)

/** 模型恢复中（状态条按钮 busy） */
const restoring = ref(false)

/** 当前触发的取图入口（loading 时仅触发按钮显示 spinner，另一按钮保持原样禁用） */
const activeAction = ref<'album' | 'camera' | null>(null)

/** 取图识别公共错误处理：Toast + 刷新引擎状态（识别失败可能是模型刚被删除） */
function handleError(err: unknown): void {
  const mapped = mapRecognizeError(err)
  context.dialogs.showToast(t(mapped.key, mapped.params), 'error')
  void ocr.refreshEngineStatus()
}

/** 相册选图 → 识别 → 跳结果页 */
async function handleAlbum(): Promise<void> {
  if (busy.value) return
  activeAction.value = 'album'
  try {
    if (await ocr.recognizeFromAlbum()) {
      context.ui.openPage(RESULT_ROUTE_ID)
    }
  } catch (err) {
    handleError(err)
  } finally {
    activeAction.value = null
  }
}

/** 拍照 → 识别 → 跳结果页 */
async function handleCamera(): Promise<void> {
  if (busy.value) return
  activeAction.value = 'camera'
  try {
    if (await ocr.recognizeFromCamera()) {
      context.ui.openPage(RESULT_ROUTE_ID)
    }
  } catch (err) {
    handleError(err)
  } finally {
    activeAction.value = null
  }
}

/** 状态条「恢复模型」 */
async function handleRestore(): Promise<void> {
  if (restoring.value || ocr.modelBusy.value) return
  restoring.value = true
  try {
    await ocr.restoreModels()
    context.dialogs.showToast(t('ocr.settings.restored'), 'success')
  } catch {
    context.dialogs.showToast(t('ocr.settings.restoreFailed'), 'error')
  } finally {
    restoring.value = false
  }
}

const busy = computed(() => ocr.recognizing.value || restoring.value)
/** 模型未就绪/引擎不可用时禁用取图入口 */
const actionsDisabled = computed(
  () =>
    busy.value ||
    ocr.enginePhase.value === 'missing' ||
    ocr.enginePhase.value === 'unavailable',
)

onMounted(() => {
  void ocr.refreshEngineStatus()
})
</script>

<template>
  <div class="flex flex-col gap-3 p-4">
    <!-- 引擎状态条 -->
    <div
      class="flex items-center gap-2 px-3 py-2.5 rounded-lg bg-[var(--mobile-bg-tertiary)] text-[var(--mobile-text-secondary)]"
      role="status"
    >
      <span
        v-if="ocr.enginePhase.value === 'ready'"
        class="w-2 h-2 rounded-full bg-[var(--mobile-success)] flex-shrink-0"
      ></span>
      <span
        v-else-if="ocr.enginePhase.value === 'missing'"
        class="w-2 h-2 rounded-full bg-[var(--mobile-warning)] flex-shrink-0"
      ></span>
      <span
        v-else-if="ocr.enginePhase.value === 'unavailable'"
        class="w-2 h-2 rounded-full bg-[var(--mobile-error)] flex-shrink-0"
      ></span>
      <span
        v-else
        class="w-2 h-2 rounded-full bg-[var(--mobile-text-disabled)] flex-shrink-0"
      ></span>

      <span class="flex-1 min-w-0 truncate text-[var(--font-size-s)]">
        <template v-if="ocr.enginePhase.value === 'ready'">
          {{ t('ocr.home.engineReady') }}
        </template>
        <template v-else-if="ocr.enginePhase.value === 'loading'">
          {{ t('ocr.home.engineLoading') }}
        </template>
        <template v-else-if="ocr.enginePhase.value === 'unavailable'">
          {{ t('ocr.home.engineUnavailable') }}
        </template>
        <template v-else-if="ocr.enginePhase.value === 'missing'">
          {{ t('ocr.home.modelsMissing') }}
        </template>
        <template v-else>{{ t('ocr.home.engineLoading') }}</template>
      </span>

      <!-- 模型缺失引导恢复 -->
      <button
        v-if="ocr.enginePhase.value === 'missing'"
        class="ocr-btn-accent flex-shrink-0 h-8 px-3 rounded-lg transition-colors duration-200 active:opacity-80"
        :disabled="restoring"
        @click="handleRestore"
      >
        {{ restoring ? t('ocr.home.restoring') : t('ocr.home.restoreModels') }}
      </button>
    </div>

    <!-- 模型缺失说明（text-pretty 平衡中文断行，避免断在词中） -->
    <p
      v-if="ocr.enginePhase.value === 'missing'"
      class="px-1 text-[var(--font-size-xs)] text-[var(--mobile-text-muted)] leading-relaxed text-pretty"
    >
      {{ t('ocr.home.modelsMissingDesc') }}
    </p>

    <!-- 两个主入口按钮 -->
    <div class="flex gap-3 mt-1">
      <button
        class="ocr-primary-btn flex-1 flex flex-col items-center gap-1.5 rounded-2xl bg-[var(--mobile-bg-card)] text-[var(--mobile-text-primary)] shadow-[var(--mobile-card-shadow)] transition-colors duration-200"
        :disabled="actionsDisabled"
        @click="handleAlbum"
      >
        <span
          v-if="ocr.recognizing.value && activeAction === 'album'"
          class="w-5 h-5 rounded-full border-2 border-[var(--mobile-text-muted)] border-t-transparent animate-spin flex-shrink-0"
          aria-hidden="true"
        ></span>
        <svg
          v-else
          class="w-6 h-6"
          fill="none"
          stroke="currentColor"
          viewBox="0 0 24 24"
          aria-hidden="true"
        >
          <path
            stroke-linecap="round"
            stroke-linejoin="round"
            stroke-width="1.8"
            d="M4 16l4.586-4.586a2 2 0 012.828 0L16 16m-2-2l1.586-1.586a2 2 0 012.828 0L20 14m-6-6h.01M6 20h12a2 2 0 002-2V6a2 2 0 00-2-2H6a2 2 0 00-2 2v12a2 2 0 002 2z"
          />
        </svg>
        <span class="text-[var(--font-size-s)] font-medium">
          {{ ocr.recognizing.value && activeAction === 'album' ? t('ocr.home.recognizing') : t('ocr.home.pickAlbum') }}
        </span>
      </button>

      <button
        class="ocr-primary-btn flex-1 flex flex-col items-center gap-1.5 rounded-2xl bg-[var(--mobile-bg-card)] text-[var(--mobile-text-primary)] shadow-[var(--mobile-card-shadow)] transition-colors duration-200"
        :disabled="actionsDisabled"
        @click="handleCamera"
      >
        <span
          v-if="ocr.recognizing.value && activeAction === 'camera'"
          class="w-5 h-5 rounded-full border-2 border-[var(--mobile-text-muted)] border-t-transparent animate-spin flex-shrink-0"
          aria-hidden="true"
        ></span>
        <svg
          v-else
          class="w-6 h-6"
          fill="none"
          stroke="currentColor"
          viewBox="0 0 24 24"
          aria-hidden="true"
        >
          <path
            stroke-linecap="round"
            stroke-linejoin="round"
            stroke-width="1.8"
            d="M3 9a2 2 0 012-2h.93a2 2 0 001.664-.89l.812-1.22A2 2 0 0110.07 4h3.86a2 2 0 011.664.89l.812 1.22A2 2 0 0018.07 7H19a2 2 0 012 2v9a2 2 0 01-2 2H5a2 2 0 01-2-2V9z"
          />
          <path
            stroke-linecap="round"
            stroke-linejoin="round"
            stroke-width="1.8"
            d="M15 13a3 3 0 11-6 0 3 3 0 016 0z"
          />
        </svg>
        <span class="text-[var(--font-size-s)] font-medium">
          {{ ocr.recognizing.value && activeAction === 'camera' ? t('ocr.home.recognizing') : t('ocr.home.capture') }}
        </span>
      </button>
    </div>

    <!-- 识别中提示 -->
    <Transition name="ocr-fade">
      <p
        v-if="ocr.recognizing.value"
        class="text-center text-[var(--font-size-xs)] text-[var(--mobile-text-muted)]"
      >
        {{ t('ocr.home.recognizingDesc') }}
      </p>
    </Transition>
  </div>
</template>

<style scoped>
/* 状态条恢复按钮在全局 styles.css（.ocr-btn-accent） */
.ocr-fade-enter-active,
.ocr-fade-leave-active {
  transition: opacity 0.2s ease;
}

.ocr-fade-enter-from,
.ocr-fade-leave-to {
  opacity: 0;
}
</style>
