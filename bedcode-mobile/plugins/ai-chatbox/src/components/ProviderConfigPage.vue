<template>
  <div class="h-full flex flex-col bg-[var(--mobile-bg-primary)]">
    <!-- 顶栏 -->
    <header class="mobile-header-safe flex items-center justify-between px-3 pb-2 pt-1 border-b border-[var(--mobile-border)] bg-[var(--mobile-bg-card)]">
      <button
        class="h-11 px-2 -ml-2 flex items-center gap-1 text-[var(--font-size-sm)] text-[var(--mobile-text-secondary)] active:opacity-80 rounded-xl transition-opacity"
        @click="emit('back')"
      >
        <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 19l-7-7 7-7" />
        </svg>
        {{ t('mobile.plugin.aiChatbox.backToChat') }}
      </button>
      <h3 class="text-[var(--font-size-base)] font-medium text-[var(--mobile-text-primary)]">{{ t('mobile.plugin.aiChatbox.providerConfig') }}</h3>
      <div class="w-16"></div>
    </header>

    <!-- 主体：单栏滚动（预设 → 自定义 → 表单） -->
    <div class="flex-1 overflow-y-auto px-4 py-4 space-y-6">
      <!-- 预设供应商 -->
      <section>
        <div class="text-xs font-medium text-[var(--mobile-text-muted)] mb-2">
          {{ t('mobile.plugin.aiChatbox.presetProviders') }}
        </div>
        <div class="flex gap-2 overflow-x-auto pb-1 -mx-4 px-4">
          <button
            v-for="preset in PROVIDER_PRESETS"
            :key="preset.name"
            class="flex-shrink-0 h-11 px-4 rounded-xl text-[var(--font-size-sm)] border transition-colors"
            :class="selectedPresetName === preset.name
              ? 'bg-[var(--mobile-accent-muted)] border-[var(--mobile-border-active)] text-[var(--mobile-accent)]'
              : 'bg-[var(--mobile-bg-card)] border-[var(--mobile-border)] text-[var(--mobile-text-primary)] active:bg-[var(--mobile-bg-tertiary)]'"
            @click="selectPreset(preset)"
          >
            {{ preset.name }}
          </button>
        </div>
      </section>

      <!-- 自定义供应商 -->
      <section>
        <div class="text-xs font-medium text-[var(--mobile-text-muted)] mb-2">
          {{ t('mobile.plugin.aiChatbox.customProviders') }}
        </div>
        <div class="space-y-1.5">
          <button
            v-for="p in providers"
            :key="p.id"
            class="w-full flex items-center gap-2 px-3 h-12 rounded-xl text-[var(--font-size-base)] transition-colors"
            :class="selectedProviderId === p.id
              ? 'bg-[var(--mobile-accent-muted)] text-[var(--mobile-text-primary)]'
              : 'bg-[var(--mobile-bg-card)] text-[var(--mobile-text-secondary)] active:bg-[var(--mobile-bg-tertiary)]'"
            @click="selectProvider(p.id)"
          >
            <span class="flex-1 min-w-0 truncate text-left">{{ p.name }}</span>
            <span
              v-if="activeProviderId === p.id"
              class="w-2 h-2 rounded-full bg-[var(--mobile-accent)] flex-shrink-0"
              :title="t('mobile.plugin.aiChatbox.activeProvider')"
            ></span>
          </button>

          <button
            class="w-full text-left px-3 h-12 rounded-xl text-[var(--font-size-base)] text-[var(--mobile-accent)] active:bg-[var(--mobile-bg-tertiary)] transition-colors"
            @click="addCustom"
          >
            {{ t('mobile.plugin.aiChatbox.addCustomProvider') }}
          </button>
        </div>
      </section>

      <!-- 表单区 -->
      <section class="pt-1">
        <ProviderForm
          :key="formKey"
          :mode="editingMode"
          :initial-values="formInitialValues"
          :existing-names="existingNames"
          :fetch-models="props.fetchModels"
          :test-connection="props.testConnection"
          @save="handleSave"
          @delete="handleDelete"
        />
      </section>
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * 供应商配置页（移动端全屏）— 预设目录/自定义供应商列表 + 表单编辑
 */
import { ref, computed } from 'vue'
import { useI18n } from 'vue-i18n'
import ProviderForm from './ProviderForm.vue'
import { PROVIDER_PRESETS } from '../types'
import type { ApiProvider, ProviderPreset } from '../types'

const props = defineProps<{
  providers: ApiProvider[]
  activeProviderId?: string
  /** 拉取模型列表（ChatView 注入 config.fetchModels） */
  fetchModels: (provider: ApiProvider) => Promise<string[]>
  /** 测试连接（ChatView 注入 config.testConnection） */
  testConnection: (provider: ApiProvider) => Promise<string>
}>()

const emit = defineEmits<{
  back: []
  add: [provider: ApiProvider]
  update: [provider: ApiProvider]
  remove: [id: string]
}>()

const { t } = useI18n()

const selectedProviderId = ref('')
const selectedPresetName = ref('')
const editingMode = ref<'add' | 'edit'>('add')
const formKey = ref(0)

/** 当前编辑的供应商初始值 */
const formInitialValues = computed<ApiProvider | undefined>(() => {
  if (editingMode.value === 'edit' && selectedProviderId.value) {
    return props.providers.find(p => p.id === selectedProviderId.value)
  }
  return undefined
})

const existingNames = computed(() => props.providers.map(p => p.name))

/** 从预设添加 */
function selectPreset(preset: ProviderPreset): void {
  selectedProviderId.value = ''
  selectedPresetName.value = preset.name
  editingMode.value = 'add'
  formKey.value++
}

/** 编辑已有供应商 */
function selectProvider(id: string): void {
  selectedProviderId.value = id
  selectedPresetName.value = ''
  editingMode.value = 'edit'
  formKey.value++
}

/** 自定义添加（空表单） */
function addCustom(): void {
  selectedProviderId.value = ''
  selectedPresetName.value = ''
  editingMode.value = 'add'
  formKey.value++
}

/** 保存（新增或更新） */
function handleSave(provider: ApiProvider): void {
  if (editingMode.value === 'edit' && selectedProviderId.value) {
    emit('update', provider)
  } else {
    emit('add', provider)
    // 添加后自动切换到编辑模式
    selectedProviderId.value = provider.id
    selectedPresetName.value = ''
    editingMode.value = 'edit'
    formKey.value++
  }
}

/** 删除供应商 */
function handleDelete(id: string): void {
  emit('remove', id)
  selectedProviderId.value = ''
  selectedPresetName.value = ''
  editingMode.value = 'add'
  formKey.value++
}
</script>
