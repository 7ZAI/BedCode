<template>
  <div class="max-w-xl space-y-5">
    <div class="flex items-center justify-between">
      <h4 class="text-base font-medium text-[var(--text-primary)]">
        {{ mode === 'edit' ? t('desktop.plugin.aiChatbox.editProvider') : t('desktop.plugin.aiChatbox.addProvider') }}
      </h4>
      <span
        v-if="mode === 'edit'"
        class="text-xs px-2 py-1 rounded-full bg-[var(--bg-hover)] text-[var(--text-tertiary)]"
      >{{ initialValues?.name }}</span>
    </div>

    <!-- 名称 -->
    <div>
      <label class="block text-xs font-medium mb-1.5 text-[var(--text-secondary)]">
        {{ t('desktop.plugin.aiChatbox.name') }}
      </label>
      <input
        v-model="form.name"
        type="text"
        class="w-full h-[36px] px-3 text-sm bg-[var(--bg-input)] text-[var(--text-primary)] border border-[var(--border-input)] rounded-input placeholder:text-[var(--text-tertiary)] focus:outline-none focus:border-brand transition-colors"
      />
    </div>

    <!-- Base URL -->
    <div>
      <label class="block text-xs font-medium mb-1.5 text-[var(--text-secondary)]">
        {{ t('desktop.plugin.aiChatbox.baseUrl') }}
      </label>
      <input
        v-model="form.baseUrl"
        type="text"
        placeholder="https://api.example.com/v1"
        class="w-full h-[36px] px-3 text-sm bg-[var(--bg-input)] text-[var(--text-primary)] border border-[var(--border-input)] rounded-input placeholder:text-[var(--text-tertiary)] focus:outline-none focus:border-brand transition-colors"
      />
    </div>

    <!-- API Key -->
    <div>
      <label class="block text-xs font-medium mb-1.5 text-[var(--text-secondary)]">
        {{ t('desktop.plugin.aiChatbox.apiKey') }}
      </label>
      <div class="flex gap-2">
        <input
          v-model="form.apiKey"
          :type="showKey ? 'text' : 'password'"
          placeholder="sk-..."
          class="flex-1 h-[36px] px-3 text-sm bg-[var(--bg-input)] text-[var(--text-primary)] border border-[var(--border-input)] rounded-input placeholder:text-[var(--text-tertiary)] focus:outline-none focus:border-brand transition-colors"
        />
        <button
          class="h-[36px] px-3 text-sm rounded-btn bg-[var(--bg-hover)] text-[var(--text-secondary)] hover:bg-[var(--bg-input)] transition-colors flex-shrink-0"
          @click="showKey = !showKey"
        >
          {{ showKey ? t('desktop.plugin.aiChatbox.hide') : t('desktop.plugin.aiChatbox.show') }}
        </button>
      </div>
      <p class="mt-1 text-xs text-[var(--text-tertiary)]">
        {{ t('desktop.plugin.aiChatbox.apiKeyHint') }}
      </p>
    </div>

    <!-- 拉取模型 + 测试连接 -->
    <div class="flex items-center gap-2">
      <button
        class="h-[36px] px-4 text-sm rounded-btn bg-[var(--bg-hover)] text-[var(--text-secondary)] hover:bg-[var(--bg-input)] transition-colors disabled:opacity-50"
        :disabled="fetching || !form.baseUrl || !form.apiKey"
        @click="onFetchModels"
      >
        {{ fetching ? t('desktop.plugin.aiChatbox.fetchingModels') : t('desktop.plugin.aiChatbox.fetchModels') }}
      </button>
      <button
        class="h-[36px] px-4 text-sm rounded-btn bg-[var(--bg-hover)] text-[var(--text-secondary)] hover:bg-[var(--bg-input)] transition-colors disabled:opacity-50"
        :disabled="testing || !form.baseUrl || !form.apiKey"
        @click="onTestConnection"
      >
        {{ testing ? t('desktop.plugin.aiChatbox.testing') : t('desktop.plugin.aiChatbox.testConnection') }}
      </button>
      <span
        v-if="fetchError"
        class="text-xs text-[var(--color-danger)] flex-1 break-words"
      >{{ fetchError }}</span>
      <span
        v-else-if="testResult !== null"
        class="text-xs flex-1 break-words"
        :class="testOk ? 'text-[var(--color-primary)]' : 'text-[var(--color-danger)]'"
      >{{ testOk ? t('desktop.plugin.aiChatbox.testOk') : testResult }}</span>
    </div>

    <!-- 模型列表 -->
    <div>
      <label class="block text-xs font-medium mb-1.5 text-[var(--text-secondary)]">
        {{ t('desktop.plugin.aiChatbox.modelList') }}
      </label>
      <ModelListEditor v-model:models="form.models" />
    </div>

    <!-- 操作 -->
    <div class="flex items-center justify-between pt-2">
      <button
        v-if="mode === 'edit'"
        class="h-[36px] px-4 text-sm rounded-btn bg-[var(--color-danger-light)] text-[var(--color-danger)] hover:opacity-80 transition-opacity"
        @click="askDelete"
      >
        {{ deleting ? t('desktop.plugin.aiChatbox.confirmDeleteShort') : t('desktop.plugin.aiChatbox.deleteProvider') }}
      </button>
      <span v-else></span>
      <div class="flex gap-2">
        <button
          class="h-[36px] px-5 text-sm rounded-btn bg-brand text-[var(--color-primary-contrast)] hover:bg-brand-hover transition-colors disabled:opacity-50"
          :disabled="!canSave"
          @click="save"
        >
          {{ t('desktop.plugin.aiChatbox.saveProvider') }}
        </button>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * ProviderForm — 供应商编辑表单
 *
 * 预设/自定义共用：名称 + BaseURL + API Key（明文存储，与现状一致）+ 拉取模型
 * （真实 GET /models，失败回退不阻塞）+ 测试连接（非流式短请求）+ 模型列表编辑。
 * 删除用内联二次确认（禁原生 confirm 弹窗）。
 */
import { ref, reactive, computed } from 'vue'
import { useI18n } from 'vue-i18n'
import ModelListEditor from './ModelListEditor.vue'
import { PROVIDER_PRESETS, generateId } from '../types'
import type { ApiProvider } from '../types'

const props = defineProps<{
  mode: 'add' | 'edit'
  initialValues?: ApiProvider
  existingNames: string[]
  /** 拉取模型列表（经宿主命令，由 ChatView 注入 config.fetchModels） */
  fetchModels: (provider: ApiProvider) => Promise<string[]>
  /** 测试连接（经宿主命令，由 ChatView 注入 config.testConnection） */
  testConnection: (provider: ApiProvider) => Promise<string>
}>()

const emit = defineEmits<{
  save: [provider: ApiProvider]
  delete: [id: string]
}>()

const { t } = useI18n()

/** 预设回填（无 initialValues 且匹配预设名时） */
const preset = computed(() =>
  PROVIDER_PRESETS.find(p => p.name === props.initialValues?.name) || null
)

const form = reactive<ApiProvider>({
  id: props.initialValues?.id || generateId(),
  name: props.initialValues?.name || props.mode === 'add' && preset.value?.name ? preset.value!.name : '',
  apiKey: props.initialValues?.apiKey || '',
  baseUrl: props.initialValues?.baseUrl || preset.value?.baseUrl || '',
  apiFormat: 'openai',
  models: props.initialValues?.models?.length
    ? [...props.initialValues.models]
    : preset.value?.models ? [...preset.value.models] : [],
  activeModel: props.initialValues?.activeModel || '',
})

const showKey = ref(false)
const fetching = ref(false)
const testing = ref(false)
const fetchError = ref('')
const testResult = ref<string | null>(null)
const testOk = ref(false)
const deleting = ref(false)

const canSave = computed(() =>
  form.name.trim() !== '' && form.baseUrl.trim() !== '' && form.apiKey.trim() !== ''
)

/** 拉取模型列表：成功替换 models；失败提示并保留现有列表 */
async function onFetchModels(): Promise<void> {
  fetching.value = true
  fetchError.value = ''
  try {
    const result = await props.fetchModels({ ...form })
    if (result.length > 0) {
      form.models = result
    } else {
      fetchError.value = t('desktop.plugin.aiChatbox.fetchModelsEmpty')
    }
  } catch (e: any) {
    fetchError.value = `${t('desktop.plugin.aiChatbox.fetchModelsFailed')}: ${String(e?.message || e)}`
  } finally {
    fetching.value = false
  }
}

/** 测试连接：成功显示回复预览；失败显示错误 */
async function onTestConnection(): Promise<void> {
  testing.value = true
  testResult.value = null
  try {
    const reply = await props.testConnection({ ...form })
    testOk.value = true
    testResult.value = reply.slice(0, 120)
  } catch (e: any) {
    testOk.value = false
    testResult.value = String(e?.message || e)
  } finally {
    testing.value = false
  }
}

function askDelete(): void {
  if (deleting.value) {
    emit('delete', props.initialValues!.id)
    deleting.value = false
  } else {
    deleting.value = true
    // 3 秒后复位，避免误触
    setTimeout(() => { deleting.value = false }, 3000)
  }
}

function save(): void {
  if (!canSave.value) return
  emit('save', {
    ...form,
    activeModel: form.models[0] || '',
  })
}
</script>
