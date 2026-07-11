<template>
  <div class="h-full flex flex-col">
    <!-- 标题区 -->
    <div class="mb-6">
      <h2 class="text-lg font-semibold text-[var(--text-primary)]">
        {{ mode === 'add' ? t('desktop.plugin.aiChatbox.addProvider') : t('desktop.plugin.aiChatbox.editProvider') }}
      </h2>
      <p v-if="mode === 'add'" class="mt-1 text-sm text-[var(--text-tertiary)]">
        {{ t('desktop.plugin.aiChatbox.subtitle') }}
      </p>
    </div>

    <!-- 表单 -->
    <div class="flex-1 space-y-5">
      <!-- 名称 -->
      <div>
        <label class="block text-sm text-[var(--text-secondary)] mb-1.5">{{ t('desktop.plugin.aiChatbox.name') }}</label>
        <input
          v-model="form.name"
          type="text"
          :placeholder="t('desktop.plugin.aiChatbox.name')"
          class="w-full bg-[var(--bg-card)] border rounded-md px-3 py-2 text-sm text-[var(--text-primary)] outline-none focus:border-brand focus:ring-1 focus:ring-brand/30 transition-colors"
          :class="errors.name ? 'border-[var(--color-danger)]' : 'border-[var(--border)]'"
        />
        <p v-if="errors.name" class="mt-1 text-xs text-[var(--color-danger)]">{{ errors.name }}</p>
      </div>

      <!-- Base URL -->
      <div>
        <label class="block text-sm text-[var(--text-secondary)] mb-1.5">{{ t('desktop.plugin.aiChatbox.baseUrl') }}</label>
        <input
          v-model="form.baseUrl"
          type="text"
          placeholder="https://api.example.com/v1"
          class="w-full bg-[var(--bg-card)] border rounded-md px-3 py-2 text-sm text-[var(--text-primary)] outline-none focus:border-brand focus:ring-1 focus:ring-brand/30 transition-colors"
          :class="errors.baseUrl ? 'border-[var(--color-danger)]' : 'border-[var(--border)]'"
        />
        <p v-if="errors.baseUrl" class="mt-1 text-xs text-[var(--color-danger)]">{{ errors.baseUrl }}</p>
      </div>

      <!-- API Key -->
      <div>
        <label class="block text-sm text-[var(--text-secondary)] mb-1.5">{{ t('desktop.plugin.aiChatbox.apiKey') }}</label>
        <div class="relative">
          <input
            v-model="form.apiKey"
            :type="showApiKey ? 'text' : 'password'"
            :placeholder="t('desktop.plugin.aiChatbox.apiKey')"
            class="w-full bg-[var(--bg-card)] border rounded-md px-3 py-2 pr-10 text-sm text-[var(--text-primary)] outline-none focus:border-brand focus:ring-1 focus:ring-brand/30 transition-colors"
            :class="errors.apiKey ? 'border-[var(--color-danger)]' : 'border-[var(--border)]'"
          />
          <button
            class="absolute right-2 top-1/2 -translate-y-1/2 p-1 text-[var(--text-tertiary)] hover:text-[var(--text-secondary)] transition-colors"
            @click="showApiKey = !showApiKey"
          >
            <svg v-if="showApiKey" class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13.875 18.825A10.05 10.05 0 0112 19c-4.478 0-8.268-2.943-9.543-7a9.97 9.97 0 011.563-3.029m5.858.908a3 3 0 114.243 4.243M9.878 9.878l4.242 4.242M9.88 9.88l-3.29-3.29m7.532 7.532l3.29 3.29M3 3l3.59 3.59m0 0A9.953 9.953 0 0112 5c4.478 0 8.268 2.943 9.543 7a10.025 10.025 0 01-4.132 5.411m0 0L21 21" />
            </svg>
            <svg v-else class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M2.458 12C3.732 7.943 7.523 5 12 5c4.478 0 8.268 2.943 9.542 7-1.274 4.057-5.064 7-9.542 7-4.477 0-8.268-2.943-9.542-7z" />
            </svg>
          </button>
        </div>
        <p v-if="errors.apiKey" class="mt-1 text-xs text-[var(--color-danger)]">{{ errors.apiKey }}</p>
      </div>

      <!-- API 格式 -->
      <div>
        <label class="block text-sm text-[var(--text-secondary)] mb-1.5">{{ t('desktop.plugin.aiChatbox.apiFormat') }}</label>
        <select
          v-model="form.apiFormat"
          class="w-full bg-[var(--bg-card)] border border-[var(--border)] rounded-md px-3 py-2 text-sm text-[var(--text-primary)] outline-none focus:border-brand focus:ring-1 focus:ring-brand/30 transition-colors appearance-none cursor-pointer"
        >
          <option v-for="opt in apiFormatOptions" :key="opt.value" :value="opt.value">
            {{ opt.label }}
          </option>
        </select>
      </div>

      <!-- 模型列表 -->
      <ModelListEditor :models="form.models" @update="form.models = $event" />
      <p v-if="errors.models" class="text-xs text-[var(--color-danger)]">{{ errors.models }}</p>
    </div>

    <!-- 底部操作 -->
    <div class="flex items-center gap-3 pt-6 mt-6 border-t border-[var(--border)]">
      <button
        class="px-4 py-2 text-sm bg-brand hover:bg-brand-hover text-white rounded-md transition-colors"
        @click="handleSave"
      >
        {{ mode === 'add' ? t('desktop.plugin.aiChatbox.addProvider') : t('desktop.plugin.aiChatbox.saveProvider') }}
      </button>
      <button
        v-if="mode === 'edit'"
        class="px-4 py-2 text-sm bg-[var(--bg-hover)] text-[var(--color-danger)] hover:bg-[var(--bg-hover)]/80 rounded-md transition-colors"
        @click="emit('delete', editingId)"
      >
        {{ t('desktop.plugin.aiChatbox.deleteProvider') }}
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * 供应商表单 — 新增/编辑共用
 */
import { reactive, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import i18n from '@/locales'
import ModelListEditor from './ModelListEditor.vue'
import type { ApiProvider, ApiFormat } from '../types'
import { API_FORMAT_OPTIONS, generateId } from '../types'

const { t } = useI18n()

const props = defineProps<{
  mode: 'add' | 'edit'
  initialValues?: ApiProvider
  existingNames?: string[]
}>()

const emit = defineEmits<{
  save: [provider: ApiProvider]
  delete: [id: string]
}>()

const apiFormatOptions = API_FORMAT_OPTIONS.map(opt => ({
  ...opt,
  label: t(`desktop.plugin.aiChatbox.format${opt.value.charAt(0).toUpperCase() + opt.value.slice(1)}`),
}))

const showApiKey = ref(false)
const editingId = ref(props.initialValues?.id || '')

interface FormState {
  name: string
  baseUrl: string
  apiKey: string
  apiFormat: ApiFormat
  models: string[]
}

const form = reactive<FormState>({
  name: props.initialValues?.name || '',
  baseUrl: props.initialValues?.baseUrl || '',
  apiKey: props.initialValues?.apiKey || '',
  apiFormat: props.initialValues?.apiFormat || 'openai',
  models: props.initialValues?.models ? [...props.initialValues.models] : [],
})

interface FormErrors {
  name?: string
  baseUrl?: string
  apiKey?: string
  models?: string
}

const errors = reactive<FormErrors>({})

// 编辑模式下监听 initialValues 变化（切换供应商时）
watch(() => props.initialValues, (val) => {
  if (val) {
    form.name = val.name
    form.baseUrl = val.baseUrl
    form.apiKey = val.apiKey
    form.apiFormat = val.apiFormat
    form.models = [...val.models]
    editingId.value = val.id
  }
}, { deep: true })

function validate(): boolean {
  errors.name = ''
  errors.baseUrl = ''
  errors.apiKey = ''
  errors.models = ''

  let valid = true

  if (!form.name.trim()) {
    errors.name = i18n.global.t('desktop.plugin.aiChatbox.nameRequired')
    valid = false
  } else if (props.mode === 'add' && props.existingNames?.includes(form.name.trim())) {
    errors.name = i18n.global.t('desktop.plugin.aiChatbox.providerExists', { name: form.name })
    valid = false
  }

  if (!form.baseUrl.trim()) {
    errors.baseUrl = i18n.global.t('desktop.plugin.aiChatbox.baseUrlRequired')
    valid = false
  }

  // Ollama 不需要 API Key
  if (form.apiFormat !== 'ollama' && !form.apiKey.trim()) {
    errors.apiKey = i18n.global.t('desktop.plugin.aiChatbox.apiKeyRequired')
    valid = false
  }

  const nonEmptyModels = form.models.filter(m => m.trim())
  if (nonEmptyModels.length === 0) {
    errors.models = i18n.global.t('desktop.plugin.aiChatbox.modelRequired')
    valid = false
  }

  return valid
}

function handleSave(): void {
  if (!validate()) return

  const nonEmptyModels = form.models.filter(m => m.trim())
  const provider: ApiProvider = {
    id: props.mode === 'edit' ? editingId.value : generateId(),
    name: form.name.trim(),
    apiKey: form.apiKey.trim(),
    baseUrl: form.baseUrl.trim(),
    apiFormat: form.apiFormat,
    models: nonEmptyModels,
    activeModel: props.initialValues?.activeModel || nonEmptyModels[0] || '',
  }

  emit('save', provider)
}
</script>
