<template>
  <form @submit.prevent="handleSubmit" class="space-y-5">
    <!-- Name -->
    <Input
      v-model="form.name"
      :label="$t('desktop.form.name')"
      :placeholder="$t('desktop.form.namePlaceholder')"
      required
    />

    <!-- Environment -->
    <Select
      v-model="form.environment"
      :label="$t('desktop.form.environment')"
      :options="environmentOptions"
      required
    />

    <!-- WSL Distribution -->
    <div v-if="form.environment === 'wsl2'">
      <Select
        v-if="!wslStore.isLoading"
        v-model="form.wslDistro"
        :label="$t('desktop.form.wslDistro')"
        :options="wslDistroOptions"
        :placeholder="$t('desktop.form.wslDistroPlaceholder')"
        :disabled="wslDistroOptions.length === 0"
        required
      />
      <!-- WSL 初始化中的加载提示 -->
      <div v-else class="form-group">
        <label class="block text-xs font-medium mb-1.5 text-[var(--text-secondary)]">
          {{ $t('desktop.form.wslDistro') }}
          <span class="text-red-500">*</span>
        </label>
        <div class="flex items-center gap-2 border rounded-input px-4 py-2 border-[var(--border-input)] bg-[var(--bg-input)] text-[var(--text-tertiary)]">
          <Spinner size="sm" color="primary" />
          <span class="text-sm">{{ $t('desktop.form.wslInitializing') }}</span>
        </div>
      </div>
      <!-- WSL 不可用或加载失败的提示 -->
      <p v-if="!wslStore.isLoading && !wslStore.isAvailable" class="mt-1 text-sm text-amber-500">
        {{ $t('desktop.form.wslNotDetected') }}
      </p>
      <p v-else-if="wslStore.error" class="mt-1 text-sm text-red-500">
        {{ $t('desktop.form.wslDetectFailed', { error: wslStore.error }) }}
      </p>
    </div>

    <!-- Working Directory -->
    <Input
      v-model="form.workingDir"
      :label="$t('desktop.form.workingDir')"
      placeholder="C:\Users\..."
      required
    >
      <template #suffix>
        <button
          type="button"
          @click="browseDir"
          class="text-brand hover:text-[var(--color-primary-hover)]"
        >
          {{ $t('common.button.browse') }}
        </button>
      </template>
    </Input>

    <!-- Command -->
    <Input
      v-model="form.command"
      :label="$t('desktop.form.command')"
      placeholder="claude"
      required
      :help="$t('desktop.form.commandHelp')"
    />

    <!-- Auto Start -->
    <Toggle
      v-model="form.autoStart"
      :label="$t('desktop.form.autoStart')"
    />
  </form>
</template>

<script setup lang="ts">
/**
 * SessionForm - 会话配置表单
 *
 * 使用 WSL Store 读取缓存的 WSL 信息，避免每次打开弹窗时重复执行 wsl 命令
 */
import { ref, computed, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import type { SessionConfig } from '@/stores/session'
import Input from '@/components/Input.vue'
import Select from '@/components/Select.vue'
import Toggle from '@/components/Toggle.vue'
import Spinner from '@/components/Spinner.vue'
import { useWslStore } from '@/stores/wsl'
import { open } from '@tauri-apps/plugin-dialog'
import { useSettingsStore } from '@/stores/settings'

const props = defineProps<{
  config?: SessionConfig | null
}>()

const emit = defineEmits<{
  (e: 'save', form: SessionFormData): void
}>()

interface SessionFormData {
  name: string
  environment: string
  wslDistro: string
  workingDir: string
  command: string
  autoStart: boolean
}

const wslStore = useWslStore()
const settingsStore = useSettingsStore()
const { t } = useI18n()

const form = ref<SessionFormData>({
  name: '',
  environment: 'windows',
  wslDistro: '',
  workingDir: '',
  command: 'claude',
  autoStart: false,
})

const environmentOptions = computed(() => [
  { value: 'windows', label: t('desktop.form.windowsNative') },
  { value: 'wsl2', label: 'WSL2' },
])

const wslDistroOptions = computed(() =>
  wslStore.distros.map(d => ({
    value: d.name,
    label: d.name,
  }))
)

watch(() => props.config, (config) => {
  if (config) {
    form.value = {
      name: config.name,
      environment: config.environment,
      wslDistro: config.wslDistro || config.wsl_distro || '',
      workingDir: config.workingDir || config.working_dir || '',
      command: config.command || '',
      autoStart: config.autoStart ?? config.auto_start ?? false,
    }
  } else {
    form.value = {
      name: '',
      environment: settingsStore.settings.session.default_environment || 'windows',
      wslDistro: settingsStore.settings.session.default_wsl_distro || '',
      workingDir: settingsStore.settings.session.default_working_dir || '',
      command: settingsStore.settings.session.default_command || 'claude',
      autoStart: false,
    }
  }
}, { immediate: true })

async function browseDir() {
  try {
    const selected = await open({
      directory: true,
      multiple: false,
      defaultPath: form.value.workingDir || undefined,
    })
    if (selected) {
      form.value.workingDir = selected as string
    }
  } catch (e) {
    console.error('Failed to browse directory:', e)
  }
}

function handleSubmit() {
  emit('save', form.value)
}

// 暴露表单数据供父组件获取
defineExpose({
  form,
  validate: () => {
    return !!form.value.name && !!form.value.workingDir && !!form.value.command
  }
})
</script>
