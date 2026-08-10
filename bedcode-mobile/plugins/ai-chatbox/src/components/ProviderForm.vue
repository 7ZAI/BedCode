<template>
  <div class="space-y-5">
    <div class="flex items-center justify-between">
      <h4 class="text-[var(--font-size-lg)] font-medium text-[var(--mobile-text-primary)]">
        {{ mode === 'edit' ? t('mobile.plugin.aiChatbox.editProvider') : t('mobile.plugin.aiChatbox.addProvider') }}
      </h4>
      <span
        v-if="mode === 'edit'"
        class="text-xs px-2.5 py-1 rounded-full bg-[var(--mobile-bg-tertiary)] text-[var(--mobile-text-muted)]"
      >{{ initialValues?.name }}</span>
    </div>

    <!-- 名称 -->
    <div>
      <label class="block text-xs font-medium mb-1.5 text-[var(--mobile-text-secondary)]">
        {{ t('mobile.plugin.aiChatbox.name') }}
      </label>
      <input
        v-model="form.name"
        type="text"
        class="w-full min-h-[44px] px-3 text-[var(--font-size-base)] bg-[var(--mobile-input-bg)] text-[var(--mobile-text-primary)] border border-[var(--mobile-input-border)] rounded-xl placeholder:text-[var(--mobile-input-placeholder)] focus:outline-none focus:border-[var(--mobile-input-focus)] transition-colors"
      />
    </div>

    <!-- Base URL -->
    <div>
      <label class="block text-xs font-medium mb-1.5 text-[var(--mobile-text-secondary)]">
        {{ t('mobile.plugin.aiChatbox.baseUrl') }}
      </label>
      <input
        v-model="form.baseUrl"
        type="text"
        placeholder="https://api.example.com/v1"
        class="w-full min-h-[44px] px-3 text-[var(--font-size-base)] bg-[var(--mobile-input-bg)] text-[var(--mobile-text-primary)] border border-[var(--mobile-input-border)] rounded-xl placeholder:text-[var(--mobile-input-placeholder)] focus:outline-none focus:border-[var(--mobile-input-focus)] transition-colors"
      />
    </div>

    <!-- API Key -->
    <div>
      <label class="block text-xs font-medium mb-1.5 text-[var(--mobile-text-secondary)]">
        {{ t('mobile.plugin.aiChatbox.apiKey') }}
      </label>
      <div class="flex gap-2">
        <input
          v-model="form.apiKey"
          :type="showKey ? 'text' : 'password'"
          placeholder="sk-..."
          class="flex-1 min-w-0 min-h-[44px] px-3 text-[var(--font-size-base)] bg-[var(--mobile-input-bg)] text-[var(--mobile-text-primary)] border border-[var(--mobile-input-border)] rounded-xl placeholder:text-[var(--mobile-input-placeholder)] focus:outline-none focus:border-[var(--mobile-input-focus)] transition-colors"
        />
        <button
          class="min-h-[44px] px-3 text-[var(--font-size-sm)] rounded-xl bg-[var(--mobile-bg-tertiary)] text-[var(--mobile-text-secondary)] active:opacity-80 transition-opacity flex-shrink-0"
          @click="showKey = !showKey"
        >
          {{ showKey ? t('mobile.plugin.aiChatbox.hide') : t('mobile.plugin.aiChatbox.show') }}
        </button>
      </div>
      <p class="mt-1.5 text-xs text-[var(--mobile-text-muted)]">
        {{ t('mobile.plugin.aiChatbox.apiKeyHint') }}
      </p>
    </div>

    <!-- 拉取模型 + 测试连接 -->
    <div class="space-y-2">
      <div class="flex items-center gap-2">
        <button
          class="min-h-[44px] px-4 text-[var(--font-size-sm)] rounded-xl bg-[var(--mobile-bg-tertiary)] text-[var(--mobile-text-secondary)] active:opacity-80 transition-opacity disabled:opacity-40 flex-shrink-0"
          :disabled="fetching || !form.baseUrl || !form.apiKey"
          @click="onFetchModels"
        >
          {{ fetching ? t('mobile.plugin.aiChatbox.fetchingModels') : t('mobile.plugin.aiChatbox.fetchModels') }}
        </button>
        <button
          class="min-h-[44px] px-4 text-[var(--font-size-sm)] rounded-xl bg-[var(--mobile-bg-tertiary)] text-[var(--mobile-text-secondary)] active:opacity-80 transition-opacity disabled:opacity-40 flex-shrink-0"
          :disabled="testing || !form.baseUrl || !form.apiKey"
          @click="onTestConnection"
        >
          {{ testing ? t('mobile.plugin.aiChatbox.testing') : t('mobile.plugin.aiChatbox.testConnection') }}
        </button>
      </div>
      <p v-if="fetchError" class="text-xs text-[var(--mobile-error)] break-words">
        {{ fetchError }}
      </p>
      <p
        v-else-if="testResult !== null"
        class="text-xs break-words"
        :class="testOk ? 'text-[var(--mobile-success)]' : 'text-[var(--mobile-error)]'"
      >{{ testOk ? t('mobile.plugin.aiChatbox.testOk') : testResult }}</p>
    </div>

    <!-- 模型列表 -->
    <div>
      <label class="block text-xs font-medium mb-1.5 text-[var(--mobile-text-secondary)]">
        {{ t('mobile.plugin.aiChatbox.modelList') }}
      </label>
      <ModelListEditor v-model:models="form.models" />
    </div>

    <!-- 操作 -->
    <div class="flex items-center justify-between pt-2">
      <button
        v-if="mode === 'edit'"
        class="min-h-[44px] px-4 text-[var(--font-size-sm)] rounded-xl bg-[var(--mobile-error-muted)] text-[var(--mobile-error)] active:opacity-80 transition-opacity"
        @click="askDelete"
      >
        {{ deleting ? t('mobile.plugin.aiChatbox.confirmDeleteShort') : t('mobile.plugin.aiChatbox.deleteProvider') }}
      </button>
      <span v-else></span>
      <button
        class="min-h-[44px] px-6 text-[var(--font-size-base)] rounded-xl bg-[var(--mobile-accent)] text-[var(--mobile-text-on-accent)] active:opacity-80 transition-opacity disabled:opacity-40"
        :disabled="!canSave"
        @click="save"
      >
        {{ t('mobile.plugin.aiChatbox.saveProvider') }}
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * ProviderForm — 供应商编辑表单（移动端）
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
      fetchError.value = t('mobile.plugin.aiChatbox.fetchModelsEmpty')
    }
  } catch (e: any) {
    fetchError.value = `${t('mobile.plugin.aiChatbox.fetchModelsFailed')}: ${String(e?.message || e)}`
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
