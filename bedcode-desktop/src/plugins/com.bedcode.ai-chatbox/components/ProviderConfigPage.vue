<template>
  <div class="h-full flex flex-col bg-[var(--bg-page)]">
    <!-- 顶栏 -->
    <header class="h-12 flex items-center justify-between px-4 border-b border-[var(--border)] bg-[var(--bg-hover)]">
      <button
        class="flex items-center gap-1.5 text-sm text-[var(--text-secondary)] hover:text-[var(--text-primary)] transition-colors"
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
      <!-- 左侧边栏 -->
      <ProviderSidebar
        :providers="providers"
        :selected-provider-id="selectedProviderId"
        :selected-preset-name="selectedPresetName"
        :is-add-mode="editingMode === 'add' && !selectedPresetName"
        @select-provider="handleSelectProvider"
        @select-preset="handleSelectPreset"
        @add-new="handleAddNew"
      />

      <!-- 右侧表单区 -->
      <div class="flex-1 overflow-y-auto p-6">
        <ProviderForm
          :key="formKey"
          :mode="editingMode"
          :initial-values="formInitialValues"
          :existing-names="existingNames"
          @save="handleSave"
          @delete="handleDelete"
        />
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * 供应商配置页 — 独立页面，左右分栏
 */
import { ref, computed } from 'vue'
import { useI18n } from 'vue-i18n'
import i18n from '@/locales'
import ProviderSidebar from './ProviderSidebar.vue'
import ProviderForm from './ProviderForm.vue'
import type { ApiProvider, ProviderPreset } from '../types'

const { t } = useI18n()

const props = defineProps<{
  providers: ApiProvider[]
}>()

const emit = defineEmits<{
  back: []
  add: [provider: ApiProvider]
  update: [id: string, provider: ApiProvider]
  remove: [id: string]
}>()

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

/** 已存在的供应商名称（用于重名检测） */
const existingNames = computed(() =>
  props.providers.map(p => p.name)
)

/** 切换到编辑模式 */
function handleSelectProvider(id: string): void {
  selectedProviderId.value = id
  selectedPresetName.value = ''
  editingMode.value = 'edit'
  formKey.value++
}

/** 从预设添加 */
function handleSelectPreset(preset: ProviderPreset): void {
  selectedProviderId.value = ''
  selectedPresetName.value = preset.name
  editingMode.value = 'add'
  formKey.value++
}

/** 新增空白供应商 */
function handleAddNew(): void {
  selectedProviderId.value = ''
  selectedPresetName.value = ''
  editingMode.value = 'add'
  formKey.value++
}

/** 保存（新增或更新） */
function handleSave(provider: ApiProvider): void {
  if (editingMode.value === 'edit' && selectedProviderId.value) {
    emit('update', selectedProviderId.value, provider)
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
  if (!confirm(i18n.global.t('desktop.plugin.aiChatbox.confirmDelete'))) return
  emit('remove', id)
  selectedProviderId.value = ''
  selectedPresetName.value = ''
  editingMode.value = 'add'
  formKey.value++
}
</script>
