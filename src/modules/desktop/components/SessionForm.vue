<template>
  <form @submit.prevent="handleSubmit" class="space-y-4">
    <!-- Name -->
    <Input
      v-model="form.name"
      label="名称"
      placeholder="会话名称"
      required
    />

    <!-- Environment -->
    <Select
      v-model="form.environment"
      label="执行环境"
      :options="environmentOptions"
      required
    />

    <!-- WSL Distribution -->
    <div v-if="form.environment === 'wsl2'">
      <Select
        v-if="!wslStore.isLoading"
        v-model="form.wslDistro"
        label="WSL 发行版"
        :options="wslDistroOptions"
        placeholder="选择发行版"
        :disabled="wslDistroOptions.length === 0"
        required
      />
      <!-- WSL 初始化中的加载提示 -->
      <div v-else class="form-group">
        <label class="block text-sm mb-2 text-gray-700 dark:text-dark-300">
          WSL 发行版
          <span class="text-red-500">*</span>
        </label>
        <div class="flex items-center gap-2 border rounded-lg px-4 py-2 border-gray-300 dark:border-dark-600 bg-white dark:bg-dark-700 text-gray-500 dark:text-dark-400">
          <Spinner size="sm" color="primary" />
          <span class="text-sm">WSL 初始化中...</span>
        </div>
      </div>
      <!-- WSL 不可用或加载失败的提示 -->
      <p v-if="!wslStore.isLoading && !wslStore.isAvailable" class="mt-1 text-sm text-yellow-500">
        未检测到 WSL，请确认已安装 WSL2
      </p>
      <p v-else-if="wslStore.error" class="mt-1 text-sm text-red-500">
        WSL 检测失败: {{ wslStore.error }}
      </p>
    </div>

    <!-- Working Directory -->
    <Input
      v-model="form.workingDir"
      label="工作目录"
      placeholder="C:\Users\..."
      required
    >
      <template #suffix>
        <button
          type="button"
          @click="browseDir"
          class="text-primary-400 hover:text-primary-300"
        >
          浏览
        </button>
      </template>
    </Input>

    <!-- Command -->
    <Input
      v-model="form.command"
      label="启动命令"
      placeholder="claude"
      required
      help="输入要执行的命令，如 claude、npm run dev 等"
    />

    <!-- Auto Start -->
    <Toggle
      v-model="form.autoStart"
      label="开机自动启动"
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

const form = ref<SessionFormData>({
  name: '',
  environment: 'windows',
  wslDistro: '',
  workingDir: '',
  command: 'claude',
  autoStart: false,
})

const environmentOptions = [
  { value: 'windows', label: 'Windows 原生' },
  { value: 'wsl2', label: 'WSL2' },
]

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
