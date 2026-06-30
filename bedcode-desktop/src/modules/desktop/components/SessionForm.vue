<template>
  <form @submit.prevent="handleSubmit" class="space-y-4">
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
        <label class="block text-sm mb-2 text-slate-700 dark:text-dark-300">
          {{ $t('desktop.form.wslDistro') }}
          <span class="text-red-500">*</span>
        </label>
        <div class="flex items-center gap-2 border rounded-lg px-4 py-2 border-slate-300 dark:border-dark-600 bg-white dark:bg-dark-700 text-slate-500 dark:text-dark-400">
          <Spinner size="sm" color="primary" />
          <span class="text-sm">{{ $t('desktop.form.wslInitializing') }}</span>
        </div>
      </div>
      <!-- WSL 不可用或加载失败的提示 -->
      <p v-if="!wslStore.isLoading && !wslStore.isAvailable" class="mt-1 text-sm text-yellow-500">
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
          class="text-primary-400 hover:text-primary-300"
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
import type { SessionConfig } from '@/modules/shared/stores/session'
import Input from '@/modules/shared/components/Input.vue'
import Select from '@/modules/shared/components/Select.vue'
import Toggle from '@/modules/shared/components/Toggle.vue'
import Spinner from '@/modules/shared/components/Spinner.vue'
import { useWslStore } from '@/modules/desktop/stores/wsl'
import { open } from '@tauri-apps/plugin-dialog'
import { useSettingsStore } from '@/modules/shared/stores/settings'

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
