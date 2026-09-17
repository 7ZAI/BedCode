<template>
  <div class="h-full flex flex-col bg-[var(--bg-page)]">
    <!-- ==================== 工具栏页头 ==================== -->
    <div class="wb-toolbar">
      <div class="flex items-center gap-2.5">
        <svg
          class="w-4 h-4 text-[var(--text-secondary)]"
          fill="none"
          stroke="currentColor"
          viewBox="0 0 24 24"
        >
          <path
            stroke-linecap="round"
            stroke-linejoin="round"
            stroke-width="1.5"
            d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z"
          />
          <path
            stroke-linecap="round"
            stroke-linejoin="round"
            stroke-width="1.5"
            d="M15 12a3 3 0 11-6 0 3 3 0 016 0z"
          />
        </svg>
        <h2 class="text-[calc(13px*var(--ui-scale))] font-semibold text-[var(--text-primary)]">
          {{ t('settings.title') }}
        </h2>
      </div>
      <div class="flex items-center gap-2">
        <PluginPageToolbar target="settings" />
        <button class="wb-btn-ghost" @click="handleCheckUpdate">
          {{ getUpdateStatusText() }}
        </button>
      </div>
    </div>

    <!-- 内容区：按功能分 section，section 间 24px；语言切换淡出作用于整个容器 -->
    <div class="flex-1 overflow-auto px-6 py-6">
      <div
        class="lang-fade-content max-w-3xl mx-auto space-y-6"
        :class="{ 'lang-fading': langFading }"
        :style="{ transitionDuration: animationsEnabled ? '0.4s' : '0s' }"
      >
        <SettingsAppearanceSection
          :language-options="languageOptions"
          :current-language="currentLanguage"
          :animations-enabled="animationsEnabled"
          :on-switch-language="switchLanguage"
          :on-toggle-animations="toggleAnimations"
        />
        <SettingsPairingSection />
        <SettingsLinkCryptoSection />
        <SettingsSessionSection />
        <SettingsSystemSection />
        <SettingsLoggingSection />
        <SettingsAboutSection />
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * 设置视图 — 桌面端设置页面（编排层）
 * Warm Workbench 风格：分段控件 + 方角开关 + section 分组；支持多主题色板预留。
 * 各分组拆分为 src/components/settings/ 下的独立组件（外观 / 配对 / 链路加密 /
 * 会话 / 系统 / 日志 / 关于），本组件仅保留工具栏、语言切换过渡状态
 * （作用于整个容器）与全局防抖保存编排。
 */
import { ref, watch, computed, onMounted, onBeforeUnmount } from 'vue'
import { useI18n } from 'vue-i18n'
import { useSettingsStore } from '@/stores/settings'
import { useI18nStore } from '@/stores/i18n'
import { useUpdateChecker } from '@/composables/useUpdateChecker'
import { useToast } from '@/composables/useToast'
import i18n from '@/locales'
import PluginPageToolbar from '@/plugin/components/PluginPageToolbar.vue'
import SettingsAppearanceSection from '@/components/settings/SettingsAppearanceSection.vue'
import SettingsPairingSection from '@/components/settings/SettingsPairingSection.vue'
import SettingsLinkCryptoSection from '@/components/settings/SettingsLinkCryptoSection.vue'
import SettingsSessionSection from '@/components/settings/SettingsSessionSection.vue'
import SettingsSystemSection from '@/components/settings/SettingsSystemSection.vue'
import SettingsLoggingSection from '@/components/settings/SettingsLoggingSection.vue'
import SettingsAboutSection from '@/components/settings/SettingsAboutSection.vue'

const { t } = useI18n()
const settingsStore = useSettingsStore()
const i18nStore = useI18nStore()
const toast = useToast()

// 更新检查（模块级单例）：工具栏按钮 + 关于分组的下载/安装进度共用同一状态
const {
  status: updateStatus,
  checkForUpdate,
  getUpdateStatusText,
} = useUpdateChecker()

// ==================== 语言切换（作用于整个设置容器） ====================

const languageOptions = [
  { value: 'zh-CN', label: '中文' },
  { value: 'en', label: 'English' },
]

const currentLanguage = computed({
  get: () => settingsStore.settings.ui.language || 'zh-CN',
  set: (value: string) => i18nStore.setLanguage(value),
})

// 全局动画总开关：直接读写 store，由 deep watch 防抖持久化（无需即时保存）
const animationsEnabled = computed({
  get: () => settingsStore.settings.ui.animations_enabled ?? true,
  set: (value: boolean) => {
    settingsStore.settings.ui.animations_enabled = value
  },
})

function toggleAnimations() {
  animationsEnabled.value = !animationsEnabled.value
}

// 语言切换过渡：先淡出当前内容，再在不可见时换语言，最后淡入新内容，
// 避免新旧文案重叠造成的闪烁；总时长由「动画效果」开关与 0.4s 时长控制。
const FADE_MS = 400
const langFading = ref(false)
let langFadeTimer: ReturnType<typeof setTimeout> | null = null

function switchLanguage(value: string) {
  if (currentLanguage.value === value) return
  if (langFadeTimer) clearTimeout(langFadeTimer)
  if (!animationsEnabled.value) {
    void i18nStore.setLanguage(value)
    return
  }
  langFading.value = true
  langFadeTimer = setTimeout(() => {
    void i18nStore.setLanguage(value)
    requestAnimationFrame(() => {
      requestAnimationFrame(() => {
        langFading.value = false
      })
    })
  }, FADE_MS)
}

async function handleCheckUpdate() {
  const update = await checkForUpdate()
  if (!update && updateStatus.value === 'latest') {
    toast.info(i18n.global.t('settings.about.alreadyLatest'))
  } else if (!update && updateStatus.value === 'failed') {
    toast.error(i18n.global.t('settings.about.checkFailed'))
  }
}

// ==================== 全局防抖保存 ====================

// 设置变更 500ms 后统一持久化；组件卸载时立即 flush，避免 500ms 窗口内切页导致
// 变更丢失（theme_palette/theme 的 setter 已即时保存，此处兜底字体/环境等其余字段）。
// 保存回写（settings.value 被 store 重新赋值）会触发本 watch——经
// store.isPersisted 比对内容后跳过，不会形成保存循环。
let saveTimeout: ReturnType<typeof setTimeout> | null = null

watch(
  () => settingsStore.settings,
  () => {
    if (settingsStore.isPersisted(settingsStore.settings)) return
    if (saveTimeout) clearTimeout(saveTimeout)
    saveTimeout = setTimeout(() => {
      void settingsStore.saveSettings(settingsStore.settings)
    }, 500)
  },
  { deep: true },
)

onBeforeUnmount(() => {
  // 立即 flush 未保存的变更（卸载后 watch 不再触发）
  if (saveTimeout) {
    clearTimeout(saveTimeout)
    saveTimeout = null
    if (!settingsStore.isPersisted(settingsStore.settings)) {
      void settingsStore.saveSettings(settingsStore.settings)
    }
  }
})

onMounted(async () => {
  await settingsStore.loadSettings()
})
</script>

<style scoped>
/* 语言切换：淡出 → 换文案 → 淡入（单元素不重挂载，无重叠闪烁） */
.lang-fade-content {
  transition-property: opacity;
  transition-timing-function: ease;
}
.lang-fade-content.lang-fading {
  opacity: 0;
}
</style>
