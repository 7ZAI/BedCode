<template>
  <div class="h-full flex flex-col bg-[var(--bg-page)]">
    <!-- 顶栏 -->
    <header class="h-12 flex items-center justify-between px-4 border-b border-[var(--border)] bg-[var(--bg-card)]">
      <button
        class="flex items-center gap-1.5 text-sm text-[var(--text-secondary)] hover:text-[var(--text-primary)] hover:bg-[var(--bg-hover)] px-2 py-1 -ml-2 rounded-btn transition-colors"
        @click="emit('back')"
      >
        <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 19l-7-7 7-7" />
        </svg>
        {{ t('desktop.plugin.aiChatbox.backToChat') }}
      </button>
      <h3 class="text-sm font-medium text-[var(--text-primary)]">{{ t('desktop.plugin.aiChatbox.providerConfig') }}</h3>
      <div class="w-20"></div>
    </header>

    <!-- 主体：左右分栏 -->
    <div class="flex-1 flex overflow-hidden">
      <!-- 左侧：预设目录 + 已配置供应商 -->
      <div class="w-56 flex-shrink-0 flex flex-col border-r border-[var(--border)] bg-[var(--bg-card)] overflow-y-auto">
        <!-- 预设目录 -->
        <div class="px-3 pt-3 pb-1 text-xs font-medium text-[var(--text-tertiary)]">
          {{ t('desktop.plugin.aiChatbox.presetProviders') }}
        </div>
        <div class="px-2 space-y-0.5">
          <button
            v-for="preset in PROVIDER_PRESETS"
            :key="preset.name"
            class="w-full text-left px-2.5 py-1.5 text-sm rounded-btn transition-colors"
            :class="selectedPresetName === preset.name
              ? 'bg-[var(--bg-hover)] text-[var(--text-primary)]'
              : 'text-[var(--text-primary)] hover:bg-[var(--bg-hover)]'"
            @click="selectPreset(preset)"
          >
            {{ preset.name }}
          </button>
        </div>

        <!-- 自定义供应商 -->
        <div class="px-3 pt-3 pb-1 text-xs font-medium text-[var(--text-tertiary)]">
          {{ t('desktop.plugin.aiChatbox.customProviders') }}
        </div>
        <div class="px-2 space-y-0.5">
          <button
            v-for="p in providers"
            :key="p.id"
            class="w-full flex items-center gap-2 px-2.5 py-1.5 text-sm rounded-btn transition-colors"
            :class="selectedProviderId === p.id
              ? 'bg-[var(--bg-hover)] text-[var(--text-primary)]'
              : 'text-[var(--text-secondary)] hover:bg-[var(--bg-hover)] hover:text-[var(--text-primary)]'"
            @click="selectProvider(p.id)"
          >
            <span class="flex-1 min-w-0 truncate">{{ p.name }}</span>
            <span
              v-if="activeProviderId === p.id"
              class="w-1.5 h-1.5 rounded-full bg-brand flex-shrink-0"
              :title="t('desktop.plugin.aiChatbox.activeProvider')"
            ></span>
          </button>

          <button
            class="w-full text-left px-2.5 py-1.5 text-sm rounded-btn text-[var(--color-primary)] hover:bg-[var(--bg-hover)] transition-colors"
            @click="addCustom"
          >
            {{ t('desktop.plugin.aiChatbox.addCustomProvider') }}
          </button>
        </div>
      </div>

      <!-- 右侧表单区 -->
      <div class="flex-1 overflow-y-auto p-6">
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
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * 供应商配置页 — 预设目录/自定义供应商列表 + 表单编辑
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
